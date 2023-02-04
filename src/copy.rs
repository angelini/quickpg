use std::{io, path::PathBuf};

use async_recursion::async_recursion;
use tokio::task::JoinSet;

const ROOT_FILES: &'static [&'static str] = &[
    "pg_hba.conf",
    "pg_ident.conf",
    "PG_VERSION",
    "postmaster.opts",
];
const EMPTY_DIRS: &'static [&'static str] = &[
    "pg_commit_ts",
    "pg_dynshmem",
    "pg_notify",
    "pg_replslot",
    "pg_serial",
    "pg_snapshots",
    "pg_stat_tmp",
    "pg_tblspc",
    "pg_twophase",
];
const SMALL_DIRS: &'static [&'static str] = &[
    "global",
    "pg_logical",
    "pg_multixact",
    "pg_stat",
    "pg_subtrans",
    "pg_wal",
    "pg_xact",
];
const LARGE_DIRS: &'static [&'static str] = &["base"];

#[async_recursion]
async fn copy_internal(source: PathBuf, destination: PathBuf) -> io::Result<()> {
    let mut dir = tokio::fs::read_dir(source).await?;

    while let Some(entry) = dir.next_entry().await? {
        let filetype = entry.file_type().await?;
        let new_path = destination.join(entry.file_name());

        if filetype.is_dir() {
            tokio::fs::DirBuilder::new()
                .mode(0o700)
                .create(&new_path)
                .await?;
            copy_internal(entry.path(), new_path).await?;
        } else {
            tokio::fs::copy(entry.path(), new_path).await?;
        }
    }

    Ok(())
}

pub async fn copy_pgdata(source: PathBuf, destination: PathBuf) -> io::Result<()> {
    tokio::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&destination)
        .await?;

    let mut set = JoinSet::new();

    set.spawn({
        let source = source.clone();
        let destination = destination.clone();
        async move {
            for file in ROOT_FILES {
                tokio::fs::copy(source.join(file), destination.join(file)).await?;
            }
            Result::<(), io::Error>::Ok(())
        }
    });

    set.spawn({
        let destination = destination.clone();
        async move {
            for dir in EMPTY_DIRS {
                tokio::fs::DirBuilder::new()
                    .mode(0o700)
                    .create(destination.join(dir))
                    .await?;
            }
            Result::<(), io::Error>::Ok(())
        }
    });

    for dir in SMALL_DIRS {
        let source = source.join(dir);
        let destination = destination.join(dir);
        set.spawn(async move {
            tokio::fs::DirBuilder::new()
                .mode(0o700)
                .create(&destination)
                .await?;
            copy_internal(source, destination).await
        });
    }

    for dir in LARGE_DIRS {
        let mut reader = tokio::fs::read_dir(source.join(dir)).await?;
        tokio::fs::DirBuilder::new()
            .mode(0o700)
            .create(&destination.join(dir))
            .await?;

        while let Some(entry) = reader.next_entry().await? {
            let nested_source = source.join(dir).join(entry.file_name());
            let nested_destination = destination.join(dir).join(entry.file_name());
            set.spawn(async move {
                tokio::fs::DirBuilder::new()
                    .mode(0o700)
                    .create(&nested_destination)
                    .await?;
                copy_internal(nested_source, nested_destination).await
            });
        }
    }

    while let Some(value) = set.join_next().await {
        match value {
            Err(join_err) => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("join_err: {}", join_err),
                ))
            }
            Ok(Err(io_err)) => return Err(io_err),
            Ok(_) => (),
        }
    }

    Ok(())
}
