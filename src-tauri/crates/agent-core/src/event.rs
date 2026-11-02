use crate::ids::{RequestId, ToolId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use smol_str::SmolStr;

/// The normalized event stream every adapter emits.
///
/// This is the narrow waist of the whole app: adapters translate their native
/// protocol into these, and the UI, transcript, and persistence layers only
/// ever see these. Adding a variant here is a cross-cutting change — prefer
/// extending an existing payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    /// Backend accepted the session. `native_session_id` is a *hint* used to
    /// attempt lossless resume later; the canonical transcript is ours.
    Started {
        native_session_id: Option<String>,
        model: Option<SmolStr>,
    },

    /// Extended-thinking / reasoning delta. Rendered collapsed by default.
    Reasoning {
        delta: String,
    },

    /// Assistant-visible text delta.
    Text {
        delta: String,
    },

    /// A tool invocation began. `title` is a short human label the UI shows on
    /// the collapsed card (e.g. `ls vendor/gpui-component`). A backend may send
    /// it early with partial `input` and again with the same `id` once all of
    /// the input has arrived; the repeat updates the same call.
    ToolCall {
        id: ToolId,
        name: SmolStr,
        input: Value,
        title: String,
    },

    /// Incremental output from a still-running tool (bash stdout, etc.).
    /// Adapters that cannot stream tool output simply never emit this.
    ToolProgress {
        id: ToolId,
        chunk: String,
    },

    ToolResult {
        id: ToolId,
        output: ToolOutput,
        is_error: bool,
    },

    /// Backend is blocked waiting on the user to allow/deny an action.
    Permission {
        id: RequestId,
        action: Action,
        suggestions: Vec<Decision>,
        /// The tool call waiting on this, when the backend says which.
        tool_use: Option<ToolId>,
        /// The tool's full input, to show what exactly would happen.
        input: Value,
        /// The agent's own one-line account of the action, if it gave one.
        reason: Option<String>,
    },

    /// The agent asks the user to choose (Claude's AskUserQuestion). Answer
    /// with `AgentSession::answer`.
    Question {
        id: RequestId,
        tool_use: Option<ToolId>,
        questions: Vec<Question>,
    },

    /// A pending `Permission` or `Question` no longer needs an answer.
    AskCancelled {
        id: RequestId,
    },

    Usage(Usage),

    /// The turn failed: the API's HTTP status when the backend reports it,
    /// and what it said. Comes just before its `TurnEnd`.
    TurnFailed {
        status: Option<u16>,
        message: String,
    },

    TurnEnd {
        stop: StopReason,
    },

    /// Session is dead. The supervisor decides whether to restart.
    Fatal {
        message: String,
        retryable: bool,
    },

    /// A message sent while a turn ran (`UserTurn::id`) has reached the
    /// model: the backend queued it and has now read it.
    UserDelivered {
        id: String,
    },

    /// A message sent while a turn ran was dropped by the backend unread.
    UserDropped {
        id: String,
    },
}

/// Structured tool output so the UI can pick a renderer instead of dumping text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolOutput {
    /// Terminal-ish output, rendered through the alacritty grid (keeps ANSI).
    Terminal {
        text: String,
        exit_code: Option<i32>,
    },
    /// A file excerpt, rendered with syntax highlighting + jump-to-line.
    FileExcerpt {
        path: String,
        text: String,
        start_line: u32,
    },
    /// A unified diff, rendered as an inline diff view.
    Diff { path: String, unified: String },
    /// Structured payload (typically MCP), rendered by a typed renderer keyed
    /// on tool name with a JSON tree fallback.
    Structured(Value),
    /// Anything else.
    Text(String),
    /// Output exceeded the byte budget and was spilled to disk.
    Truncated {
        preview: String,
        total_bytes: u64,
        spill_path: Option<String>,
    },
}

impl ToolOutput {
    /// Cheap preview for the collapsed card. Never allocates the full body.
    pub fn preview(&self, max: usize) -> &str {
        let s = match self {
            ToolOutput::Terminal { text, .. } => text.as_str(),
            ToolOutput::FileExcerpt { text, .. } => text.as_str(),
            ToolOutput::Diff { unified, .. } => unified.as_str(),
            ToolOutput::Text(t) => t.as_str(),
            ToolOutput::Truncated { preview, .. } => preview.as_str(),
            ToolOutput::Structured(_) => return "{…}",
        };
        let end = s
            .char_indices()
            .take(max)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        &s[..end]
    }
}

/// One question in an `AgentEvent::Question`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Question {
    /// A short label for it ("Color").
    pub header: String,
    pub question: String,
    pub options: Vec<QuestionOption>,
    /// Several options may be picked.
    pub multi: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

/// An action the backend wants to take that our permission engine gates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    Bash {
        command: String,
        cwd: Option<String>,
    },
    WriteFile {
        path: String,
    },
    EditFile {
        path: String,
    },
    ReadFile {
        path: String,
    },
    Fetch {
        url: String,
    },
    McpCall {
        server: SmolStr,
        tool: SmolStr,
        input: Value,
    },
    Other {
        name: SmolStr,
        input: Value,
    },
}

impl Action {
    pub fn summary(&self) -> String {
        match self {
            Action::Bash { command, .. } => command.clone(),
            Action::WriteFile { path } => format!("write {path}"),
            Action::EditFile { path } => format!("edit {path}"),
            Action::ReadFile { path } => format!("read {path}"),
            Action::Fetch { url } => format!("fetch {url}"),
            Action::McpCall { server, tool, .. } => format!("{server}/{tool}"),
            Action::Other { name, .. } => name.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    /// Allow this one invocation.
    Allow,
    /// Allow and remember for the rest of the session.
    AllowAlways,
    /// Deny this one invocation; the agent may try something else.
    Deny,
    /// Deny and abort the turn.
    Abort,
}

// No `Eq`: `cost_usd` is an f64.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// Backends that report cost directly; otherwise the UI prices it locally.
    pub cost_usd: Option<f64>,
    /// Tokens the model's context held at its latest call. A reading, not a
    /// count: the newest wins, and 0 means none came.
    #[serde(default)]
    pub context: u64,
    /// How many tokens that context can hold, when the backend says.
    #[serde(default)]
    pub context_window: Option<u64>,
}

impl Usage {
    /// Adapters report tokens and cost per turn; this folds a turn into a
    /// session total. Readings (context and its window) replace the last.
    pub fn accumulate(&mut self, other: &Usage) {
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
        match (self.cost_usd, other.cost_usd) {
            (Some(a), Some(b)) => self.cost_usd = Some(a + b),
            (None, Some(b)) => self.cost_usd = Some(b),
            _ => {}
        }
        if other.context > 0 {
            self.context = other.context;
        }
        if other.context_window.is_some() {
            self.context_window = other.context_window;
        }
    }

    /// Tokens through the model, cached reads included.
    pub fn tokens(&self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_write
    }

    /// How full the context is, 0.0 to 1.0, when both sides are known.
    pub fn context_used(&self) -> Option<f32> {
        let window = self.context_window.filter(|w| *w > 0)?;
        (self.context > 0).then(|| (self.context as f32 / window as f32).min(1.))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    /// Agent finished its turn normally.
    EndTurn,
    /// User interrupted.
    Interrupted,
    /// Hit a token or turn limit.
    MaxTokens,
    MaxTurns,
    /// Permission denial aborted the turn.
    Refused,
    Error,
}

impl StopReason {
    pub fn is_clean(self) -> bool {
        matches!(self, StopReason::EndTurn)
    }
}
