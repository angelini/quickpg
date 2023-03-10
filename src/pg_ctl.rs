use std::{
    env, io,
    path::{Path, PathBuf},
    process::Output,
    str,
};

use serde::{Deserialize, Serialize};
use tokio::{self, io::AsyncWriteExt, process::Command};
use tokio_postgres::{self, Config, NoTls};

use crate::{config::PostgresqlConf, copy};

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Postgres(tokio_postgres::Error),
    CliError(String),
    InvalidPidFile(PathBuf),
    DataDirNotFound(PathBuf),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<tokio_postgres::Error> for Error {
    fn from(err: tokio_postgres::Error) -> Self {
        Error::Postgres(err)
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
    async fn to_file(&self, path: &Path) -> io::Result<()> {
        let serialized = serde_json::to_vec(self)?;

        let mut file = tokio::fs::File::create(path).await?;
        file.write_all(&serialized).await?;
        file.flush().await?;

        Ok(())
    }

    async fn from_file(path: &Path) -> io::Result<Metadata> {
        let content = tokio::fs::read_to_string(path).await?;
        Ok(serde_json::from_str(&content)?)
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
            .output()
            .await?;

        PgCtl::check_output(&output)?;

        conf.to_config()
            .to_file(&self.data.join(id).join("postgresql.conf"))
            .await?;

        let meta = Metadata {
            dbname: dbname.to_string(),
            port: conf.port,
        };
        meta.to_file(&self.data.join(id).join("quickpg.json"))
            .await?;

        self.start(id).await?;

        PgCtl::create_database(dbname, &self.user, conf.port).await?;

        Ok(())
    }

    pub fn exists(&self, id: &str) -> bool {
        self.data.join(id).is_dir()
    }

    pub async fn start(&self, id: &str) -> Result<()> {
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
            .output()
            .await?;

        PgCtl::check_output(&output)
    }

    pub fn is_running(&self, id: &str) -> bool {
        let pidfile = self.data.join(id).join("postmaster.pid");
        pidfile.is_file()
    }

    pub async fn status(&self, id: &str) -> Result<Status> {
        let data = self.data.join(id);
        if !data.is_dir() {
            return Err(Error::DataDirNotFound(data));
        }

        let meta = Metadata::from_file(&data.join("quickpg.json")).await?;

        let pidfile = data.join("postmaster.pid");
        if !pidfile.is_file() {
            return Ok(Status::stopped(id, meta.dbname, meta.port));
        }

        let content = tokio::fs::read_to_string(&pidfile).await?;

        if let Some(pid_end_index) = content.find("\n") {
            if let Ok(pid) = content[0..pid_end_index].parse::<u32>() {
                return Ok(Status::running(id, meta.dbname, meta.port, pid));
            }
        }

        Err(Error::InvalidPidFile(pidfile))
    }

    pub async fn stop(&self, id: &str, wait: bool) -> Result<()> {
        let data = join_str(&self.data, id);
        let mut args = vec!["--pgdata", &data];
        if !wait {
            args.push("--no-wait");
        }
        args.push("stop");

        let output = Command::new(&self.binary).args(args).output().await?;

        PgCtl::check_output(&output)
    }

    pub async fn fork<'a>(
        &self,
        template: &str,
        target: &str,
        dbname: &str,
        conf: &PostgresqlConf<'a>,
    ) -> Result<()> {
        let template_data = self.data.join(template);
        if !template_data.is_dir() {
            return Err(Error::DataDirNotFound(template_data));
        }
        copy::copy_pgdata(template_data, self.data.join(target)).await?;

        conf.to_config()
            .to_file(&self.data.join(target).join("postgresql.conf"))
            .await?;

        let meta = Metadata {
            dbname: dbname.to_string(),
            port: conf.port,
        };
        meta.to_file(&self.data.join(target).join("quickpg.json"))
            .await?;

        return self.start(target).await;
    }

    pub async fn destroy(&self, id: &str) -> Result<()> {
        let data = self.data.join(id);
        if !data.is_dir() {
            return Err(Error::DataDirNotFound(data));
        }

        tokio::fs::remove_dir_all(data).await?;

        let log = self.logs.join(format!("{}.log", id));
        if log.is_file() {
            tokio::fs::remove_file(self.logs.join(format!("{}.log", id))).await?;
        }

        Ok(())
    }

    pub async fn list(&self) -> Result<Vec<Status>> {
        let mut dir = tokio::fs::read_dir(&self.data).await?;
        let mut results = vec![];

        while let Some(entry) = dir.next_entry().await? {
            let id = entry.file_name().to_string_lossy().into_owned();
            let status = self.status(&id).await?;
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
