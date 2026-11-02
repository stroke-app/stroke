//! Declarative descriptions of the coding CLIs we drive.
//!
//! Every flag here was read off the tool's own `--help` on a machine with it
//! installed, not guessed, and re-checked against these versions:
//! opencode 1.18.10, codex-cli 0.146.0, gemini 0.53.1, grok 1.0.60,
//! vibe 2.26.1. Where a CLI offers a structured output mode whose event schema
//! we have observed, we use it: plain text gives no tool calls, no errors and
//! no session id to resume with.

use agent_core::{Caps, ModelSpec, PermissionMode};

/// How to read a backend's stdout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wire {
    /// NDJSON in opencode's `run --format json` shape. Only used when the
    /// long-lived `opencode serve` cannot be started (see `opencode.rs`).
    OpenCodeJson,
    /// `codex exec --json`: `thread.started`, `item.*`, `turn.*` events.
    CodexJson,
    /// `gemini --output-format stream-json`: `init`, `message`, `tool_use`,
    /// `tool_result`, `error`, `result`.
    GeminiJson,
    /// `grok --output-format streaming-json`: `text`, `thought`, `end`, `error`.
    GrokJson,
    /// Everything the process prints is the reply. Used for CLIs whose JSON
    /// event schema we have not verified — a wrong decoder is worse than no
    /// decoder, because it silently drops content.
    PlainText,
}

/// How a later turn picks up the conversation of an earlier one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resume {
    /// No way to: each turn is a fresh conversation.
    Never,
    /// A flag meaning "the most recent session here". Can attach to another
    /// chat's session in the same folder, so only used where the CLI reports
    /// no session id to resume by.
    Latest(&'static str),
    /// `<flag> <id>` with the id the CLI reported for this chat.
    Flag(&'static str),
    /// `<subcommand> <id>` after the other options: `codex exec … resume <id>`.
    Subcommand(&'static str),
}

#[derive(Debug, Clone)]
pub struct CliSpec {
    pub id: &'static str,
    pub display_name: &'static str,
    pub bin: &'static str,
    pub wire: Wire,
    /// Argv before everything else.
    pub base_args: &'static [&'static str],
    /// Flag pair for the working directory, if the CLI takes one.
    pub cwd_flag: Option<&'static str>,
    /// Flag pair for the model.
    pub model_flag: Option<&'static str>,
    /// How to continue this chat's conversation on a later turn.
    pub resume: Resume,
    /// Flags that skip tool approval prompts. Required for headless use:
    /// without them these CLIs either hang waiting for a TTY or refuse every
    /// tool. The first one is the approval switch itself.
    pub yolo_args: &'static [&'static str],
    /// How the prompt is passed: behind this flag as `--flag=<prompt>`, or as
    /// a trailing positional after `--`. Either way a prompt that starts with
    /// `-` (a Markdown list item) cannot be mistaken for an option.
    pub prompt_flag: Option<&'static str>,
    /// The command that signs a user in, for the error shown when the CLI
    /// says nobody is.
    pub login: Option<&'static str>,
    pub caps: Caps,
    pub models: &'static [ModelSpec],
    /// A command that lists the models the CLI can use right now. Once it has
    /// been read, its list replaces `models`.
    pub list_models: Option<ListModels>,
    /// Environment set for every turn, before the session's own.
    pub env: &'static [(&'static str, &'static str)],
    /// How to install or sign in, when the CLI is missing. Taken from each
    /// project's own published instructions, not invented.
    pub setup: Option<agent_core::Setup>,
}

/// A command that prints a CLI's models, and how to read what it prints.
#[derive(Debug, Clone, Copy)]
pub struct ListModels {
    /// Argv after the binary.
    pub args: &'static [&'static str],
    pub parse: fn(&str) -> Vec<crate::models::Listed>,
}

impl CliSpec {
    /// The flag that skips approvals, if the CLI has one.
    pub fn yolo_flag(&self) -> Option<&'static str> {
        self.yolo_args.first().copied()
    }

    /// Argv for one turn. `resume` is this chat's session id from an earlier
    /// turn, if there is one.
    pub fn args(
        &self,
        prompt: &str,
        cwd: &str,
        model: Option<&str>,
        resume: Option<&str>,
        permission: PermissionMode,
    ) -> Vec<String> {
        let mut a: Vec<String> = self.base_args.iter().map(|s| s.to_string()).collect();
        if let Some(f) = self.cwd_flag {
            a.push(f.into());
            a.push(cwd.into());
        }
        if let (Some(f), Some(m)) = (self.model_flag, model) {
            a.push(f.into());
            a.push(m.into());
        }
        // Anything but `Ask` means the user has accepted unattended tool use;
        // these CLIs have no per-call hook for us to answer.
        if permission != PermissionMode::Ask {
            a.extend(self.yolo_args.iter().map(|s| s.to_string()));
        }
        match (self.resume, resume) {
            (Resume::Never, _) => {}
            (Resume::Latest(f), Some(_)) => a.push(f.into()),
            (Resume::Flag(f), Some(id)) => {
                a.push(f.into());
                a.push(id.into());
            }
            // Must come last: the subcommand takes the prompt after the id.
            (Resume::Subcommand(s), Some(id)) => {
                a.push(s.into());
                a.push(id.into());
            }
            (_, None) => {}
        }
        match self.prompt_flag {
            Some(f) => a.push(format!("{f}={prompt}")),
            None => {
                a.push("--".into());
                a.push(prompt.into());
            }
        }
        a
    }
}

/// Caps for a CLI we drive one turn at a time over plain text.
const PLAIN_CAPS: Caps = Caps {
    native_mcp: false,
    native_skills: false,
    native_resume: false,
    permission_hook: false,
    images: false,
    streaming_tools: false,
    usage: false,
    soft_interrupt: false,
    queues_input: false,
    structured: false,
};

/// Caps for a CLI driven one turn at a time over a verified JSON stream: tool
/// calls, usage and a session id to resume by, but no approvals to answer.
const JSON_CAPS: Caps = Caps {
    structured: true,
    usage: true,
    native_resume: true,
    native_mcp: false,
    native_skills: false,
    permission_hook: false,
    images: false,
    streaming_tools: false,
    soft_interrupt: false,
    queues_input: false,
};

pub const OPENCODE: CliSpec = CliSpec {
    id: "opencode",
    display_name: "OpenCode",
    bin: "opencode",
    // Normally driven through `opencode serve` (see `opencode.rs`); these are
    // the args for the one-shot `run` it falls back to.
    wire: Wire::OpenCodeJson,
    base_args: &["run", "--format", "json", "--thinking"],
    cwd_flag: Some("--dir"),
    model_flag: Some("--model"),
    resume: Resume::Flag("--session"),
    yolo_args: &["--auto"],
    prompt_flag: None,
    login: Some("opencode auth login"),
    caps: Caps {
        // Through the server: streamed text, live tool output, approvals,
        // and a real stop.
        structured: true,
        usage: true,
        native_resume: true,
        native_mcp: true,
        streaming_tools: true,
        permission_hook: true,
        soft_interrupt: true,
        native_skills: false,
        images: false,
        queues_input: false,
    },
    // Which models there are depends on the providers signed in to.
    models: &[],
    list_models: Some(ListModels {
        args: &["models", "--verbose"],
        parse: crate::models::parse_opencode,
    }),
    env: &[],
    setup: Some(agent_core::Setup {
        verb: "Install",
        command: "npm install -g opencode-ai",
        source: "opencode.ai",
    }),
};

pub const VIBE: CliSpec = CliSpec {
    id: "vibe",
    display_name: "Vibe",
    bin: "vibe",
    wire: Wire::PlainText,
    // `--output streaming` exists but its event schema is unverified here, so
    // text mode is used until it can be observed against the real CLI.
    base_args: &["--output", "text"],
    cwd_flag: Some("--workdir"),
    model_flag: None,
    // `--resume` without an id opens an interactive picker.
    resume: Resume::Latest("--continue"),
    // vibe 2.14 dropped `--auto-approve` ("unrecognized arguments"); the
    // approval policy is now an agent. `--trust` skips the folder-trust prompt.
    yolo_args: &["--agent", "auto-approve", "--trust"],
    prompt_flag: Some("--prompt"),
    login: Some("vibe --setup"),
    caps: PLAIN_CAPS,
    models: &[],
    list_models: None,
    // In headless mode vibe prints nothing while it retries a failed request,
    // and it retries for five minutes by default: a 429 from a key with no
    // quota left was retried silently the whole time, so the chat looked hung.
    // Twenty seconds covers a passing blip; past that the error reaches the
    // chat, and deck's own retry, which says what it waits on, takes over.
    env: &[("VIBE_API_RETRY_MAX_ELAPSED_TIME", "20")],
    setup: Some(agent_core::Setup {
        verb: "Install",
        command: "npm install -g @vibe-kit/vibe",
        source: "the Vibe docs",
    }),
};

pub const GROK: CliSpec = CliSpec {
    id: "grok",
    display_name: "Grok",
    bin: "grok",
    wire: Wire::GrokJson,
    base_args: &["--output-format", "streaming-json"],
    cwd_flag: Some("--cwd"),
    model_flag: Some("--model"),
    resume: Resume::Flag("--resume"),
    yolo_args: &["--always-approve"],
    prompt_flag: Some("--single"),
    login: Some("grok login"),
    caps: JSON_CAPS,
    // `grok models` offers this account only `grok-4.7`, its default, so a
    // picker would have nothing to choose between.
    models: &[],
    list_models: None,
    env: &[],
    setup: Some(agent_core::Setup {
        verb: "Install",
        command: "npm install -g @vibe-kit/grok-cli",
        source: "the grok-cli README",
    }),
};

pub const CRUSH: CliSpec = CliSpec {
    id: "crush",
    display_name: "Crush",
    bin: "crush",
    wire: Wire::PlainText,
    base_args: &["run"],
    cwd_flag: Some("--cwd"),
    model_flag: None,
    resume: Resume::Latest("--continue"),
    yolo_args: &["--yolo"],
    prompt_flag: None,
    login: None,
    caps: PLAIN_CAPS,
    models: &[],
    list_models: None,
    env: &[],
    setup: Some(agent_core::Setup {
        verb: "Install",
        command: "npm install -g @charmland/crush",
        source: "charmbracelet/crush",
    }),
};

pub const GEMINI: CliSpec = CliSpec {
    id: "gemini",
    display_name: "Gemini",
    bin: "gemini",
    wire: Wire::GeminiJson,
    base_args: &["--output-format", "stream-json"],
    cwd_flag: None,
    model_flag: Some("--model"),
    resume: Resume::Flag("--resume"),
    // Without `--skip-trust`, a folder never opened in gemini's own UI is
    // untrusted and `--yolo` is quietly overridden back to "default", which
    // headless mode cannot ask about.
    yolo_args: &["--yolo", "--skip-trust"],
    prompt_flag: Some("--prompt"),
    login: Some("gemini"),
    caps: JSON_CAPS,
    // gemini's own aliases: it resolves each to the newest model the account
    // may use (`pro` is gemini-3-pro-preview with preview access, else
    // gemini-2.5-pro). There is no `gemini-3-pro`.
    models: &[
        ModelSpec {
            id: "pro",
            label: "Gemini Pro",
            note: "most capable",
        },
        ModelSpec {
            id: "flash",
            label: "Gemini Flash",
            note: "fast",
        },
        ModelSpec {
            id: "flash-lite",
            label: "Gemini Flash Lite",
            note: "fastest",
        },
    ],
    list_models: None,
    env: &[],
    setup: Some(agent_core::Setup {
        verb: "Install",
        command: "npm install -g @google/gemini-cli",
        source: "the Gemini CLI README",
    }),
};

pub const CODEX: CliSpec = CliSpec {
    id: "codex",
    display_name: "Codex",
    bin: "codex",
    wire: Wire::CodexJson,
    // Without `--skip-git-repo-check` codex refuses any folder that is not a
    // git repository it trusts.
    base_args: &["exec", "--json", "--skip-git-repo-check"],
    cwd_flag: Some("--cd"),
    model_flag: Some("--model"),
    resume: Resume::Subcommand("resume"),
    yolo_args: &["--dangerously-bypass-approvals-and-sandbox"],
    prompt_flag: None,
    login: Some("codex login"),
    caps: JSON_CAPS,
    // What `codex debug models` lists for a ChatGPT account; both answered a
    // turn through `exec --json --model`. With none chosen, see
    // `models::codex_default`.
    models: &[
        ModelSpec {
            id: "gpt-5.6-terra",
            label: "GPT-5.6 Terra",
            note: "balanced",
        },
        ModelSpec {
            id: "gpt-5.6-luna",
            label: "GPT-5.6 Luna",
            note: "fastest",
        },
    ],
    list_models: None,
    env: &[],
    setup: Some(agent_core::Setup {
        verb: "Install",
        command: "npm install -g @openai/codex",
        source: "the Codex README",
    }),
};

pub const ALL: &[&CliSpec] = &[&OPENCODE, &VIBE, &GROK, &CRUSH, &GEMINI, &CODEX];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opencode_argv_matches_the_verified_cli_surface() {
        let a = OPENCODE.args(
            "hi",
            "/repo",
            Some("anthropic/x"),
            Some("ses_1"),
            PermissionMode::Yolo,
        );
        assert_eq!(
            a,
            [
                "run",
                "--format",
                "json",
                "--thinking",
                "--dir",
                "/repo",
                "--model",
                "anthropic/x",
                "--auto",
                "--session",
                "ses_1",
                "--",
                "hi"
            ]
        );
    }

    #[test]
    fn codex_resumes_by_thread_id_with_the_options_before_the_subcommand() {
        // Verified: `codex exec --json … resume <id> <prompt>` continues the
        // thread; `resume` itself does not accept `--cd`.
        let a = CODEX.args(
            "again",
            "/r",
            None,
            Some("01a14317-28ae"),
            PermissionMode::Yolo,
        );
        assert_eq!(
            a,
            [
                "exec",
                "--json",
                "--skip-git-repo-check",
                "--cd",
                "/r",
                "--dangerously-bypass-approvals-and-sandbox",
                "resume",
                "01a14317-28ae",
                "--",
                "again"
            ]
        );
    }

    #[test]
    fn codex_takes_the_model_on_a_first_turn_and_on_a_resumed_one() {
        // Verified: `exec … --model gpt-5.6-luna -- <prompt>` and
        // `exec … --model gpt-5.6-terra … resume <id> -- <prompt>` both answer.
        // Without it, resume falls back to the config's model too.
        let a = CODEX.args("x", "/r", Some("gpt-5.6-luna"), None, PermissionMode::Ask);
        let i = a.iter().position(|x| x == "--model").unwrap();
        assert_eq!(a[i + 1], "gpt-5.6-luna");
        let a = CODEX.args(
            "x",
            "/r",
            Some("gpt-5.6-terra"),
            Some("01a14413"),
            PermissionMode::Yolo,
        );
        let m = a.iter().position(|x| x == "--model").unwrap();
        let r = a.iter().position(|x| x == "resume").unwrap();
        assert_eq!(a[m + 1], "gpt-5.6-terra");
        assert!(m < r, "options go before the subcommand: {a:?}");
        assert_eq!(a[r + 1], "01a14413");
    }

    #[test]
    fn the_listed_models_are_ones_the_clis_accept() {
        let ids = |s: &CliSpec| s.models.iter().map(|m| m.id).collect::<Vec<_>>();
        assert_eq!(ids(&CODEX), ["gpt-5.6-terra", "gpt-5.6-luna"]);
        // gemini's aliases; `gemini-3-pro` is not a model it knows.
        assert_eq!(ids(&GEMINI), ["pro", "flash", "flash-lite"]);
        for spec in ALL {
            if !spec.models.is_empty() || spec.list_models.is_some() {
                assert!(spec.model_flag.is_some(), "{} has no model flag", spec.id);
            }
        }
    }

    #[test]
    fn vibe_gives_up_retrying_before_the_chat_looks_hung() {
        let budget = VIBE
            .env
            .iter()
            .find(|(k, _)| *k == "VIBE_API_RETRY_MAX_ELAPSED_TIME")
            .map(|(_, v)| v.parse::<u32>().unwrap());
        assert!(budget.is_some_and(|s| s <= 30), "{budget:?}");
    }

    #[test]
    fn gemini_and_grok_resume_by_the_id_they_reported() {
        let a = GEMINI.args("x", "/r", None, Some("cfd4f06c"), PermissionMode::Edit);
        let i = a.iter().position(|x| x == "--resume").unwrap();
        assert_eq!(a[i + 1], "cfd4f06c");
        assert!(a.contains(&"--skip-trust".to_string()), "{a:?}");
        let a = GROK.args("x", "/r", None, Some("abc"), PermissionMode::Ask);
        let i = a.iter().position(|x| x == "--resume").unwrap();
        assert_eq!(a[i + 1], "abc");
    }

    #[test]
    fn prompt_is_positional_or_flagged_per_cli() {
        // crush takes the prompt positionally, after `--`...
        let a = CRUSH.args("do it", "/r", None, None, PermissionMode::Ask);
        assert_eq!(&a[a.len() - 2..], ["--", "do it"]);

        // ...vibe takes it behind its flag, glued on so it is one argument.
        let a = VIBE.args("do it", "/r", None, None, PermissionMode::Ask);
        assert_eq!(a.last().unwrap(), "--prompt=do it");
    }

    #[test]
    fn a_prompt_that_looks_like_an_option_stays_the_prompt() {
        for spec in ALL {
            let a = spec.args("- fix the list", "/r", None, None, PermissionMode::Ask);
            let last = a.last().unwrap();
            match spec.prompt_flag {
                Some(f) => assert_eq!(last, &format!("{f}=- fix the list")),
                None => {
                    assert_eq!(last, "- fix the list");
                    assert_eq!(a[a.len() - 2], "--", "{}", spec.id);
                }
            }
        }
    }

    #[test]
    fn ask_mode_never_passes_a_yolo_flag() {
        for spec in ALL {
            let a = spec.args("x", "/r", None, None, PermissionMode::Ask);
            for flag in spec.yolo_args {
                assert!(
                    !a.contains(&flag.to_string()),
                    "{} leaked {flag} in Ask mode",
                    spec.id
                );
            }
        }
    }

    #[test]
    fn non_ask_modes_do_pass_the_approval_flag() {
        for spec in ALL {
            let a = spec.args("x", "/r", None, None, PermissionMode::Yolo);
            if let Some(flag) = spec.yolo_flag() {
                assert!(a.contains(&flag.to_string()), "{} missing {flag}", spec.id);
            }
        }
    }

    #[test]
    fn vibe_uses_the_approval_agent_not_the_removed_flag() {
        let a = VIBE.args("x", "/r", None, None, PermissionMode::Yolo);
        assert!(!a.contains(&"--auto-approve".to_string()));
        let i = a.iter().position(|x| x == "--agent").unwrap();
        assert_eq!(a[i + 1], "auto-approve");
    }

    #[test]
    fn resume_flag_only_appears_when_resuming() {
        let a = OPENCODE.args("x", "/r", None, None, PermissionMode::Ask);
        assert!(!a.contains(&"--session".to_string()));
        let a = OPENCODE.args("x", "/r", None, Some("ses_9"), PermissionMode::Ask);
        assert!(a.contains(&"--session".to_string()));
        let a = CRUSH.args("x", "/r", None, Some("any"), PermissionMode::Ask);
        assert!(a.contains(&"--continue".to_string()));
        assert!(!a.contains(&"any".to_string()), "crush has no id to pass");
    }

    #[test]
    fn model_flag_is_omitted_when_no_model_is_chosen() {
        let a = GEMINI.args("x", "/r", None, None, PermissionMode::Ask);
        assert!(!a.contains(&"--model".to_string()));
    }

    #[test]
    fn plain_text_backends_declare_themselves_unstructured() {
        // The UI marks these sessions degraded; that must not drift silently.
        for spec in ALL {
            if spec.wire == Wire::PlainText {
                assert!(!spec.caps.structured, "{} claims structured", spec.id);
                assert!(!spec.caps.permission_hook, "{}", spec.id);
            } else {
                assert!(spec.caps.structured, "{} decodes JSON", spec.id);
            }
        }
    }

    #[test]
    fn every_spec_has_a_unique_id() {
        let mut ids: Vec<&str> = ALL.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "duplicate agent id");
    }
}
