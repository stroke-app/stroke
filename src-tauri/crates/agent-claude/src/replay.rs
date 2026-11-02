//! Rendering a canonical transcript back into prose, so a *different* backend
//! can pick up a conversation another one started.
//!
//! This is what makes mid-thread agent switching work, so it is deliberately
//! plain text: no backend-specific structure, no tool-call replay (the new agent
//! must not think it already ran those tools — it is being told what happened).

use agent_core::{Block, Role, Transcript};

/// How much of the replay we spend on a single tool result.
const TOOL_PREVIEW: usize = 600;

pub struct Replay {
    pub text: String,
    /// Oldest messages dropped to fit the budget.
    pub dropped: usize,
}

/// Render the transcript as a handoff briefing, newest-biased.
///
/// `budget_tokens` is approximate; we drop whole messages from the front until
/// the estimate fits, because a truncated first message reads as corruption.
pub fn render(transcript: &Transcript, budget_tokens: usize) -> Replay {
    let mut start = 0;
    // Cheap loop: recompute the estimate as we drop from the front. Transcripts
    // are small enough (thousands, not millions) that this stays trivial.
    loop {
        let est = estimate(transcript, start);
        if est <= budget_tokens || start >= transcript.messages.len() {
            break;
        }
        start += 1;
    }

    let mut text = String::from(
        "You are taking over an in-progress conversation from another coding agent. \
         The transcript below is what happened so far. Tools listed were already \
         executed — do not re-run them unless you need fresh results.\n\n\
         <handoff_transcript>\n",
    );
    if start > 0 {
        text.push_str("[earlier turns omitted for length]\n\n");
    }

    for msg in &transcript.messages[start..] {
        let who = match msg.role {
            Role::User => "User".to_string(),
            Role::Assistant => match &msg.agent {
                Some(a) => format!("Assistant ({a})"),
                None => "Assistant".to_string(),
            },
        };
        text.push_str(&who);
        text.push_str(":\n");
        for block in &msg.blocks {
            match block {
                Block::Text { text: t, .. } => {
                    text.push_str(t.trim());
                    text.push('\n');
                }
                // Reasoning is intentionally dropped: it is another model's
                // private scratchpad and mostly confuses the successor.
                Block::Reasoning { .. } => {}
                Block::Tool(t) => {
                    text.push_str(&format!("[ran {} — {}]\n", t.name, t.title));
                    if let Some(out) = &t.result {
                        let body = out.preview(TOOL_PREVIEW);
                        if !body.trim().is_empty() {
                            text.push_str("  → ");
                            text.push_str(body.trim());
                            text.push('\n');
                        }
                    }
                }
                Block::Attachment { path, .. } => {
                    text.push_str(&format!("[attached {path}]\n"));
                }
                Block::Handoff { to, .. } => {
                    text.push_str(&format!("[handed off to {to}]\n"));
                }
            }
        }
        text.push('\n');
    }
    text.push_str("</handoff_transcript>\n");

    Replay {
        text,
        dropped: start,
    }
}

fn estimate(t: &Transcript, start: usize) -> usize {
    let chars: usize = t.messages[start..]
        .iter()
        .flat_map(|m| m.blocks.iter())
        .map(|b| match b {
            Block::Text { text, .. } => text.len(),
            Block::Reasoning { .. } => 0,
            Block::Tool(t) => {
                t.name.len()
                    + t.title.len()
                    + t.result
                        .as_ref()
                        .map_or(0, |r| r.preview(TOOL_PREVIEW).len())
            }
            _ => 32,
        })
        .sum();
    chars / 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::{AgentId, ToolInvocation, ToolOutput};

    fn transcript() -> Transcript {
        let mut t = Transcript::default();
        t.push_user("does this project use gpui".into(), vec![], 0);
        t.apply(
            &AgentId::new("claude"),
            &agent_core::AgentEvent::Reasoning {
                delta: "private thoughts".into(),
            },
            0,
        );
        t.apply(
            &AgentId::new("claude"),
            &agent_core::AgentEvent::Text {
                delta: "Yes, gpui 0.2.2.".into(),
            },
            0,
        );
        if let Some(m) = t.messages.last_mut() {
            m.blocks.push(Block::Tool(ToolInvocation {
                id: agent_core::ToolId::new("t1"),
                name: "Bash".into(),
                input: serde_json::Value::Null,
                title: "grep gpui Cargo.toml".into(),
                progress: String::new(),
                result: Some(ToolOutput::Terminal {
                    text: "gpui = \"0.2.2\"".into(),
                    exit_code: Some(0),
                }),
                is_error: false,
            }));
        }
        t
    }

    #[test]
    fn replay_includes_turns_and_tool_summaries() {
        let r = render(&transcript(), 10_000);
        assert_eq!(r.dropped, 0);
        assert!(r.text.contains("does this project use gpui"));
        assert!(r.text.contains("Assistant (claude)"));
        assert!(r.text.contains("[ran Bash — grep gpui Cargo.toml]"));
        assert!(r.text.contains("gpui = \"0.2.2\""));
    }

    #[test]
    fn replay_omits_other_models_reasoning() {
        let r = render(&transcript(), 10_000);
        assert!(
            !r.text.contains("private thoughts"),
            "another model's scratchpad must not leak into the handoff"
        );
    }

    #[test]
    fn replay_tells_successor_not_to_rerun_tools() {
        let r = render(&transcript(), 10_000);
        assert!(r.text.contains("do not re-run"));
    }

    #[test]
    fn oversized_transcript_drops_oldest_and_reports_it() {
        let mut t = Transcript::default();
        for i in 0..50 {
            t.push_user(format!("message {i} {}", "padding ".repeat(50)), vec![], 0);
        }
        let r = render(&t, 200);
        assert!(r.dropped > 0, "should have dropped something");
        assert!(r.text.contains("earlier turns omitted"));
        assert!(
            r.text.contains("message 49"),
            "must keep the newest turn: {}",
            &r.text[..200.min(r.text.len())]
        );
    }

    #[test]
    fn empty_transcript_renders_without_panicking() {
        let r = render(&Transcript::default(), 100);
        assert_eq!(r.dropped, 0);
        assert!(r.text.contains("</handoff_transcript>"));
    }
}
