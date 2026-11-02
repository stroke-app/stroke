use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("agent `{0}` is not installed")]
    NotInstalled(String),

    #[error("agent `{agent}` needs authentication: {hint}")]
    NeedsAuth { agent: String, hint: String },

    #[error("failed to spawn agent process: {0}")]
    Spawn(#[source] std::io::Error),

    /// The backend produced bytes we could not decode. Carries the offending
    /// line so adapter bugs are reportable rather than silent.
    #[error("protocol error from `{agent}`: {detail}")]
    Protocol {
        agent: String,
        detail: String,
        line: Option<String>,
    },

    #[error("agent session closed unexpectedly (exit: {exit:?})")]
    Closed { exit: Option<i32> },

    #[error("operation unsupported by this agent: {0}")]
    Unsupported(&'static str),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl AgentError {
    /// Whether the supervisor should try restarting rather than failing the
    /// session outright.
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            AgentError::Closed { .. } | AgentError::Io(_) | AgentError::Spawn(_)
        )
    }

    pub fn protocol(agent: &str, detail: impl Into<String>, line: Option<&str>) -> Self {
        AgentError::Protocol {
            agent: agent.to_string(),
            detail: detail.into(),
            line: line.map(|l| l.chars().take(512).collect()),
        }
    }
}
