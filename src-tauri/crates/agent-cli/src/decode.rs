//! Decoders for the CLI backends.
//!
//! Each turn of these backends is one process, and each decoder reads that
//! process's stdout a line at a time. A decoder never ends the turn itself:
//! [`Decode::end`] does, once the process has exited, because only then is it
//! known whether the turn failed (a non-zero exit, an error event, or a CLI
//! that printed its complaint to stderr and nothing at all to stdout). That is
//! also where a failure is turned into something a person can read and act on,
//! instead of an empty reply.

use crate::spec::{CliSpec, Wire};
use agent_core::{AgentEvent, StopReason, ToolId, ToolOutput, Usage};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// How the turn's process ended.
#[derive(Debug, Clone, Default)]
pub struct Exit {
    /// `None` when it was killed by a signal.
    pub code: Option<i32>,
    /// We killed it because the user pressed stop.
    pub interrupted: bool,
    /// The last lines it wrote to stderr.
    pub stderr: Vec<String>,
}

impl Exit {
    pub fn clean() -> Self {
        Self {
            code: Some(0),
            ..Default::default()
        }
    }
}

/// A failed turn, as the backend reported it.
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    pub status: Option<u16>,
    pub message: String,
}

/// One backend's stdout, decoded line by line.
pub trait Decode: Send {
    fn decode_line(&mut self, line: &str) -> Vec<AgentEvent>;
    /// The process exited: close the turn, as a failure if it was one.
    fn end(&mut self, exit: &Exit) -> Vec<AgentEvent>;
}

/// The decoder for a backend's wire format.
pub fn for_spec(spec: &'static CliSpec) -> Box<dyn Decode> {
    match spec.wire {
        Wire::OpenCodeJson => Box::new(OpenCodeDecoder::for_spec(spec)),
        Wire::CodexJson => Box::new(CodexDecoder::new(spec)),
        Wire::GeminiJson => Box::new(GeminiDecoder::new(spec)),
        Wire::GrokJson => Box::new(GrokDecoder::new(spec)),
        Wire::PlainText => Box::new(PlainDecoder::for_spec(spec)),
    }
}

/// What every decoder tracks about the turn in flight.
#[derive(Debug, Default)]
pub(crate) struct Turn {
    /// Visible reply text went out.
    text: bool,
    /// Anything at all went out: text, reasoning or a tool.
    any: bool,
    failure: Option<Failure>,
    stop: Option<StopReason>,
}

impl Turn {
    pub(crate) fn note(&mut self, events: &[AgentEvent]) {
        for e in events {
            match e {
                AgentEvent::Text { .. } => {
                    self.text = true;
                    self.any = true;
                }
                AgentEvent::Reasoning { .. }
                | AgentEvent::ToolCall { .. }
                | AgentEvent::ToolResult { .. } => self.any = true,
                _ => {}
            }
        }
    }

    fn fail(&mut self, raw: &str) {
        let (status, message) = readable_error(raw);
        self.fail_with(status, message);
    }

    pub(crate) fn failed(&self) -> bool {
        self.failure.is_some()
    }

    pub(crate) fn fail_with(&mut self, status: Option<u16>, message: String) {
        // The first error is the cause; later ones are usually its echo.
        if self.failure.is_none() && !message.is_empty() {
            self.failure = Some(Failure { status, message });
        }
    }

    /// The turn's closing events, given how its process ended.
    pub(crate) fn conclude(&mut self, exit: &Exit, spec: Option<&CliSpec>) -> Vec<AgentEvent> {
        let turn = std::mem::take(self);
        if exit.interrupted {
            return vec![AgentEvent::TurnEnd {
                stop: StopReason::Interrupted,
            }];
        }
        let failure = turn.failure.or_else(|| {
            let said = from_stderr(&exit.stderr);
            match exit.code {
                Some(0) if turn.any || said.is_none() => None,
                Some(0) => said.map(|message| Failure {
                    status: None,
                    message,
                }),
                code => Some(Failure {
                    status: None,
                    message: said.unwrap_or_else(|| {
                        let who = spec.map_or("the agent", |s| s.display_name);
                        match code {
                            Some(c) => format!("{who} exited with code {c} and said nothing."),
                            None => format!("{who} was stopped by a signal."),
                        }
                    }),
                }),
            }
        });
        let Some(mut f) = failure else {
            return vec![AgentEvent::TurnEnd {
                stop: turn.stop.unwrap_or(StopReason::EndTurn),
            }];
        };
        if let Some(spec) = spec {
            f.message = explain(spec, &f.message);
        }
        let mut out = Vec::new();
        // The reply would otherwise be empty, and the error easy to miss.
        if !turn.text {
            out.push(AgentEvent::Text {
                delta: f.message.clone(),
            });
        }
        out.push(AgentEvent::TurnFailed {
            status: f.status.filter(|s| retryable_status(*s)),
            message: f.message,
        });
        out.push(AgentEvent::TurnEnd {
            stop: StopReason::Error,
        });
        out
    }
}

/// Statuses worth reporting as such. For the rest (400, 401, 404…) the CLI's
/// own words say more than a generic label for the code would, so the status
/// is left out and the message is what gets read.
fn retryable_status(s: u16) -> bool {
    matches!(s, 408 | 429 | 500..=599)
}

/// Say what fixes a failure the user can fix: signing in, or another model.
pub fn explain(spec: &CliSpec, message: &str) -> String {
    const SIGNED_OUT: &[&str] = &[
        "not signed in",
        "not logged in",
        "please set an auth method",
        "login required",
        "please log in",
        "please login",
        "unauthenticated",
        // vibe: "Missing MISTRAL_API_KEY environment variable for mistral provider."
        "_api_key environment variable",
    ];
    // codex: "The 'gpt-5.5' model is not supported when using Codex with a
    // ChatGPT account."
    const MODEL_REFUSED: &[&str] = &[
        "model is not supported",
        "unknown model",
        "model not found",
        "model_not_found",
    ];
    let lower = message.to_lowercase();
    let pickable = !spec.models.is_empty() || spec.list_models.is_some();
    match spec.login {
        Some(login) if SIGNED_OUT.iter().any(|n| lower.contains(n)) => format!(
            "{} is not signed in: run `{login}` in a terminal.",
            spec.display_name
        ),
        // All vibe's preview TUI says when it cannot open a session, most
        // often for want of an API key.
        Some(login) if lower == "session/start" => format!(
            "{} could not start a session. If it is not signed in, run `{login}` in a terminal.",
            spec.display_name
        ),
        // Short enough that the app's 80-character reason keeps the fix.
        _ if pickable && MODEL_REFUSED.iter().any(|n| lower.contains(n)) => match quoted(message) {
            Some(model) => format!(
                "{} cannot use {model} with this account: pick a model in the composer.",
                spec.display_name
            ),
            None => format!("{message} Pick a model in the composer."),
        },
        _ => message.to_string(),
    }
}

/// The first word in quotes: `gpt-5.5` from "The 'gpt-5.5' model is…".
fn quoted(s: &str) -> Option<&str> {
    let (i, q) = s
        .char_indices()
        .find(|(_, c)| matches!(c, '\'' | '`' | '"'))?;
    let rest = &s[i + 1..];
    let word = &rest[..rest.find(q)?];
    (!word.is_empty() && word.len() <= 60 && !word.contains(char::is_whitespace)).then_some(word)
}

/// The readable part of an error a CLI reported, and its HTTP status if it
/// gave one. These arrive as JSON inside a string inside JSON (codex wraps the
/// API's error body; gemini wraps it twice), so layers are peeled until the
/// innermost `message` is reached.
pub fn readable_error(raw: &str) -> (Option<u16>, String) {
    let mut status = None;
    let mut msg = raw.trim().to_string();
    for _ in 0..6 {
        let Some(v) = embedded_json(&msg) else { break };
        let (s, m) = dig(&v);
        status = s.or(status);
        match m {
            Some(m) if m.trim() != msg => msg = m.trim().to_string(),
            _ => break,
        }
    }
    let flat = msg.split_whitespace().collect::<Vec<_>>().join(" ");
    (status, clip(&flat, 400))
}

fn embedded_json(s: &str) -> Option<Value> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str::<Value>(&s[start..=end])
        .ok()
        .filter(Value::is_object)
}

fn dig(v: &Value) -> (Option<u16>, Option<String>) {
    let status = [
        "status",
        "code",
        "http_status",
        "statusCode",
        "raw_status_code",
    ]
    .iter()
    .find_map(|k| v.get(k).and_then(Value::as_u64))
    .and_then(|n| u16::try_from(n).ok())
    .filter(|n| (100..600).contains(n));
    let own = v.get("message").and_then(Value::as_str).map(str::to_string);
    for k in ["error", "data"] {
        match v.get(k) {
            Some(inner) if inner.is_object() => {
                let (s, m) = dig(inner);
                if m.is_some() {
                    return (s.or(status), m);
                }
            }
            Some(Value::String(s)) if !s.is_empty() => return (status, Some(s.clone())),
            _ => {}
        }
    }
    (status, own)
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// What a CLI said on stderr, minus the chatter these CLIs print on every
/// run. `None` when nothing meaningful is left.
pub fn from_stderr(lines: &[String]) -> Option<String> {
    const NOISE: &[&str] = &[
        "yolo mode is enabled",
        "reading additional input from stdin",
        "approval mode overridden",
        "skill conflict detected",
        "[startup]",
        "256-color support",
        "loaded cached credentials",
        "opencode_server_password",
        "duplicate skill name",
        "data collection is disabled",
        "level=info",
        "level=debug",
        "level=warn",
        // vibe's preview TUI, on every failure.
        "you are using the new vibe tui",
    ];
    let mut kept: Vec<String> = Vec::new();
    for line in lines {
        let t = strip_ansi(line).trim().to_string();
        if t.is_empty() || t.starts_with("at ") || t.starts_with("file://") {
            continue;
        }
        let lower = t.to_lowercase();
        if NOISE.iter().any(|n| lower.contains(n)) {
            continue;
        }
        // grok prints its error twice, once behind "Error: ".
        let t = t.strip_prefix("Error: ").unwrap_or(&t).to_string();
        if !kept.contains(&t) {
            kept.push(t);
        }
    }
    if kept.is_empty() {
        return None;
    }
    let tail = kept[kept.len().saturating_sub(6)..].join(" ");
    let (_, msg) = readable_error(&tail);
    Some(msg)
}

/// Joins separate text blocks of one reply with a blank line, so two messages
/// do not run into each other.
#[derive(Default)]
pub(crate) struct Joiner {
    /// The block the last text came from.
    last: Option<String>,
}

impl Joiner {
    pub(crate) fn text(&mut self, block: &str, delta: &str) -> String {
        let out = match &self.last {
            Some(prev) if prev != block => format!("\n\n{delta}"),
            _ => delta.to_string(),
        };
        self.last = Some(block.to_string());
        out
    }

    /// A tool ran: the next text starts a block of its own anyway.
    pub(crate) fn break_(&mut self) {
        self.last = None;
    }
}

/// Only what `full` adds to what was already sent. A value that is not an
/// extension was rewritten, so it is sent whole.
pub(crate) fn suffix(seen: &mut String, full: &str) -> String {
    let delta = if full.len() >= seen.len() && full.starts_with(seen.as_str()) {
        full[seen.len()..].to_string()
    } else {
        full.to_string()
    };
    *seen = full.to_string();
    delta
}

/// Decoder for `opencode run --format json`.
///
/// Written against events captured from the real CLI:
/// ```text
/// {"type":"step_start","sessionID":"ses_…","part":{…,"type":"step-start"}}
/// {"type":"text","sessionID":"ses_…","part":{"id":"prt_…","type":"text","text":"Hi",…}}
/// {"type":"step_finish","sessionID":"ses_…","part":{"reason":"stop","tokens":{…},"cost":0,…}}
/// ```
///
/// `run` prints a text part only once it is complete, and a tool only once it
/// finished, which is why deck drives `opencode serve` instead and only falls
/// back to this. Text is still diffed per `part.id`, so a part that does
/// arrive twice is not duplicated.
#[derive(Default)]
pub struct OpenCodeDecoder {
    session_id: Option<String>,
    /// Last text seen per part id, so repeated snapshots become deltas.
    parts: HashMap<String, String>,
    tools: HashMap<String, String>,
    join: Joiner,
    turn: Turn,
    spec: Option<&'static CliSpec>,
}

impl OpenCodeDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    fn for_spec(spec: &'static CliSpec) -> Self {
        Self {
            spec: Some(spec),
            ..Self::default()
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn decode_line(&mut self, line: &str) -> Vec<AgentEvent> {
        let out = self.decode(line);
        self.turn.note(&out);
        out
    }

    fn decode(&mut self, line: &str) -> Vec<AgentEvent> {
        let line = line.trim();
        if line.is_empty() || !line.starts_with('{') {
            return Vec::new();
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            tracing::debug!(line = %&line[..line.len().min(160)], "undecodable opencode line");
            return Vec::new();
        };

        let mut out = Vec::new();
        if let Some(sid) = v.get("sessionID").and_then(Value::as_str) {
            if self.session_id.is_none() {
                self.session_id = Some(sid.to_string());
                out.push(AgentEvent::Started {
                    native_session_id: Some(sid.to_string()),
                    model: None,
                });
            }
        }

        let part = v.get("part");
        match v.get("type").and_then(Value::as_str).unwrap_or("") {
            "text" => {
                if let Some(part) = part {
                    let id = part.get("id").and_then(Value::as_str).unwrap_or("");
                    let full = part.get("text").and_then(Value::as_str).unwrap_or("");
                    let seen = self.parts.entry(id.to_string()).or_default();
                    let delta = suffix(seen, full);
                    if !delta.is_empty() {
                        out.push(AgentEvent::Text {
                            delta: self.join.text(id, &delta),
                        });
                    }
                }
            }

            "reasoning" => {
                if let Some(part) = part {
                    let id = format!("r{}", part.get("id").and_then(Value::as_str).unwrap_or(""));
                    let full = part
                        .get("text")
                        .or_else(|| part.get("reasoning"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let seen = self.parts.entry(id).or_default();
                    let delta = suffix(seen, full);
                    if !delta.is_empty() {
                        out.push(AgentEvent::Reasoning { delta });
                    }
                }
            }

            "tool" | "tool_start" | "tool_use" => {
                if let Some(part) = part {
                    self.join.break_();
                    let id = part
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let name = part
                        .get("tool")
                        .or_else(|| part.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .to_string();
                    let state = part.get("state");
                    let input = state
                        .and_then(|s| s.get("input"))
                        .or_else(|| part.get("input"))
                        .cloned()
                        .unwrap_or(Value::Null);
                    let status = state
                        .and_then(|s| s.get("status"))
                        .and_then(Value::as_str)
                        .unwrap_or("");

                    if !self.tools.contains_key(&id) {
                        self.tools.insert(id.clone(), name.clone());
                        out.push(AgentEvent::ToolCall {
                            id: ToolId::new(id.as_str()),
                            title: tool_title(&name, &input),
                            name: name.into(),
                            input,
                        });
                    }

                    // A completed tool carries its output on the same event.
                    if matches!(status, "completed" | "error") {
                        let text = state
                            .and_then(|s| s.get("output").or_else(|| s.get("error")))
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        out.push(AgentEvent::ToolResult {
                            id: ToolId::new(id.as_str()),
                            output: ToolOutput::Terminal {
                                text,
                                exit_code: state
                                    .and_then(|s| s.pointer("/metadata/exit"))
                                    .and_then(Value::as_i64)
                                    .map(|c| c as i32),
                            },
                            is_error: status == "error",
                        });
                    }
                }
            }

            "step_finish" => {
                if let Some(u) = part.and_then(opencode_usage) {
                    out.push(AgentEvent::Usage(u));
                }
            }

            "error" => {
                let e = v.get("error").unwrap_or(&v);
                if e.get("name").and_then(Value::as_str) == Some("MessageAbortedError") {
                    self.turn.stop = Some(StopReason::Interrupted);
                } else {
                    let (status, message) = opencode_failure(e);
                    self.turn.fail_with(status, message);
                }
            }

            _ => {}
        }
        out
    }

    /// Called when the process exits; a one-shot run ends its turn there.
    pub fn finish(&mut self) -> Vec<AgentEvent> {
        self.end(&Exit::clean())
    }

    fn reset(&mut self) {
        self.parts.clear();
        self.tools.clear();
        self.join = Joiner::default();
    }
}

impl Decode for OpenCodeDecoder {
    fn decode_line(&mut self, line: &str) -> Vec<AgentEvent> {
        OpenCodeDecoder::decode_line(self, line)
    }

    fn end(&mut self, exit: &Exit) -> Vec<AgentEvent> {
        self.reset();
        self.turn.conclude(exit, self.spec)
    }
}

/// Token counts from an opencode `step-finish` part.
pub(crate) fn opencode_usage(part: &Value) -> Option<Usage> {
    let t = part.get("tokens")?;
    let n = |k: &str| t.get(k).and_then(Value::as_u64).unwrap_or(0);
    let cn = |k: &str| {
        t.pointer(&format!("/cache/{k}"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    let (input, output) = (n("input"), n("output"));
    let (read, write) = (cn("read"), cn("write"));
    Some(Usage {
        input,
        output,
        cache_read: read,
        cache_write: write,
        cost_usd: part.get("cost").and_then(Value::as_f64),
        // Each step sends the whole thread again.
        context: input + read + write + output + n("reasoning"),
        ..Default::default()
    })
}

/// The status and words of an opencode error object:
/// `{"name":"APIError","data":{"message":"…","statusCode":429}}`.
pub(crate) fn opencode_failure(e: &Value) -> (Option<u16>, String) {
    let raw = e
        .pointer("/data/message")
        .or_else(|| e.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| e.get("name").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| "opencode reported an error".into());
    let (inner, message) = readable_error(&raw);
    let status = e
        .pointer("/data/statusCode")
        .and_then(Value::as_u64)
        .and_then(|n| u16::try_from(n).ok());
    (status.or(inner), message)
}

/// Decoder for `codex exec --json`.
///
/// Captured from codex-cli 0.146.0:
/// ```text
/// {"type":"thread.started","thread_id":"01a14317-…"}
/// {"type":"turn.started"}
/// {"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"/bin/zsh -lc ls",…,"status":"in_progress"}}
/// {"type":"item.completed","item":{"id":"item_0","type":"command_execution",…,"aggregated_output":"a.txt\nb.rs\n","exit_code":0,"status":"completed"}}
/// {"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"There are 2 files"}}
/// {"type":"turn.completed","usage":{"input_tokens":29189,"cached_input_tokens":0,…,"output_tokens":149}}
/// ```
/// and on failure `{"type":"turn.failed","error":{"message":"<the API's error JSON>"}}`.
pub struct CodexDecoder {
    spec: &'static CliSpec,
    /// Last text seen per item, so an item updated in place sends a delta.
    seen: HashMap<String, String>,
    /// Commands whose output was already streamed, by item id.
    output: HashMap<String, String>,
    tools: HashSet<String>,
    join: Joiner,
    turn: Turn,
    /// The last top-level `error`, for a run that dies without `turn.failed`.
    last_error: Option<String>,
}

impl CodexDecoder {
    pub fn new(spec: &'static CliSpec) -> Self {
        Self {
            spec,
            seen: HashMap::new(),
            output: HashMap::new(),
            tools: HashSet::new(),
            join: Joiner::default(),
            turn: Turn::default(),
            last_error: None,
        }
    }

    fn item(&mut self, item: &Value, done: bool, out: &mut Vec<AgentEvent>) {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let s = |k: &str| item.get(k).and_then(Value::as_str).unwrap_or("");
        match s("type") {
            "agent_message" => {
                let seen = self.seen.entry(id.clone()).or_default();
                let delta = suffix(seen, s("text"));
                if !delta.is_empty() {
                    out.push(AgentEvent::Text {
                        delta: self.join.text(&id, &delta),
                    });
                }
            }
            "reasoning" => {
                let seen = self.seen.entry(format!("r{id}")).or_default();
                let delta = suffix(seen, s("text"));
                if !delta.is_empty() {
                    out.push(AgentEvent::Reasoning { delta });
                }
            }
            "command_execution" => {
                let command = s("command").to_string();
                self.call(&id, "bash", serde_json::json!({ "command": command }), out);
                let text = s("aggregated_output");
                let seen = self.output.entry(id.clone()).or_default();
                if done {
                    let code = item.get("exit_code").and_then(Value::as_i64);
                    out.push(AgentEvent::ToolResult {
                        id: ToolId::new(id.as_str()),
                        output: ToolOutput::Terminal {
                            text: text.to_string(),
                            exit_code: code.map(|c| c as i32),
                        },
                        is_error: s("status") == "failed" || code.is_some_and(|c| c != 0),
                    });
                } else {
                    let chunk = suffix(seen, text);
                    if !chunk.is_empty() {
                        out.push(AgentEvent::ToolProgress {
                            id: ToolId::new(id.as_str()),
                            chunk,
                        });
                    }
                }
            }
            "file_change" => {
                let changes = item.get("changes").cloned().unwrap_or(Value::Null);
                let paths: Vec<String> = changes
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|c| {
                                let path = c.get("path")?.as_str()?;
                                let kind =
                                    c.get("kind").and_then(Value::as_str).unwrap_or("update");
                                Some(format!("{kind} {path}"))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let first = changes
                    .pointer("/0/path")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                self.call(
                    &id,
                    "apply_patch",
                    serde_json::json!({ "path": first, "changes": changes }),
                    out,
                );
                if done {
                    out.push(AgentEvent::ToolResult {
                        id: ToolId::new(id.as_str()),
                        output: ToolOutput::Text(paths.join("\n")),
                        is_error: s("status") == "failed",
                    });
                }
            }
            "mcp_tool_call" => {
                let name = format!("mcp__{}__{}", s("server"), s("tool"));
                let input = item.get("arguments").cloned().unwrap_or(Value::Null);
                self.call(&id, &name, input, out);
                if done {
                    let error = item.pointer("/error/message").and_then(Value::as_str);
                    out.push(AgentEvent::ToolResult {
                        id: ToolId::new(id.as_str()),
                        output: match error {
                            Some(e) => ToolOutput::Text(e.to_string()),
                            None => ToolOutput::Structured(
                                item.get("result").cloned().unwrap_or(Value::Null),
                            ),
                        },
                        is_error: error.is_some() || s("status") == "failed",
                    });
                }
            }
            "web_search" => {
                let query = s("query").to_string();
                self.call(
                    &id,
                    "web_search",
                    serde_json::json!({ "query": query }),
                    out,
                );
                if done {
                    out.push(AgentEvent::ToolResult {
                        id: ToolId::new(id.as_str()),
                        output: ToolOutput::Text(String::new()),
                        is_error: false,
                    });
                }
            }
            "todo_list" => {
                let items = item.get("items").cloned().unwrap_or(Value::Null);
                let n = items.as_array().map_or(0, Vec::len);
                if self.tools.insert(id.clone()) {
                    out.push(AgentEvent::ToolCall {
                        id: ToolId::new(id.as_str()),
                        name: "TodoWrite".into(),
                        input: serde_json::json!({ "todos": items }),
                        title: format!("{n} items"),
                    });
                }
                if done {
                    out.push(AgentEvent::ToolResult {
                        id: ToolId::new(id.as_str()),
                        output: ToolOutput::Text(String::new()),
                        is_error: false,
                    });
                }
            }
            // A non-fatal notice ("model metadata not found"): not part of
            // the reply.
            "error" => tracing::debug!(message = s("message"), "codex notice"),
            other => tracing::debug!(kind = other, "unhandled codex item"),
        }
    }

    fn call(&mut self, id: &str, name: &str, input: Value, out: &mut Vec<AgentEvent>) {
        if self.tools.insert(id.to_string()) {
            self.join.break_();
            out.push(AgentEvent::ToolCall {
                id: ToolId::new(id),
                title: tool_title(name, &input),
                name: name.into(),
                input,
            });
        }
    }
}

impl Decode for CodexDecoder {
    fn decode_line(&mut self, line: &str) -> Vec<AgentEvent> {
        let Some(v) = json_line(line) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        match v.get("type").and_then(Value::as_str).unwrap_or("") {
            "thread.started" => {
                if let Some(id) = v.get("thread_id").and_then(Value::as_str) {
                    out.push(AgentEvent::Started {
                        native_session_id: Some(id.to_string()),
                        model: None,
                    });
                }
            }
            "item.started" | "item.updated" => {
                if let Some(item) = v.get("item") {
                    self.item(item, false, &mut out);
                }
            }
            "item.completed" => {
                if let Some(item) = v.get("item") {
                    self.item(item, true, &mut out);
                }
            }
            "turn.completed" => {
                if let Some(u) = v.get("usage") {
                    let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
                    // OpenAI counts cached tokens inside `input_tokens`.
                    let cached = n("cached_input_tokens");
                    out.push(AgentEvent::Usage(Usage {
                        input: n("input_tokens").saturating_sub(cached),
                        output: n("output_tokens"),
                        cache_read: cached,
                        cache_write: n("cache_write_input_tokens"),
                        cost_usd: None,
                        ..Default::default()
                    }));
                }
            }
            "turn.failed" => {
                let msg = v
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| self.last_error.clone())
                    .unwrap_or_else(|| "codex reported a failed turn".into());
                self.turn.fail(&msg);
            }
            "error" => {
                self.last_error = v.get("message").and_then(Value::as_str).map(str::to_string);
            }
            _ => {}
        }
        self.turn.note(&out);
        out
    }

    fn end(&mut self, exit: &Exit) -> Vec<AgentEvent> {
        if exit.code != Some(0) && !exit.interrupted {
            if let Some(e) = self.last_error.take() {
                self.turn.fail(&e);
            }
        }
        self.seen.clear();
        self.output.clear();
        self.tools.clear();
        self.join = Joiner::default();
        self.last_error = None;
        self.turn.conclude(exit, Some(self.spec))
    }
}

/// Decoder for `gemini --output-format stream-json`.
///
/// The schema is gemini-cli 0.53.1's `StreamJsonFormatter`; these lines were
/// captured from it:
/// ```text
/// {"type":"init","timestamp":"…","session_id":"cfd4f06c-…","model":"auto"}
/// {"type":"message","timestamp":"…","role":"user","content":"Reply with the word ok."}
/// {"type":"result","timestamp":"…","status":"error","error":{"type":"unknown","message":"[API Error: …]"},"stats":{…}}
/// ```
/// Replies come as `{"type":"message","role":"assistant","content":"…","delta":true}`,
/// tools as `tool_use` {tool_name, tool_id, parameters} and `tool_result`
/// {tool_id, status, output, error}.
pub struct GeminiDecoder {
    spec: &'static CliSpec,
    tools: HashMap<String, String>,
    turn: Turn,
}

impl GeminiDecoder {
    pub fn new(spec: &'static CliSpec) -> Self {
        Self {
            spec,
            tools: HashMap::new(),
            turn: Turn::default(),
        }
    }
}

impl Decode for GeminiDecoder {
    fn decode_line(&mut self, line: &str) -> Vec<AgentEvent> {
        let Some(v) = json_line(line) else {
            return Vec::new();
        };
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("");
        let mut out = Vec::new();
        match s("type") {
            "init" => out.push(AgentEvent::Started {
                native_session_id: Some(s("session_id").to_string()).filter(|s| !s.is_empty()),
                model: Some(s("model").into()).filter(|m: &smol_str::SmolStr| !m.is_empty()),
            }),
            // The prompt is echoed back as a user message.
            "message" if s("role") == "assistant" => {
                let delta = s("content");
                if !delta.is_empty() {
                    out.push(AgentEvent::Text {
                        delta: delta.to_string(),
                    });
                }
            }
            "tool_use" => {
                let id = s("tool_id").to_string();
                let name = s("tool_name").to_string();
                let input = v.get("parameters").cloned().unwrap_or(Value::Null);
                self.tools.insert(id.clone(), name.clone());
                out.push(AgentEvent::ToolCall {
                    id: ToolId::new(id.as_str()),
                    title: tool_title(&name, &input),
                    name: name.into(),
                    input,
                });
            }
            "tool_result" => {
                let failed = s("status") == "error";
                let text = if failed {
                    v.pointer("/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or(s("output"))
                } else {
                    s("output")
                };
                out.push(AgentEvent::ToolResult {
                    id: ToolId::new(s("tool_id")),
                    output: ToolOutput::Terminal {
                        text: text.to_string(),
                        exit_code: None,
                    },
                    is_error: failed,
                });
            }
            "error" => {
                // Warnings (a loop detected, a blocked tool) let the turn go
                // on; a `result` with status "error" follows a real failure.
                if s("severity") == "error" {
                    self.turn.fail(s("message"));
                } else {
                    tracing::debug!(message = s("message"), "gemini warning");
                }
            }
            "result" => {
                if let Some(st) = v.get("stats") {
                    let n = |k: &str| st.get(k).and_then(Value::as_u64);
                    let cached = n("cached").unwrap_or(0);
                    let input = n("input")
                        .unwrap_or_else(|| n("input_tokens").unwrap_or(0).saturating_sub(cached));
                    if n("total_tokens").unwrap_or(0) > 0 {
                        out.push(AgentEvent::Usage(Usage {
                            input,
                            output: n("output_tokens").unwrap_or(0),
                            cache_read: cached,
                            cache_write: 0,
                            cost_usd: None,
                            ..Default::default()
                        }));
                    }
                }
                if s("status") == "error" {
                    let msg = v
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or("gemini reported an error");
                    // The detailed reason beats the generic one before it.
                    self.turn.failure = None;
                    self.turn.fail(msg);
                }
            }
            _ => {}
        }
        self.turn.note(&out);
        out
    }

    fn end(&mut self, exit: &Exit) -> Vec<AgentEvent> {
        self.tools.clear();
        self.turn.conclude(exit, Some(self.spec))
    }
}

/// Decoder for `grok --output-format streaming-json`.
///
/// The schema is the one documented inside grok 0.2.103 itself:
/// ```text
/// {"type":"text","data":"Here's"}
/// {"type":"thought","data":"Analyzing the directory structure..."}
/// {"type":"end","stopReason":"EndTurn","sessionId":"abc123","requestId":"xyz789","usage":{…}}
/// {"type":"error","message":"Couldn't start session: ..."}
/// ```
/// Tool calls are not part of this stream. A line that is not JSON is passed
/// through as reply text, so if the format ever changes the reply still shows.
pub struct GrokDecoder {
    spec: &'static CliSpec,
    plain: PlainDecoder,
    turn: Turn,
}

impl GrokDecoder {
    pub fn new(spec: &'static CliSpec) -> Self {
        Self {
            spec,
            plain: PlainDecoder::new(),
            turn: Turn::default(),
        }
    }
}

impl Decode for GrokDecoder {
    fn decode_line(&mut self, line: &str) -> Vec<AgentEvent> {
        let Some(v) = json_line(line) else {
            let out = if line.trim_start().starts_with('{') {
                Vec::new()
            } else {
                self.plain.decode_line(line)
            };
            self.turn.note(&out);
            return out;
        };
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("");
        let mut out = Vec::new();
        match s("type") {
            "text" if !s("data").is_empty() => out.push(AgentEvent::Text {
                delta: s("data").to_string(),
            }),
            "thought" if !s("data").is_empty() => out.push(AgentEvent::Reasoning {
                delta: s("data").to_string(),
            }),
            "end" => {
                if !s("sessionId").is_empty() {
                    out.push(AgentEvent::Started {
                        native_session_id: Some(s("sessionId").to_string()),
                        model: None,
                    });
                }
                if let Some(u) = v.get("usage") {
                    let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
                    out.push(AgentEvent::Usage(Usage {
                        input: n("input_tokens"),
                        output: n("output_tokens"),
                        cache_read: n("cache_read_input_tokens"),
                        cache_write: 0,
                        cost_usd: v.get("total_cost_usd").and_then(Value::as_f64),
                        ..Default::default()
                    }));
                }
                self.turn.stop = Some(match s("stopReason") {
                    "MaxTurns" | "MaxTurnRequests" | "max_turns" => StopReason::MaxTurns,
                    "MaxTokens" | "max_tokens" => StopReason::MaxTokens,
                    "Cancelled" | "cancelled" => StopReason::Interrupted,
                    "Refusal" | "refusal" => StopReason::Refused,
                    _ => StopReason::EndTurn,
                });
            }
            "max_turns_reached" => self.turn.stop = Some(StopReason::MaxTurns),
            "error" => self.turn.fail(s("message")),
            _ => {}
        }
        self.turn.note(&out);
        out
    }

    fn end(&mut self, exit: &Exit) -> Vec<AgentEvent> {
        self.plain = PlainDecoder::new();
        self.turn.conclude(exit, Some(self.spec))
    }
}

fn json_line(line: &str) -> Option<Value> {
    let line = line.trim();
    if !line.starts_with('{') {
        return None;
    }
    match serde_json::from_str::<Value>(line) {
        Ok(v) => Some(v),
        Err(_) => {
            tracing::debug!(line = %clip(line, 160), "undecodable agent line");
            None
        }
    }
}

pub(crate) fn tool_title(name: &str, input: &Value) -> String {
    let s = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("");
    let first = |keys: &[&str]| {
        keys.iter()
            .map(|k| s(k))
            .find(|v| !v.is_empty())
            .unwrap_or("")
            .to_string()
    };
    let pick = match name.to_ascii_lowercase().as_str() {
        "bash" | "shell" | "run_shell_command" => shell_command(s("command")),
        "read" | "write" | "edit" | "read_file" | "write_file" | "replace" | "apply_patch" => {
            first(&["filePath", "file_path", "absolute_path", "path"])
        }
        "grep" | "glob" | "search_file_content" => first(&["pattern", "query"]),
        "list_directory" | "ls" | "list" => first(&["path", "dir_path"]),
        "webfetch" | "web_fetch" => first(&["url", "prompt"]),
        "websearch" | "web_search" | "google_web_search" => first(&["query"]),
        "task" => first(&["description", "prompt"]),
        _ => first(&[
            "command",
            "filePath",
            "file_path",
            "path",
            "pattern",
            "query",
        ]),
    };
    pick.chars().take(160).collect()
}

/// `/bin/zsh -lc 'ls -la'` → `ls -la`: the wrapper codex runs every command
/// through is not what the model asked for.
fn shell_command(cmd: &str) -> String {
    for wrapper in [
        "/bin/zsh -lc ",
        "/bin/bash -lc ",
        "bash -lc ",
        "zsh -lc ",
        "/bin/sh -c ",
    ] {
        if let Some(rest) = cmd.strip_prefix(wrapper) {
            let rest = rest.trim();
            for q in ['\'', '"'] {
                if let Some(inner) = rest.strip_prefix(q).and_then(|r| r.strip_suffix(q)) {
                    return inner.to_string();
                }
            }
            return rest.to_string();
        }
    }
    cmd.to_string()
}

/// Decoder for CLIs we drive in plain-text mode.
///
/// Everything on stdout is the reply. ANSI escapes are stripped, because several
/// of these colour their output even when not attached to a TTY, and raw escape
/// bytes in a chat bubble look like corruption.
///
/// Except a failure printed there: vibe's preview TUI ends a failed run with
/// `Error: <why>` on stdout, not stderr. From a line starting `Error: ` on,
/// output is held until the exit says which it was: the reply after a clean
/// exit, the failure after any other.
#[derive(Default)]
pub struct PlainDecoder {
    seen_any: bool,
    held: Vec<String>,
    turn: Turn,
    spec: Option<&'static CliSpec>,
}

impl PlainDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    fn for_spec(spec: &'static CliSpec) -> Self {
        Self {
            spec: Some(spec),
            ..Self::default()
        }
    }

    pub fn decode_line(&mut self, line: &str) -> Vec<AgentEvent> {
        let clean = strip_ansi(line);
        // Leading blank lines are banner padding, not content.
        if !self.seen_any && clean.trim().is_empty() {
            return Vec::new();
        }
        self.seen_any = true;
        if !self.held.is_empty() || clean.starts_with("Error: ") {
            self.held.push(clean);
            return Vec::new();
        }
        let out = vec![AgentEvent::Text {
            delta: format!("{clean}\n"),
        }];
        self.turn.note(&out);
        out
    }

    pub fn finish(&mut self) -> Vec<AgentEvent> {
        self.end(&Exit::clean())
    }
}

impl Decode for PlainDecoder {
    fn decode_line(&mut self, line: &str) -> Vec<AgentEvent> {
        PlainDecoder::decode_line(self, line)
    }

    fn end(&mut self, exit: &Exit) -> Vec<AgentEvent> {
        self.seen_any = false;
        let held = std::mem::take(&mut self.held);
        let mut out = Vec::new();
        if !held.is_empty() {
            if exit.code == Some(0) || exit.interrupted {
                out = held
                    .into_iter()
                    .map(|l| AgentEvent::Text {
                        delta: format!("{l}\n"),
                    })
                    .collect();
                self.turn.note(&out);
            } else {
                let said = held.join(" ");
                self.turn
                    .fail(said.strip_prefix("Error: ").unwrap_or(&said));
            }
        }
        out.extend(self.turn.conclude(exit, self.spec));
        out
    }
}

/// Remove CSI/OSC escape sequences.
pub fn strip_ansi(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == 0x1b {
            match b.get(i + 1) {
                // CSI: ends at the first byte in 0x40..=0x7e
                Some(b'[') => {
                    i += 2;
                    while i < b.len() && !(0x40..=0x7e).contains(&b[i]) {
                        i += 1;
                    }
                    i += 1;
                }
                // OSC: ends at BEL or ST
                Some(b']') => {
                    i += 2;
                    while i < b.len() && b[i] != 0x07 {
                        if b[i] == 0x1b && b.get(i + 1) == Some(&b'\\') {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                    i += 1;
                }
                _ => i += 2,
            }
            continue;
        }
        // Copy one whole char so multi-byte sequences survive.
        let ch_len = s[i..].chars().next().map(char::len_utf8).unwrap_or(1);
        out.push_str(&s[i..i + ch_len]);
        i += ch_len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec;

    fn text_of(events: &[AgentEvent]) -> String {
        events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Text { delta } => Some(delta.as_str()),
                _ => None,
            })
            .collect()
    }

    fn run(d: &mut dyn Decode, lines: &[&str], exit: Exit) -> Vec<AgentEvent> {
        let mut all = Vec::new();
        for l in lines {
            all.extend(d.decode_line(l));
        }
        all.extend(d.end(&exit));
        all
    }

    fn failed(code: i32, stderr: &[&str]) -> Exit {
        Exit {
            code: Some(code),
            interrupted: false,
            stderr: stderr.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn failure_of(events: &[AgentEvent]) -> Option<(Option<u16>, String)> {
        events.iter().find_map(|e| match e {
            AgentEvent::TurnFailed { status, message } => Some((*status, message.clone())),
            _ => None,
        })
    }

    fn stop_of(events: &[AgentEvent]) -> Option<StopReason> {
        events.iter().rev().find_map(|e| match e {
            AgentEvent::TurnEnd { stop } => Some(*stop),
            _ => None,
        })
    }

    /// Verbatim capture from `opencode run --format json "say hi"`.
    const REAL_STEP_START: &str = r#"{"type":"step_start","timestamp":1785475426548,"sessionID":"ses_0495f1f9bffesDc1ALe3Uyksmu","part":{"id":"prt_fb6a0e8f00013Dea2biNnVs0yE","messageID":"msg_fb6a0e1da001CKyXMxQWEHotVD","sessionID":"ses_0495f1f9bffesDc1ALe3Uyksmu","type":"step-start"}}"#;
    const REAL_TEXT: &str = r#"{"type":"text","timestamp":1785475427417,"sessionID":"ses_0495f1f9bffesDc1ALe3Uyksmu","part":{"id":"prt_fb6a0ec24001EPU2P6RaG9Uu3h","messageID":"msg_fb6a0e1da001CKyXMxQWEHotVD","sessionID":"ses_0495f1f9bffesDc1ALe3Uyksmu","type":"text","text":"Hi","time":{"start":1785475427364,"end":1785475427401}}}"#;
    const REAL_STEP_FINISH: &str = r#"{"type":"step_finish","timestamp":1785475427417,"sessionID":"ses_0495f1f9bffesDc1ALe3Uyksmu","part":{"id":"prt_fb6a0ec4e001zSKN3n7BGEHUMq","reason":"stop","messageID":"msg_fb6a0e1da001CKyXMxQWEHotVD","sessionID":"ses_0495f1f9bffesDc1ALe3Uyksmu","type":"step-finish","tokens":{"total":12843,"input":12828,"output":2,"reasoning":13,"cache":{"write":0,"read":0}},"cost":0}}"#;

    #[test]
    fn real_opencode_session_decodes_end_to_end() {
        let mut d = OpenCodeDecoder::new();
        let start = d.decode_line(REAL_STEP_START);
        assert!(matches!(
            start.first(),
            Some(AgentEvent::Started { native_session_id: Some(s), .. })
                if s == "ses_0495f1f9bffesDc1ALe3Uyksmu"
        ));
        assert_eq!(d.session_id(), Some("ses_0495f1f9bffesDc1ALe3Uyksmu"));

        assert_eq!(text_of(&d.decode_line(REAL_TEXT)), "Hi");

        let fin = d.decode_line(REAL_STEP_FINISH);
        match fin.first() {
            Some(AgentEvent::Usage(u)) => {
                assert_eq!((u.input, u.output), (12828, 2));
                assert_eq!(u.context, 12843);
                assert_eq!(u.cost_usd, Some(0.0));
            }
            other => panic!("expected usage, got {other:?}"),
        }

        assert!(matches!(
            d.finish().first(),
            Some(AgentEvent::TurnEnd {
                stop: StopReason::EndTurn
            })
        ));
    }

    #[test]
    fn repeated_part_snapshots_become_deltas_not_duplicates() {
        // The bug this guards: opencode re-sends the whole part as it grows.
        let mut d = OpenCodeDecoder::new();
        let mut all = Vec::new();
        for full in ["Hi", "Hi there", "Hi there!"] {
            let line = serde_json::json!({
                "type": "text",
                "sessionID": "ses_1",
                "part": {"id": "prt_1", "type": "text", "text": full}
            });
            all.extend(d.decode_line(&line.to_string()));
        }
        assert_eq!(text_of(&all), "Hi there!", "text must not be duplicated");
    }

    #[test]
    fn distinct_parts_are_separated_by_a_blank_line() {
        let mut d = OpenCodeDecoder::new();
        let mut all = Vec::new();
        for (id, t) in [("p1", "one"), ("p2", "two")] {
            let line = serde_json::json!({
                "type": "text", "sessionID": "s",
                "part": {"id": id, "type": "text", "text": t}
            });
            all.extend(d.decode_line(&line.to_string()));
        }
        assert_eq!(text_of(&all), "one\n\ntwo");
    }

    #[test]
    fn a_rewritten_part_is_re_emitted_whole() {
        let mut d = OpenCodeDecoder::new();
        let mk = |t: &str| {
            serde_json::json!({
                "type": "text", "sessionID": "s",
                "part": {"id": "p", "type": "text", "text": t}
            })
            .to_string()
        };
        d.decode_line(&mk("long original text"));
        // Shorter and not a prefix: treat as a replacement rather than silently
        // emitting nothing.
        let ev = d.decode_line(&mk("new"));
        assert_eq!(text_of(&ev), "new");
    }

    #[test]
    fn tool_events_emit_a_call_once_then_a_result() {
        let mut d = OpenCodeDecoder::new();
        let running = serde_json::json!({
            "type": "tool", "sessionID": "s",
            "part": {"id": "t1", "type": "tool", "tool": "bash",
                     "state": {"status": "running", "input": {"command": "ls -la"}}}
        });
        let ev = d.decode_line(&running.to_string());
        // `Started` is emitted first in this same batch, so search rather than
        // assuming a position.
        match ev.iter().find(|e| matches!(e, AgentEvent::ToolCall { .. })) {
            Some(AgentEvent::ToolCall { name, title, .. }) => {
                assert_eq!(name, "bash");
                assert_eq!(title, "ls -la");
            }
            other => panic!("{other:?}"),
        }

        let done = serde_json::json!({
            "type": "tool", "sessionID": "s",
            "part": {"id": "t1", "type": "tool", "tool": "bash",
                     "state": {"status": "completed", "input": {"command": "ls -la"},
                               "output": "a\nb\n"}}
        });
        let ev = d.decode_line(&done.to_string());
        assert!(
            !ev.iter().any(|e| matches!(e, AgentEvent::ToolCall { .. })),
            "the call must not be emitted twice"
        );
        match ev
            .iter()
            .find(|e| matches!(e, AgentEvent::ToolResult { .. }))
        {
            Some(AgentEvent::ToolResult {
                output, is_error, ..
            }) => {
                assert!(!is_error);
                assert!(matches!(output, ToolOutput::Terminal { text, .. } if text == "a\nb\n"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_opencode_error_event_fails_the_turn_with_its_message() {
        // `run` prints `{"type":"error","error":<the session error>}`, whose
        // words are in `error.data.message`; this used to fall back to a
        // generic "opencode reported an error" and kill the session.
        let mut d = OpenCodeDecoder::for_spec(&spec::OPENCODE);
        let line = r#"{"type":"error","timestamp":1,"sessionID":"ses_1","error":{"name":"APIError","data":{"message":"Rate limit exceeded","statusCode":429}}}"#;
        let ev = run(&mut d, &[line], failed(1, &[]));
        assert_eq!(
            failure_of(&ev),
            Some((Some(429), "Rate limit exceeded".to_string()))
        );
        assert!(!ev.iter().any(|e| matches!(e, AgentEvent::Fatal { .. })));
        assert_eq!(stop_of(&ev), Some(StopReason::Error));
    }

    #[test]
    fn garbage_and_banner_lines_are_ignored() {
        let mut d = OpenCodeDecoder::new();
        assert!(d.decode_line("█▀▀█ █▀▀█").is_empty());
        assert!(d.decode_line("").is_empty());
        assert!(d.decode_line("not json").is_empty());
        assert!(d.decode_line(r#"{"type":"brand_new"}"#).is_empty());
        // Still functional afterwards.
        assert_eq!(text_of(&d.decode_line(REAL_TEXT)), "Hi");
    }

    // codex-cli 0.146.0, `codex exec --json … "Run the shell command ls…"`.
    const CODEX_TOOL_RUN: &[&str] = &[
        r#"{"type":"thread.started","thread_id":"01a14317-28ae-7d32-b04b-8b3a1a31c050"}"#,
        r#"{"type":"turn.started"}"#,
        r#"{"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"/bin/zsh -lc ls","aggregated_output":"","exit_code":null,"status":"in_progress"}}"#,
        r#"{"type":"item.completed","item":{"id":"item_0","type":"command_execution","command":"/bin/zsh -lc ls","aggregated_output":"a.txt\nb.rs\n","exit_code":0,"status":"completed"}}"#,
        r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"There are 2 files: `a.txt` and `b.rs`."}}"#,
        r#"{"type":"turn.completed","usage":{"input_tokens":29189,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":149,"reasoning_output_tokens":38}}"#,
    ];

    #[test]
    fn real_codex_tool_turn_decodes_to_a_call_a_result_and_the_reply() {
        let mut d = CodexDecoder::new(&spec::CODEX);
        let ev = run(&mut d, CODEX_TOOL_RUN, Exit::clean());
        assert!(matches!(
            &ev[0],
            AgentEvent::Started { native_session_id: Some(id), .. }
                if id == "01a14317-28ae-7d32-b04b-8b3a1a31c050"
        ));
        match ev.iter().find(|e| matches!(e, AgentEvent::ToolCall { .. })) {
            Some(AgentEvent::ToolCall { name, title, .. }) => {
                assert_eq!(name, "bash");
                assert_eq!(title, "ls", "the zsh -lc wrapper is not the command");
            }
            other => panic!("{other:?}"),
        }
        match ev
            .iter()
            .find(|e| matches!(e, AgentEvent::ToolResult { .. }))
        {
            Some(AgentEvent::ToolResult {
                output: ToolOutput::Terminal { text, exit_code },
                is_error,
                ..
            }) => {
                assert_eq!(text, "a.txt\nb.rs\n");
                assert_eq!(*exit_code, Some(0));
                assert!(!is_error);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(text_of(&ev), "There are 2 files: `a.txt` and `b.rs`.");
        assert!(ev.iter().any(|e| matches!(
            e,
            AgentEvent::Usage(u) if u.input == 29189 && u.output == 149
        )));
        assert_eq!(stop_of(&ev), Some(StopReason::EndTurn));
    }

    #[test]
    fn codex_cached_tokens_are_not_counted_twice() {
        let mut d = CodexDecoder::new(&spec::CODEX);
        let ev = d.decode_line(r#"{"type":"turn.completed","usage":{"input_tokens":14502,"cached_input_tokens":6400,"cache_write_input_tokens":0,"output_tokens":5,"reasoning_output_tokens":0}}"#);
        match ev.first() {
            Some(AgentEvent::Usage(u)) => assert_eq!((u.input, u.cache_read), (8102, 6400)),
            other => panic!("{other:?}"),
        }
    }

    // The user's own config named a model their ChatGPT plan does not offer:
    // codex exits 1 and, in plain mode, said so only on stderr.
    const CODEX_BAD_MODEL: &[&str] = &[
        r#"{"type":"thread.started","thread_id":"01a14317-8213-78c1-b07b-a010aa5632f6"}"#,
        r#"{"type":"item.completed","item":{"id":"item_0","type":"error","message":"Model metadata for `gpt-5.5` not found. Defaulting to fallback metadata; this can degrade performance and cause issues."}}"#,
        r#"{"type":"turn.started"}"#,
        r#"{"type":"error","message":"{\"type\":\"error\",\"status\":400,\"error\":{\"type\":\"invalid_request_error\",\"message\":\"The 'gpt-5.5' model is not supported when using Codex with a ChatGPT account.\"}}"}"#,
        r#"{"type":"turn.failed","error":{"message":"{\"type\":\"error\",\"status\":400,\"error\":{\"type\":\"invalid_request_error\",\"message\":\"The 'gpt-5.5' model is not supported when using Codex with a ChatGPT account.\"}}"}}"#,
    ];

    #[test]
    fn a_failed_codex_turn_says_why_instead_of_ending_silently() {
        let mut d = CodexDecoder::new(&spec::CODEX);
        let ev = run(
            &mut d,
            CODEX_BAD_MODEL,
            failed(1, &["Reading additional input from stdin..."]),
        );
        let want = "Codex cannot use gpt-5.5 with this account: pick a model in the composer.";
        // A 400 is left out so the app reads these words, not "rejected".
        assert_eq!(failure_of(&ev), Some((None, want.to_string())));
        assert!(want.len() <= 80, "the app clips longer reasons");
        assert_eq!(text_of(&ev), want, "the reply shows the reason");
        assert_eq!(stop_of(&ev), Some(StopReason::Error));
    }

    #[test]
    fn a_codex_notice_item_is_not_part_of_the_reply() {
        let mut d = CodexDecoder::new(&spec::CODEX);
        assert!(text_of(&d.decode_line(CODEX_BAD_MODEL[1])).is_empty());
    }

    // gemini 0.53.1, `gemini --output-format stream-json …` with an invalid
    // GEMINI_API_KEY: the CLI exits 144 after these three lines.
    const GEMINI_BAD_KEY: &[&str] = &[
        r#"{"type":"init","timestamp":"2026-10-16T05:02:10.880Z","session_id":"cfd4f06c-8109-4663-a13f-e622f46dc7fa","model":"auto"}"#,
        r#"{"type":"message","timestamp":"2026-10-16T05:02:10.881Z","role":"user","content":"Reply with the word ok."}"#,
        r#"{"type":"result","timestamp":"2026-10-16T05:02:11.919Z","status":"error","error":{"type":"unknown","message":"[API Error: {\"error\":{\"message\":\"{\\n  \\\"error\\\": {\\n    \\\"code\\\": 400,\\n    \\\"message\\\": \\\"API key not valid. Please pass a valid API key.\\\",\\n    \\\"status\\\": \\\"INVALID_ARGUMENT\\\"\\n  }\\n}\\n\",\"code\":400,\"status\":\"Bad Request\"}}]"},"stats":{"total_tokens":0,"input_tokens":0,"output_tokens":0,"cached":0,"input":0,"duration_ms":0,"tool_calls":0,"models":{}}}"#,
    ];

    #[test]
    fn real_gemini_error_is_unwrapped_to_its_innermost_message() {
        let mut d = GeminiDecoder::new(&spec::GEMINI);
        let ev = run(&mut d, GEMINI_BAD_KEY, failed(144, &[]));
        assert!(matches!(
            &ev[0],
            AgentEvent::Started { native_session_id: Some(id), .. }
                if id == "cfd4f06c-8109-4663-a13f-e622f46dc7fa"
        ));
        assert_eq!(
            failure_of(&ev).unwrap().1,
            "API key not valid. Please pass a valid API key."
        );
        // The echoed prompt is not the reply; the error is.
        assert_eq!(
            text_of(&ev),
            "API key not valid. Please pass a valid API key."
        );
    }

    #[test]
    fn gemini_reply_tools_and_usage_stream_in_order() {
        // Shapes from gemini-cli 0.53.1's non-interactive loop.
        let lines = [
            r#"{"type":"init","timestamp":"t","session_id":"s1","model":"gemini-3-flash"}"#,
            r#"{"type":"message","timestamp":"t","role":"user","content":"list files"}"#,
            r#"{"type":"tool_use","timestamp":"t","tool_name":"run_shell_command","tool_id":"run_shell_command-1","parameters":{"command":"ls","description":"List files"}}"#,
            r#"{"type":"tool_result","timestamp":"t","tool_id":"run_shell_command-1","status":"success","output":"a.txt\nb.rs"}"#,
            r#"{"type":"message","timestamp":"t","role":"assistant","content":"Two ","delta":true}"#,
            r#"{"type":"message","timestamp":"t","role":"assistant","content":"files.","delta":true}"#,
            r#"{"type":"result","timestamp":"t","status":"success","stats":{"total_tokens":120,"input_tokens":100,"output_tokens":20,"cached":40,"input":60,"duration_ms":900,"tool_calls":1,"models":{}}}"#,
        ];
        let mut d = GeminiDecoder::new(&spec::GEMINI);
        let ev = run(&mut d, &lines, Exit::clean());
        match ev.iter().find(|e| matches!(e, AgentEvent::ToolCall { .. })) {
            Some(AgentEvent::ToolCall { title, .. }) => assert_eq!(title, "ls"),
            other => panic!("{other:?}"),
        }
        assert!(ev.iter().any(|e| matches!(
            e,
            AgentEvent::ToolResult {
                is_error: false,
                ..
            }
        )));
        assert_eq!(text_of(&ev), "Two files.");
        assert!(ev.iter().any(|e| matches!(
            e,
            AgentEvent::Usage(u) if u.input == 60 && u.cache_read == 40 && u.output == 20
        )));
        assert_eq!(stop_of(&ev), Some(StopReason::EndTurn));
    }

    #[test]
    fn gemini_without_sign_in_names_the_fix() {
        // Real: exit 41, nothing on stdout.
        let mut d = GeminiDecoder::new(&spec::GEMINI);
        let ev = run(
            &mut d,
            &[],
            failed(
                41,
                &[
                    "YOLO mode is enabled. All tool calls will be automatically approved.",
                    "Please set an Auth method in your /Users/nees/.gemini/settings.json or specify one of the following environment variables before running: GEMINI_API_KEY, GOOGLE_GENAI_USE_VERTEXAI, GOOGLE_GENAI_USE_GCA",
                ],
            ),
        );
        assert_eq!(
            failure_of(&ev).unwrap().1,
            "Gemini is not signed in: run `gemini` in a terminal."
        );
    }

    // grok 0.2.103 with no credentials: exit 1, and the same words on stdout
    // (as JSON) and twice on stderr.
    const GROK_SIGNED_OUT: &str = r#"{"type":"error","message":"Not signed in. To authenticate without a browser, run:\n  grok login --device-code\n\nAlternatively, set the XAI_API_KEY environment variable or run `grok login` on a machine with a browser."}"#;

    #[test]
    fn grok_signed_out_is_reported_with_the_login_command() {
        let mut d = GrokDecoder::new(&spec::GROK);
        let ev = run(
            &mut d,
            &[GROK_SIGNED_OUT],
            failed(
                1,
                &["Error: Not signed in. To authenticate without a browser, run:"],
            ),
        );
        let (status, message) = failure_of(&ev).unwrap();
        assert_eq!(status, None);
        assert_eq!(
            message,
            "Grok is not signed in: run `grok login` in a terminal."
        );
        assert!(message.len() < 80, "the app clips longer reasons");
        assert_eq!(stop_of(&ev), Some(StopReason::Error));
    }

    #[test]
    fn grok_api_errors_are_unwrapped() {
        // Real, with a bogus XAI_API_KEY.
        let line = r#"{"type":"error","message":"Internal error: {\n  \"message\": \"API error (status 400 Bad Request): invalid-argument: Incorrect API key provided. You can obtain an API key from https://console.x.ai.\\n\\nRequest URL: https://api.x.ai/v1/responses\",\n  \"http_status\": 400\n}"}"#;
        let mut d = GrokDecoder::new(&spec::GROK);
        let ev = run(&mut d, &[line], failed(1, &[]));
        let (_, message) = failure_of(&ev).unwrap();
        assert!(
            message.starts_with(
                "API error (status 400 Bad Request): invalid-argument: Incorrect API key provided."
            ),
            "{message}"
        );
    }

    #[test]
    fn grok_stream_decodes_text_thought_and_end() {
        // The example in grok 0.2.103's own docs for `streaming-json`.
        let lines = [
            r#"{"type":"text","data":"Here's"}"#,
            r#"{"type":"text","data":" a summary"}"#,
            r#"{"type":"thought","data":"Analyzing the directory structure..."}"#,
            r#"{"type":"end","stopReason":"EndTurn","sessionId":"abc123","requestId":"xyz789","usage":{"input_tokens":7210,"cache_read_input_tokens":41000,"output_tokens":1893},"total_cost_usd":0.01268905}"#,
        ];
        let mut d = GrokDecoder::new(&spec::GROK);
        let ev = run(&mut d, &lines, Exit::clean());
        assert_eq!(text_of(&ev), "Here's a summary");
        assert!(ev.iter().any(|e| matches!(e, AgentEvent::Reasoning { .. })));
        assert!(ev.iter().any(|e| matches!(
            e,
            AgentEvent::Started { native_session_id: Some(id), .. } if id == "abc123"
        )));
        assert!(ev.iter().any(|e| matches!(
            e,
            AgentEvent::Usage(u) if u.cache_read == 41000 && u.cost_usd == Some(0.01268905)
        )));
        assert_eq!(stop_of(&ev), Some(StopReason::EndTurn));
    }

    #[test]
    fn grok_text_that_is_not_json_still_shows() {
        let mut d = GrokDecoder::new(&spec::GROK);
        assert_eq!(text_of(&d.decode_line("plain reply")), "plain reply\n");
    }

    #[test]
    fn an_interrupted_turn_ends_as_interrupted_not_failed() {
        let mut d = CodexDecoder::new(&spec::CODEX);
        d.decode_line(CODEX_TOOL_RUN[0]);
        let ev = d.end(&Exit {
            code: None,
            interrupted: true,
            stderr: vec![],
        });
        assert!(failure_of(&ev).is_none());
        assert_eq!(stop_of(&ev), Some(StopReason::Interrupted));
    }

    #[test]
    fn a_crash_with_nothing_said_is_still_reported() {
        let mut d = PlainDecoder::for_spec(&spec::VIBE);
        let ev = d.end(&failed(2, &[]));
        assert_eq!(
            failure_of(&ev).unwrap().1,
            "Vibe exited with code 2 and said nothing."
        );
    }

    #[test]
    fn a_rejected_flag_is_reported_from_stderr() {
        // Real: vibe 2.14.1 given the old `--auto-approve`.
        let mut d = PlainDecoder::for_spec(&spec::VIBE);
        let ev = d.end(&failed(
            2,
            &[
                "usage: vibe [-h] [-v] [-p [TEXT]] [--max-turns N] [--max-price DOLLARS]",
                "vibe: error: unrecognized arguments: --auto-approve",
            ],
        ));
        assert!(failure_of(&ev)
            .unwrap()
            .1
            .ends_with("vibe: error: unrecognized arguments: --auto-approve"));
    }

    /// vibe 2.26.1's preview TUI, `vibe --output text … --prompt=…` with a
    /// key whose plan allows 0 requests a minute, once its retries were spent:
    /// exit 1, the failure on stdout and a notice on stderr.
    const VIBE_429_STDOUT: &str = r#"Error: API error occurred: Status 429. Body: {"object":"error","message":"Rate limit exceeded","type":"rate_limited","param":null,"code":"1300","raw_status_code":429}"#;
    const VIBE_PREVIEW_NOTICE: &str = "You are using the new Vibe TUI, currently in preview. Please report this error at https://github.com/mistralai/mistral-vibe/issues. To switch back to the classic TUI, set VIBE_CLI=python.";

    #[test]
    fn vibe_failure_on_stdout_fails_the_turn_instead_of_being_the_reply() {
        let mut d = PlainDecoder::for_spec(&spec::VIBE);
        let ev = run(
            &mut d,
            &[VIBE_429_STDOUT],
            failed(1, &[VIBE_PREVIEW_NOTICE]),
        );
        assert_eq!(
            failure_of(&ev),
            Some((Some(429), "Rate limit exceeded".into()))
        );
        assert_eq!(text_of(&ev), "Rate limit exceeded", "not the raw error");
        assert_eq!(stop_of(&ev), Some(StopReason::Error));
    }

    #[test]
    fn vibe_classic_failure_on_stderr_reads_the_same() {
        // VIBE_CLI=python: the same words, on stderr, exit 2.
        let mut d = PlainDecoder::for_spec(&spec::VIBE);
        let ev = run(&mut d, &[], failed(2, &[VIBE_429_STDOUT]));
        assert_eq!(failure_of(&ev).unwrap().1, "Rate limit exceeded");
    }

    #[test]
    fn vibe_without_an_api_key_names_the_fix() {
        // Real, with an empty VIBE_HOME: the classic CLI says why on stderr...
        let mut d = PlainDecoder::for_spec(&spec::VIBE);
        let ev = run(
            &mut d,
            &[],
            failed(1, &["Error: Missing MISTRAL_API_KEY environment variable for mistral provider. Set the environment variable (e.g. in ~/.vibe/.env or your shell), or run `vibe --setup` once interactively."]),
        );
        assert_eq!(
            failure_of(&ev).unwrap().1,
            "Vibe is not signed in: run `vibe --setup` in a terminal."
        );
        // ...the preview TUI only that it could not start.
        let ev = run(
            &mut d,
            &["Error: session/start"],
            failed(1, &[VIBE_PREVIEW_NOTICE]),
        );
        assert_eq!(
            failure_of(&ev).unwrap().1,
            "Vibe could not start a session. If it is not signed in, run `vibe --setup` in a terminal."
        );
    }

    #[test]
    fn a_reply_line_that_starts_with_error_is_still_the_reply() {
        let mut d = PlainDecoder::for_spec(&spec::VIBE);
        let ev = run(
            &mut d,
            &["The log says:", "Error: disk full", "so free some space."],
            Exit::clean(),
        );
        assert!(failure_of(&ev).is_none());
        assert_eq!(
            text_of(&ev),
            "The log says:\nError: disk full\nso free some space.\n"
        );
    }

    #[test]
    fn real_grok_1_0_stream_decodes() {
        // grok 1.0.60, `--output-format streaming-json --single=Reply with ok`,
        // with the tool list and the repeated thought deltas cut.
        let lines = [
            r#"{"type":"available_commands","tools":["run_terminal_command","read_file"]}"#,
            r#"{"type":"thought","data":"The user wants me to reply with \"ok\"."}"#,
            r#"{"type":"text","data":"ok"}"#,
            r#"{"type":"usage","usage":{"input_tokens":11883,"output_tokens":25,"cache_read_input_tokens":8576,"cache_creation_input_tokens":0,"reasoning_tokens":24},"signature":"yFZE"}"#,
            r#"{"type":"end","stopReason":"end_turn","sessionId":"01a14416-6906-70a1-bfd5-fceab713bc69","requestId":"b5b830ae-24c8-4dc9-9f22-e18de1f0a4ed","cwd":"/tmp/grok-probe","usage":{"input_tokens":11883,"cache_read_input_tokens":8576,"cache_creation_input_tokens":0,"output_tokens":25,"reasoning_tokens":24,"total_tokens":20484}}"#,
        ];
        let mut d = GrokDecoder::new(&spec::GROK);
        let ev = run(&mut d, &lines, Exit::clean());
        assert_eq!(text_of(&ev), "ok");
        assert!(ev.iter().any(|e| matches!(
            e,
            AgentEvent::Started { native_session_id: Some(id), .. }
                if id == "01a14416-6906-70a1-bfd5-fceab713bc69"
        )));
        assert_eq!(stop_of(&ev), Some(StopReason::EndTurn));
    }

    #[test]
    fn grok_unknown_model_is_reported_in_its_words() {
        // Real: `--model grok-nope`. Grok has no model list, so no picker to
        // point at.
        let line = r#"{"type":"error","message":"Couldn't set model 'grok-nope': Invalid params: \"unknown model id\". Run 'grok models' to see available models."}"#;
        let mut d = GrokDecoder::new(&spec::GROK);
        let ev = run(&mut d, &[line], failed(1, &[]));
        assert!(failure_of(&ev)
            .unwrap()
            .1
            .starts_with("Couldn't set model 'grok-nope'"));
    }

    #[test]
    fn a_clean_exit_with_only_chatter_on_stderr_is_not_a_failure() {
        let mut d = GeminiDecoder::new(&spec::GEMINI);
        d.decode_line(r#"{"type":"message","role":"assistant","content":"ok","delta":true}"#);
        let ev = d.end(&Exit {
            code: Some(0),
            interrupted: false,
            stderr: vec![
                "YOLO mode is enabled. All tool calls will be automatically approved.".into(),
            ],
        });
        assert!(failure_of(&ev).is_none());
        assert_eq!(stop_of(&ev), Some(StopReason::EndTurn));
    }

    #[test]
    fn retryable_statuses_are_kept_and_others_dropped() {
        let mut d = GrokDecoder::new(&spec::GROK);
        let ev = run(
            &mut d,
            &[r#"{"type":"error","message":"{\"message\":\"overloaded\",\"http_status\":529}"}"#],
            failed(1, &[]),
        );
        assert_eq!(failure_of(&ev), Some((Some(529), "overloaded".into())));
    }

    #[test]
    fn plain_decoder_passes_lines_through_and_drops_leading_blanks() {
        let mut d = PlainDecoder::new();
        assert!(d.decode_line("   ").is_empty(), "banner padding dropped");
        assert_eq!(text_of(&d.decode_line("hello")), "hello\n");
        // Blank lines *inside* the reply are content.
        assert_eq!(text_of(&d.decode_line("")), "\n");
    }

    #[test]
    fn plain_decoder_strips_ansi_colour() {
        let mut d = PlainDecoder::new();
        let ev = d.decode_line("\x1b[1;32mgreen\x1b[0m text");
        assert_eq!(text_of(&ev), "green text\n");
    }

    #[test]
    fn strip_ansi_handles_osc_and_preserves_unicode() {
        assert_eq!(strip_ansi("\x1b]0;title\x07body"), "body");
        assert_eq!(strip_ansi("héllo 🚀"), "héllo 🚀");
        assert_eq!(strip_ansi("\x1b[Kclean"), "clean");
        // Truncated escape at end of buffer must not panic.
        let _ = strip_ansi("text\x1b[");
        let _ = strip_ansi("text\x1b");
    }

    #[test]
    fn readable_error_leaves_plain_words_alone() {
        assert_eq!(
            readable_error("  just words  "),
            (None, "just words".into())
        );
        assert_eq!(
            readable_error("weird {not json} here"),
            (None, "weird {not json} here".into())
        );
    }
}
