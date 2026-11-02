//! OpenCode, driven through `opencode serve` rather than `opencode run`.
//!
//! `opencode run --format json` is a poor fit for a chat: every turn boots the
//! whole app again (about two seconds before the model is even asked), and it
//! prints a text part only once that part is complete, so a long answer
//! appears all at once at the very end. The server is what opencode's own UI
//! talks to: it streams every token (`message.part.delta`), live tool output,
//! approval requests, and has a real stop (`/session/{id}/abort`).
//!
//! Each chat gets its own server, started as soon as the chat's session is,
//! with the chat's environment — the `deck` CLI inside its tools must act for
//! this chat and no other — and stopped when the chat closes or has been idle
//! for a while. If the server cannot be started at all, turns fall back to one
//! `opencode run` each.

use crate::decode::{
    self, opencode_failure, opencode_usage, suffix, tool_title, Exit, Joiner, Turn,
};
use crate::http::{self, query_escape, Endpoint, EventStream};
use crate::spec::{self, CliSpec};
use crate::{compose, kill_tree, primer_for, spawn_turn, TurnHandle, EVENT_BUFFER};
use agent_core::{
    Action, AgentError, AgentEvent, AgentSession, Decision, HydrateMode, PermissionMode, RequestId,
    SessionCtx, StopReason, ToolId, ToolOutput, Transcript, UserTurn,
};
use async_trait::async_trait;
use futures::stream::BoxStream;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// How long `opencode serve` gets to print the address it listens on.
const START_TIMEOUT: Duration = Duration::from_secs(30);
/// For one request to the local server.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// A chat's server is stopped after this long without a turn; the next turn
/// starts it again and carries on in the same opencode session.
const IDLE_SHUTDOWN: Duration = Duration::from_secs(10 * 60);

/// Runs `opencode serve` (the arguments after `$0`) and ties its life to
/// deck's: deck holds this shell's stdin open, and when that pipe closes —
/// deck quit, crashed or was killed — the whole process group goes. Without
/// it, a deck that dies without cleaning up leaves a server running for good.
/// It also exits when the server does, so a dead server is noticed.
const LIFELINE: &str = r#"exec 3<&0
"$@" </dev/null 3<&- &
pid=$!
( cat <&3 >/dev/null; kill -TERM 0 ) &
exec 3<&-
wait "$pid"
kill -TERM 0"#;

/// One running `opencode serve`.
struct Server {
    child: Child,
    ep: Endpoint,
    drains: Vec<JoinHandle<()>>,
    /// Dropping this (or deck dying) stops the server.
    _lifeline: Option<tokio::process::ChildStdin>,
}

impl Server {
    async fn start(bin: &str, ctx: &SessionCtx) -> Result<Server, String> {
        // The server runs tools; nothing else on this machine should be able
        // to drive it.
        let password = token();
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", LIFELINE, "opencode-serve", bin])
            .args(["serve", "--hostname", "127.0.0.1", "--port", "0"])
            .current_dir(&ctx.cwd)
            .envs(ctx.env.iter().map(|(k, v)| (k, v)))
            .env("OPENCODE_SERVER_PASSWORD", &password)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Its own group: the lifeline's `kill 0`, and our stop, reach the
        // language and MCP servers it starts, and nothing of deck's.
        #[cfg(unix)]
        cmd.process_group(0);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("could not start `{bin} serve`: {e}"))?;
        let lifeline = child.stdin.take();

        let stderr = child.stderr.take().expect("piped");
        let tail: Arc<Mutex<Vec<String>>> = Arc::default();
        let tail_w = tail.clone();
        let err_drain = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!(target: "agent.opencode.stderr", "{line}");
                let mut t = tail_w.lock();
                t.push(line);
                if t.len() > 20 {
                    t.remove(0);
                }
            }
        });

        // "opencode server listening on http://127.0.0.1:4096"
        let mut lines = BufReader::new(child.stdout.take().expect("piped")).lines();
        let url = tokio::time::timeout(START_TIMEOUT, async {
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!(target: "agent.opencode.stdout", "{line}");
                if let Some(i) = line.find("http://") {
                    return Some(line[i..].trim().to_string());
                }
            }
            None
        })
        .await;
        let url = match url {
            Ok(Some(url)) => url,
            Ok(None) => {
                let _ = tokio::time::timeout(Duration::from_secs(1), child.wait()).await;
                kill_tree(&mut child);
                let said = decode::from_stderr(&tail.lock()).unwrap_or_default();
                err_drain.abort();
                return Err(format!("`{bin} serve` exited before listening. {said}"));
            }
            Err(_) => {
                kill_tree(&mut child);
                err_drain.abort();
                return Err(format!(
                    "`{bin} serve` did not start within {} s",
                    START_TIMEOUT.as_secs()
                ));
            }
        };
        let out_drain = tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!(target: "agent.opencode.stdout", "{line}");
            }
        });
        let Some(ep) = Endpoint::parse(&url, Some(http::basic_auth("opencode", &password))) else {
            kill_tree(&mut child);
            return Err(format!("unexpected server address {url:?}"));
        };
        Ok(Server {
            child,
            ep,
            drains: vec![err_drain, out_drain],
            _lifeline: lifeline,
        })
    }

    fn running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Its group also holds the language servers and MCP servers it ran.
        kill_tree(&mut self.child);
        for d in &self.drains {
            d.abort();
        }
    }
}

/// 128 random bits as hex, for the server's password.
fn token() -> String {
    use std::io::Read;
    let mut b = [0u8; 16];
    let random = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b));
    if random.is_err() {
        use std::hash::{BuildHasher, Hasher};
        for chunk in b.chunks_mut(8) {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u128(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
            );
            chunk.copy_from_slice(&h.finish().to_le_bytes()[..chunk.len()]);
        }
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// What a part of an assistant message holds, so a bare delta can be routed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Text,
    Reasoning,
    Other,
}

/// An approval opencode asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    /// Answer "once" without asking: the chat runs with approvals off.
    Auto(String),
    /// Shown to the user as this request; opencode's id for it.
    User(RequestId, String),
}

/// Turns the server's event stream into agent events for one session.
///
/// Built against events captured from `opencode serve` 1.18.10:
/// `message.part.updated` carries a part's whole state (a tool's status and
/// live `metadata.output`, a text part as it opens and closes),
/// `message.part.delta` the text in between, `session.idle` the end of the
/// turn, `permission.asked` an approval.
pub struct EventDecoder {
    session: Option<String>,
    auto_approve: bool,
    in_turn: bool,
    aborted: bool,
    busy: bool,
    user_msgs: HashSet<String>,
    kinds: HashMap<String, Kind>,
    seen: HashMap<String, String>,
    tool_out: HashMap<String, String>,
    tools: HashSet<String>,
    done_tools: HashSet<String>,
    /// opencode's tool call id → the part id our tool events use.
    calls: HashMap<String, String>,
    /// Approvals shown and not yet answered: opencode id → ours.
    pending: HashMap<String, RequestId>,
    asks: Vec<Ask>,
    join: Joiner,
    turn: Turn,
}

impl EventDecoder {
    pub fn new(auto_approve: bool) -> Self {
        Self {
            session: None,
            auto_approve,
            in_turn: false,
            aborted: false,
            busy: false,
            user_msgs: HashSet::new(),
            kinds: HashMap::new(),
            seen: HashMap::new(),
            tool_out: HashMap::new(),
            tools: HashSet::new(),
            done_tools: HashSet::new(),
            calls: HashMap::new(),
            pending: HashMap::new(),
            asks: Vec::new(),
            join: Joiner::default(),
            turn: Turn::default(),
        }
    }

    pub fn set_session(&mut self, id: &str) {
        self.session = Some(id.to_string());
    }

    pub fn begin_turn(&mut self) {
        self.in_turn = true;
        self.aborted = false;
        self.busy = false;
    }

    pub fn in_turn(&self) -> bool {
        self.in_turn
    }

    pub fn mark_aborted(&mut self) {
        self.aborted = true;
    }

    /// Approvals to act on, in the order they were asked.
    pub fn take_asks(&mut self) -> Vec<Ask> {
        std::mem::take(&mut self.asks)
    }

    /// The user answered this one through us.
    pub fn answered(&mut self, perm: &str) {
        self.pending.remove(perm);
    }

    /// End the turn now, with this failure, because the server cannot.
    pub fn fail_now(&mut self, status: Option<u16>, message: String) -> Vec<AgentEvent> {
        if !self.in_turn {
            return Vec::new();
        }
        self.turn.fail_with(status, message);
        self.conclude()
    }

    /// End the turn now as interrupted, because the server cannot.
    pub fn abort_now(&mut self) -> Vec<AgentEvent> {
        if !self.in_turn {
            return Vec::new();
        }
        self.aborted = true;
        self.conclude()
    }

    fn conclude(&mut self) -> Vec<AgentEvent> {
        self.in_turn = false;
        self.busy = false;
        self.kinds.clear();
        self.seen.clear();
        self.tool_out.clear();
        self.tools.clear();
        self.done_tools.clear();
        self.join = Joiner::default();
        let exit = Exit {
            code: Some(0),
            interrupted: std::mem::take(&mut self.aborted),
            stderr: Vec::new(),
        };
        let mut out = self.turn.conclude(&exit, Some(&spec::OPENCODE));
        // Approvals still showing can no longer be answered.
        for (_, id) in self.pending.drain() {
            out.insert(0, AgentEvent::AskCancelled { id });
        }
        out
    }

    pub fn decode(&mut self, v: &Value) -> Vec<AgentEvent> {
        let Some(ours) = self.session.clone() else {
            return Vec::new();
        };
        let p = v.get("properties").unwrap_or(&Value::Null);
        let sid = p
            .get("sessionID")
            .or_else(|| p.pointer("/part/sessionID"))
            .or_else(|| p.pointer("/info/sessionID"))
            .and_then(Value::as_str);
        // Sub-agents run in sessions of their own; their work shows up here
        // as the `task` tool's part.
        if sid != Some(ours.as_str()) {
            return Vec::new();
        }
        let mut out = Vec::new();
        match v.get("type").and_then(Value::as_str).unwrap_or("") {
            "message.updated" => {
                let info = p.get("info").unwrap_or(&Value::Null);
                if info.get("role").and_then(Value::as_str) == Some("user") {
                    if let Some(id) = info.get("id").and_then(Value::as_str) {
                        self.user_msgs.insert(id.to_string());
                    }
                }
            }
            "message.part.updated" => {
                if let Some(part) = p.get("part") {
                    self.part(part, &mut out);
                }
            }
            "message.part.delta" => self.delta(p, &mut out),
            "session.status" => match p.pointer("/status/type").and_then(Value::as_str) {
                Some("busy") | Some("retry") => self.busy = true,
                Some("idle") if self.in_turn && (self.busy || self.turn.failed()) => {
                    out.extend(self.conclude());
                }
                _ => {}
            },
            "session.idle" if self.in_turn => out.extend(self.conclude()),
            "session.error" => {
                let e = p.get("error").unwrap_or(&Value::Null);
                if e.get("name").and_then(Value::as_str) == Some("MessageAbortedError") {
                    self.aborted = true;
                } else {
                    let (status, message) = opencode_failure(e);
                    self.turn.fail_with(status, message);
                }
            }
            "permission.asked" => self.permission(p, &mut out),
            "permission.replied" => {
                // Answered somewhere else (opencode's own UI).
                if let Some(id) = p
                    .get("requestID")
                    .and_then(Value::as_str)
                    .and_then(|r| self.pending.remove(r))
                {
                    out.push(AgentEvent::AskCancelled { id });
                }
            }
            _ => {}
        }
        self.turn.note(&out);
        out
    }

    fn part(&mut self, part: &Value, out: &mut Vec<AgentEvent>) {
        let s = |k: &str| part.get(k).and_then(Value::as_str).unwrap_or("");
        let id = s("id").to_string();
        if self.user_msgs.contains(s("messageID")) {
            return;
        }
        match s("type") {
            "text" => {
                self.kinds.insert(id.clone(), Kind::Text);
                if part.get("synthetic").and_then(Value::as_bool) == Some(true)
                    || part.get("ignored").and_then(Value::as_bool) == Some(true)
                {
                    return;
                }
                let seen = self.seen.entry(id.clone()).or_default();
                let delta = suffix(seen, s("text"));
                if !delta.is_empty() {
                    out.push(AgentEvent::Text {
                        delta: self.join.text(&id, &delta),
                    });
                }
            }
            "reasoning" => {
                self.kinds.insert(id.clone(), Kind::Reasoning);
                let seen = self.seen.entry(id).or_default();
                let delta = suffix(seen, s("text"));
                if !delta.is_empty() {
                    out.push(AgentEvent::Reasoning { delta });
                }
            }
            "tool" => self.tool(part, &id, out),
            "step-finish" => {
                if let Some(u) = opencode_usage(part) {
                    out.push(AgentEvent::Usage(u));
                }
            }
            _ => {
                self.kinds.insert(id, Kind::Other);
            }
        }
    }

    fn tool(&mut self, part: &Value, id: &str, out: &mut Vec<AgentEvent>) {
        self.kinds.insert(id.to_string(), Kind::Other);
        let name = part.get("tool").and_then(Value::as_str).unwrap_or("tool");
        if let Some(call) = part.get("callID").and_then(Value::as_str) {
            self.calls.insert(call.to_string(), id.to_string());
        }
        let state = part.get("state").unwrap_or(&Value::Null);
        let status = state.get("status").and_then(Value::as_str).unwrap_or("");
        // A pending tool has no input yet; it is shown once it runs.
        if status == "pending" || status.is_empty() {
            return;
        }
        let input = state.get("input").cloned().unwrap_or(Value::Null);
        if self.tools.insert(id.to_string()) {
            self.join.break_();
            let mut title = tool_title(name, &input);
            if title.is_empty() {
                title = state
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .chars()
                    .take(160)
                    .collect();
            }
            out.push(AgentEvent::ToolCall {
                id: ToolId::new(id),
                name: name.into(),
                input,
                title,
            });
        }
        match status {
            "running" => {
                if let Some(o) = state.pointer("/metadata/output").and_then(Value::as_str) {
                    let seen = self.tool_out.entry(id.to_string()).or_default();
                    let chunk = suffix(seen, o);
                    if !chunk.is_empty() {
                        out.push(AgentEvent::ToolProgress {
                            id: ToolId::new(id),
                            chunk,
                        });
                    }
                }
            }
            "completed" | "error" if self.done_tools.insert(id.to_string()) => {
                let text = state
                    .get(if status == "error" { "error" } else { "output" })
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                out.push(AgentEvent::ToolResult {
                    id: ToolId::new(id),
                    output: ToolOutput::Terminal {
                        text,
                        exit_code: state
                            .pointer("/metadata/exit")
                            .and_then(Value::as_i64)
                            .map(|c| c as i32),
                    },
                    is_error: status == "error",
                });
            }
            _ => {}
        }
    }

    fn delta(&mut self, p: &Value, out: &mut Vec<AgentEvent>) {
        let s = |k: &str| p.get(k).and_then(Value::as_str).unwrap_or("");
        if self.user_msgs.contains(s("messageID")) || s("field") != "text" {
            return;
        }
        let (id, delta) = (s("partID").to_string(), s("delta"));
        if delta.is_empty() {
            return;
        }
        // The part's opening `message.part.updated` always comes first; a
        // delta for a part never seen is taken as reply text.
        let kind = self.kinds.get(&id).copied().unwrap_or(Kind::Text);
        if kind == Kind::Other {
            return;
        }
        self.seen.entry(id.clone()).or_default().push_str(delta);
        out.push(match kind {
            Kind::Reasoning => AgentEvent::Reasoning {
                delta: delta.to_string(),
            },
            _ => AgentEvent::Text {
                delta: self.join.text(&id, delta),
            },
        });
    }

    fn permission(&mut self, p: &Value, out: &mut Vec<AgentEvent>) {
        let Some(perm) = p.get("id").and_then(Value::as_str).map(str::to_string) else {
            return;
        };
        if self.auto_approve {
            self.asks.push(Ask::Auto(perm));
            return;
        }
        let kind = p.get("permission").and_then(Value::as_str).unwrap_or("");
        let meta = p.get("metadata").cloned().unwrap_or(Value::Null);
        let patterns: Vec<String> = p
            .get("patterns")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let m = |k: &str| meta.get(k).and_then(Value::as_str).map(str::to_string);
        let first = patterns.first().cloned().unwrap_or_default();
        let action = match kind {
            "bash" => Action::Bash {
                command: m("command").unwrap_or_else(|| patterns.join(" ")),
                cwd: None,
            },
            "edit" | "write" | "patch" => Action::EditFile {
                path: m("filepath").or_else(|| m("filePath")).unwrap_or(first),
            },
            "read" => Action::ReadFile {
                path: m("filePath").or_else(|| m("filepath")).unwrap_or(first),
            },
            "webfetch" => Action::Fetch {
                url: m("url").unwrap_or(first),
            },
            other => Action::Other {
                name: other.into(),
                input: json!({ "patterns": patterns, "metadata": meta }),
            },
        };
        let id = RequestId::new();
        self.pending.insert(perm.clone(), id);
        let tool_use = p
            .pointer("/tool/callID")
            .and_then(Value::as_str)
            .and_then(|c| self.calls.get(c))
            .map(|part| ToolId::new(part.as_str()));
        out.push(AgentEvent::Permission {
            id,
            action,
            suggestions: vec![Decision::Allow, Decision::AllowAlways, Decision::Deny],
            tool_use,
            input: if meta.is_null() {
                json!({ "patterns": patterns })
            } else {
                meta
            },
            reason: None,
        });
        self.asks.push(Ask::User(id, perm));
    }
}

enum Cmd {
    Prompt(String),
    Interrupt,
    Reply {
        perm: String,
        reply: &'static str,
        abort: bool,
    },
}

/// An OpenCode chat. The work happens in a task of its own (the worker), so
/// a slow server start never blocks the app's driver.
pub struct OpenCodeSession {
    cmd: mpsc::UnboundedSender<Cmd>,
    rx: mpsc::Receiver<AgentEvent>,
    native: Arc<Mutex<Option<String>>>,
    asks: Arc<Mutex<HashMap<RequestId, String>>>,
    system_prompt: Arc<str>,
    primer: Option<String>,
    turns: u32,
}

impl OpenCodeSession {
    /// Starts the chat's server right away, so it is warm by the time the
    /// first message is typed.
    pub fn start(spec: &'static CliSpec, ctx: SessionCtx) -> Self {
        let (tx, rx) = mpsc::channel(EVENT_BUFFER);
        let (cmd, cmds) = mpsc::unbounded_channel();
        let native = Arc::new(Mutex::new(ctx.resume.clone()));
        let asks: Arc<Mutex<HashMap<RequestId, String>>> = Arc::default();
        let turns = u32::from(ctx.resume.is_some());
        let system_prompt = ctx.system_prompt.clone();
        let worker = Worker {
            spec,
            decoder: EventDecoder::new(ctx.permission_mode != PermissionMode::Ask),
            session: ctx.resume.clone(),
            checked: false,
            ctx,
            tx,
            native: native.clone(),
            asks: asks.clone(),
            server: None,
            events: None,
            pump: None,
            use_run: false,
            run_turn: None,
            last_used: Instant::now(),
            closed: false,
        };
        tokio::spawn(worker.run(cmds));
        Self {
            cmd,
            rx,
            native,
            asks,
            system_prompt,
            primer: None,
            turns,
        }
    }
}

#[async_trait]
impl AgentSession for OpenCodeSession {
    async fn send(&mut self, turn: UserTurn) -> Result<(), AgentError> {
        let prompt = compose(self.turns, &self.system_prompt, &mut self.primer, &turn);
        self.turns += 1;
        self.cmd
            .send(Cmd::Prompt(prompt))
            .map_err(|_| AgentError::Closed { exit: None })
    }

    fn events(&mut self) -> BoxStream<'_, AgentEvent> {
        // Ends only if the worker itself is gone.
        Box::pin(futures::stream::poll_fn(move |cx| self.rx.poll_recv(cx)))
    }

    async fn interrupt(&mut self) -> Result<(), AgentError> {
        let _ = self.cmd.send(Cmd::Interrupt);
        Ok(())
    }

    async fn decide(&mut self, id: RequestId, d: Decision) -> Result<(), AgentError> {
        let Some(perm) = self.asks.lock().remove(&id) else {
            return Err(AgentError::Unsupported(
                "that approval is no longer pending",
            ));
        };
        let reply = match d {
            Decision::Allow => "once",
            Decision::AllowAlways => "always",
            Decision::Deny | Decision::Abort => "reject",
        };
        self.cmd
            .send(Cmd::Reply {
                perm,
                reply,
                abort: d == Decision::Abort,
            })
            .map_err(|_| AgentError::Closed { exit: None })
    }

    async fn hydrate(&mut self, transcript: &Transcript) -> Result<HydrateMode, AgentError> {
        let (primer, mode) = primer_for(transcript)?;
        if primer.is_some() {
            self.primer = primer;
        }
        Ok(mode)
    }

    fn native_session_id(&self) -> Option<String> {
        self.native.lock().clone()
    }
}

struct Worker {
    spec: &'static CliSpec,
    ctx: SessionCtx,
    tx: mpsc::Sender<AgentEvent>,
    native: Arc<Mutex<Option<String>>>,
    asks: Arc<Mutex<HashMap<RequestId, String>>>,
    server: Option<Server>,
    /// Payloads from the server's event stream.
    events: Option<mpsc::Receiver<io::Result<String>>>,
    pump: Option<JoinHandle<()>>,
    decoder: EventDecoder,
    /// This chat's opencode session.
    session: Option<String>,
    /// The session is known to exist on the server.
    checked: bool,
    /// The server could not be started: one `opencode run` per turn.
    use_run: bool,
    run_turn: Option<TurnHandle>,
    last_used: Instant,
    closed: bool,
}

async fn next_payload(
    rx: &mut Option<mpsc::Receiver<io::Result<String>>>,
) -> Option<io::Result<String>> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

impl Worker {
    async fn run(mut self, mut cmds: mpsc::UnboundedReceiver<Cmd>) {
        if let Err(e) = self.ensure_server().await {
            // Tried again at the first message, which falls back to `run`.
            tracing::warn!(error = %e, "opencode server did not start");
        }
        loop {
            let idle = self.server.is_some() && !self.decoder.in_turn();
            let deadline = tokio::time::Instant::from_std(self.last_used + IDLE_SHUTDOWN);
            tokio::select! {
                cmd = cmds.recv() => match cmd {
                    None => break,
                    Some(Cmd::Prompt(text)) => self.prompt(text).await,
                    Some(Cmd::Interrupt) => self.interrupt().await,
                    Some(Cmd::Reply { perm, reply, abort }) => self.reply(perm, reply, abort).await,
                },
                payload = next_payload(&mut self.events) => self.on_event(payload).await,
                _ = tokio::time::sleep_until(deadline), if idle => {
                    tracing::info!("stopping idle opencode server");
                    self.stop_server();
                }
            }
            if self.closed {
                break;
            }
        }
        self.stop_server();
        if let Some(t) = self.run_turn.take() {
            t.finish().await;
        }
    }

    async fn emit(&mut self, events: Vec<AgentEvent>) {
        for ev in events {
            if self.tx.send(ev).await.is_err() {
                self.closed = true;
                return;
            }
        }
    }

    fn dir(&self) -> String {
        format!(
            "directory={}",
            query_escape(&self.ctx.cwd.to_string_lossy())
        )
    }

    fn stop_server(&mut self) {
        if let Some(p) = self.pump.take() {
            p.abort();
        }
        self.events = None;
        self.server = None;
    }

    async fn ensure_server(&mut self) -> Result<(), String> {
        if let Some(s) = &mut self.server {
            if s.running() && self.events.is_some() {
                return Ok(());
            }
        }
        self.stop_server();
        let started = Instant::now();
        let server = Server::start(self.spec.bin, &self.ctx).await?;
        let path = format!("/event?{}", self.dir());
        let mut stream = EventStream::connect(&server.ep, &path, REQUEST_TIMEOUT)
            .await
            .map_err(|e| format!("opencode event stream: {e}"))?;
        // Its first event says the subscription is live: nothing caused from
        // here on can be missed.
        match tokio::time::timeout(REQUEST_TIMEOUT, stream.next()).await {
            Ok(Ok(Some(_))) => {}
            other => return Err(format!("opencode event stream did not open: {other:?}")),
        }
        let (tx, rx) = mpsc::channel(4096);
        let pump = tokio::spawn(async move {
            loop {
                let item = match stream.next().await {
                    Ok(Some(data)) => Ok(data),
                    Ok(None) => Err(io::Error::new(io::ErrorKind::UnexpectedEof, "closed")),
                    Err(e) => Err(e),
                };
                let last = item.is_err();
                if tx.send(item).await.is_err() || last {
                    return;
                }
            }
        });
        tracing::info!(
            ms = started.elapsed().as_millis() as u64,
            port = server.ep.port,
            "opencode server ready"
        );
        self.server = Some(server);
        self.events = Some(rx);
        self.pump = Some(pump);
        Ok(())
    }

    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> io::Result<http::Response> {
        let Some(s) = &self.server else {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "no opencode server",
            ));
        };
        let sep = if path.contains('?') { '&' } else { '?' };
        let path = format!("{path}{sep}{}", self.dir());
        http::request(&s.ep, method, &path, body, REQUEST_TIMEOUT).await
    }

    async fn prompt(&mut self, text: String) {
        self.last_used = Instant::now();
        if self.use_run {
            return self.prompt_run(text).await;
        }
        if let Err(e) = self.ensure_server().await {
            tracing::warn!(error = %e, "opencode server unavailable; using `opencode run` per turn");
            self.use_run = true;
            return self.prompt_run(text).await;
        }
        self.decoder.begin_turn();
        let Some(id) = self.session_id().await else {
            return;
        };
        let mut body = json!({ "parts": [{ "type": "text", "text": text }] });
        if let Some((provider, model)) = self.ctx.model.as_deref().and_then(|m| m.split_once('/')) {
            body["model"] = json!({ "providerID": provider, "modelID": model });
        }
        let sent = self
            .call("POST", &format!("/session/{id}/prompt_async"), Some(&body))
            .await;
        let failed = match sent {
            Ok(r) if r.ok() => None,
            Ok(r) => {
                let (status, message) = decode::readable_error(&r.text());
                Some((
                    status.or(Some(r.status)),
                    if message.is_empty() {
                        format!("OpenCode refused the message (HTTP {})", r.status)
                    } else {
                        message
                    },
                ))
            }
            Err(e) => Some((None, format!("could not reach the OpenCode server: {e}"))),
        };
        if let Some((status, message)) = failed {
            let evs = self.decoder.fail_now(status, message);
            self.emit(evs).await;
        }
    }

    /// This chat's opencode session: the one it is resuming, or a new one.
    /// `None` when that failed, with the failure already sent.
    async fn session_id(&mut self) -> Option<String> {
        if let (Some(id), true) = (&self.session, self.checked) {
            return Some(id.clone());
        }
        if let Some(id) = self.session.clone() {
            // From an earlier run of deck; it must still exist.
            match self.call("GET", &format!("/session/{id}"), None).await {
                Ok(r) if r.ok() => {
                    self.checked = true;
                    self.decoder.set_session(&id);
                    return Some(id);
                }
                Ok(r) if r.status == 404 => {
                    // Fatal is what makes the app replay the thread into a
                    // fresh session instead.
                    self.decoder =
                        EventDecoder::new(self.ctx.permission_mode != PermissionMode::Ask);
                    self.session = None;
                    *self.native.lock() = None;
                    self.emit(vec![AgentEvent::Fatal {
                        message: format!("OpenCode no longer has session {id}"),
                        retryable: false,
                    }])
                    .await;
                    return None;
                }
                other => {
                    let evs = self.decoder.fail_now(
                        None,
                        format!("could not open OpenCode session {id}: {}", describe(&other)),
                    );
                    self.emit(evs).await;
                    return None;
                }
            }
        }
        // A headless chat cannot answer opencode's question tool or its plan
        // mode switches; `opencode run` denies them the same way.
        let body = json!({ "permission": [
            { "permission": "question", "action": "deny", "pattern": "*" },
            { "permission": "plan_enter", "action": "deny", "pattern": "*" },
            { "permission": "plan_exit", "action": "deny", "pattern": "*" },
        ]});
        let created = self.call("POST", "/session", Some(&body)).await;
        let id = match &created {
            Ok(r) if r.ok() => r
                .json()
                .and_then(|v| v.get("id").and_then(Value::as_str).map(str::to_string)),
            _ => None,
        };
        let Some(id) = id else {
            let evs = self.decoder.fail_now(
                None,
                format!(
                    "could not start an OpenCode session: {}",
                    describe(&created)
                ),
            );
            self.emit(evs).await;
            return None;
        };
        self.session = Some(id.clone());
        self.checked = true;
        self.decoder.set_session(&id);
        *self.native.lock() = Some(id.clone());
        self.emit(vec![AgentEvent::Started {
            native_session_id: Some(id.clone()),
            model: None,
        }])
        .await;
        Some(id)
    }

    async fn prompt_run(&mut self, text: String) {
        if let Some(prev) = self.run_turn.take() {
            prev.finish().await;
        }
        let resume = self.native.lock().clone();
        // Only a session saved by an earlier run of deck can have vanished.
        let resuming = !self.checked && resume.is_some() && resume == self.ctx.resume;
        self.checked = true;
        let args = self.spec.args(
            &text,
            &self.ctx.cwd.to_string_lossy(),
            self.ctx.model.as_deref(),
            resume.as_deref(),
            self.ctx.permission_mode,
        );
        match spawn_turn(
            self.spec,
            &self.ctx,
            args,
            decode::for_spec(self.spec),
            self.tx.clone(),
            self.native.clone(),
            resuming,
        ) {
            Ok(t) => self.run_turn = Some(t),
            Err(e) => {
                self.emit(vec![
                    AgentEvent::TurnFailed {
                        status: None,
                        message: e.to_string(),
                    },
                    AgentEvent::TurnEnd {
                        stop: StopReason::Error,
                    },
                ])
                .await
            }
        }
    }

    async fn interrupt(&mut self) {
        if let Some(t) = &mut self.run_turn {
            t.interrupt();
        }
        if !self.decoder.in_turn() {
            return;
        }
        self.decoder.mark_aborted();
        let Some(id) = self.session.clone() else {
            let evs = self.decoder.abort_now();
            return self.emit(evs).await;
        };
        match self
            .call("POST", &format!("/session/{id}/abort"), None)
            .await
        {
            // The server now ends the turn itself (`session.idle`).
            Ok(r) if r.ok() => {}
            other => {
                tracing::warn!(result = %describe(&other), "opencode abort failed");
                let evs = self.decoder.abort_now();
                self.emit(evs).await;
            }
        }
    }

    async fn reply(&mut self, perm: String, reply: &'static str, abort: bool) {
        self.decoder.answered(&perm);
        let body = json!({ "reply": reply });
        let r = self
            .call("POST", &format!("/permission/{perm}/reply"), Some(&body))
            .await;
        if !matches!(&r, Ok(r) if r.ok()) {
            tracing::warn!(result = %describe(&r), "opencode permission reply failed");
        }
        if abort {
            self.interrupt().await;
        }
    }

    async fn on_event(&mut self, payload: Option<io::Result<String>>) {
        let data = match payload {
            Some(Ok(data)) => data,
            Some(Err(e)) => return self.server_lost(e.to_string()).await,
            None => return self.server_lost("event stream closed".into()).await,
        };
        let Ok(v) = serde_json::from_str::<Value>(&data) else {
            return;
        };
        let was_in_turn = self.decoder.in_turn();
        let events = self.decoder.decode(&v);
        for ask in self.decoder.take_asks() {
            match ask {
                Ask::Auto(perm) => {
                    let r = self
                        .call(
                            "POST",
                            &format!("/permission/{perm}/reply"),
                            Some(&json!({ "reply": "once" })),
                        )
                        .await;
                    if !matches!(&r, Ok(r) if r.ok()) {
                        tracing::warn!(result = %describe(&r), "opencode auto-approval failed");
                    }
                }
                // Filed before the event goes out, so an answer can never
                // arrive for a request not on file.
                Ask::User(id, perm) => {
                    self.asks.lock().insert(id, perm);
                }
            }
        }
        // Idle counts from the end of the last turn; heartbeats and other
        // chats' events do not keep a server alive.
        if was_in_turn && !self.decoder.in_turn() {
            self.last_used = Instant::now();
        }
        self.emit(events).await;
    }

    async fn server_lost(&mut self, why: String) {
        tracing::warn!(why, "opencode server went away");
        self.stop_server();
        let evs = self.decoder.fail_now(
            None,
            "The OpenCode server stopped in the middle of the reply.".into(),
        );
        self.emit(evs).await;
    }
}

fn describe(r: &io::Result<http::Response>) -> String {
    match r {
        Ok(r) => format!("HTTP {} {}", r.status, r.text()),
        Err(e) => e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LS_TURN: &str = include_str!("testdata/opencode_serve_ls.jsonl");
    const ASK_ABORT: &str = include_str!("testdata/opencode_serve_ask_abort.jsonl");

    fn feed(d: &mut EventDecoder, jsonl: &str) -> Vec<AgentEvent> {
        jsonl
            .lines()
            .filter(|l| !l.trim().is_empty())
            .flat_map(|l| d.decode(&serde_json::from_str(l).unwrap()))
            .collect()
    }

    fn text_of(events: &[AgentEvent]) -> String {
        events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Text { delta } => Some(delta.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_real_tool_turn_streams_reasoning_tool_output_and_text() {
        let mut d = EventDecoder::new(true);
        d.set_session("ses_1");
        d.begin_turn();
        let ev = feed(&mut d, LS_TURN);

        // The prompt is echoed as a user part; it is not the reply.
        assert_eq!(text_of(&ev), "2");
        let reasoning: String = ev
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Reasoning { delta } => Some(delta.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            reasoning, "Run ls.2 files.",
            "deltas, not repeated snapshots"
        );

        let calls: Vec<_> = ev
            .iter()
            .filter_map(|e| match e {
                AgentEvent::ToolCall {
                    id, name, title, ..
                } => Some((id.clone(), name.clone(), title.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(calls.len(), 1, "shown once, when it starts running");
        assert_eq!(calls[0].1, "bash");
        assert_eq!(calls[0].2, "ls");
        assert!(ev.iter().any(|e| matches!(
            e,
            AgentEvent::ToolProgress { chunk, .. } if chunk == "a.txt\nb.rs\n"
        )));
        assert!(ev.iter().any(|e| matches!(
            e,
            AgentEvent::ToolResult { output: ToolOutput::Terminal { text, exit_code: Some(0) }, is_error: false, .. }
                if text == "a.txt\nb.rs\n"
        )));
        let usage: u64 = ev
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Usage(u) => Some(u.output),
                _ => None,
            })
            .sum();
        assert_eq!(usage, 19 + 3, "one usage per step");

        // Exactly one end, at the first idle.
        let ends: Vec<_> = ev
            .iter()
            .filter(|e| matches!(e, AgentEvent::TurnEnd { .. }))
            .collect();
        assert_eq!(ends.len(), 1);
        assert!(matches!(
            ev.iter()
                .rev()
                .find(|e| matches!(e, AgentEvent::TurnEnd { .. })),
            Some(AgentEvent::TurnEnd {
                stop: StopReason::EndTurn
            })
        ));
        assert!(!d.in_turn());
    }

    #[test]
    fn events_of_other_sessions_are_ignored() {
        let mut d = EventDecoder::new(true);
        d.set_session("ses_other");
        d.begin_turn();
        assert!(feed(&mut d, LS_TURN).is_empty());
        assert!(d.in_turn());
    }

    #[test]
    fn a_real_approval_request_is_shown_and_linked_to_its_tool() {
        let mut d = EventDecoder::new(false);
        d.set_session("ses_ask");
        d.begin_turn();
        let mut shown = None;
        let mut asks = Vec::new();
        for l in ASK_ABORT.lines().filter(|l| !l.is_empty()) {
            let v: Value = serde_json::from_str(l).unwrap();
            let is_ask = v["type"] == "permission.asked";
            let ev = d.decode(&v);
            asks.extend(d.take_asks());
            if is_ask {
                shown = ev
                    .into_iter()
                    .find(|e| matches!(e, AgentEvent::Permission { .. }));
                break;
            }
        }
        match shown {
            Some(AgentEvent::Permission {
                id,
                action: Action::Bash { command, .. },
                tool_use,
                ..
            }) => {
                assert_eq!(command, "ls");
                assert!(tool_use.is_some(), "linked through the tool's callID");
                assert_eq!(
                    asks,
                    vec![Ask::User(id, "per_1432d0d58001PpN04NHtfUJhBN".into())]
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn with_approvals_off_a_request_is_answered_not_shown() {
        let mut d = EventDecoder::new(true);
        d.set_session("ses_ask");
        d.begin_turn();
        let ev = feed(&mut d, ASK_ABORT);
        assert!(!ev
            .iter()
            .any(|e| matches!(e, AgentEvent::Permission { .. })));
        // `take_asks` drains, so collect from a fresh pass.
        let mut d = EventDecoder::new(true);
        d.set_session("ses_ask");
        d.begin_turn();
        let mut asks = Vec::new();
        for l in ASK_ABORT.lines().filter(|l| !l.is_empty()) {
            d.decode(&serde_json::from_str(l).unwrap());
            asks.extend(d.take_asks());
        }
        assert_eq!(
            asks,
            vec![Ask::Auto("per_1432d0d58001PpN04NHtfUJhBN".into())]
        );
        assert!(ev.iter().any(|e| matches!(
            e,
            AgentEvent::TurnEnd {
                stop: StopReason::EndTurn
            }
        )));
    }

    #[test]
    fn a_real_abort_ends_the_turn_as_interrupted_once() {
        let mut d = EventDecoder::new(true);
        d.set_session("ses_abort");
        d.begin_turn();
        d.mark_aborted();
        let ev = feed(&mut d, ASK_ABORT);
        assert!(
            !text_of(&ev).is_empty(),
            "the part streamed before the stop"
        );
        let ends: Vec<_> = ev
            .iter()
            .filter_map(|e| match e {
                AgentEvent::TurnEnd { stop } => Some(*stop),
                _ => None,
            })
            .collect();
        // Two `session.idle` follow an abort; the turn ends once.
        assert_eq!(ends, vec![StopReason::Interrupted]);
        assert!(!ev
            .iter()
            .any(|e| matches!(e, AgentEvent::TurnFailed { .. })));
    }

    #[test]
    fn a_session_error_fails_the_turn_with_its_words() {
        let mut d = EventDecoder::new(true);
        d.set_session("s");
        d.begin_turn();
        let lines = [
            r#"{"type":"session.status","properties":{"sessionID":"s","status":{"type":"busy"}}}"#,
            r#"{"type":"session.error","properties":{"sessionID":"s","error":{"name":"ProviderAuthError","data":{"providerID":"anthropic","message":"Not logged in to anthropic"}}}}"#,
            r#"{"type":"session.idle","properties":{"sessionID":"s"}}"#,
        ];
        let ev: Vec<_> = lines
            .iter()
            .flat_map(|l| d.decode(&serde_json::from_str(l).unwrap()))
            .collect();
        assert!(ev.iter().any(|e| matches!(
            e,
            AgentEvent::TurnFailed { message, .. }
                if message == "OpenCode is not signed in: run `opencode auth login` in a terminal."
        )), "{ev:?}");
        assert!(matches!(
            ev.last(),
            Some(AgentEvent::TurnEnd {
                stop: StopReason::Error
            })
        ));
    }

    #[test]
    fn a_turn_that_cannot_be_sent_fails_at_once() {
        let mut d = EventDecoder::new(true);
        d.set_session("s");
        assert!(
            d.fail_now(None, "x".into()).is_empty(),
            "no turn, nothing to end"
        );
        d.begin_turn();
        let ev = d.fail_now(Some(503), "server unavailable".into());
        assert!(
            matches!(
                &ev[..],
                [
                    AgentEvent::Text { .. },
                    AgentEvent::TurnFailed {
                        status: Some(503),
                        ..
                    },
                    AgentEvent::TurnEnd {
                        stop: StopReason::Error
                    }
                ]
            ),
            "{ev:?}"
        );
    }

    #[test]
    fn a_pending_approval_is_withdrawn_when_the_turn_ends() {
        let mut d = EventDecoder::new(false);
        d.set_session("ses_ask");
        d.begin_turn();
        let ask = ASK_ABORT
            .lines()
            .find(|l| l.contains("\"permission.asked\""))
            .unwrap();
        let ev = d.decode(&serde_json::from_str(ask).unwrap());
        let Some(AgentEvent::Permission { id, .. }) = ev.first().cloned() else {
            panic!("{ev:?}")
        };
        let end = d.abort_now();
        assert!(matches!(end.first(), Some(AgentEvent::AskCancelled { id: x }) if *x == id));
    }

    #[test]
    fn tokens_are_long_and_unique() {
        let (a, b) = (token(), token());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }
}
