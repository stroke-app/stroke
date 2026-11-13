//! Local agent CLIs (Claude Code, Codex, OpenCode) as AI providers.
//!
//! The CLI drivers are deck's (`crates/agent-*`). What lives here is Stroke's
//! policy for them: every session runs in an empty folder with the CLI's own
//! file and shell tools switched off, and reaches the database only through a
//! private copy of Stroke's MCP server that refuses any statement above the
//! chat's access level (read, read & write, modify schema, dangerous). A
//! refused change comes back as SQL in the reply, for the user to run through
//! the console's review.

use agent_core::{
    Action, AgentAdapter, AgentEvent, Availability, Decision, McpServerSpec, McpTransport,
    PermissionMode, SessionCtx, SessionId, UserTurn,
};
use futures::StreamExt;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tauri::{ipc::Channel, AppHandle, Manager, State};
use tokio::sync::{mpsc, oneshot, Mutex};

/// The MCP server name each CLI sees. Its tools come out as `mcp__stroke__*`
/// in Claude Code and `stroke_*` in OpenCode.
const MCP_NAME: &str = "stroke";
/// Codex reads the bearer token from this variable rather than its argv, so it
/// never shows up in `ps`.
const TOKEN_ENV: &str = "STROKE_AGENT_MCP_TOKEN";
/// Where the private MCP server starts looking for a free port. Clear of the
/// public one on 39847 and the 20 ports it may walk up through.
const PRIVATE_MCP_PORT: u16 = 39880;

#[derive(Default)]
pub struct AgentsState {
    sessions: Mutex<HashMap<String, mpsc::UnboundedSender<Cmd>>>,
    /// One private server per access level (`mcp::access`).
    mcp: Mutex<HashMap<u8, PrivateMcp>>,
}

/// The effort levels both Claude Code (`--effort`) and Codex
/// (`model_reasoning_effort`) accept.
const EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

struct PrivateMcp {
    url: String,
    token: String,
    /// The server stops when this is dropped, so it is held for the app's life.
    _shutdown: oneshot::Sender<()>,
}

enum Cmd {
    Send(String),
    Interrupt,
    Stop,
}

/// One adapter per CLI for the life of the app: the CLI adapters cache the
/// model list they read off the binary.
fn adapters() -> &'static [(&'static str, Arc<dyn AgentAdapter>)] {
    static ALL: OnceLock<Vec<(&'static str, Arc<dyn AgentAdapter>)>> = OnceLock::new();
    ALL.get_or_init(|| {
        vec![
            ("claude", Arc::new(agent_claude::ClaudeAdapter::new()) as Arc<dyn AgentAdapter>),
            ("codex", Arc::new(agent_cli::CliAdapter::new(&agent_cli::spec::CODEX))),
            ("opencode", Arc::new(agent_cli::CliAdapter::new(&agent_cli::spec::OPENCODE))),
        ]
    })
}

fn adapter(id: &str) -> Result<Arc<dyn AgentAdapter>, String> {
    adapters()
        .iter()
        .find(|(k, _)| *k == id)
        .map(|(_, a)| a.clone())
        .ok_or_else(|| format!("Unknown agent: {id}"))
}

/// A desktop launch hands the app a bare PATH that misses the version-manager
/// directories (mise, nvm, npm's global bin) the CLIs are installed in. The
/// adapters spawn by name, so widen the process PATH once to the login shell's.
async fn ensure_user_path() {
    static READY: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
    READY
        .get_or_init(|| async {
            std::env::set_var("PATH", crate::omniroute::user_path().await);
        })
        .await;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentModel {
    id: String,
    label: String,
    note: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSetup {
    verb: String,
    command: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    id: String,
    name: String,
    /// `ready`, `needs_auth` or `missing`.
    status: &'static str,
    version: Option<String>,
    hint: Option<String>,
    setup: Option<AgentSetup>,
    models: Vec<AgentModel>,
}

/// Which agent CLIs are installed, with their versions and models.
#[tauri::command]
pub async fn agent_list() -> Result<Vec<AgentInfo>, String> {
    ensure_user_path().await;
    let probes = adapters().iter().map(|(id, a)| async move {
        let probed = a.probe().await;
        let (status, version, hint) = match probed {
            Ok(Availability::Ready { version }) => ("ready", version, None),
            Ok(Availability::NeedsAuth { hint }) => ("needs_auth", None, Some(hint)),
            Ok(Availability::Missing { hint }) => ("missing", None, Some(hint)),
            Err(e) => ("missing", None, Some(e.to_string())),
        };
        AgentInfo {
            id: id.to_string(),
            name: match *id {
                "claude" => "Claude Code".into(),
                _ => a.display_name().to_string(),
            },
            status,
            version,
            hint,
            setup: a.setup().map(|s| AgentSetup {
                verb: s.verb.into(),
                command: s.command.into(),
            }),
            models: a
                .models()
                .iter()
                .map(|m| AgentModel {
                    id: m.id.into(),
                    label: m.label.into(),
                    note: m.note.into(),
                })
                .collect(),
        }
    });
    Ok(futures::future::join_all(probes).await)
}

/// Start a private MCP server on first use. It shares the live connection
/// with the public one but has its own token and its own read-only setting,
/// so turning the public server off, or on with writes allowed, changes
/// nothing for the agents.
async fn private_mcp(
    state: &AgentsState,
    mcp: &crate::mcp::McpState,
    level: u8,
) -> Result<(String, String), String> {
    let mut servers = state.mcp.lock().await;
    if let Some(m) = servers.get(&level) {
        return Ok((m.url.clone(), m.token.clone()));
    }
    let port = crate::mcp::find_free_port(PRIVATE_MCP_PORT).await?;
    let token = crate::mcp::generate_token();
    let (tx, rx) = oneshot::channel();
    tokio::spawn(crate::mcp::server::run(
        port,
        token.clone(),
        Arc::clone(&mcp.conn),
        Arc::clone(&mcp.connections),
        Arc::clone(&mcp.active_conn_id),
        Arc::new(std::sync::Mutex::new(level == crate::mcp::access::READ)),
        Some(level),
        rx,
    ));
    let url = format!("http://127.0.0.1:{port}/mcp");
    servers.insert(level, PrivateMcp { url: url.clone(), token: token.clone(), _shutdown: tx });
    Ok((url, token))
}

/// The MCP servers in the user's own Codex config. Codex has no flag to load
/// only the ones given on the command line, so each of these is switched off
/// by name. Only the names are read: the listing carries their env values too.
/// Kept for a minute, since every session start would otherwise wait on it.
async fn codex_user_mcp_names() -> Vec<String> {
    static CACHE: std::sync::Mutex<Option<(std::time::Instant, Vec<String>)>> = std::sync::Mutex::new(None);
    if let Ok(c) = CACHE.lock() {
        if let Some((at, names)) = c.as_ref() {
            if at.elapsed() < std::time::Duration::from_secs(60) {
                return names.clone();
            }
        }
    }
    let names = read_codex_user_mcp_names().await;
    if let Ok(mut c) = CACHE.lock() {
        *c = Some((std::time::Instant::now(), names.clone()));
    }
    names
}

async fn read_codex_user_mcp_names() -> Vec<String> {
    let out = tokio::process::Command::new("codex")
        .args(["mcp", "list", "--json"])
        .stdin(std::process::Stdio::null())
        .output()
        .await;
    let Ok(out) = out else { return Vec::new() };
    let Ok(list) = serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|s| s.get("name")?.as_str())
        .filter(|n| *n != MCP_NAME && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .map(String::from)
        .collect()
}

/// The per-CLI part of "database tools only", plus the effort level.
async fn lock_down(agent: &str, ctx: &mut SessionCtx, url: &str, token: &str, effort: Option<&str>, web: bool) {
    match agent {
        "claude" => {
            ctx.allowed_tools = vec![format!("mcp__{MCP_NAME}")];
            if web {
                ctx.allowed_tools.push("WebSearch".into());
                ctx.allowed_tools.push("WebFetch".into());
            }
            ctx.extra_args = vec![
                // No built-in tools: no shell, files or subagents. Web search
                // and fetch only when the user turned on web access.
                "--tools".into(),
                if web { "WebSearch,WebFetch".into() } else { String::new() },
                // Only Stroke's server, not the ones in the user's config.
                "--strict-mcp-config".into(),
                // No user hooks or permission rules either.
                "--setting-sources".into(),
                String::new(),
            ];
            if let Some(e) = effort {
                ctx.extra_args.push("--effort".into());
                ctx.extra_args.push(e.into());
            }
        }
        "codex" => {
            // Codex cannot drop its shell, so it gets a read-only sandbox in an
            // empty folder instead.
            let mut a: Vec<String> = vec!["--sandbox".into(), "read-only".into()];
            for kv in [
                format!("mcp_servers.{MCP_NAME}.url=\"{url}\""),
                format!("mcp_servers.{MCP_NAME}.bearer_token_env_var=\"{TOKEN_ENV}\""),
                // exec mode has nobody to ask, so an unapproved MCP call is
                // cancelled; Stroke's read-only tools are approved up front.
                format!("mcp_servers.{MCP_NAME}.default_tools_approval_mode=\"approve\""),
            ] {
                a.push("-c".into());
                a.push(kv);
            }
            if let Some(e) = effort {
                a.push("-c".into());
                a.push(format!("model_reasoning_effort=\"{e}\""));
            }
            if web {
                a.push("-c".into());
                a.push("tools.web_search=true".into());
            }
            for name in codex_user_mcp_names().await {
                a.push("-c".into());
                a.push(format!("mcp_servers.{name}.enabled=false"));
            }
            ctx.extra_args = a;
            ctx.env.push((TOKEN_ENV.into(), token.into()));
        }
        "opencode" => {
            let config = serde_json::json!({
                "$schema": "https://opencode.ai/config.json",
                "mcp": { MCP_NAME: {
                    "type": "remote",
                    "url": url,
                    "headers": { "Authorization": format!("Bearer {token}") },
                    "enabled": true,
                }},
                // Every tool off, then Stroke's back on.
                "tools": { "*": false, format!("{MCP_NAME}_*"): true, "webfetch": web },
            });
            ctx.env.push(("OPENCODE_CONFIG_CONTENT".into(), config.to_string()));
        }
        _ => {}
    }
}

/// Whether a permission request is for one of Stroke's own tools. Anything
/// else is outside "database tools only" and is refused.
/// A web search or page fetch - allowed only while web access is on.
fn is_web_tool(action: &Action) -> bool {
    match action {
        Action::Other { name, .. } => {
            let n = name.to_ascii_lowercase();
            matches!(n.as_str(), "websearch" | "webfetch" | "web_search" | "web_fetch")
        }
        _ => false,
    }
}

fn is_stroke_tool(action: &Action) -> bool {
    match action {
        Action::McpCall { server, .. } => server == MCP_NAME,
        Action::Other { name, .. } => {
            name.starts_with(&format!("mcp__{MCP_NAME}__")) || name.starts_with(&format!("{MCP_NAME}_"))
        }
        _ => false,
    }
}

/// Start (or restart) the agent session behind one AI chat conversation. Its
/// events stream to `on_event` until it is stopped or replaced.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn agent_start(
    app: AppHandle,
    state: State<'_, AgentsState>,
    mcp: State<'_, crate::mcp::McpState>,
    conversation_id: String,
    agent: String,
    model: Option<String>,
    system_prompt: String,
    resume: Option<String>,
    effort: Option<String>,
    access: Option<String>,
    web_access: Option<bool>,
    on_event: Channel<AgentEvent>,
) -> Result<(), String> {
    let web = web_access.unwrap_or(false);
    ensure_user_path().await;
    let adapter = adapter(&agent)?;
    let effort = effort.filter(|e| EFFORTS.contains(&e.as_str()));
    use crate::mcp::access::{DANGEROUS, MODIFY, READ, WRITE};
    let level = match access.as_deref() {
        Some("write") => WRITE,
        Some("modify") => MODIFY,
        Some("dangerous") => DANGEROUS,
        _ => READ,
    };
    let (url, token) = private_mcp(&state, &mcp, level).await?;

    let base = app.path().app_data_dir().map_err(|e| e.to_string())?.join("agents");
    let cwd = base.join("workspace");
    let session_id = SessionId::new();
    let runtime_dir = base.join("run").join(session_id.to_string());
    for d in [&cwd, &runtime_dir] {
        std::fs::create_dir_all(d).map_err(|e| format!("Could not create {}: {e}", d.display()))?;
    }

    let mut ctx = SessionCtx {
        session_id,
        cwd,
        system_prompt: Arc::from(system_prompt),
        model: model.filter(|m| !m.is_empty()).map(Into::into),
        mcp: Arc::new(vec![McpServerSpec {
            name: MCP_NAME.into(),
            transport: McpTransport::Http {
                url: url.clone(),
                headers: vec![("Authorization".into(), format!("Bearer {token}"))],
            },
            // OpenCode and Codex get theirs in lock_down; only Claude Code reads this.
            enabled: agent == "claude",
        }]),
        skills_dir: None,
        runtime_dir: runtime_dir.clone(),
        resume: resume.filter(|r| !r.is_empty()),
        permission_mode: PermissionMode::Ask,
        env: Vec::new(),
        allowed_tools: Vec::new(),
        extra_args: Vec::new(),
    };
    lock_down(&agent, &mut ctx, &url, &token, effort.as_deref(), web).await;

    let mut session = adapter
        .spawn(ctx)
        .await
        .map_err(|e| format!("Could not start {}: {e}", adapter.display_name()))?;

    let (tx, mut rx) = mpsc::unbounded_channel();
    if let Some(prev) = state.sessions.lock().await.insert(conversation_id, tx) {
        let _ = prev.send(Cmd::Stop);
    }

    tokio::spawn(async move {
        enum Next {
            Event(Option<AgentEvent>),
            Cmd(Option<Cmd>),
        }
        loop {
            let next = {
                let mut events = session.events();
                tokio::select! {
                    e = events.next() => Next::Event(e),
                    c = rx.recv() => Next::Cmd(c),
                }
            };
            match next {
                Next::Event(Some(AgentEvent::Permission { id, action, .. })) => {
                    let decision = if is_stroke_tool(&action) || (web && is_web_tool(&action)) { Decision::Allow } else { Decision::Deny };
                    let _ = session.decide(id, decision).await;
                }
                Next::Event(Some(ev)) => {
                    let fatal = matches!(ev, AgentEvent::Fatal { .. });
                    if on_event.send(ev).is_err() || fatal {
                        break;
                    }
                }
                Next::Cmd(Some(Cmd::Send(text))) => {
                    if let Err(e) = session.send(UserTurn::text(text)).await {
                        let _ = on_event.send(AgentEvent::Fatal { message: e.to_string(), retryable: true });
                        break;
                    }
                }
                Next::Cmd(Some(Cmd::Interrupt)) => {
                    let _ = session.interrupt().await;
                }
                Next::Event(None) | Next::Cmd(Some(Cmd::Stop)) | Next::Cmd(None) => break,
            }
        }
        drop(session);
        // Holds the generated MCP config, token included.
        let _ = tokio::fs::remove_dir_all(&runtime_dir).await;
    });
    Ok(())
}

async fn command(state: &AgentsState, conversation_id: &str, cmd: Cmd) -> Result<(), String> {
    let mut sessions = state.sessions.lock().await;
    let sent = sessions.get(conversation_id).map(|tx| tx.send(cmd).is_ok());
    match sent {
        Some(true) => Ok(()),
        Some(false) => {
            sessions.remove(conversation_id);
            Err("NO_SESSION".into())
        }
        None => Err("NO_SESSION".into()),
    }
}

/// Send a user message. `NO_SESSION` means the agent has exited and the
/// caller should start it again (with `resume`) and resend.
#[tauri::command]
pub async fn agent_send(state: State<'_, AgentsState>, conversation_id: String, text: String) -> Result<(), String> {
    command(&state, &conversation_id, Cmd::Send(text)).await
}

#[tauri::command]
pub async fn agent_interrupt(state: State<'_, AgentsState>, conversation_id: String) -> Result<(), String> {
    command(&state, &conversation_id, Cmd::Interrupt).await
}

#[tauri::command]
pub async fn agent_stop(state: State<'_, AgentsState>, conversation_id: String) -> Result<(), String> {
    if let Some(tx) = state.sessions.lock().await.remove(&conversation_id) {
        let _ = tx.send(Cmd::Stop);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_stroke_tools_pass() {
        let mcp = |server: &str| Action::McpCall { server: server.into(), tool: "x".into(), input: serde_json::json!({}) };
        let other = |name: &str| Action::Other { name: name.into(), input: serde_json::json!({}) };
        assert!(is_stroke_tool(&mcp("stroke")));
        assert!(!is_stroke_tool(&mcp("github")));
        assert!(is_stroke_tool(&other("mcp__stroke__execute_sql")));
        assert!(is_stroke_tool(&other("stroke_list_tables")));
        assert!(!is_stroke_tool(&other("mcp__strokes__x")));
        assert!(!is_stroke_tool(&other("bash")));
        // Web tools are their own class: allowed only when web access is on.
        assert!(is_web_tool(&other("WebSearch")));
        assert!(is_web_tool(&other("WebFetch")));
        assert!(is_web_tool(&other("web_search")));
        assert!(!is_web_tool(&other("bash")));
        assert!(!is_web_tool(&mcp("stroke")));
        assert!(!is_stroke_tool(&Action::Bash { command: "ls".into(), cwd: None }));
    }
}
