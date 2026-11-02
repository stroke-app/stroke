//! Claude Code adapter.
//!
//! Talks to the `claude` CLI over `--input-format stream-json` /
//! `--output-format stream-json`, which is the richest protocol of the backends
//! we support — native session ids, resume, MCP config, usage and cost. The
//! other adapters degrade from the shape established here.

pub mod decode;
pub mod replay;
pub mod wire;

use agent_core::{
    AgentAdapter, AgentError, AgentEvent, AgentId, AgentSession, Availability, Caps, Decision,
    HydrateMode, McpTransport, PermissionMode, RequestId, SessionCtx, Transcript, UserTurn,
};
use async_trait::async_trait;
use futures::stream::BoxStream;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc;

/// Tokens of transcript we are willing to replay on a handoff.
const REPLAY_BUDGET: usize = 60_000;

/// Bound on buffered events. Deep enough to absorb a fast burst, shallow enough
/// that a stalled UI applies backpressure instead of growing without limit.
const EVENT_BUFFER: usize = 1024;

pub fn agent_id() -> AgentId {
    AgentId::new("claude")
}

pub struct ClaudeAdapter {
    /// Path to the `claude` binary; overridable for tests and odd installs.
    bin: PathBuf,
}

impl Default for ClaudeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeAdapter {
    pub fn new() -> Self {
        Self {
            bin: PathBuf::from("claude"),
        }
    }

    pub fn with_bin(bin: impl Into<PathBuf>) -> Self {
        Self { bin: bin.into() }
    }

    /// Build the argv for a session. Split out so it is testable without
    /// spawning anything — argv bugs are otherwise only visible at runtime.
    fn args(&self, ctx: &SessionCtx, mcp_config: Option<&PathBuf>) -> Vec<String> {
        let mut a: Vec<String> = vec![
            "-p".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--input-format".into(),
            "stream-json".into(),
            // Required by the CLI for stream-json output.
            "--verbose".into(),
            // Token-level deltas rather than whole messages.
            "--include-partial-messages".into(),
            // Echo each message we send as the model reads it, so one sent
            // mid-turn can be shown where it landed.
            "--replay-user-messages".into(),
            // Ask us, not nobody: a tool call that needs approval (and
            // AskUserQuestion) comes over stdout as a `can_use_tool` request,
            // and the user answers it in deck.
            "--permission-prompt-tool".into(),
            "stdio".into(),
        ];

        // Append rather than replace: the vendor's own harness stays intact and
        // ours layers on top.
        if !ctx.system_prompt.is_empty() {
            a.push("--append-system-prompt".into());
            a.push(ctx.system_prompt.to_string());
        }
        if let Some(model) = &ctx.model {
            a.push("--model".into());
            a.push(model.to_string());
        }
        if let Some(resume) = &ctx.resume {
            a.push("--resume".into());
            a.push(resume.clone());
        }
        if let Some(cfg) = mcp_config {
            a.push("--mcp-config".into());
            a.push(cfg.display().to_string());
        }
        if !ctx.allowed_tools.is_empty() {
            a.push("--allowedTools".into());
            a.push(ctx.allowed_tools.join(","));
        }
        a.push("--permission-mode".into());
        a.push(
            match ctx.permission_mode {
                PermissionMode::Ask => "default",
                PermissionMode::Edit => "acceptEdits",
                PermissionMode::Auto => "default",
                PermissionMode::Yolo => "bypassPermissions",
            }
            .into(),
        );
        a
    }

    /// Project the shared MCP registry into the CLI's config format.
    async fn write_mcp_config(&self, ctx: &SessionCtx) -> Result<Option<PathBuf>, AgentError> {
        let enabled: Vec<_> = ctx.mcp.iter().filter(|s| s.enabled).collect();
        if enabled.is_empty() {
            return Ok(None);
        }
        let mut servers = serde_json::Map::new();
        for spec in enabled {
            let entry = match &spec.transport {
                McpTransport::Stdio { command, args, env } => serde_json::json!({
                    "type": "stdio",
                    "command": command,
                    "args": args,
                    "env": to_json_map(env),
                }),
                McpTransport::Http { url, headers } => serde_json::json!({
                    "type": "http",
                    "url": url,
                    "headers": to_json_map(headers),
                }),
            };
            servers.insert(spec.name.to_string(), entry);
        }
        let doc = serde_json::json!({ "mcpServers": servers });

        tokio::fs::create_dir_all(&ctx.runtime_dir).await?;
        let path = ctx.runtime_dir.join("mcp.json");
        tokio::fs::write(&path, serde_json::to_vec_pretty(&doc).expect("json")).await?;
        Ok(Some(path))
    }
}

#[async_trait]
impl AgentAdapter for ClaudeAdapter {
    fn id(&self) -> AgentId {
        agent_id()
    }

    fn display_name(&self) -> &str {
        "Claude"
    }

    fn models(&self) -> &'static [agent_core::ModelSpec] {
        use agent_core::ModelSpec;
        // Aliases rather than pinned ids: the CLI resolves them to the current
        // snapshot, so this list does not rot every release.
        &[
            ModelSpec {
                id: "opus",
                label: "Opus",
                note: "most capable",
            },
            ModelSpec {
                id: "sonnet",
                label: "Sonnet",
                note: "balanced",
            },
            ModelSpec {
                id: "haiku",
                label: "Haiku",
                note: "fastest",
            },
        ]
    }

    fn setup(&self) -> Option<agent_core::Setup> {
        Some(agent_core::Setup {
            verb: "Install",
            command: "npm install -g @anthropic-ai/claude-code",
            source: "the Claude Code docs",
        })
    }

    fn caps(&self) -> Caps {
        Caps { ..Caps::FULL }
    }

    async fn probe(&self) -> Result<Availability, AgentError> {
        let out = Command::new(&self.bin).arg("--version").output().await;
        match out {
            Ok(o) if o.status.success() => Ok(Availability::Ready {
                version: Some(String::from_utf8_lossy(&o.stdout).trim().to_string()),
            }),
            Ok(o) => Ok(Availability::NeedsAuth {
                hint: String::from_utf8_lossy(&o.stderr).trim().to_string(),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Availability::Missing {
                hint: "install the Claude Code CLI and ensure `claude` is on PATH".into(),
            }),
            Err(e) => Err(AgentError::Spawn(e)),
        }
    }

    async fn spawn(&self, ctx: SessionCtx) -> Result<Box<dyn AgentSession>, AgentError> {
        let mcp_config = self.write_mcp_config(&ctx).await?;
        let args = self.args(&ctx, mcp_config.as_ref());

        let mut child = Command::new(&self.bin)
            .args(&args)
            .envs(ctx.env.iter().map(|(k, v)| (k, v)))
            .current_dir(&ctx.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(AgentError::Spawn)?;

        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        let (tx, rx) = mpsc::channel(EVENT_BUFFER);

        // Decode stdout on its own task. The decoder lives here, so `Decoder`'s
        // state never needs a lock.
        let session_tx = tx.clone();
        let native_id = std::sync::Arc::new(Slot::default());
        let native_id_writer = native_id.clone();
        let asks: Asks = Default::default();
        let asks_writer = asks.clone();
        tokio::spawn(async move {
            let mut decoder = decode::Decoder::new();
            let mut lines = BufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let events = decoder.decode_line(&line);
                        // Filed before the event goes out, so an answer can
                        // never arrive for a request we have not recorded.
                        asks_writer.lock().extend(decoder.take_asks());
                        for ev in events {
                            if let AgentEvent::Started {
                                native_session_id: Some(id),
                                ..
                            } = &ev
                            {
                                native_id_writer.set(id.clone());
                            }
                            if session_tx.send(ev).await.is_err() {
                                return; // consumer dropped
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        let _ = session_tx
                            .send(AgentEvent::Fatal {
                                message: format!("stdout read failed: {e}"),
                                retryable: true,
                            })
                            .await;
                        return;
                    }
                }
            }
            // Stdout closed: the CLI exited. Report it so the supervisor can
            // decide whether to restart.
            let _ = session_tx
                .send(AgentEvent::Fatal {
                    message: "claude exited".into(),
                    retryable: true,
                })
                .await;
        });

        // Keep the last few stderr lines; they are the only diagnostic when the
        // CLI refuses to start.
        let stderr_tail = std::sync::Arc::new(Slot::default());
        let stderr_writer = stderr_tail.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!(target: "agent.claude.stderr", "{line}");
                stderr_writer.set(line);
            }
        });

        Ok(Box::new(ClaudeSession {
            child,
            stdin: Some(stdin),
            rx,
            native_id,
            stderr_tail,
            resumed_from: ctx.resume.clone(),
            control_seq: 0,
            asks,
        }))
    }
}

pub struct ClaudeSession {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: mpsc::Receiver<AgentEvent>,
    native_id: std::sync::Arc<Slot>,
    stderr_tail: std::sync::Arc<Slot>,
    resumed_from: Option<String>,
    control_seq: u64,
    /// Permission requests waiting for the user, by our id.
    asks: Asks,
}

type Asks = std::sync::Arc<
    parking_lot::Mutex<std::collections::HashMap<agent_core::RequestId, decode::PendingAsk>>,
>;

impl ClaudeSession {
    async fn write_line(&mut self, value: &serde_json::Value) -> Result<(), AgentError> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or(AgentError::Closed { exit: None })?;
        let mut buf = serde_json::to_vec(value).expect("serializable");
        buf.push(b'\n');
        match stdin.write_all(&buf).await {
            Ok(()) => stdin.flush().await.map_err(Into::into),
            Err(e) => {
                // Broken pipe means the CLI is gone; surface its stderr, which is
                // far more useful than the io error.
                self.stdin = None;
                if let Some(tail) = self.stderr_tail.get() {
                    Err(AgentError::protocol("claude", tail, None))
                } else {
                    Err(AgentError::Io(e))
                }
            }
        }
    }

    async fn respond(
        &mut self,
        request_id: &str,
        response: serde_json::Value,
    ) -> Result<(), AgentError> {
        self.write_line(&serde_json::json!({
            "type": "control_response",
            "response": {"subtype": "success", "request_id": request_id, "response": response}
        }))
        .await
    }

    async fn send_text(&mut self, text: &str) -> Result<(), AgentError> {
        self.write_line(&serde_json::json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{"type": "text", "text": text}]
            }
        }))
        .await
    }
}

#[async_trait]
impl AgentSession for ClaudeSession {
    async fn send(&mut self, turn: UserTurn) -> Result<(), AgentError> {
        let mut content = Vec::new();
        if !turn.text.is_empty() {
            content.push(serde_json::json!({"type": "text", "text": turn.text}));
        }
        // Images the model can see go in as images, so it looks at them
        // without a tool call; anything else (or too big to inline) is passed
        // by path and the CLI reads it itself.
        for att in &turn.attachments {
            match inline_image(att) {
                Some(block) => content.push(block),
                None => content.push(serde_json::json!({
                    "type": "text",
                    "text": format!("[attached file: {}]", att.path.display())
                })),
            }
        }
        let mut line = serde_json::json!({
            "type": "user",
            "parent_tool_use_id": null,
            "message": {"role": "user", "content": content}
        });
        // Sent mid-turn, the CLI queues it and reads it at the next step;
        // `now` asks it to stop or background the current work instead.
        if let Some(id) = &turn.id {
            line["uuid"] = serde_json::json!(id);
        }
        if turn.now {
            line["priority"] = serde_json::json!("now");
        }
        self.write_line(&line).await
    }

    fn events(&mut self) -> BoxStream<'_, AgentEvent> {
        let rx = &mut self.rx;
        Box::pin(futures::stream::poll_fn(move |cx| rx.poll_recv(cx)))
    }

    async fn interrupt(&mut self) -> Result<(), AgentError> {
        self.control_seq += 1;
        let req = serde_json::json!({
            "type": "control_request",
            "request_id": format!("int-{}", self.control_seq),
            "request": {"subtype": "interrupt"}
        });
        // If the control channel is not honoured, killing the child is the only
        // reliable stop. The supervisor restarts it for the next turn.
        if self.write_line(&req).await.is_err() {
            let _ = self.child.start_kill();
        }
        Ok(())
    }

    async fn decide(&mut self, id: RequestId, decision: Decision) -> Result<(), AgentError> {
        let Some(ask) = self.asks.lock().remove(&id) else {
            return Ok(()); // already answered or cancelled
        };
        let response = match decision {
            Decision::Allow => serde_json::json!({"behavior": "allow", "updatedInput": ask.input}),
            // "Always" means for this chat: the CLI suggests rules saved to
            // the project's settings file, which deck must not write behind
            // the user's back, so they are kept to the session.
            Decision::AllowAlways => serde_json::json!({
                "behavior": "allow",
                "updatedInput": ask.input,
                "updatedPermissions": session_rules(&ask.suggestions),
            }),
            Decision::Deny => serde_json::json!({
                "behavior": "deny",
                "message": "The user declined this. Do not retry it; ask them, or try another way."
            }),
            Decision::Abort => serde_json::json!({
                "behavior": "deny",
                "message": "The user declined this and stopped the turn.",
                "interrupt": true
            }),
        };
        self.respond(&ask.request_id, response).await
    }

    async fn answer(
        &mut self,
        id: RequestId,
        answers: Vec<(String, String)>,
    ) -> Result<(), AgentError> {
        let Some(ask) = self.asks.lock().remove(&id) else {
            return Ok(());
        };
        let mut input = ask.input;
        let map: serde_json::Map<String, serde_json::Value> = answers
            .into_iter()
            .map(|(q, a)| (q, serde_json::Value::String(a)))
            .collect();
        if let Some(obj) = input.as_object_mut() {
            obj.insert("answers".into(), serde_json::Value::Object(map));
        }
        self.respond(
            &ask.request_id,
            serde_json::json!({"behavior": "allow", "updatedInput": input}),
        )
        .await
    }

    async fn hydrate(&mut self, transcript: &Transcript) -> Result<HydrateMode, AgentError> {
        // Resume was requested at spawn time via `--resume`, so the backend
        // already holds this history; replaying it would duplicate context.
        if self.resumed_from.is_some() {
            return Ok(HydrateMode::NativeResume);
        }
        if transcript.messages.is_empty() {
            return Ok(HydrateMode::Fresh);
        }
        let replay = replay::render(transcript, REPLAY_BUDGET);
        self.send_text(&replay.text).await?;
        Ok(if replay.dropped > 0 {
            HydrateMode::ReplayedPartial {
                dropped_messages: replay.dropped,
            }
        } else {
            HydrateMode::Replayed
        })
    }

    fn native_session_id(&self) -> Option<String> {
        self.native_id.get()
    }
}

/// The CLI's "always allow" suggestions, kept to this session: new rules and
/// directories only (not a mode switch), never written to a settings file.
fn session_rules(suggestions: &[serde_json::Value]) -> Vec<serde_json::Value> {
    suggestions
        .iter()
        .filter(|s| {
            matches!(
                s.get("type").and_then(|t| t.as_str()),
                Some("addRules" | "addDirectories")
            )
        })
        .map(|s| {
            let mut s = s.clone();
            s["destination"] = serde_json::json!("session");
            s
        })
        .collect()
}

/// Largest image sent inline. The API's ceiling is 5 MB of base64, which a
/// 3.75 MB file just fits; bigger files go by path.
const INLINE_IMAGE_MAX: u64 = 3_750_000;

fn inline_image(att: &agent_core::Attachment) -> Option<serde_json::Value> {
    use base64::Engine;
    if !matches!(
        att.mime.as_str(),
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    ) {
        return None;
    }
    if std::fs::metadata(&att.path).ok()?.len() > INLINE_IMAGE_MAX {
        return None;
    }
    let bytes = std::fs::read(&att.path).ok()?;
    Some(serde_json::json!({
        "type": "image",
        "source": {
            "type": "base64",
            "media_type": att.mime.as_str(),
            "data": base64::engine::general_purpose::STANDARD.encode(bytes),
        }
    }))
}

fn to_json_map(pairs: &[(String, String)]) -> serde_json::Map<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect()
}

/// A string cell shared between a background reader task and the session handle.
#[derive(Default)]
struct Slot(parking_lot::Mutex<Option<String>>);

impl Slot {
    fn set(&self, value: String) {
        *self.0.lock() = Some(value);
    }

    fn get(&self) -> Option<String> {
        self.0.lock().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::McpServerSpec;
    use std::sync::Arc;

    fn ctx() -> SessionCtx {
        SessionCtx {
            session_id: agent_core::SessionId::new(),
            cwd: PathBuf::from("/tmp"),
            system_prompt: Arc::from("HARNESS"),
            model: Some("claude-opus-5".into()),
            mcp: Arc::new(vec![]),
            skills_dir: None,
            runtime_dir: PathBuf::from("/tmp/does-not-matter"),
            resume: None,
            permission_mode: PermissionMode::Ask,
            env: Vec::new(),
            allowed_tools: Vec::new(),
        }
    }

    #[test]
    fn argv_requests_streaming_json_both_directions() {
        let a = ClaudeAdapter::new().args(&ctx(), None);
        let joined = a.join(" ");
        assert!(joined.contains("--output-format stream-json"));
        assert!(joined.contains("--input-format stream-json"));
        assert!(joined.contains("--include-partial-messages"));
        // The CLI rejects stream-json output without --verbose.
        assert!(a.iter().any(|x| x == "--verbose"));
    }

    #[test]
    fn argv_appends_rather_than_replaces_the_vendor_prompt() {
        let a = ClaudeAdapter::new().args(&ctx(), None);
        assert!(a.iter().any(|x| x == "--append-system-prompt"));
        assert!(
            !a.iter().any(|x| x == "--system-prompt"),
            "replacing the vendor prompt must be opt-in, not the default"
        );
        let i = a
            .iter()
            .position(|x| x == "--append-system-prompt")
            .unwrap();
        assert_eq!(a[i + 1], "HARNESS");
    }

    #[test]
    fn argv_passes_resume_id_when_reattaching() {
        let mut c = ctx();
        c.resume = Some("sess-123".into());
        let a = ClaudeAdapter::new().args(&c, None);
        let i = a.iter().position(|x| x == "--resume").unwrap();
        assert_eq!(a[i + 1], "sess-123");
    }

    #[test]
    fn permission_modes_map_to_cli_flags() {
        for (mode, expected) in [
            (PermissionMode::Ask, "default"),
            (PermissionMode::Edit, "acceptEdits"),
            (PermissionMode::Yolo, "bypassPermissions"),
        ] {
            let mut c = ctx();
            c.permission_mode = mode;
            let a = ClaudeAdapter::new().args(&c, None);
            let i = a.iter().position(|x| x == "--permission-mode").unwrap();
            assert_eq!(a[i + 1], expected, "for {mode:?}");
        }
    }

    #[tokio::test]
    async fn mcp_config_is_projected_into_the_cli_format() {
        let dir = std::env::temp_dir().join(format!("yourcode-test-{}", uuid_like()));
        let mut c = ctx();
        c.runtime_dir = dir.clone();
        c.mcp = Arc::new(vec![
            McpServerSpec {
                name: "azure".into(),
                transport: McpTransport::Stdio {
                    command: "npx".into(),
                    args: vec!["-y".into(), "azure-mcp".into()],
                    env: vec![("TOKEN".into(), "x".into())],
                },
                enabled: true,
            },
            McpServerSpec {
                name: "disabled".into(),
                transport: McpTransport::Http {
                    url: "https://example.com".into(),
                    headers: vec![],
                },
                enabled: false,
            },
        ]);

        let path = ClaudeAdapter::new()
            .write_mcp_config(&c)
            .await
            .unwrap()
            .expect("config written");
        let doc: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(doc["mcpServers"]["azure"]["command"], "npx");
        assert_eq!(doc["mcpServers"]["azure"]["env"]["TOKEN"], "x");
        assert!(
            doc["mcpServers"].get("disabled").is_none(),
            "disabled servers must not be projected"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn no_mcp_config_written_when_registry_is_empty() {
        assert!(ClaudeAdapter::new()
            .write_mcp_config(&ctx())
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn probe_reports_missing_binary_instead_of_erroring() {
        let a = ClaudeAdapter::with_bin("definitely-not-a-real-binary-xyz");
        match a.probe().await.unwrap() {
            Availability::Missing { .. } => {}
            other => panic!("expected Missing, got {other:?}"),
        }
    }

    fn uuid_like() -> u128 {
        // Test-only unique suffix without pulling in uuid here.
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    #[test]
    fn always_allow_stays_in_the_session() {
        let suggestions = vec![
            serde_json::json!({"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"touch a"}],"behavior":"allow","destination":"localSettings"}),
            serde_json::json!({"type":"addDirectories","directories":["/tmp/x"],"destination":"session"}),
            serde_json::json!({"type":"setMode","mode":"acceptEdits","destination":"session"}),
        ];
        let kept = super::session_rules(&suggestions);
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().all(|s| s["destination"] == "session"));
        assert_eq!(kept[0]["type"], "addRules");
    }
}
