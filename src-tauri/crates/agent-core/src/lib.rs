//! The narrow waist of the app.
//!
//! Every agent backend — Claude Code, OpenCode, Vibe, Grok — is reduced to the
//! [`AgentAdapter`] / [`AgentSession`] pair and the [`AgentEvent`] stream
//! defined here. The UI, persistence, and harness layers depend only on this
//! crate, never on a specific backend.

pub mod adapter;
pub mod error;
pub mod event;
pub mod ids;
pub mod transcript;

pub use adapter::{
    AgentAdapter, AgentSession, Attachment, Availability, Caps, HydrateMode, McpServerSpec,
    McpTransport, ModelSpec, PermissionMode, SessionCtx, Setup, UserTurn,
};
pub use error::AgentError;
pub use event::{
    Action, AgentEvent, Decision, Question, QuestionOption, StopReason, ToolOutput, Usage,
};
pub use ids::{AgentId, MessageId, RequestId, SessionId, ToolId};
pub use transcript::{
    Applied, AppliedKind, AttachmentKind, Block, Message, Role, ToolInvocation, Transcript,
};
