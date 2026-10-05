use std::sync::OnceLock;
use tauri::Emitter;

// ── Shared HTTP client for AI requests ────────────────────────────────────────
// One client for the whole process lifetime. reqwest maintains an internal
// connection pool, so subsequent requests to the same host reuse the TLS
// session and avoid the ~300-700 ms handshake on every call.
static AI_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn ai_http_client() -> &'static reqwest::Client {
    AI_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .pool_max_idle_per_host(4)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .build()
            .expect("failed to build AI HTTP client")
    })
}

// ── Streaming cancellation ────────────────────────────────────────────────────
// In-flight streaming requests, keyed by request_id. Stop in the UI fires
// `ai_fetch_cancel`, which breaks the drain loop below; dropping the byte
// stream closes the connection, so a local model server (Ollama, LM Studio)
// stops generating instead of finishing the whole completion into the void.
static AI_CANCELS: OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<()>>>,
> = OnceLock::new();

fn ai_cancel_senders(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<()>>>
{
    AI_CANCELS.get_or_init(Default::default)
}

/// Cancel an in-flight streaming `ai_fetch`. Fires the request's oneshot, which
/// interrupts the drain loop even while it is parked awaiting the next chunk.
#[tauri::command]
pub fn ai_fetch_cancel(request_id: String) {
    let sender = ai_cancel_senders()
        .lock()
        .ok()
        .and_then(|mut map| map.remove(&request_id));
    if let Some(tx) = sender {
        tx.send(()).ok();
    }
}

/// Proxy an OpenAI-compatible chat completions request through the Rust backend,
/// bypassing WebView CORS restrictions for local AI models (Ollama, LM Studio, etc.).
///
/// For streaming requests, response chunks are emitted as Tauri events:
///   `ai-stream-{request_id}`       - text chunk payload
///   `ai-stream-done-{request_id}`  - stream finished
///   `ai-stream-error-{request_id}` - error message payload
#[tauri::command]
pub async fn ai_fetch(
    app: tauri::AppHandle,
    url: String,
    api_key: Option<String>,
    body: serde_json::Value,
    stream: bool,
    request_id: String,
    extra_headers: Option<std::collections::HashMap<String, String>>,
) -> Result<Option<serde_json::Value>, String> {
    let client = ai_http_client();
    let mut builder = client
        .post(&url)
        .header("Content-Type", "application/json");

    if let Some(key) = &api_key {
        if !key.is_empty() {
            builder = builder.header("Authorization", format!("Bearer {}", key));
        }
    }

    if let Some(headers) = extra_headers {
        for (k, v) in headers {
            builder = builder.header(k, v);
        }
    }

    let response = builder.json(&body).send().await.map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        let detail: String = text.chars().take(400).collect();
        return Err(format!("AI API {}: {}", status, detail));
    }

    if stream {
        use futures::StreamExt;
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
        if let Ok(mut map) = ai_cancel_senders().lock() {
            map.insert(request_id.clone(), cancel_tx);
        }
        let mut byte_stream = response.bytes_stream();
        loop {
            let chunk = tokio::select! {
                _ = &mut cancel_rx => break,
                chunk = byte_stream.next() => chunk,
            };
            let Some(chunk) = chunk else {
                app.emit(&format!("ai-stream-done-{}", request_id), true).ok();
                break;
            };
            match chunk {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    app.emit(&format!("ai-stream-{}", request_id), text).ok();
                }
                Err(e) => {
                    app.emit(&format!("ai-stream-error-{}", request_id), e.to_string()).ok();
                    break;
                }
            }
        }
        if let Ok(mut map) = ai_cancel_senders().lock() {
            map.remove(&request_id);
        }
        Ok(None)
    } else {
        let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
        Ok(Some(json))
    }
}

/// Stable device identifier, used to key the free AI tier's per-device quota.
///
/// Reuses the same value the licensing/trial flow already sends, so the free tier
/// needs no second identity and no signup. It is NOT a secret and NOT a
/// credential: it identifies a device to our gateway for rate limiting, nothing
/// more, and the gateway must never let it authorise anything else.
#[tauri::command]
pub fn ai_device_id() -> String {
    crate::license::device_id()
}

/// The models ollama.com currently offers, name and size.
///
/// Fetched here rather than from the webview because ollama.com sends no
/// `Access-Control-Allow-Origin`, so a browser fetch fails outright - and the
/// packaged app's origin differs per platform (`tauri://localhost` on macOS,
/// `http://tauri.localhost` on Windows and Linux), which would make a CORS
/// dependency fail differently on each. Going through Rust removes the question
/// on every OS, the same reason `ai_list_models` exists.
#[tauri::command]
pub async fn ollama_registry() -> Result<serde_json::Value, String> {
    let response = ai_http_client()
        .get("https://ollama.com/api/tags")
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "Timed out reaching ollama.com".to_string()
            } else {
                format!("Could not reach ollama.com: {e}")
            }
        })?;

    if !response.status().is_success() {
        return Err(format!("ollama.com returned {}", response.status().as_u16()));
    }
    response.json().await.map_err(|e| e.to_string())
}

/// The models a server actually has, so the picker never guesses a tag -
/// a hardcoded preset like `llama3.1` fails against an install that has
/// `llama3.1:8b`. Goes through Rust for the same reason `ai_fetch` does: the
/// WebView can't reach localhost cross-origin.
#[tauri::command]
pub async fn ai_list_models(url: String, api_key: Option<String>) -> Result<Vec<String>, String> {
    let mut builder = ai_http_client()
        .get(&url)
        .timeout(std::time::Duration::from_secs(8));

    if let Some(key) = &api_key {
        if !key.is_empty() {
            builder = builder.header("Authorization", format!("Bearer {}", key));
        }
    }

    let response = builder.send().await.map_err(|e| {
        // reqwest's chained source text is unreadable in a dialog; the two cases a
        // user can act on are "nothing is listening" and "it timed out".
        if e.is_connect() {
            format!("Could not reach {url} - is the server running?")
        } else if e.is_timeout() {
            format!("Timed out reaching {url}")
        } else {
            e.to_string()
        }
    })?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        let detail: String = text.chars().take(200).collect();
        return Err(format!("AI API {}: {}", status, detail));
    }

    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    Ok(json
        .get("data")
        .and_then(|d| d.as_array())
        .map(|models| {
            models
                .iter()
                .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default())
}

/// Write text content to a path chosen by the user via a native save dialog.
/// Uses async I/O so large export files don't block the Tokio executor thread.
#[tauri::command]
pub async fn save_file(path: String, content: String) -> Result<(), String> {
    tokio::fs::write(&path, content).await.map_err(|e| e.to_string())
}

/// Write binary content to a path chosen by the user via a native save dialog.
/// The webview's `<a download>` is a no-op inside WKWebView, so exports that
/// produce bytes (PNG diagrams) come through here instead.
///
/// Payload is base64 rather than `Vec<u8>`: the IPC bridge has no binary channel,
/// so raw bytes would cross it as a JSON array - one number per byte, roughly 4x
/// the payload for a multi-megabyte image.
#[tauri::command]
pub async fn save_file_bytes(path: String, base64: String) -> Result<(), String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64.as_bytes())
        .map_err(|e| format!("Malformed export payload: {e}"))?;
    tokio::fs::write(&path, bytes).await.map_err(|e| e.to_string())
}

/// Search the web on the agent's behalf. Gated in the UI by the AI web-access
/// setting; the command itself is unconditional so the MCP server can use it too.
#[tauri::command]
pub async fn ai_web_search(
    query: String,
    limit: Option<usize>,
) -> Result<Vec<crate::web_search::SearchHit>, String> {
    crate::web_search::search(query, limit.unwrap_or(5)).await
}

/// Fetch one page and return its readable text, for following up a search hit.
#[tauri::command]
pub async fn ai_fetch_page(url: String) -> Result<String, String> {
    crate::web_search::fetch_page(url).await
}

/// Read a text file from disk - used by the notebook open flow.
#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    tokio::fs::read_to_string(&path).await.map_err(|e| e.to_string())
}

/// Open another Stroke window.
///
/// Windows share this process, and with it the active connection: the backend
/// holds one pool, not one per window. That is the point - the second window is
/// another view of the same session (a table on one screen, the SQL editor on
/// the other), not a second client - but it does mean switching connection in
/// either window switches it for both.
///
/// The chrome is built to match the main window exactly: frameless everywhere,
/// macOS keeping its native traffic lights, and the dark base colour painted
/// before the first frame so no white flash escapes while the frontend boots.
#[tauri::command]
pub fn open_new_window(app: tauri::AppHandle) -> Result<(), String> {
    // Labels must be unique and stable-ish; the counter restarts with the app,
    // and a closed label is free to reuse, so probe for the first gap.
    let label = (2..64)
        .map(|n| format!("main-{n}"))
        .find(|l| app.webview_windows().get(l.as_str()).is_none())
        .ok_or_else(|| "Too many windows are already open".to_string())?;

    let mut builder = tauri::WebviewWindowBuilder::new(
        &app,
        &label,
        tauri::WebviewUrl::App("/".into()),
    )
    .title("Stroke")
    .inner_size(1280.0, 800.0)
    .min_inner_size(960.0, 600.0)
    .resizable(true)
    // Hidden until its page reveals itself, same as the main window.
    .visible(false)
    .background_color(tauri::window::Color(8, 8, 8, 255));

    #[cfg(target_os = "macos")]
    {
        builder = builder
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true);
    }
    #[cfg(not(target_os = "macos"))]
    {
        builder = builder.decorations(false);
    }

    let window = builder.build().map_err(|e| e.to_string())?;
    crate::arm_reveal_failsafe(&window);
    Ok(())
}

/// Show the calling window. Windows are built hidden and the frontend calls this
/// on its first finished screen (src/lib/app-reveal.js). An app command rather
/// than the core `window.show` so it works from every window label - the
/// capability file only grants core window permissions to `main`.
#[tauri::command]
pub fn reveal_window(window: tauri::WebviewWindow) {
    if !window.is_visible().unwrap_or(false) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Restart the application - called after an update is installed.
#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) {
    app.restart();
}

/// Toggle the WebView developer tools. Only functional in debug builds;
/// in release builds this is a no-op so the command stays safe to expose.
#[tauri::command]
pub fn toggle_devtools(window: tauri::WebviewWindow) {
    #[cfg(debug_assertions)]
    {
        if window.is_devtools_open() {
            window.close_devtools();
        } else {
            window.open_devtools();
        }
    }
    #[cfg(not(debug_assertions))]
    let _ = window;
}

use crate::db::{
    connect, connect_clickhouse, connect_d1, connect_duckdb, connect_libsql, connect_mssql, connect_mysql, connect_redis, connect_sqlite, disconnect,
    delete_table_row, delete_table_rows, execute_ddl, execute_sql, execute_sql_multi, get_table_rows, count_table_rows, insert_table_row,
    list_schemas, list_tables, list_indexes, list_enums, list_functions, list_triggers, list_sequences, ping_connection, table_row_counts,
    truncate_table, drop_table, get_table_column_structure, get_schema_column_structure, get_incoming_foreign_keys, get_table_ddl as db_get_table_ddl,
    test_clickhouse_connection, test_connection, test_d1_connection, test_duckdb_connection, test_libsql_connection, test_mssql_connection, test_mysql_connection, test_redis_connection, test_sqlite_connection,
    update_table_cell, fetch_cell_value, ConnectionConfig, D1Config, DbState, EnumInfo, FunctionInfo, ExplainResult, IndexInfo, LibSqlConfig,
    SqlResult, SqliteConfig, TableInfo, TableRowCount, TableRows, TriggerInfo, SequenceInfo,
    ColumnStructureRow, TableColumnStructure, IncomingForeignKey, InsertRowResult, TunnelState,
    explain_pg, explain_mysql, explain_sqlite, explain_from_text_lines, explain_from_sqlite_plan,
};
use crate::db::connection::{require_conn, ClickhouseConfig, DuckdbConfig, MssqlConfig, MysqlConfig, RedisConfig};
use crate::db::ActiveConnection;
use serde_json::Value;
use std::collections::HashMap;
use tauri::{Manager, State};

// ── PostgreSQL ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn test_postgres_connection(config: ConnectionConfig) -> Result<(), String> {
    test_connection(config).await
}

#[tauri::command]
pub async fn connect_postgres(
    state: State<'_, DbState>,
    tunnel_state: State<'_, TunnelState>,
    config: ConnectionConfig,
) -> Result<(), String> {
    connect(state, tunnel_state, config).await
}

// ── SQLite ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn test_sqlite(config: SqliteConfig) -> Result<(), String> {
    test_sqlite_connection(config).await
}

#[tauri::command]
pub async fn connect_sqlite_db(
    state: State<'_, DbState>,
    config: SqliteConfig,
) -> Result<(), String> {
    connect_sqlite(state, config).await
}

// ── MySQL ─────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn test_mysql(config: MysqlConfig) -> Result<(), String> {
    test_mysql_connection(config).await
}

#[tauri::command]
pub async fn connect_mysql_db(
    state: State<'_, DbState>,
    tunnel_state: State<'_, TunnelState>,
    config: MysqlConfig,
) -> Result<(), String> {
    connect_mysql(state, tunnel_state, config).await
}

// ── Cloudflare D1 ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn test_d1(config: D1Config) -> Result<(), String> {
    test_d1_connection(config).await
}

#[tauri::command]
pub async fn connect_d1_db(
    state: State<'_, DbState>,
    config: D1Config,
) -> Result<(), String> {
    connect_d1(state, config).await
}

// ── LibSQL / Turso ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn test_libsql(config: LibSqlConfig) -> Result<(), String> {
    test_libsql_connection(config).await
}

#[tauri::command]
pub async fn connect_libsql_db(state: State<'_, DbState>, config: LibSqlConfig) -> Result<(), String> {
    connect_libsql(state, config).await
}

// ── PostHog ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn test_posthog(config: crate::db::connection::PosthogConfig) -> Result<(), String> {
    crate::db::connection::test_posthog_connection(config).await
}

#[tauri::command]
pub async fn connect_posthog_db(
    state: State<'_, DbState>,
    config: crate::db::connection::PosthogConfig,
) -> Result<(), String> {
    crate::db::connection::connect_posthog(state, config).await
}

// ── ClickHouse ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn test_clickhouse(config: ClickhouseConfig) -> Result<(), String> {
    test_clickhouse_connection(config).await
}

#[tauri::command]
pub async fn connect_clickhouse_db(state: State<'_, DbState>, config: ClickhouseConfig) -> Result<(), String> {
    connect_clickhouse(state, config).await
}

// ── Redis ─────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn test_redis(config: RedisConfig) -> Result<(), String> {
    test_redis_connection(config).await
}

#[tauri::command]
pub async fn connect_redis_db(state: State<'_, DbState>, config: RedisConfig) -> Result<(), String> {
    connect_redis(state, config).await
}

/// Non-blocking keyspace iteration for the active Redis connection. Returns one
/// SCAN page (next cursor + keys); the frontend loops until the cursor is "0".
#[tauri::command]
pub async fn redis_scan(
    state: State<'_, DbState>,
    cursor: String,
    pattern: Option<String>,
    count: u32,
) -> Result<crate::db::redis::ScanReply, String> {
    match require_conn(&state)? {
        ActiveConnection::Redis(cfg) => crate::db::redis::scan(&cfg, &cursor, pattern, count).await,
        _ => Err("Not connected to a Redis database".into()),
    }
}

// ── DuckDB ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn test_duckdb(config: DuckdbConfig) -> Result<(), String> {
    test_duckdb_connection(config).await
}

#[tauri::command]
pub async fn connect_duckdb_db(state: State<'_, DbState>, config: DuckdbConfig) -> Result<(), String> {
    connect_duckdb(state, config).await
}

// ── MS SQL Server ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn test_mssql(config: MssqlConfig) -> Result<(), String> {
    test_mssql_connection(config).await
}

#[tauri::command]
pub async fn connect_mssql_db(state: State<'_, DbState>, config: MssqlConfig) -> Result<(), String> {
    connect_mssql(state, config).await
}

// ── Live mode ─────────────────────────────────────────────────────────────────

use crate::db::live::{self, LiveState};

/// Start pushing change notifications for `schema.table` (Postgres / SQLite only).
#[tauri::command]
pub async fn live_start(
    app: tauri::AppHandle,
    state: State<'_, DbState>,
    live_state: State<'_, LiveState>,
    schema: String,
    table: String,
) -> Result<(), String> {
    let conn = require_conn(&state)?;
    live::start(app, conn, &live_state, schema, table)
}

/// Stop live mode.
#[tauri::command]
pub async fn live_stop(live_state: State<'_, LiveState>) -> Result<(), String> {
    live::stop(&live_state);
    Ok(())
}

// ── Shared disconnect ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn disconnect_postgres(
    state: State<'_, DbState>,
    tunnel_state: State<'_, TunnelState>,
    live_state: State<'_, LiveState>,
) -> Result<(), String> {
    live::stop(&live_state);
    disconnect(state, tunnel_state).await
}

/// Flatten an EXPLAIN result set to plan lines (first column of each row) for
/// engines whose EXPLAIN returns a textual, indented plan.
fn explain_rows_to_lines(res: &SqlResult) -> Vec<String> {
    res.rows
        .iter()
        .filter_map(|r| r.first())
        .map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))
        .collect()
}

/// Turn a SQL Server SHOWPLAN_XML document into indented plan lines - one line
/// per `<RelOp>` operator, indented by its RelOp-nesting depth - so the shared
/// `explain_from_text_lines` builder produces the same `{Node Type, Plans}` tree
/// the renderer already draws for other engines. Uses a lightweight tag scan
/// (no XML crate is available) that only tracks `<RelOp>` open/close: the
/// intermediate elements between a parent and child RelOp don't affect depth,
/// so the counter mirrors the operator hierarchy exactly.
fn mssql_plan_lines(xml: &str) -> Vec<String> {
    // Read the value of `name="..."` out of an element's attribute text.
    fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
        let key = format!("{name}=\"");
        let start = tag.find(&key)? + key.len();
        let rest = &tag[start..];
        rest.find('"').map(|end| &rest[..end])
    }

    let mut lines: Vec<String> = Vec::new();
    let mut depth: usize = 0;
    let mut rest = xml;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt + 1..];
        let Some(gt) = rest.find('>') else { break };
        let tag = &rest[..gt];
        rest = &rest[gt + 1..];

        if tag == "/RelOp" {
            depth = depth.saturating_sub(1);
        } else if let Some(after) = tag.strip_prefix("RelOp") {
            // Opening RelOp tag (the name is followed by whitespace before its
            // attributes, or nothing at all) - not `RelOpXyz` or a comment.
            if after.is_empty() || after.starts_with(char::is_whitespace) {
                let phys = attr(tag, "PhysicalOp");
                let logi = attr(tag, "LogicalOp");
                let label = match (phys, logi) {
                    (Some(p), Some(l)) if !l.eq_ignore_ascii_case(p) => format!("{p} ({l})"),
                    (Some(p), _) => p.to_string(),
                    (None, Some(l)) => l.to_string(),
                    (None, None) => "RelOp".to_string(),
                };
                lines.push(format!("{}{label}", "  ".repeat(depth)));
                // `<RelOp .../>` (self-closing) carries no child operators.
                if !tag.trim_end().ends_with('/') {
                    depth += 1;
                }
            }
        }
    }
    lines
}

#[tauri::command]
pub async fn pg_explain_sql(
    state: State<'_, DbState>,
    sql: String,
) -> Result<ExplainResult, String> {
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => explain_pg(&pool, &sql).await,
        ActiveConnection::Mysql(pool) => explain_mysql(&pool, &sql).await,
        ActiveConnection::Sqlite(pool) => explain_sqlite(&pool, &sql).await,
        ActiveConnection::Duckdb(h) => {
            let res = crate::db::duckdb::execute_sql(&h, &format!("EXPLAIN {sql}")).await?;
            Ok(explain_from_text_lines(explain_rows_to_lines(&res), "duckdb"))
        }
        ActiveConnection::Clickhouse(cfg) => {
            let res = crate::db::clickhouse::query(&cfg, &format!("EXPLAIN {sql}")).await?;
            Ok(explain_from_text_lines(explain_rows_to_lines(&res), "clickhouse"))
        }
        ActiveConnection::D1(cfg) => {
            // D1 is SQLite over HTTP - EXPLAIN QUERY PLAN returns the same
            // (id, parent, notused, detail) rows the SQLite path builds its tree from.
            let res = crate::db::d1::query(&cfg, &format!("EXPLAIN QUERY PLAN {sql}"), vec![]).await?;
            Ok(explain_from_sqlite_plan(&res, "d1"))
        }
        ActiveConnection::Mssql(h) => {
            // SHOWPLAN_XML is session state: turning it ON makes the *next* query
            // return its plan as an XML column instead of executing it. All three
            // statements must therefore run on the same tiberius session - and it
            // is a single persistent connection (no pool), so holding the guard
            // across the sequence keeps them on that session with no interleaving.
            // `SET SHOWPLAN_XML ON` must also be alone in its batch, which each
            // separate `simple_query` call satisfies.
            let query = {
                let q = sql.trim().trim_end_matches(';').trim();
                match q.get(..8) {
                    Some(p) if p.eq_ignore_ascii_case("EXPLAIN ") => q[8..].trim_start(),
                    _ => q,
                }
            };
            let mut client = h.lock().await;

            // Bail before running the query if plan capture can't be enabled -
            // otherwise the query would execute for real (e.g. a DML statement).
            match client.simple_query("SET SHOWPLAN_XML ON").await {
                Ok(stream) => {
                    stream.into_results().await.map_err(|e| format!("EXPLAIN failed: {e}"))?;
                }
                Err(e) => return Err(format!("EXPLAIN failed: {e}")),
            }
            // Run the target query; with SHOWPLAN on it yields the plan XML.
            let captured = match client.simple_query(query).await {
                Ok(stream) => stream.into_results().await,
                Err(e) => Err(e),
            };
            // Restore session state no matter what - this connection is reused
            // for every later query, so it must not stay in plan-only mode.
            if let Ok(off) = client.simple_query("SET SHOWPLAN_XML OFF").await {
                let _ = off.into_results().await;
            }

            let result_sets = captured.map_err(|e| format!("EXPLAIN failed: {e}"))?;
            let mut xml = String::new();
            for rows in &result_sets {
                for row in rows {
                    if let Ok(Some(x)) = row.try_get::<&tiberius::xml::XmlData, _>(0usize) {
                        xml.push_str(x.as_ref());
                    } else if let Ok(Some(s)) = row.try_get::<&str, _>(0usize) {
                        xml.push_str(s);
                    }
                }
            }
            drop(client);

            if xml.trim().is_empty() {
                return Err("Empty EXPLAIN result".into());
            }
            Ok(explain_from_text_lines(mssql_plan_lines(&xml), "mssql"))
        }
        _ => Err("EXPLAIN isn't supported for this engine yet".into()),
    }
}

// ── DB-agnostic query commands ────────────────────────────────────────────────

#[tauri::command]
pub async fn pg_list_schemas(state: State<'_, DbState>) -> Result<Vec<String>, String> {
    list_schemas(state).await
}

#[tauri::command]
pub async fn pg_list_tables(
    state: State<'_, DbState>,
    schema: String,
) -> Result<Vec<TableInfo>, String> {
    list_tables(state, schema).await
}

/// Background pass: exact COUNT(*) for tables `pg_list_tables` returned with an
/// unknown (-1) row count. See `db::schema::table_row_counts`.
#[tauri::command]
pub async fn pg_table_row_counts(
    state: State<'_, DbState>,
    schema: String,
    tables: Vec<String>,
) -> Result<Vec<TableRowCount>, String> {
    table_row_counts(state, schema, tables).await
}

#[tauri::command]
pub async fn pg_list_indexes(
    state: State<'_, DbState>,
    schema: String,
) -> Result<Vec<IndexInfo>, String> {
    list_indexes(state, schema).await
}

#[tauri::command]
pub async fn pg_get_table_column_structure(
    state: State<'_, DbState>,
    schema: String,
    table: String,
) -> Result<Vec<ColumnStructureRow>, String> {
    get_table_column_structure(state, schema, table).await
}

/// Column structure for every table in one schema - one call instead of one per
/// table. Used by the ER diagram, which needs the whole schema up front.
#[tauri::command]
pub async fn pg_get_schema_column_structure(
    state: State<'_, DbState>,
    schema: String,
) -> Result<Vec<TableColumnStructure>, String> {
    get_schema_column_structure(state, schema).await
}

#[tauri::command]
pub async fn pg_get_incoming_foreign_keys(
    state: State<'_, DbState>,
    schema: String,
    table: String,
) -> Result<Vec<IncomingForeignKey>, String> {
    get_incoming_foreign_keys(state, schema, table).await
}

#[tauri::command]
pub async fn pg_list_enums(
    state: State<'_, DbState>,
    schema: String,
) -> Result<Vec<EnumInfo>, String> {
    list_enums(state, schema).await
}

#[tauri::command]
pub async fn pg_list_triggers(
    state: State<'_, DbState>,
    schema: String,
) -> Result<Vec<TriggerInfo>, String> {
    list_triggers(state, schema).await
}

#[tauri::command]
pub async fn pg_list_sequences(
    state: State<'_, DbState>,
    schema: String,
) -> Result<Vec<SequenceInfo>, String> {
    list_sequences(state, schema).await
}

#[tauri::command]
pub async fn pg_list_functions(
    state: State<'_, DbState>,
    schema: String,
) -> Result<Vec<FunctionInfo>, String> {
    list_functions(state, schema).await
}

/// Functions, procedures, triggers, sequences, types and events in one schema,
/// and which of those kinds the engine has (the sidebar's Objects tab).
#[tauri::command]
pub async fn list_db_objects(
    state: State<'_, DbState>,
    schema: String,
) -> Result<crate::db::objects::ObjectListing, String> {
    crate::db::objects::list_db_objects(state, schema).await
}

#[tauri::command]
pub async fn get_object_definition(
    state: State<'_, DbState>,
    kind: String,
    schema: String,
    name: String,
    args: Option<String>,
    table: Option<String>,
) -> Result<String, String> {
    crate::db::objects::get_object_definition(state, kind, schema, name, args.unwrap_or_default(), table.unwrap_or_default()).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn drop_db_object(
    state: State<'_, DbState>,
    kind: String,
    schema: String,
    name: String,
    args: Option<String>,
    table: Option<String>,
    subtype: Option<String>,
    cascade: Option<bool>,
) -> Result<String, String> {
    crate::db::objects::drop_object(
        state,
        kind,
        schema,
        name,
        args.unwrap_or_default(),
        table.unwrap_or_default(),
        subtype.unwrap_or_default(),
        cascade.unwrap_or(false),
    )
    .await
}

/// Table and view comments for one schema (the sidebar's comments setting).
#[tauri::command]
pub async fn list_object_comments(
    state: State<'_, DbState>,
    schema: String,
) -> Result<Vec<crate::db::objects::ObjectComment>, String> {
    crate::db::objects::list_object_comments(state, schema).await
}

#[tauri::command]
pub async fn ping_db_connection(state: State<'_, DbState>) -> Result<(), String> {
    ping_connection(state).await
}

#[tauri::command]
pub async fn pg_truncate_table(
    state: State<'_, DbState>,
    schema: String,
    table: String,
) -> Result<(), String> {
    truncate_table(state, schema, table).await
}

#[tauri::command]
pub async fn pg_drop_table(
    state: State<'_, DbState>,
    schema: String,
    table: String,
    cascade: bool,
    // `table` (the default), `view` or `materialized_view`.
    kind: Option<String>,
) -> Result<(), String> {
    drop_table(state, schema, table, cascade, kind).await
}

/// Copy a database on the current server under a new name, structure and rows.
#[tauri::command]
pub async fn pg_clone_database(
    state: State<'_, DbState>,
    source: String,
    target: String,
) -> Result<crate::db::admin::CloneSummary, String> {
    let conn = crate::db::connection::require_conn(&state)?;
    crate::db::admin::clone_database(&conn, &source, &target).await
}

#[tauri::command]
pub async fn get_table_ddl(
    state: State<'_, DbState>,
    schema: String,
    table: String,
) -> Result<String, String> {
    db_get_table_ddl(state, schema, table).await
}

#[tauri::command]
pub async fn get_table_ddl_on_connection(
    config: crate::db::AnyConnectionConfig,
    schema: String,
    table: String,
) -> Result<String, String> {
    crate::db::get_table_ddl_on_conn(config, schema, table).await
}

#[tauri::command]
pub async fn pg_get_table_rows(
    state: State<'_, DbState>,
    schema: String,
    table: String,
    limit: i64,
    offset: i64,
    search: Option<String>,
    search_is_regex: Option<bool>,
    // Optional - defaults to false. Case-sensitive substring search.
    search_case_sensitive: Option<bool>,
    sort_column: Option<String>,
    sort_direction: Option<String>,
    filters: Option<Vec<crate::db::RowFilter>>,
    // Optional - defaults to true (full metadata). Repeat fetches pass false.
    include_meta: Option<bool>,
    // Optional - defaults to true. When false, the count is skipped (total = -1)
    // and fetched separately via `pg_count_table_rows` so rows paint immediately.
    include_count: Option<bool>,
    // Multi-column sort keys (Postgres); overrides sort_column when non-empty.
    sorts: Option<Vec<crate::db::SortSpec>>,
    // Keyset (cursor) pagination anchor; absent = classic OFFSET.
    keyset: Option<crate::db::KeysetCursor>,
    // Null placement for ORDER BY ("first"/"last"); absent keeps NULLS LAST.
    // Applied on dialects with explicit null placement (Postgres, SQLite, D1/libSQL).
    nulls_order: Option<String>,
    // Optional - defaults to true. When false, a wide column is fetched whole
    // like every other column, which is what the app did before and what the
    // setting turns back on for anyone who wants it.
    preview_wide: Option<bool>,
) -> Result<TableRows, String> {
    get_table_rows(
        state,
        schema,
        table,
        limit,
        offset,
        search,
        search_is_regex.unwrap_or(false),
        search_case_sensitive.unwrap_or(false),
        sort_column,
        sort_direction,
        filters,
        include_meta.unwrap_or(true),
        include_count.unwrap_or(true),
        sorts.unwrap_or_default(),
        keyset,
        nulls_order,
        preview_wide.unwrap_or(true),
    )
    .await
}

/// Background row count for the main grid (see `count_table_rows`). Called
/// after `pg_get_table_rows` returns rows with `include_count: false`, so the
/// total streams in without blocking the initial paint.
#[tauri::command]
pub async fn pg_count_table_rows(
    state: State<'_, DbState>,
    schema: String,
    table: String,
    search: Option<String>,
    search_is_regex: Option<bool>,
    // Optional - defaults to false. Mirrors the rows query's own flag.
    search_case_sensitive: Option<bool>,
    filters: Option<Vec<crate::db::RowFilter>>,
    // Optional - defaults to false. True counts a large table instead of
    // taking the planner's estimate (the "Exact row count" setting).
    exact: Option<bool>,
) -> Result<i64, String> {
    count_table_rows(
        state,
        schema,
        table,
        search,
        search_is_regex.unwrap_or(false),
        search_case_sensitive.unwrap_or(false),
        filters,
        exact.unwrap_or(false),
    )
    .await
}

#[tauri::command]
pub async fn pg_get_column_stats(
    state: State<'_, DbState>,
    schema: String,
    table: String,
    column: String,
) -> Result<crate::db::ColumnStats, String> {
    crate::db::get_column_stats(state, schema, table, column).await
}

// ── Geo view (PostGIS) ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn geo_overview(
    state: State<'_, DbState>,
) -> Result<crate::db::GeoOverview, String> {
    crate::db::geo_overview(state).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn geo_features(
    state: State<'_, DbState>,
    schema: String,
    table: String,
    column: String,
    kind: String,
    srid: i32,
    geom_type: String,
    bbox: Option<crate::db::GeoBbox>,
    limit: i64,
    simplify: f64,
    cluster_cell: f64,
    filters: Option<Vec<crate::db::RowFilter>>,
    include_extent: bool,
) -> Result<crate::db::GeoFeatures, String> {
    crate::db::geo_features(
        state, schema, table, column, kind, srid, geom_type, bbox, limit, simplify, cluster_cell,
        filters, include_extent,
    )
    .await
}

// ── Instance Insights (PostgreSQL + MySQL monitoring dashboard) ────────────────

#[tauri::command]
pub async fn instance_version(
    state: State<'_, DbState>,
) -> Result<crate::db::InstanceVersion, String> {
    crate::db::instance_version(state).await
}

#[tauri::command]
pub async fn instance_activity(
    state: State<'_, DbState>,
) -> Result<crate::db::InstanceActivity, String> {
    crate::db::instance_activity(state).await
}

#[tauri::command]
pub async fn instance_state(
    state: State<'_, DbState>,
) -> Result<crate::db::InstanceState, String> {
    crate::db::instance_state(state).await
}

#[tauri::command]
pub async fn instance_config(
    state: State<'_, DbState>,
) -> Result<Vec<crate::db::ConfigSetting>, String> {
    crate::db::instance_config(state).await
}

/// Write one server setting (Postgres `ALTER SYSTEM`, MySQL `SET PERSIST`).
/// `value = None` resets it to the server default.
#[tauri::command]
pub async fn instance_set_config(
    state: State<'_, DbState>,
    name: String,
    value: Option<String>,
) -> Result<crate::db::SetConfigResult, String> {
    crate::db::instance_set_config(state, name, value).await
}

/// Read-only lint pass over the connected database (Advisor tab). Catalog queries
/// only - it never reads user rows and never writes.
#[tauri::command]
pub async fn advisor_scan(
    state: State<'_, DbState>,
) -> Result<crate::db::advisor::AdvisorReport, String> {
    let conn = crate::db::connection::require_conn(&state)?;
    crate::db::advisor::scan(&conn).await
}

#[tauri::command]
pub async fn instance_replication(
    state: State<'_, DbState>,
) -> Result<crate::db::InstanceReplication, String> {
    crate::db::instance_replication(state).await
}

/// `queryId` (optional) names this run so `cancel_query` can target it. Omitting
/// it keeps the single-slot behaviour older callers relied on.
#[tauri::command]
pub async fn pg_execute_sql(
    state: State<'_, DbState>,
    sql: String,
    query_id: Option<String>,
) -> Result<SqlResult, String> {
    execute_sql(state, sql, query_id).await
}

#[tauri::command]
pub async fn pg_execute_sql_multi(
    state: State<'_, DbState>,
    sql: String,
    query_id: Option<String>,
) -> Result<Vec<SqlResult>, String> {
    execute_sql_multi(state, sql, query_id).await
}

/// `pg_execute_sql_multi` for the SQL console: a single Postgres statement's
/// rows arrive through `on_rows` in chunks while they stream in, and the reply
/// carries none. The channel always ends with a `done` chunk, whatever the
/// engine or outcome, so the window knows every row has landed.
///
/// `keep_in_store`: the rows go to the result store under `query_id` instead
/// of through the channel, which then carries progress (Settings → Database →
/// Stream results). The window reads them with `result_window`.
#[tauri::command]
pub async fn pg_execute_sql_stream(
    state: State<'_, DbState>,
    store: State<'_, crate::db::result_store::ResultStore>,
    sql: String,
    query_id: Option<String>,
    on_rows: tauri::ipc::Channel<crate::db::RowChunk>,
    keep_in_store: Option<bool>,
) -> Result<Vec<crate::db::StatementOutcome>, String> {
    let store_id = query_id.clone().filter(|_| keep_in_store == Some(true));
    let spool = match store_id.as_deref() {
        Some(id) => Some(store.create(id)?),
        None => None,
    };
    let mut sink = crate::db::RowSink::with_spool(on_rows, spool);
    let result = crate::db::execute_sql_script(state, sql, query_id, Some(&mut sink)).await;
    // A streamed run already sent its end marker; anything else gets one now.
    sink.end();
    // Nothing streamed (a script, a write, an engine that answers in the
    // reply) or the run failed: no stored result to keep.
    if let Some(id) = store_id.as_deref() {
        let failed = result.as_ref().map_or(true, |o| o.iter().any(|s| s.error.is_some()));
        if !sink.started() || failed {
            store.drop_result(id);
        }
    }
    result
}

/// Rows `[start, start + count)` of a stored result, in its current order, as
/// a JSON array of row arrays (sent as stored, never re-encoded).
#[tauri::command]
pub async fn result_window(
    store: State<'_, crate::db::result_store::ResultStore>,
    id: String,
    start: usize,
    count: usize,
) -> Result<tauri::ipc::Response, String> {
    let result = store.get(&id).ok_or("This result is no longer available. Run the query again.")?;
    let text = tauri::async_runtime::spawn_blocking(move || result.read_window(start, count))
        .await
        .map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(text))
}

/// Sort a stored result by a column (typed by the column's database type), or
/// back to the order it streamed in with `column: null`.
#[tauri::command]
pub async fn result_sort(
    store: State<'_, crate::db::result_store::ResultStore>,
    id: String,
    column: Option<usize>,
    desc: bool,
) -> Result<(), String> {
    let result = store.get(&id).ok_or("This result is no longer available. Run the query again.")?;
    tauri::async_runtime::spawn_blocking(move || result.sort(column, desc))
        .await
        .map_err(|e| e.to_string())?
}

/// A line from the dev-only frame probe (src/lib/perf-probe.js) into the log.
#[tauri::command]
pub fn perf_log(line: String) {
    log::info!("perf: {line}");
}

/// Forget a stored result (its tab ran again or closed); its file goes with it.
#[tauri::command]
pub fn result_drop(store: State<'_, crate::db::result_store::ResultStore>, id: String) {
    store.drop_result(&id);
}

/// Execute SQL against an arbitrary saved connection without switching the
/// global active connection. Used by Data Diff for cross-host comparisons.
#[tauri::command]
pub async fn execute_sql_on_connection(
    config: crate::db::AnyConnectionConfig,
    sql: String,
) -> Result<SqlResult, String> {
    crate::db::execute_sql_on_conn(config, &sql).await
}

/// List schemas for a saved connection without switching the active connection.
#[tauri::command]
pub async fn list_schemas_on_connection(
    config: crate::db::AnyConnectionConfig,
) -> Result<Vec<String>, String> {
    crate::db::list_schemas_on_conn(config).await
}

/// List table names for a saved connection + schema without switching the active connection.
#[tauri::command]
pub async fn list_tables_on_connection(
    config: crate::db::AnyConnectionConfig,
    schema: String,
) -> Result<Vec<String>, String> {
    crate::db::list_tables_on_conn(config, schema).await
}

/// Run a DDL statement outside a transaction (required for CREATE/DROP DATABASE etc.).
#[tauri::command]
pub async fn pg_execute_ddl(state: State<'_, DbState>, sql: String) -> Result<(), String> {
    execute_ddl(state, sql).await
}

#[tauri::command]
pub async fn pg_update_table_cell(
    state: State<'_, DbState>,
    schema: String,
    table: String,
    primary_key: HashMap<String, Value>,
    column: String,
    value: Value,
) -> Result<(), String> {
    update_table_cell(state, schema, table, primary_key, column, value).await
}

/// Load one cell in full - the value a browse page deliberately did not fetch.
#[tauri::command]
pub async fn pg_fetch_cell_value(
    state: State<'_, DbState>,
    schema: String,
    table: String,
    primary_key: HashMap<String, Value>,
    column: String,
    max_bytes: Option<i64>,
) -> Result<crate::db::CellValueResult, String> {
    fetch_cell_value(state, schema, table, primary_key, column, max_bytes).await
}

#[tauri::command]
pub async fn pg_delete_table_row(
    state: State<'_, DbState>,
    schema: String,
    table: String,
    primary_key: HashMap<String, Value>,
) -> Result<(), String> {
    delete_table_row(state, schema, table, primary_key).await
}

#[tauri::command]
pub async fn pg_delete_table_rows(
    state: State<'_, DbState>,
    schema: String,
    table: String,
    primary_keys: Vec<HashMap<String, Value>>,
) -> Result<u64, String> {
    delete_table_rows(state, schema, table, primary_keys).await
}

#[tauri::command]
pub async fn pg_insert_table_row(
    state: State<'_, DbState>,
    schema: String,
    table: String,
    values: HashMap<String, Value>,
) -> Result<InsertRowResult, String> {
    insert_table_row(state, schema, table, values).await
}

// ── Cancel running query ──────────────────────────────────────────────────────

/// Cancel one running query by the `queryId` it was started with. Without an id,
/// cancels every query in flight (the old whole-app "stop" behaviour).
#[tauri::command]
pub async fn cancel_query(
    state: State<'_, DbState>,
    query_id: Option<String>,
) -> Result<(), String> {
    if let Ok(mut map) = state.cancels.lock() {
        match query_id {
            Some(id) => {
                if let Some(tx) = map.remove(&id) {
                    let _ = tx.send(());
                }
            }
            None => {
                for (_, tx) in map.drain() {
                    let _ = tx.send(());
                }
            }
        }
    }
    Ok(())
}

// ── Saved connections ─────────────────────────────────────────────────────────

/// The durable saved-connections payload, or `null` before the first write.
#[tauri::command(async)]
pub fn connections_store_read(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    crate::connection_store::read(&dir)
}

#[tauri::command(async)]
pub fn connections_store_write(app: tauri::AppHandle, json: String) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    crate::connection_store::write(&dir, &json)
}

// ── License ───────────────────────────────────────────────────────────────────

// `async` so the first call (device-fingerprint subprocess + trial-file I/O)
// runs off the main thread instead of blocking the UI during startup.
#[tauri::command(async)]
pub fn check_license_status(app: tauri::AppHandle) -> crate::license::LicenseStatus {
    match app.path().app_data_dir() {
        Ok(dir) => crate::license::check_status(&dir),
        Err(e) => crate::license::LicenseStatus::Error {
            message: e.to_string(),
        },
    }
}

#[tauri::command]
pub async fn activate_license(
    app: tauri::AppHandle,
    key: String,
) -> Result<serde_json::Value, String> {
    // 1. Verify signature locally (works offline, catches forged keys)
    let parsed = crate::license::verify_key(&key)?;
    let device_id = crate::license::device_id();

    // 2. Register with server. Blocks on seat_limit_exceeded / revoked.
    //    On network failure api_activate returns Ok(None) - we allow offline activation.
    match crate::license::api_activate(&key, &device_id).await {
        Ok(None) => {} // offline - proceed with local-only activation
        Ok(Some(_)) => {} // server accepted
        Err(e) => {
            // Convert server error codes to user-friendly messages
            let msg = match e.as_str() {
                "seat_limit_exceeded" => "Seat limit reached. Deactivate another device first.",
                "revoked" => "This license key has been revoked.",
                "expired" => "This license key has expired.",
                "not_found" => "License key not found.",
                _ => return Err(e),
            };
            return Err(msg.to_string());
        }
    }

    // 3. Save locally
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let lic = crate::license::LicenseFile {
        version: 1,
        key,
        email: parsed.email.clone(),
        plan: parsed.plan.clone(),
        issued_at: parsed.issued_at,
        expires_at: parsed.expires_at,
        device_id,
        activated_at: now,
        last_check_at: now,
    };
    crate::license::save_license(&dir, &lic)?;
    Ok(serde_json::json!({
        "email": parsed.email,
        "plan": parsed.plan,
        "issued_at": parsed.issued_at,
        "expires_at": parsed.expires_at,
    }))
}

#[tauri::command]
pub async fn deactivate_license(app: tauri::AppHandle) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    // Best-effort server deactivation (release the seat)
    if let Some(lic) = crate::license::load_license(&dir) {
        crate::license::api_deactivate(&lic.key, &lic.device_id).await;
    }
    crate::license::delete_license(&dir)
}

/// Validates the license against the server on every launch.
/// - valid: true  → update last_check_at, return Valid
/// - valid: false → delete local license, return Trial/TrialExpired
/// - network error → fall back to local signature (offline grace)
/// - no license → local trial check only (no network call)
#[tauri::command]
pub async fn run_license_check(
    app: tauri::AppHandle,
) -> crate::license::LicenseStatus {
    let dir = match app.path().app_data_dir() {
        Ok(d) => d,
        Err(e) => return crate::license::LicenseStatus::Error { message: e.to_string() },
    };

    if let Some(mut lic) = crate::license::load_license(&dir) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        match crate::license::api_check(&lic.key, &lic.device_id).await {
            Some(true) => {
                lic.last_check_at = now;
                let _ = crate::license::save_license(&dir, &lic);
            }
            Some(false) => {
                // Server says invalid/revoked - remove local license
                let _ = crate::license::delete_license(&dir);
            }
            None => {
                // Network unreachable - use local signature as offline grace
            }
        }
    }

    // No valid paid license → reconcile the trial clock with the server so a
    // reinstall/disk-wipe can't grant a fresh trial (server keeps the earliest
    // start per device). Silent no-op when offline.
    if crate::license::load_license(&dir).is_none() {
        crate::license::reconcile_trial(&dir).await;
    }

    crate::license::check_status(&dir)
}

// ── License debug helpers (debug builds only) ─────────────────────────────────

/// Backdate the trial file so the UI shows it as expired (or N days elapsed).
/// Only compiled in debug mode - stripped from release builds entirely.
#[cfg(debug_assertions)]
#[tauri::command]
pub fn debug_set_trial_days_ago(app: tauri::AppHandle, days_ago: u64) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    crate::license::debug_set_trial_days_ago(&dir, days_ago)
}

#[cfg(debug_assertions)]
#[tauri::command]
pub fn debug_reset_trial(app: tauri::AppHandle) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    crate::license::debug_reset_trial(&dir)
}

// ── Sample database ────────────────────────────────────────────────────────────

/// Ensures the bundled sample SQLite database exists in the app data directory.
/// Creates and seeds it on first call; subsequent calls are a no-op.
/// Returns the absolute path to the database file.
#[tauri::command]
pub async fn init_sample_db(app: tauri::AppHandle) -> Result<String, String> {
    use sqlx::sqlite::SqlitePoolOptions;

    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

    let db_path = data_dir.join("sample.db");
    let db_path_str = db_path.to_string_lossy().to_string();

    if db_path.exists() {
        return Ok(db_path_str);
    }

    let url = format!("sqlite:{}", db_path_str);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .map_err(|e| e.to_string())?;

    seed_sample_database(&pool).await?;
    pool.close().await;

    Ok(db_path_str)
}

// ── Autostart ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn enable_autostart(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().enable().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn disable_autostart(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().disable().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_autostart_status(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

async fn seed_sample_database(pool: &sqlx::SqlitePool) -> Result<(), String> {
    let stmts: &[&str] = &[
        // ── Schema ───────────────────────────────────────────────────────────
        "PRAGMA foreign_keys = ON",
        "CREATE TABLE IF NOT EXISTS users (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            name       TEXT    NOT NULL,
            email      TEXT    UNIQUE NOT NULL,
            country    TEXT,
            created_at TEXT    DEFAULT (datetime('now'))
        )",
        "CREATE TABLE IF NOT EXISTS categories (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL,
            description TEXT
        )",
        "CREATE TABLE IF NOT EXISTS products (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            category_id INTEGER REFERENCES categories(id),
            name        TEXT    NOT NULL,
            price       REAL    NOT NULL,
            stock       INTEGER NOT NULL DEFAULT 0,
            description TEXT
        )",
        "CREATE TABLE IF NOT EXISTS orders (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id    INTEGER REFERENCES users(id),
            status     TEXT NOT NULL DEFAULT 'pending',
            total      REAL NOT NULL,
            created_at TEXT DEFAULT (datetime('now'))
        )",
        "CREATE TABLE IF NOT EXISTS order_items (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            order_id   INTEGER REFERENCES orders(id),
            product_id INTEGER REFERENCES products(id),
            quantity   INTEGER NOT NULL,
            unit_price REAL    NOT NULL
        )",
        // ── Seed: categories ─────────────────────────────────────────────────
        "INSERT INTO categories (name, description) VALUES
            ('Electronics',   'Gadgets, devices, and accessories'),
            ('Clothing',       'Apparel for all seasons'),
            ('Books',          'Fiction, non-fiction, and technical titles'),
            ('Home & Garden',  'Everything for your living space'),
            ('Sports',         'Gear and equipment for active lifestyles')",
        // ── Seed: users ───────────────────────────────────────────────────────
        "INSERT INTO users (name, email, country, created_at) VALUES
            ('Alice Martin',    'alice@example.com',   'US', '2024-01-10 08:00:00'),
            ('Bob Chen',        'bob@example.com',     'CN', '2024-01-15 09:30:00'),
            ('Clara Smith',     'clara@example.com',   'GB', '2024-02-01 10:00:00'),
            ('David Nguyen',    'david@example.com',   'VN', '2024-02-14 11:00:00'),
            ('Eva Rossi',       'eva@example.com',     'IT', '2024-03-01 12:00:00'),
            ('Frank Müller',    'frank@example.com',   'DE', '2024-03-20 13:00:00'),
            ('Grace Kim',       'grace@example.com',   'KR', '2024-04-05 14:00:00'),
            ('Hiro Tanaka',     'hiro@example.com',    'JP', '2024-04-18 15:00:00'),
            ('Isabel Ferreira', 'isabel@example.com',  'BR', '2024-05-02 16:00:00'),
            ('James Okafor',    'james@example.com',   'NG', '2024-05-19 17:00:00'),
            ('Karen Lee',       'karen@example.com',   'US', '2024-06-01 08:00:00'),
            ('Luca Bianchi',    'luca@example.com',    'IT', '2024-06-15 09:00:00'),
            ('Maria Garcia',    'maria@example.com',   'ES', '2024-07-01 10:00:00'),
            ('Noah Wilson',     'noah@example.com',    'AU', '2024-07-20 11:00:00'),
            ('Olivia Patel',    'olivia@example.com',  'IN', '2024-08-05 12:00:00')",
        // ── Seed: products ───────────────────────────────────────────────────
        "INSERT INTO products (category_id, name, price, stock, description) VALUES
            (1, 'Wireless Headphones',     89.99,  42, 'Over-ear noise-cancelling headphones'),
            (1, 'USB-C Hub 7-in-1',        34.99, 120, 'Expand your laptop ports'),
            (1, 'Mechanical Keyboard',    109.00,  55, 'Tactile switches, RGB backlight'),
            (1, 'Webcam 1080p',            49.99,  88, 'Crystal-clear video calls'),
            (1, 'Portable SSD 1TB',        79.99,  30, 'Fast NVMe external storage'),
            (2, 'Classic T-Shirt',         19.99, 200, 'Comfortable everyday cotton tee'),
            (2, 'Slim-Fit Jeans',          49.99,  75, 'Modern cut, stretch denim'),
            (2, 'Hoodie - Charcoal',       39.99,  60, 'Warm fleece-lined pullover'),
            (2, 'Running Shorts',          24.99, 110, 'Lightweight moisture-wicking'),
            (3, 'Clean Code',              35.00,  40, 'Robert C. Martin - software craftsmanship'),
            (3, 'The Pragmatic Programmer',33.00,  38, 'Hunt & Thomas - timeless dev advice'),
            (3, 'Designing Data-Intensive', 55.00,  25, 'Martin Kleppmann - distributed systems'),
            (4, 'Succulent Plant Set',     22.00,  90, 'Set of 4 low-maintenance succulents'),
            (4, 'Ceramic Pour-Over Kit',   45.00,  35, 'Elegant coffee brewing set'),
            (4, 'LED Desk Lamp',           38.50,  68, 'Adjustable colour temperature'),
            (5, 'Yoga Mat Pro',            28.00,  50, 'Non-slip 6mm thickness'),
            (5, 'Resistance Bands Set',    18.00,  95, 'Five resistance levels'),
            (5, 'Water Bottle 1L',         15.99, 140, 'BPA-free insulated stainless'),
            (5, 'Jump Rope Speed',         12.00,  80, 'Ball-bearing handles'),
            (1, 'Smart LED Strip 5m',      25.99,  65, 'Wi-Fi, 16M colours, app-controlled')",
        // ── Seed: orders ─────────────────────────────────────────────────────
        "INSERT INTO orders (user_id, status, total, created_at) VALUES
            ( 1, 'delivered',  124.98, '2024-03-05 10:00:00'),
            ( 2, 'delivered',   34.99, '2024-03-12 11:30:00'),
            ( 3, 'shipped',    188.99, '2024-04-01 09:00:00'),
            ( 4, 'delivered',   19.99, '2024-04-10 14:00:00'),
            ( 5, 'processing',  68.00, '2024-05-02 16:00:00'),
            ( 6, 'pending',     55.00, '2024-05-20 08:00:00'),
            ( 7, 'delivered',  149.98, '2024-06-01 12:00:00'),
            ( 8, 'cancelled',   35.00, '2024-06-15 10:00:00'),
            ( 9, 'shipped',     84.99, '2024-07-01 11:00:00'),
            (10, 'delivered',   46.00, '2024-07-18 15:00:00'),
            (11, 'delivered',   63.98, '2024-08-01 09:00:00'),
            (12, 'processing', 109.00, '2024-08-10 14:00:00'),
            (13, 'pending',     22.00, '2024-08-20 10:00:00'),
            (14, 'shipped',    118.49, '2024-08-25 13:00:00'),
            (15, 'delivered',   43.99, '2024-09-01 08:00:00')",
        // ── Seed: order_items ────────────────────────────────────────────────
        "INSERT INTO order_items (order_id, product_id, quantity, unit_price) VALUES
            ( 1,  1, 1,  89.99),
            ( 1,  2, 1,  34.99),
            ( 2,  2, 1,  34.99),
            ( 3,  3, 1, 109.00),
            ( 3,  4, 1,  49.99),
            ( 3,  5, 1,  79.99),
            ( 4,  6, 1,  19.99),
            ( 5, 10, 1,  35.00),
            ( 5, 16, 1,  28.00),
            ( 6, 12, 1,  55.00),
            ( 7,  1, 1,  89.99),
            ( 7,  9, 1,  24.99),
            ( 7, 18, 2,  15.99),
            ( 8, 10, 1,  35.00),
            ( 9,  1, 1,  89.99),
            (10, 13, 1,  22.00),
            (10, 17, 1,  18.00),
            (10, 19, 1,   6.00),
            (11,  6, 1,  19.99),
            (11,  9, 1,  24.99),
            (11, 18, 1,  18.00),
            (12,  3, 1, 109.00),
            (13, 13, 1,  22.00),
            (14,  1, 1,  89.99),
            (14,  4, 1,  49.99),
            (15,  7, 1,  49.99),
            (15, 19, 2,  12.00)",
    ];

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    for stmt in stmts {
        sqlx::query(stmt)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Seed error: {e}"))?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}
