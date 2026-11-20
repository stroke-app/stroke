//! Stop for the console, shared by every engine that is not Postgres.
//!
//! The window sends one Stop per run (a oneshot), but a script runs several
//! statements and each engine wants the signal in its own shape: a receiver,
//! a future to race, or a callback on another thread. `Stop` turns the one
//! signal into as many of those as a run needs.
//!
//! `Arm` is the callback shape, for drivers that interrupt a running call from
//! another thread (SQLite's `sqlite3_interrupt`, DuckDB's `InterruptHandle`).
//! The interrupt goes to whatever the connection is running *now*, so it must
//! never fire after the run is over and the connection has moved on to the next
//! caller: `disarm` and the watcher take the same lock, and the watcher checks
//! `done` under it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{oneshot, Notify};

#[derive(Clone)]
pub struct Stop {
    fired: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl Stop {
    /// Listen on the run's Stop. If the run ends first, the sender is dropped
    /// and the listener exits.
    pub fn from_rx(rx: oneshot::Receiver<()>) -> Self {
        let stop = Self { fired: Arc::new(AtomicBool::new(false)), notify: Arc::new(Notify::new()) };
        let s = stop.clone();
        tauri::async_runtime::spawn(async move {
            if rx.await.is_ok() {
                s.fired.store(true, Ordering::SeqCst);
                s.notify.notify_waiters();
            }
        });
        stop
    }

    pub fn fired(&self) -> bool {
        self.fired.load(Ordering::SeqCst)
    }

    /// Resolves once Stop is pressed (at once if it already was).
    pub async fn wait(&self) {
        loop {
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.fired() {
                return;
            }
            notified.await;
        }
    }

    /// A receiver for one statement. Its forwarder exits when the statement
    /// drops the receiver, so an unpressed Stop leaves nothing behind.
    pub fn receiver(&self) -> oneshot::Receiver<()> {
        let (mut tx, rx) = oneshot::channel();
        let s = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::select! {
                _ = s.wait() => { let _ = tx.send(()); }
                _ = tx.closed() => {}
            }
        });
        rx
    }
}

#[derive(Default)]
struct Flags {
    done: bool,
    fired: bool,
}

/// An interrupt armed for one run on one connection.
pub struct Arm {
    flags: Arc<Mutex<Flags>>,
}

impl Arm {
    pub fn new<F>(rx: oneshot::Receiver<()>, interrupt: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        let flags = Arc::new(Mutex::new(Flags::default()));
        let f = flags.clone();
        tauri::async_runtime::spawn(async move {
            if rx.await.is_err() {
                return;
            }
            let mut g = f.lock().unwrap_or_else(|e| e.into_inner());
            if !g.done {
                g.fired = true;
                interrupt();
            }
        });
        Self { flags }
    }

    /// The run is over: no interrupt can reach the connection from here on.
    /// True when Stop interrupted it.
    pub fn disarm(self) -> bool {
        let mut g = self.flags.lock().unwrap_or_else(|e| e.into_inner());
        g.done = true;
        g.fired
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn one_stop_reaches_every_statement() {
        let (tx, rx) = oneshot::channel();
        let stop = Stop::from_rx(rx);
        let a = stop.receiver();
        let b = stop.receiver();
        tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(1), stop.wait()).await.unwrap();
        assert!(a.await.is_ok() && b.await.is_ok());
        // A receiver taken after Stop still fires.
        assert!(stop.receiver().await.is_ok());
    }

    #[tokio::test]
    async fn an_arm_never_fires_after_disarm() {
        let (tx, rx) = oneshot::channel();
        let hit = Arc::new(AtomicBool::new(false));
        let h = hit.clone();
        let arm = Arm::new(rx, move || h.store(true, Ordering::SeqCst));
        assert!(!arm.disarm());
        let _ = tx.send(());
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!hit.load(Ordering::SeqCst), "a late Stop must not interrupt the connection's next user");
    }

    #[tokio::test]
    async fn an_arm_fires_while_armed() {
        let (tx, rx) = oneshot::channel();
        let hit = Arc::new(AtomicBool::new(false));
        let h = hit.clone();
        let arm = Arm::new(rx, move || h.store(true, Ordering::SeqCst));
        tx.send(()).unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(hit.load(Ordering::SeqCst));
        assert!(arm.disarm());
    }
}

/// Stop has to end the statement, not just the wait for it.
#[cfg(test)]
mod engine_tests {
    use std::time::{Duration, Instant};
    use tokio::sync::oneshot;

    /// Runs forever unless interrupted.
    const SQLITE_FOREVER: &str =
        "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT count(*) FROM c WHERE x < 0";

    #[tokio::test]
    async fn sqlite_stop_interrupts_the_statement() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let _ = tx.send(());
        });
        let t = Instant::now();
        let r = crate::db::sqlite::execute_sql_cancellable(&pool, SQLITE_FOREVER, rx).await;
        assert_eq!(r.unwrap_err(), crate::db::query::QUERY_CANCELLED);
        assert!(t.elapsed() < Duration::from_secs(3), "took {:?}", t.elapsed());
        // The one connection is free and clean: the statement really ended.
        let r = crate::db::sqlite::execute_sql(&pool, "SELECT 1").await.unwrap();
        assert_eq!(r.row_count, Some(1));
    }

    #[tokio::test]
    async fn sqlite_finishes_when_nobody_stops_it() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        let (_tx, rx) = oneshot::channel::<()>();
        let r = crate::db::sqlite::execute_sql_cancellable(&pool, "SELECT 42", rx).await.unwrap();
        assert_eq!(r.rows[0][0], serde_json::json!(42));
    }

    #[tokio::test]
    async fn duckdb_stop_interrupts_the_statement() {
        let cfg = crate::db::connection::DuckdbConfig { name: "t".into(), file_path: ":memory:".into() };
        let h = crate::db::connection::open_duckdb(&cfg).await.unwrap();
        let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let _ = tx.send(());
        });
        let t = Instant::now();
        let r = crate::db::duckdb::execute_sql_cancellable(
            &h,
            "SELECT count(*) FROM range(100000000) a, range(100000000) b WHERE a.range + b.range < 0",
            rx,
        )
        .await;
        assert_eq!(r.unwrap_err(), crate::db::query::QUERY_CANCELLED);
        assert!(t.elapsed() < Duration::from_secs(5), "took {:?}", t.elapsed());
        let r = crate::db::duckdb::execute_sql(&h, "SELECT 1").await.unwrap();
        assert_eq!(r.row_count, Some(1));
    }
}
