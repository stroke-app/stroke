//! Adapters for coding CLIs driven one turn at a time.
//!
//! Unlike the Claude adapter, these CLIs have no bidirectional streaming input
//! mode: each turn is a fresh process invocation. Continuity comes from the
//! session id each CLI reports (`--resume <id>`, `exec resume <id>`, …) or, on
//! a handoff, from replaying our transcript — which is exactly what
//! `AgentSession::hydrate` was designed for.
//!
//! OpenCode is the exception: it has a real server (`opencode serve`), and
//! driving that instead of `opencode run` is what makes it stream (see
//! [`opencode`]).

pub mod decode;
mod http;
pub mod models;
pub mod opencode;
pub mod spec;

use agent_core::{
    AgentAdapter, AgentError, AgentEvent, AgentId, AgentSession, Availability, Caps, Decision,
    HydrateMode, ModelSpec, RequestId, SessionCtx, Transcript, UserTurn,
};
use async_trait::async_trait;
use decode::{Decode, Exit};
use futures::stream::BoxStream;
use parking_lot::Mutex;
use spec::{CliSpec, Resume, Wire};
use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

const EVENT_BUFFER: usize = 1024;
const REPLAY_BUDGET: usize = 40_000;
/// Stderr lines kept for the error message of a failed turn.
const STDERR_TAIL: usize = 40;
/// After the CLI exits, how long something it started (a server it ran in
/// the background) may keep its stdout open before the turn is called over.
const DRAIN_GRACE: Duration = Duration::from_millis(1500);
/// How long a probe waits for the CLI's model list. A slower list still
/// arrives, for the next time the models are asked for.
const PROBE_MODELS_WAIT: Duration = Duration::from_secs(5);

pub struct CliAdapter {
    spec: &'static CliSpec,
    /// The models the CLI listed, for one that can list them.
    listed: Arc<Mutex<models::Cache>>,
}

impl CliAdapter {
    pub fn new(spec: &'static CliSpec) -> Self {
        Self {
            spec,
            listed: Arc::default(),
        }
    }

    /// Read the CLI's model list on a thread of its own, if it has one and a
    /// read is due. The receiver fires once the list is stored.
    fn refresh_models(&self) -> Option<oneshot::Receiver<()>> {
        let list = self.spec.list_models?;
        if !self.listed.lock().claim() {
            return None;
        }
        let (done, rx) = oneshot::channel();
        let cache = self.listed.clone();
        let bin = self.spec.bin;
        let read = move || {
            let found = models::read(bin, &list);
            cache.lock().store(found);
            let _ = done.send(());
        };
        if let Err(e) = std::thread::Builder::new()
            .name(format!("{bin} models"))
            .spawn(read)
        {
            tracing::debug!(error = %e, "could not start the model list read");
            self.listed.lock().store(None);
            return None;
        }
        Some(rx)
    }

    /// Every installed CLI backend, ready to register.
    pub fn all() -> Vec<Arc<dyn AgentAdapter>> {
        spec::ALL
            .iter()
            .map(|s| Arc::new(CliAdapter::new(s)) as Arc<dyn AgentAdapter>)
            .collect()
    }
}

#[async_trait]
impl AgentAdapter for CliAdapter {
    fn id(&self) -> AgentId {
        AgentId::new(self.spec.id)
    }

    fn display_name(&self) -> &str {
        self.spec.display_name
    }

    fn caps(&self) -> Caps {
        self.spec.caps
    }

    /// Cheap enough to call on every frame: the list is read off the CLI in
    /// the background, at most once a minute, and the last one returned.
    fn models(&self) -> &'static [ModelSpec] {
        if self.spec.list_models.is_none() {
            return self.spec.models;
        }
        self.refresh_models();
        self.listed.lock().get().unwrap_or(self.spec.models)
    }

    fn setup(&self) -> Option<agent_core::Setup> {
        self.spec.setup
    }

    async fn probe(&self) -> Result<Availability, AgentError> {
        // Alongside, so the list is there by the time the picker is.
        let listing = self.refresh_models();
        // `--version` is the cheapest liveness check that does not hit the
        // network or consume credits.
        let version = Command::new(self.spec.bin)
            .arg("--version")
            .stdin(Stdio::null())
            .output()
            .await;
        if let (Some(rx), Ok(_)) = (listing, &version) {
            let _ = tokio::time::timeout(PROBE_MODELS_WAIT, rx).await;
        }
        match version {
            Ok(o) if o.status.success() => Ok(Availability::Ready {
                version: Some(
                    String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                ),
            }),
            Ok(_) => Ok(Availability::Ready { version: None }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Availability::Missing {
                hint: format!("`{}` is not on PATH", self.spec.bin),
            }),
            Err(e) => Err(AgentError::Spawn(e)),
        }
    }

    async fn spawn(&self, mut ctx: SessionCtx) -> Result<Box<dyn AgentSession>, AgentError> {
        if self.spec.wire == Wire::OpenCodeJson {
            return Ok(Box::new(opencode::OpenCodeSession::start(self.spec, ctx)));
        }
        if ctx.model.is_none() && self.spec.wire == Wire::CodexJson {
            ctx.model = models::codex_default(self.spec.bin).await.map(Into::into);
        }
        Ok(Box::new(CliSession::new(self.spec, ctx)))
    }
}

/// The text actually sent to the CLI for a turn.
///
/// These backends take a single prompt string per invocation, so the harness
/// prompt and any handoff briefing have to ride along inside the first one —
/// there is no system-prompt flag to put them in.
pub(crate) fn compose(
    turns: u32,
    system_prompt: &str,
    primer: &mut Option<String>,
    turn: &UserTurn,
) -> String {
    let mut prompt = String::new();
    if turns == 0 {
        if !system_prompt.is_empty() {
            prompt.push_str(system_prompt);
            prompt.push_str("\n\n");
        }
        if let Some(primer) = primer.take() {
            prompt.push_str(&primer);
            prompt.push_str("\n\n");
        }
    }
    prompt.push_str(&turn.text);
    for att in &turn.attachments {
        prompt.push_str(&format!("\n[attached file: {}]", att.path.display()));
    }
    prompt
}

/// The briefing a fresh backend gets for a thread begun elsewhere.
pub(crate) fn primer_for(
    transcript: &Transcript,
) -> Result<(Option<String>, HydrateMode), AgentError> {
    if transcript.messages.is_empty() {
        return Ok((None, HydrateMode::Fresh));
    }
    // Reuse the Claude adapter's briefing renderer: it is backend-agnostic
    // by design and already drops the previous model's private reasoning.
    let replay = agent_claude::replay::render(transcript, REPLAY_BUDGET);
    let dropped = replay.dropped;
    Ok((
        Some(replay.text),
        if dropped > 0 {
            HydrateMode::ReplayedPartial {
                dropped_messages: dropped,
            }
        } else {
            HydrateMode::Replayed
        },
    ))
}

/// A running turn's process.
pub(crate) struct TurnHandle {
    kill: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

impl TurnHandle {
    /// Stop the process; its turn then ends as interrupted.
    pub(crate) fn interrupt(&mut self) {
        if let Some(k) = self.kill.take() {
            let _ = k.send(());
        }
    }

    /// Stop it if it still runs, and wait until its last event is out, so it
    /// cannot be mistaken for the next turn's.
    pub(crate) async fn finish(mut self) {
        if !self.task.is_finished() {
            self.interrupt();
            let _ = tokio::time::timeout(Duration::from_secs(5), &mut self.task).await;
        }
    }
}

/// Run one turn of a CLI: spawn it, stream its stdout through `decoder` into
/// `tx`, and end the turn — interrupted, failed with what it said, or done —
/// once it exits.
///
/// `resuming` marks a turn that re-attaches to a session saved by an earlier
/// run of deck. If that fails before the CLI produced anything, the session
/// is reported dead (`Fatal`): that is what makes the app replay the thread
/// into a fresh one, instead of retrying a session that is gone.
pub(crate) fn spawn_turn(
    spec: &'static CliSpec,
    ctx: &SessionCtx,
    args: Vec<String>,
    mut decoder: Box<dyn Decode>,
    tx: mpsc::Sender<AgentEvent>,
    native: Arc<Mutex<Option<String>>>,
    resuming: bool,
) -> Result<TurnHandle, AgentError> {
    let mut cmd = Command::new(spec.bin);
    cmd.args(&args)
        .envs(spec.env.iter().copied())
        .envs(ctx.env.iter().map(|(k, v)| (k, v)))
        .current_dir(&ctx.cwd)
        // None of these may wait on a terminal: codex reads stdin when it is
        // not one ("Reading additional input from stdin..."), and gets EOF.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Its own process group, so stop also reaches what it started (gemini
    // relaunches itself under a second node process; tools run in shells).
    #[cfg(unix)]
    cmd.process_group(0);
    let mut child = cmd.spawn().map_err(AgentError::Spawn)?;
    tracing::debug!(bin = spec.bin, ?args, "agent turn started");

    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let (kill_tx, mut kill_rx) = oneshot::channel::<()>();

    let task = tokio::spawn(async move {
        let mut out = BufReader::new(stdout).lines();
        let mut err = BufReader::new(stderr).lines();
        let (mut out_open, mut err_open) = (true, true);
        let mut tail: VecDeque<String> = VecDeque::new();
        let mut status: Option<Option<i32>> = None;
        let mut interrupted = false;
        let mut armed = true;
        let mut produced = false;
        let grace = tokio::time::sleep(Duration::from_secs(365 * 24 * 3600));
        tokio::pin!(grace);

        loop {
            if !out_open && !err_open && status.is_some() {
                break;
            }
            tokio::select! {
                line = out.next_line(), if out_open => match line {
                    Ok(Some(line)) => {
                        for ev in decoder.decode_line(&line) {
                            match &ev {
                                AgentEvent::Started { native_session_id: Some(id), .. } => {
                                    *native.lock() = Some(id.clone());
                                }
                                AgentEvent::Text { .. }
                                | AgentEvent::Reasoning { .. }
                                | AgentEvent::ToolCall { .. } => produced = true,
                                _ => {}
                            }
                            if tx.send(ev).await.is_err() {
                                kill_tree(&mut child);
                                return;
                            }
                        }
                    }
                    _ => out_open = false,
                },
                line = err.next_line(), if err_open => match line {
                    Ok(Some(line)) => {
                        tracing::debug!(target: "agent.cli.stderr", "{line}");
                        tail.push_back(line);
                        if tail.len() > STDERR_TAIL {
                            tail.pop_front();
                        }
                    }
                    _ => err_open = false,
                },
                st = child.wait(), if status.is_none() => {
                    status = Some(st.ok().and_then(|s| s.code()));
                    grace.as_mut().reset(tokio::time::Instant::now() + DRAIN_GRACE);
                }
                r = &mut kill_rx, if armed => {
                    armed = false;
                    // A dropped sender means the session itself went away.
                    interrupted = r.is_ok();
                    kill_tree(&mut child);
                }
                _ = &mut grace, if status.is_some() => {
                    tracing::debug!("agent exited but its stdout stayed open; ending the turn");
                    break;
                }
            }
        }

        let exit = Exit {
            code: status.flatten(),
            interrupted,
            stderr: tail.into(),
        };
        if exit.code != Some(0) && !exit.interrupted {
            tracing::info!(
                code = ?exit.code,
                stderr = %exit.stderr.join(" | "),
                "agent turn failed"
            );
        }
        let tail = decoder.end(&exit);
        if resuming && !produced {
            let failed = tail.iter().find_map(|e| match e {
                AgentEvent::TurnFailed { message, .. } => Some(message.clone()),
                _ => None,
            });
            if let Some(message) = failed {
                *native.lock() = None;
                let _ = tx
                    .send(AgentEvent::Fatal {
                        message: format!(
                            "could not resume the {} session: {message}",
                            spec.display_name
                        ),
                        retryable: false,
                    })
                    .await;
                return;
            }
        }
        for ev in tail {
            if tx.send(ev).await.is_err() {
                break;
            }
        }
    });

    Ok(TurnHandle {
        kill: Some(kill_tx),
        task,
    })
}

/// Kill a CLI and everything in its process group.
pub(crate) fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // The group was made by `process_group(0)`, so its id is the pid.
        let _ = Command::new("kill")
            .args(["-TERM", &format!("-{pid}")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
    let _ = child.start_kill();
}

pub struct CliSession {
    spec: &'static CliSpec,
    ctx: SessionCtx,
    /// Every turn's events go out on this one channel; the session keeps a
    /// sender, so the stream never ends between turns.
    tx: mpsc::Sender<AgentEvent>,
    rx: mpsc::Receiver<AgentEvent>,
    turn: Option<TurnHandle>,
    /// This chat's session id as the CLI reported it, to resume next turn.
    native_session: Arc<Mutex<Option<String>>>,
    /// Transcript briefing prepended to the first prompt, set by `hydrate`.
    primer: Option<String>,
    /// Turns the CLI has had in this conversation, counting the ones before
    /// a resume (so the harness prompt is sent once).
    turns: u32,
    /// Turns this session has sent.
    sent: u32,
}

impl CliSession {
    pub fn new(spec: &'static CliSpec, ctx: SessionCtx) -> Self {
        let (tx, rx) = mpsc::channel(EVENT_BUFFER);
        // Re-attaching to a session from an earlier run: it already has the
        // harness prompt.
        let turns = u32::from(ctx.resume.is_some());
        Self {
            spec,
            native_session: Arc::new(Mutex::new(ctx.resume.clone())),
            ctx,
            tx,
            rx,
            turn: None,
            primer: None,
            turns,
            sent: 0,
        }
    }

    fn compose(&mut self, turn: &UserTurn) -> String {
        compose(self.turns, &self.ctx.system_prompt, &mut self.primer, turn)
    }

    /// What to resume, if anything: this chat's own session id once the CLI
    /// has reported one. A CLI that reports none can only continue "the
    /// latest" session, and only after a turn of ours has made it ours.
    fn resume_arg(&self) -> Option<String> {
        match self.spec.resume {
            Resume::Never => None,
            Resume::Latest(_) => (self.turns > 0).then(String::new),
            Resume::Flag(_) | Resume::Subcommand(_) => self.native_session.lock().clone(),
        }
    }
}

#[async_trait]
impl AgentSession for CliSession {
    async fn send(&mut self, turn: UserTurn) -> Result<(), AgentError> {
        let prompt = self.compose(&turn);
        let resume = self.resume_arg();
        let args = self.spec.args(
            &prompt,
            &self.ctx.cwd.to_string_lossy(),
            self.ctx.model.as_deref(),
            resume.as_deref(),
            self.ctx.permission_mode,
        );
        self.turns += 1;
        let resuming = self.sent == 0 && self.ctx.resume.is_some() && resume.is_some();
        self.sent += 1;
        // The app waits for a turn to end before the next; if one is somehow
        // still running, stop it first so its end is not taken for this one's.
        if let Some(prev) = self.turn.take() {
            prev.finish().await;
        }
        self.turn = Some(spawn_turn(
            self.spec,
            &self.ctx,
            args,
            decode::for_spec(self.spec),
            self.tx.clone(),
            self.native_session.clone(),
            resuming,
        )?);
        Ok(())
    }

    fn events(&mut self) -> BoxStream<'_, AgentEvent> {
        // Never ends: this session holds a sender. Ending would make the
        // driver exit between turns.
        Box::pin(futures::stream::poll_fn(move |cx| self.rx.poll_recv(cx)))
    }

    async fn interrupt(&mut self) -> Result<(), AgentError> {
        // No control channel exists for a one-shot process, so killing it is the
        // only real stop. The next turn spawns a fresh one.
        if let Some(t) = &mut self.turn {
            t.interrupt();
        }
        Ok(())
    }

    async fn decide(&mut self, _id: RequestId, _d: Decision) -> Result<(), AgentError> {
        Err(AgentError::Unsupported(
            "this CLI has no per-call permission hook; approval is set at spawn",
        ))
    }

    async fn hydrate(&mut self, transcript: &Transcript) -> Result<HydrateMode, AgentError> {
        let (primer, mode) = primer_for(transcript)?;
        if primer.is_some() {
            self.primer = primer;
        }
        Ok(mode)
    }

    fn native_session_id(&self) -> Option<String> {
        self.native_session.lock().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use std::path::PathBuf;

    fn ctx() -> SessionCtx {
        SessionCtx {
            session_id: agent_core::SessionId::new(),
            cwd: PathBuf::from("/tmp"),
            system_prompt: Arc::from("HARNESS"),
            model: None,
            mcp: Arc::new(vec![]),
            skills_dir: None,
            runtime_dir: PathBuf::from("/tmp"),
            resume: None,
            permission_mode: agent_core::PermissionMode::Ask,
            env: Vec::new(),
            allowed_tools: Vec::new(),
        }
    }

    fn session(spec: &'static CliSpec) -> CliSession {
        CliSession::new(spec, ctx())
    }

    #[test]
    fn the_first_prompt_carries_the_harness_and_later_ones_do_not() {
        let mut s = session(&spec::CRUSH);
        let first = s.compose(&UserTurn::text("hello"));
        assert!(first.starts_with("HARNESS"));
        assert!(first.ends_with("hello"));

        s.turns = 1;
        let second = s.compose(&UserTurn::text("again"));
        assert_eq!(second, "again", "harness must not be resent every turn");
    }

    #[test]
    fn a_resumed_session_does_not_get_the_harness_again() {
        let mut c = ctx();
        c.resume = Some("thread-1".into());
        let mut s = CliSession::new(&spec::CODEX, c);
        assert_eq!(s.compose(&UserTurn::text("hi")), "hi");
        assert_eq!(s.resume_arg().as_deref(), Some("thread-1"));
    }

    #[tokio::test]
    async fn a_hydrated_transcript_rides_along_in_the_first_prompt() {
        let mut s = session(&spec::CRUSH);
        let mut t = Transcript::default();
        t.push_user("earlier question".into(), vec![], 0);

        let mode = s.hydrate(&t).await.unwrap();
        assert_eq!(mode, HydrateMode::Replayed);

        let prompt = s.compose(&UserTurn::text("follow up"));
        assert!(prompt.contains("earlier question"), "briefing missing");
        assert!(prompt.contains("do not re-run"));
        assert!(prompt.ends_with("follow up"));
    }

    #[tokio::test]
    async fn an_empty_transcript_hydrates_as_fresh_with_no_primer() {
        let mut s = session(&spec::CRUSH);
        assert_eq!(
            s.hydrate(&Transcript::default()).await.unwrap(),
            HydrateMode::Fresh
        );
        assert!(s.primer.is_none());
    }

    static MISSING: CliSpec = CliSpec {
        id: "nope",
        display_name: "Nope",
        bin: "definitely-not-installed-xyz",
        wire: Wire::PlainText,
        base_args: &[],
        cwd_flag: None,
        model_flag: None,
        resume: Resume::Never,
        yolo_args: &[],
        prompt_flag: None,
        login: None,
        caps: Caps::DEGRADED,
        models: &[],
        list_models: None,
        env: &[],
        setup: None,
    };

    #[tokio::test]
    async fn probe_reports_missing_binaries_rather_than_erroring() {
        let a = CliAdapter::new(&MISSING);
        match a.probe().await.unwrap() {
            Availability::Missing { hint } => assert!(hint.contains("PATH")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn all_registers_every_spec() {
        let all = CliAdapter::all();
        assert_eq!(all.len(), spec::ALL.len());
        let ids: Vec<String> = all.iter().map(|a| a.id().to_string()).collect();
        for want in ["opencode", "vibe", "grok", "crush", "gemini", "codex"] {
            assert!(ids.contains(&want.to_string()), "{want} not registered");
        }
    }

    #[test]
    fn resume_is_not_used_on_the_very_first_turn() {
        // Otherwise `--continue` attaches to an unrelated prior session that
        // happens to exist in this directory.
        let s = session(&spec::CRUSH);
        assert_eq!(s.resume_arg(), None);
        let s = session(&spec::GROK);
        assert_eq!(s.resume_arg(), None);
    }

    #[test]
    fn the_session_id_a_turn_reports_is_resumed_next_turn() {
        let s = session(&spec::GEMINI);
        *s.native_session.lock() = Some("cfd4f06c".into());
        assert_eq!(s.resume_arg().as_deref(), Some("cfd4f06c"));
        assert_eq!(s.native_session_id().as_deref(), Some("cfd4f06c"));
    }

    /// A fake CLI: a shell script standing in for the binary.
    static SH: CliSpec = CliSpec {
        id: "sh",
        display_name: "Shell",
        bin: "/bin/sh",
        wire: Wire::PlainText,
        base_args: &["-c"],
        cwd_flag: None,
        model_flag: None,
        resume: Resume::Never,
        yolo_args: &[],
        prompt_flag: None,
        login: Some("sh-login"),
        caps: Caps::DEGRADED,
        models: &[],
        list_models: None,
        env: &[],
        setup: None,
    };

    async fn turn_of(script: &str) -> Vec<AgentEvent> {
        turn_on(&SH, script).await
    }

    async fn turn_on(spec: &'static CliSpec, script: &str) -> Vec<AgentEvent> {
        let mut c = ctx();
        c.system_prompt = Arc::from("");
        let mut s = CliSession::new(spec, c);
        // `sh -c -- <script>`: `--` ends sh's options, the script follows.
        s.send(UserTurn::text(script)).await.unwrap();
        let mut out = Vec::new();
        let mut events = s.events();
        while let Ok(Some(ev)) = tokio::time::timeout(Duration::from_secs(10), events.next()).await
        {
            let end = matches!(ev, AgentEvent::TurnEnd { .. });
            out.push(ev);
            if end {
                break;
            }
        }
        out
    }

    /// A fake CLI that cannot find the session it is asked to resume, as
    /// gemini says when its session is gone. The flag and id land in `$@`.
    static GONE: CliSpec = CliSpec {
        id: "gone",
        display_name: "Fake",
        bin: "/bin/sh",
        wire: Wire::PlainText,
        base_args: &[
            "-c",
            "echo 'Error resuming session: No previous sessions found for this project.' >&2; exit 42",
            "fake",
        ],
        cwd_flag: None,
        model_flag: None,
        resume: Resume::Flag("--resume"),
        yolo_args: &[],
        prompt_flag: None,
        login: None,
        caps: Caps::DEGRADED,
        models: &[],
        list_models: None,
        env: &[],
        setup: None,
    };

    #[tokio::test]
    async fn a_saved_session_that_cannot_be_resumed_is_reported_dead() {
        // Fatal, not a failed turn: the app then replays the thread into a
        // fresh session rather than retrying the dead one forever.
        let mut c = ctx();
        c.resume = Some("e58ddba7".into());
        let mut s = CliSession::new(&GONE, c);
        s.send(UserTurn::text("hello")).await.unwrap();
        let ev = {
            let mut events = s.events();
            tokio::time::timeout(Duration::from_secs(10), events.next())
                .await
                .unwrap()
        };
        match ev {
            Some(AgentEvent::Fatal { message, retryable }) => {
                assert!(!retryable);
                assert!(
                    message.ends_with("No previous sessions found for this project."),
                    "{message}"
                );
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(s.native_session_id(), None, "the dead id is forgotten");
    }

    #[tokio::test]
    async fn a_cli_that_fails_on_stderr_alone_ends_the_turn_with_its_words() {
        let ev = turn_of("echo 'Not signed in.' >&2; exit 1").await;
        assert!(
            ev.iter().any(|e| matches!(
                e,
                AgentEvent::TurnFailed { message, .. }
                    if message == "Shell is not signed in: run `sh-login` in a terminal."
            )),
            "{ev:?}"
        );
        assert!(matches!(
            ev.last(),
            Some(AgentEvent::TurnEnd {
                stop: agent_core::StopReason::Error
            })
        ));
    }

    /// A fake vibe: a shell standing in for it, with the spec's own
    /// environment.
    static FAKE_VIBE: CliSpec = CliSpec {
        id: "fake-vibe",
        display_name: "Vibe",
        bin: "/bin/sh",
        wire: Wire::PlainText,
        base_args: &["-c"],
        cwd_flag: None,
        model_flag: None,
        resume: Resume::Never,
        yolo_args: &[],
        prompt_flag: None,
        login: Some("vibe --setup"),
        caps: Caps::DEGRADED,
        models: &[],
        list_models: None,
        env: &[("DECK_SPEC_ENV", "from-spec")],
        setup: None,
    };

    #[tokio::test]
    async fn the_spec_environment_reaches_the_cli() {
        let ev = turn_on(&FAKE_VIBE, "echo $DECK_SPEC_ENV").await;
        assert!(
            ev.iter()
                .any(|e| matches!(e, AgentEvent::Text { delta } if delta == "from-spec\n")),
            "{ev:?}"
        );
    }

    #[tokio::test]
    async fn a_vibe_that_gave_up_on_a_429_fails_the_turn_with_why() {
        // What vibe 2.26.1 printed once its retries were spent.
        let script = r#"printf '%s\n' 'Error: API error occurred: Status 429. Body: {"object":"error","message":"Rate limit exceeded","type":"rate_limited","param":null,"code":"1300","raw_status_code":429}'
echo 'You are using the new Vibe TUI, currently in preview. Please report this error at https://github.com/mistralai/mistral-vibe/issues. To switch back to the classic TUI, set VIBE_CLI=python.' >&2
exit 1"#;
        let ev = turn_on(&FAKE_VIBE, script).await;
        assert!(
            ev.iter().any(|e| matches!(
                e,
                AgentEvent::TurnFailed { status: Some(429), message } if message == "Rate limit exceeded"
            )),
            "{ev:?}"
        );
        assert!(matches!(
            ev.last(),
            Some(AgentEvent::TurnEnd {
                stop: agent_core::StopReason::Error
            })
        ));
    }

    #[tokio::test]
    async fn output_streams_and_a_clean_exit_ends_the_turn() {
        let ev = turn_of("echo one; echo two").await;
        let text: String = ev
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Text { delta } => Some(delta.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(text, "one\ntwo\n");
        assert!(matches!(
            ev.last(),
            Some(AgentEvent::TurnEnd {
                stop: agent_core::StopReason::EndTurn
            })
        ));
    }

    #[tokio::test]
    async fn a_background_child_holding_stdout_does_not_hang_the_turn() {
        let started = std::time::Instant::now();
        let ev = turn_of("echo hi; (sleep 30 &) ; exit 0").await;
        assert!(
            matches!(ev.last(), Some(AgentEvent::TurnEnd { .. })),
            "{ev:?}"
        );
        assert!(started.elapsed() < Duration::from_secs(8));
    }

    #[tokio::test]
    async fn interrupt_kills_the_process_and_ends_as_interrupted() {
        let mut c = ctx();
        c.system_prompt = Arc::from("");
        let mut s = CliSession::new(&SH, c);
        s.send(UserTurn::text("echo started; sleep 30"))
            .await
            .unwrap();
        // Wait for it to be running.
        {
            let mut events = s.events();
            let first = tokio::time::timeout(Duration::from_secs(10), events.next())
                .await
                .unwrap();
            assert!(matches!(first, Some(AgentEvent::Text { .. })));
        }
        let started = std::time::Instant::now();
        s.interrupt().await.unwrap();
        let mut events = s.events();
        let end = tokio::time::timeout(Duration::from_secs(10), events.next())
            .await
            .unwrap();
        assert!(
            matches!(
                end,
                Some(AgentEvent::TurnEnd {
                    stop: agent_core::StopReason::Interrupted
                })
            ),
            "{end:?}"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
