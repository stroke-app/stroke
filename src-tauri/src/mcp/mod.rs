pub mod server;
pub mod tools;
pub mod access;

use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use crate::db::ActiveConnection;
use tauri::{AppHandle, Manager, State};

/// Port we always try first - stable so AI clients don't need reconfiguration.
const PREFERRED_PORT: u16 = 39847;

/// Credential-free metadata about a saved connection - safe to pass to the MCP layer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConnMeta {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub conn_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

pub struct McpState {
    pub conn: Arc<Mutex<Option<ActiveConnection>>>,
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    /// Actual bound port (0 = not yet started).
    pub port: Mutex<u16>,
    /// Persistent token - loaded from disk once in `init_token`, never regenerated.
    token: Mutex<String>,
    /// Credential-free list of all saved connections - synced from frontend.
    pub connections: Arc<Mutex<Vec<ConnMeta>>>,
    /// ID of the currently active connection (matches one entry in `connections`).
    pub active_conn_id: Arc<Mutex<Option<String>>>,
    /// When true, only SELECT / read-only SQL is allowed through execute_sql.
    pub read_only: Arc<Mutex<bool>>,
}

impl McpState {
    pub fn new(conn: Arc<Mutex<Option<ActiveConnection>>>) -> Self {
        Self {
            conn,
            shutdown_tx: Mutex::new(None),
            port: Mutex::new(0),
            token: Mutex::new(String::new()),
            connections: Arc::new(Mutex::new(Vec::new())),
            active_conn_id: Arc::new(Mutex::new(None)),
            read_only: Arc::new(Mutex::new(false)),
        }
    }

    /// Called once from `setup()` - loads or generates a stable token from disk.
    pub fn init_token(&self, app: &AppHandle) {
        let _ = APP.set(app.clone());
        let mut tok = self.token.lock().unwrap_or_else(|e| e.into_inner());
        if !tok.is_empty() {
            return;
        }
        *tok = load_or_persist_token(app);
    }
}

/// Load token from `{app_data_dir}/mcp-token.txt` or generate + save a new one.
fn load_or_persist_token(app: &AppHandle) -> String {
    let path = app
        .path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("mcp-token.txt"));

    if let Some(ref p) = path {
        if let Ok(raw) = std::fs::read_to_string(p) {
            let t = raw.trim().to_string();
            if !t.is_empty() {
                return t;
            }
        }
    }

    let tok = generate_token();
    if let Some(p) = path {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&p, &tok);
    }
    tok
}

#[derive(serde::Serialize, Clone)]
pub struct McpStatus {
    pub running: bool,
    pub port: u16,
    pub url: String,
    pub token: String,
}

#[tauri::command]
pub async fn mcp_start(mcp: State<'_, McpState>) -> Result<McpStatus, String> {
    // Already running - return current status.
    {
        let running = mcp.shutdown_tx.lock().map_err(|e| e.to_string())?.is_some();
        if running {
            let port = *mcp.port.lock().map_err(|e| e.to_string())?;
            let token = mcp.token.lock().map_err(|e| e.to_string())?.clone();
            return Ok(McpStatus {
                running: true,
                port,
                url: format!("http://127.0.0.1:{port}"),
                token,
            });
        }
    }

    // Always try the preferred port first - fall back only if it's taken by another process.
    let preferred = {
        let p = *mcp.port.lock().map_err(|e| e.to_string())?;
        if p > 0 { p } else { PREFERRED_PORT }
    };
    let port = find_free_port(preferred).await?;
    let token = mcp.token.lock().map_err(|e| e.to_string())?.clone();

    let conn = Arc::clone(&mcp.conn);
    let connections = Arc::clone(&mcp.connections);
    let active_conn_id = Arc::clone(&mcp.active_conn_id);
    let read_only = Arc::clone(&mcp.read_only);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let token_clone = token.clone();
    tokio::spawn(async move {
        server::run(port, token_clone, conn, connections, active_conn_id, read_only, None, shutdown_rx).await;
    });

    *mcp.port.lock().map_err(|e| e.to_string())? = port;
    *mcp.shutdown_tx.lock().map_err(|e| e.to_string())? = Some(shutdown_tx);

    Ok(McpStatus {
        running: true,
        port,
        url: format!("http://127.0.0.1:{port}"),
        token,
    })
}

#[tauri::command]
pub async fn mcp_stop(mcp: State<'_, McpState>) -> Result<(), String> {
    if let Some(tx) = mcp.shutdown_tx.lock().map_err(|e| e.to_string())?.take() {
        let _ = tx.send(());
    }
    // Intentionally keep port and token stable - AI clients stay configured.
    Ok(())
}

#[tauri::command(async)]
pub fn mcp_status(mcp: State<'_, McpState>) -> Result<McpStatus, String> {
    let running = mcp.shutdown_tx.lock().map_err(|e| e.to_string())?.is_some();
    let raw_port = *mcp.port.lock().map_err(|e| e.to_string())?;
    // Return PREFERRED_PORT when not yet started so install links are valid immediately.
    let port = if raw_port > 0 { raw_port } else { PREFERRED_PORT };
    let token = mcp.token.lock().map_err(|e| e.to_string())?.clone();
    Ok(McpStatus {
        running,
        port,
        url: format!("http://127.0.0.1:{port}"),
        token,
    })
}

/// Called by the frontend to sync credential-free connection metadata into the MCP layer.
/// Must be called on app start and whenever the connection list or active connection changes.
#[tauri::command(async)]
pub fn mcp_update_connections(
    connections: Vec<ConnMeta>,
    active_id: Option<String>,
    mcp: State<'_, McpState>,
) -> Result<(), String> {
    *mcp.connections.lock().map_err(|e| e.to_string())? = connections;
    *mcp.active_conn_id.lock().map_err(|e| e.to_string())? = active_id;
    Ok(())
}

/// Enable or disable read-only mode for the MCP server.
/// When enabled, execute_sql only allows SELECT / read-only queries.
#[tauri::command(async)]
pub fn mcp_set_readonly(readonly: bool, mcp: State<'_, McpState>) -> Result<(), String> {
    *mcp.read_only.lock().map_err(|e| e.to_string())? = readonly;
    Ok(())
}

pub(crate) async fn find_free_port(preferred: u16) -> Result<u16, String> {
    for port in preferred..preferred + 20 {
        if tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
            .await
            .is_ok()
        {
            return Ok(port);
        }
    }
    Err(format!("No free port found starting at {preferred}"))
}

pub(crate) fn generate_token() -> String {
    // Use OS-provided randomness (getrandom syscall on Linux/macOS, BCryptGenRandom
    // on Windows) for a 128-bit token - far stronger than PID+nanos (~32 bits).
    let mut bytes = [0u8; 16];
    if getrandom::getrandom(&mut bytes).is_ok() {
        return bytes.iter().map(|b| format!("{b:02x}")).collect();
    }
    // getrandom failure is extremely rare; fall back to time+pid as last resort.
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().subsec_nanos();
    let pid = std::process::id();
    format!("{pid:08x}{nanos:08x}")
}


/// The app, for the activity events the server emits. Set once at setup.
static APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

/// One MCP tool call, as the Logs page shows it.
#[derive(serde::Serialize, Clone)]
pub struct McpActivity {
    /// Who called: the client's User-Agent ("claude-code/2.1", "Cursor/…"), or
    /// "Stroke agent" for an AI chat's private server.
    pub client: String,
    pub tool: String,
    /// The SQL, or a short summary of the other arguments.
    pub detail: String,
    pub ok: bool,
    /// Refused by the access gate before it ran.
    pub refused: bool,
    pub error: Option<String>,
    pub ms: u64,
}

/// Tell the frontend about a tool call (Logs page → MCP). Never fails the call.
pub fn emit_activity(a: McpActivity) {
    if let Some(app) = APP.get() {
        use tauri::Emitter;
        let _ = app.emit("mcp-activity", a);
    }
}

/// What `open_in_stroke` asks the window to show.
#[derive(serde::Serialize, Clone)]
pub struct McpOpen {
    pub table: Option<String>,
    pub schema: Option<String>,
    pub query: Option<String>,
}

/// Ask Stroke's window to open a table or a prepared query (nothing is run).
pub fn emit_open(o: McpOpen) {
    if let Some(app) = APP.get() {
        use tauri::Emitter;
        let _ = app.emit("mcp-open", o);
    }
}
