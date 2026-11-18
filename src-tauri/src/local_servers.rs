// Local database servers: Postgres, MySQL, MariaDB, Redis and MongoDB running
// on this computer, any version side by side, each on its own port.
//
// Two runtimes:
// - Docker: one container per server, `stroke-<id>`, its data in the named
//   volume of the same name, published on 127.0.0.1 only.
// - Native: Postgres binaries downloaded per version (theseus-rs builds, the
//   checksum checked), or the MySQL / Redis already installed on this computer.
//   The data lives in the app's data folder, one directory per server.
//
// Homebrew services (macOS, Linux) are listed beside them, with Start/Stop.
//
// Every server listens on 127.0.0.1 with no password (trust auth, an empty
// root password): it's for development on this machine, like DBngin.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::process::Command;

use crate::proc::quiet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalServer {
    pub id: String,
    pub name: String,
    /// postgres | mysql | mariadb | redis | mongodb
    pub engine: String,
    /// What was asked for ("16", "8.4"); native Postgres also keeps the exact build.
    pub version: String,
    pub port: u16,
    /// docker | native
    pub runtime: String,
    #[serde(default)]
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerState {
    #[serde(flatten)]
    pub server: LocalServer,
    pub running: bool,
    /// Why it can't run right now (Docker not running, binary gone).
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    /// `docker version` answered: the engine is installed and running.
    pub docker: bool,
    /// The CLI exists but the engine didn't answer (Docker Desktop not started).
    pub docker_installed: bool,
    pub brew: bool,
    /// Engines that can run natively here, besides Postgres (always downloadable).
    pub native: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrewService {
    pub name: String,
    pub status: String,
    pub running: bool,
}

// ── Storage ─────────────────────────────────────────────────────────────────

fn root(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("local-servers");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    Ok(dir)
}

fn load(app: &AppHandle) -> Result<Vec<LocalServer>, String> {
    let path = root(app)?.join("servers.json");
    match std::fs::read(&path) {
        Ok(b) => serde_json::from_slice(&b).map_err(|e| format!("{} is damaged: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.to_string()),
    }
}

fn store(app: &AppHandle, servers: &[LocalServer]) -> Result<(), String> {
    let path = root(app)?.join("servers.json");
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(servers).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

fn find(app: &AppHandle, id: &str) -> Result<LocalServer, String> {
    load(app)?.into_iter().find(|s| s.id == id).ok_or_else(|| "That server no longer exists".to_string())
}

fn server_dir(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    Ok(root(app)?.join(id))
}

// ── Engines ─────────────────────────────────────────────────────────────────

pub const ENGINES: &[&str] = &["postgres", "mysql", "mariadb", "redis", "mongodb"];

/// The port each engine listens on inside its container, and by default.
pub fn default_port(engine: &str) -> u16 {
    match engine {
        "postgres" => 5432,
        "mysql" | "mariadb" => 3306,
        "redis" => 6379,
        "mongodb" => 27017,
        _ => 0,
    }
}

fn valid_version(v: &str) -> bool {
    !v.is_empty() && v.len() <= 20 && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// The Docker image for an engine and version, and its environment.
fn docker_image(engine: &str, version: &str) -> Result<(String, Vec<&'static str>), String> {
    if !valid_version(version) {
        return Err(format!("\"{version}\" isn't a version"));
    }
    Ok(match engine {
        "postgres" => (format!("postgres:{version}"), vec!["POSTGRES_HOST_AUTH_METHOD=trust"]),
        "mysql" => (format!("mysql:{version}"), vec!["MYSQL_ALLOW_EMPTY_PASSWORD=yes"]),
        "mariadb" => (format!("mariadb:{version}"), vec!["MARIADB_ALLOW_EMPTY_ROOT_PASSWORD=yes"]),
        "redis" => (format!("redis:{version}"), vec![]),
        "mongodb" => (format!("mongo:{version}"), vec![]),
        _ => return Err(format!("{engine} isn't supported")),
    })
}

/// Where each engine keeps its data inside the container.
fn docker_data_path(engine: &str) -> &'static str {
    match engine {
        "postgres" => "/var/lib/postgresql/data",
        "mysql" | "mariadb" => "/var/lib/mysql",
        "redis" => "/data",
        _ => "/data/db",
    }
}

fn container(id: &str) -> String {
    format!("stroke-{id}")
}

// ── Running commands ────────────────────────────────────────────────────────

/// Run a program to completion; stdout on success, its stderr as the error.
async fn run(program: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(std::process::Stdio::null());
    let out = tokio::time::timeout(Duration::from_secs(600), quiet(&mut cmd).output())
        .await
        .map_err(|_| format!("{} did not finish in time", program.display()))?
        .map_err(|e| format!("Could not run {}: {e}", program.display()))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let err = if err.is_empty() { String::from_utf8_lossy(&out.stdout).trim().to_string() } else { err };
        Err(if err.is_empty() { format!("{} exited with {}", program.display(), out.status) } else { err })
    }
}

/// A program on PATH, or in the usual Homebrew and distro folders the app's
/// PATH misses when it's started from the Dock.
pub fn which(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    for extra in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/usr/sbin", "/usr/local/sbin", "/opt/homebrew/sbin"] {
        dirs.push(PathBuf::from(extra));
    }
    if cfg!(windows) {
        dirs.push(PathBuf::from(r"C:\Program Files\Docker\Docker\resources\bin"));
    }
    if cfg!(target_os = "macos") {
        // Docker Desktop's per-user install and its app bundle, when the
        // /usr/local/bin link wasn't made.
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(home).join(".docker").join("bin"));
        }
        dirs.push(PathBuf::from("/Applications/Docker.app/Contents/Resources/bin"));
        dirs.push(PathBuf::from("/Applications/OrbStack.app/Contents/MacOS/xbin"));
    }
    dirs.into_iter().map(|d| d.join(&exe)).find(|p| p.is_file())
}

fn docker() -> Result<PathBuf, String> {
    which("docker").ok_or_else(|| "Docker isn't installed. Install Docker Desktop, or pick Native.".to_string())
}

async fn docker_up() -> bool {
    match which("docker") {
        Some(d) => run(&d, &["version", "--format", "{{.Server.Version}}"]).await.is_ok(),
        None => false,
    }
}

/// Binaries a native server of this engine needs, when they're installed.
fn native_binary(engine: &str) -> Option<PathBuf> {
    match engine {
        "mysql" => which("mysqld"),
        "mariadb" => which("mariadbd").or_else(|| which("mysqld")),
        "redis" => which("redis-server"),
        "mongodb" => which("mongod"),
        _ => None,
    }
}

/// Is something already listening on this port?
async fn port_busy(port: u16) -> bool {
    tokio::time::timeout(Duration::from_millis(400), tokio::net::TcpStream::connect(("127.0.0.1", port)))
        .await
        .is_ok_and(|r| r.is_ok())
}

/// The first port from `from` up that no server here owns and nothing listens on.
async fn free_port(from: u16, servers: &[LocalServer]) -> u16 {
    let mut p = from.max(1024);
    while p < u16::MAX && (servers.iter().any(|s| s.port == p) || port_busy(p).await) {
        p += 1;
    }
    p
}

/// A port to suggest for a new server: the engine's default, or the next free one.
#[tauri::command]
pub async fn local_server_free_port(app: AppHandle, from: u16) -> Result<u16, String> {
    Ok(free_port(from, &load(&app)?).await)
}

/// Wait until the server accepts connections, up to `secs`.
async fn wait_for_port(port: u16, secs: u64) -> bool {
    let end = std::time::Instant::now() + Duration::from_secs(secs);
    while std::time::Instant::now() < end {
        if port_busy(port).await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    false
}

fn progress(app: &AppHandle, id: &str, message: &str, fraction: Option<f64>) {
    let _ = app.emit("local-server-progress", serde_json::json!({ "id": id, "message": message, "fraction": fraction }));
}

// ── Native Postgres builds ──────────────────────────────────────────────────

fn pg_target() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        _ => return None,
    })
}

/// The newest build of a Postgres major ("16" → "16.11.0").
async fn pg_release(major: &str) -> Result<String, String> {
    let url = "https://api.github.com/repos/theseus-rs/postgresql-binaries/releases?per_page=100";
    let releases: Vec<serde_json::Value> = reqwest::Client::new()
        .get(url)
        .header("user-agent", "stroke-desktop")
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("Could not list Postgres builds: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Could not list Postgres builds: {e}"))?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let mut tags: Vec<Vec<u32>> = releases
        .iter()
        .filter_map(|r| r.get("tag_name")?.as_str())
        .filter(|t| t.split('.').next() == Some(major))
        .map(|t| t.split('.').filter_map(|p| p.parse().ok()).collect())
        .collect();
    tags.sort();
    tags.pop()
        .map(|v| v.iter().map(u32::to_string).collect::<Vec<_>>().join("."))
        .ok_or_else(|| format!("There's no Postgres {major} build for this computer"))
}

/// Download and unpack a Postgres build once; servers of that version share it.
async fn pg_install(app: &AppHandle, id: &str, major: &str) -> Result<PathBuf, String> {
    let target = pg_target().ok_or("Native Postgres isn't available for this computer. Pick Docker.")?;
    let builds = root(app)?.join("builds");
    // A version already unpacked: no network at all.
    if let Ok(rd) = std::fs::read_dir(&builds) {
        let mut have: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&format!("postgresql-{major}.")))
                && pg_bin(p).is_some()
        }).collect();
        have.sort();
        if let Some(p) = have.pop() {
            return Ok(p);
        }
    }
    progress(app, id, &format!("Finding the newest Postgres {major}"), None);
    let version = pg_release(major).await?;
    let name = format!("postgresql-{version}-{target}.tar.gz");
    let base = format!("https://github.com/theseus-rs/postgresql-binaries/releases/download/{version}/{name}");
    let client = reqwest::Client::builder().user_agent("stroke-desktop").build().map_err(|e| e.to_string())?;
    let sum = client.get(format!("{base}.sha256")).send().await.and_then(|r| r.error_for_status()).map_err(|e| e.to_string())?
        .text().await.map_err(|e| e.to_string())?;
    let want = sum.split_whitespace().next().unwrap_or_default().to_ascii_lowercase();

    let mut resp = client.get(&base).send().await.and_then(|r| r.error_for_status()).map_err(|e| format!("Could not download Postgres {version}: {e}"))?;
    let total = resp.content_length();
    let mut bytes = Vec::with_capacity(total.unwrap_or(0) as usize);
    let mut last = 0.0;
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("Download stopped: {e}"))? {
        bytes.extend_from_slice(&chunk);
        if let Some(t) = total {
            let f = bytes.len() as f64 / t as f64;
            if f - last >= 0.02 {
                last = f;
                progress(app, id, &format!("Downloading Postgres {version}"), Some(f));
            }
        }
    }
    use sha2::Digest;
    let got = hex::encode(sha2::Sha256::digest(&bytes));
    if want.len() == 64 && got != want {
        return Err("The Postgres download didn't match its checksum. Try again.".into());
    }
    progress(app, id, &format!("Unpacking Postgres {version}"), None);
    let dest = builds.join(format!("postgresql-{version}"));
    let tmp = builds.join(format!(".postgresql-{version}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let unpack = tmp.clone();
    tokio::task::spawn_blocking(move || {
        tar::Archive::new(flate2::read::GzDecoder::new(bytes.as_slice())).unpack(&unpack)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("Could not unpack Postgres: {e}"))?;
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    pg_bin(&dest).ok_or("The Postgres download has no pg_ctl")?;
    Ok(dest)
}

/// The `bin` folder inside an unpacked build (it sits one folder down).
fn pg_bin(build: &Path) -> Option<PathBuf> {
    let exe = if cfg!(windows) { "pg_ctl.exe" } else { "pg_ctl" };
    let direct = build.join("bin");
    if direct.join(exe).is_file() {
        return Some(direct);
    }
    std::fs::read_dir(build).ok()?.flatten().map(|e| e.path().join("bin")).find(|b| b.join(exe).is_file())
}

fn bin_exe(dir: &Path, name: &str) -> PathBuf {
    dir.join(if cfg!(windows) { format!("{name}.exe") } else { name.to_string() })
}

/// The build a native Postgres server runs: recorded at create time.
fn pg_build_of(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    let p = std::fs::read_to_string(server_dir(app, id)?.join("build")).map_err(|_| "This server's Postgres build is missing. Delete it and create it again.".to_string())?;
    let build = PathBuf::from(p.trim());
    pg_bin(&build).ok_or_else(|| "This server's Postgres build is missing. Delete it and create it again.".to_string())
}

// ── Native process bookkeeping ──────────────────────────────────────────────

fn pid_file(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    Ok(server_dir(app, id)?.join("server.pid"))
}

fn pid_alive(pid: u32) -> bool {
    use sysinfo::{Pid, ProcessesToUpdate, System};
    let mut sys = System::new();
    let p = Pid::from_u32(pid);
    sys.refresh_processes(ProcessesToUpdate::Some(&[p]), true);
    sys.process(p).is_some()
}

fn read_pid(app: &AppHandle, id: &str) -> Option<u32> {
    std::fs::read_to_string(pid_file(app, id).ok()?).ok()?.lines().next()?.trim().parse().ok()
}

fn kill_pid(pid: u32) {
    use sysinfo::{Pid, ProcessesToUpdate, Signal, System};
    let mut sys = System::new();
    let p = Pid::from_u32(pid);
    sys.refresh_processes(ProcessesToUpdate::Some(&[p]), true);
    if let Some(proc_) = sys.process(p) {
        if proc_.kill_with(Signal::Term).is_none() {
            proc_.kill();
        }
    }
}

/// Start a long-running server process, detached from the app, its output in `log`.
fn spawn_detached(program: &Path, args: &[String], log: &Path) -> Result<u32, String> {
    let out = std::fs::File::create(log).map_err(|e| e.to_string())?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(program);
    cmd.args(args).stdin(std::process::Stdio::null()).stdout(out).stderr(err);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own process group: closing the app or Ctrl+C in a terminal leaves it running.
        cmd.process_group(0);
    }
    crate::proc::quiet_std(&mut cmd);
    let child = cmd.spawn().map_err(|e| format!("Could not start {}: {e}", program.display()))?;
    Ok(child.id())
}

fn log_tail(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().rev().take(6).collect();
    lines.into_iter().rev().collect::<Vec<_>>().join("\n")
}

// ── Status ──────────────────────────────────────────────────────────────────

async fn docker_running(id: &str) -> Result<bool, String> {
    let d = docker()?;
    match run(&d, &["inspect", "-f", "{{.State.Running}}", &container(id)]).await {
        Ok(s) => Ok(s.trim() == "true"),
        Err(e) if e.contains("No such") => Err("The container is gone. Delete this server and create it again.".into()),
        Err(e) => Err(e),
    }
}

async fn state_of(app: &AppHandle, s: LocalServer, docker_ok: bool) -> ServerState {
    let (running, problem) = if s.runtime == "docker" {
        if !docker_ok {
            (false, Some("Docker isn't running".to_string()))
        } else {
            match docker_running(&s.id).await {
                Ok(r) => (r, None),
                Err(e) => (false, Some(e)),
            }
        }
    } else if s.engine == "postgres" {
        match pg_build_of(app, &s.id) {
            Ok(bin) => {
                let data = server_dir(app, &s.id).map(|d| d.join("data")).unwrap_or_default();
                let up = run(&bin_exe(&bin, "pg_ctl"), &["status", "-D", &data.to_string_lossy()]).await.is_ok();
                (up, None)
            }
            Err(e) => (false, Some(e)),
        }
    } else {
        let up = read_pid(app, &s.id).is_some_and(pid_alive);
        let problem = if up || native_binary(&s.engine).is_some() { None } else { Some(format!("{} isn't installed anymore", s.engine)) };
        (up, problem)
    };
    ServerState { server: s, running, problem }
}

// ── Commands ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn local_servers_env() -> Result<Environment, String> {
    let docker_installed = which("docker").is_some();
    let docker = docker_installed && docker_up().await;
    let native = ["mysql", "mariadb", "redis", "mongodb"]
        .into_iter()
        .filter(|e| native_binary(e).is_some())
        .map(String::from)
        .collect();
    Ok(Environment { docker, docker_installed, brew: which("brew").is_some(), native })
}

#[tauri::command]
pub async fn local_servers_list(app: AppHandle) -> Result<Vec<ServerState>, String> {
    let servers = load(&app)?;
    let docker_ok = if servers.iter().any(|s| s.runtime == "docker") { docker_up().await } else { false };
    let mut out = Vec::with_capacity(servers.len());
    for s in servers {
        out.push(state_of(&app, s, docker_ok).await);
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewServer {
    pub name: String,
    pub engine: String,
    pub version: String,
    pub port: u16,
    pub runtime: String,
}

/// Check a new server before anything is downloaded or created.
fn validate(spec: &NewServer, existing: &[LocalServer]) -> Result<(), String> {
    if !ENGINES.contains(&spec.engine.as_str()) {
        return Err(format!("{} isn't supported", spec.engine));
    }
    if !valid_version(&spec.version) {
        return Err(format!("\"{}\" isn't a version", spec.version));
    }
    if spec.port < 1024 {
        return Err("Pick a port from 1024 up".into());
    }
    if spec.name.trim().is_empty() || spec.name.len() > 60 {
        return Err("Give the server a name".into());
    }
    if let Some(other) = existing.iter().find(|s| s.port == spec.port) {
        return Err(format!("{} already uses port {}", other.name, spec.port));
    }
    match spec.runtime.as_str() {
        "docker" => Ok(()),
        "native" if spec.engine == "postgres" => {
            if spec.version.contains('.') {
                Err("Native Postgres takes a major version, like 16".into())
            } else {
                Ok(())
            }
        }
        "native" if native_binary(&spec.engine).is_some() => Ok(()),
        "native" => Err(format!("{} isn't installed on this computer. Pick Docker.", spec.engine)),
        other => Err(format!("Unknown runtime {other}")),
    }
}

#[tauri::command]
pub async fn local_server_create(app: AppHandle, spec: NewServer) -> Result<LocalServer, String> {
    let mut servers = load(&app)?;
    validate(&spec, &servers)?;
    // Before any download: a port something else listens on can never start.
    if port_busy(spec.port).await {
        let free = free_port(spec.port + 1, &servers).await;
        return Err(format!("Port {} is in use on this computer. Try {free}.", spec.port));
    }
    let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
    let server = LocalServer {
        id: id.clone(),
        name: spec.name.trim().to_string(),
        engine: spec.engine.clone(),
        version: spec.version.clone(),
        port: spec.port,
        runtime: spec.runtime.clone(),
        created_at: chrono::Utc::now().timestamp_millis(),
    };
    let dir = server_dir(&app, &id)?;
    let made = async {
        if server.runtime == "docker" {
            let d = docker()?;
            if !docker_up().await {
                return Err("Docker isn't running. Start Docker Desktop and try again.".into());
            }
            let (image, env) = docker_image(&server.engine, &server.version)?;
            progress(&app, &id, &format!("Pulling {image}"), None);
            run(&d, &["pull", &image]).await?;
            let publish = format!("127.0.0.1:{}:{}", server.port, default_port(&server.engine));
            let volume = format!("{}:{}", container(&id), docker_data_path(&server.engine));
            let name = container(&id);
            let mut args = vec!["create", "--name", &name, "-p", &publish, "-v", &volume, "--label", "app=stroke"];
            for e in &env {
                args.push("-e");
                args.push(e);
            }
            args.push(&image);
            progress(&app, &id, "Creating the container", None);
            run(&d, &args).await?;
        } else {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let data = dir.join("data");
            match server.engine.as_str() {
                "postgres" => {
                    let build = pg_install(&app, &id, &server.version).await?;
                    std::fs::write(dir.join("build"), build.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
                    let bin = pg_bin(&build).ok_or("The Postgres build has no bin folder")?;
                    progress(&app, &id, "Creating the database cluster", None);
                    run(&bin_exe(&bin, "initdb"), &["-D", &data.to_string_lossy(), "-U", "postgres", "-A", "trust", "-E", "UTF8", "--no-locale"]).await?;
                }
                "mysql" | "mariadb" => {
                    let bin = native_binary(&server.engine).ok_or("mysqld isn't installed")?;
                    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
                    progress(&app, &id, "Creating the data directory", None);
                    let datadir = format!("--datadir={}", data.to_string_lossy());
                    if server.engine == "mariadb" || bin.file_name().is_some_and(|n| n.to_string_lossy().starts_with("mariadb")) {
                        let install = which("mariadb-install-db").or_else(|| which("mysql_install_db")).ok_or("mariadb-install-db isn't installed")?;
                        run(&install, &[&datadir, "--auth-root-authentication-method=normal"]).await?;
                    } else {
                        run(&bin, &["--initialize-insecure", &datadir]).await?;
                    }
                }
                _ => {
                    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
                }
            }
        }
        Ok::<(), String>(())
    }
    .await;
    if let Err(e) = made {
        let _ = std::fs::remove_dir_all(&dir);
        if server.runtime == "docker" {
            if let Ok(d) = docker() {
                let _ = run(&d, &["rm", "-f", &container(&id)]).await;
            }
        }
        return Err(e);
    }
    servers.push(server.clone());
    store(&app, &servers)?;
    progress(&app, &id, "Ready", Some(1.0));
    Ok(server)
}

#[tauri::command]
pub async fn local_server_start(app: AppHandle, id: String) -> Result<(), String> {
    let s = find(&app, &id)?;
    if port_busy(s.port).await {
        if state_of(&app, s.clone(), true).await.running {
            return Ok(());
        }
        return Err(format!("Something else is using port {}. Stop it, or change this server's port.", s.port));
    }
    let dir = server_dir(&app, &id)?;
    let data = dir.join("data");
    let log = dir.join("server.log");
    if s.runtime == "docker" {
        run(&docker()?, &["start", &container(&id)]).await?;
    } else {
        match s.engine.as_str() {
            "postgres" => {
                let bin = pg_build_of(&app, &id)?;
                let mut opts = format!("-p {} -c listen_addresses=127.0.0.1", s.port);
                if cfg!(unix) {
                    // No Unix socket: Stroke connects over TCP, two versions can't
                    // fight over /tmp/.s.PGSQL.5432, and a socket path under
                    // macOS's "Application Support" can pass the 107-byte limit.
                    opts.push_str(" -k \"\"");
                }
                run(&bin_exe(&bin, "pg_ctl"), &["start", "-D", &data.to_string_lossy(), "-l", &log.to_string_lossy(), "-o", &opts, "-w", "-t", "60"])
                    .await
                    .map_err(|e| format!("{e}\n{}", log_tail(&log)))?;
            }
            "mysql" | "mariadb" => {
                let bin = native_binary(&s.engine).ok_or("mysqld isn't installed")?;
                let mut args = vec![
                    format!("--datadir={}", data.to_string_lossy()),
                    format!("--port={}", s.port),
                    "--bind-address=127.0.0.1".to_string(),
                    format!("--pid-file={}", dir.join("mysqld.pid").to_string_lossy()),
                ];
                if cfg!(unix) {
                    args.push(format!("--socket={}", dir.join("mysql.sock").to_string_lossy()));
                }
                let pid = spawn_detached(&bin, &args, &log)?;
                std::fs::write(pid_file(&app, &id)?, pid.to_string()).map_err(|e| e.to_string())?;
            }
            "redis" => {
                let bin = native_binary("redis").ok_or("redis-server isn't installed")?;
                let args = vec![
                    "--port".to_string(), s.port.to_string(),
                    "--bind".to_string(), "127.0.0.1".to_string(),
                    "--dir".to_string(), data.to_string_lossy().to_string(),
                    "--save".to_string(), "60 1".to_string(),
                ];
                let pid = spawn_detached(&bin, &args, &log)?;
                std::fs::write(pid_file(&app, &id)?, pid.to_string()).map_err(|e| e.to_string())?;
            }
            "mongodb" => {
                let bin = native_binary("mongodb").ok_or("mongod isn't installed")?;
                let args = vec![
                    "--port".to_string(), s.port.to_string(),
                    "--bind_ip".to_string(), "127.0.0.1".to_string(),
                    "--dbpath".to_string(), data.to_string_lossy().to_string(),
                ];
                let pid = spawn_detached(&bin, &args, &log)?;
                std::fs::write(pid_file(&app, &id)?, pid.to_string()).map_err(|e| e.to_string())?;
            }
            e => return Err(format!("{e} isn't supported")),
        }
    }
    if !wait_for_port(s.port, 60).await {
        let tail = if s.runtime == "docker" {
            run(&docker()?, &["logs", "--tail", "6", &container(&id)]).await.unwrap_or_default()
        } else {
            log_tail(&log)
        };
        return Err(format!("{} started but isn't answering on port {}.\n{tail}", s.name, s.port).trim().to_string());
    }
    Ok(())
}


#[tauri::command]
pub async fn local_server_stop(app: AppHandle, id: String) -> Result<(), String> {
    let s = find(&app, &id)?;
    if s.runtime == "docker" {
        run(&docker()?, &["stop", &container(&id)]).await?;
        return Ok(());
    }
    if s.engine == "postgres" {
        let bin = pg_build_of(&app, &id)?;
        let data = server_dir(&app, &id)?.join("data");
        run(&bin_exe(&bin, "pg_ctl"), &["stop", "-D", &data.to_string_lossy(), "-m", "fast", "-w"]).await?;
        return Ok(());
    }
    if let Some(pid) = read_pid(&app, &id) {
        kill_pid(pid);
        for _ in 0..50 {
            if !pid_alive(pid) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let _ = std::fs::remove_file(pid_file(&app, &id)?);
    }
    Ok(())
}

#[tauri::command]
pub async fn local_server_delete(app: AppHandle, id: String) -> Result<(), String> {
    let s = find(&app, &id)?;
    let _ = local_server_stop(app.clone(), id.clone()).await;
    if s.runtime == "docker" {
        if let Ok(d) = docker() {
            let name = container(&id);
            let _ = run(&d, &["rm", "-f", &name]).await;
            let _ = run(&d, &["volume", "rm", &name]).await;
        }
    }
    let _ = std::fs::remove_dir_all(server_dir(&app, &id)?);
    let servers: Vec<LocalServer> = load(&app)?.into_iter().filter(|x| x.id != id).collect();
    store(&app, &servers)
}

/// Rename a server or move it to another port (it has to be stopped for a port change).
#[tauri::command]
pub async fn local_server_update(app: AppHandle, id: String, name: Option<String>, port: Option<u16>) -> Result<LocalServer, String> {
    let mut servers = load(&app)?;
    let idx = servers.iter().position(|s| s.id == id).ok_or("That server no longer exists")?;
    if let Some(n) = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) {
        servers[idx].name = n;
    }
    if let Some(p) = port.filter(|p| *p != servers[idx].port) {
        if p < 1024 {
            return Err("Pick a port from 1024 up".into());
        }
        if let Some(o) = servers.iter().find(|s| s.port == p) {
            return Err(format!("{} already uses port {p}", o.name));
        }
        if state_of(&app, servers[idx].clone(), docker_up().await).await.running {
            return Err("Stop the server before changing its port".into());
        }
        if servers[idx].runtime == "docker" {
            // A container's published port is fixed: make a new one on the same volume.
            let s = servers[idx].clone();
            let d = docker()?;
            let (image, env) = docker_image(&s.engine, &s.version)?;
            let name = container(&id);
            run(&d, &["rm", "-f", &name]).await?;
            let publish = format!("127.0.0.1:{p}:{}", default_port(&s.engine));
            let volume = format!("{name}:{}", docker_data_path(&s.engine));
            let mut args = vec!["create", "--name", &name, "-p", &publish, "-v", &volume, "--label", "app=stroke"];
            for e in &env {
                args.push("-e");
                args.push(e);
            }
            args.push(&image);
            run(&d, &args).await?;
        }
        servers[idx].port = p;
    }
    store(&app, &servers)?;
    Ok(servers[idx].clone())
}

/// The last lines a server wrote, for when it won't start.
#[tauri::command]
pub async fn local_server_logs(app: AppHandle, id: String) -> Result<String, String> {
    let s = find(&app, &id)?;
    if s.runtime == "docker" {
        let d = docker()?;
        let mut cmd = Command::new(&d);
        cmd.args(["logs", "--tail", "200", &container(&id)]);
        let out = quiet(&mut cmd).output().await.map_err(|e| e.to_string())?;
        // Server images log to stderr; keep both, in order of the streams.
        return Ok(format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)));
    }
    let text = std::fs::read_to_string(server_dir(&app, &id)?.join("server.log")).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    Ok(lines[lines.len().saturating_sub(200)..].join("\n"))
}

// ── Homebrew ────────────────────────────────────────────────────────────────

/// Database services Homebrew knows about (postgresql@16, mysql, redis, ...).
#[tauri::command]
pub async fn brew_services_list() -> Result<Vec<BrewService>, String> {
    let Some(brew) = which("brew") else { return Ok(Vec::new()) };
    let out = run(&brew, &["services", "list", "--json"]).await?;
    let v: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap_or_default();
    Ok(v.into_iter()
        .filter_map(|s| {
            let name = s.get("name")?.as_str()?.to_string();
            let status = s.get("status").and_then(|x| x.as_str()).unwrap_or("none").to_string();
            let db = ["postgres", "mysql", "mariadb", "redis", "mongo", "valkey", "memcached", "clickhouse"].iter().any(|k| name.contains(k));
            db.then(|| BrewService { running: status == "started", name, status })
        })
        .collect())
}

#[tauri::command]
pub async fn brew_service_action(name: String, action: String) -> Result<(), String> {
    if !matches!(action.as_str(), "start" | "stop" | "restart") {
        return Err(format!("Unknown action {action}"));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || "@.-_".contains(c)) {
        return Err("That isn't a Homebrew service".into());
    }
    let brew = which("brew").ok_or("Homebrew isn't installed")?;
    run(&brew, &["services", &action, &name]).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(engine: &str, version: &str, port: u16, runtime: &str) -> NewServer {
        NewServer { name: "t".into(), engine: engine.into(), version: version.into(), port, runtime: runtime.into() }
    }

    #[test]
    fn new_servers_are_checked_first() {
        assert!(validate(&spec("postgres", "16", 5433, "docker"), &[]).is_ok());
        assert!(validate(&spec("postgres", "16", 5433, "native"), &[]).is_ok());
        assert!(validate(&spec("postgres", "16.4", 5433, "native"), &[]).unwrap_err().contains("major"));
        assert!(validate(&spec("oracle", "23", 1521, "docker"), &[]).is_err());
        assert!(validate(&spec("redis", "7; rm -rf /", 6380, "docker"), &[]).is_err());
        assert!(validate(&spec("redis", "7", 80, "docker"), &[]).unwrap_err().contains("1024"));
        let taken = LocalServer { id: "a".into(), name: "Air".into(), engine: "postgres".into(), version: "16".into(), port: 5432, runtime: "docker".into(), created_at: 0 };
        assert!(validate(&spec("mysql", "8.4", 5432, "docker"), &[taken]).unwrap_err().contains("Air"));
    }

    #[test]
    fn docker_images_and_ports() {
        assert_eq!(docker_image("mongodb", "8.0").unwrap().0, "mongo:8.0");
        assert_eq!(docker_image("postgres", "17").unwrap().1, vec!["POSTGRES_HOST_AUTH_METHOD=trust"]);
        assert!(docker_image("mysql", "8.4 --privileged").is_err());
        assert_eq!(default_port("mariadb"), 3306);
        assert_eq!(docker_data_path("mongodb"), "/data/db");
    }

    #[test]
    fn a_postgres_build_is_found_one_folder_down() {
        let dir = std::env::temp_dir().join(format!("stroke-pgbin-{}", std::process::id()));
        let bin = dir.join("postgresql-16.0.0-x").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join(if cfg!(windows) { "pg_ctl.exe" } else { "pg_ctl" }), b"").unwrap();
        assert_eq!(pg_bin(&dir), Some(bin));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
