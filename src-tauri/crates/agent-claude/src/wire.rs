//! Serde types for Claude Code's `--output-format stream-json` protocol.
//!
//! Every enum has an `Unknown` catch-all: the CLI adds event types over time,
//! and an unrecognized line must be ignored, never fatal.

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Line {
    System(SystemLine),
    Assistant(MessageLine),
    User(MessageLine),
    Result(ResultLine),
    /// Emitted only with `--include-partial-messages`; wraps a raw Anthropic
    /// streaming event.
    StreamEvent(StreamEventLine),
    /// What became of a message sent on stdin: queued, started, completed,
    /// cancelled.
    CommandLifecycle(CommandLifecycleLine),
    /// The CLI asks us something and waits for a `control_response`: with
    /// `--permission-prompt-tool stdio`, whether a tool may run.
    ControlRequest(ControlRequestLine),
    /// A `control_request` that no longer needs an answer.
    ControlCancelRequest(ControlCancelLine),
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
pub struct SystemLine {
    #[serde(default)]
    pub subtype: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MessageLine {
    pub message: InnerMessage,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub uuid: Option<String>,
    /// A message we sent on stdin, echoed back as the model reads it
    /// (`--replay-user-messages`).
    #[serde(default, rename = "isReplay")]
    pub is_replay: bool,
    /// Set on a sub-agent's messages, whose context is its own.
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ControlRequestLine {
    pub request_id: String,
    #[serde(default)]
    pub request: Value,
}

#[derive(Debug, Deserialize)]
pub struct ControlCancelLine {
    pub request_id: String,
}

#[derive(Debug, Deserialize)]
pub struct CommandLifecycleLine {
    #[serde(default)]
    pub command_uuid: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InnerMessage {
    #[serde(default)]
    pub content: ContentField,
    #[serde(default)]
    pub usage: Option<WireUsage>,
    #[serde(default)]
    pub stop_reason: Option<String>,
}

/// `content` is a string on some user messages and an array of blocks on
/// assistant messages.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ContentField {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

impl Default for ContentField {
    fn default() -> Self {
        ContentField::Blocks(Vec::new())
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        #[serde(default)]
        text: String,
    },
    Thinking {
        #[serde(default)]
        thinking: String,
    },
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        #[serde(default)]
        content: Value,
        #[serde(default)]
        is_error: bool,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
pub struct ResultLine {
    #[serde(default)]
    pub subtype: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub total_cost_usd: Option<f64>,
    #[serde(default)]
    pub usage: Option<WireUsage>,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub result: Option<String>,
    /// The HTTP status of the API call that failed the turn, when one did.
    #[serde(default)]
    pub api_error_status: Option<u16>,
    /// Per model, since this process started.
    #[serde(default, rename = "modelUsage")]
    pub model_usage: HashMap<String, ModelUsage>,
}

#[derive(Debug, Deserialize)]
pub struct ModelUsage {
    #[serde(default, rename = "contextWindow")]
    pub context_window: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct StreamEventLine {
    pub event: RawEvent,
    #[serde(default)]
    pub parent_tool_use_id: Option<String>,
}

/// Raw Anthropic Messages-API streaming events, as forwarded by the CLI.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RawEvent {
    ContentBlockStart {
        index: u32,
        content_block: ContentBlock,
    },
    ContentBlockDelta {
        index: u32,
        delta: BlockDelta,
    },
    ContentBlockStop {
        index: u32,
    },
    MessageDelta {
        #[serde(default)]
        usage: Option<WireUsage>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockDelta {
    TextDelta {
        text: String,
    },
    ThinkingDelta {
        thinking: String,
    },
    /// Tool input arrives as incrementally-concatenated JSON fragments.
    InputJsonDelta {
        partial_json: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Default, Deserialize)]
pub struct WireUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
}

impl WireUsage {
    pub fn to_core(&self, cost_usd: Option<f64>) -> agent_core::Usage {
        agent_core::Usage {
            input: self.input_tokens,
            output: self.output_tokens,
            cache_read: self.cache_read_input_tokens,
            cache_write: self.cache_creation_input_tokens,
            cost_usd,
            ..Default::default()
        }
    }

    /// What the context held at this call: the whole prompt, cached or
    /// not, and the reply. `None` when only output was reported.
    pub fn context(&self) -> Option<u64> {
        let prompt =
            self.input_tokens + self.cache_read_input_tokens + self.cache_creation_input_tokens;
        (prompt > 0).then_some(prompt + self.output_tokens)
    }
}
