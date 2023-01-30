use std::{
    env,
    fs::{self, File},
    io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    str,
};

use regex::Regex;
use rocket::{
    serde::{Deserialize, Serialize},
    tokio,
};
use tokio_postgres::{self, Config, NoTls};

use crate::config::PostgresqlConf;

#[derive(Debug, Responder)]
pub enum Error {
    Io(io::Error),
    Postgres(String),
    CliError(String),
    InvalidOutput(String),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<tokio_postgres::Error> for Error {
    fn from(err: tokio_postgres::Error) -> Self {
        Error::Postgres(format!("{}", err))
    }
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Status {
    pub id: String,
    pub dbname: String,
    pub port: u32,
    pub pid: Option<u32>,
}

impl Status {
    pub fn is_running(&self) -> bool {
        self.pid.is_some()
    }

    fn running(id: impl Into<String>, dbname: impl Into<String>, port: u32, pid: u32) -> Status {
        Status {
            id: id.into(),
            dbname: dbname.into(),
            port,
            pid: Some(pid),
        }
    }

    fn stopped(id: impl Into<String>, dbname: impl Into<String>, port: u32) -> Status {
        Status {
            id: id.into(),
            dbname: dbname.into(),
            port,
            pid: None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Metadata {
    dbname: String,
    port: u32,
}

impl Metadata {
    fn to_file(&self, path: &Path) -> io::Result<()> {
        let file = File::create(path)?;
        serde_json::to_writer(io::BufWriter::new(file), self)?;
        Ok(())
    }

    fn from_file(path: &Path) -> io::Result<Metadata> {
        let file = File::open(path)?;
        Ok(serde_json::from_reader(io::BufReader::new(file))?)
    }
}

#[derive(Debug)]
pub struct PgCtl {
    pub user: String,
    binary: PathBuf,
    logs: PathBuf,
    data: PathBuf,
    sockets: PathBuf,
}

impl PgCtl {
    pub fn new(user: impl Into<String>, root: &Path) -> PgCtl {
        PgCtl {
            user: user.into(),
            binary: root.join("bin/pg_ctl"),
            logs: root.join("logs"),
            data: root.join("data"),
            sockets: root.join("sockets"),
        }
    }

    pub async fn init<'a>(&self, id: &str, dbname: &str, conf: &PostgresqlConf<'a>) -> Result<()> {
        let output = Command::new(&self.binary)
            .args(["--pgdata", &join_str(&self.data, id), "-o--no-sync", "init"])
            .output()?;

        PgCtl::check_output(&output)?;

        conf.to_config()
            .to_file(&self.data.join(id).join("postgresql.conf"))?;

        let meta = Metadata {
            dbname: dbname.to_string(),
            port: conf.port,
        };
        meta.to_file(&self.data.join(id).join("quickpg.json"))?;

        self.start(id)?;

        PgCtl::create_database(dbname, &self.user, conf.port).await?;

        Ok(())
    }

    pub fn exists(&self, id: &str) -> bool {
        self.data.join(id).is_dir()
    }

    pub fn start(&self, id: &str) -> Result<()> {
        let absolute_sockets = env::current_dir()?
            .join(&self.sockets)
            .to_string_lossy()
            .into_owned();

        let output = Command::new(&self.binary)
            .args([
                "--pgdata",
                &join_str(&self.data, id),
                "--log",
                &join_str(&self.logs, &*format!("{}.log", id)),
                "--options",
                &format!("-k{}", absolute_sockets),
                "start",
            ])
            .output()?;

        PgCtl::check_output(&output)
    }

    pub fn status(&self, id: &str) -> Result<Status> {
        let meta = Metadata::from_file(&self.data.join(id).join("quickpg.json"))?;

        let output = Command::new(&self.binary)
            .args(["--pgdata", &join_str(&self.data, id), "status"])
            .output()?;
        let stdout = str::from_utf8(&output.stdout).unwrap().to_string();

        if stdout.starts_with("pg_ctl: no server running") {
            return Ok(Status::stopped(id, meta.dbname, meta.port));
        }

        PgCtl::check_output(&output)?;

        let re = Regex::new(r"\(PID: (\d+)\)").unwrap();
        match re.captures(&stdout) {
            Some(caps) => Ok(Status::running(
                id,
                meta.dbname,
                meta.port,
                caps[1].parse::<u32>().unwrap(),
            )),
            None => Err(Error::InvalidOutput(stdout)),
        }
    }

    pub fn stop(&self, id: &str) -> Result<()> {
        let output = Command::new(&self.binary)
            .args(["--pgdata", &join_str(&self.data, id), "stop"])
            .output()?;

        PgCtl::check_output(&output)
    }

    pub fn fork(
        &self,
        template: &str,
        target: &str,
        dbname: &str,
        conf: &PostgresqlConf,
    ) -> Result<()> {
        copy_recursively(self.data.join(template), self.data.join(target))?;

        conf.to_config()
            .to_file(&self.data.join(target).join("postgresql.conf"))?;

        let meta = Metadata {
            dbname: dbname.to_string(),
            port: conf.port,
        };
        meta.to_file(&self.data.join(target).join("quickpg.json"))?;

        return Ok(());
    }

    pub fn destroy(&self, id: &str) -> Result<()> {
        let log = self.logs.join(format!("{}.log", id));

        fs::remove_dir_all(self.data.join(id))?;
        if log.is_file() {
            fs::remove_file(self.logs.join(format!("{}.log", id)))?;
        }

        Ok(())
    }

    pub fn list(&self) -> Result<Vec<Status>> {
        let mut results = vec![];

        for entry in fs::read_dir(&self.data)? {
            let entry = entry?;
            let id = entry.file_name().to_string_lossy().into_owned();
            let status = self.status(&id)?;
            results.push(status)
        }

        Ok(results)
    }

    fn check_output(output: &Output) -> Result<()> {
        if output.status.success() {
            Ok(())
        } else {
            Err(Error::CliError(
                str::from_utf8(&output.stderr).unwrap().to_string(),
            ))
        }
    }

    async fn create_database(dbname: &str, user: &str, port: u32) -> Result<()> {
        let mut config = Config::new();
        config.host("127.0.0.1");
        config.port(port as u16);
        config.dbname("postgres");
        config.user(user);

        let (client, connection) = config.connect(NoTls).await?;
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        client
            .execute(
                &format!("CREATE DATABASE {} OWNER {}", dbname, user),
                &vec![],
            )
            .await?;

        Ok(())
    }
}

fn join_str<'a, S: Into<&'a str>>(directory: &Path, id: S) -> String {
    directory.join(id.into()).to_string_lossy().into_owned()
}

pub fn copy_recursively(source: impl AsRef<Path>, destination: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(&destination)?;
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o700))?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let filetype = entry.file_type()?;
        if filetype.is_dir() {
            copy_recursively(entry.path(), destination.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), destination.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}
