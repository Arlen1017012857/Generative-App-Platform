use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

/// A single worker owns SQLite. No SQL or filesystem writes run on the UI thread.
#[derive(Clone)]
pub(crate) struct Store {
    jobs: mpsc::Sender<Job>,
    directory: PathBuf,
}

impl Store {
    pub(crate) fn open(directory: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(directory.join("sessions"))?;
        let connection = Connection::open(directory.join("desktop.sqlite"))?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS records (kind TEXT NOT NULL, id TEXT NOT NULL, value TEXT NOT NULL, PRIMARY KEY(kind,id));
            PRAGMA user_version=1;")?;
        let (jobs, receive) = mpsc::channel::<Job>();
        thread::Builder::new()
            .name("desktop-storage".into())
            .spawn(move || {
                let mut connection = connection;
                for job in receive {
                    job(&mut connection);
                }
            })?;
        Ok(Self { jobs, directory })
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    ) -> impl std::future::Future<Output = Result<T>> + Send + 'static {
        let (send, receive) = async_channel::bounded(1);
        let queued = self
            .jobs
            .send(Box::new(move |connection| {
                let _ = send.send_blocking(f(connection));
            }))
            .map_err(|_| anyhow::anyhow!("Storage worker stopped"));
        async move {
            queued?;
            receive.recv().await.context("Storage response lost")?
        }
    }

    pub(crate) async fn load<T: DeserializeOwned + Send + 'static>(
        &self,
        kind: &str,
    ) -> Result<Vec<T>> {
        let kind = kind.to_owned();
        self.call(move |connection| {
            let mut statement =
                connection.prepare("SELECT value FROM records WHERE kind=? ORDER BY rowid")?;
            let values = statement.query_map([kind], |row| row.get::<_, String>(0))?;
            values
                .map(|value| Ok(serde_json::from_str(&value?)?))
                .collect()
        })
        .await
    }

    /// Enqueues immediately; awaiting only observes completion, so task scheduling cannot reorder writes.
    pub(crate) fn save<T: Serialize + Clone + Send + 'static>(
        &self,
        kind: &str,
        id: &str,
        value: &T,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static {
        let (kind, id, value) = (kind.to_owned(), id.to_owned(), value.clone());
        self.call(move |connection| {
            let value = serde_json::to_string(&value)?;
            connection.execute("INSERT INTO records(kind,id,value) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET value=excluded.value", [kind,id,value])?;
            Ok(())
        })
    }

    pub(crate) async fn get<T: DeserializeOwned + Send + 'static>(
        &self,
        kind: &str,
        id: &str,
    ) -> Result<Option<T>> {
        let (kind, id) = (kind.to_owned(), id.to_owned());
        self.call(move |connection| {
            use rusqlite::OptionalExtension;
            let value = connection
                .query_row(
                    "SELECT value FROM records WHERE kind=?1 AND id=?2",
                    [kind, id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            value
                .map(|value| Ok(serde_json::from_str(&value)?))
                .transpose()
        })
        .await
    }

    pub(crate) fn remove(
        &self,
        kind: &str,
        id: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static {
        let (kind, id) = (kind.to_owned(), id.to_owned());
        self.call(move |connection| {
            connection.execute("DELETE FROM records WHERE kind=?1 AND id=?2", [kind, id])?;
            Ok(())
        })
    }

    pub(crate) async fn flush(&self) -> Result<()> {
        self.call(|connection| {
            connection.execute_batch("PRAGMA wal_checkpoint(PASSIVE)")?;
            Ok(())
        })
        .await
    }

    pub(crate) fn remove_project(
        &self,
        id: &str,
        path: &Path,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static {
        let id = id.to_owned();
        let path = path.to_string_lossy().to_string();
        self.call(move |connection| {
            let transaction = connection.transaction()?;
            transaction.execute(
                "DELETE FROM records WHERE kind='session' AND json_extract(value,'$.project')=?1",
                [&id],
            )?;
            transaction.execute("DELETE FROM records WHERE kind='project' AND id=?1", [&id])?;
            transaction.execute(
                "DELETE FROM records WHERE kind='file-navigation' AND id=?1",
                [&path],
            )?;
            transaction.commit()?;
            Ok(())
        })
    }
}

pub(crate) fn data_directory() -> PathBuf {
    std::env::var_os("GAP_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").expect("HOME is required"))
                .join("Library/Application Support/Generative App Platform")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn writes_are_ordered_and_persisted() {
        futures::executor::block_on(async {
            let temporary = tempfile::tempdir().unwrap();
            let store = Store::open(temporary.path().into()).unwrap();
            let first = store.save("draft", "a", &"one");
            let second = store.save("draft", "a", &"two 中文");
            // Completion futures may be polled in any order; commits preserve submission order.
            second.await.unwrap();
            first.await.unwrap();
            store.flush().await.unwrap();
            let reopened = Store::open(temporary.path().into()).unwrap();
            assert_eq!(
                reopened.load::<String>("draft").await.unwrap(),
                ["two 中文"]
            );
        });
    }

    #[test]
    fn project_deletion_rolls_back_all_records_on_failure() {
        futures::executor::block_on(async {
            let temporary = tempfile::tempdir().unwrap();
            let store = Store::open(temporary.path().into()).unwrap();
            store
                .save("project", "p", &serde_json::json!({"id":"p"}))
                .await
                .unwrap();
            store
                .save("session", "a", &serde_json::json!({"id":"a","project":"p"}))
                .await
                .unwrap();
            let database = Connection::open(temporary.path().join("desktop.sqlite")).unwrap();
            database.execute_batch("CREATE TRIGGER reject_project_delete BEFORE DELETE ON records WHEN OLD.kind='project' BEGIN SELECT RAISE(ABORT,'fixture project deletion failure'); END;").unwrap();
            assert!(store.remove_project("p", temporary.path()).await.is_err());
            assert!(
                store
                    .get::<serde_json::Value>("project", "p")
                    .await
                    .unwrap()
                    .is_some()
            );
            assert!(
                store
                    .get::<serde_json::Value>("session", "a")
                    .await
                    .unwrap()
                    .is_some()
            );
        });
    }
}
