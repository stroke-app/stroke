//! Model lists read off the CLIs themselves, and the model a Codex chat with
//! none chosen runs on.
//!
//! `ModelSpec` holds `&'static str`s, so a list read at run time is leaked
//! into a static one. A list is leaked only when it differs from the last one,
//! and the command runs at most once a minute, so this stays a few kilobytes
//! for the life of the app.

use crate::spec::ListModels;
use agent_core::ModelSpec;
use serde_json::Value;
use std::process::Stdio;
use std::time::{Duration, Instant};

/// How long a list read off a CLI is used before it is read again.
pub const TTL: Duration = Duration::from_secs(60);
/// For the list command to answer before it is given up on.
const READ_TIMEOUT: Duration = Duration::from_secs(15);

/// One model as a CLI listed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub id: String,
    pub label: String,
    pub note: String,
}

/// The last list read off a CLI.
#[derive(Default)]
pub(crate) struct Cache {
    models: Option<&'static [ModelSpec]>,
    read_at: Option<Instant>,
    reading: bool,
}

impl Cache {
    /// The list as last read, if one has been.
    pub(crate) fn get(&self) -> Option<&'static [ModelSpec]> {
        self.models
    }

    /// Whether a read is due, and if so marks one as started.
    pub(crate) fn claim(&mut self) -> bool {
        if self.reading || self.read_at.is_some_and(|t| t.elapsed() < TTL) {
            return false;
        }
        self.reading = true;
        true
    }

    /// Keep what a read found. A failed read keeps the list from before.
    pub(crate) fn store(&mut self, listed: Option<Vec<Listed>>) {
        self.reading = false;
        self.read_at = Some(Instant::now());
        let Some(listed) = listed else { return };
        if self.models.is_some_and(|m| same(m, &listed)) {
            return;
        }
        self.models = Some(leak(listed));
    }
}

fn same(have: &[ModelSpec], listed: &[Listed]) -> bool {
    have.len() == listed.len()
        && have
            .iter()
            .zip(listed)
            .all(|(a, b)| a.id == b.id && a.label == b.label && a.note == b.note)
}

fn leak(listed: Vec<Listed>) -> &'static [ModelSpec] {
    let s = |v: String| -> &'static str { Box::leak(v.into_boxed_str()) };
    let specs: Vec<ModelSpec> = listed
        .into_iter()
        .map(|m| ModelSpec {
            id: s(m.id),
            label: s(m.label),
            note: s(m.note),
        })
        .collect();
    Box::leak(specs.into_boxed_slice())
}

/// Run a CLI's list command and read its answer. Blocks, so it runs on a
/// thread of its own.
pub(crate) fn read(bin: &str, list: &ListModels) -> Option<Vec<Listed>> {
    let child = std::process::Command::new(bin)
        .args(list.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(READ_TIMEOUT) {
        Ok(Ok(out)) if out.status.success() => {
            Some((list.parse)(&String::from_utf8_lossy(&out.stdout)))
        }
        Ok(_) => None,
        Err(_) => {
            tracing::debug!(bin, "model list timed out");
            let _ = std::process::Command::new("kill")
                .arg(pid.to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            None
        }
    }
}

/// `opencode models --verbose`: each model's `provider/model` id on a line of
/// its own, then its metadata as indented JSON. Without the metadata (no
/// `--verbose`, or an opencode without it) the ids alone still make a list.
pub fn parse_opencode(out: &str) -> Vec<Listed> {
    let mut found: Vec<(String, String)> = Vec::new();
    for line in out.lines() {
        let t = line.trim_end();
        let is_id = !t.is_empty()
            && !t.starts_with(['{', '}', ' ', '\t'])
            && t.contains('/')
            && !t.contains(char::is_whitespace);
        if is_id {
            found.push((t.to_string(), String::new()));
        } else if let Some((_, meta)) = found.last_mut() {
            meta.push_str(line);
            meta.push('\n');
        }
    }
    found
        .into_iter()
        .map(|(id, meta)| {
            let meta = serde_json::from_str::<Value>(&meta).ok();
            let (provider, model) = id.split_once('/').unwrap_or(("", id.as_str()));
            let label = meta
                .as_ref()
                .and_then(|m| m.get("name"))
                .and_then(Value::as_str)
                .filter(|n| !n.is_empty())
                .unwrap_or(model)
                .to_string();
            Listed {
                note: provider.to_string(),
                label,
                id,
            }
        })
        .collect()
}

/// The model a Codex chat with none chosen runs on, when codex's own choice
/// would fail.
///
/// Codex falls back to the `model` in its config.toml, which can name one the
/// account cannot use: `gpt-5.5` on a ChatGPT account fails every turn with
/// "not supported when using Codex with a ChatGPT account". Codex's catalog
/// (`codex debug models`) is what it knows for this account, and it warns
/// "Model metadata for `gpt-5.5` not found" for one missing from it. Such a
/// model is replaced with the catalog's first listed one; otherwise `None`,
/// and codex keeps its own choice. The config itself is never changed.
pub(crate) async fn codex_default(bin: &str) -> Option<String> {
    let configured = codex_config()
        .as_deref()
        .and_then(|text| top_level(text, "model"))?;
    let out = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new(bin)
            .args(["debug", "models"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    if !out.status.success() {
        return None;
    }
    let pick = codex_replacement(&String::from_utf8_lossy(&out.stdout), &configured)?;
    tracing::info!(
        configured,
        pick,
        "codex's configured model is not in its catalog; using another"
    );
    Some(pick)
}

/// What to run instead of `configured`, given codex's catalog: `None` when
/// the catalog knows it (or cannot be read).
fn codex_replacement(catalog: &str, configured: &str) -> Option<String> {
    let v: Value = serde_json::from_str(catalog).ok()?;
    let models = v.get("models")?.as_array()?;
    let slug = |m: &Value| m.get("slug").and_then(Value::as_str).map(str::to_string);
    if models
        .iter()
        .any(|m| slug(m).as_deref() == Some(configured))
    {
        return None;
    }
    let mut listed: Vec<&Value> = models
        .iter()
        .filter(|m| m.get("visibility").and_then(Value::as_str) == Some("list"))
        .collect();
    listed.sort_by_key(|m| {
        m.get("priority")
            .and_then(Value::as_i64)
            .unwrap_or(i64::MAX)
    });
    listed.first().and_then(|m| slug(m))
}

/// Codex's config.toml, from `$CODEX_HOME` or `~/.codex`.
fn codex_config() -> Option<String> {
    let home = std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".codex")))?;
    std::fs::read_to_string(home.join("config.toml")).ok()
}

/// A string key of a TOML file, outside any table: `model = "gpt-5.5"`.
fn top_level(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            break;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k.trim() != key {
            continue;
        }
        let v = v.trim();
        let q = v.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let rest = &v[1..];
        return rest.find(q).map(|end| rest[..end].to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from `opencode models --verbose` (opencode 1.18.10), trimmed
    /// to two models.
    const OPENCODE_VERBOSE: &str = r#"opencode/big-pickle
{
  "id": "big-pickle",
  "providerID": "opencode",
  "name": "Big Pickle",
  "family": "big-pickle",
  "cost": {
    "input": 0,
    "output": 0
  },
  "release_date": "2025-10-17",
  "variants": {}
}
opencode/deepseek-v4.1-flash-free
{
  "id": "deepseek-v4.1-flash-free",
  "providerID": "opencode",
  "name": "DeepSeek V4.1 Flash Free",
  "variants": {}
}
"#;

    #[test]
    fn opencode_verbose_list_reads_ids_names_and_providers() {
        let got = parse_opencode(OPENCODE_VERBOSE);
        assert_eq!(
            got,
            [
                Listed {
                    id: "opencode/big-pickle".into(),
                    label: "Big Pickle".into(),
                    note: "opencode".into(),
                },
                Listed {
                    id: "opencode/deepseek-v4.1-flash-free".into(),
                    label: "DeepSeek V4.1 Flash Free".into(),
                    note: "opencode".into(),
                },
            ]
        );
    }

    #[test]
    fn opencode_plain_list_still_reads() {
        let got = parse_opencode("anthropic/claude-x\nopenrouter/meta/llama-4\n");
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].id, "openrouter/meta/llama-4");
        assert_eq!(got[1].label, "meta/llama-4");
        assert_eq!(got[1].note, "openrouter");
    }

    #[test]
    fn an_unchanged_list_is_not_leaked_again() {
        let mut c = Cache::default();
        assert!(c.claim());
        assert!(!c.claim(), "one read at a time");
        c.store(Some(parse_opencode(OPENCODE_VERBOSE)));
        let first = c.get().unwrap();
        assert_eq!(first[0].id, "opencode/big-pickle");
        assert!(!c.claim(), "read less than a minute ago");

        c.read_at = None;
        assert!(c.claim());
        c.store(Some(parse_opencode(OPENCODE_VERBOSE)));
        assert!(std::ptr::eq(first, c.get().unwrap()));

        // A failed read keeps what was there.
        c.read_at = None;
        assert!(c.claim());
        c.store(None);
        assert!(std::ptr::eq(first, c.get().unwrap()));
    }

    /// `codex debug models` for a ChatGPT account (codex-cli 0.146.0), with
    /// each entry cut to the fields read here.
    const CODEX_CATALOG: &str = r#"{"models":[
        {"slug":"gpt-reserve","visibility":"hide","priority":4},
        {"slug":"gpt-5.6-terra","visibility":"list","priority":8},
        {"slug":"gpt-5.6-luna","visibility":"list","priority":9},
        {"slug":"codex-auto-review","visibility":"hide","priority":43}
    ]}"#;

    #[test]
    fn a_configured_codex_model_the_account_lacks_is_replaced() {
        assert_eq!(
            codex_replacement(CODEX_CATALOG, "gpt-5.5").as_deref(),
            Some("gpt-5.6-terra")
        );
        assert_eq!(codex_replacement(CODEX_CATALOG, "gpt-5.6-luna"), None);
        assert_eq!(codex_replacement("not json", "gpt-5.5"), None);
    }

    #[test]
    fn the_top_level_model_key_is_read_and_tables_are_not() {
        let toml = "model = \"gpt-5.5\"\nmodel_reasoning_effort = \"medium\"\n\
                    [profiles.x]\nmodel = \"o3\"\n";
        assert_eq!(top_level(toml, "model").as_deref(), Some("gpt-5.5"));
        assert_eq!(
            top_level(toml, "model_reasoning_effort").as_deref(),
            Some("medium")
        );
        assert_eq!(top_level("[x]\nmodel = 'o3'\n", "model"), None);
        assert_eq!(
            top_level("model = 'o3' # mine\n", "model").as_deref(),
            Some("o3")
        );
    }
}
