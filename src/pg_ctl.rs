use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, Output},
    str,
};

use fs_extra::dir::CopyOptions;
use regex::Regex;

use crate::config::PostgresqlConf;

#[derive(Debug, Responder)]
pub enum Error {
    Io(io::Error),
    FsExtra(String),
    CliError(String),
    InvalidOutput(String),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<fs_extra::error::Error> for Error {
    fn from(err: fs_extra::error::Error) -> Self {
        Error::FsExtra(err.to_string())
    }
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct PgCtl {
    binary: PathBuf,
    logs: PathBuf,
    data: PathBuf,
    sockets: PathBuf,
}

impl PgCtl {
    pub fn new(root: &Path) -> PgCtl {
        PgCtl {
            binary: root.join("bin/pg_ctl"),
            logs: root.join("logs"),
            data: root.join("data"),
            sockets: root.join("sockets"),
        }
    }

    pub fn init(&self, name: &str, conf: &PostgresqlConf) -> Result<()> {
        let output = Command::new(&self.binary)
            .args(["--pgdata", &self.data_dir(name), "-o--no-sync", "init"])
            .output()?;

        PgCtl::check_output(&output)?;

        conf.to_config()
            .to_file(&self.data.join(name).join("postgresql.conf"))?;

        Ok(())
    }

    pub fn exists(&self, name: &str) -> bool {
        self.data.join(name).is_dir()
    }

    pub fn start(&self, name: &str) -> Result<()> {
        let absolute_sockets = env::current_dir()?
            .join(&self.sockets)
            .to_string_lossy()
            .into_owned();

        let output = Command::new(&self.binary)
            .args([
                "--pgdata",
                &self.data_dir(name),
                "--log",
                &self.log_file(name),
                "--options",
                &format!("-k{}", absolute_sockets),
                "--no-wait",
                "start",
            ])
            .output()?;

        PgCtl::check_output(&output)
    }

    pub fn status(&self, name: &str) -> Result<Option<u32>> {
        let output = Command::new(&self.binary)
            .args(["--pgdata", &self.data_dir(name), "status"])
            .output()?;

        let stdout = str::from_utf8(&output.stdout).unwrap().to_string();

        if stdout.starts_with("pg_ctl: no server running") {
            return Ok(None);
        }

        PgCtl::check_output(&output)?;

        let re = Regex::new(r"\(PID: (\d+)\)").unwrap();
        match re.captures(&stdout) {
            Some(caps) => Ok(Some(caps[1].parse::<u32>().unwrap())),
            None => Err(Error::InvalidOutput(stdout)),
        }
    }

    pub fn stop(&self, name: &str) -> Result<()> {
        let output = Command::new(&self.binary)
            .args(["--pgdata", &self.data_dir(name), "--no-wait", "stop"])
            .output()?;

        PgCtl::check_output(&output)
    }

    pub fn fork(&self, template: &str, target: &str, conf: &PostgresqlConf) -> Result<()> {
        let options = CopyOptions {
            overwrite: false,
            skip_exist: false,
            buffer_size: 64000, //64kb
            copy_inside: true,
            content_only: false,
            depth: 0,
        };

        // FIXME: Incorrect data dir permissions

        fs_extra::dir::copy(self.data.join(template), self.data.join(target), &options)?;

        conf.to_config()
            .to_file(&self.data.join(target).join("postgresql.conf"))?;

        return Ok(());
    }

    pub fn list(&self) -> Result<Vec<(String, Option<u32>)>> {
        let mut results = vec![];

        for entry in fs::read_dir(&self.data)? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            let pid = self.status(&name)?;
            results.push((name, pid))
        }

        Ok(results)
    }

    fn data_dir(&self, name: &str) -> String {
        self.data.join(name).to_string_lossy().into_owned()
    }

    fn log_file(&self, name: &str) -> String {
        self.logs
            .join(format!("{}.log", name))
            .to_string_lossy()
            .into_owned()
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
}
