use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use crate::db::ActiveConnection;
use super::{access, tools, ConnMeta};

#[derive(Clone)]
struct AppState {
    conn: Arc<Mutex<Option<ActiveConnection>>>,
    connections: Arc<Mutex<Vec<ConnMeta>>>,
    active_conn_id: Arc<Mutex<Option<String>>>,
    token: String,
    read_only: Arc<Mutex<bool>>,
    /// An agent session's access level (see `access`); None for the public
    /// server, which only knows read-only on or off.
    max_level: Option<u8>,
}

#[derive(Deserialize)]
struct RpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    method: String,
    params: Option<Value>,
    id: Option<Value>,
}

#[derive(Serialize)]
struct RpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
}

#[derive(Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

impl RpcResponse {
    fn ok(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: Some(result),
            error: None,
            id,
        }
    }
    fn err(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
            id,
        }
    }
}

pub async fn run(
    port: u16,
    token: String,
    conn: Arc<Mutex<Option<ActiveConnection>>>,
    connections: Arc<Mutex<Vec<ConnMeta>>>,
    active_conn_id: Arc<Mutex<Option<String>>>,
    read_only: Arc<Mutex<bool>>,
    max_level: Option<u8>,
    shutdown_rx: oneshot::Receiver<()>,
) {
    let state = AppState { conn, connections, active_conn_id, token, read_only, max_level };
    // Local origins only. A web page in a browser has no business reading this
    // server's answers, token or not (MCP spec: validate Origin on local servers).
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _| {
            origin.to_str().is_ok_and(is_local_origin)
        }))
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/", post(handle_rpc))
        .route("/mcp", post(handle_rpc))
        .route("/health", get(health))
        .layer(cors)
        .with_state(state);

    let listener = match tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[MCP] bind failed: {e}");
            return;
        }
    };

    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            shutdown_rx.await.ok();
        })
        .await
    {
        eprintln!("[MCP] server error: {e}");
    }
}

async fn health() -> &'static str {
    "ok"
}

/// An Origin that belongs to this machine (or to Stroke's own webview).
fn is_local_origin(origin: &str) -> bool {
    let o = origin.trim().to_ascii_lowercase();
    if o == "null" || o.starts_with("tauri://") || o.starts_with("http://tauri.localhost") || o.starts_with("https://tauri.localhost") {
        return true;
    }
    let host = o.split("://").nth(1).unwrap_or("");
    let host = host.split('/').next().unwrap_or("");
    let host = if host.starts_with('[') { host.split(']').next().map(|h| h.trim_start_matches('[')).unwrap_or("") } else { host.split(':').next().unwrap_or("") };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

async fn handle_rpc(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RpcRequest>,
) -> Response {
    // DNS rebinding: a browser page whose host name resolves to 127.0.0.1 still
    // sends its own Origin. MCP clients (CLIs, desktop apps) send none, or a
    // local one. Anything else is refused before the token is even looked at.
    if let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok()) {
        if !is_local_origin(origin) {
            return (StatusCode::FORBIDDEN, "Forbidden origin").into_response();
        }
    }

    // Optional bearer token check
    if !state.token.is_empty() {
        let auth = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let expected = format!("Bearer {}", state.token);
        if auth != expected {
            return Json(RpcResponse::err(req.id, -32001, "Unauthorized")).into_response();
        }
    }

    // Notifications have no id and need no response; return 202
    if req.id.is_none() && req.method.starts_with("notifications/") {
        return StatusCode::ACCEPTED.into_response();
    }

    let id = req.id.clone();
    let started = std::time::Instant::now();
    let call = (req.method == "tools/call").then(|| {
        let p = req.params.clone().unwrap_or(json!({}));
        let tool = p["name"].as_str().unwrap_or("?").to_string();
        let args = &p["arguments"];
        let detail = args["sql"].as_str().map(str::to_string).unwrap_or_else(|| {
            let s = args.to_string();
            if s == "null" || s == "{}" { String::new() } else { s.chars().take(300).collect() }
        });
        (tool, detail)
    });
    let client = if state.max_level.is_some() {
        "Stroke agent".to_string()
    } else {
        headers.get("user-agent").and_then(|v| v.to_str().ok()).unwrap_or("MCP client").chars().take(80).collect()
    };
    let result = dispatch(&state, &req.method, req.params).await;
    if let Some((tool, detail)) = call {
        let err = result.as_ref().err().cloned();
        super::emit_activity(super::McpActivity {
            client,
            tool,
            detail,
            ok: err.is_none(),
            refused: err.as_deref().is_some_and(|e| e.starts_with("Refused") || e.starts_with("Read-only mode")),
            error: err,
            ms: started.elapsed().as_millis() as u64,
        });
    }
    match result {
        Ok(val) => Json(RpcResponse::ok(id, val)).into_response(),
        Err(msg) => Json(RpcResponse::err(id, -32000, msg)).into_response(),
    }
}

async fn dispatch(
    state: &AppState,
    method: &str,
    params: Option<Value>,
) -> Result<Value, String> {
    match method {
        "initialize" => {
            // Answer in the client's protocol version when we speak it, else
            // our newest: newer clients (2025-*) then use their own framing.
            const SUPPORTED: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
            let asked = params.as_ref().and_then(|p| p["protocolVersion"].as_str()).unwrap_or("");
            let version = if SUPPORTED.contains(&asked) { asked } else { SUPPORTED[0] };
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": {}, "resources": {}, "prompts": {} },
                "serverInfo": { "name": "Stroke", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Stroke is the user's database client. Tools work on the database connected in Stroke right now. Look before you write: list_tables, describe_table and sample_rows show the real schema and values. Use open_in_stroke to show the user a table or hand them a query to review and run."
            }))
        }

        // Each table of the active schema as a resource; reading one returns
        // its description (columns, types, keys).
        "resources/list" => {
            let conn = state.conn.lock().map_err(|e| e.to_string())?.clone();
            let Some(conn) = conn else { return Ok(json!({ "resources": [] })) };
            let schema = default_schema(&conn);
            let listed = tools::call_tool(&conn, "list_tables", &json!({ "schema": schema }), true).await.unwrap_or_default();
            let parsed: Value = serde_json::from_str(&listed).unwrap_or(json!({}));
            let resources: Vec<Value> = parsed["tables"]
                .as_array()
                .map(|ts| {
                    ts.iter()
                        .filter_map(|t| t["name"].as_str())
                        .map(|name| json!({
                            "uri": format!("stroke://table/{schema}/{name}"),
                            "name": name,
                            "description": format!("Columns, types and keys of {name}"),
                            "mimeType": "application/json",
                        }))
                        .collect()
                })
                .unwrap_or_default();
            Ok(json!({ "resources": resources }))
        }

        "resources/templates/list" => Ok(json!({ "resourceTemplates": [{
            "uriTemplate": "stroke://table/{schema}/{table}",
            "name": "Table",
            "description": "Columns, types and keys of a table",
            "mimeType": "application/json",
        }] })),

        "resources/read" => {
            let p = params.ok_or("Missing params")?;
            let uri = p["uri"].as_str().ok_or("Missing uri")?;
            let rest = uri.strip_prefix("stroke://table/").ok_or("Unknown resource")?;
            let (schema, table) = rest.split_once('/').ok_or("Expected stroke://table/<schema>/<table>")?;
            let conn = state.conn.lock().map_err(|e| e.to_string())?.clone().ok_or("Not connected to a database.")?;
            let text = tools::call_tool(&conn, "describe_table", &json!({ "schema": schema, "table": table }), true).await?;
            Ok(json!({ "contents": [{ "uri": uri, "mimeType": "application/json", "text": text }] }))
        }

        "prompts/list" => Ok(json!({ "prompts": PROMPTS.iter().map(|(name, desc, arg, _)| json!({
            "name": name,
            "description": desc,
            "arguments": [{ "name": arg, "description": arg.replace('_', " "), "required": true }],
        })).collect::<Vec<_>>() })),

        "prompts/get" => {
            let p = params.ok_or("Missing params")?;
            let name = p["name"].as_str().ok_or("Missing prompt name")?;
            let (_, desc, arg, template) = PROMPTS.iter().find(|(n, ..)| *n == name).ok_or("Unknown prompt")?;
            let value = p["arguments"][*arg].as_str().unwrap_or("");
            Ok(json!({
                "description": desc,
                "messages": [{ "role": "user", "content": { "type": "text", "text": template.replace("{}", value) } }],
            }))
        }

        "ping" => Ok(json!({})),

        "tools/list" => Ok(json!({ "tools": tools::tool_list() })),

        "tools/call" => {
            let p = params.ok_or("Missing params")?;
            let name = p["name"].as_str().ok_or("Missing tool name")?;
            let args = p.get("arguments").cloned().unwrap_or(json!({}));

            let conn = state.conn.lock().map_err(|e| e.to_string())?.clone();
            let read_only = *state.read_only.lock().map_err(|e| e.to_string())?;
            let session = Session {
                max: if read_only { access::READ } else { state.max_level.unwrap_or(access::DANGEROUS) },
                agent: state.max_level.is_some(),
                dialect: conn
                    .as_ref()
                    .map(|c| access::Dialect::for_driver(c.driver()))
                    .unwrap_or(access::Dialect::Sql),
            };
            // Every tool call, whatever its name, passes this one check.
            gate(name, &args, session)?;

            // These tools don't require an active connection.
            if name == "list_databases" || name == "current_database" {
                let connections = state.connections.lock().map_err(|e| e.to_string())?.clone();
                let active_id = state.active_conn_id.lock().map_err(|e| e.to_string())?.clone();
                let text = tools::call_meta_tool(name, &connections, active_id.as_deref())?;
                return Ok(json!({ "content": [{ "type": "text", "text": text }] }));
            }

            let conn = conn.ok_or("Not connected to a database. Call list_databases to see available connections and ask the user to select one in Stroke.")?;
            let text = tools::call_tool(&conn, name, &args, session.max == access::READ).await?;
            Ok(json!({ "content": [{ "type": "text", "text": text }] }))
        }

        _ => Err(format!("Unknown method: {method}")),
    }
}

/// Ready-made workflows clients can offer (name, description, argument, text).
const PROMPTS: &[(&str, &str, &str, &str)] = &[
    (
        "explain_slow_query",
        "Find out why a query is slow and how to fix it",
        "query",
        "This query is slow:\n\n{}\n\nUse explain_query to read its plan and describe_table on the tables it touches. Say what makes it slow, then propose the fix (an index, a rewrite) with the exact SQL. Do not create anything; hand the SQL to me with open_in_stroke.",
    ),
    (
        "write_migration",
        "Write a migration for a schema change",
        "change",
        "I want this schema change: {}\n\nRead the affected tables with describe_table and sample_rows first. Write the migration as SQL for this engine (up and down), keeping existing data safe, and open it in Stroke with open_in_stroke for me to review. Do not run it.",
    ),
    (
        "summarize_table",
        "Describe what a table holds and how it is used",
        "table",
        "Describe the table {}: use describe_table and sample_rows, then explain in plain words what each column holds, how it relates to other tables, and anything odd in the data (nulls, formats, outliers).",
    ),
];

/// The schema tools default to for this engine.
fn default_schema(conn: &ActiveConnection) -> &'static str {
    match conn {
        ActiveConnection::Postgres(_) => "public",
        ActiveConnection::Mssql(_) => "dbo",
        ActiveConnection::Sqlite(_) | ActiveConnection::D1(_) | ActiveConnection::LibSql(_) | ActiveConnection::Duckdb(_) => "main",
        _ => "",
    }
}

/// What the caller of a tool may do.
#[derive(Clone, Copy)]
struct Session {
    /// Highest access level allowed (`access::READ` in read-only mode).
    max: u8,
    /// An agent chat's private server, as opposed to the user's own MCP server.
    agent: bool,
    dialect: access::Dialect,
}

/// The only tool that may run a script of several statements.
const MULTI_STATEMENT_TOOLS: &[&str] = &["execute_sql"];

/// The one access check for every tool call. It looks at the arguments, not
/// the tool name, so a new tool that takes `sql` or `path` is covered:
/// - `sql` is classified for the connected engine and refused above the level;
/// - tools other than execute_sql take exactly one statement (explain_query
///   pastes its SQL after EXPLAIN, export_query must be one read);
/// - `path` (a file write on this machine) is refused for agents and in
///   read-only mode, since text in a row can steer the agent into writing a
///   startup script.
fn gate(name: &str, args: &Value, s: Session) -> Result<(), String> {
    if args.get("path").is_some_and(|p| !p.is_null()) && (s.agent || s.max == access::READ) {
        return Err(format!(
            "Refused: {name} can't write files from {}. Leave out `path` to get the content inline.",
            if s.agent { "an agent chat" } else { "Stroke's MCP server in read-only mode" },
        ));
    }
    let Some(sql) = args.get("sql") else { return Ok(()) };
    let sql = sql.as_str().ok_or("The sql argument must be a string")?;
    let count = access::statement_count(s.dialect, sql);
    if count > 1 && !MULTI_STATEMENT_TOOLS.contains(&name) {
        return Err(format!(
            "Refused: {name} takes one statement and this has {count}. Send them one at a time."
        ));
    }
    let needs = access::script_level_in(s.dialect, sql);
    if needs <= s.max {
        return Ok(());
    }
    if s.agent {
        Err(format!(
            "Refused: this needs \"{}\" access and the chat is set to \"{}\". \
             Do not retry it. Give the user the SQL in a ```sql block to review and run, \
             or ask them to raise the access level in Stroke's composer.",
            access::level_name(needs),
            access::level_name(s.max),
        ))
    } else {
        Err("Read-only mode is enabled. Only read queries are permitted. \
             Disable read-only mode in Stroke's MCP settings to run write operations."
            .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use access::{Dialect, DANGEROUS, MODIFY, READ, WRITE};

    fn state(read_only: bool, max_level: Option<u8>) -> AppState {
        AppState {
            conn: Arc::new(Mutex::new(None)),
            connections: Arc::new(Mutex::new(Vec::new())),
            active_conn_id: Arc::new(Mutex::new(None)),
            token: String::new(),
            read_only: Arc::new(Mutex::new(read_only)),
            max_level,
        }
    }

    async fn call(st: &AppState, name: &str, args: Value) -> Result<Value, String> {
        dispatch(st, "tools/call", Some(json!({ "name": name, "arguments": args }))).await
    }

    /// Passed the gate: with no connection, the call only fails for that.
    fn passed(r: Result<Value, String>) -> bool {
        matches!(r, Err(e) if e.starts_with("Not connected"))
    }

    fn refused(r: Result<Value, String>) -> bool {
        matches!(r, Err(e) if e.starts_with("Refused") || e.starts_with("Read-only mode"))
    }

    fn agent(max: u8) -> Session {
        Session { max, agent: true, dialect: Dialect::Sql }
    }

    #[tokio::test]
    async fn agent_read_only_covers_every_sql_tool() {
        let st = state(true, Some(READ));
        assert!(passed(call(&st, "execute_sql", json!({ "sql": "SELECT 1" })).await));
        assert!(refused(call(&st, "execute_sql", json!({ "sql": "DELETE FROM t WHERE id = 1" })).await));
        // export_query and explain_query used to skip the level check.
        assert!(refused(call(&st, "export_query", json!({ "sql": "SELECT 1; DROP TABLE x" })).await));
        assert!(refused(call(&st, "export_query", json!({ "sql": "SELECT pg_terminate_backend(1)" })).await));
        assert!(refused(call(&st, "explain_query", json!({ "sql": "SET SHOWPLAN_ALL OFF; DROP TABLE x" })).await));
        assert!(refused(call(&st, "explain_query", json!({ "sql": "DROP TABLE x" })).await));
        assert!(passed(call(&st, "explain_query", json!({ "sql": "SELECT * FROM t" })).await));
        // A tool the gate has never heard of is still checked by its arguments.
        assert!(refused(call(&st, "some_future_tool", json!({ "sql": "DROP TABLE x" })).await));
    }

    #[tokio::test]
    async fn agents_cannot_write_files() {
        for level in [READ, WRITE, MODIFY, DANGEROUS] {
            let st = state(level == READ, Some(level));
            let r = call(&st, "export_query", json!({ "sql": "SELECT 'x'", "path": "/Users/me/.bashrc" })).await;
            assert!(refused(r), "level {level}");
            assert!(passed(call(&st, "export_query", json!({ "sql": "SELECT 'x'" })).await));
        }
    }

    #[tokio::test]
    async fn public_read_only_mode() {
        let st = state(true, None);
        assert!(refused(call(&st, "export_query", json!({ "sql": "SELECT 1", "path": "/tmp/x.csv" })).await));
        assert!(refused(call(&st, "export_query", json!({ "sql": "SELECT 1; DROP TABLE x" })).await));
        assert!(refused(call(&st, "execute_sql", json!({ "sql": "SELECT * INTO t2 FROM t" })).await));
        assert!(refused(call(&st, "explain_query", json!({ "sql": "DELETE FROM t" })).await));
        assert!(passed(call(&st, "execute_sql", json!({ "sql": "SELECT 1" })).await));

        // With writes allowed, the user's own server may still export to a file.
        let st = state(false, None);
        assert!(passed(call(&st, "export_query", json!({ "sql": "SELECT 1", "path": "/tmp/x.csv" })).await));
        assert!(passed(call(&st, "execute_sql", json!({ "sql": "SELECT 1; DROP TABLE x" })).await));
        // ...but explain/export never take a second statement.
        assert!(refused(call(&st, "explain_query", json!({ "sql": "SELECT 1; DROP TABLE x" })).await));
    }

    #[test]
    fn only_local_origins_pass() {
        assert!(is_local_origin("http://localhost:5173"));
        assert!(is_local_origin("http://127.0.0.1:3000"));
        assert!(is_local_origin("http://[::1]:8080"));
        assert!(is_local_origin("tauri://localhost"));
        assert!(!is_local_origin("https://evil.example"));
        assert!(!is_local_origin("http://localhost.evil.example"));
        assert!(!is_local_origin("http://127.0.0.1.nip.io"));
    }

    #[test]
    fn gate_levels() {
        let sql = |s: &str| json!({ "sql": s });
        assert!(gate("execute_sql", &sql("INSERT INTO t VALUES (1)"), agent(WRITE)).is_ok());
        assert!(gate("execute_sql", &sql("INSERT INTO t VALUES (1)"), agent(READ)).is_err());
        assert!(gate("execute_sql", &sql("SELECT 1; INSERT INTO t VALUES (1)"), agent(WRITE)).is_ok());
        assert!(gate("execute_sql", &sql("DROP TABLE t"), agent(MODIFY)).is_err());
        assert!(gate("execute_sql", &json!({ "sql": 42 }), agent(DANGEROUS)).is_err());
        assert!(gate("list_tables", &json!({ "schema": "public" }), agent(READ)).is_ok());
    }

    #[test]
    fn gate_nosql_engines() {
        let s = |d, max| Session { max, agent: true, dialect: d };
        let sql = |q: &str| json!({ "sql": q });
        assert!(gate("execute_sql", &sql("db.users.find({})"), s(Dialect::Mongo, READ)).is_ok());
        assert!(gate("execute_sql", &sql("db.users.insertOne({a: 1})"), s(Dialect::Mongo, READ)).is_err());
        assert!(gate("execute_sql", &sql("db.users.deleteMany({})"), s(Dialect::Mongo, WRITE)).is_err());
        assert!(gate("execute_sql", &sql("GET k"), s(Dialect::Redis, READ)).is_ok());
        assert!(gate("execute_sql", &sql("SET k v"), s(Dialect::Redis, READ)).is_err());
        assert!(gate("execute_sql", &sql("FLUSHALL"), s(Dialect::Redis, WRITE)).is_err());
        assert!(gate("execute_sql", &sql("INSERT INTO t VALUE {'a': 1}"), s(Dialect::Sql, READ)).is_err());
        assert!(gate("execute_sql", &sql("DELETE FROM ks.t WHERE id = 1"), s(Dialect::Sql, READ)).is_err());
    }

    /// Every tool argument is either SQL (gated), a file path (gated) or one
    /// of these plain values. A new tool that takes SQL under another name
    /// fails here instead of slipping past the gate.
    #[test]
    fn every_tool_argument_is_known() {
        // `query` (open_in_stroke) is SQL text but is never run: it only lands
        // in the editor for the user to review, so the gate does not apply.
        const PLAIN: &[&str] = &["schema", "table", "format", "max_rows", "limit", "query"];
        for tool in tools::tool_list().as_array().unwrap() {
            let name = tool["name"].as_str().unwrap();
            let props = tool["inputSchema"]["properties"].as_object().unwrap();
            for arg in props.keys() {
                assert!(
                    arg == "sql" || arg == "path" || PLAIN.contains(&arg.as_str()),
                    "{name} takes `{arg}`: gate it in server::gate or add it to PLAIN if it is not SQL"
                );
            }
        }
    }
}
