//! SQLite access on a dedicated thread (research R8). All queries go through [`Db::call`], so the
//! async runtime and the UI never block on disk I/O.

use crate::error::{AppError, AppResult};
use rusqlite::Connection;
use std::path::Path;
use std::sync::mpsc;
use tokio::sync::oneshot;

/// Embedded migrations, applied in order. `PRAGMA user_version` stores how many have run.
const MIGRATIONS: &[&str] = &[include_str!("migrations/0001_init.sql")];

type Job = Box<dyn FnOnce(&mut Connection) + Send>;

#[derive(Clone)]
pub struct Db {
    tx: mpsc::Sender<Job>,
}

impl Db {
    /// Open (or create) the database, configure it and apply pending migrations.
    pub fn open(path: &Path) -> AppResult<Self> {
        let mut conn = Connection::open(path)?;
        configure(&conn)?;
        migrate(&mut conn)?;
        Ok(Self::spawn(conn))
    }

    /// In-memory database for tests.
    pub fn open_in_memory() -> AppResult<Self> {
        let mut conn = Connection::open_in_memory()?;
        configure(&conn)?;
        migrate(&mut conn)?;
        Ok(Self::spawn(conn))
    }

    fn spawn(mut conn: Connection) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("db".into())
            .spawn(move || {
                for job in rx {
                    job(&mut conn);
                }
            })
            .expect("failed to start database thread");
        Self { tx }
    }

    /// Run `f` on the database thread and await its result.
    pub async fn call<T, F>(&self, f: F) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> AppResult<T> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Box::new(move |conn| {
                let _ = tx.send(f(conn));
            }))
            .map_err(|_| AppError::internal("database thread stopped"))?;
        rx.await
            .map_err(|_| AppError::internal("database thread dropped a request"))?
    }
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    let applied: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(applied as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

/// Current UTC time as ISO-8601, as stored in every timestamp column.
pub fn now() -> String {
    // SQLite-compatible, second precision, always UTC.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    format_unix(secs)
}

/// Format seconds since the Unix epoch as `YYYY-MM-DDTHH:MM:SSZ` (UTC).
pub fn format_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Civil-from-days (Howard Hinnant), proleptic Gregorian calendar.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    #[test]
    fn formats_unix_time() {
        assert_eq!(super::format_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(super::format_unix(1_791_742_282), "2026-10-11T18:11:22Z");
    }
}
