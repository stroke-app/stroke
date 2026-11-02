use crate::event::{AgentEvent, StopReason, ToolOutput, Usage};
use crate::ids::{AgentId, MessageId, ToolId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use smol_str::SmolStr;

/// The canonical, agent-agnostic conversation. This — not any backend's
/// session file — is the source of truth, which is what makes mid-thread agent
/// switching possible.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Transcript {
    pub messages: Vec<Message>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub role: Role,
    /// Which agent produced this. `None` for user messages. Rendered as a badge
    /// so provenance stays visible in mixed-agent threads.
    pub agent: Option<AgentId>,
    pub blocks: Vec<Block>,
    /// Unix millis. Passed in rather than sampled so replay stays deterministic.
    pub created_at: i64,
    pub stop: Option<StopReason>,
}

impl Message {
    pub fn new(role: Role, agent: Option<AgentId>, created_at: i64) -> Self {
        Self {
            id: MessageId::new(),
            role,
            agent,
            blocks: Vec::new(),
            created_at,
            stop: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    User,
    Assistant,
}

/// One renderable unit. Messages are `Vec<Block>` rather than a single string
/// so that streaming only invalidates the tail block's layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Block {
    Text {
        text: String,
        /// Open blocks are still receiving deltas; closed ones have frozen
        /// layout and cached measured height.
        open: bool,
    },
    Reasoning {
        text: String,
        open: bool,
    },
    Tool(ToolInvocation),
    /// Marks a mid-thread agent handoff, rendered as a divider.
    Handoff {
        from: Option<AgentId>,
        to: AgentId,
        mode: crate::adapter::HydrateMode,
    },
    /// Attachments on a user turn.
    Attachment {
        path: String,
        kind: AttachmentKind,
    },
}

impl Block {
    pub fn is_open(&self) -> bool {
        match self {
            Block::Text { open, .. } | Block::Reasoning { open, .. } => *open,
            Block::Tool(t) => t.result.is_none(),
            Block::Handoff { .. } | Block::Attachment { .. } => false,
        }
    }

    fn close(&mut self) {
        match self {
            Block::Text { open, .. } | Block::Reasoning { open, .. } => *open = false,
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentKind {
    Image,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocation {
    pub id: ToolId,
    pub name: SmolStr,
    pub input: Value,
    pub title: String,
    /// Streaming output accumulated before the result lands.
    pub progress: String,
    pub result: Option<ToolOutput>,
    pub is_error: bool,
}

impl ToolInvocation {
    /// Still running — the card renders a spinner and live `progress`.
    pub fn is_open(&self) -> bool {
        self.result.is_none()
    }
}

/// Outcome of folding an event into the transcript, so the UI can invalidate
/// exactly one list row instead of re-rendering the thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Applied {
    pub kind: AppliedKind,
    /// Index of the message that changed, when one did. A late tool result can
    /// land on a message that is no longer the tail, so the UI must not assume
    /// the last row is the only one that needs remeasuring.
    pub message: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppliedKind {
    /// Nothing changed structurally; the tail block's text grew.
    TailGrew,
    /// A block was added, closed, or replaced — remeasure that message.
    BlockChanged,
    /// A message was added, or its stop reason was set.
    MessageChanged,
    /// Event carried no transcript-visible content (usage, permission prompts).
    Ignored,
}

impl Applied {
    const IGNORED: Applied = Applied {
        kind: AppliedKind::Ignored,
        message: None,
    };

    fn at(kind: AppliedKind, message: usize) -> Applied {
        Applied {
            kind,
            message: Some(message),
        }
    }
}

impl Transcript {
    /// Fold one adapter event into the canonical transcript.
    ///
    /// `now` is supplied by the caller (never sampled here) so that replaying a
    /// persisted event log reproduces byte-identical state.
    pub fn apply(&mut self, agent: &AgentId, event: &AgentEvent, now: i64) -> Applied {
        match event {
            AgentEvent::Started { .. } => Applied::IGNORED,
            // The chat reads it to decide on a retry; the transcript keeps
            // the error from the turn's own text.
            AgentEvent::TurnFailed { .. } => Applied::IGNORED,

            AgentEvent::Text { delta } => self.push_delta(agent, now, delta, false),
            AgentEvent::Reasoning { delta } => self.push_delta(agent, now, delta, true),

            AgentEvent::ToolCall {
                id,
                name,
                input,
                title,
            } => {
                // A call shown before its input finished: fill it in.
                if let Some((idx, t)) = self.find_tool_mut(id) {
                    t.input = input.clone();
                    t.title = title.clone();
                    return Applied::at(AppliedKind::BlockChanged, idx);
                }
                let idx = self.assistant_tail_index(agent, now);
                let msg = &mut self.messages[idx];
                if let Some(b) = msg.blocks.last_mut() {
                    if b.is_open() {
                        b.close();
                    }
                }
                msg.blocks.push(Block::Tool(ToolInvocation {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                    title: title.clone(),
                    progress: String::new(),
                    result: None,
                    is_error: false,
                }));
                Applied::at(AppliedKind::BlockChanged, idx)
            }

            AgentEvent::ToolProgress { id, chunk } => match self.find_tool_mut(id) {
                Some((idx, t)) => {
                    t.progress.push_str(chunk);
                    Applied::at(AppliedKind::TailGrew, idx)
                }
                None => Applied::IGNORED,
            },

            AgentEvent::ToolResult {
                id,
                output,
                is_error,
            } => match self.find_tool_mut(id) {
                Some((idx, t)) => {
                    t.result = Some(output.clone());
                    t.is_error = *is_error;
                    Applied::at(AppliedKind::BlockChanged, idx)
                }
                None => Applied::IGNORED,
            },

            AgentEvent::Usage(u) => {
                self.usage.accumulate(u);
                Applied::IGNORED
            }

            AgentEvent::TurnEnd { stop } => self.finish_tail(Some(*stop)),

            AgentEvent::Permission { .. }
            | AgentEvent::Question { .. }
            | AgentEvent::AskCancelled { .. } => Applied::IGNORED,

            // The UI places these messages itself, where the model read them.
            AgentEvent::UserDelivered { .. } | AgentEvent::UserDropped { .. } => Applied::IGNORED,

            AgentEvent::Fatal { .. } => self.finish_tail(Some(StopReason::Error)),
        }
    }

    fn push_delta(&mut self, agent: &AgentId, now: i64, delta: &str, reasoning: bool) -> Applied {
        let idx = self.assistant_tail_index(agent, now);
        let msg = &mut self.messages[idx];
        let appended = match msg.blocks.last_mut() {
            Some(Block::Text { text, open: true }) if !reasoning => {
                text.push_str(delta);
                true
            }
            Some(Block::Reasoning { text, open: true }) if reasoning => {
                text.push_str(delta);
                true
            }
            // An open block of the *other* kind must be closed first, so text and
            // reasoning never merge into one run.
            Some(b) if b.is_open() => {
                b.close();
                false
            }
            _ => false,
        };
        if appended {
            return Applied::at(AppliedKind::TailGrew, idx);
        }
        msg.blocks.push(if reasoning {
            Block::Reasoning {
                text: delta.to_string(),
                open: true,
            }
        } else {
            Block::Text {
                text: delta.to_string(),
                open: true,
            }
        });
        Applied::at(AppliedKind::BlockChanged, idx)
    }

    fn finish_tail(&mut self, stop: Option<StopReason>) -> Applied {
        match self.messages.len().checked_sub(1) {
            Some(idx) => {
                let msg = &mut self.messages[idx];
                if let Some(b) = msg.blocks.last_mut() {
                    b.close();
                }
                msg.stop = stop;
                Applied::at(AppliedKind::MessageChanged, idx)
            }
            None => Applied::IGNORED,
        }
    }

    pub fn push_user(&mut self, text: String, attachments: Vec<Block>, now: i64) -> MessageId {
        let mut msg = Message::new(Role::User, None, now);
        if !text.is_empty() {
            msg.blocks.push(Block::Text { text, open: false });
        }
        msg.blocks.extend(attachments);
        let id = msg.id;
        self.messages.push(msg);
        id
    }

    pub fn push_handoff(
        &mut self,
        from: Option<AgentId>,
        to: AgentId,
        mode: crate::adapter::HydrateMode,
        now: i64,
    ) {
        let mut msg = Message::new(Role::Assistant, Some(to.clone()), now);
        msg.blocks.push(Block::Handoff { from, to, mode });
        msg.stop = Some(StopReason::EndTurn);
        self.messages.push(msg);
    }

    /// Index of the assistant message currently being streamed into, creating
    /// one if the last message is a user turn or a finished assistant turn.
    fn assistant_tail_index(&mut self, agent: &AgentId, now: i64) -> usize {
        let needs_new = match self.messages.last() {
            Some(m) => {
                m.role != Role::Assistant || m.stop.is_some() || m.agent.as_ref() != Some(agent)
            }
            None => true,
        };
        if needs_new {
            self.messages
                .push(Message::new(Role::Assistant, Some(agent.clone()), now));
        }
        self.messages.len() - 1
    }

    /// Tool results can arrive out of order and after later blocks were opened,
    /// so search backwards rather than assuming the tail.
    fn find_tool_mut(&mut self, id: &ToolId) -> Option<(usize, &mut ToolInvocation)> {
        self.messages
            .iter_mut()
            .enumerate()
            .rev()
            .find_map(|(idx, m)| {
                m.blocks.iter_mut().rev().find_map(|b| match b {
                    Block::Tool(t) if &t.id == id => Some(idx),
                    _ => None,
                })
            })
            .map(|idx| {
                let t = self.messages[idx]
                    .blocks
                    .iter_mut()
                    .rev()
                    .find_map(|b| match b {
                        Block::Tool(t) if &t.id == id => Some(t),
                        _ => None,
                    })
                    .expect("located above");
                (idx, t)
            })
    }

    /// Rough token estimate, used to decide replay vs. summarized handoff.
    pub fn approx_tokens(&self) -> usize {
        let chars: usize = self
            .messages
            .iter()
            .flat_map(|m| m.blocks.iter())
            .map(|b| match b {
                Block::Text { text, .. } | Block::Reasoning { text, .. } => text.len(),
                Block::Tool(t) => {
                    t.title.len() + t.result.as_ref().map_or(0, |r| r.preview(2048).len())
                }
                _ => 0,
            })
            .sum();
        chars / 4
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::AgentEvent;

    fn agent() -> AgentId {
        AgentId::new("claude")
    }

    fn text(s: &str) -> AgentEvent {
        AgentEvent::Text {
            delta: s.to_string(),
        }
    }

    #[test]
    fn text_deltas_coalesce_into_one_block() {
        let mut t = Transcript::default();
        assert_eq!(
            t.apply(&agent(), &text("Hel"), 0).kind,
            AppliedKind::BlockChanged
        );
        assert_eq!(
            t.apply(&agent(), &text("lo"), 0).kind,
            AppliedKind::TailGrew
        );
        assert_eq!(t.messages.len(), 1);
        assert_eq!(t.messages[0].blocks.len(), 1);
        match &t.messages[0].blocks[0] {
            Block::Text { text, open } => {
                assert_eq!(text, "Hello");
                assert!(open);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn tool_call_closes_open_text_and_result_attaches() {
        let mut t = Transcript::default();
        t.apply(&agent(), &text("checking"), 0);
        let id = ToolId::new("t1");
        t.apply(
            &agent(),
            &AgentEvent::ToolCall {
                id: id.clone(),
                name: "bash".into(),
                input: serde_json::json!({"command":"ls"}),
                title: "ls".into(),
            },
            0,
        );
        assert!(!t.messages[0].blocks[0].is_open(), "text should be closed");

        t.apply(
            &agent(),
            &AgentEvent::ToolProgress {
                id: id.clone(),
                chunk: "a\nb\n".into(),
            },
            0,
        );
        t.apply(
            &agent(),
            &AgentEvent::ToolResult {
                id,
                output: ToolOutput::Terminal {
                    text: "a\nb\n".into(),
                    exit_code: Some(0),
                },
                is_error: false,
            },
            0,
        );

        match &t.messages[0].blocks[1] {
            Block::Tool(t) => {
                assert_eq!(t.progress, "a\nb\n");
                assert!(t.result.is_some());
                assert!(!t.is_open());
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_repeated_tool_call_fills_in_the_same_card() {
        let mut t = Transcript::default();
        let call = |input: Value| AgentEvent::ToolCall {
            id: ToolId::new("w"),
            name: "Write".into(),
            input,
            title: "/a.py".into(),
        };
        t.apply(
            &agent(),
            &call(serde_json::json!({"file_path": "/a.py"})),
            0,
        );
        let applied = t.apply(
            &agent(),
            &call(serde_json::json!({"file_path": "/a.py", "content": "x"})),
            0,
        );
        assert_eq!(applied.kind, AppliedKind::BlockChanged);
        assert_eq!(applied.message, Some(0));
        assert_eq!(t.messages.len(), 1);
        match t.messages[0].blocks.as_slice() {
            [Block::Tool(tool)] => assert_eq!(tool.input["content"], "x"),
            other => panic!("expected one card, got {other:?}"),
        }
    }

    #[test]
    fn reasoning_and_text_never_merge_into_one_block() {
        let mut t = Transcript::default();
        t.apply(
            &agent(),
            &AgentEvent::Reasoning {
                delta: "thinking".into(),
            },
            0,
        );
        t.apply(&agent(), &text("answer"), 0);
        let blocks = &t.messages[0].blocks;
        assert_eq!(blocks.len(), 2, "got {blocks:?}");
        assert!(matches!(&blocks[0], Block::Reasoning { text, open: false } if text == "thinking"));
        assert!(matches!(&blocks[1], Block::Text { text, .. } if text == "answer"));
    }

    #[test]
    fn switching_agent_starts_a_new_message() {
        let mut t = Transcript::default();
        t.apply(&agent(), &text("from claude"), 0);
        t.apply(&AgentId::new("grok"), &text("from grok"), 0);
        assert_eq!(t.messages.len(), 2);
        assert_eq!(t.messages[1].agent, Some(AgentId::new("grok")));
    }

    #[test]
    fn turn_end_closes_tail_and_next_delta_opens_a_message() {
        let mut t = Transcript::default();
        t.apply(&agent(), &text("one"), 0);
        t.apply(
            &agent(),
            &AgentEvent::TurnEnd {
                stop: StopReason::EndTurn,
            },
            0,
        );
        t.apply(&agent(), &text("two"), 0);
        assert_eq!(t.messages.len(), 2);
    }

    #[test]
    fn preview_never_splits_a_utf8_boundary() {
        let out = ToolOutput::Text("héllo wörld".into());
        for n in 0..14 {
            let _ = out.preview(n); // must not panic
        }
        assert_eq!(out.preview(2), "hé");
    }

    #[test]
    fn late_tool_result_finds_its_call_behind_newer_blocks() {
        let mut t = Transcript::default();
        let a = ToolId::new("a");
        let b = ToolId::new("b");
        for id in [&a, &b] {
            t.apply(
                &agent(),
                &AgentEvent::ToolCall {
                    id: id.clone(),
                    name: "bash".into(),
                    input: Value::Null,
                    title: String::new(),
                },
                0,
            );
        }
        // `a` resolves after `b` was opened.
        let applied = t.apply(
            &agent(),
            &AgentEvent::ToolResult {
                id: a,
                output: ToolOutput::Text("done".into()),
                is_error: false,
            },
            0,
        );
        assert_eq!(applied.kind, AppliedKind::BlockChanged);
        assert_eq!(
            applied.message,
            Some(0),
            "the UI must be told which row to remeasure"
        );
        match &t.messages[0].blocks[0] {
            Block::Tool(t) => assert!(t.result.is_some()),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn usage_sums_counts_and_keeps_the_newest_readings() {
        let mut t = Transcript::default();
        let turn = |input, context, window| {
            AgentEvent::Usage(Usage {
                input,
                output: 10,
                cost_usd: Some(0.5),
                context,
                context_window: window,
                ..Default::default()
            })
        };
        t.apply(&agent(), &turn(100, 40_000, Some(200_000)), 0);
        t.apply(&agent(), &turn(50, 0, None), 0);
        t.apply(&agent(), &turn(0, 30_000, None), 0);
        assert_eq!((t.usage.input, t.usage.output), (150, 30));
        assert_eq!(t.usage.cost_usd, Some(1.5));
        // Compaction shrinks the context; the window outlives a turn that
        // did not restate it.
        assert_eq!(t.usage.context, 30_000);
        assert_eq!(t.usage.context_window, Some(200_000));
        assert_eq!(t.usage.context_used(), Some(0.15));
    }

    #[test]
    fn usage_from_before_readings_still_loads() {
        let u: Usage = serde_json::from_str(
            r#"{"input":1,"output":2,"cache_read":3,"cache_write":4,"cost_usd":null}"#,
        )
        .unwrap();
        assert_eq!(u.tokens(), 10);
        assert_eq!((u.context, u.context_window), (0, None));
    }
}
