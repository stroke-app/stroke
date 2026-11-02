use crate::event::{AgentEvent, Decision};
use crate::ids::{AgentId, RequestId, SessionId};
use crate::transcript::Transcript;
use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::path::PathBuf;
use std::sync::Arc;

/// What a backend can actually do. The UI reads this to disable affordances up
/// front rather than discovering failure mid-turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caps {
    /// Accepts an MCP server list natively (vs. needing our proxy tool).
    pub native_mcp: bool,
    /// Has a skills mechanism we can project into.
    pub native_skills: bool,
    /// Can resume its own session by id — enables lossless re-attach.
    pub native_resume: bool,
    /// Surfaces permission requests to us before acting.
    pub permission_hook: bool,
    /// Accepts image attachments.
    pub images: bool,
    /// Streams tool stdout incrementally.
    pub streaming_tools: bool,
    /// Reports token usage.
    pub usage: bool,
    /// Can be interrupted mid-turn without killing the process.
    pub soft_interrupt: bool,
    /// Takes messages while a turn runs, queues them itself and reads them
    /// at the next step (reported with `UserDelivered`), as Claude Code does.
    pub queues_input: bool,
    /// Emits structured events rather than being screen-scraped. A `false` here
    /// makes the UI mark the session as degraded.
    pub structured: bool,
}

impl Caps {
    /// Everything a first-class, protocol-native backend supports.
    pub const FULL: Caps = Caps {
        native_mcp: true,
        native_skills: true,
        native_resume: true,
        permission_hook: true,
        images: true,
        streaming_tools: true,
        usage: true,
        soft_interrupt: true,
        queues_input: true,
        structured: true,
    };

    /// Floor for a PTY-scraped backend.
    pub const DEGRADED: Caps = Caps {
        native_mcp: false,
        native_skills: false,
        native_resume: false,
        permission_hook: false,
        images: false,
        streaming_tools: true,
        usage: false,
        soft_interrupt: false,
        queues_input: false,
        structured: false,
    };
}

/// How faithfully a backend absorbed an existing transcript on handoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HydrateMode {
    /// Backend resumed its own native session — nothing was lost.
    NativeResume,
    /// Canonical transcript was replayed into a fresh session as context.
    Replayed,
    /// Transcript exceeded the context budget, so the oldest turns were dropped
    /// from the replay. Lossy in a way the user must be told about.
    ReplayedPartial { dropped_messages: usize },
    /// Transcript exceeded the context budget and was summarized first.
    Summarized,
    /// Fresh session with no prior context (first turn, or handoff declined).
    Fresh,
}

impl HydrateMode {
    pub fn is_lossless(self) -> bool {
        matches!(self, HydrateMode::NativeResume | HydrateMode::Fresh)
    }
}

/// Everything the harness resolved for one session, handed to the adapter at
/// spawn time. Adapters project the shared substrate into their own config
/// format from this — they never read global config themselves.
#[derive(Debug, Clone)]
pub struct SessionCtx {
    pub session_id: SessionId,
    pub cwd: PathBuf,
    /// Composed system prompt. Stable prefix first, volatile blocks last, so
    /// backends with prompt caching keep hitting cache.
    pub system_prompt: Arc<str>,
    pub model: Option<SmolStr>,
    /// MCP servers to expose, already resolved from the shared registry.
    pub mcp: Arc<Vec<McpServerSpec>>,
    /// Directory of shared skills to project in.
    pub skills_dir: Option<PathBuf>,
    /// Per-session scratch dir for generated config. Deleted on session close.
    pub runtime_dir: PathBuf,
    /// Native session id to resume, if we are re-attaching.
    pub resume: Option<String>,
    pub permission_mode: PermissionMode,
    /// Extra environment for the agent process: how a host app hands the
    /// agent credentials to call back into it (deck's `DECK_SOCKET`/`DECK_TOKEN`).
    pub env: Vec<(String, String)>,
    /// Tools pre-approved regardless of `permission_mode`, in the backend's own
    /// syntax (Claude: `Bash(deck:*)`). Lets a gated session still drive the host.
    pub allowed_tools: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionMode {
    /// Ask for everything not on the allowlist.
    Ask,
    /// Reads free, writes and commands prompted.
    Edit,
    /// Allowlist only; anything else auto-denied (for unattended runs).
    Auto,
    /// No prompting at all. Caller is responsible for sandboxing.
    Yolo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerSpec {
    pub name: SmolStr,
    pub transport: McpTransport,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpTransport {
    Stdio {
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
    Http {
        url: String,
        headers: Vec<(String, String)>,
    },
}

/// A backend the app can talk to. One instance per installed agent; it is a
/// factory, so it must stay cheap to hold and `Sync`.
#[async_trait]
pub trait AgentAdapter: Send + Sync + 'static {
    fn id(&self) -> AgentId;

    /// Human-facing name for the picker.
    fn display_name(&self) -> &str;

    fn caps(&self) -> Caps;

    /// Models this backend can be asked for, most capable first. Empty means
    /// the backend chooses and the picker is hidden rather than showing a
    /// single fake entry.
    fn models(&self) -> &'static [ModelSpec] {
        &[]
    }

    /// Whether the backend is installed and usable right now. Runs at startup
    /// to decide what appears in the agent picker.
    async fn probe(&self) -> Result<Availability, crate::AgentError>;

    async fn spawn(&self, ctx: SessionCtx) -> Result<Box<dyn AgentSession>, crate::AgentError>;

    /// How a user would install or sign in to this backend, if we know.
    ///
    /// Returned as a shell command rather than run for them: these commands
    /// install software and open browsers, so the user gets to read the thing
    /// before it happens. The UI puts it in the terminal with the cursor after
    /// it — one keystroke away, still their decision.
    fn setup(&self) -> Option<Setup> {
        None
    }
}

/// A command that makes an unavailable backend available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Setup {
    /// What the button says: "Install", "Sign in".
    pub verb: &'static str,
    pub command: &'static str,
    /// Where the command came from, so a user can check it before running it.
    pub source: &'static str,
}

/// One selectable model. `id` is what the backend expects on the wire; `label`
/// is what the picker shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelSpec {
    pub id: &'static str,
    pub label: &'static str,
    /// Short qualifier shown under the label, e.g. "fastest".
    pub note: &'static str,
}

#[derive(Debug, Clone)]
pub enum Availability {
    Ready {
        version: Option<String>,
    },
    /// Installed but unusable, e.g. not logged in.
    NeedsAuth {
        hint: String,
    },
    Missing {
        hint: String,
    },
}

impl Availability {
    pub fn is_ready(&self) -> bool {
        matches!(self, Availability::Ready { .. })
    }
}

/// One live conversation with one backend.
///
/// Implementations own a child process or HTTP connection. Dropping this must
/// terminate the backend — the supervisor relies on it for cleanup.
#[async_trait]
pub trait AgentSession: Send {
    /// Submit a user turn. Returns once the backend has accepted it; the
    /// response arrives on the event stream.
    async fn send(&mut self, turn: UserTurn) -> Result<(), crate::AgentError>;

    /// The event stream. Borrowing `&mut self` keeps a single consumer, which is
    /// what lets adapters own the decode buffer without locking.
    fn events(&mut self) -> BoxStream<'_, AgentEvent>;

    async fn interrupt(&mut self) -> Result<(), crate::AgentError>;

    /// Answer a pending `AgentEvent::Permission`.
    async fn decide(&mut self, id: RequestId, decision: Decision) -> Result<(), crate::AgentError>;

    /// Answer a pending `AgentEvent::Question`: (question, chosen answer)
    /// pairs, several answers to one question joined with ", ".
    async fn answer(
        &mut self,
        _id: RequestId,
        _answers: Vec<(String, String)>,
    ) -> Result<(), crate::AgentError> {
        Err(crate::AgentError::Unsupported(
            "this agent does not ask questions",
        ))
    }

    /// Seed this session with an existing canonical transcript. Called on
    /// mid-thread agent switches, before the first `send`.
    async fn hydrate(&mut self, transcript: &Transcript) -> Result<HydrateMode, crate::AgentError>;

    /// Native session id, once known, for later lossless resume. Returns an
    /// owned string because adapters learn it from a background reader task.
    fn native_session_id(&self) -> Option<String>;
}

#[derive(Debug, Clone, Default)]
pub struct UserTurn {
    pub text: String,
    pub attachments: Vec<Attachment>,
    /// Lets a backend that queues input report when it read this message.
    pub id: Option<String>,
    /// Sent mid-turn to be read now: the backend interrupts, or moves work
    /// that can run in the background aside, rather than waiting for the
    /// current step to finish.
    pub now: bool,
}

impl UserTurn {
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            text: s.into(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub path: PathBuf,
    pub mime: SmolStr,
}
