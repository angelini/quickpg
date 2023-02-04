use std::{fmt, io, path::Path};

use byte_unit::Byte;
use tokio::{self, io::AsyncWriteExt};

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
    pub async fn to_file(&self, path: &Path) -> io::Result<()> {
        let mut file = tokio::fs::File::create(path).await?;

        for row in self.to_strings() {
            file.write_all(&row.as_bytes()).await?;
            file.write_all(b"\n").await?;
        }

        file.flush().await?;
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
    pub port: u32,
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
                // Crash unsafe performance settings
                KeyVal::str("fsync", "off"),
                KeyVal::str("full_page_writes", "off"),
                KeyVal::str("synchronous_commit", "off"),
                KeyVal::str("wal_level", "minimal"),
                KeyVal::int("max_wal_senders", 0),
            ],
        }
    }
}
