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
use tower_http::cors::{Any, CorsLayer};
use crate::db::ActiveConnection;
use super::{tools, ConnMeta};

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
    let cors = CorsLayer::new()
        .allow_origin(Any)
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

async fn handle_rpc(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RpcRequest>,
) -> Response {
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
    let result = dispatch(&state, &req.method, req.params).await;
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
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "Stroke", "version": env!("CARGO_PKG_VERSION") }
        })),

        "ping" => Ok(json!({})),

        "tools/list" => Ok(json!({ "tools": tools::tool_list() })),

        "tools/call" => {
            let p = params.ok_or("Missing params")?;
            let name = p["name"].as_str().ok_or("Missing tool name")?;
            let args = p.get("arguments").cloned().unwrap_or(json!({}));

            // These tools don't require an active connection.
            if name == "list_databases" || name == "current_database" {
                let connections = state.connections.lock().map_err(|e| e.to_string())?.clone();
                let active_id = state.active_conn_id.lock().map_err(|e| e.to_string())?.clone();
                let text = tools::call_meta_tool(name, &connections, active_id.as_deref())?;
                return Ok(json!({ "content": [{ "type": "text", "text": text }] }));
            }

            // Enforce read-only mode: only SELECT-family queries are allowed.
            let is_readonly = *state.read_only.lock().map_err(|e| e.to_string())?;
            if is_readonly && name == "execute_sql" {
                let sql = args["sql"].as_str().unwrap_or("");
                if !tools::is_read_only_sql(sql) {
                    return Err(
                        "Read-only mode is enabled. Only SELECT queries are permitted. \
                         Disable read-only mode in Stroke's MCP settings to run write operations."
                            .to_string(),
                    );
                }
            }

            if let (Some(max), "execute_sql") = (state.max_level, name) {
                let needs = super::access::script_level(args["sql"].as_str().unwrap_or(""));
                if needs > max {
                    return Err(format!(
                        "Refused: this needs \"{}\" access and the chat is set to \"{}\". \
                         Do not retry it. Give the user the SQL in a ```sql block to review and run, \
                         or ask them to raise the access level in Stroke's composer.",
                        super::access::level_name(needs),
                        super::access::level_name(max),
                    ));
                }
            }

            let conn = state
                .conn
                .lock()
                .map_err(|e| e.to_string())?
                .clone()
                .ok_or("Not connected to a database. Call list_databases to see available connections and ask the user to select one in Stroke.")?;
            let text = tools::call_tool(&conn, name, &args).await?;
            Ok(json!({ "content": [{ "type": "text", "text": text }] }))
        }

        _ => Err(format!("Unknown method: {method}")),
    }
}
