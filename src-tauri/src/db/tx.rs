/*!
Explicit transactions for the SQL console.

The ordinary execute path is autocommit: a statement lands the moment it runs.
This module keeps one database connection checked out across several commands so
the user can run a few statements, look at what they did, and then decide
whether to keep them.

A transaction holds a real connection (and, on most engines, locks) for as long
as it stays open, so a session that is left behind is rolled back after
`IDLE_TIMEOUT` by a timer (`sweep_idle`), and every session is rolled back when
the active connection changes (`connection_changed`).

A session ends through `release` and nowhere else. Its `BEGIN` was a plain
statement, so sqlx does not know the connection is in a transaction: dropping
the `PoolConnection` hands it back to the pool still inside it (sqlx only pings
on release), and the next grid edit or console run that picks it up joins the
abandoned transaction. Its writes are never committed and the locks stay held.
`release` rolls back first and closes the socket if the rollback fails.
*/

use crate::db::connection::Db;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

use futures::TryStreamExt;
use serde::Serialize;
use serde_json::Value;
use sqlx::{Column, Row, TypeInfo};
use tauri::Emitter;

use super::connection::{require_conn, ActiveConnection};
use super::query::{
    cell_to_json, is_row_returning_sql, pg_type_label, split_sql_statements, ColumnInfo, SqlResult,
    EXECUTE_SQL_TIMEOUT_MS,
};

/// An open transaction holds a connection out of the pool. If the window is
/// closed or the tab forgotten, that connection never comes back - so a session
/// idle this long is rolled back and released on the next transaction command.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// How often the timer looks for sessions past `IDLE_TIMEOUT`.
pub const SWEEP_EVERY: Duration = Duration::from_secs(60);

/// Postgres's own backstop, a little past ours: if this process dies or hangs
/// with a transaction open, the server ends the session itself.
const SERVER_IDLE_TIMEOUT: &str = "35min";

/// A ROLLBACK that takes longer than this is talking to a dead socket.
const RELEASE_TIMEOUT: Duration = Duration::from_secs(5);

/// The checked-out connection, per engine.
enum TxConn {
    Postgres(sqlx::pool::PoolConnection<sqlx::Postgres>),
    Mysql(sqlx::pool::PoolConnection<sqlx::MySql>),
    Sqlite(sqlx::pool::PoolConnection<sqlx::Sqlite>),
}

impl TxConn {
    /// Take the connection out of the pool for good. Dropping the raw
    /// connection closes its socket, which makes the server roll back.
    fn close_hard(self) {
        match self {
            TxConn::Postgres(c) => drop(c.detach()),
            TxConn::Mysql(c) => drop(c.detach()),
            TxConn::Sqlite(c) => drop(c.detach()),
        }
    }

    fn engine(&self) -> &'static str {
        match self {
            TxConn::Postgres(_) => "postgres",
            TxConn::Mysql(_) => "mysql",
            TxConn::Sqlite(_) => "sqlite",
        }
    }
}

/// Where to ask who this transaction is holding up: a second pooled
/// connection and this session's server-side id.
enum Blocker {
    Postgres(sqlx::PgPool, i32),
    Mysql(sqlx::MySqlPool, u64),
}

struct TxSession {
    conn: TxConn,
    /// The connection epoch at BEGIN (see `connection_changed`).
    epoch: u64,
    blocker: Option<Blocker>,
    /// Statements run since BEGIN, for the status line.
    statements: usize,
    /// Rows reported changed by those statements.
    rows_affected: u64,
    started: Instant,
    last_used: Instant,
}

/// Open sessions by id. Module state rather than Tauri state so a connection
/// switch (`connection_changed`, called from `db::connection`) can reach them.
static SESSIONS: LazyLock<Mutex<HashMap<String, TxSession>>> = LazyLock::new(Default::default);

/// Bumped every time the active connection is replaced, parked or closed. A
/// session from an older epoch belongs to a database the window has left.
static EPOCH: AtomicU64 = AtomicU64::new(0);

/// For the `tx-ended` event the window listens to.
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

pub fn init(app: tauri::AppHandle) {
    let _ = APP.set(app);
}

/// Kept so `lib.rs` can still `.manage()` it; the sessions live in `SESSIONS`.
#[derive(Default)]
pub struct TxState;

/// Why a session ended without the user pressing Commit or Rollback.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum EndReason {
    Idle,
    ConnectionChanged,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TxEnded {
    session_id: String,
    reason: EndReason,
    /// False when the rollback failed and the socket was closed instead (the
    /// server rolls back on its own when it sees the socket go).
    rolled_back: bool,
}

/// Roll the session back and give its connection back to the pool, or close
/// it if the rollback does not come back clean. True when it rolled back.
async fn release(mut session: TxSession) -> bool {
    let ok = matches!(
        tokio::time::timeout(RELEASE_TIMEOUT, run_statement(&mut session.conn, "ROLLBACK")).await,
        Ok(Ok(_))
    );
    if !ok {
        log::info!("tx: rollback failed, closing the {} connection", session.conn.engine());
        session.conn.close_hard();
    }
    ok
}

/// End sessions taken out of the map, tell the window, and report how many.
async fn end_sessions(ended: Vec<(String, TxSession)>, reason: EndReason) -> usize {
    let n = ended.len();
    for (id, session) in ended {
        let rolled_back = release(session).await;
        if let Some(app) = APP.get() {
            let _ = app.emit("tx-ended", TxEnded { session_id: id, reason, rolled_back });
        }
    }
    n
}

/// The active connection is about to be replaced, parked or closed. Every open
/// session belongs to the old one: roll them all back. Without this, a tab's
/// session kept a live connection to the database just left, and its next
/// statement ran there while the window showed the new one.
pub fn connection_changed() {
    EPOCH.fetch_add(1, Ordering::SeqCst);
    let ended: Vec<(String, TxSession)> = SESSIONS.lock().unwrap().drain().collect();
    if ended.is_empty() {
        return;
    }
    tauri::async_runtime::spawn(end_sessions(ended, EndReason::ConnectionChanged));
}

/// Roll back sessions idle past `IDLE_TIMEOUT`. Runs on a timer (lib.rs) as well
/// as on every begin and status call.
pub async fn sweep_idle() -> usize {
    let ended: Vec<(String, TxSession)> = {
        let mut sessions = SESSIONS.lock().unwrap();
        let stale: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| s.last_used.elapsed() >= IDLE_TIMEOUT)
            .map(|(id, _)| id.clone())
            .collect();
        stale.into_iter().filter_map(|id| sessions.remove(&id).map(|s| (id, s))).collect()
    };
    end_sessions(ended, EndReason::Idle).await
}

/// Ids of the open sessions, for the quit guard.
#[tauri::command]
pub fn tx_open_sessions() -> Vec<String> {
    SESSIONS.lock().unwrap().keys().cloned().collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TxStatus {
    pub open: bool,
    pub statements: usize,
    pub rows_affected: u64,
    /// Milliseconds since BEGIN, so the UI can warn about a long-held lock.
    pub open_ms: u64,
    pub engine: Option<String>,
    /// Other sessions waiting on a lock this transaction holds. None when the
    /// engine can't say (SQLite) or the server would not tell us.
    pub blocking: Option<i64>,
}

impl TxStatus {
    fn closed() -> Self {
        Self { open: false, statements: 0, rows_affected: 0, open_ms: 0, engine: None, blocking: None }
    }

    fn of(session: &TxSession) -> Self {
        Self {
            open: true,
            statements: session.statements,
            rows_affected: session.rows_affected,
            open_ms: session.started.elapsed().as_millis() as u64,
            engine: Some(session.conn.engine().to_string()),
            blocking: None,
        }
    }
}

// ── Session bookkeeping ───────────────────────────────────────────────────────
//
// Every command that touches a connection *takes* the session out of the map,
// awaits without holding the lock, then puts it back. That keeps the mutex
// uncontended across awaits, and makes a second concurrent command for the same
// session fail fast instead of interleaving statements on one connection.

fn take(id: &str) -> Option<TxSession> {
    SESSIONS.lock().unwrap().remove(id)
}

/// Put a session back after a command. If the connection changed while the
/// command ran, `connection_changed` has already drained the map, so the
/// session comes back to a world that has moved on: end it instead.
async fn put(id: &str, session: TxSession) -> Result<(), String> {
    if session.epoch != EPOCH.load(Ordering::SeqCst) {
        let id = id.to_string();
        end_sessions(vec![(id, session)], EndReason::ConnectionChanged).await;
        return Err(LEFT_CONNECTION.into());
    }
    SESSIONS.lock().unwrap().insert(id.to_string(), session);
    Ok(())
}

const LEFT_CONNECTION: &str =
    "This transaction was on a connection you have since left, so it was rolled back. Nothing it did was saved.";

fn no_session(id: &str) -> String {
    format!("No open transaction for {id}. Start one before running statements in it.")
}

/// This session's server-side id, so `tx_status` can ask who it is blocking.
async fn blocker_for(conn: &mut TxConn, db: &ActiveConnection) -> Option<Blocker> {
    match (conn, db) {
        (TxConn::Postgres(c), ActiveConnection::Postgres(pool)) => {
            let pid = sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()").fetch_one(&mut **c).await.ok()?;
            Some(Blocker::Postgres(pool.clone(), pid))
        }
        (TxConn::Mysql(c), ActiveConnection::Mysql(pool)) => {
            let id = sqlx::query_scalar::<_, u64>("SELECT CONNECTION_ID()").fetch_one(&mut **c).await.ok()?;
            Some(Blocker::Mysql(pool.clone(), id))
        }
        _ => None,
    }
}

/// Sessions waiting on a lock this one holds, asked on a second connection.
/// None when the server won't say (MySQL needs performance_schema access).
async fn blocking_count(blocker: &Blocker) -> Option<i64> {
    let ask = async {
        match blocker {
            Blocker::Postgres(pool, pid) => sqlx::query_scalar::<_, i64>(
                "SELECT count(*)::bigint FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid))",
            )
            .bind(pid)
            .fetch_one(pool)
            .await
            .ok(),
            Blocker::Mysql(pool, id) => sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM performance_schema.data_lock_waits w \
                 JOIN performance_schema.threads t ON t.THREAD_ID = w.BLOCKING_THREAD_ID \
                 WHERE t.PROCESSLIST_ID = ?",
            )
            .bind(id)
            .fetch_one(pool)
            .await
            .ok(),
        }
    };
    tokio::time::timeout(Duration::from_secs(2), ask).await.ok().flatten()
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Open a transaction and hold its connection until commit or rollback.
#[tauri::command]
pub async fn tx_begin(db: Db<'_>, session_id: String) -> Result<TxStatus, String> {
    sweep_idle().await;
    if SESSIONS.lock().unwrap().contains_key(&session_id) {
        return Err("A transaction is already open here. Commit or roll it back first.".into());
    }
    let epoch = EPOCH.load(Ordering::SeqCst);
    let active = require_conn(&db)?;

    let mut conn = match &active {
        ActiveConnection::Postgres(pool) => TxConn::Postgres(
            pool.acquire().await.map_err(|e| format!("Could not open a transaction: {e}"))?,
        ),
        ActiveConnection::Mysql(pool) => TxConn::Mysql(
            pool.acquire().await.map_err(|e| format!("Could not open a transaction: {e}"))?,
        ),
        ActiveConnection::Sqlite(pool) => TxConn::Sqlite(
            pool.acquire().await.map_err(|e| format!("Could not open a transaction: {e}"))?,
        ),
        ActiveConnection::Duckdb(_) | ActiveConnection::Mssql(_) | ActiveConnection::Oracle(_) => {
            return Err("Explicit transactions are not supported for this engine yet".into())
        }
        ActiveConnection::D1(_) | ActiveConnection::LibSql(_) => {
            return Err("Explicit transactions need a persistent connection, which HTTP-based engines do not have".into())
        }
        ActiveConnection::Clickhouse(_) => {
            return Err("ClickHouse does not support transactions".into())
        }
        ActiveConnection::Posthog(_) => return Err("PostHog does not support transactions".into()),
        ActiveConnection::Redis(_) => return Err("Redis does not support SQL transactions".into()),
        ActiveConnection::Dynamodb(_) | ActiveConnection::Doc(_) => return Err("DynamoDB does not support SQL transactions here".into()),
    };

    run_statement(&mut conn, "BEGIN").await.map_err(|e| format!("Could not begin: {e}"))?;
    if let TxConn::Postgres(_) = conn {
        // A statement that hangs inside a transaction holds its locks for as
        // long as it runs, so cap it the same way the autocommit path does.
        let _ = run_statement(&mut conn, &format!("SET LOCAL statement_timeout = {EXECUTE_SQL_TIMEOUT_MS}")).await;
        // The server's own backstop for a session this process never comes
        // back to. Postgres 9.6+; older servers refuse it, which is harmless.
        let _ = run_statement(
            &mut conn,
            &format!("SET LOCAL idle_in_transaction_session_timeout = '{SERVER_IDLE_TIMEOUT}'"),
        )
        .await;
    }
    let blocker = blocker_for(&mut conn, &active).await;

    let now = Instant::now();
    let session = TxSession {
        conn,
        epoch,
        blocker,
        statements: 0,
        rows_affected: 0,
        started: now,
        last_used: now,
    };
    let status = TxStatus::of(&session);
    put(&session_id, session).await?;
    Ok(status)
}

/// Run SQL inside the open transaction. Nothing is visible to anyone else until
/// the transaction commits.
#[tauri::command]
pub async fn tx_execute(session_id: String, sql: String) -> Result<SqlResult, String> {
    let trimmed = sql.trim().to_string();
    if trimmed.is_empty() {
        return Err("Query is empty".into());
    }
    // Let the buttons own the transaction's lifetime. Typing COMMIT into the
    // editor would end the transaction while the session still believed it was
    // open, and every later statement would silently run in autocommit.
    if let Some(word) = leading_keyword(&trimmed) {
        if matches!(word.as_str(), "commit" | "rollback" | "begin" | "start") {
            return Err(format!(
                "Use the transaction controls rather than {} here, so the session and the database agree on what is open.",
                word.to_uppercase()
            ));
        }
    }

    let mut session = take(&session_id).ok_or_else(|| no_session(&session_id))?;
    if session.epoch != EPOCH.load(Ordering::SeqCst) {
        end_sessions(vec![(session_id, session)], EndReason::ConnectionChanged).await;
        return Err(LEFT_CONNECTION.into());
    }
    let started = Instant::now();
    let result = execute_in(&mut session.conn, &trimmed).await;

    session.last_used = Instant::now();
    match &result {
        Ok(r) => {
            session.statements += 1;
            session.rows_affected += r.row_count.unwrap_or(0).max(0) as u64;
        }
        Err(_) => {
            // The statement failed. Postgres aborts the whole transaction on any
            // error, so leave the session open but let the UI show the failure -
            // the user's only sensible next move is to roll back.
            session.statements += 1;
        }
    }
    put(&session_id, session).await?;

    result.map(|mut r| {
        r.query_ms = started.elapsed().as_millis() as u64;
        r.sql = trimmed;
        r
    })
}

/// Commit the open transaction and give the connection back.
#[tauri::command]
pub async fn tx_commit(session_id: String) -> Result<TxStatus, String> {
    let mut session = take(&session_id).ok_or_else(|| no_session(&session_id))?;
    if session.epoch != EPOCH.load(Ordering::SeqCst) {
        end_sessions(vec![(session_id, session)], EndReason::ConnectionChanged).await;
        return Err(LEFT_CONNECTION.into());
    }
    match run_statement(&mut session.conn, "COMMIT").await {
        // Clean: the connection is out of its transaction, back to the pool.
        Ok(_) => Ok(TxStatus::closed()),
        Err(e) => {
            // The commit failed, so the transaction is neither applied nor
            // cleanly closed. Roll back what is left, or close the socket.
            release(session).await;
            Err(format!("Could not commit: {e}. Nothing was saved."))
        }
    }
}

/// Roll the open transaction back and give the connection back.
#[tauri::command]
pub async fn tx_rollback(session_id: String) -> Result<TxStatus, String> {
    let session = take(&session_id).ok_or_else(|| no_session(&session_id))?;
    // Whatever happens the session is over: a failed ROLLBACK closes the
    // socket, and the server rolls back when it sees it go.
    if release(session).await {
        Ok(TxStatus::closed())
    } else {
        Err("The rollback did not complete, so the connection was closed. The server rolls back a closed connection's transaction, so nothing was saved.".into())
    }
}

/// What the UI needs to render the transaction indicator.
#[tauri::command]
pub async fn tx_status(session_id: String) -> Result<TxStatus, String> {
    sweep_idle().await;
    let (mut status, blocker) = {
        let sessions = SESSIONS.lock().unwrap();
        match sessions.get(&session_id) {
            None => return Ok(TxStatus::closed()),
            Some(s) => (
                TxStatus::of(s),
                s.blocker.as_ref().map(|b| match b {
                    Blocker::Postgres(p, pid) => Blocker::Postgres(p.clone(), *pid),
                    Blocker::Mysql(p, id) => Blocker::Mysql(p.clone(), *id),
                }),
            ),
        }
    };
    if let Some(b) = blocker {
        status.blocking = blocking_count(&b).await;
    }
    Ok(status)
}

/// The first word of a statement, lowercased, skipping leading comments.
fn leading_keyword(sql: &str) -> Option<String> {
    let mut rest = sql.trim_start();
    loop {
        if let Some(after) = rest.strip_prefix("--") {
            rest = after.split_once('\n').map_or("", |(_, r)| r).trim_start();
            continue;
        }
        if let Some(after) = rest.strip_prefix("/*") {
            rest = after.split_once("*/").map_or("", |(_, r)| r).trim_start();
            continue;
        }
        break;
    }
    rest.split(|c: char| c.is_whitespace() || c == ';')
        .find(|w| !w.is_empty())
        .map(|w| w.to_ascii_lowercase())
}

/// Run one statement for its side effect, ignoring any rows.
async fn run_statement(conn: &mut TxConn, sql: &str) -> Result<u64, String> {
    match conn {
        TxConn::Postgres(c) => sqlx::query(sql).execute(&mut **c).await.map(|r| r.rows_affected()),
        TxConn::Mysql(c) => sqlx::query(sql).execute(&mut **c).await.map(|r| r.rows_affected()),
        TxConn::Sqlite(c) => sqlx::query(sql).execute(&mut **c).await.map(|r| r.rows_affected()),
    }
    .map_err(|e| e.to_string())
}

/// Run the user's SQL, returning rows when the last statement produces them.
///
/// Mirrors the autocommit executor: a script is split so a multi-statement
/// editor selection works, and only the final statement's rows are returned.
async fn execute_in(conn: &mut TxConn, sql: &str) -> Result<SqlResult, String> {
    let split = split_sql_statements(sql);
    let stmts: Vec<&str> = split.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if stmts.is_empty() {
        return Err("Query is empty".into());
    }
    let last = stmts.len() - 1;
    let mut affected: u64 = 0;

    for (i, stmt) in stmts.iter().enumerate() {
        if i == last && is_row_returning_sql(stmt) {
            return fetch_rows(conn, stmt).await;
        }
        affected += run_statement(conn, stmt)
            .await
            .map_err(|e| format!("Query failed: {e}"))?;
    }

    Ok(SqlResult {
        columns: Vec::new(),
        rows: Vec::new(),
        row_count: Some(affected as i64),
        message: Some(format!(
            "{affected} row{} affected (not committed yet)",
            if affected == 1 { "" } else { "s" }
        )),
        query_ms: 0,
        sql: sql.to_string(),
    })
}

async fn fetch_rows(conn: &mut TxConn, stmt: &str) -> Result<SqlResult, String> {
    let mut columns: Vec<ColumnInfo> = Vec::new();
    let mut rows: Vec<Vec<Value>> = Vec::new();

    match conn {
        TxConn::Postgres(c) => {
            let mut stream = sqlx::query(stmt).fetch(&mut **c);
            while let Some(row) = stream.try_next().await.map_err(|e| format!("Query failed: {e}"))? {
                if rows.is_empty() {
                    columns = row
                        .columns()
                        .iter()
                        .map(|col| {
                            ColumnInfo::new(col.name(), pg_type_label(col.type_info().name()))
                        })
                        .collect();
                }
                rows.push((0..row.len()).map(|i| cell_to_json(&row, i)).collect());
            }
        }
        TxConn::Mysql(c) => {
            let fetched = sqlx::query(stmt)
                .fetch_all(&mut **c)
                .await
                .map_err(|e| format!("Query failed: {e}"))?;
            for row in &fetched {
                if rows.is_empty() {
                    columns = row
                        .columns()
                        .iter()
                        .map(|col| ColumnInfo::new(col.name(), col.type_info().name().to_string()))
                        .collect();
                }
                rows.push(
                    (0..row.len()).map(|i| super::mysql::cell_to_json(row, i)).collect(),
                );
            }
        }
        TxConn::Sqlite(c) => {
            let fetched = sqlx::query(stmt)
                .fetch_all(&mut **c)
                .await
                .map_err(|e| format!("Query failed: {e}"))?;
            for row in &fetched {
                if rows.is_empty() {
                    columns = row
                        .columns()
                        .iter()
                        .map(|col| ColumnInfo::new(col.name(), col.type_info().name().to_string()))
                        .collect();
                }
                rows.push(
                    (0..row.len()).map(|i| super::sqlite::cell_to_json(row, i)).collect(),
                );
            }
        }
    }

    let count = rows.len() as i64;
    Ok(SqlResult {
        columns,
        rows,
        row_count: Some(count),
        message: None,
        query_ms: 0,
        sql: stmt.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::leading_keyword;

    // Live: a released session must give back a connection that is out of its
    // transaction. Before `release`, the idle sweep dropped the PoolConnection,
    // sqlx returned it to the pool still inside the BEGIN, and the next user of
    // the pool ran in the abandoned transaction.
    //   STROKE_TEST_PG_URL=postgres://postgres@127.0.0.1:5433/postgres \
    //   cargo test --lib tx::tests::release_ -- --nocapture
    #[tokio::test]
    async fn release_returns_a_clean_connection() {
        use super::*;
        let Ok(url) = std::env::var("STROKE_TEST_PG_URL") else { return };
        // One connection, so the next acquire gets the one we released.
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap();
        sqlx::query("CREATE TABLE IF NOT EXISTS stroke_tx_release (v int)").execute(&pool).await.unwrap();
        sqlx::query("TRUNCATE stroke_tx_release").execute(&pool).await.unwrap();

        let session = |conn| TxSession {
            conn,
            epoch: 0,
            blocker: None,
            statements: 0,
            rows_affected: 0,
            started: Instant::now(),
            last_used: Instant::now(),
        };
        let open = |pool: &sqlx::PgPool| {
            let pool = pool.clone();
            async move {
                let mut conn = TxConn::Postgres(pool.acquire().await.unwrap());
                run_statement(&mut conn, "BEGIN").await.unwrap();
                run_statement(&mut conn, "INSERT INTO stroke_tx_release VALUES (1)").await.unwrap();
                conn
            }
        };
        let in_tx = |pool: &sqlx::PgPool| {
            let pool = pool.clone();
            async move {
                // Assigned only inside a transaction that wrote something.
                sqlx::query_scalar::<_, Option<i64>>("SELECT txid_current_if_assigned()")
                    .fetch_one(&pool)
                    .await
                    .unwrap()
                    .is_some()
            }
        };

        // What the sweep used to do: drop it.
        drop(session(open(&pool).await));
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(in_tx(&pool).await, "a dropped session leaks its transaction into the pool");
        // Clean up after the demonstration.
        sqlx::query("ROLLBACK").execute(&pool).await.unwrap();

        // What every exit does now.
        assert!(release(session(open(&pool).await)).await);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!in_tx(&pool).await, "a released session must not leave a transaction open");
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM stroke_tx_release").fetch_one(&pool).await.unwrap();
        assert_eq!(rows, 0, "the uncommitted insert must be gone");
        sqlx::query("DROP TABLE stroke_tx_release").execute(&pool).await.unwrap();
    }

    // Live: switching connections ends every open session.
    #[tokio::test]
    async fn connection_change_ends_sessions() {
        use super::*;
        let Ok(url) = std::env::var("STROKE_TEST_PG_URL") else { return };
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(2).connect(&url).await.unwrap();
        let mut conn = TxConn::Postgres(pool.acquire().await.unwrap());
        run_statement(&mut conn, "BEGIN").await.unwrap();
        let epoch = EPOCH.load(Ordering::SeqCst);
        let now = Instant::now();
        let s = TxSession { conn, epoch, blocker: None, statements: 0, rows_affected: 0, started: now, last_used: now };
        put("test-switch", s).await.unwrap();
        assert!(tx_open_sessions().contains(&"test-switch".to_string()));
        connection_changed();
        assert!(!tx_open_sessions().contains(&"test-switch".to_string()));
        let err = tx_execute("test-switch".into(), "SELECT 1".into()).await.unwrap_err();
        assert!(err.contains("No open transaction"), "{err}");
    }

    #[test]
    fn finds_the_first_keyword() {
        assert_eq!(leading_keyword("SELECT 1").as_deref(), Some("select"));
        assert_eq!(leading_keyword("  commit  ").as_deref(), Some("commit"));
        assert_eq!(leading_keyword("COMMIT;").as_deref(), Some("commit"));
    }

    #[test]
    fn looks_past_comments() {
        // A COMMIT hidden behind a comment still ends the transaction, so the
        // guard has to see it.
        assert_eq!(leading_keyword("-- save it\nCOMMIT").as_deref(), Some("commit"));
        assert_eq!(leading_keyword("/* save it */ COMMIT").as_deref(), Some("commit"));
    }

    #[test]
    fn empty_input_has_no_keyword() {
        assert_eq!(leading_keyword(""), None);
        assert_eq!(leading_keyword("   "), None);
        assert_eq!(leading_keyword("-- only a comment"), None);
    }
}
