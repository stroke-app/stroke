use log::LevelFilter;
use serde::{Deserialize, Serialize};
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{ConnectOptions, MySqlPool, PgPool, SqlitePool};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::State;
use tokio::sync::oneshot;

use super::ssh_tunnel::{SshConfig, SshTunnel, TunnelState};

// ── PostgreSQL ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PgConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
    pub ssl: bool,
    /// libpq `sslmode`, for when `ssl` alone is not the whole answer: a managed
    /// database usually wants `verify-full`, which needs a CA to verify against.
    /// `None` keeps the previous behaviour (`require` when `ssl`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_mode: Option<String>,
    /// Path to a CA certificate (`sslrootcert`). Required by `verify-ca` and
    /// `verify-full` unless the CA is already in the system trust store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_root_cert: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<SshConfig>,
    /// Session time zone applied on every pooled connection (IANA name, e.g.
    /// "America/New_York"). `None`/"SYSTEM" leaves the server default in place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

impl PgConfig {
    pub fn connection_url(&self) -> String {
        // `sslMode` wins when set; `ssl` is the old boolean and still means
        // `require`. A CA path only travels with a mode that verifies one.
        let mode = self
            .ssl_mode
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .unwrap_or(if self.ssl { "require" } else { "" });
        let mut params: Vec<String> = Vec::new();
        if !mode.is_empty() {
            params.push(format!("sslmode={mode}"));
            if let Some(ca) = self
                .ssl_root_cert
                .as_deref()
                .map(str::trim)
                .filter(|c| !c.is_empty())
            {
                params.push(format!("sslrootcert={}", urlencoding::encode(ca)));
            }
        }
        let ssl = if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        };
        format!(
            "postgres://{}:{}@{}:{}/{}{}",
            urlencoding::encode(&self.user),
            urlencoding::encode(&self.password),
            self.host,
            self.port,
            // Encoded like the credentials: sqlx percent-decodes the path, so a
            // database named `sales%2024` or `a#b` was read as something else.
            urlencoding::encode(&self.database),
            ssl
        )
    }
}

/// Kept for backward-compat with all existing code that uses `ConnectionConfig`.
pub type ConnectionConfig = PgConfig;

// ── SQLite ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqliteConfig {
    pub name: String,
    /// Absolute file path, or `:memory:` for an in-memory database.
    pub file_path: String,
}

// ── MySQL ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MysqlConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
    pub ssl: bool,
    /// MySQL `ssl-mode`: DISABLED | PREFERRED | REQUIRED | VERIFY_CA |
    /// VERIFY_IDENTITY. `None` keeps the previous behaviour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_mode: Option<String>,
    /// Path to a CA certificate (`ssl-ca`), for the verifying modes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_root_cert: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<SshConfig>,
    /// Session time zone applied on connect (`SET time_zone`). `None`/"SYSTEM"
    /// leaves the server default in place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

impl MysqlConfig {
    pub fn connection_url(&self) -> String {
        let mode = self
            .ssl_mode
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .unwrap_or(if self.ssl { "required" } else { "disabled" });
        let mut params = vec![format!("ssl-mode={mode}")];
        if let Some(ca) = self
            .ssl_root_cert
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
        {
            params.push(format!("ssl-ca={}", urlencoding::encode(ca)));
        }
        format!(
            "mysql://{}:{}@{}:{}/{}?{}",
            urlencoding::encode(&self.user),
            urlencoding::encode(&self.password),
            self.host,
            self.port,
            urlencoding::encode(&self.database),
            params.join("&")
        )
    }
}

// ── Cloudflare D1 ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct D1Config {
    pub name: String,
    pub account_id: String,
    pub database_id: String,
    pub api_token: String,
}

// ── LibSQL / Turso ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibSqlConfig {
    pub name: String,
    /// Database URL: libsql://*.turso.io, https://*.turso.io, or http://localhost:PORT
    pub url: String,
    /// Auth token (Turso). Leave None/empty for local unauthenticated libsql-server.
    pub auth_token: Option<String>,
}

// ── ClickHouse ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClickhouseConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
    /// Use HTTPS (port 8443) instead of plain HTTP (port 8123).
    #[serde(default)]
    pub secure: bool,
}

impl ClickhouseConfig {
    /// Base HTTP(S) URL for the ClickHouse HTTP interface, no trailing slash.
    pub fn base_url(&self) -> String {
        let scheme = if self.secure { "https" } else { "http" };
        format!("{scheme}://{}:{}", self.host, self.port)
    }
}

// ── PostHog ───────────────────────────────────────────────────────────────────

/// A PostHog project, queried with HogQL over PostHog's query API. PostHog keeps
/// its data in ClickHouse, but Cloud offers no direct ClickHouse access; the API
/// is the way in, with a personal API key that has the Query Read scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PosthogConfig {
    pub name: String,
    /// Base URL: `https://us.posthog.com`, `https://eu.posthog.com`, or a
    /// self-hosted instance.
    pub host: String,
    pub project_id: String,
    pub api_key: String,
}

impl PosthogConfig {
    pub fn base_url(&self) -> String {
        let h = self.host.trim().trim_end_matches('/');
        if h.starts_with("http://") || h.starts_with("https://") {
            h.to_string()
        } else {
            format!("https://{h}")
        }
    }
}

// ── Redis ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
    /// Logical database index (0-15 by default).
    #[serde(default)]
    pub db: u8,
    /// Use TLS (rediss://) instead of plain TCP (redis://).
    #[serde(default)]
    pub tls: bool,
}

// ── DuckDB ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuckdbConfig {
    pub name: String,
    /// Absolute file path, or `:memory:` for an in-memory database.
    pub file_path: String,
}

/// DuckDB's `Connection` is synchronous and not clonable, so we wrap it in an
/// `Arc<Mutex<…>>`. All access happens inside `spawn_blocking` to keep the async
/// runtime free; the lock is held only for the duration of a single statement.
pub type DuckdbHandle = Arc<Mutex<duckdb::Connection>>;

// ── MS SQL Server ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MssqlConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
    /// Negotiate TLS encryption for the connection.
    #[serde(default)]
    pub encrypt: bool,
    /// Skip TLS certificate validation (self-signed dev servers).
    #[serde(default)]
    pub trust_cert: bool,
}

/// tiberius `Client` is async and not clonable; share it behind an async mutex.
/// Only one desktop connection is ever live, so lock contention is a non-issue.
pub type MssqlHandle = Arc<tokio::sync::Mutex<crate::db::mssql::MssqlClient>>;

// ── Any-connection config for cross-connection queries ────────────────────────

/// Accepts the full saved-connection JSON from the frontend and routes to the
/// correct backend without touching the global active-connection state.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type")]
pub enum AnyConnectionConfig {
    #[serde(rename = "postgres")]
    Postgres(PgConfig),
    #[serde(rename = "sqlite")]
    Sqlite(SqliteConfig),
    #[serde(rename = "d1")]
    D1(D1Config),
    #[serde(rename = "mysql")]
    Mysql(MysqlConfig),
    #[serde(rename = "libsql")]
    Libsql(LibSqlConfig),
    #[serde(rename = "clickhouse")]
    Clickhouse(ClickhouseConfig),
    #[serde(rename = "redis")]
    Redis(RedisConfig),
    #[serde(rename = "duckdb")]
    Duckdb(DuckdbConfig),
    #[serde(rename = "mssql")]
    Mssql(MssqlConfig),
    #[serde(rename = "posthog")]
    Posthog(PosthogConfig),
}

// ── Active connection ─────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum ActiveConnection {
    Postgres(PgPool),
    Sqlite(SqlitePool),
    Mysql(MySqlPool),
    D1(D1Config),
    LibSql(LibSqlConfig),
    Clickhouse(ClickhouseConfig),
    Redis(RedisConfig),
    Duckdb(DuckdbHandle),
    Mssql(MssqlHandle),
    Posthog(PosthogConfig),
}

impl ActiveConnection {
    pub fn driver(&self) -> &'static str {
        match self {
            Self::Postgres(_) => "postgres",
            Self::Sqlite(_) => "sqlite",
            Self::Mysql(_) => "mysql",
            Self::D1(_) => "d1",
            Self::LibSql(_) => "libsql",
            Self::Clickhouse(_) => "clickhouse",
            Self::Redis(_) => "redis",
            Self::Duckdb(_) => "duckdb",
            Self::Mssql(_) => "mssql",
            Self::Posthog(_) => "posthog",
        }
    }
}

// ── DbState ───────────────────────────────────────────────────────────────────

pub struct DbState {
    pub conn: Arc<Mutex<Option<ActiveConnection>>>,
    /// Cancel handles for the queries currently in flight, keyed by the id the
    /// caller sent. Keyed rather than a single slot because the UI runs several
    /// queries at once (one per editor tab) - with one slot, the second run
    /// overwrote the first's handle and "Cancel" hit whichever query registered
    /// last instead of the one whose button was pressed.
    pub cancels: Arc<Mutex<std::collections::HashMap<String, oneshot::Sender<()>>>>,
}

impl Default for DbState {
    fn default() -> Self {
        Self {
            conn: Arc::new(Mutex::new(None)),
            cancels: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }
}

/// Register a cancel handle for `id`, replacing (and dropping) any handle already
/// stored under it.
pub fn register_cancel(state: &State<'_, DbState>, id: &str, tx: oneshot::Sender<()>) {
    if let Ok(mut map) = state.cancels.lock() {
        map.insert(id.to_string(), tx);
    }
}

/// Drop the cancel handle for `id` (query finished, cancelling it is meaningless).
pub fn unregister_cancel(state: &State<'_, DbState>, id: &str) {
    if let Ok(mut map) = state.cancels.lock() {
        map.remove(id);
    }
}

/// Returns a clone of the active connection, or an error if disconnected.
pub fn require_conn(state: &State<'_, DbState>) -> Result<ActiveConnection, String> {
    state
        .conn
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "Not connected to a database".to_string())
}

/// Convenience helper kept for all existing PostgreSQL-specific code.
pub fn require_pool(state: &State<'_, DbState>) -> Result<PgPool, String> {
    match require_conn(state)? {
        ActiveConnection::Postgres(pool) => Ok(pool),
        other => Err(format!(
            "Expected a PostgreSQL connection, got {}",
            other.driver()
        )),
    }
}

/// The same `Arc` that `DbState` and `McpState` share, parked so code without a
/// `State` handle can reach the live connection. Set once from `setup()`.
static ACTIVE: std::sync::OnceLock<Arc<Mutex<Option<ActiveConnection>>>> = std::sync::OnceLock::new();

/// Called once from `setup()`. Later calls are ignored.
pub fn register_active_conn(conn: Arc<Mutex<Option<ActiveConnection>>>) {
    let _ = ACTIVE.set(conn);
}

/// Swap in a newly-refreshed Cloudflare token on the live D1 connection.
///
/// Without this, a refresh fixes only the one request that triggered it: the
/// connection still holds the dead token, so the *next* query 401s and pays for
/// its own refresh round trip, forever. No-op unless a D1 connection is open.
pub fn update_d1_token(token: &str) {
    let Some(slot) = ACTIVE.get() else { return };
    let Ok(mut guard) = slot.lock() else { return };
    if let Some(ActiveConnection::D1(cfg)) = guard.as_mut() {
        cfg.api_token = token.to_string();
    }
}

fn set_conn(state: &State<'_, DbState>, conn: Option<ActiveConnection>) -> Result<(), String> {
    *state.conn.lock().map_err(|e| e.to_string())? = conn;
    Ok(())
}

async fn close_existing(state: &State<'_, DbState>) {
    let old = state.conn.lock().ok().and_then(|mut g| g.take());
    let Some(conn) = old else { return };
    // Drain the old pool in the background. Pool::close() waits for every
    // connection to be handed back - and against a dead peer (sleep/wake,
    // network change: exactly the state a reconnect follows) that stalls for
    // seconds. Nothing downstream needs the drain to finish - the state slot
    // is already empty - so never make connect/switch wait on it. The timeout
    // caps the drain; after that the OS cleans up the sockets.
    tokio::spawn(async move {
        let timeout = std::time::Duration::from_secs(3);
        match conn {
            ActiveConnection::Postgres(p) => {
                let _ = tokio::time::timeout(timeout, p.close()).await;
            }
            ActiveConnection::Sqlite(p) => {
                let _ = tokio::time::timeout(timeout, p.close()).await;
            }
            ActiveConnection::Mysql(p) => {
                let _ = tokio::time::timeout(timeout, p.close()).await;
            }
            // Handle-based engines (DuckDB, MSSQL) and HTTP configs clean up in Drop.
            _ => {}
        }
    });
}

// ── Reachability preflight ────────────────────────────────────────────────────

/// Name-resolution budget. Split from the TCP budget because a stalled resolver
/// (VPN split-DNS, an unreachable DNS server) is a distinct failure from a
/// stalled handshake and must not eat the whole probe window.
const DNS_BUDGET: Duration = Duration::from_secs(5);
/// Per-address TCP handshake budget for the probe.
const TCP_BUDGET: Duration = Duration::from_secs(6);
/// Hard ceiling on one whole connect attempt (probe + handshake + auth). Exists
/// so an indeterminate stall surfaces a real error instead of spinning: a
/// blackholed route gives no reply at all, and the OS SYN-retry window is ~21 s
/// on Windows, well past any point where waiting is still useful.
const CONNECT_DEADLINE: Duration = Duration::from_secs(20);

/// What the pre-connect probe learned about `host:port`.
enum Preflight {
    /// A candidate address completed a TCP handshake. Drivers dial it directly:
    /// that skips a second name lookup and, on a dual-stack host, skips the
    /// address family that is blackholed.
    Reachable(std::net::SocketAddr),
    /// Definitive answer: the name doesn't resolve, nothing is listening, or
    /// there is no route. Waiting longer or retrying cannot change it.
    Unreachable(String),
    /// Indeterminate: candidates were still in flight when the budget ran out.
    /// A slow VPN/satellite link looks exactly like this, so this must NEVER
    /// veto the real connect - it only supplies the message if that stalls too.
    Inconclusive(String),
}

/// Probe `host:port` before building a pool so unreachable hosts and wrong ports
/// fail with a clear message instead of stalling on the pool's acquire timeout.
async fn preflight(host: &str, port: u16) -> Preflight {
    let host = host.trim();
    if host.is_empty() {
        return Preflight::Unreachable("Host is empty".to_string());
    }
    let target = format!("{host}:{port}");

    let addrs = match tokio::time::timeout(DNS_BUDGET, tokio::net::lookup_host(&target)).await {
        Ok(Ok(it)) => it.collect::<Vec<_>>(),
        Ok(Err(e)) => return Preflight::Unreachable(format!("Can't resolve host {host}: {e}")),
        Err(_) => {
            return Preflight::Inconclusive(format!(
                "Name lookup for {host} didn't answer within {}s",
                DNS_BUDGET.as_secs()
            ))
        }
    };
    if addrs.is_empty() {
        return Preflight::Unreachable(format!("Host {host} didn't resolve to any address"));
    }

    // Happy eyeballs: probe every resolved address CONCURRENTLY and take the
    // first that completes. Both `TcpStream::connect(host_str)` and the sqlx /
    // tiberius drivers walk the address list SEQUENTIALLY, so a host whose AAAA
    // record has no working route stalls on IPv6 for the full OS SYN-retry
    // window (~21 s on Windows, where an unroutable v6 destination black-holes
    // instead of returning ENETUNREACH like Linux usually does) before the
    // working IPv4 address is ever tried. Racing them removes that stall and
    // hands the driver the address we know answers.
    let mut probes = tokio::task::JoinSet::new();
    for addr in addrs {
        probes.spawn(async move {
            let r = tokio::time::timeout(TCP_BUDGET, tokio::net::TcpStream::connect(addr)).await;
            (addr, r)
        });
    }

    let mut last_err: Option<String> = None;
    let mut stalled = false;
    while let Some(joined) = probes.join_next().await {
        match joined {
            // Dropping `probes` here aborts the remaining in-flight probes.
            Ok((addr, Ok(Ok(_stream)))) => return Preflight::Reachable(addr),
            Ok((addr, Ok(Err(e)))) => last_err = Some(format!("{addr}: {e}")),
            Ok((_, Err(_))) => stalled = true,
            Err(_) => {} // probe task panicked or was cancelled - ignore
        }
    }

    if stalled {
        // Some address never answered. Could be a slow link, so stay advisory.
        Preflight::Inconclusive(format!(
            "Can't reach {target} - no response within {}s. Check the host/port, and whether a firewall or VPN is blocking it.",
            TCP_BUDGET.as_secs()
        ))
    } else {
        Preflight::Unreachable(format!(
            "Can't reach {target} ({})",
            last_err.unwrap_or_else(|| "no route to host".to_string())
        ))
    }
}

/// Race the driver's real connect against the reachability probe.
///
/// The probe NEVER vetoes a connect that could still succeed. It only
/// short-circuits on a *definitive* answer (the name doesn't resolve, nothing is
/// listening, no route). Two separate failure modes forced this shape, both
/// observed against a real remote database:
///
///   • Probing *before* connecting adds the probe's full cost to every connect.
///     On a lossy link a dropped SYN is retried at 1s/2s/4s, so the probe can
///     burn 6s for a database that then completes its handshake in 335ms.
///   • Treating an inconclusive probe as failure aborts the connect for hosts
///     that are merely slow to answer. That is the "can't connect at all on some
///     machines" report: the app returned "Can't reach host" having never
///     attempted a handshake, and the frontend then retried that same verdict.
///
/// Racing gives all three properties at once: no added latency on the happy
/// path, a fast clear error for genuinely dead hosts, and a slow-but-reachable
/// host still connects.
///
/// Phase timings are logged at info level. "Connecting is slow" is otherwise
/// unattributable from the outside - name lookup, TCP, and TLS+auth stall for
/// completely different reasons, and the machines where this reproduces (Windows
/// behind a VPN, corporate DNS) are rarely the machine doing the debugging.
async fn connect_racing_probe<T>(
    host: &str,
    port: u16,
    // Set when TCP is proven, for the Postgres retry ladder. None for engines
    // whose connect isn't retried.
    tcp_ok: Option<&std::sync::atomic::AtomicBool>,
    connect: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    let t0 = std::time::Instant::now();
    let probe = preflight(host, port);
    let deadline = tokio::time::sleep(CONNECT_DEADLINE);
    tokio::pin!(probe);
    tokio::pin!(connect);
    tokio::pin!(deadline);

    // Diagnosis to report if the handshake never finishes. Only an inconclusive
    // probe produces one; a definitive failure returns immediately instead.
    let mut hint: Option<String> = None;
    let mut probe_pending = true;

    loop {
        tokio::select! {
            // `probe_pending` stops select! from polling an already-completed
            // future on the next loop iteration.
            outcome = &mut probe, if probe_pending => {
                probe_pending = false;
                let ms = t0.elapsed().as_millis();
                match outcome {
                    Preflight::Reachable(addr) => {
                        log::info!("preflight {host}:{port} -> {addr} reachable in {ms}ms");
                        // Tells retry_fast to stop restarting the handshake: a SYN
                        // that gets through here is getting through there too.
                        if let Some(flag) = tcp_ok {
                            flag.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    Preflight::Unreachable(msg) => {
                        log::warn!("preflight {host}:{port} definitively unreachable in {ms}ms: {msg}");
                        return Err(msg);
                    }
                    Preflight::Inconclusive(h) => {
                        log::warn!("preflight {host}:{port} inconclusive after {ms}ms - still waiting on the handshake");
                        hint = Some(h);
                    }
                }
            }
            result = &mut connect => {
                let ms = t0.elapsed().as_millis();
                match &result {
                    Ok(_) => log::info!("connected to {host}:{port} in {ms}ms"),
                    Err(e) => log::warn!("connect to {host}:{port} failed after {ms}ms: {e}"),
                }
                return result;
            }
            _ = &mut deadline => {
                let ms = t0.elapsed().as_millis();
                log::warn!("connect to {host}:{port} gave up after {ms}ms");
                return Err(hint.unwrap_or_else(|| {
                    format!("Connection timed out after {}s", CONNECT_DEADLINE.as_secs())
                }));
            }
        }
    }
}

// ── PostgreSQL connect / test ─────────────────────────────────────────────────

/// How long a pooled connection may sit idle before it is worth a liveness ping.
///
/// The two settings below pull in opposite directions: `min_connections` keeps
/// connections standing by so a burst never pays a handshake, and
/// `test_before_acquire(false)` skips the ping that would prove they're alive.
/// Together they mean that after a laptop sleep, a VPN flip or a wifi change
/// every standing connection is dead and the next query gets one of them - which
/// is the slow, error-then-reconnect path the user actually feels.
///
/// Pinging every acquire costs a full round trip six times over on one table
/// open, which is why it was turned off. But a connection handed back seconds ago
/// cannot have died in the meantime, and one that has been idle for a minute very
/// well might. So the ping is gated on idle time: free on the hot path, and the
/// dead-connection case heals inside `acquire()` instead of surfacing as a failed
/// query and a full reconnect.
const STALE_AFTER: Duration = Duration::from_secs(25);

/// What we have measured about the network path to this database.
///
/// A connection killed by a laptop sleep, a VPN flip, a wifi change or a NAT
/// idle-timeout is usually not *closed*: the peer simply stops answering, and as
/// far as this side is concerned the socket is still open. `SELECT 1` on it then
/// neither succeeds nor fails - the kernel retransmits into the void until the
/// TCP stack gives up, which takes minutes. Keepalive probes would catch it
/// sooner, but macOS does not start them for two hours by default and sqlx
/// exposes no way to configure them.
///
/// So the ping has to bound itself, and a fixed bound cannot be right for both a
/// database on localhost and one across an ocean: too tight throws away healthy
/// connections on a slow host (each costing a fresh handshake), too loose leaves
/// the fast host waiting on a corpse. This measures the host instead - see
/// `deadline` - and remembers that the path was alive, so the other five queries
/// in a table open do not each re-prove it (`recently_verified`).
#[derive(Debug, Default)]
struct PathHealth {
    /// `(when a ping last succeeded, EWMA of its round trip in microseconds)`.
    ///
    /// One lock, never held across an await: the two values are only ever read
    /// and written together, and every caller does so between awaits.
    state: Mutex<(Option<std::time::Instant>, u64)>,
}

impl PathHealth {
    /// How long one connection's successful ping vouches for the others.
    ///
    /// The failure this guards against is the whole network path disappearing,
    /// which takes every connection with it at once - so if one answered a
    /// moment ago, the rest are alive too. A single connection dying on its own
    /// (a server-side idle timeout, an admin terminate) is closed properly by
    /// the server, so it comes back as an immediate error rather than a hang,
    /// and needs no ping to find. That makes this safe, and it removes five of
    /// the six round trips a table open used to pay after any pause.
    const FRESH_FOR: Duration = Duration::from_secs(2);

    /// Before the first measurement, allow this much. Generous enough for a
    /// cold remote host, far short of the pool's `acquire_timeout`.
    const UNMEASURED: Duration = Duration::from_secs(2);

    fn recently_verified(&self) -> bool {
        let guard = self.state.lock().unwrap();
        guard.0.is_some_and(|at| at.elapsed() < Self::FRESH_FOR)
    }

    /// How long to wait for a ping before calling the connection dead.
    ///
    /// Ten times this host's own measured round trip. A healthy server would
    /// have to become an order of magnitude slower than its own normal to be
    /// mistaken for a dead one, so the "we discarded a connection that was
    /// merely busy" case effectively does not arise - while a fast host is
    /// declared dead in well under a second instead of waiting out a timeout
    /// picked for someone else's network.
    fn deadline(&self) -> Duration {
        let rtt_us = self.state.lock().unwrap().1;
        if rtt_us == 0 {
            return Self::UNMEASURED;
        }
        Duration::from_micros(rtt_us.saturating_mul(10))
            .clamp(Duration::from_millis(500), Duration::from_secs(15))
    }

    fn record(&self, rtt: Duration) {
        let mut guard = self.state.lock().unwrap();
        guard.0 = Some(std::time::Instant::now());
        let sample = rtt.as_micros().min(u128::from(u64::MAX)) as u64;
        // Weighted to the recent past so a network that gets slower is followed
        // quickly, without one stall moving the deadline far.
        guard.1 = if guard.1 == 0 { sample } else { guard.1 / 2 + sample / 2 };
    }
}

/// Ping a connection that has been idle long enough to have died, and say
/// whether the pool should still hand it out.
///
/// Shared by the Postgres and MySQL pools - `SELECT 1` and the reasoning are
/// the same for both.
macro_rules! ping_if_stale {
    ($health:expr, $conn:expr, $meta:expr) => {{
        if $meta.idle_for < STALE_AFTER || $health.recently_verified() {
            Ok(true)
        } else {
            let started = std::time::Instant::now();
            match tokio::time::timeout(
                $health.deadline(),
                sqlx::query("SELECT 1").execute(&mut *$conn),
            )
            .await
            {
                Ok(Ok(_)) => {
                    $health.record(started.elapsed());
                    Ok(true)
                }
                Ok(Err(e)) => Err(e),
                // Silence is the dead case. Report it as an error rather than
                // `Ok(false)`: sqlx closes a rejected connection gracefully,
                // which means writing a Terminate to the same socket that just
                // failed to answer, and waiting on that too. An error makes it
                // drop the connection outright and open a fresh one.
                Err(_elapsed) => Err(sqlx::Error::PoolTimedOut),
            }
        }
    }};
}

/// Measured handshake cost per `host:port`, in ms, remembered across restarts.
///
/// Measured with psql from here: Neon, Supabase, Nile and Prisma Postgres all
/// take 1.8-3.4s to open a connection (TCP + TLS + startup + SCRAM, six or seven
/// round trips to a far region) but only 265-535ms to answer a query on an open
/// one. sqlx opens a NEW connection for every query that finds no idle one, so
/// the six queries of a table open each paid a full handshake instead of
/// waiting half a second for a busy connection to come back. A host known to be
/// that far gets a small fixed pool instead (`pg_pool_for`).
static HANDSHAKES: std::sync::OnceLock<Mutex<std::collections::HashMap<String, u64>>> = std::sync::OnceLock::new();
static DATA_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
const HANDSHAKE_FILE: &str = "handshake-costs.json";

/// Called once from setup, so the handshake memory survives a restart: the
/// reconnect on launch is exactly the connect that needs it.
pub fn set_data_dir(dir: std::path::PathBuf) {
    let _ = DATA_DIR.set(dir);
}

fn handshakes() -> &'static Mutex<std::collections::HashMap<String, u64>> {
    HANDSHAKES.get_or_init(|| {
        let map = DATA_DIR
            .get()
            .and_then(|d| std::fs::read_to_string(d.join(HANDSHAKE_FILE)).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Mutex::new(map)
    })
}

fn known_handshake_ms(host: &str, port: u16) -> Option<u64> {
    handshakes().lock().unwrap_or_else(|e| e.into_inner()).get(&format!("{host}:{port}")).copied()
}

fn record_handshake(host: &str, port: u16, ms: u64) {
    let snapshot = {
        let mut map = handshakes().lock().unwrap_or_else(|e| e.into_inner());
        let key = format!("{host}:{port}");
        // Half old, half new: one unlucky connect doesn't flip the pool shape.
        let v = map.get(&key).map_or(ms, |old| old / 2 + ms / 2);
        if map.get(&key) == Some(&v) {
            return;
        }
        map.insert(key, v);
        serde_json::to_string(&*map).ok()
    };
    if let (Some(dir), Some(text)) = (DATA_DIR.get(), snapshot) {
        let _ = std::fs::write(dir.join(HANDSHAKE_FILE), text);
    }
}

/// A handshake slower than this means queueing behind an open connection beats
/// opening another. Nearby hosts measured 320-690ms, far providers 1.8-3.4s.
const FAR_HANDSHAKE_MS: u64 = 1200;

/// Connections a far host keeps open: one per query of a table open (rows,
/// count and four catalog lookups), so browsing once connected runs every query
/// at once, exactly as on a nearby host.
const FAR_POOL: u32 = 6;

/// Pool shape for this host. A far host's pool is exactly `FAR_POOL` wide and
/// never shrinks: `warm_in_parallel` opens all of it right after connect, and a
/// query that arrives before those land waits ~300-500ms for a free connection
/// instead of opening one more at 2-3s. Unknown and nearby hosts keep the wide
/// pool, where a handshake is cheap.
fn pg_pool_for(host: &str, port: u16) -> PgPoolOptions {
    match known_handshake_ms(host, port) {
        Some(ms) if ms >= FAR_HANDSHAKE_MS => {
            log::info!("{host}:{port} handshake ~{ms}ms: {FAR_POOL}-connection pool, warmed in parallel");
            pg_pool_builder().min_connections(FAR_POOL).max_connections(FAR_POOL)
        }
        _ => pg_pool_builder(),
    }
}

/// Open the rest of a far host's pool at once, one handshake's wait in total.
///
/// sqlx's own min-connections fill opens them one after another (six in a row
/// at 3s is 18s of a pool that isn't ready). These are spawned while the
/// caller still holds the first connection, so none of them can grab it and
/// every one opens a fresh connection; each is handed back the moment it
/// lands, so none is held away from a query that needs it.
fn warm_in_parallel<DB: sqlx::Database>(pool: &sqlx::Pool<DB>, total: u32) {
    let extra = total.saturating_sub(pool.size());
    for _ in 0..extra {
        let pool = pool.clone();
        tokio::spawn(async move {
            if let Ok(conn) = pool.acquire().await {
                drop(conn);
            }
        });
    }
}

fn pg_pool_builder() -> PgPoolOptions {
    let health = std::sync::Arc::new(PathHealth::default());
    PgPoolOptions::new()
        // Headroom for the first-open burst (6 concurrent: rows + count + four
        // catalog lookups) PLUS the background warm, which HOLDS its connections
        // until it has opened them all. Sized at 8 with a warm of 6, the burst
        // found 2 free and queued the rest until acquire_timeout - surfacing as
        // "pool timed out while waiting for an open connection" on every connect.
        // The extras stay idle and close after idle_timeout, so steady state
        // still settles back to ~4.
        .max_connections(10)
        // Two, not four. The pool's maintenance task opens these in the
        // BACKGROUND so a burst finds them ready instead of opening its own
        // mid-flight. Idle connections coming back dead after sleep/wake is
        // handled by the idle-gated ping below, so this only has to cover the
        // burst - and on a host where one handshake costs seconds (a proxied
        // serverless Postgres measured at ~5.7s) four background opens raced the
        // first table open's six queries for the same ten slots. The row counts
        // lost that race, which is exactly what "pool timed out while waiting for
        // an open connection" was in the log.
        .min_connections(2)
        // 20s, not 10: a single handshake to a proxied serverless host measured
        // 5.7s, so a query queued behind two of them blew a 10s ceiling and failed
        // as "pool timed out" - reporting a timeout for a connection that was
        // simply still being made. The connect path has its own CONNECT_DEADLINE;
        // this only bounds how long a query waits for a slot.
        .acquire_timeout(Duration::from_secs(20))
        // Keep connections warm for the whole active session. A short idle_timeout
        // (was 30 s) meant any pause longer than that forced a full TCP+TLS+auth
        // re-handshake on the next query - on a remote/SSL host that's seconds of
        // latency per connection, and a single table open opens several. 10 min
        // keeps the pool warm between interactions so repeat fetches stay fast;
        // test_before_acquire (sqlx default) still drops connections killed by
        // sleep/wake. max_lifetime caps server-side staleness.
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        // sqlx pings the connection before handing it out. On a remote host that
        // is a full round trip on EVERY acquire - and a single table open
        // acquires six (rows + count + four catalog lookups), so the ping alone
        // cost six RTTs before any real query was sent. Worse, a failed ping
        // makes the pool discard and reopen, retrying until acquire_timeout,
        // which is how warming five connections took exactly 10s.
        //
        // Staleness is bounded by max_lifetime, and anything idle long enough to
        // have been killed by a sleep/wake is checked by `before_acquire` below.
        .test_before_acquire(false)
        // Ping only what might be dead - see STALE_AFTER. A failure here makes the
        // pool drop this connection and hand over another (or open one), so a
        // stale pool repairs itself during acquire rather than after a failed query.
        .before_acquire(move |conn, meta| {
            let health = health.clone();
            Box::pin(async move { ping_if_stale!(health, conn, meta) })
        })
}

// The pool used to force three extra connections open right after connect
// (`warm_pool`), from a time when `min_connections` was 0 and the pool started
// with exactly the one connection the handshake produced. `min_connections(4)`
// now has the pool's own maintenance task doing that in the background, so the
// warm added nothing but three more acquires per connect - and because it HELD
// each one until the last landed, it was also what made "pool timed out while
// waiting for an open connection" reachable on a slow link. Removed rather than
// tuned: the pool already does this, and one mechanism is easier to reason about
// than two fighting over the same ceiling.

/// Turn sqlx's `PoolTimedOut` into the error that actually caused it.
///
/// A pool `acquire()` retries internally and reports only "pool timed out while
/// waiting for an open connection" - which says nothing about *why* no
/// connection could be established (bad password, server at max_connections, TLS
/// refused). One direct connection attempt surfaces the real message. Runs only
/// on the failure path, so a successful connect pays nothing.
async fn explain_pg_failure(opts: &PgConnectOptions, pool_err: String) -> String {
    if !pool_err.contains("pool timed out") {
        return pool_err;
    }
    use sqlx::Connection;
    match tokio::time::timeout(
        Duration::from_secs(10),
        sqlx::postgres::PgConnection::connect_with(opts),
    )
    .await
    {
        Ok(Ok(c)) => {
            // A direct connection works, so the pool timeout was contention or a
            // transient stall rather than a broken configuration.
            let _ = c.close().await;
            // A single connection works, so this is contention, not configuration:
            // either the server is at its connection limit, or the pool's own
            // connections are all tied up in long-running queries.
            format!("{pool_err} (a direct connection did succeed, so the server is reachable - the pool's connections are all busy, or the server is at its connection limit)")
        }
        Ok(Err(e)) => format!("Connection failed: {e}"),
        Err(_) => pool_err,
    }
}


/// Retry a connection attempt on a SHORT clock instead of the kernel's.
///
/// Measured on a lossy link: the median TCP connect to the database was 36ms,
/// but ~20% of attempts lost their SYN and then sat through the kernel's
/// retransmit backoff - 4145ms, 4160ms, 11254ms. Linux will not retry a SYN
/// sooner than ~1s, so waiting on it is the wrong move: a brand-new attempt
/// sends a fresh SYN immediately. With a 6-connection burst, the chance that at
/// least one attempt stalls is ~74%, and the slowest one gates the whole UI.
///
/// Escalating budgets so a genuinely slow-but-healthy host (cold serverless
/// Postgres, distant region) still gets time to answer rather than being retried
/// forever; the last attempt is unbounded and carries any real error back.
async fn retry_fast<T, F, Fut>(tcp_ok: &std::sync::atomic::AtomicBool, mut attempt: F) -> Result<T, sqlx::Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, sqlx::Error>>,
{
    // ONE fast retry, then wait it out. Each attempt builds a pool, and an
    // abandoned pool can leave a half-open connection behind, so retrying three
    // times multiplied connections against a server that may itself be at its
    // limit - making the thing we were trying to avoid more likely.
    // A healthy connect here measures 265-364ms end to end (36ms TCP + ~120ms TLS
    // + auth), so the first budget sits just above that ceiling: it catches a
    // lost SYN without ever firing on a connection that was merely slow.
    //
    // Every attempt is bounded. With a single bounded attempt followed by an
    // unbounded one, a run of lost SYNs on the second attempt sat through the
    // kernel's full backoff - 1+2+4+8s - and the DevTools waterfall showed
    // connect_postgres at 15.24s. The ladder keeps doubling instead, so no single
    // attempt can cost more than its own budget, and the outer CONNECT_DEADLINE
    // still caps the whole thing.
    //
    // A timed-out attempt drops its half-built pool, which closes whatever
    // connections it had opened, so retrying does not pile connections onto the
    // server.
    // The retry only ever earned its place against a LOST SYN - a packet dropped
    // before the socket exists, where the kernel then sits on its ~1s retransmit.
    // It cannot help a handshake that is merely slow, because restarting one pays
    // the TLS and auth round trips again from zero.
    //
    // The ladder used to fire on every connect, calibrated against a nearby host
    // ("a healthy connect measures 265-364ms"). Against a proxied serverless
    // Postgres whose handshake genuinely takes ~5.7s it did this:
    //
    //     preflight reachable in 91ms
    //     attempt 1 exceeded 800ms   → thrown away
    //     attempt 2 exceeded 1500ms  → thrown away
    //     attempt 3 exceeded 3000ms  → thrown away
    //     connected in 11067ms
    //
    // 5.3 seconds of progress binned, and four half-built pools left for the
    // server to clean up - which is also how "pool timed out" showed up on the
    // first table open.
    //
    // So the probe decides. It opens its own TCP connection to the same host, and
    // the moment that succeeds we know SYNs are getting through: any stall after
    // that is slowness, not loss, and the attempt in flight is the fastest one
    // we will ever have. Only while TCP is still unproven is a fresh SYN worth
    // sending.
    const BUDGETS_MS: [u64; 3] = [800, 1500, 3000];
    for (i, ms) in BUDGETS_MS.iter().enumerate() {
        if tcp_ok.load(std::sync::atomic::Ordering::Relaxed) {
            // TCP demonstrably works. Stop bounding the handshake and let it land.
            break;
        }
        let fut = attempt();
        tokio::pin!(fut);
        match tokio::time::timeout(Duration::from_millis(*ms), &mut fut).await {
            Ok(res) => return res,
            // The probe proved TCP WHILE this attempt was running. Checking only
            // before an attempt missed that case, which is the common one against
            // a distant host: a Sydney pooler probed reachable at 376ms, then
            // attempt 1 was binned at 800ms and the connect landed at 4560ms,
            // having paid TCP + TLS + auth twice. The stall is slowness, so keep
            // the handshake that already has a head start.
            Err(_) if tcp_ok.load(std::sync::atomic::Ordering::Relaxed) => return fut.await,
            Err(_) => log::info!("connect attempt {} exceeded {ms}ms, retrying with a fresh SYN", i + 1),
        }
    }
    // Unbounded here, but the caller's CONNECT_DEADLINE still caps the whole thing.
    attempt().await
}

/// Open a pool and return once ONE connection works; the rest fill in behind it.
///
/// sqlx's `connect_with` does not do that. It opens every `min_connections`
/// connection one after another before returning (sqlx-core 0.8.6,
/// `PoolOptions::connect_with` → `try_min_connections`), so `min_connections(2)`
/// made every connect pay two full handshakes in a row. Against Prisma Postgres,
/// where psql measures a handshake at 3.3-3.5s and a query at ~500ms, that was
/// the 6.2s connect in the log. A lazy pool starts the same min-connections fill
/// as a background task, and the acquire below races it, so the connect costs
/// one handshake and the second connection is ready moments later.
async fn connect_pg_pool(builder: PgPoolOptions, opts: PgConnectOptions) -> Result<PgPool, sqlx::Error> {
    let (host, port) = (opts.get_host().to_string(), opts.get_port());
    let t0 = std::time::Instant::now();
    let pool = builder.connect_lazy_with(opts);
    // Proves the address, TLS and credentials, with the same errors
    // `connect_with` gave; the connection goes back to the pool for the first query.
    let first = pool.acquire().await?;
    let ms = t0.elapsed().as_millis() as u64;
    record_handshake(&host, port, ms);
    // Far host, first time or not: warm the rest now, before `first` is released.
    if ms >= FAR_HANDSHAKE_MS {
        warm_in_parallel(&pool, FAR_POOL);
    }
    drop(first);
    Ok(pool)
}

pub(crate) async fn open_pg(config: &PgConfig) -> Result<PgPool, String> {
    let opts: PgConnectOptions = config
        .connection_url()
        .parse()
        .map_err(|e| format!("Connection failed: {e}"))?;
    // The hostname is deliberately kept rather than swapped for the probed IP:
    // the driver needs it for TLS SNI (managed Postgres behind a proxy routes on
    // it) and the OS resolver caches the lookup anyway.
    let opts = opts.log_slow_statements(LevelFilter::Debug, Duration::from_secs(5));

    // Kill truly runaway queries. 10 min covers bulk inserts / migrations while
    // still bounding accidental full-table scans that would pin a connection.
    //
    // Preferred path: ride statement_timeout in the startup packet (`options=`),
    // which saves one round trip per pooled connection vs an after_connect SET -
    // on a remote host that's a full RTT for every connection the pool opens.
    // Optional session time zone (Settings → Database). Ride it in the startup
    // packet next to statement_timeout so the whole pool inherits it with no
    // extra round trip; "SYSTEM"/empty leaves the server default untouched.
    let tz: Option<String> = config
        .timezone
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty() && !t.eq_ignore_ascii_case("SYSTEM"))
        .map(|t| t.to_string());

    let mut startup: Vec<(&str, String)> = vec![("statement_timeout", "10min".to_string())];
    if let Some(ref tz) = tz {
        startup.push(("TimeZone", tz.clone()));
    }
    let fast_opts = opts.clone().options(startup);

    // Fallback SET (single-quote-escaped) for poolers that reject the `options`
    // startup parameter; run per connection in after_connect below.
    let tz_set: Option<String> = tz
        .as_ref()
        .map(|t| format!("SET TIME ZONE '{}'", t.replace('\'', "''")));

    let explain_opts = opts.clone();
    // Shared with the preflight: set the moment a TCP handshake to this host
    // succeeds, so the retry ladder stops restarting a handshake that is fine.
    let tcp_ok = std::sync::atomic::AtomicBool::new(false);
    let connect = async {
        match retry_fast(&tcp_ok, || connect_pg_pool(pg_pool_for(&config.host, config.port), fast_opts.clone())).await {
            Ok(pool) => Ok(pool),
            // Some poolers (PgBouncer without `ignore_startup_parameters=options`)
            // reject the `options` startup parameter outright. Fall back to the
            // slower after_connect SET so those hosts still connect.
            Err(e) if e.to_string().contains("unsupported startup parameter") => {
connect_pg_pool(
                                pg_pool_for(&config.host, config.port)
                    .after_connect(move |conn, _meta| {
                        let tz_set = tz_set.clone();
                        Box::pin(async move {
                            sqlx::query("SET statement_timeout = '10min'")
                                .execute(&mut *conn)
                                .await?;
                            if let Some(stmt) = tz_set {
                                sqlx::query(&stmt).execute(&mut *conn).await?;
                            }
                            Ok(())
                        })
                    }),
                    opts,
                )
                    .await
                    .map_err(|e| format!("Connection failed: {e}"))
            }
            Err(e) => Err(explain_pg_failure(&explain_opts, format!("Connection failed: {e}")).await),
        }
    };

    connect_racing_probe(&config.host, config.port, Some(&tcp_ok), connect).await
}

pub async fn test_connection(config: PgConfig) -> Result<(), String> {
    let (effective, _tunnel) = resolve_pg_ssh(config).await?;
    let pool = open_pg(&effective).await?;
    sqlx::query("SELECT 1")
        .execute(&pool)
        .await
        .map_err(|e| format!("Query failed: {e}"))?;
    pool.close().await;
    Ok(())
}

pub async fn connect(
    state: State<'_, DbState>,
    tunnel_state: State<'_, TunnelState>,
    config: PgConfig,
) -> Result<(), String> {
    tunnel_state.clear();
    let (effective, tunnel) = resolve_pg_ssh(config).await?;
    let pool = open_pg(&effective).await?;
    close_existing(&state).await;
    set_conn(&state, Some(ActiveConnection::Postgres(pool)))?;
    // Nothing else to do here: `min_connections` fills the pool from the pool's
    // own maintenance task, off the critical path (see `connect_pg_pool`), and the
    // connect returns as soon as the first connection is usable.
    tunnel_state.set(tunnel);
    Ok(())
}

/// Establish an SSH tunnel if `config.ssh` is set, return a direct config pointing
/// at the local forwarded port. The tunnel's lifetime must outlive the connection.
async fn resolve_pg_ssh(config: PgConfig) -> Result<(PgConfig, Option<SshTunnel>), String> {
    if let Some(ref ssh_cfg) = config.ssh {
        let tunnel = SshTunnel::establish(ssh_cfg, &config.host, config.port).await?;
        let local_port = tunnel.local_port;
        let mut direct = config.clone();
        direct.host = "127.0.0.1".to_string();
        direct.port = local_port;
        direct.ssh = None;
        Ok((direct, Some(tunnel)))
    } else {
        Ok((config, None))
    }
}

// ── SQLite connect / test ─────────────────────────────────────────────────────

pub(crate) const NO_SQLITE_FILE: &str = "This SQLite connection has no database file. Choose one (or create a new one) in the connection's settings: without a file, SQLite keeps the data in a temporary file it deletes on disconnect, so nothing created would be kept.";

fn sqlite_url(path: &str) -> String {
    if path == ":memory:" {
        "sqlite::memory:".to_string()
    } else {
        format!("sqlite:{path}")
    }
}

pub(crate) async fn open_sqlite(config: &SqliteConfig) -> Result<SqlitePool, String> {
    // An empty filename is not an error to SQLite: it opens a private temporary
    // database and deletes it when the connection closes. A saved connection
    // with no file therefore connected fine, and every table made in it was
    // gone after the next disconnect.
    if config.file_path.trim().is_empty() {
        return Err(NO_SQLITE_FILE.into());
    }
    let opts: SqliteConnectOptions = sqlite_url(&config.file_path)
        .parse()
        .map_err(|e| format!("SQLite connection failed: {e}"))?;
    // A path to a file that does not exist yet is how a new database is made
    // (the form's "New file" picks one); create it rather than refuse.
    let opts = opts
        .create_if_missing(true)
        .log_slow_statements(LevelFilter::Debug, Duration::from_secs(5));

    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .map_err(|e| format!("SQLite connection failed: {e}"))
}

pub async fn test_sqlite_connection(config: SqliteConfig) -> Result<(), String> {
    let pool = open_sqlite(&config).await?;
    sqlx::query("SELECT 1")
        .execute(&pool)
        .await
        .map_err(|e| format!("Query failed: {e}"))?;
    pool.close().await;
    Ok(())
}

pub async fn connect_sqlite(state: State<'_, DbState>, config: SqliteConfig) -> Result<(), String> {
    let pool = open_sqlite(&config).await?;
    close_existing(&state).await;
    set_conn(&state, Some(ActiveConnection::Sqlite(pool)))
}

// ── MySQL connect / test ──────────────────────────────────────────────────────

pub(crate) async fn open_mysql(config: &MysqlConfig) -> Result<MySqlPool, String> {
    let opts: MySqlConnectOptions = config
        .connection_url()
        .parse()
        .map_err(|e| format!("Connection failed: {e}"))?;
    let opts = opts.log_slow_statements(LevelFilter::Debug, Duration::from_secs(5));

    // Optional session time zone (Settings → Database), applied per connection
    // below; "SYSTEM"/empty leaves the server default in place.
    let tz: Option<String> = config
        .timezone
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty() && !t.eq_ignore_ascii_case("SYSTEM"))
        .map(|t| format!("SET time_zone = '{}'", t.replace('\'', "''")));

    let health = std::sync::Arc::new(PathHealth::default());
    // A far host (TiDB in Tokyo, Railway, PlanetScale) keeps its whole pool
    // open and warms it in parallel behind the first connection: see
    // `pg_pool_for` and `warm_in_parallel`.
    let far = known_handshake_ms(&config.host, config.port).is_some_and(|ms| ms >= FAR_HANDSHAKE_MS);
    if far {
        log::info!("{}:{} is a far host: 4-connection pool, warmed in parallel", config.host, config.port);
    }
    let (host, port) = (config.host.clone(), config.port);
    let builder = MySqlPoolOptions::new()
        // Same rationale as PG: 4 is the real-world ceiling for a desktop app.
        .max_connections(4)
        .min_connections(if far { 4 } else { 0 })
        // See pg_pool_builder: must clear a cold-pool handshake on a slow link.
        .acquire_timeout(Duration::from_secs(10))
        // Keep connections warm for the session (see open_pg for the full rationale)
        // so repeat fetches don't pay a fresh TCP+TLS+auth handshake.
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        // See pg_pool_builder: the pre-acquire ping is a round trip per acquire,
        // so it is gated on idle time instead of run on every one.
        .test_before_acquire(false)
        .before_acquire(move |conn, meta| {
            let health = health.clone();
            Box::pin(async move { ping_if_stale!(health, conn, meta) })
        })
        // Enable ANSI_QUOTES on every connection so double-quoted identifiers
        // ("col") work the same as backtick identifiers (`col`). This makes
        // standard SQL and AI-generated queries work without rewriting syntax.
        .after_connect(move |conn, _meta| {
            let tz = tz.clone();
            Box::pin(async move {
                sqlx::query("SET sql_mode = CONCAT(@@sql_mode, ',ANSI_QUOTES')")
                    .execute(&mut *conn)
                    .await?;
                if let Some(stmt) = tz {
                    sqlx::query(&stmt).execute(&mut *conn).await?;
                }
                Ok(())
            })
        });
    // Lazy plus one acquire, not `connect_with`, for the reason in `connect_pg_pool`:
    // the min-connections fill runs in the background instead of in a row.
    let connect = async move {
        let t0 = std::time::Instant::now();
        let pool = builder.connect_lazy_with(opts);
        let first = pool.acquire().await?;
        let ms = t0.elapsed().as_millis() as u64;
        record_handshake(&host, port, ms);
        if ms >= FAR_HANDSHAKE_MS {
            warm_in_parallel(&pool, 4);
        }
        drop(first);
        Ok::<_, sqlx::Error>(pool)
    };

    connect_racing_probe(
        &config.host,
        config.port,
        None,
        async { connect.await.map_err(|e| format!("Connection failed: {e}")) },
    )
    .await
}

pub async fn test_mysql_connection(config: MysqlConfig) -> Result<(), String> {
    let (effective, _tunnel) = resolve_mysql_ssh(config).await?;
    let pool = open_mysql(&effective).await?;
    sqlx::query("SELECT 1")
        .execute(&pool)
        .await
        .map_err(|e| format!("Query failed: {e}"))?;
    pool.close().await;
    Ok(())
}

pub async fn connect_mysql(
    state: State<'_, DbState>,
    tunnel_state: State<'_, TunnelState>,
    config: MysqlConfig,
) -> Result<(), String> {
    tunnel_state.clear();
    let (effective, tunnel) = resolve_mysql_ssh(config).await?;
    let pool = open_mysql(&effective).await?;
    close_existing(&state).await;
    set_conn(&state, Some(ActiveConnection::Mysql(pool)))?;
    tunnel_state.set(tunnel);
    Ok(())
}

async fn resolve_mysql_ssh(config: MysqlConfig) -> Result<(MysqlConfig, Option<SshTunnel>), String> {
    if let Some(ref ssh_cfg) = config.ssh {
        let tunnel = SshTunnel::establish(ssh_cfg, &config.host, config.port).await?;
        let local_port = tunnel.local_port;
        let mut direct = config.clone();
        direct.host = "127.0.0.1".to_string();
        direct.port = local_port;
        direct.ssh = None;
        Ok((direct, Some(tunnel)))
    } else {
        Ok((config, None))
    }
}

// ── D1 connect / test ─────────────────────────────────────────────────────────

pub async fn test_d1_connection(config: D1Config) -> Result<(), String> {
    crate::db::d1::query(&config, "SELECT 1", vec![]).await?;
    Ok(())
}

pub async fn connect_d1(state: State<'_, DbState>, config: D1Config) -> Result<(), String> {
    // Validate credentials before storing
    test_d1_connection(config.clone()).await?;
    close_existing(&state).await;
    set_conn(&state, Some(ActiveConnection::D1(config)))
}

// ── LibSQL / Turso connect / test ─────────────────────────────────────────────

pub async fn test_libsql_connection(config: LibSqlConfig) -> Result<(), String> {
    crate::db::libsql::query(&config, "SELECT 1", vec![]).await?;
    Ok(())
}

pub async fn connect_libsql(state: State<'_, DbState>, config: LibSqlConfig) -> Result<(), String> {
    test_libsql_connection(config.clone()).await?;
    close_existing(&state).await;
    set_conn(&state, Some(ActiveConnection::LibSql(config)))
}

// ── ClickHouse connect / test ─────────────────────────────────────────────────

pub async fn test_clickhouse_connection(config: ClickhouseConfig) -> Result<(), String> {
    let (host, port) = (config.host.clone(), config.port);
    connect_racing_probe(&host, port, None, async move {
        crate::db::clickhouse::query(&config, "SELECT 1").await?;
        Ok(())
    })
    .await
}

pub async fn connect_clickhouse(state: State<'_, DbState>, config: ClickhouseConfig) -> Result<(), String> {
    // Validate credentials/reachability before storing.
    test_clickhouse_connection(config.clone()).await?;
    close_existing(&state).await;
    set_conn(&state, Some(ActiveConnection::Clickhouse(config)))
}

// ── PostHog connect / test ────────────────────────────────────────────────────

pub async fn test_posthog_connection(config: PosthogConfig) -> Result<(), String> {
    crate::db::posthog::query(&config, "SELECT 1").await.map(|_| ())
}

pub async fn connect_posthog(state: State<'_, DbState>, config: PosthogConfig) -> Result<(), String> {
    test_posthog_connection(config.clone()).await?;
    close_existing(&state).await;
    set_conn(&state, Some(ActiveConnection::Posthog(config)))
}

// ── Redis connect / test ──────────────────────────────────────────────────────

pub async fn test_redis_connection(config: RedisConfig) -> Result<(), String> {
    let (host, port) = (config.host.clone(), config.port);
    connect_racing_probe(&host, port, None, crate::db::redis::ping(&config)).await
}

pub async fn connect_redis(state: State<'_, DbState>, config: RedisConfig) -> Result<(), String> {
    // Validate credentials/reachability before storing.
    test_redis_connection(config.clone()).await?;
    close_existing(&state).await;
    set_conn(&state, Some(ActiveConnection::Redis(config)))
}

// ── DuckDB connect / test ──────────────────────────────────────────────────────

/// Open a DuckDB connection on a blocking thread (the driver is synchronous).
pub(crate) async fn open_duckdb(config: &DuckdbConfig) -> Result<DuckdbHandle, String> {
    let path = config.file_path.clone();
    // Same trap as SQLite's: an empty path opened an in-memory database, so a
    // "Local DuckDB" saved without a file lost everything on disconnect.
    // In-memory is `:memory:`, chosen on purpose.
    if path.trim().is_empty() {
        return Err("This DuckDB connection has no database file. Choose one (or create a new one) in the connection's settings: without a file, everything created would be lost on disconnect.".into());
    }
    tokio::task::spawn_blocking(move || {
        let conn = if path == ":memory:" {
            duckdb::Connection::open_in_memory()
        } else {
            duckdb::Connection::open(&path)
        }
        .map_err(|e| format!("DuckDB connection failed: {e}"))?;
        Ok::<DuckdbHandle, String>(Arc::new(Mutex::new(conn)))
    })
    .await
    .map_err(|e| format!("DuckDB open task failed: {e}"))?
}

pub async fn test_duckdb_connection(config: DuckdbConfig) -> Result<(), String> {
    let handle = open_duckdb(&config).await?;
    crate::db::duckdb::ping(&handle).await
}

pub async fn connect_duckdb(state: State<'_, DbState>, config: DuckdbConfig) -> Result<(), String> {
    let handle = open_duckdb(&config).await?;
    crate::db::duckdb::ping(&handle).await?;
    close_existing(&state).await;
    set_conn(&state, Some(ActiveConnection::Duckdb(handle)))
}

// ── MS SQL Server connect / test ────────────────────────────────────────────────

pub(crate) async fn open_mssql(config: &MssqlConfig) -> Result<MssqlHandle, String> {
    let client =
        connect_racing_probe(&config.host, config.port, None, crate::db::mssql::connect(config)).await?;
    Ok(Arc::new(tokio::sync::Mutex::new(client)))
}

pub async fn test_mssql_connection(config: MssqlConfig) -> Result<(), String> {
    let handle = open_mssql(&config).await?;
    crate::db::mssql::ping(&handle).await
}

pub async fn connect_mssql(state: State<'_, DbState>, config: MssqlConfig) -> Result<(), String> {
    let handle = open_mssql(&config).await?;
    close_existing(&state).await;
    set_conn(&state, Some(ActiveConnection::Mssql(handle)))
}

// ── Disconnect ────────────────────────────────────────────────────────────────

pub async fn disconnect(
    state: State<'_, DbState>,
    tunnel_state: State<'_, TunnelState>,
) -> Result<(), String> {
    // close_existing already sets state to None atomically via take() before
    // starting the (potentially slow) pool close. Calling set_conn(None) again
    // after the async close would race with any concurrent connect_* call that
    // set a new connection while the pool was draining, wiping it out.
    close_existing(&state).await;
    // Kill the SSH tunnel (if any) after the pool is closed.
    tunnel_state.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    // ── TLS in the connection URL ─────────────────────────────────────────────

    fn pg(ssl: bool, mode: Option<&str>, ca: Option<&str>) -> PgConfig {
        PgConfig {
            name: "t".into(),
            host: "db.example.com".into(),
            port: 5432,
            database: "shop".into(),
            user: "ada".into(),
            password: "p@ss word".into(),
            ssl,
            ssl_mode: mode.map(str::to_string),
            ssl_root_cert: ca.map(str::to_string),
            ssh: None,
            timezone: None,
        }
    }

    /// A password ending in `%` and a database name with `%`/`#` must reach
    /// sqlx intact: it percent-decodes the whole URL, so anything left raw was
    /// cut or misread.
    #[test]
    fn pg_url_round_trips_special_characters_through_sqlx() {
        let mut c = pg(false, None, None);
        c.password = "Lm$$pR0D54%".into();
        c.database = "sales%2024#a".into();
        let opts: PgConnectOptions = c.connection_url().parse().expect("url parses");
        assert_eq!(opts.get_database(), Some("sales%2024#a"));
        assert_eq!(opts.get_username(), "ada");
    }

    #[test]
    fn pg_url_omits_tls_params_when_tls_is_off() {
        let url = pg(false, None, None).connection_url();
        assert!(!url.contains("sslmode"), "{url}");
        // Credentials are still encoded - the password here has an @ and a space.
        assert!(url.contains("ada:p%40ss%20word@"), "{url}");
    }

    #[test]
    fn pg_url_keeps_the_boolean_meaning_require() {
        assert!(pg(true, None, None).connection_url().ends_with("?sslmode=require"));
    }

    #[test]
    fn pg_url_takes_the_explicit_mode_over_the_boolean() {
        let url = pg(true, Some("verify-full"), None).connection_url();
        assert!(url.ends_with("?sslmode=verify-full"), "{url}");
    }

    #[test]
    fn pg_url_carries_the_ca_path_encoded() {
        let url = pg(true, Some("verify-ca"), Some("/home/a b/ca.pem")).connection_url();
        assert!(url.contains("sslmode=verify-ca"), "{url}");
        assert!(url.contains("sslrootcert=%2Fhome%2Fa%20b%2Fca.pem"), "{url}");
    }

    #[test]
    fn pg_url_drops_a_ca_path_with_no_mode_to_verify_it() {
        // TLS off means no TLS params at all, CA or not: a stale path left in a
        // saved connection must not silently re-enable verification.
        let url = pg(false, None, Some("/ca.pem")).connection_url();
        assert!(!url.contains("sslrootcert"), "{url}");
    }

    fn my(ssl: bool, mode: Option<&str>, ca: Option<&str>) -> MysqlConfig {
        MysqlConfig {
            name: "t".into(),
            host: "db.example.com".into(),
            port: 3306,
            database: "shop".into(),
            user: "root".into(),
            password: "secret".into(),
            ssl,
            ssl_mode: mode.map(str::to_string),
            ssl_root_cert: ca.map(str::to_string),
            ssh: None,
            timezone: None,
        }
    }

    #[test]
    fn mysql_url_maps_the_boolean_to_its_own_spelling() {
        assert!(my(false, None, None).connection_url().contains("ssl-mode=disabled"));
        assert!(my(true, None, None).connection_url().contains("ssl-mode=required"));
    }

    #[test]
    fn mysql_url_takes_a_verifying_mode_and_its_ca() {
        let url = my(true, Some("verify_identity"), Some("/ca.pem")).connection_url();
        assert!(url.contains("ssl-mode=verify_identity"), "{url}");
        assert!(url.contains("ssl-ca=%2Fca.pem"), "{url}");
    }

    // ── PathHealth ────────────────────────────────────────────────────────────

    #[test]
    fn unmeasured_path_gets_a_starting_deadline() {
        assert_eq!(PathHealth::default().deadline(), PathHealth::UNMEASURED);
    }

    #[test]
    fn deadline_tracks_the_host_that_was_measured() {
        // A nearby host is declared dead quickly; a distant one is given room.
        // A fixed timeout can only be right for one of them.
        let near = PathHealth::default();
        near.record(Duration::from_millis(50));
        assert_eq!(near.deadline(), Duration::from_millis(500));

        let far = PathHealth::default();
        far.record(Duration::from_millis(900));
        assert_eq!(far.deadline(), Duration::from_secs(9));
    }

    #[test]
    fn deadline_stays_inside_its_bounds() {
        // Sub-millisecond localhost must not get a deadline it can trip over,
        // and a pathologically slow sample must not park the pool for a minute.
        let local = PathHealth::default();
        local.record(Duration::from_micros(200));
        assert_eq!(local.deadline(), Duration::from_millis(500));

        let awful = PathHealth::default();
        awful.record(Duration::from_secs(30));
        assert_eq!(awful.deadline(), Duration::from_secs(15));
    }

    #[test]
    fn a_healthy_connection_has_an_order_of_magnitude_of_headroom() {
        // The property that matters: a server would have to get ~10x slower
        // than its own normal before a live connection is thrown away.
        let health = PathHealth::default();
        health.record(Duration::from_millis(46)); // the measured host
        assert!(health.deadline() >= Duration::from_millis(460));
    }

    #[test]
    fn measurement_follows_the_network_without_chasing_one_stall() {
        let health = PathHealth::default();
        health.record(Duration::from_millis(100));
        health.record(Duration::from_millis(200));
        // Halfway, not all the way: one slow sample moves the deadline, but
        // does not hand the next connection a 10x budget on its own.
        assert_eq!(health.state.lock().unwrap().1, 150_000);
    }

    #[test]
    fn one_success_vouches_for_the_pool_briefly() {
        // This is what removes five of the six round trips a table open paid.
        let health = PathHealth::default();
        assert!(!health.recently_verified(), "nothing proven yet");
        health.record(Duration::from_millis(10));
        assert!(health.recently_verified());
    }

    #[test]
    fn a_failed_ping_never_vouches_for_anything() {
        // `record` is only reachable on success, so a pool that has only ever
        // timed out keeps pinging rather than waving connections through.
        let health = PathHealth::default();
        assert!(!health.recently_verified());
        assert_eq!(health.deadline(), PathHealth::UNMEASURED);
    }

    /// The whole point of the probe signal: once TCP is proven, a slow handshake
    /// must be waited out, not restarted. Restarting pays TLS and auth again, and
    /// against a host whose handshake takes ~5s the old ladder threw away 5.3s
    /// before the attempt that finally landed.
    #[tokio::test(start_paused = true)]
    async fn a_proven_tcp_path_is_never_retried() {
        let tries = AtomicUsize::new(0);
        let tcp_ok = AtomicBool::new(true);
        let got = retry_fast(&tcp_ok, || async {
            tries.fetch_add(1, Ordering::Relaxed);
            // Far longer than every budget in the ladder combined.
            tokio::time::sleep(Duration::from_secs(9)).await;
            Ok::<u8, sqlx::Error>(7)
        })
        .await;
        assert_eq!(got.unwrap(), 7);
        assert_eq!(tries.load(Ordering::Relaxed), 1, "a slow but healthy handshake was restarted");
    }

    /// The probe usually lands WHILE the first attempt is running. That attempt
    /// already has TCP and TLS behind it and must be kept, not restarted.
    #[tokio::test(start_paused = true)]
    async fn tcp_proven_mid_attempt_keeps_that_attempt() {
        let tries = AtomicUsize::new(0);
        let tcp_ok = std::sync::Arc::new(AtomicBool::new(false));
        let flag = tcp_ok.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(376)).await;
            flag.store(true, Ordering::Relaxed);
        });
        let got = retry_fast(&tcp_ok, || async {
            tries.fetch_add(1, Ordering::Relaxed);
            tokio::time::sleep(Duration::from_secs(4)).await;
            Ok::<u8, sqlx::Error>(7)
        })
        .await;
        assert_eq!(got.unwrap(), 7);
        assert_eq!(tries.load(Ordering::Relaxed), 1, "a handshake past a proven TCP path was restarted");
    }

    /// While TCP is still unproven a stall really might be a lost SYN, and a fresh
    /// SYN is the only thing that helps - so there the ladder still fires.
    #[tokio::test(start_paused = true)]
    async fn an_unproven_tcp_path_still_gets_a_fresh_syn() {
        let tries = AtomicUsize::new(0);
        let tcp_ok = AtomicBool::new(false);
        let got = retry_fast(&tcp_ok, || async {
            let n = tries.fetch_add(1, Ordering::Relaxed);
            // First attempt stalls past its 800ms budget; the retry answers.
            if n == 0 {
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            Ok::<u8, sqlx::Error>(7)
        })
        .await;
        assert_eq!(got.unwrap(), 7);
        assert_eq!(tries.load(Ordering::Relaxed), 2);
    }

    /// Nothing listening is a definitive answer: it must fail fast rather than
    /// come back Inconclusive (which the caller treats as "keep waiting") or be
    /// retried by the frontend.
    #[tokio::test]
    async fn refused_port_is_definitive() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener); // free the port so connects are refused

        let started = std::time::Instant::now();
        match preflight("127.0.0.1", port).await {
            Preflight::Unreachable(msg) => assert!(msg.contains("Can't reach"), "{msg}"),
            Preflight::Reachable(a) => panic!("closed port reported reachable: {a}"),
            Preflight::Inconclusive(m) => panic!("closed port must be definitive, got: {m}"),
        }
        assert!(started.elapsed() < TCP_BUDGET, "refusal should not burn the probe budget");
    }

    #[tokio::test]
    async fn open_port_returns_the_probed_address() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let want = listener.local_addr().unwrap();
        match preflight("127.0.0.1", want.port()).await {
            Preflight::Reachable(addr) => assert_eq!(addr, want),
            Preflight::Unreachable(m) | Preflight::Inconclusive(m) => panic!("{m}"),
        }
    }

    #[tokio::test]
    async fn unresolvable_host_is_definitive() {
        // .invalid is reserved by RFC 2606 and never resolves.
        match preflight("stroke-no-such-host.invalid", 5432).await {
            Preflight::Unreachable(_) => {}
            Preflight::Reachable(a) => panic!("bogus host resolved to {a}"),
            Preflight::Inconclusive(m) => panic!("DNS failure must be definitive, got: {m}"),
        }
    }

    #[tokio::test]
    async fn empty_host_fails_without_a_lookup() {
        assert!(matches!(preflight("   ", 5432).await, Preflight::Unreachable(_)));
    }

    /// A dual-stack name where one family is dead must still resolve to the live
    /// one. `localhost` is the portable stand-in: it resolves to both ::1 and
    /// 127.0.0.1, and only one of them has a listener here.
    #[tokio::test]
    async fn dual_stack_picks_the_family_that_answers() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let addrs: Vec<_> = tokio::net::lookup_host(format!("localhost:{port}"))
            .await
            .map(|it| it.collect())
            .unwrap_or_default();
        if addrs.len() < 2 {
            return; // single-stack resolver - nothing to prove
        }
        match preflight("localhost", port).await {
            Preflight::Reachable(addr) => assert!(addr.is_ipv4(), "expected the IPv4 listener, got {addr}"),
            Preflight::Unreachable(m) | Preflight::Inconclusive(m) => panic!("{m}"),
        }
    }

    /// The regression that made databases unconnectable: an inconclusive probe
    /// (host slow to answer) must NOT abort a handshake that then succeeds.
    /// 127.0.0.1:<closed port> can't be used here - that's *definitive* - so this
    /// drives the race with a host that swallows SYNs. 192.0.2.0/24 (RFC 5737
    /// TEST-NET-1) is reserved and unrouteable, so the probe stalls.
    #[tokio::test]
    async fn slow_probe_does_not_veto_a_successful_connect() {
        let connect = async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok::<u8, String>(7)
        };
        // The probe is still in flight (or inconclusive) when connect resolves.
        let got = connect_racing_probe("192.0.2.1", 5432, None, connect).await;
        assert_eq!(got, Ok(7), "a completed handshake must win over a stalled probe");
    }

    /// A definitively dead host short-circuits instead of waiting out the
    /// connect: that's what keeps a wrong port from spinning for 20s.
    #[tokio::test]
    async fn definitive_probe_failure_short_circuits() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let started = std::time::Instant::now();
        // A connect that would never finish on its own.
        let never = async {
            tokio::time::sleep(CONNECT_DEADLINE * 2).await;
            Ok::<u8, String>(0)
        };
        let got = connect_racing_probe("127.0.0.1", port, None, never).await;
        assert!(got.is_err(), "closed port must fail, got {got:?}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "should fail fast, took {:?}",
            started.elapsed()
        );
    }

    /// The whole attempt is bounded, and the probe's diagnosis is what surfaces.
    /// Paused clock so the deadline fires without waiting CONNECT_DEADLINE.
    #[tokio::test(start_paused = true)]
    async fn deadline_bounds_a_stalled_connect() {
        let never = async {
            tokio::time::sleep(CONNECT_DEADLINE * 3).await;
            Ok::<u8, String>(0)
        };
        let got = connect_racing_probe("192.0.2.1", 5432, None, never).await;
        assert!(got.is_err(), "a stalled connect must not hang forever");
    }
}

/// Resolve hosts into the OS resolver cache, ahead of any connect.
///
/// Measured on a cold cache, `preflight` spent 4147ms in `lookup_host` while the
/// same lookup took 58ms once cached - the connect tracked it almost exactly,
/// because the driver has to resolve the same name again. The app knows which
/// hosts the user might pick (the saved connections) long before they click, so
/// it can pay that cost while nobody is waiting.
///
/// Best effort and non-blocking: failures are ignored, since this only primes a
/// cache. Never let it delay anything.
#[tauri::command]
pub async fn prewarm_dns(hosts: Vec<String>) {
    for host in hosts {
        let host = host.trim().to_string();
        if host.is_empty() {
            continue;
        }
        tokio::spawn(async move {
            let t = std::time::Instant::now();
            let target = format!("{host}:0");
            match tokio::time::timeout(DNS_BUDGET, tokio::net::lookup_host(target)).await {
                Ok(Ok(addrs)) => {
                    let n = addrs.count();
                    log::info!("prewarm dns {host} -> {n} addr(s) in {}ms", t.elapsed().as_millis());
                }
                _ => log::info!("prewarm dns {host} did not resolve in {}ms", t.elapsed().as_millis()),
            }
        });
    }
}

/// Live checks for the release-ping patch in `vendor/sqlx-postgres`. Run with
/// `STROKE_PG_URL=postgres://… cargo test --lib pg_release_live -- --ignored --nocapture`.
#[cfg(test)]
mod pg_release_live {
    use futures::TryStreamExt;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::{Connection, Row};

    async fn pool() -> sqlx::PgPool {
        let url = std::env::var("STROKE_PG_URL").expect("STROKE_PG_URL");
        // One connection, so every step below reuses the same one.
        PgPoolOptions::new().max_connections(1).test_before_acquire(false).connect(&url).await.unwrap()
    }

    #[tokio::test]
    #[ignore]
    async fn a_dropped_transaction_still_rolls_back() {
        let pool = pool().await;
        {
            let mut tx = pool.begin().await.unwrap();
            // Transaction-local: gone once the transaction ends, still set if it didn't.
            sqlx::query("SELECT set_config('stroke.probe', 'in_tx', true)").execute(&mut *tx).await.unwrap();
            // Dropped without commit: sqlx queues a ROLLBACK.
        }
        let mut conn = pool.acquire().await.unwrap();
        let probe: Option<String> = sqlx::query("SELECT current_setting('stroke.probe', true)").fetch_one(&mut *conn).await.unwrap().get(0);
        assert_ne!(probe.as_deref(), Some("in_tx"), "the connection came back still inside the transaction");
        conn.ping().await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn a_half_read_stream_is_cleaned_up() {
        let pool = pool().await;
        {
            let mut conn = pool.acquire().await.unwrap();
            let mut rows = sqlx::query("SELECT g FROM generate_series(1, 100000) g").fetch(&mut *conn);
            let _first = rows.try_next().await.unwrap();
            // Dropped mid result set.
        }
        let n: i64 = sqlx::query("SELECT 41::bigint + 1").fetch_one(&pool).await.unwrap().get(0);
        assert_eq!(n, 42);
    }

    #[tokio::test]
    #[ignore]
    async fn sequential_queries_reuse_one_connection() {
        let pool = pool().await;
        let t = std::time::Instant::now();
        for _ in 0..5 {
            sqlx::query("SELECT 1").execute(&pool).await.unwrap();
        }
        println!("5 sequential queries on one pooled connection: {}ms", t.elapsed().as_millis());
    }
}
