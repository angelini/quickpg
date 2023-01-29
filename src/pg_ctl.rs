use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, Output},
    str,
};

use fs_extra::dir::CopyOptions;
use regex::Regex;

use crate::config::{self, PostgresqlConf};

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
            .args([
                "--pgdata",
                &join_str(&self.data, name),
                "-o--no-sync",
                "init",
            ])
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
                &join_str(&self.data, name),
                "--log",
                &join_str(&self.logs, &*format!("{}.log", name)),
                "--options",
                &format!("-k{}", absolute_sockets),
                "start",
            ])
            .output()?;

        PgCtl::check_output(&output)
    }

    pub fn status(&self, name: &str) -> Result<(u32, Option<u32>)> {
        let port = config::read_port(&self.data.join(name).join("postgresql.conf"))?;

        let output = Command::new(&self.binary)
            .args(["--pgdata", &join_str(&self.data, name), "status"])
            .output()?;
        let stdout = str::from_utf8(&output.stdout).unwrap().to_string();

        if stdout.starts_with("pg_ctl: no server running") {
            return Ok((port, None));
        }

        PgCtl::check_output(&output)?;

        let re = Regex::new(r"\(PID: (\d+)\)").unwrap();
        match re.captures(&stdout) {
            Some(caps) => Ok((port, Some(caps[1].parse::<u32>().unwrap()))),
            None => Err(Error::InvalidOutput(stdout)),
        }
    }

    pub fn stop(&self, name: &str) -> Result<()> {
        let output = Command::new(&self.binary)
            .args(["--pgdata", &join_str(&self.data, name), "stop"])
            .output()?;

        PgCtl::check_output(&output)
    }

    pub fn fork(&self, template: &str, target: &str, conf: &PostgresqlConf) -> Result<()> {
        fs_extra::dir::copy(
            self.data.join(template),
            self.data.join(target),
            &CopyOptions::new(),
        )?;

        conf.to_config()
            .to_file(&self.data.join(target).join("postgresql.conf"))?;

        return Ok(());
    }

    pub fn destroy(&self, name: &str) -> Result<()> {
        let log = self.logs.join(format!("{}.log", name));

        fs::remove_dir_all(self.data.join(name))?;
        if log.is_file() {
            fs::remove_file(self.logs.join(format!("{}.log", name)))?;
        }

        Ok(())
    }

    pub fn list(&self) -> Result<Vec<(String, u32, Option<u32>)>> {
        let mut results = vec![];

        for entry in fs::read_dir(&self.data)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let (port, pid) = self.status(&name)?;
            results.push((name, port, pid))
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
}

fn join_str<'a, S: Into<&'a str>>(directory: &Path, name: S) -> String {
    directory.join(name.into()).to_string_lossy().into_owned()
}
