use std::{
    fmt,
    fs::File,
    io::{self, BufRead, LineWriter, Write},
    path::Path,
};

use byte_unit::Byte;

enum Value<'a> {
    Byte(Byte),
    Int(u32),
    Str(&'a str),
}

impl<'a> fmt::Display for Value<'a> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Value::Byte(b) => b
                .get_appropriate_unit(false)
                .format(0)
                .replace(" ", "")
                .fmt(formatter),
            Value::Int(i) => i.fmt(formatter),
            Value::Str(s) => format!("'{}'", s).fmt(formatter),
        }
    }
}

struct KeyVal<'a> {
    key: &'a str,
    val: Value<'a>,
}

impl<'a> KeyVal<'a> {
    fn byte(key: &'a str, val: Byte) -> KeyVal<'a> {
        KeyVal {
            key,
            val: Value::Byte(val),
        }
    }

    fn int(key: &'a str, val: u32) -> KeyVal<'a> {
        KeyVal {
            key,
            val: Value::Int(val),
        }
    }

    fn str(key: &'a str, val: &'a str) -> KeyVal<'a> {
        KeyVal {
            key,
            val: Value::Str(val),
        }
    }
}

pub struct Config<'a> {
    rows: Vec<KeyVal<'a>>,
}

impl<'a> Config<'a> {
    pub fn to_file(&self, path: &Path) -> io::Result<()> {
        let file = File::create(path)?;
        let mut file = LineWriter::new(file);

        for row in self.to_strings() {
            file.write_all(&row.as_bytes())?;
            file.write_all(b"\n")?;
        }

        file.flush()?;
        Ok(())
    }

    fn to_strings(&self) -> Vec<String> {
        self.rows
            .iter()
            .map(|row| format!("{} = {}", row.key, row.val))
            .collect()
    }
}

#[derive(Debug)]
pub struct PostgresqlConf<'a> {
    listen_addresses: &'a str,
    port: u32,
    max_connections: u32,
    shared_buffers: Byte,
    max_wal_size: Byte,
    min_wal_size: Byte,
    locale: &'a str,
    timezone: &'a str,
}

impl<'a> PostgresqlConf<'a> {
    pub fn default(port: u32) -> PostgresqlConf<'a> {
        PostgresqlConf {
            listen_addresses: "*",
            port,
            max_connections: 100,
            shared_buffers: Byte::from_string("128MB").unwrap(),
            max_wal_size: Byte::from_string("1GB").unwrap(),
            min_wal_size: Byte::from_string("80MB").unwrap(),
            timezone: "America/Toronto",
            locale: "en_US.UTF-8",
        }
    }

    pub fn to_config(&self) -> Config<'a> {
        Config {
            rows: vec![
                KeyVal::str("listen_addresses", self.listen_addresses),
                KeyVal::int("port", self.port),
                KeyVal::int("max_connections", self.max_connections),
                KeyVal::byte("shared_buffers", self.shared_buffers),
                KeyVal::str("dynamic_shared_memory_type", "posix"),
                KeyVal::byte("max_wal_size", self.max_wal_size),
                KeyVal::byte("min_wal_size", self.min_wal_size),
                KeyVal::str("log_timezone", self.timezone),
                KeyVal::str("datestyle", "iso, mdy"),
                KeyVal::str("timezone", self.timezone),
                KeyVal::str("lc_messages", self.locale),
                KeyVal::str("lc_monetary", self.locale),
                KeyVal::str("lc_numeric", self.locale),
                KeyVal::str("lc_time", self.locale),
                KeyVal::str("default_text_search_config", "pg_catalog.english"),
            ],
        }
    }
}

pub fn read_port(path: &Path) -> io::Result<u32> {
    let file = File::open(path)?;
    for line in io::BufReader::new(file).lines() {
        let line = line?;
        if line.starts_with("port = ") {
            return line
                .strip_prefix("port = ")
                .unwrap()
                .parse::<u32>()
                .map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "cannot parse port value")
                });
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "port config missing",
    ))
}
