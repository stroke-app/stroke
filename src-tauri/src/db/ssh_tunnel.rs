use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// SSH tunnel configuration - serialized alongside saved connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConfig {
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub username: String,
    /// Path to an identity file (e.g. `~/.ssh/id_rsa`).
    /// Leave empty to rely on the system SSH agent.
    #[serde(default)]
    pub private_key_path: String,
    /// `ServerAliveInterval`, in seconds. A tunnel that sits idle behind a NAT
    /// or a load balancer is dropped silently, and the first query after that
    /// is what discovers it; a keepalive is how long the tunnel waits before
    /// proving it is still there. 0 disables it.
    #[serde(default = "default_keepalive")]
    pub keepalive: u16,
}

fn default_keepalive() -> u16 {
    30
}

fn default_ssh_port() -> u16 {
    22
}

// ── Loss tracking ─────────────────────────────────────────────────────────────
//
// A query that fails because its tunnel went away has two very different
// meanings. If the tunnel was already down when it started, nothing reached the
// server. If the tunnel dropped while it ran, nobody knows: a write may have
// committed before the reply was lost. The query paths read these counters
// around a run (`RunWatch`) and say which one happened.

/// Bumped every time a tunnel's ssh process exits on its own.
static LOSSES: AtomicU64 = AtomicU64::new(0);
/// Tunnels currently down and reconnecting.
static DOWN: AtomicUsize = AtomicUsize::new(0);
/// For the `tunnel-status` events the window listens to.
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

pub fn init(app: tauri::AppHandle) {
    let _ = APP.set(app);
}

/// Taken before a query runs; asked about its error afterwards.
#[derive(Clone, Copy)]
pub struct RunWatch {
    losses: u64,
    down: bool,
}

impl RunWatch {
    pub fn start() -> Self {
        Self { losses: LOSSES.load(Ordering::SeqCst), down: DOWN.load(Ordering::SeqCst) > 0 }
    }

    /// Reword a failed run's error when a tunnel explains it. `writes` is
    /// whether the SQL could have changed anything.
    pub fn explain(self, err: String, writes: bool) -> String {
        if err == super::query::QUERY_CANCELLED {
            return err;
        }
        if self.down {
            return format!("The SSH tunnel is down and reconnecting, so this did not reach the server. Run it again once the tunnel is back. ({err})");
        }
        if LOSSES.load(Ordering::SeqCst) != self.losses {
            return if writes {
                format!("Result unknown: the SSH tunnel dropped while this ran, so it may or may not have been applied. Check the data before running it again. ({err})")
            } else {
                format!("The SSH tunnel dropped while this ran. Nothing was changed; run it again once the tunnel is back. ({err})")
            };
        }
        err
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TunnelEvent {
    /// `user@host:port` of the SSH server.
    via: String,
    /// "down" (lost, reconnecting), "retrying" (a reconnect failed), "up",
    /// or "closed" (the connection was left while it was down).
    state: &'static str,
    message: Option<String>,
}

fn announce(via: &str, state: &'static str, message: Option<String>) {
    if let Some(app) = APP.get() {
        let _ = app.emit("tunnel-status", TunnelEvent { via: via.to_string(), state, message });
    }
}

// ── SshTunnel ─────────────────────────────────────────────────────────────────

/// A live SSH local-port-forward tunnel backed by a system `ssh -N -L` process,
/// watched by a supervisor that rebuilds it on the same local port when the
/// process exits. Dropping the struct stops the supervisor, which kills ssh.
pub struct SshTunnel {
    pub local_port: u16,
    supervisor: tauri::async_runtime::JoinHandle<()>,
    /// This tunnel is down and its supervisor is reconnecting.
    down: Arc<std::sync::atomic::AtomicBool>,
    via: String,
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        // The child is spawned with kill_on_drop, and the supervisor owns it.
        self.supervisor.abort();
        // Closed while down (the connection was left mid-reconnect): it no
        // longer counts as down, and the window's "reconnecting" toast goes.
        // Left counted, every later failed query read as a tunnel failure.
        if self.down.swap(false, Ordering::SeqCst) {
            DOWN.fetch_sub(1, Ordering::SeqCst);
            announce(&self.via, "closed", None);
        }
    }
}

fn find_free_port() -> Result<u16, String> {
    // Bind to :0 - the OS assigns an ephemeral port - then release the listener.
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("Cannot find a free local port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| e.to_string())?
        .port();
    Ok(port)
}

/// The last lines ssh wrote to stderr: the only place it says *why*.
type StderrTail = Arc<Mutex<VecDeque<String>>>;

fn args_for(ssh: &SshConfig, forward: &str) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-N".into(),
        // Kill immediately if port-forward binding fails
        "-o".into(), "ExitOnForwardFailure=yes".into(),
        // Never prompt: there is no terminal to answer, so a passphrase or
        // password prompt would hang until the deadline and then say nothing.
        "-o".into(), "BatchMode=yes".into(),
        // Accept new hosts but reject changed fingerprints
        "-o".into(), "StrictHostKeyChecking=accept-new".into(),
        "-o".into(), "ServerAliveCountMax=3".into(),
        "-o".into(), "ConnectTimeout=10".into(),
        "-p".into(), ssh.port.to_string(),
        "-L".into(), forward.to_string(),
    ];
    // Keep-alive so long-idle tunnels don't stall the pool. Configurable
    // because the useful value is a property of the network in between, not
    // of the database: 30s is right behind a NAT, pointless on a LAN, and
    // too slow for a load balancer that reaps at 20.
    if ssh.keepalive > 0 {
        args.push("-o".into());
        args.push(format!("ServerAliveInterval={}", ssh.keepalive));
    }
    if !ssh.private_key_path.trim().is_empty() {
        args.push("-i".into());
        args.push(ssh.private_key_path.clone());
    }
    // Tests run against a throwaway sshd and must not write to ~/.ssh.
    #[cfg(test)]
    if let Ok(file) = std::env::var("STROKE_TEST_KNOWN_HOSTS") {
        args.push("-o".into());
        args.push(format!("UserKnownHostsFile={file}"));
    }
    args.push(format!("{}@{}", ssh.username, ssh.host));
    args
}

/// Turn what ssh printed into a sentence someone can act on.
fn explain_ssh_failure(ssh: &SshConfig, tail: &[String], fallback: &str) -> String {
    let all = tail.join("\n");
    let has = |needle: &str| all.contains(needle);
    let who = format!("{}@{}:{}", ssh.username, ssh.host, ssh.port);
    let reason = if has("REMOTE HOST IDENTIFICATION HAS CHANGED") || has("Host key verification failed") {
        format!("The host key for {} has changed since it was last seen. If the server was rebuilt, remove its old entry from ~/.ssh/known_hosts; otherwise do not connect.", ssh.host)
    } else if has("Permission denied") {
        if ssh.private_key_path.trim().is_empty() {
            format!("{who} rejected the login. No key file is set, so ssh used your agent: check the key is loaded (ssh-add -l) and allowed on the server.")
        } else {
            format!("{who} rejected the key {}. Check it is the right key and that it has no passphrase, or load it into your SSH agent.", ssh.private_key_path)
        }
    } else if has("Could not resolve hostname") {
        format!("Could not find the SSH host {}. Check the name.", ssh.host)
    } else if has("Connection refused") {
        format!("{} refused the SSH connection on port {}. Check the port and that sshd is running.", ssh.host, ssh.port)
    } else if has("timed out") {
        format!("No answer from {} on port {} within 10 seconds. Check the host is reachable and the port is not blocked.", ssh.host, ssh.port)
    } else if has("Address already in use") || has("cannot listen to port") {
        "The local end of the tunnel could not bind its port. Another program may have taken it.".to_string()
    } else if has("Identity file") && has("not accessible") {
        format!("The key file {} could not be read. Check the path.", ssh.private_key_path)
    } else {
        return match tail.iter().rev().find(|l| !l.trim().is_empty()) {
            Some(line) => format!("SSH tunnel to {who} failed: {}", line.trim()),
            None => fallback.to_string(),
        };
    };
    reason
}

/// Spawn ssh forwarding `local_port` and wait until the port answers, ssh
/// exits (fail with its own reason), or the deadline passes.
async fn spawn_ready(ssh: &SshConfig, local_port: u16, db_host: &str, db_port: u16) -> Result<(Child, StderrTail), String> {
    let forward = format!("{local_port}:{db_host}:{db_port}");
    let mut cmd = Command::new("ssh");
    cmd.args(args_for(ssh, &forward))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    // quiet: the tunnel is a background process the user never interacts with,
    // so on Windows it must not get a console window of its own.
    let mut child = crate::proc::quiet(&mut cmd)
        .spawn()
        .map_err(|e| format!("Failed to launch SSH: {e}. Ensure the 'ssh' binary is on your PATH."))?;

    let tail: StderrTail = Arc::new(Mutex::new(VecDeque::new()));
    if let Some(stderr) = child.stderr.take() {
        let tail = tail.clone();
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut t = tail.lock().unwrap_or_else(|e| e.into_inner());
                if t.len() == 20 {
                    t.pop_front();
                }
                t.push_back(line);
            }
        });
    }

    // Poll until the forwarded port accepts connections (up to 10 s).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(Some(_)) = child.try_wait() {
            // Give the reader a moment to collect the last lines.
            tokio::time::sleep(Duration::from_millis(100)).await;
            let lines: Vec<String> = tail.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect();
            return Err(explain_ssh_failure(ssh, &lines, &format!("SSH exited before the tunnel to {}@{} was up.", ssh.username, ssh.host)));
        }
        if tokio::net::TcpStream::connect(format!("127.0.0.1:{local_port}")).await.is_ok() {
            return Ok((child, tail));
        }
        if tokio::time::Instant::now() >= deadline {
            let lines: Vec<String> = tail.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect();
            return Err(explain_ssh_failure(
                ssh,
                &lines,
                &format!(
                    "SSH tunnel to {}@{} timed out - check credentials, host reachability, and that port {} is not blocked",
                    ssh.username, ssh.host, ssh.port
                ),
            ));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Backoff between reconnect attempts: 1, 2, 4, 8, 16, then every 30 s.
fn backoff(attempt: u32) -> Duration {
    Duration::from_secs((1u64 << attempt.min(5)).min(30))
}

impl SshTunnel {
    /// Spawn `ssh -N -L {local_port}:{db_host}:{db_port} …`, block until the
    /// forwarded port is reachable, then supervise it for as long as the
    /// tunnel lives.
    pub async fn establish(ssh: &SshConfig, db_host: &str, db_port: u16) -> Result<Self, String> {
        let local_port = find_free_port()?;
        let (child, tail) = spawn_ready(ssh, local_port, db_host, db_port).await?;
        let down = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let supervisor = tauri::async_runtime::spawn(supervise(ssh.clone(), local_port, db_host.to_string(), db_port, child, tail, down.clone()));
        Ok(SshTunnel { local_port, supervisor, down, via: format!("{}@{}:{}", ssh.username, ssh.host, ssh.port) })
    }
}

/// Wait on ssh; when it exits on its own, say so and bring it back on the same
/// local port, so the pool's address stays valid and its keepalive replaces the
/// dead connections without anyone reconnecting by hand.
async fn supervise(
    ssh: SshConfig,
    local_port: u16,
    db_host: String,
    db_port: u16,
    mut child: Child,
    mut tail: StderrTail,
    down: Arc<std::sync::atomic::AtomicBool>,
) {
    let via = format!("{}@{}:{}", ssh.username, ssh.host, ssh.port);
    loop {
        let _ = child.wait().await;
        LOSSES.fetch_add(1, Ordering::SeqCst);
        if !down.swap(true, Ordering::SeqCst) {
            DOWN.fetch_add(1, Ordering::SeqCst);
        }
        let lines: Vec<String> = tail.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect();
        let why = explain_ssh_failure(&ssh, &lines, "The SSH connection closed.");
        log::info!("ssh tunnel {via} lost: {why}");
        announce(&via, "down", Some(why));

        let mut attempt = 0;
        loop {
            tokio::time::sleep(backoff(attempt)).await;
            match spawn_ready(&ssh, local_port, &db_host, db_port).await {
                Ok((c, t)) => {
                    child = c;
                    tail = t;
                    break;
                }
                Err(e) => {
                    attempt += 1;
                    announce(&via, "retrying", Some(e));
                }
            }
        }
        if down.swap(false, Ordering::SeqCst) {
            DOWN.fetch_sub(1, Ordering::SeqCst);
        }
        log::info!("ssh tunnel {via} back on 127.0.0.1:{local_port}");
        announce(&via, "up", None);
    }
}

// ── HTTP engines through a tunnel ─────────────────────────────────────────────

/// A base URL pointed through `via`: the port becomes the tunnel's, and the
/// host stays the server's name (TLS checks it; `http_client` resolves it to
/// the tunnel). An IP address has no name to keep, so it becomes 127.0.0.1.
pub fn url_via(mut url: reqwest::Url, via: Option<std::net::SocketAddr>) -> reqwest::Url {
    let Some(via) = via else { return url };
    let is_ip = url.host_str().map(|h| h.trim_matches(['[', ']'])).and_then(|h| h.parse::<std::net::IpAddr>().ok()).is_some();
    if is_ip {
        let _ = url.set_host(Some("127.0.0.1"));
    }
    let _ = url.set_port(Some(via.port()));
    url
}

/// The HTTP client for a server reached through `via`: its name resolves to
/// the tunnel. One per (name, tunnel), kept. None without a tunnel, so the
/// caller keeps its shared client.
pub fn http_client(host: &str, via: Option<std::net::SocketAddr>, insecure: bool) -> Option<reqwest::Client> {
    static CLIENTS: std::sync::LazyLock<Mutex<std::collections::HashMap<(String, std::net::SocketAddr, bool), reqwest::Client>>> =
        std::sync::LazyLock::new(Default::default);
    let via = via?;
    let key = (host.to_string(), via, insecure);
    let mut map = CLIENTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(c) = map.get(&key) {
        return Some(c.clone());
    }
    let c = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .connect_timeout(Duration::from_secs(10))
        .danger_accept_invalid_certs(insecure)
        .resolve(host, via)
        .build()
        .ok()?;
    map.insert(key, c.clone());
    Some(c)
}

/// Host and port of an HTTP(S) URL, the port defaulting by scheme.
pub fn url_target(url: &str) -> Result<(String, u16), String> {
    let raw = url.trim().trim_end_matches('/');
    let raw = if raw.contains("://") { raw.to_string() } else { format!("http://{raw}") };
    let u = reqwest::Url::parse(&raw).map_err(|e| format!("Invalid URL \"{raw}\": {e}"))?;
    let host = u.host_str().ok_or_else(|| format!("No host in \"{raw}\""))?.trim_matches(['[', ']']).to_string();
    let port = u.port_or_known_default().unwrap_or(80);
    Ok((host, port))
}

/// A `mongodb://` URI pointed at `local`: the first host replaced by the
/// tunnel and `directConnection=true`, since a replica set would otherwise
/// hand back member names this machine can't reach. Refuses what a tunnel
/// can't carry: `mongodb+srv://` (several hosts found by DNS) and TLS (the
/// driver can't dial one address while verifying another name).
pub fn mongo_uri_via(uri: &str, local: u16) -> Result<String, String> {
    let uri = uri.trim();
    if uri.starts_with("mongodb+srv://") {
        return Err("An SSH tunnel needs a mongodb:// connection string with a host and port; mongodb+srv:// finds its hosts through DNS. Use the standard connection string from your provider.".into());
    }
    let rest = uri.strip_prefix("mongodb://").ok_or("The connection string starts with mongodb://")?;
    let (auth, after_auth) = match rest.rfind('@') {
        Some(i) if rest[..i].find('/').is_none() => (&rest[..=i], &rest[i + 1..]),
        _ => ("", rest),
    };
    let end = after_auth.find(['/', '?']).unwrap_or(after_auth.len());
    let tail = &after_auth[end..];
    let query = tail.split_once('?').map(|(_, q)| q).unwrap_or("");
    let tls = query.split('&').any(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (k.eq_ignore_ascii_case("tls") || k.eq_ignore_ascii_case("ssl")) && v.eq_ignore_ascii_case("true")
    });
    if tls {
        return Err("MongoDB with TLS can't go through an SSH tunnel here: the driver would check the certificate against 127.0.0.1. Turn TLS off for a server only reachable through the tunnel, or connect without SSH.".into());
    }
    let path = if tail.is_empty() { "/".to_string() } else if tail.starts_with('?') { format!("/{tail}") } else { tail.to_string() };
    let sep = if path.contains('?') { "&" } else { "?" };
    let direct = if query.to_ascii_lowercase().contains("directconnection=") { String::new() } else { format!("{sep}directConnection=true") };
    Ok(format!("mongodb://{auth}127.0.0.1:{local}{path}{direct}"))
}

/// The first host of a `mongodb://` URI, port defaulting to 27017.
pub fn mongo_first_host(uri: &str) -> Option<(String, u16)> {
    let rest = uri.trim().strip_prefix("mongodb://")?;
    let after_auth = match rest.rfind('@') {
        Some(i) if rest[..i].find('/').is_none() => &rest[i + 1..],
        _ => rest,
    };
    let hosts = &after_auth[..after_auth.find(['/', '?']).unwrap_or(after_auth.len())];
    let first = hosts.split(',').next()?.trim();
    if let Some(v6) = first.strip_prefix('[') {
        let (h, rest) = v6.split_once(']')?;
        let port = rest.strip_prefix(':').and_then(|p| p.parse().ok()).unwrap_or(27017);
        return Some((h.to_string(), port));
    }
    match first.rsplit_once(':') {
        Some((h, p)) => Some((h.to_string(), p.parse().ok()?)),
        None => Some((first.to_string(), 27017)),
    }
}

// ── TunnelState ───────────────────────────────────────────────────────────────

/// The tunnel under the open connection, and the tunnels under connections
/// Labs has set aside (by the same key as the parked pool). A tunnel moves with
/// its connection: before, there was one slot, so connecting anything through
/// SSH killed the tunnel a parked connection still depended on, and switching
/// to an engine without SSH left the old tunnel's ssh running.
#[derive(Default)]
struct Tunnels {
    active: Option<SshTunnel>,
    parked: std::collections::HashMap<String, SshTunnel>,
}

static TUNNELS: std::sync::LazyLock<Mutex<Tunnels>> = std::sync::LazyLock::new(Default::default);

fn tunnels() -> std::sync::MutexGuard<'static, Tunnels> {
    TUNNELS.lock().unwrap_or_else(|e| e.into_inner())
}

/// The open connection is being set aside under `key`: so is its tunnel.
pub fn park_active(key: &str) {
    let mut t = tunnels();
    match t.active.take() {
        Some(tunnel) => {
            t.parked.insert(key.to_string(), tunnel);
        }
        None => {
            t.parked.remove(key);
        }
    }
}

/// The connection under `key` is open again: its tunnel is the active one.
pub fn resume_parked(key: &str) {
    let mut t = tunnels();
    let tunnel = t.parked.remove(key);
    t.active = tunnel;
}

/// Parked connections were closed: close their tunnels.
pub fn drop_parked(keys: &[String]) {
    let mut t = tunnels();
    for k in keys {
        t.parked.remove(k);
    }
}

/// The open connection is closing: close its tunnel.
pub fn drop_active() {
    tunnels().active = None;
}

/// Tauri-managed handle on the active tunnel (kept so commands can take it as
/// state). Set on connect, cleared on disconnect; dropping a tunnel stops its
/// supervisor and ssh.
pub struct TunnelState;

impl TunnelState {
    pub fn new() -> Self {
        Self
    }

    pub fn set(&self, tunnel: Option<SshTunnel>) {
        tunnels().active = tunnel;
    }

    pub fn clear(&self) {
        self.set(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(key: &str) -> SshConfig {
        SshConfig { host: "bastion".into(), port: 22, username: "me".into(), private_key_path: key.into(), keepalive: 30 }
    }

    #[test]
    fn names_the_real_reason() {
        let key = explain_ssh_failure(&cfg("~/.ssh/id"), &["me@bastion: Permission denied (publickey).".into()], "x");
        assert!(key.contains("rejected the key ~/.ssh/id"), "{key}");
        let agent = explain_ssh_failure(&cfg(""), &["Permission denied (publickey).".into()], "x");
        assert!(agent.contains("ssh-add -l"), "{agent}");
        let changed = explain_ssh_failure(&cfg(""), &["@ WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED! @".into()], "x");
        assert!(changed.contains("known_hosts"), "{changed}");
        let other = explain_ssh_failure(&cfg(""), &["".to_string(), "kex_exchange_identification: read: Connection reset".into(), "".into()], "x");
        assert!(other.ends_with("kex_exchange_identification: read: Connection reset"), "{other}");
        assert_eq!(explain_ssh_failure(&cfg(""), &[], "fallback"), "fallback");
    }

    // Live: a tunnel whose ssh dies comes back on the same local port, and a
    // bad key says so. Needs an sshd that forwards to a Postgres:
    //   STROKE_TEST_SSH=user@127.0.0.1:2222 STROKE_TEST_SSH_KEY=/path/key
    //   STROKE_TEST_SSH_WRONG_KEY=/path/other STROKE_TEST_KNOWN_HOSTS=/tmp/kh
    //   STROKE_TEST_PG_PORT=5433 cargo test --lib ssh_tunnel::tests::live_ -- --nocapture
    #[tokio::test]
    async fn live_tunnel_comes_back_after_ssh_dies() {
        let (Ok(target), Ok(key), Ok(pg_port)) = (
            std::env::var("STROKE_TEST_SSH"),
            std::env::var("STROKE_TEST_SSH_KEY"),
            std::env::var("STROKE_TEST_PG_PORT"),
        ) else {
            return;
        };
        let (user, hostport) = target.split_once('@').unwrap();
        let (host, port) = hostport.split_once(':').unwrap();
        let ssh = SshConfig { host: host.into(), port: port.parse().unwrap(), username: user.into(), private_key_path: key, keepalive: 5 };
        let pg_port: u16 = pg_port.parse().unwrap();

        let tunnel = SshTunnel::establish(&ssh, "127.0.0.1", pg_port).await.unwrap();
        let url = format!("postgres://postgres@127.0.0.1:{}/postgres", tunnel.local_port);
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(2).connect(&url).await.unwrap();
        let one: i32 = sqlx::query_scalar("SELECT 1").fetch_one(&pool).await.unwrap();
        assert_eq!(one, 1);

        // A dropped network: ssh exits on its own.
        let watch = RunWatch::start();
        let killed = std::process::Command::new("pkill")
            .args(["-f", &format!("{}:127.0.0.1:{pg_port} ", tunnel.local_port)])
            .status()
            .unwrap();
        assert!(killed.success(), "found the tunnel's ssh to kill");
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(watch.explain("x".into(), true).starts_with("Result unknown"), "the loss is counted");
        assert!(RunWatch::start().down, "the tunnel reports down while it reconnects");

        // Back on the same port within the first backoff (1 s) plus a connect.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            if DOWN.load(Ordering::SeqCst) == 0 {
                break;
            }
            assert!(tokio::time::Instant::now() < deadline, "tunnel did not come back");
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        // The pool's address never changed: a fresh connection just works.
        let pool2 = sqlx::postgres::PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap();
        let two: i32 = sqlx::query_scalar("SELECT 2").fetch_one(&pool2).await.unwrap();
        assert_eq!(two, 2);
        drop(tunnel);
    }

    // Live: Labs parks a tunnelled connection and opens another through SSH.
    // The parked one's tunnel must survive, come back on resume, and the one
    // in front at that point must close.
    #[tokio::test]
    async fn live_parked_tunnels_survive_and_come_back() {
        let (Ok(target), Ok(key), Ok(pg_port)) = (
            std::env::var("STROKE_TEST_SSH"),
            std::env::var("STROKE_TEST_SSH_KEY"),
            std::env::var("STROKE_TEST_PG_PORT"),
        ) else {
            return;
        };
        let (user, hostport) = target.split_once('@').unwrap();
        let (host, port) = hostport.split_once(':').unwrap();
        let ssh = SshConfig { host: host.into(), port: port.parse().unwrap(), username: user.into(), private_key_path: key, keepalive: 30 };
        let pg_port: u16 = pg_port.parse().unwrap();
        let up = |p: u16| async move { tokio::net::TcpStream::connect(("127.0.0.1", p)).await.is_ok() };

        let a = SshTunnel::establish(&ssh, "127.0.0.1", pg_port).await.unwrap();
        let a_port = a.local_port;
        let state = TunnelState::new();
        state.set(Some(a));
        park_active("conn-a|db");

        // Connecting B: what connect does (close_existing, then set).
        drop_active();
        let b = SshTunnel::establish(&ssh, "127.0.0.1", pg_port).await.unwrap();
        let b_port = b.local_port;
        state.set(Some(b));
        assert!(up(a_port).await, "the parked connection's tunnel must stay up");

        // Back to A: B is set aside by the page first in real use, but a
        // resume alone replaces whatever was in front.
        resume_parked("conn-a|db");
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(up(a_port).await, "A's tunnel is in front again");
        assert!(!up(b_port).await, "B's tunnel closed with it");

        drop_active();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!up(a_port).await);
    }

    // Live: leaving a connection while its tunnel is down must not leave the
    // tunnel counted as down (later failures would all read as tunnel loss).
    #[tokio::test]
    async fn live_dropping_a_down_tunnel_settles_it() {
        let (Ok(target), Ok(key), Ok(pg_port), Ok(pidfile)) = (
            std::env::var("STROKE_TEST_SSH"),
            std::env::var("STROKE_TEST_SSH_KEY"),
            std::env::var("STROKE_TEST_PG_PORT"),
            std::env::var("STROKE_TEST_SSHD_PID"),
        ) else {
            return;
        };
        let (user, hostport) = target.split_once('@').unwrap();
        let (host, port) = hostport.split_once(':').unwrap();
        let ssh = SshConfig { host: host.into(), port: port.parse().unwrap(), username: user.into(), private_key_path: key, keepalive: 30 };
        let t = SshTunnel::establish(&ssh, "127.0.0.1", pg_port.parse().unwrap()).await.unwrap();
        // Take the server away and kill the tunnel: it stays down, retrying.
        let sshd = std::fs::read_to_string(&pidfile).unwrap().trim().to_string();
        std::process::Command::new("kill").arg(&sshd).status().unwrap();
        std::process::Command::new("pkill").args(["-f", &format!("{}:127.0.0.1:{pg_port} ", t.local_port)]).status().unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(t.down.load(Ordering::SeqCst));
        let before = DOWN.load(Ordering::SeqCst);
        drop(t);
        assert_eq!(DOWN.load(Ordering::SeqCst), before - 1);
    }

    #[tokio::test]
    async fn live_wrong_key_says_rejected() {
        let (Ok(target), Ok(key)) = (std::env::var("STROKE_TEST_SSH"), std::env::var("STROKE_TEST_SSH_WRONG_KEY")) else {
            return;
        };
        let (user, hostport) = target.split_once('@').unwrap();
        let (host, port) = hostport.split_once(':').unwrap();
        let ssh = SshConfig { host: host.into(), port: port.parse().unwrap(), username: user.into(), private_key_path: key.clone(), keepalive: 30 };
        let err = SshTunnel::establish(&ssh, "127.0.0.1", 5433).await.err().expect("a wrong key must fail");
        assert!(err.contains(&format!("rejected the key {key}")), "{err}");
    }

    #[test]
    fn points_a_mongo_uri_at_the_tunnel() {
        assert_eq!(
            mongo_uri_via("mongodb://u:p%40ss@db1:27018,db2:27019/shop?replicaSet=rs0", 40001).unwrap(),
            "mongodb://u:p%40ss@127.0.0.1:40001/shop?replicaSet=rs0&directConnection=true"
        );
        assert_eq!(mongo_uri_via("mongodb://db1", 40001).unwrap(), "mongodb://127.0.0.1:40001/?directConnection=true");
        assert_eq!(mongo_uri_via("mongodb://db1/?directConnection=false", 7).unwrap(), "mongodb://127.0.0.1:7/?directConnection=false");
        assert!(mongo_uri_via("mongodb+srv://c.example.net/shop", 7).unwrap_err().contains("mongodb+srv"));
        assert!(mongo_uri_via("mongodb://db1/?tls=true", 7).unwrap_err().contains("TLS"));
        assert_eq!(mongo_first_host("mongodb://u:p@db1:27018,db2/shop"), Some(("db1".into(), 27018)));
        assert_eq!(mongo_first_host("mongodb://db1"), Some(("db1".into(), 27017)));
        assert_eq!(mongo_first_host("mongodb://[::1]:27020/x"), Some(("::1".into(), 27020)));
    }

    #[test]
    fn points_an_http_url_at_the_tunnel() {
        assert_eq!(url_target("https://es.example.com").unwrap(), ("es.example.com".into(), 443));
        assert_eq!(url_target("couch.local:5984").unwrap(), ("couch.local".into(), 5984));
        let via: std::net::SocketAddr = "127.0.0.1:41000".parse().unwrap();
        let named = url_via(reqwest::Url::parse("https://es.example.com:9200/").unwrap(), Some(via));
        assert_eq!(named.as_str(), "https://es.example.com:41000/", "the name stays for TLS");
        let ip = url_via(reqwest::Url::parse("http://10.0.0.5:5984/").unwrap(), Some(via));
        assert_eq!(ip.as_str(), "http://127.0.0.1:41000/");
        let direct = url_via(reqwest::Url::parse("http://10.0.0.5:5984/").unwrap(), None);
        assert_eq!(direct.as_str(), "http://10.0.0.5:5984/");
    }

    #[test]
    fn backs_off_to_thirty_seconds() {
        let s: Vec<u64> = (0..8).map(|a| backoff(a).as_secs()).collect();
        assert_eq!(s, vec![1, 2, 4, 8, 16, 30, 30, 30]);
    }

    #[test]
    fn says_whether_a_failed_run_reached_the_server() {
        let before = RunWatch { losses: LOSSES.load(Ordering::SeqCst), down: false };
        assert_eq!(before.explain("boom".into(), true), "boom");
        LOSSES.fetch_add(1, Ordering::SeqCst);
        assert!(before.explain("boom".into(), true).starts_with("Result unknown"));
        assert!(before.explain("boom".into(), false).contains("Nothing was changed"));
        let down = RunWatch { losses: 0, down: true };
        assert!(down.explain("refused".into(), true).contains("did not reach the server"));
        assert_eq!(down.explain(crate::db::query::QUERY_CANCELLED.into(), true), crate::db::query::QUERY_CANCELLED);
    }
}
