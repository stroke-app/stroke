//! Line-oriented decoder: Claude Code stream-json -> [`AgentEvent`].
//!
//! Kept pure and stateful-but-synchronous so it is exhaustively testable
//! without spawning a process. All the protocol risk lives here.

use crate::wire::{self, BlockDelta, ContentBlock, ContentField, Line, RawEvent, WireUsage};
use agent_core::{Action, AgentEvent, Decision, RequestId, StopReason, ToolId, ToolOutput};
use serde_json::Value;
use std::collections::HashMap;

/// Hard cap on a single tool result we keep in memory. Beyond this the output is
/// reported as `Truncated`; unbounded tool output is the classic way this class
/// of app dies.
const TOOL_OUTPUT_CAP: usize = 256 * 1024;

pub struct Decoder {
    session_id: Option<String>,
    /// Tool name by tool_use id, so a `tool_result` can be rendered with the
    /// right renderer (the result line carries no tool name).
    tool_names: HashMap<String, String>,
    /// Partial `input_json_delta` accumulation, keyed by content-block index.
    partial_tools: HashMap<u32, PartialTool>,
    /// True once we have streamed actual text/thinking for the message currently
    /// in flight. The CLI sends *both* partial deltas and a final complete
    /// `assistant` message, so without this the text would emit twice.
    ///
    /// Scoped per-message and set only by content-bearing events: a contentless
    /// `stream_event` (`ping`, `message_start`) must not suppress a whole-message
    /// line, and a turn that streams one message then sends another unstreamed
    /// must still emit the second.
    streamed_content: bool,
    /// Our ids for the CLI's open permission requests, by its request id.
    asks: HashMap<String, RequestId>,
    /// Requests decoded since the last `take_asks`, for the session to answer.
    new_asks: Vec<(RequestId, PendingAsk)>,
    /// `total_cost_usd` as of the last result: the CLI keeps a running total
    /// for the process, and a turn's cost is what it grew by.
    cost_seen: f64,
}

/// What answering a permission request needs: the CLI's request id, the
/// tool input to echo back (possibly with answers added), and the "always
/// allow" rules it suggested.
#[derive(Debug, Clone)]
pub struct PendingAsk {
    pub request_id: String,
    pub input: Value,
    pub suggestions: Vec<Value>,
}

struct PartialTool {
    id: String,
    name: String,
    json: String,
    /// Already sent as a `ToolCall` before its input finished arriving.
    shown: bool,
}

/// How far into a streaming tool input to look for its title field. The
/// field comes first in practice; past this the card waits for the end.
const TITLE_SCAN_LIMIT: usize = 4096;

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            session_id: None,
            tool_names: HashMap::new(),
            partial_tools: HashMap::new(),
            streamed_content: false,
            asks: HashMap::new(),
            new_asks: Vec::new(),
            cost_seen: 0.,
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Decode one NDJSON line. Unparseable or unknown lines yield no events
    /// rather than failing the session.
    pub fn decode_line(&mut self, line: &str) -> Vec<AgentEvent> {
        let line = line.trim();
        if line.is_empty() {
            return Vec::new();
        }
        let parsed: Line = match serde_json::from_str(line) {
            Ok(l) => l,
            Err(e) => {
                tracing::debug!(error = %e, line = %truncate(line, 200), "undecodable stream-json line");
                return Vec::new();
            }
        };
        self.decode(parsed)
    }

    /// Permission requests decoded since the last call.
    pub fn take_asks(&mut self) -> Vec<(RequestId, PendingAsk)> {
        std::mem::take(&mut self.new_asks)
    }

    /// `can_use_tool`: a question for the user (AskUserQuestion) or an
    /// approval for a tool call.
    fn decode_control(&mut self, c: wire::ControlRequestLine, out: &mut Vec<AgentEvent>) {
        let r = &c.request;
        if r.get("subtype").and_then(Value::as_str) != Some("can_use_tool") {
            tracing::debug!(request = %r, "unhandled control request");
            return;
        }
        let tool = r
            .get("tool_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let input = r.get("input").cloned().unwrap_or(Value::Null);
        let tool_use = r
            .get("tool_use_id")
            .and_then(Value::as_str)
            .map(ToolId::new);
        let id = RequestId::new();
        self.asks.insert(c.request_id.clone(), id);
        self.new_asks.push((
            id,
            PendingAsk {
                request_id: c.request_id,
                input: input.clone(),
                suggestions: r
                    .get("permission_suggestions")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            },
        ));
        if tool == "AskUserQuestion" {
            out.push(AgentEvent::Question {
                id,
                tool_use,
                questions: questions(&input),
            });
            return;
        }
        let s = |k: &str| {
            input
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        let action = match tool.as_str() {
            "Bash" => Action::Bash {
                command: s("command"),
                cwd: None,
            },
            "Write" => Action::WriteFile {
                path: s("file_path"),
            },
            "Edit" | "MultiEdit" | "NotebookEdit" => Action::EditFile {
                path: if s("file_path").is_empty() {
                    s("notebook_path")
                } else {
                    s("file_path")
                },
            },
            "Read" => Action::ReadFile {
                path: s("file_path"),
            },
            "WebFetch" => Action::Fetch { url: s("url") },
            name if name.starts_with("mcp__") => {
                let mut parts = name.trim_start_matches("mcp__").splitn(2, "__");
                Action::McpCall {
                    server: parts.next().unwrap_or("").into(),
                    tool: parts.next().unwrap_or("").into(),
                    input: input.clone(),
                }
            }
            name => Action::Other {
                name: name.into(),
                input: input.clone(),
            },
        };
        let reason = r
            .get("description")
            .or_else(|| r.get("decision_reason"))
            .and_then(Value::as_str)
            .map(str::to_string);
        out.push(AgentEvent::Permission {
            id,
            action,
            suggestions: vec![Decision::Allow, Decision::AllowAlways, Decision::Deny],
            tool_use,
            input,
            reason,
        });
    }

    fn decode(&mut self, line: Line) -> Vec<AgentEvent> {
        let mut out = Vec::new();
        match line {
            Line::System(s) => {
                if let Some(id) = s.session_id {
                    let first = self.session_id.is_none();
                    self.session_id = Some(id.clone());
                    if first {
                        out.push(AgentEvent::Started {
                            native_session_id: Some(id),
                            model: s.model.map(Into::into),
                        });
                    }
                }
            }

            Line::StreamEvent(se) => {
                // A message's last word on its tokens. Only a reading of the
                // context: the turn's total comes with its result.
                if let RawEvent::MessageDelta { usage: Some(u) } = &se.event {
                    if se.parent_tool_use_id.is_none() {
                        out.extend(context_reading(u));
                    }
                }
                self.decode_raw(se.event, &mut out);
            }

            Line::Assistant(m) => {
                if m.parent_tool_use_id.is_none() {
                    out.extend(m.message.usage.as_ref().and_then(context_reading));
                }
                if let ContentField::Blocks(blocks) = m.message.content {
                    for b in blocks {
                        match b {
                            // Already streamed via partial deltas.
                            ContentBlock::Text { text } if !self.streamed_content => {
                                out.push(AgentEvent::Text { delta: text });
                            }
                            ContentBlock::Thinking { thinking } if !self.streamed_content => {
                                out.push(AgentEvent::Reasoning { delta: thinking });
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                // Record the name even when partials already
                                // emitted the call, so tool_result can find it.
                                let known = self.tool_names.insert(id.clone(), name.clone());
                                if known.is_none() {
                                    out.push(tool_call_event(&id, &name, input));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                // This message is complete; a following one streams afresh.
                self.streamed_content = false;
            }

            // Our own message read back: the model has it now.
            Line::User(m) if m.is_replay => {
                if let Some(id) = m.uuid {
                    out.push(AgentEvent::UserDelivered { id });
                }
            }

            Line::ControlRequest(c) => self.decode_control(c, &mut out),

            Line::ControlCancelRequest(c) => {
                if let Some(id) = self.asks.remove(&c.request_id) {
                    out.push(AgentEvent::AskCancelled { id });
                }
            }

            Line::CommandLifecycle(c) => {
                if let (Some(id), Some("cancelled")) = (c.command_uuid, c.state.as_deref()) {
                    out.push(AgentEvent::UserDropped { id });
                }
            }

            // Tool results come back as a synthetic `user` message.
            Line::User(m) => {
                if let ContentField::Blocks(blocks) = m.message.content {
                    for b in blocks {
                        if let ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            is_error,
                        } = b
                        {
                            let name = self
                                .tool_names
                                .get(&tool_use_id)
                                .map(String::as_str)
                                .unwrap_or("");
                            out.push(AgentEvent::ToolResult {
                                id: ToolId::new(tool_use_id.as_str()),
                                output: classify_output(name, &content),
                                is_error,
                            });
                        }
                    }
                }
            }

            Line::Result(r) => {
                let cost = r.total_cost_usd.map(|total| {
                    // Lower than before: a new running total began.
                    let turn = if total >= self.cost_seen {
                        total - self.cost_seen
                    } else {
                        total
                    };
                    self.cost_seen = total;
                    turn
                });
                let window = r
                    .model_usage
                    .values()
                    .filter_map(|m| m.context_window)
                    .max();
                if r.usage.is_some() || cost.is_some() || window.is_some() {
                    let mut u = r.usage.as_ref().map_or_else(
                        || agent_core::Usage {
                            cost_usd: cost,
                            ..Default::default()
                        },
                        |u| u.to_core(cost),
                    );
                    u.context_window = window;
                    out.push(AgentEvent::Usage(u));
                }
                if r.is_error {
                    out.push(AgentEvent::TurnFailed {
                        status: r.api_error_status,
                        message: r.result.clone().unwrap_or_default(),
                    });
                }
                let stop = match r.subtype.as_deref() {
                    Some("error_max_turns") => StopReason::MaxTurns,
                    Some("error_during_execution") => StopReason::Error,
                    _ if r.is_error => StopReason::Error,
                    _ => StopReason::EndTurn,
                };
                out.push(AgentEvent::TurnEnd { stop });
                // A new turn may reuse block indices and will stream fresh
                // partials; reset per-turn state but keep tool names, since
                // results can straddle the boundary.
                self.partial_tools.clear();
                self.streamed_content = false;
            }

            Line::Unknown => {}
        }
        out
    }

    fn decode_raw(&mut self, event: RawEvent, out: &mut Vec<AgentEvent>) {
        match event {
            RawEvent::ContentBlockStart {
                index,
                content_block,
            } => match content_block {
                ContentBlock::ToolUse { id, name, .. } => {
                    self.tool_names.insert(id.clone(), name.clone());
                    // A title that comes from the name alone is real already.
                    let shown = title_key(&name).is_none();
                    if shown {
                        out.push(tool_call_event(&id, &name, Value::Null));
                    }
                    self.partial_tools.insert(
                        index,
                        PartialTool {
                            id,
                            name,
                            json: String::new(),
                            shown,
                        },
                    );
                }
                ContentBlock::Text { text } if !text.is_empty() => {
                    self.streamed_content = true;
                    out.push(AgentEvent::Text { delta: text });
                }
                ContentBlock::Thinking { thinking } if !thinking.is_empty() => {
                    self.streamed_content = true;
                    out.push(AgentEvent::Reasoning { delta: thinking });
                }
                _ => {}
            },

            RawEvent::ContentBlockDelta { index, delta } => match delta {
                BlockDelta::TextDelta { text } => {
                    self.streamed_content = true;
                    out.push(AgentEvent::Text { delta: text });
                }
                BlockDelta::ThinkingDelta { thinking } => {
                    self.streamed_content = true;
                    out.push(AgentEvent::Reasoning { delta: thinking });
                }
                BlockDelta::InputJsonDelta { partial_json } => {
                    if let Some(p) = self.partial_tools.get_mut(&index) {
                        p.json.push_str(&partial_json);
                        // Show the card as soon as its title field has closed,
                        // so a long input (a whole file for Write) streams
                        // under a real title instead of nothing at all.
                        if !p.shown && p.json.len() <= TITLE_SCAN_LIMIT {
                            if let Some(k) = title_key(&p.name) {
                                if let Some(v) = closed_string_field(&p.json, k) {
                                    p.shown = true;
                                    let input = serde_json::json!({ k: v });
                                    out.push(tool_call_event(&p.id, &p.name, input));
                                }
                            }
                        }
                    }
                }
                BlockDelta::Unknown => {}
            },

            // The complete input. A call already shown gets it as a repeat
            // `ToolCall`, which the transcript applies to the same card.
            RawEvent::ContentBlockStop { index } => {
                if let Some(p) = self.partial_tools.remove(&index) {
                    let input = if p.json.trim().is_empty() {
                        Value::Null
                    } else {
                        serde_json::from_str(&p.json).unwrap_or(Value::Null)
                    };
                    out.push(tool_call_event(&p.id, &p.name, input));
                }
            }

            // Read for the context above; counting it here as well would
            // count every turn twice.
            RawEvent::MessageDelta { .. } => {}

            RawEvent::Unknown => {}
        }
    }
}

/// The context's size at one call, and nothing else.
fn context_reading(u: &WireUsage) -> Option<AgentEvent> {
    u.context().map(|context| {
        AgentEvent::Usage(agent_core::Usage {
            context,
            ..Default::default()
        })
    })
}

fn tool_call_event(id: &str, name: &str, input: Value) -> AgentEvent {
    AgentEvent::ToolCall {
        id: ToolId::new(id),
        name: name.into(),
        title: tool_title(name, &input),
        input,
    }
}

/// The input field a tool's card title is read from; `None` when the title
/// comes from the tool's name.
fn title_key(name: &str) -> Option<&'static str> {
    match name {
        "Bash" | "bash" => Some("command"),
        "Read" | "read" | "Write" | "write" | "Edit" | "edit" => Some("file_path"),
        "Glob" | "glob" | "Grep" | "grep" => Some("pattern"),
        "WebFetch" => Some("url"),
        "Task" => Some("description"),
        _ => None,
    }
}

/// Short label for the collapsed tool card, mirroring what the CLI shows.
fn tool_title(name: &str, input: &Value) -> String {
    match title_key(name) {
        Some(k) => {
            let s = input.get(k).and_then(Value::as_str).unwrap_or("");
            match k {
                "command" | "file_path" => truncate(s, 160).to_string(),
                _ => s.to_string(),
            }
        }
        // MCP tools are named `mcp__server__tool`; show `server/tool`.
        None => match name.strip_prefix("mcp__") {
            Some(rest) => rest.replacen("__", "/", 1),
            None => name.to_string(),
        },
    }
}

/// The string at top-level `key` of a JSON object still being streamed, once
/// its closing quote has arrived.
fn closed_string_field(json: &str, key: &str) -> Option<String> {
    let b = json.as_bytes();
    // Index of the quote closing the string opened at `open`, if it has come.
    let close = |open: usize| {
        let mut i = open + 1;
        while i < b.len() {
            match b[i] {
                b'\\' => i += 2,
                b'"' => return Some(i),
                _ => i += 1,
            }
        }
        None
    };
    let after_ws = |mut i: usize| {
        while b.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        i
    };
    let mut depth = 0usize;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth = depth.saturating_sub(1),
            b'"' => {
                let end = close(i)?;
                let colon = after_ws(end + 1);
                if depth == 1 && b.get(colon) == Some(&b':') && &json[i + 1..end] == key {
                    let open = after_ws(colon + 1);
                    if b.get(open) != Some(&b'"') {
                        return None;
                    }
                    let end = close(open)?;
                    return serde_json::from_str(&json[open..=end]).ok();
                }
                i = end;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Pick a renderer for a tool result based on which tool produced it.
fn classify_output(tool_name: &str, content: &Value) -> ToolOutput {
    let text = flatten_content(content);
    if text.len() > TOOL_OUTPUT_CAP {
        return ToolOutput::Truncated {
            preview: truncate(&text, 4096).to_string(),
            total_bytes: text.len() as u64,
            spill_path: None,
        };
    }
    match tool_name {
        "Bash" | "bash" => ToolOutput::Terminal {
            text,
            exit_code: None,
        },
        "Read" | "read" => ToolOutput::FileExcerpt {
            path: String::new(),
            text,
            start_line: 1,
        },
        "Edit" | "edit" | "Write" | "write" => ToolOutput::Text(text),
        _ if content.is_object() || content.is_array() && text.is_empty() => {
            ToolOutput::Structured(content.clone())
        }
        _ => ToolOutput::Text(text),
    }
}

/// `tool_result.content` is a string, or an array of `{type:"text",text}`
/// blocks, or an arbitrary object.
fn flatten_content(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => {
            let mut out = String::new();
            for item in items {
                if let Some(t) = item.get("text").and_then(Value::as_str) {
                    out.push_str(t);
                }
            }
            out
        }
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Truncate on a char boundary.
fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let end = s
        .char_indices()
        .take_while(|(i, _)| *i <= max)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
    &s[..end]
}

/// AskUserQuestion's input: `{questions: [{question, header, options:
/// [{label, description}], multiSelect}]}`.
fn questions(input: &Value) -> Vec<agent_core::Question> {
    let str_of = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    input
        .get("questions")
        .and_then(Value::as_array)
        .map(|qs| {
            qs.iter()
                .map(|q| agent_core::Question {
                    header: str_of(q, "header"),
                    question: str_of(q, "question"),
                    options: q
                        .get("options")
                        .and_then(Value::as_array)
                        .map(|os| {
                            os.iter()
                                .map(|o| agent_core::QuestionOption {
                                    label: str_of(o, "label"),
                                    description: str_of(o, "description"),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    multi: q
                        .get("multiSelect")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev_text(events: &[AgentEvent]) -> String {
        events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Text { delta } => Some(delta.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn init_line_yields_started_once() {
        let mut d = Decoder::new();
        let ev = d.decode_line(
            r#"{"type":"system","subtype":"init","session_id":"abc","model":"claude-opus-5"}"#,
        );
        assert!(matches!(
            ev.as_slice(),
            [AgentEvent::Started {
                native_session_id: Some(s),
                ..
            }] if s == "abc"
        ));
        assert_eq!(d.session_id(), Some("abc"));
        // A second system line must not re-announce the session.
        let ev = d.decode_line(r#"{"type":"system","subtype":"other","session_id":"abc"}"#);
        assert!(ev.is_empty());
    }

    #[test]
    fn whole_message_mode_emits_text() {
        let mut d = Decoder::new();
        let ev = d.decode_line(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Yes. It uses gpui 0.2.2"}]}}"#,
        );
        assert_eq!(ev_text(&ev), "Yes. It uses gpui 0.2.2");
    }

    #[test]
    fn partial_mode_does_not_double_emit_final_message() {
        let mut d = Decoder::new();
        let mut all = Vec::new();
        all.extend(d.decode_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hel"}}}"#,
        ));
        all.extend(d.decode_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lo"}}}"#,
        ));
        // The CLI now repeats the complete message.
        all.extend(d.decode_line(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello"}]}}"#,
        ));
        assert_eq!(ev_text(&all), "Hello", "text emitted exactly once");
    }

    fn input_delta(d: &mut Decoder, frag: &str) -> Vec<AgentEvent> {
        let line = serde_json::json!({
            "type": "stream_event",
            "event": {
                "type": "content_block_delta",
                "index": 1,
                "delta": {"type": "input_json_delta", "partial_json": frag}
            }
        });
        d.decode_line(&line.to_string())
    }

    #[test]
    fn streamed_tool_input_shows_the_card_once_its_title_has_arrived() {
        let mut d = Decoder::new();
        let start = d.decode_line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tu_1","name":"Write","input":{}}}}"#,
        );
        assert!(start.is_empty(), "no card before its title is known");
        assert!(input_delta(&mut d, r#"{"file_path": "/tmp/a"#).is_empty());

        // The path has closed; the file's content is still on its way.
        let early = input_delta(&mut d, r#".py", "content": "one\n"#);
        match early.as_slice() {
            [AgentEvent::ToolCall {
                id, title, input, ..
            }] => {
                assert_eq!(id.as_str(), "tu_1");
                assert_eq!(title, "/tmp/a.py");
                assert_eq!(input, &serde_json::json!({"file_path": "/tmp/a.py"}));
            }
            other => panic!("expected the card early, got {other:?}"),
        }
        assert!(input_delta(&mut d, r#"two"}"#).is_empty(), "shown once");

        // The end carries the whole input, for the same card.
        let end = d.decode_line(
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":1}}"#,
        );
        match end.as_slice() {
            [AgentEvent::ToolCall {
                id, title, input, ..
            }] => {
                assert_eq!(id.as_str(), "tu_1");
                assert_eq!(title, "/tmp/a.py");
                assert_eq!(input["content"], "one\ntwo");
            }
            other => panic!("expected the full input, got {other:?}"),
        }

        // The trailing complete assistant message must not add another call.
        let dup = d.decode_line(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tu_1","name":"Write","input":{"file_path":"/tmp/a.py","content":"one\ntwo"}}]}}"#,
        );
        assert!(
            !dup.iter().any(|e| matches!(e, AgentEvent::ToolCall { .. })),
            "tool call must not be emitted again"
        );
    }

    #[test]
    fn a_tool_titled_by_its_name_shows_at_once() {
        let mut d = Decoder::new();
        let start = d.decode_line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tu_2","name":"mcp__deck__deck_panes","input":{}}}}"#,
        );
        assert!(matches!(
            start.as_slice(),
            [AgentEvent::ToolCall { title, .. }] if title == "deck/deck_panes"
        ));
    }

    #[test]
    fn closed_string_field_reads_only_a_finished_top_level_key() {
        let f = closed_string_field;
        assert_eq!(f(r#"{"command": "ls"#, "command"), None, "still open");
        assert_eq!(f(r#"{"command""#, "command"), None, "no value yet");
        assert_eq!(
            f(r#"{"command": "echo \"hi\" \\", "#, "command").as_deref(),
            Some(r#"echo "hi" \"#),
            "escapes"
        );
        assert_eq!(
            f(r#"{"description": "command", "command": "ls"}"#, "command").as_deref(),
            Some("ls"),
            "a value equal to the key is not the key"
        );
        assert_eq!(
            f(r#"{"opts": {"pattern": "x"}, "pattern": "y"}"#, "pattern").as_deref(),
            Some("y"),
            "nested keys are skipped"
        );
        assert_eq!(f(r#"{"timeout": 5, "command": 3}"#, "command"), None);
    }

    #[test]
    fn tool_result_uses_renderer_of_originating_tool() {
        let mut d = Decoder::new();
        d.decode_line(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tu_9","name":"Bash","input":{"command":"ls"}}]}}"#,
        );
        let ev = d.decode_line(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tu_9","content":[{"type":"text","text":"a\nb\n"}]}]}}"#,
        );
        match &ev[0] {
            AgentEvent::ToolResult {
                id,
                output,
                is_error,
            } => {
                assert_eq!(id.as_str(), "tu_9");
                assert!(!is_error);
                assert!(
                    matches!(output, ToolOutput::Terminal { text, .. } if text == "a\nb\n"),
                    "bash output should render as a terminal, got {output:?}"
                );
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn string_content_tool_result_flattens() {
        let mut d = Decoder::new();
        let ev = d.decode_line(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"x","content":"plain","is_error":true}]}}"#,
        );
        match &ev[0] {
            AgentEvent::ToolResult {
                output, is_error, ..
            } => {
                assert!(is_error);
                assert!(matches!(output, ToolOutput::Text(t) if t == "plain"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn result_line_reports_usage_cost_and_turn_end() {
        let mut d = Decoder::new();
        let ev = d.decode_line(
            r#"{"type":"result","subtype":"success","total_cost_usd":0.0123,"usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":5}}"#,
        );
        match &ev[0] {
            AgentEvent::Usage(u) => {
                assert_eq!((u.input, u.output, u.cache_read), (10, 20, 5));
                assert_eq!(u.cost_usd, Some(0.0123));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(
            ev[1],
            AgentEvent::TurnEnd {
                stop: StopReason::EndTurn
            }
        ));
    }

    /// Every usage event in `lines`, folded as a transcript would.
    fn folded(d: &mut Decoder, lines: &[&str]) -> agent_core::Usage {
        let mut total = agent_core::Usage::default();
        for l in lines {
            for ev in d.decode_line(l) {
                if let AgentEvent::Usage(u) = ev {
                    total.accumulate(&u);
                }
            }
        }
        total
    }

    // Shapes from Claude Code 2.1.301, trimmed.
    const ASSISTANT: &str = r#"{"type":"assistant","message":{"content":[],"usage":{"input_tokens":2,"cache_creation_input_tokens":17068,"cache_read_input_tokens":11494,"output_tokens":4}},"parent_tool_use_id":null}"#;
    const DELTA: &str = r#"{"type":"stream_event","event":{"type":"message_delta","usage":{"input_tokens":2,"cache_creation_input_tokens":17068,"cache_read_input_tokens":11494,"output_tokens":9}},"parent_tool_use_id":null}"#;
    const RESULT_1: &str = r#"{"type":"result","subtype":"success","total_cost_usd":0.0052,"usage":{"input_tokens":2,"output_tokens":9,"cache_read_input_tokens":11494,"cache_creation_input_tokens":17068},"modelUsage":{"claude-haiku-5-5":{"inputTokens":2,"costUSD":0.0052,"contextWindow":1000000,"maxOutputTokens":128000}}}"#;
    const RESULT_2: &str = r#"{"type":"result","subtype":"success","total_cost_usd":0.0081,"usage":{"input_tokens":2,"output_tokens":3,"cache_read_input_tokens":14865,"cache_creation_input_tokens":13997},"modelUsage":{"claude-haiku-5-5":{"contextWindow":1000000}}}"#;

    #[test]
    fn a_turn_counts_once_and_reads_the_context() {
        let mut d = Decoder::new();
        let u = folded(&mut d, &[ASSISTANT, DELTA, RESULT_1]);
        // The message's own usage lines are readings; the result is the count.
        assert_eq!(
            (u.input, u.output, u.cache_read, u.cache_write),
            (2, 9, 11494, 17068)
        );
        assert_eq!(u.context, 2 + 17068 + 11494 + 9);
        assert_eq!(u.context_window, Some(1_000_000));
        assert_eq!(u.cost_usd, Some(0.0052));
    }

    #[test]
    fn cost_is_what_the_running_total_grew_by() {
        let mut d = Decoder::new();
        let u = folded(&mut d, &[RESULT_1, RESULT_2]);
        assert!(
            (u.cost_usd.unwrap() - 0.0081).abs() < 1e-9,
            "{:?}",
            u.cost_usd
        );
        assert_eq!(u.output, 12);
    }

    #[test]
    fn a_sub_agents_context_is_not_the_threads() {
        let mut d = Decoder::new();
        let sub = ASSISTANT.replace(
            r#""parent_tool_use_id":null"#,
            r#""parent_tool_use_id":"toolu_1""#,
        );
        let u = folded(&mut d, &[&sub]);
        assert_eq!(u.context, 0);
    }

    #[test]
    fn a_failed_result_says_its_status() {
        let mut d = Decoder::new();
        let ev = d.decode_line(
            r#"{"type":"result","subtype":"success","is_error":true,"api_error_status":529,"result":"API Error: overloaded"}"#,
        );
        assert!(matches!(
            &ev[0],
            AgentEvent::TurnFailed { status: Some(529), message } if message == "API Error: overloaded"
        ));
        assert!(matches!(
            ev.last(),
            Some(AgentEvent::TurnEnd {
                stop: StopReason::Error
            })
        ));
        let ok = d.decode_line(r#"{"type":"result","subtype":"success","api_error_status":null}"#);
        assert!(!ok
            .iter()
            .any(|e| matches!(e, AgentEvent::TurnFailed { .. })));
    }

    #[test]
    fn max_turns_result_maps_to_max_turns_stop() {
        let mut d = Decoder::new();
        let ev = d.decode_line(r#"{"type":"result","subtype":"error_max_turns","is_error":true}"#);
        assert!(matches!(
            ev.last(),
            Some(AgentEvent::TurnEnd {
                stop: StopReason::MaxTurns
            })
        ));
    }

    #[test]
    fn thinking_deltas_become_reasoning() {
        let mut d = Decoder::new();
        let ev = d.decode_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"hmm"}}}"#,
        );
        assert!(matches!(&ev[0], AgentEvent::Reasoning { delta } if delta == "hmm"));
    }

    #[test]
    fn contentless_stream_events_do_not_suppress_a_whole_message() {
        let mut d = Decoder::new();
        // A `ping` is a stream_event carrying no content; it must not make the
        // decoder believe the upcoming message was already streamed.
        assert!(d
            .decode_line(r#"{"type":"stream_event","event":{"type":"ping"}}"#)
            .is_empty());
        let ev = d.decode_line(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"ok"}]}}"#,
        );
        assert_eq!(ev_text(&ev), "ok");
    }

    #[test]
    fn second_unstreamed_message_in_one_turn_still_emits() {
        let mut d = Decoder::new();
        let mut all = Vec::new();
        // First message streams.
        all.extend(d.decode_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"first"}}}"#,
        ));
        all.extend(d.decode_line(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"first"}]}}"#,
        ));
        // Second arrives whole, with no partials.
        all.extend(d.decode_line(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":" second"}]}}"#,
        ));
        assert_eq!(ev_text(&all), "first second");
    }

    #[test]
    fn garbage_and_unknown_lines_are_ignored_not_fatal() {
        let mut d = Decoder::new();
        assert!(d.decode_line("not json at all").is_empty());
        assert!(d
            .decode_line(r#"{"type":"brand_new_event","x":1}"#)
            .is_empty());
        assert!(d
            .decode_line(r#"{"type":"stream_event","event":{"type":"ping"}}"#)
            .is_empty());
        assert!(d.decode_line("").is_empty());
        // Decoder still works afterwards.
        let ev = d.decode_line(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"ok"}]}}"#,
        );
        assert_eq!(ev_text(&ev), "ok");
    }

    #[test]
    fn oversized_tool_output_is_reported_as_truncated() {
        let mut d = Decoder::new();
        let big = "x".repeat(TOOL_OUTPUT_CAP + 10);
        let line = serde_json::json!({
            "type": "user",
            "message": {"content": [{"type":"tool_result","tool_use_id":"t","content": big}]}
        });
        let ev = d.decode_line(&line.to_string());
        match &ev[0] {
            AgentEvent::ToolResult { output, .. } => match output {
                ToolOutput::Truncated {
                    total_bytes,
                    preview,
                    ..
                } => {
                    assert_eq!(*total_bytes as usize, TOOL_OUTPUT_CAP + 10);
                    assert!(preview.len() <= 4100);
                }
                other => panic!("expected truncation, got {other:?}"),
            },
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn mcp_tool_titles_render_as_server_slash_tool() {
        assert_eq!(
            tool_title("mcp__azure__get_work_item", &Value::Null),
            "azure/get_work_item"
        );
    }

    #[test]
    fn truncate_respects_utf8_boundaries() {
        let s = "héllo wörld";
        for n in 0..s.len() + 2 {
            let _ = truncate(s, n); // must not panic
        }
    }

    #[test]
    fn our_message_read_back_mid_turn_is_reported_as_delivered() {
        let mut d = Decoder::new();
        let ev = d.decode_line(
            r#"{"type":"user","uuid":"22222222-2222-4222-8222-222222222222","isReplay":true,"parent_tool_use_id":null,"message":{"role":"user","content":[{"type":"text","text":"also 42"}]}}"#,
        );
        assert!(
            matches!(&ev[..], [AgentEvent::UserDelivered { id }] if id == "22222222-2222-4222-8222-222222222222")
        );
        // A tool result is still a tool result.
        let ev = d.decode_line(
            r#"{"type":"user","uuid":"x","message":{"content":[{"type":"tool_result","tool_use_id":"t","content":"ok"}]}}"#,
        );
        assert!(matches!(&ev[..], [AgentEvent::ToolResult { .. }]));
    }

    #[test]
    fn a_queued_message_the_cli_cancels_is_reported_as_dropped() {
        let mut d = Decoder::new();
        let ev = d.decode_line(
            r#"{"type":"command_lifecycle","command_uuid":"abc","state":"cancelled","uuid":"z"}"#,
        );
        assert!(matches!(&ev[..], [AgentEvent::UserDropped { id }] if id == "abc"));
        assert!(d
            .decode_line(r#"{"type":"command_lifecycle","command_uuid":"abc","state":"queued"}"#)
            .is_empty());
    }

    #[test]
    fn a_tool_waiting_for_approval_becomes_a_permission_request() {
        let mut d = Decoder::new();
        let ev = d.decode_line(
            r#"{"type":"control_request","request_id":"r-1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"touch a.txt"},"description":"Create a.txt","permission_suggestions":[{"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"touch a.txt"}],"behavior":"allow","destination":"localSettings"}],"tool_use_id":"tu_1"}}"#,
        );
        let [AgentEvent::Permission {
            id,
            action,
            tool_use,
            reason,
            ..
        }] = &ev[..]
        else {
            panic!("{ev:?}")
        };
        assert!(matches!(action, Action::Bash { command, .. } if command == "touch a.txt"));
        assert_eq!(tool_use.as_ref().map(|t| t.as_str()), Some("tu_1"));
        assert_eq!(reason.as_deref(), Some("Create a.txt"));
        let asks = d.take_asks();
        assert_eq!(asks.len(), 1);
        assert_eq!(
            (
                asks[0].0,
                asks[0].1.request_id.as_str(),
                asks[0].1.suggestions.len()
            ),
            (*id, "r-1", 1)
        );
        // Cancelled before it was answered.
        let ev = d.decode_line(r#"{"type":"control_cancel_request","request_id":"r-1"}"#);
        assert!(matches!(&ev[..], [AgentEvent::AskCancelled { id: c }] if c == id));
    }

    #[test]
    fn ask_user_question_becomes_a_question() {
        let mut d = Decoder::new();
        let ev = d.decode_line(
            r#"{"type":"control_request","request_id":"q-1","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","input":{"questions":[{"question":"Which color?","header":"Color","options":[{"label":"Red","description":"warm"},{"label":"Blue","description":"cool"}],"multiSelect":false}]},"tool_use_id":"tu_2","requires_user_interaction":true}}"#,
        );
        let [AgentEvent::Question { questions, .. }] = &ev[..] else {
            panic!("{ev:?}")
        };
        assert_eq!(questions[0].question, "Which color?");
        assert_eq!(
            questions[0]
                .options
                .iter()
                .map(|o| o.label.as_str())
                .collect::<Vec<_>>(),
            ["Red", "Blue"]
        );
        assert!(!questions[0].multi);
    }
}
