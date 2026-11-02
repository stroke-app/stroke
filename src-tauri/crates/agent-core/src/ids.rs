use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::fmt;

/// Stable identifier for an agent backend, e.g. `claude`, `opencode`, `grok`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AgentId(pub SmolStr);

impl AgentId {
    pub fn new(s: impl Into<SmolStr>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

macro_rules! uuid_id {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct $name(pub uuid::Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(uuid::Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

uuid_id!(
    /// Identifies one canonical conversation. Survives agent switches.
    SessionId
);
uuid_id!(
    /// Identifies one message (a user turn or one assistant turn).
    MessageId
);
uuid_id!(
    /// Identifies a pending permission request.
    RequestId
);

/// Tool-call identifier. Backends assign these as opaque strings, so we keep
/// theirs verbatim rather than minting our own — correlating `ToolCall` with
/// `ToolResult` depends on matching the backend's spelling exactly.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ToolId(pub SmolStr);

impl ToolId {
    pub fn new(s: impl Into<SmolStr>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ToolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
