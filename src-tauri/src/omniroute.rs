// OmniRoute: an OpenAI-compatible proxy the user runs locally. Installing it by
// hand means finding Node, running a global npm install and keeping a terminal
// open, so the app can do all three - but only ever on an explicit click, and
// only for this one well-known package.
//
// Every failure here is reported with the specific missing piece (Node absent,
// npm absent, install rejected, port busy) because "could not start OmniRoute"
// gives the user nothing to act on.

use crate::proc::quiet;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Mutex;
use tauri::{Emitter, Manager};
use tokio::io::AsyncBufReadExt;
use tokio::process::{Child, Command as Cmd};

/// The running server, if this app started it. Kept so Stop can reach the child
/// and so quitting the app does not leave an orphaned proxy behind.
#[derive(Default)]
pub struct OmniRouteState {
    child: Mutex<Option<Child>>,
}

impl OmniRouteState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Synchronously kill the child if this app started one. Called from the
    /// `RunEvent::Exit` hook in lib.rs - the async `omniroute_stop` path never
    /// runs on quit, so without this the proxy outlives the app.
    pub fn kill_now(&self) {
        if let Ok(mut slot) = self.child.lock() {
            if let Some(mut c) = slot.take() {
                let _ = c.start_kill();
            }
        }
    }
}

#[derive(serde::Serialize, Clone)]
pub struct OmniLog {
    pub line: String,
    pub kind: String,
}

fn emit(app: &tauri::AppHandle, line: &str, kind: &str) {
    let _ = app.emit(
        "omniroute-log",
        OmniLog {
            line: line.to_owned(),
            kind: kind.to_owned(),
        },
    );
}

// ── Finding the user's toolchain ─────────────────────────────────────────────
//
// A windowed app does not get the PATH a terminal gets. macOS starts a .app
// from launchd, which hands it `/usr/bin:/bin:/usr/sbin:/sbin` and nothing
// else - no Homebrew, no nvm, no fnm, no Volta. Node lives in exactly those
// places, so every lookup here missed and the app told users who plainly had
// Node installed that they had none, with no way to act on it. Linux .desktop
// launches have the same gap; Windows GUI processes do inherit the user PATH.

static USER_PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// PATH to run tool lookups under. Resolved once per process.
pub(crate) async fn user_path() -> String {
    if let Some(p) = USER_PATH.get() {
        return p.clone();
    }
    let built = build_user_path().await;
    USER_PATH.get_or_init(|| built).clone()
}

async fn build_user_path() -> String {
    let sep = if cfg!(target_os = "windows") { ';' } else { ':' };
    let mut dirs: Vec<String> = Vec::new();
    fn add(dirs: &mut Vec<String>, d: &str) {
        if !d.is_empty() && !dirs.iter().any(|x| x == d) {
            dirs.push(d.to_string());
        }
    }

    if let Ok(p) = std::env::var("PATH") {
        for d in p.split(sep) {
            add(&mut dirs, d);
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(p) = login_shell_path().await {
            for d in p.split(':') {
                add(&mut dirs, d);
            }
        }
        for d in fallback_dirs() {
            add(&mut dirs, &d);
        }
    }

    dirs.join(&sep.to_string())
}

/// Ask the user's login shell what its PATH is.
///
/// This is the only approach that survives version managers: nvm and fnm create
/// their bin directory from an rc file, so it is written down nowhere a process
/// can read. Editors shell out for the same reason.
#[cfg(not(target_os = "windows"))]
async fn login_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").ok()?;
    // -i so the rc file that defines the version manager actually runs. stdin
    // closed and a hard timeout because an interactive shell is entitled to
    // wait for input, and the app must not wait with it.
    let mut cmd = Cmd::new(&shell);
    cmd.args(["-ilc", "printf %s \"$PATH\""])
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    let out = tokio::time::timeout(std::time::Duration::from_secs(4), quiet(&mut cmd).output())
    .await
    .ok()?
    .ok()?;
    // `printf` emits no newline, so the PATH is whatever follows the last one -
    // login shells are free to print a banner ahead of it.
    let text = String::from_utf8_lossy(&out.stdout);
    let p = text.rsplit('\n').next().unwrap_or("").trim().to_string();
    if p.contains('/') && !p.is_empty() { Some(p) } else { None }
}

/// Where toolchains live when the shell could not be asked.
#[cfg(not(target_os = "windows"))]
fn fallback_dirs() -> Vec<String> {
    let mut out: Vec<String> = ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();

    let Ok(home) = std::env::var("HOME") else {
        return out;
    };
    for rel in [".local/bin", ".bun/bin", ".volta/bin", ".cargo/bin", ".asdf/shims"] {
        out.push(format!("{home}/{rel}"));
    }
    // Version managers keep every Node release in its own directory, so the bin
    // path cannot be written down - enumerate, newest first.
    out.extend(node_version_bins(&format!("{home}/.nvm/versions/node"), "bin"));
    out.extend(node_version_bins(
        &format!("{home}/Library/Application Support/fnm/node-versions"),
        "installation/bin",
    ));
    out.extend(node_version_bins(
        &format!("{home}/.local/share/fnm/node-versions"),
        "installation/bin",
    ));
    out
}

/// The newest few `vMAJOR.MINOR.PATCH` directories under `root`, as bin paths.
/// Sorted numerically: lexically, v9 sorts above v24.
#[cfg(not(target_os = "windows"))]
fn node_version_bins(root: &str, suffix: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut versions: Vec<(u32, u32, u32, String)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let mut parts = name.trim_start_matches('v').split('.');
            let major = parts.next()?.parse().ok()?;
            let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
            let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
            Some((major, minor, patch, name))
        })
        .collect();
    versions.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)));
    versions
        .into_iter()
        .take(3)
        .map(|(_, _, _, name)| format!("{root}/{name}/{suffix}"))
        .collect()
}

/// A tool invocation that can actually find the tool.
async fn tool(program: &str) -> Cmd {
    let mut c = Cmd::new(program);
    c.env("PATH", user_path().await);
    // Without this every probe of node/npm/omniroute pops a cmd.exe window over
    // the app on Windows, and the settings panel probes on open.
    quiet(&mut c);
    c
}

/// npm ships as a shell script on Windows, so it is not directly executable
/// there - it has to go through cmd.exe. Everything else runs it directly.
async fn npm_command() -> Cmd {
    #[cfg(target_os = "windows")]
    {
        let mut c = tool("cmd").await;
        c.args(["/C", "npm"]);
        c
    }
    #[cfg(not(target_os = "windows"))]
    {
        tool("npm").await
    }
}

async fn omniroute_command() -> Cmd {
    #[cfg(target_os = "windows")]
    {
        let mut c = tool("cmd").await;
        c.args(["/C", "omniroute"]);
        c
    }
    #[cfg(not(target_os = "windows"))]
    {
        tool("omniroute").await
    }
}

// ── The app's own copy of OmniRoute ──────────────────────────────────────────
//
// `npm install -g` writes to a prefix most users cannot write to: a default
// nodejs.org install owns /usr/local and a distro package owns /usr/lib, so the
// install died with EACCES on macOS and Linux for exactly the people who
// followed the app's own advice to get Node from nodejs.org. All the app could
// do was tell them to go run npm in a terminal themselves.
//
// So the app keeps its own copy under its data directory. That needs no
// elevation on any platform, leaves the user's global npm prefix alone, and -
// because the entry script then sits at a known absolute path - lets the server
// start as `node <entry>`, with no PATH search and no shell in between. On
// Windows that is what stops a console window from existing, rather than merely
// hiding one.
//
// A pre-existing global install still works: every lookup falls back to it.

/// `<app data>/omniroute` - the npm project the app installs into.
fn managed_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Could not locate the app data directory: {e}"))?
        .join("omniroute"))
}

/// The installed package's entry script and version, read straight out of its
/// `package.json`. One file read replaces spawning `omniroute --version`, and
/// the bin path comes from the package rather than being hardcoded here.
fn managed_pkg(app: &tauri::AppHandle) -> Option<(PathBuf, String)> {
    let dir = managed_root(app).ok()?.join("node_modules").join("omniroute");
    let text = std::fs::read_to_string(dir.join("package.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let version = json.get("version")?.as_str()?.to_string();
    // `bin` is either a bare path or a map of command name → path.
    let rel = match json.get("bin")? {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(m) => m.get("omniroute")?.as_str()?.to_string(),
        _ => return None,
    };
    let entry = dir.join(rel.trim_start_matches("./"));
    entry.is_file().then_some((entry, version))
}

#[derive(serde::Serialize)]
pub struct NodeStatus {
    pub node: Option<String>,
    pub npm: Option<String>,
    pub omniroute: Option<String>,
}

/// What is present on this machine. Never errors: the UI needs to render the
/// gaps, not a failure.
#[tauri::command]
pub async fn omniroute_env(app: tauri::AppHandle) -> Result<NodeStatus, String> {
    async fn version(mut cmd: Cmd) -> Option<String> {
        // A tool that never exits would otherwise hang this command forever, and
        // with it the Install and Start buttons that both begin by calling it.
        let out = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            cmd.arg("--version").output(),
        )
        .await
        .ok()?
        .ok()?;
        if !out.status.success() {
            return None;
        }
        let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if v.is_empty() { None } else { Some(v) }
    }

    let omniroute = match managed_pkg(&app) {
        // The app's own copy: a file read, no process at all.
        Some((_, v)) => Some(v),
        // Otherwise honour a global install the user already had.
        None => version(omniroute_command().await).await,
    };

    Ok(NodeStatus {
        node: version(tool("node").await).await,
        npm: version(npm_command().await).await,
        omniroute,
    })
}

/// Install OmniRoute into the app's own directory, streaming npm's output to the
/// UI as it goes. Not `-g`: see the note above `managed_root`.
#[tauri::command]
pub async fn omniroute_install(app: tauri::AppHandle) -> Result<String, String> {
    let env = omniroute_env(app.clone()).await?;
    if env.node.is_none() {
        return Err(
            "Node.js is not installed. OmniRoute runs on Node - install it from nodejs.org (or your package manager), then try again."
                .to_string(),
        );
    }
    if env.npm.is_none() {
        return Err(
            "npm was not found even though Node.js is installed. Reinstall Node.js so npm comes with it, or install OmniRoute yourself and use the Custom provider."
                .to_string(),
        );
    }

    let root = managed_root(&app)?;
    std::fs::create_dir_all(&root)
        .map_err(|e| format!("Could not create {}: {e}", root.display()))?;
    // npm walks up the tree looking for a package.json when there is none here,
    // and would install into whatever project it found above the data directory.
    let manifest = root.join("package.json");
    if !manifest.exists() {
        std::fs::write(
            &manifest,
            "{\n  \"name\": \"stroke-omniroute\",\n  \"private\": true,\n  \"version\": \"1.0.0\"\n}\n",
        )
        .map_err(|e| format!("Could not write {}: {e}", manifest.display()))?;
    }

    emit(&app, &format!("npm install omniroute  (in {})", root.display()), "cmd");
    // --loglevel http: OmniRoute pulls ~1200 packages and takes minutes, and at
    // npm's default level it prints nothing at all until it finishes. The panel
    // shows the newest log line as a one-line status, so without this it sits on
    // "resolving package…" for the whole install and reads as hung.
    let mut child = npm_command().await
        .args(["install", "omniroute", "--no-fund", "--no-audit", "--loglevel", "http"])
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not run npm: {e}"))?;

    if let Some(out) = child.stdout.take() {
        let app = app.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(out).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                emit(&app, &l, "out");
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let app = app.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                emit(&app, &l, "err");
            }
        });
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("npm install failed: {e}"))?;
    if !status.success() {
        return Err(format!(
            "npm could not install OmniRoute into {}. The log above has npm's own reason - a network or registry failure is the usual one.",
            root.display()
        ));
    }

    let (_, version) = managed_pkg(&app).ok_or_else(|| {
        format!(
            "npm reported success but no OmniRoute package landed in {}.",
            root.join("node_modules").join("omniroute").display()
        )
    })?;
    Ok(version)
}

/// Is the GATEWAY answering - not merely "is the port open".
///
/// A bare TCP connect was not enough: any unrelated dev server on the port
/// answered the handshake and the UI then claimed OmniRoute was running while
/// every model request 404'd. Ask for the endpoint the app actually uses.
async fn gateway_answers(port: u16) -> bool {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(1500))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    match client
        .get(format!("http://127.0.0.1:{port}/v1/models"))
        .send()
        .await
    {
        // 401/403 still means an OpenAI-compatible server is there, just gated.
        Ok(r) => r.status().is_success() || r.status().as_u16() == 401 || r.status().as_u16() == 403,
        Err(_) => false,
    }
}

/// Start the proxy and wait until the port actually accepts connections -
/// spawning the process only proves it launched, not that it is serving.
#[tauri::command]
pub async fn omniroute_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, OmniRouteState>,
    port: u16,
) -> Result<String, String> {
    if gateway_answers(port).await {
        // Already serving: either we started it earlier or the user runs their
        // own. Either way there is nothing to do, and starting a second copy
        // would just fail on the bound port.
        return Ok(format!("http://127.0.0.1:{port}"));
    }

    // Prefer the app's own copy and run its entry script through node directly.
    // An absolute path needs no PATH search and no shell, so on Windows there is
    // no console to suppress in the first place. A global install predating this
    // still starts the old way.
    let mut cmd = match managed_pkg(&app) {
        Some((entry, _)) => {
            emit(&app, &format!("node {} --port {port}", entry.display()), "cmd");
            let mut c = tool("node").await;
            c.arg(entry);
            c
        }
        None => {
            if omniroute_env(app.clone()).await?.omniroute.is_none() {
                return Err("OmniRoute is not installed yet. Install it first.".to_string());
            }
            emit(&app, &format!("omniroute --port {port}"), "cmd");
            omniroute_command().await
        }
    };

    let mut child = cmd
        .args(["--port", &port.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Backstop for the exit hook: if the Child is ever dropped without an
        // explicit kill, take the process down with it.
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Could not start OmniRoute: {e}"))?;

    if let Some(out) = child.stdout.take() {
        let app = app.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(out).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                emit(&app, &l, "out");
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let app = app.clone();
        tokio::spawn(async move {
            let mut lines = tokio::io::BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                emit(&app, &l, "err");
            }
        });
    }

    // Take ownership before waiting, so a Stop during startup still finds it.
    // The lock is released immediately - never held across the await below.
    {
        let mut slot = state.child.lock().map_err(|_| "OmniRoute state poisoned")?;
        if let Some(mut old) = slot.take() {
            let _ = old.start_kill();
        }
        *slot = Some(child);
    }

    for _ in 0..40u32 {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        if gateway_answers(port).await {
            return Ok(format!("http://127.0.0.1:{port}"));
        }
    }
    Err(format!(
        "Started, but the gateway did not answer on port {port} within 10s. It may still be booting, or another process is holding the port."
    ))
}

#[tauri::command]
pub async fn omniroute_stop(state: tauri::State<'_, OmniRouteState>) -> Result<(), String> {
    let child = {
        let mut slot = state.child.lock().map_err(|_| "OmniRoute state poisoned")?;
        slot.take()
    };
    if let Some(mut c) = child {
        let _ = c.kill().await;
    }
    Ok(())
}

/// True when the port is serving, regardless of who started it.
#[tauri::command]
pub async fn omniroute_running(port: u16) -> Result<bool, String> {
    Ok(gateway_answers(port).await)
}
