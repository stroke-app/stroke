/*!
Explicit transactions for the SQL console.

The ordinary execute path is autocommit: a statement lands the moment it runs.
This module keeps one database connection checked out across several commands so
the user can run a few statements, look at what they did, and then decide
whether to keep them.

A transaction holds a real connection (and, on most engines, locks) for as long
as it stays open, so a session that is left behind is closed automatically after
`IDLE_TIMEOUT` - see `sweep_idle`.
*/

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use futures::TryStreamExt;
use serde::Serialize;
use serde_json::Value;
use sqlx::{Column, Row, TypeInfo};
use tauri::State;

use super::connection::{require_conn, ActiveConnection, DbState};
use super::query::{
    cell_to_json, is_row_returning_sql, pg_type_label, split_sql_statements, ColumnInfo, SqlResult,
    EXECUTE_SQL_TIMEOUT_MS,
};

/// An open transaction holds a connection out of the pool. If the window is
/// closed or the tab forgotten, that connection never comes back - so a session
/// idle this long is rolled back and released on the next transaction command.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// The checked-out connection, per engine.
enum TxConn {
    Postgres(sqlx::pool::PoolConnection<sqlx::Postgres>),
    Mysql(sqlx::pool::PoolConnection<sqlx::MySql>),
    Sqlite(sqlx::pool::PoolConnection<sqlx::Sqlite>),
}

impl TxConn {
    fn engine(&self) -> &'static str {
        match self {
            TxConn::Postgres(_) => "postgres",
            TxConn::Mysql(_) => "mysql",
            TxConn::Sqlite(_) => "sqlite",
        }
    }
}

struct TxSession {
    conn: TxConn,
    /// Statements run since BEGIN, for the status line.
    statements: usize,
    /// Rows reported changed by those statements.
    rows_affected: u64,
    started: Instant,
    last_used: Instant,
}

#[derive(Default)]
pub struct TxState {
    sessions: Mutex<HashMap<String, TxSession>>,
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
}

impl TxStatus {
    fn closed() -> Self {
        Self { open: false, statements: 0, rows_affected: 0, open_ms: 0, engine: None }
    }

    fn of(session: &TxSession) -> Self {
        Self {
            open: true,
            statements: session.statements,
            rows_affected: session.rows_affected,
            open_ms: session.started.elapsed().as_millis() as u64,
            engine: Some(session.conn.engine().to_string()),
        }
    }
}

// ── Session bookkeeping ───────────────────────────────────────────────────────
//
// Every command that touches a connection *takes* the session out of the map,
// awaits without holding the lock, then puts it back. That keeps the mutex
// uncontended across awaits, and makes a second concurrent command for the same
// session fail fast instead of interleaving statements on one connection.

/// Remove sessions idle past the timeout. Their connections are dropped, which
/// returns them to the pool; the server rolls back the abandoned transaction.
fn sweep_idle(state: &State<'_, TxState>) {
    let mut sessions = state.sessions.lock().unwrap();
    sessions.retain(|_, s| s.last_used.elapsed() < IDLE_TIMEOUT);
}

fn take(state: &State<'_, TxState>, id: &str) -> Option<TxSession> {
    state.sessions.lock().unwrap().remove(id)
}

fn put(state: &State<'_, TxState>, id: &str, session: TxSession) {
    state.sessions.lock().unwrap().insert(id.to_string(), session);
}

fn no_session(id: &str) -> String {
    format!("No open transaction for {id}. Start one before running statements in it.")
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Open a transaction and hold its connection until commit or rollback.
#[tauri::command]
pub async fn tx_begin(
    db: State<'_, DbState>,
    tx: State<'_, TxState>,
    session_id: String,
) -> Result<TxStatus, String> {
    sweep_idle(&tx);
    if tx.sessions.lock().unwrap().contains_key(&session_id) {
        return Err("A transaction is already open here. Commit or roll it back first.".into());
    }

    let mut conn = match require_conn(&db)? {
        ActiveConnection::Postgres(pool) => TxConn::Postgres(
            pool.acquire().await.map_err(|e| format!("Could not open a transaction: {e}"))?,
        ),
        ActiveConnection::Mysql(pool) => TxConn::Mysql(
            pool.acquire().await.map_err(|e| format!("Could not open a transaction: {e}"))?,
        ),
        ActiveConnection::Sqlite(pool) => TxConn::Sqlite(
            pool.acquire().await.map_err(|e| format!("Could not open a transaction: {e}"))?,
        ),
        ActiveConnection::Duckdb(_) | ActiveConnection::Mssql(_) => {
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
    };

    run_statement(&mut conn, "BEGIN").await.map_err(|e| format!("Could not begin: {e}"))?;
    // A statement that hangs inside a transaction holds its locks for as long as
    // it runs, so cap it the same way the autocommit path does.
    if let TxConn::Postgres(_) = conn {
        let _ = run_statement(
            &mut conn,
            &format!("SET LOCAL statement_timeout = {EXECUTE_SQL_TIMEOUT_MS}"),
        )
        .await;
    }

    let now = Instant::now();
    let session = TxSession {
        conn,
        statements: 0,
        rows_affected: 0,
        started: now,
        last_used: now,
    };
    let status = TxStatus::of(&session);
    put(&tx, &session_id, session);
    Ok(status)
}

/// Run SQL inside the open transaction. Nothing is visible to anyone else until
/// the transaction commits.
#[tauri::command]
pub async fn tx_execute(
    tx: State<'_, TxState>,
    session_id: String,
    sql: String,
) -> Result<SqlResult, String> {
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

    let mut session = take(&tx, &session_id).ok_or_else(|| no_session(&session_id))?;
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
    put(&tx, &session_id, session);

    result.map(|mut r| {
        r.query_ms = started.elapsed().as_millis() as u64;
        r.sql = trimmed;
        r
    })
}

/// Commit the open transaction and give the connection back.
#[tauri::command]
pub async fn tx_commit(tx: State<'_, TxState>, session_id: String) -> Result<TxStatus, String> {
    let mut session = take(&tx, &session_id).ok_or_else(|| no_session(&session_id))?;
    match run_statement(&mut session.conn, "COMMIT").await {
        // The connection is dropped with `session`, returning it to the pool.
        Ok(_) => Ok(TxStatus::closed()),
        Err(e) => {
            // The commit failed, so the transaction is neither applied nor
            // cleanly closed. Drop the connection rather than hand back a
            // session whose state nobody can describe.
            Err(format!("Could not commit: {e}"))
        }
    }
}

/// Roll the open transaction back and give the connection back.
#[tauri::command]
pub async fn tx_rollback(tx: State<'_, TxState>, session_id: String) -> Result<TxStatus, String> {
    let mut session = take(&tx, &session_id).ok_or_else(|| no_session(&session_id))?;
    run_statement(&mut session.conn, "ROLLBACK")
        .await
        .map_err(|e| format!("Could not roll back: {e}"))?;
    Ok(TxStatus::closed())
}

/// What the UI needs to render the transaction indicator.
#[tauri::command]
pub async fn tx_status(tx: State<'_, TxState>, session_id: String) -> Result<TxStatus, String> {
    sweep_idle(&tx);
    let sessions = tx.sessions.lock().unwrap();
    Ok(sessions.get(&session_id).map_or_else(TxStatus::closed, TxStatus::of))
}

// ── Execution ─────────────────────────────────────────────────────────────────

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
