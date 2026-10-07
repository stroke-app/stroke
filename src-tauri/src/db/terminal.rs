/*!
The terminal tab: the connected engine's own command-line client (`psql`,
`mysql`, `sqlite3`, `sqlcmd`, `redis-cli`) running in a pseudo-terminal, its
output streamed to xterm.js in the window byte for byte.

Nothing of the client is reimplemented. `\d+`, `\x`, tab completion, the pager
and the user's own `.psqlrc` all work because the real program is running. What
lives here is the part a desktop app has to get right around it:

- finding the binary, since a windowed app is not handed the user's shell PATH;
- passing the saved password through the variable the client reads, never argv
  (any user on the machine can read another process's argv with `ps`);
- opening the same SSH tunnel the connection uses, for as long as the client runs;
- killing the client when its tab closes or the app quits, so no `psql` is left
  holding a server connection (or an open transaction) behind the window.
*/

use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, Child, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::connection::{
    resolve_mysql_ssh, resolve_pg_ssh, AnyConnectionConfig, MssqlConfig, MysqlConfig, PgConfig,
    RedisConfig, SqliteConfig,
};
use super::ssh_tunnel::SshTunnel;

// ── Clients ───────────────────────────────────────────────────────────────────

/// An engine's command-line client.
struct Client {
    /// What the user calls it.
    name: &'static str,
    /// Executables that can serve, preferred first.
    binaries: &'static [&'static str],
    /// Where to get it, for the "not installed" state.
    install: &'static str,
}

const PSQL_INSTALL: &str = if cfg!(target_os = "macos") {
    "brew install libpq"
} else if cfg!(windows) {
    "Install PostgreSQL from postgresql.org/download/windows. The Command Line Tools component alone is enough."
} else {
    "Install the PostgreSQL client: postgresql-client on Debian and Ubuntu, postgresql on Arch and Fedora."
};

const MYSQL_INSTALL: &str = if cfg!(target_os = "macos") {
    "brew install mysql-client"
} else if cfg!(windows) {
    "Install the MySQL client from dev.mysql.com/downloads/installer, or MariaDB from mariadb.org/download. Either client works."
} else {
    "Install a MySQL client: mysql-client or mariadb-client on Debian and Ubuntu, mariadb-clients on Arch."
};

const SQLITE_INSTALL: &str = if cfg!(target_os = "macos") {
    "brew install sqlite"
} else if cfg!(windows) {
    "winget install SQLite.SQLite"
} else {
    "Install sqlite3 on Debian and Ubuntu, sqlite on Arch and Fedora."
};

const SQLCMD_INSTALL: &str = if cfg!(target_os = "macos") {
    "brew install sqlcmd"
} else if cfg!(windows) {
    "winget install sqlcmd"
} else {
    "Install go-sqlcmd from github.com/microsoft/go-sqlcmd/releases."
};

const REDIS_INSTALL: &str = if cfg!(target_os = "macos") {
    "brew install redis"
} else if cfg!(windows) {
    "redis-cli has no native Windows build. Memurai ships one, or run it under WSL."
} else {
    "Install redis-tools on Debian and Ubuntu, redis or valkey on Arch and Fedora."
};

/// The client for an engine, or why it has none.
fn client_for(config: &AnyConnectionConfig) -> Result<Client, String> {
    let client = |name, binaries, install| Ok(Client { name, binaries, install });
    match config {
        AnyConnectionConfig::Postgres(_) => client("psql", &["psql"], PSQL_INSTALL),
        // MariaDB's client first: where it is installed, `mysql` is only its
        // deprecated alias and prints a warning on every start. Oracle's MySQL
        // never ships a `mariadb`, so on those systems `mysql` is the one found.
        AnyConnectionConfig::Mysql(_) => client("mysql", &["mariadb", "mysql"], MYSQL_INSTALL),
        AnyConnectionConfig::Sqlite(c) if c.file_path.trim().is_empty() => {
            Err(super::connection::NO_SQLITE_FILE.into())
        }
        AnyConnectionConfig::Sqlite(c) if c.file_path.trim() == ":memory:" => Err(
            "This is an in-memory database that lives inside Stroke, so no other process can open it."
                .into(),
        ),
        AnyConnectionConfig::Sqlite(_) => client("sqlite3", &["sqlite3"], SQLITE_INSTALL),
        AnyConnectionConfig::Mssql(_) => client("sqlcmd", &["sqlcmd"], SQLCMD_INSTALL),
        AnyConnectionConfig::Redis(_) => client("redis-cli", &["redis-cli", "valkey-cli"], REDIS_INSTALL),
        AnyConnectionConfig::Duckdb(_) => Err(
            "DuckDB lets one process at a time open a database file, and Stroke already has this one open."
                .into(),
        ),
        AnyConnectionConfig::D1(_) => Err(
            "D1 has no interactive shell: wrangler d1 execute runs one command and exits.".into(),
        ),
        AnyConnectionConfig::Libsql(_) => Err(
            "Turso's shell signs in through the Turso CLI, not through this connection's token.".into(),
        ),
        AnyConnectionConfig::Clickhouse(_) => Err(
            "clickhouse-client speaks the native protocol, on a different port from the HTTP interface this connection uses."
                .into(),
        ),
        AnyConnectionConfig::Posthog(_) => {
            Err("PostHog is queried over its HTTP API and has no shell.".into())
        }
    }
}

/// How to install `client` here: the one-line command for this Linux's package
/// manager when it is one we know (so the page can offer it to copy), the
/// general advice otherwise.
fn install_hint(client: &Client) -> String {
    #[cfg(target_os = "linux")]
    if let Some(cmd) = linux_install(client.name) {
        return cmd.to_string();
    }
    client.install.to_string()
}

/// The package that carries each client, by distribution family, read from
/// `/etc/os-release` (`ID`, or `ID_LIKE` for derivatives such as Omarchy).
#[cfg(target_os = "linux")]
fn linux_install(client: &str) -> Option<&'static str> {
    let release = std::fs::read_to_string("/etc/os-release").ok()?;
    let ids: Vec<String> = release
        .lines()
        .filter_map(|l| l.strip_prefix("ID=").or_else(|| l.strip_prefix("ID_LIKE=")))
        .flat_map(|v| v.trim_matches('"').split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .collect();
    let is = |id: &str| ids.iter().any(|i| i == id);
    let family = if is("arch") {
        "arch"
    } else if is("debian") || is("ubuntu") {
        "debian"
    } else if is("fedora") || is("rhel") {
        "fedora"
    } else {
        return None;
    };
    Some(match (family, client) {
        ("arch", "psql") => "sudo pacman -S postgresql-libs",
        ("arch", "mysql") => "sudo pacman -S mariadb-clients",
        ("arch", "sqlite3") => "sudo pacman -S sqlite",
        ("arch", "redis-cli") => "sudo pacman -S valkey",
        ("debian", "psql") => "sudo apt install postgresql-client",
        ("debian", "mysql") => "sudo apt install mariadb-client",
        ("debian", "sqlite3") => "sudo apt install sqlite3",
        ("debian", "redis-cli") => "sudo apt install redis-tools",
        ("fedora", "psql") => "sudo dnf install postgresql",
        ("fedora", "mysql") => "sudo dnf install mariadb",
        ("fedora", "sqlite3") => "sudo dnf install sqlite",
        ("fedora", "redis-cli") => "sudo dnf install valkey",
        _ => return None,
    })
}

// ── Finding the binary ────────────────────────────────────────────────────────

/// The first of `names` found on the user's PATH, then in the places these
/// clients install to without touching PATH.
async fn find_binary(names: &[&str]) -> Option<PathBuf> {
    let path = crate::omniroute::user_path().await;
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    dirs.extend(install_dirs());
    names.iter().find_map(|name| {
        let file = if cfg!(windows) { format!("{name}.exe") } else { (*name).to_string() };
        dirs.iter().map(|d| d.join(&file)).find(|p| is_executable(p))
    })
}

/// Client installs that stay off PATH: keg-only Homebrew formulas (libpq,
/// mysql-client), Postgres.app, Debian's per-version PostgreSQL directories,
/// and the Windows installers, which give every version a folder of its own.
fn install_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(not(windows))]
    {
        for d in [
            "/opt/homebrew/opt/libpq/bin",
            "/usr/local/opt/libpq/bin",
            "/opt/homebrew/opt/mysql-client/bin",
            "/usr/local/opt/mysql-client/bin",
            "/Applications/Postgres.app/Contents/Versions/latest/bin",
            "/usr/local/mysql/bin",
            "/opt/mssql-tools18/bin",
            "/opt/mssql-tools/bin",
        ] {
            out.push(PathBuf::from(d));
        }
        out.extend(versioned(Path::new("/usr/lib/postgresql"), "", "bin"));
    }
    #[cfg(windows)]
    for var in ["ProgramFiles", "ProgramFiles(x86)"] {
        let Some(root) = std::env::var_os(var).map(PathBuf::from) else {
            continue;
        };
        out.extend(versioned(&root.join("PostgreSQL"), "", "bin"));
        out.extend(versioned(&root.join("MySQL"), "MySQL Server", "bin"));
        out.extend(versioned(&root, "MariaDB", "bin"));
        out.push(root.join("SqlCmd"));
        out.extend(versioned(
            &root.join("Microsoft SQL Server").join("Client SDK").join("ODBC"),
            "",
            "Tools\\Binn",
        ));
    }
    out
}

/// `root`'s subdirectories whose names start with `prefix`, each joined with `suffix`, newest
/// version first. Versions compare as numbers, so `17` sorts above `9.6`.
fn versioned(root: &Path, prefix: &str, suffix: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<(Vec<u32>, PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            if !name.starts_with(prefix) {
                return None;
            }
            let version = name
                .split(|c: char| !c.is_ascii_digit())
                .filter_map(|part| part.parse().ok())
                .collect();
            Some((version, e.path().join(suffix)))
        })
        .collect();
    dirs.sort_by(|a, b| b.0.cmp(&a.0));
    dirs.into_iter().map(|(_, path)| path).collect()
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// The first line of `<binary> --version`, e.g. `psql (PostgreSQL) 17.2`.
/// `None` when the client has no such flag (the ODBC build of sqlcmd).
async fn client_version(binary: &Path) -> Option<String> {
    let mut cmd = tokio::process::Command::new(binary);
    cmd.arg("--version").stdin(Stdio::null()).kill_on_drop(true);
    crate::proc::quiet(&mut cmd);
    let out = tokio::time::timeout(Duration::from_secs(3), cmd.output())
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(if out.stdout.is_empty() { &out.stderr } else { &out.stdout })
        .into_owned();
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    // sqlite3 prints only `3.46.1 2024-08-13 09:16:08 <source hash>`.
    let line = match binary.file_stem().and_then(|s| s.to_str()) {
        Some("sqlite3") => format!("SQLite {}", line.split_whitespace().next()?),
        _ => line.to_string(),
    };
    Some(line.chars().take(120).collect())
}

// ── Launching ─────────────────────────────────────────────────────────────────

/// How to start one client against one connection.
#[derive(Default)]
struct Launch {
    args: Vec<String>,
    env: Vec<(&'static str, String)>,
    /// Inherited variables that would point the client somewhere else, or hand
    /// another server's password to this one.
    unset: &'static [&'static str],
    /// The tunnel the client connects through. It closes when this drops, so it
    /// moves into the session and lives exactly as long as the client.
    tunnel: Option<SshTunnel>,
}

/// psql's default prompts (`db=# `, `db-# `) with the database name in the
/// theme's blue and the transaction marker (`*`, `!`) in its yellow. `%[ %]`
/// tell readline the escapes take no width, or line editing goes off by their
/// length. A `.psqlrc` still wins: psql reads it after these are set.
///
/// Each prompt is also wrapped in OSC 133 marks (the shell-integration codes
/// terminals use): `A` where the prompt starts, `A;k=s` for a continuation
/// line, `B` where the input starts. The page uses them to find what is being
/// typed, for its suggestions, and to know a statement is still open.
const PSQL_PROMPT1: &str =
    "%[%033]133;A%007%]%[%033[1;34m%]%/%[%033[0m%]%R%[%033[33m%]%x%[%033[0m%]%# %[%033]133;B%007%]";
const PSQL_PROMPT2: &str = "%[%033]133;A;k=s%007%]%[%033[2m%]%/%R%x%#%[%033[0m%] %[%033]133;B%007%]";

/// Table borders and the NULL marker. Unicode borders and `∅` everywhere but
/// Windows: psql writes those borders as UTF-8 whatever its client encoding,
/// and the Windows console reads its output in the ANSI code page (1252), so
/// every `│` came out as `â”‚`; `∅` reached it through a 1252 argv as `?`. There
/// psql keeps its own ASCII borders, and NULL shows as `(null)`, which read
/// the same in any code page.
fn psql_display_args(windows: bool) -> Vec<String> {
    if windows {
        vec!["--pset=null=(null)".to_string()]
    } else {
        // NULL apart from the empty string.
        vec!["--pset=linestyle=unicode".to_string(), "--pset=null=\u{2205}".to_string()]
    }
}

async fn pg_launch(config: PgConfig) -> Result<Launch, String> {
    let (c, tunnel) = resolve_pg_ssh(config).await?;
    let mut env = vec![
        ("PGHOST", c.host.clone()),
        ("PGPORT", c.port.to_string()),
        ("PGUSER", c.user.clone()),
        ("PGDATABASE", c.database.clone()),
        ("PGAPPNAME", "Stroke".to_string()),
    ];
    if !c.password.is_empty() {
        env.push(("PGPASSWORD", c.password.clone()));
    }
    // The rule `PgConfig::connection_url` follows: `sslMode` wins, the old `ssl`
    // flag means `require`, and neither leaves libpq on its default, `prefer`.
    let mode = non_empty(&c.ssl_mode).unwrap_or_else(|| if c.ssl { "require".into() } else { String::new() });
    if !mode.is_empty() {
        env.push(("PGSSLMODE", mode));
        if let Some(ca) = non_empty(&c.ssl_root_cert) {
            env.push(("PGSSLROOTCERT", ca));
        }
    }
    if let Some(tz) = session_timezone(&c.timezone) {
        env.push(("PGTZ", tz));
    }
    Ok(Launch {
        args: vec![
            format!("--set=PROMPT1={PSQL_PROMPT1}"),
            format!("--set=PROMPT2={PSQL_PROMPT2}"),
            // The tab keeps its own scrollback, so results scroll like any other
            // output. With the pager on, even a one-row result could open `less`
            // and leave the user at `(END)` with no idea it wants `q`.
            "--pset=pager=off".to_string(),
            // A terminal cannot scroll sideways, so a result wider than the tab
            // wrapped every row into a mess (`\l` is ~130 columns). With
            // `expanded=auto` such a result prints one record per block instead,
            // and narrow results stay plain tables. (`format=wrapped` was tried
            // and dropped: it squeezes columns and breaks names mid-word.)
            "--pset=expanded=auto".to_string(),
        ]
        .into_iter()
        .chain(psql_display_args(cfg!(windows)))
        .collect(),
        env,
        unset: &["PGSERVICE", "PGPASSWORD", "PGSSLMODE", "PGSSLROOTCERT", "PGTZ", "PGOPTIONS"],
        tunnel,
    })
}

async fn mysql_launch(config: MysqlConfig, mariadb: bool) -> Result<Launch, String> {
    let (c, tunnel) = resolve_mysql_ssh(config).await?;
    let mut args = vec![
        format!("--host={}", c.host),
        format!("--port={}", c.port),
        format!("--user={}", c.user),
        "--default-character-set=utf8mb4".to_string(),
        // The terminal cannot scroll sideways: a result wider than it is shown
        // one column per line (as `\G` does) instead of wrapping every row.
        "--auto-vertical-output".to_string(),
    ];
    if !c.database.is_empty() {
        args.push(format!("--database={}", c.database));
    }
    let mode = non_empty(&c.ssl_mode)
        .map(|m| m.to_ascii_uppercase())
        .unwrap_or_else(|| if c.ssl { "REQUIRED" } else { "DISABLED" }.into());
    if mariadb {
        // MariaDB's client has no --ssl-mode; it spells the same choices as switches.
        match mode.as_str() {
            "DISABLED" => args.push("--skip-ssl".into()),
            "PREFERRED" => {}
            "VERIFY_CA" | "VERIFY_IDENTITY" => {
                args.extend(["--ssl".into(), "--ssl-verify-server-cert".into()]);
            }
            _ => args.extend(["--ssl".into(), "--skip-ssl-verify-server-cert".into()]),
        }
    } else {
        args.push(format!("--ssl-mode={mode}"));
    }
    if mode != "DISABLED" {
        if let Some(ca) = non_empty(&c.ssl_root_cert) {
            args.push(format!("--ssl-ca={ca}"));
        }
    }
    if let Some(tz) = session_timezone(&c.timezone) {
        args.push(format!("--init-command=SET time_zone = '{}'", tz.replace('\'', "''")));
    }
    let mut env = Vec::new();
    // Deprecated in MySQL 8 but still read by it and by every MariaDB client,
    // and the only way to hand over a password that is neither argv nor a file.
    if !c.password.is_empty() {
        env.push(("MYSQL_PWD", c.password.clone()));
    }
    Ok(Launch { args, env, unset: &["MYSQL_PWD", "MYSQL_HOST", "MYSQL_TCP_PORT"], tunnel })
}

/// sqlite3's default `list` mode prints rows as `a|b|c` with no header. Box mode
/// draws a table, and from 3.38 wraps long values inside their column; older
/// builds get the box alone, builds before 3.33 neither. A `~/.sqliterc` means
/// the user has chosen, so it is left to decide.
fn sqlite_launch(c: &SqliteConfig, version: Option<&str>) -> Launch {
    let has_rc = home_dir().is_some_and(|h| h.join(".sqliterc").exists());
    let v: Vec<u32> = version
        .and_then(|v| v.split_whitespace().nth(1))
        .map(|v| v.split('.').filter_map(|p| p.parse().ok()).collect())
        .unwrap_or_default();
    let at_least = |major: u32, minor: u32| v.len() >= 2 && (v[0], v[1]) >= (major, minor);
    let mut args = Vec::new();
    if !has_rc && at_least(3, 38) {
        args.extend(["-cmd", ".mode box --wrap 60", "-cmd", ".nullvalue \u{2205}"].map(String::from));
    } else if !has_rc && at_least(3, 33) {
        args.push("-box".into());
    }
    args.push(c.file_path.trim().to_string());
    Launch { args, ..Default::default() }
}

fn mssql_launch(c: &MssqlConfig) -> Launch {
    let mut args = vec!["-S".to_string(), format!("{},{}", c.host, c.port)];
    if !c.database.is_empty() {
        args.extend(["-d".into(), c.database.clone()]);
    }
    let mut env = Vec::new();
    if c.user.is_empty() {
        // No login saved: Windows authentication, as the connection itself does.
        args.push("-E".into());
    } else {
        args.extend(["-U".into(), c.user.clone()]);
        // Read by both sqlcmd builds, the ODBC one and go-sqlcmd.
        env.push(("SQLCMDPASSWORD", c.password.clone()));
    }
    if c.encrypt {
        args.push("-N".into());
    }
    if c.trust_cert {
        args.push("-C".into());
    }
    Launch {
        args,
        env,
        unset: &["SQLCMDPASSWORD", "SQLCMDSERVER", "SQLCMDUSER", "SQLCMDDBNAME"],
        tunnel: None,
    }
}

fn redis_launch(c: &RedisConfig) -> Launch {
    let mut args = vec!["-h".to_string(), c.host.clone(), "-p".to_string(), c.port.to_string()];
    if c.db != 0 {
        args.extend(["-n".into(), c.db.to_string()]);
    }
    if c.tls {
        args.push("--tls".into());
    }
    let mut env = Vec::new();
    if let Some(password) = c.password.as_deref().filter(|p| !p.is_empty()) {
        env.push(("REDISCLI_AUTH", password.to_string()));
        env.push(("VALKEYCLI_AUTH", password.to_string()));
    }
    Launch { args, env, unset: &["REDISCLI_AUTH", "VALKEYCLI_AUTH"], tunnel: None }
}

fn non_empty(value: &Option<String>) -> Option<String> {
    value.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(String::from)
}

/// The connection's session time zone, unless it is the server's own.
fn session_timezone(value: &Option<String>) -> Option<String> {
    non_empty(value).filter(|tz| !tz.eq_ignore_ascii_case("SYSTEM"))
}

fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_dir())
}

// ── Sessions ──────────────────────────────────────────────────────────────────
//
// A session's bytes travel over a WebSocket on 127.0.0.1, not over Tauri IPC.
// On Linux every invoke and every Channel message goes through the GTK main
// loop of the UI process (a custom-scheme request one way, a `webview.eval` the
// other), so each keystroke queued behind that loop twice before its echo
// showed, and typing lagged. WebKit runs a WebSocket in its network process,
// beside the page, with no main-thread hop on either side. Each session has its
// own random token in the URL, so no other local process can attach to it.

/// What the reader and the waiter hand the socket.
enum Out {
    Data(Vec<u8>),
    /// The client exited. All of its output was queued before this.
    Exit(Option<u32>),
}

struct Session {
    master: Box<dyn MasterPty + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    _tunnel: Option<SshTunnel>,
}

/// The socket's half of a session, waiting for the page to connect. Output the
/// client prints before then (psql's banner, or a refused login) queues here.
struct Attach {
    token: String,
    output: tokio::sync::mpsc::UnboundedReceiver<Out>,
    /// Keystrokes for the writer thread. A queue rather than a write in place: a
    /// client busy with a query stops reading its input, and a write into a full
    /// PTY would block the socket and with it the client's output.
    input: mpsc::Sender<Vec<u8>>,
    created: Instant,
}

type Sessions = Arc<Mutex<HashMap<String, Session>>>;
type Pending = Arc<Mutex<HashMap<String, Attach>>>;

#[derive(Clone)]
struct Shared {
    sessions: Sessions,
    pending: Pending,
}

#[derive(Default)]
pub struct TerminalState {
    sessions: Sessions,
    pending: Pending,
    /// The socket server's port. It starts with the first session.
    port: tokio::sync::OnceCell<u16>,
}

impl TerminalState {
    /// Kill every client. Run on app exit: a `psql` left behind keeps its server
    /// connection, and any transaction it has open, until the server notices.
    pub fn kill_all(&self) {
        let sessions: Vec<Session> = match self.sessions.lock() {
            Ok(mut map) => map.drain().map(|(_, s)| s).collect(),
            Err(_) => return,
        };
        for mut session in sessions {
            let _ = session.killer.kill();
        }
    }

    fn shared(&self) -> Shared {
        Shared { sessions: self.sessions.clone(), pending: self.pending.clone() }
    }
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A page that never connects (it was closed while the client started) leaves
/// its session waiting; the next open reaps it after this long.
const ATTACH_TIMEOUT: Duration = Duration::from_secs(60);

/// Output is coalesced into frames of at most this many bytes.
const MAX_FRAME: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalClient {
    /// `psql`, `mysql`, `mariadb` and so on. Empty when the engine has no shell.
    pub name: String,
    /// The binary that runs. `None` when the client is not installed.
    pub path: Option<String>,
    /// Its `--version` line, e.g. `psql (PostgreSQL) 17.2`.
    pub version: Option<String>,
    /// How to install the client, or why this engine has none.
    pub hint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSession {
    pub id: String,
    /// `ws://127.0.0.1:<port>/terminal/<id>?token=<token>`. Output arrives as
    /// binary frames and keystrokes go back as binary frames; text frames carry
    /// control messages, `{"resize":[cols,rows]}` in and `{"exit":code}` out.
    pub url: String,
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Which client a connection gets, and whether it is installed.
#[tauri::command]
pub async fn terminal_client(config: AnyConnectionConfig) -> Result<TerminalClient, String> {
    let client = match client_for(&config) {
        Ok(client) => client,
        Err(why) => {
            return Ok(TerminalClient { name: String::new(), path: None, version: None, hint: why })
        }
    };
    let Some(binary) = find_binary(client.binaries).await else {
        return Ok(TerminalClient {
            name: client.name.into(),
            path: None,
            version: None,
            hint: install_hint(&client),
        });
    };
    Ok(TerminalClient {
        name: binary.file_stem().and_then(|s| s.to_str()).unwrap_or(client.name).to_string(),
        version: client_version(&binary).await,
        path: Some(binary.display().to_string()),
        hint: install_hint(&client),
    })
}

/// Start the connection's client in a `cols` x `rows` terminal, and return the
/// socket the page talks to it over.
#[tauri::command]
pub async fn terminal_open(
    state: State<'_, TerminalState>,
    config: AnyConnectionConfig,
    cols: u16,
    rows: u16,
) -> Result<TerminalSession, String> {
    open_session(&state, config, cols, rows).await
}

/// Kill the client. A connected socket gets its exit message first.
#[tauri::command]
pub async fn terminal_close(state: State<'_, TerminalState>, id: String) -> Result<(), String> {
    if let Ok(mut pending) = state.pending.lock() {
        pending.remove(&id);
    }
    let session = state.sessions.lock().map_err(|e| e.to_string())?.remove(&id);
    if let Some(mut session) = session {
        let _ = session.killer.kill();
    }
    Ok(())
}

async fn open_session(
    state: &TerminalState,
    config: AnyConnectionConfig,
    cols: u16,
    rows: u16,
) -> Result<TerminalSession, String> {
    reap_unattached(state);
    let client = client_for(&config)?;
    let binary = find_binary(client.binaries)
        .await
        .ok_or_else(|| format!("{} is not installed. {}", client.name, install_hint(&client)))?;
    let launch = match config {
        AnyConnectionConfig::Postgres(c) => pg_launch(c).await?,
        AnyConnectionConfig::Mysql(c) => {
            let stem = binary.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            let mariadb = stem == "mariadb"
                || client_version(&binary)
                    .await
                    .is_some_and(|v| v.to_ascii_lowercase().contains("mariadb"));
            mysql_launch(c, mariadb).await?
        }
        AnyConnectionConfig::Sqlite(c) => sqlite_launch(&c, client_version(&binary).await.as_deref()),
        AnyConnectionConfig::Mssql(c) => mssql_launch(&c),
        AnyConnectionConfig::Redis(c) => redis_launch(&c),
        _ => return Err("This connection has no terminal client.".into()),
    };
    let shared = state.shared();
    let port = *state.port.get_or_try_init(|| serve(shared)).await?;

    let Spawned { master, mut child, reader, writer } =
        spawn_client(&binary, client.name, &launch, cols, rows).await?;

    let id = format!("term-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    let mut token = [0u8; 16];
    getrandom::getrandom(&mut token).map_err(|e| e.to_string())?;
    let token = hex::encode(token);
    let (input, keystrokes) = mpsc::channel::<Vec<u8>>();
    let (out_tx, output) = tokio::sync::mpsc::unbounded_channel::<Out>();
    std::thread::spawn(move || write_loop(writer, keystrokes));
    let reader_out = out_tx.clone();
    let reader_thread = std::thread::spawn(move || read_loop(reader, reader_out));

    // Both in their maps before the waiter starts, so a client that exits at
    // once (a refused login) is still found and released by it.
    let session = Session { master, killer: child.clone_killer(), _tunnel: launch.tunnel };
    state.sessions.lock().map_err(|e| e.to_string())?.insert(id.clone(), session);
    let attach = Attach { token: token.clone(), output, input, created: Instant::now() };
    state.pending.lock().map_err(|e| e.to_string())?.insert(id.clone(), attach);

    let sessions = state.sessions.clone();
    let session_id = id.clone();
    std::thread::spawn(move || {
        let status = child.wait();
        // ConPTY keeps its output pipe open after the client exits, until the
        // pseudo console itself closes. Dropping the session closes it, which
        // lets the reader drain the last bytes and finish before the exit.
        let session = sessions.lock().ok().and_then(|mut map| map.remove(&session_id));
        drop(session);
        let _ = reader_thread.join();
        let _ = out_tx.send(Out::Exit(status.ok().map(|s| s.exit_code())));
    });

    Ok(TerminalSession { url: format!("ws://127.0.0.1:{port}/terminal/{id}?token={token}"), id })
}

/// Kill the sessions whose page never connected.
fn reap_unattached(state: &TerminalState) {
    let stale: Vec<String> = match state.pending.lock() {
        Ok(mut pending) => {
            let ids: Vec<String> = pending
                .iter()
                .filter(|(_, a)| a.created.elapsed() > ATTACH_TIMEOUT)
                .map(|(id, _)| id.clone())
                .collect();
            for id in &ids {
                pending.remove(id);
            }
            ids
        }
        Err(_) => return,
    };
    if let Ok(mut sessions) = state.sessions.lock() {
        for id in stale {
            if let Some(mut session) = sessions.remove(&id) {
                let _ = session.killer.kill();
            }
        }
    }
}

// ── Socket ────────────────────────────────────────────────────────────────────

async fn serve(shared: Shared) -> Result<u16, String> {
    use axum::routing::get;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| format!("Could not open the terminal socket: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let app = axum::Router::new().route("/terminal/:id", get(socket_route)).with_state(shared);
    tauri::async_runtime::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            log::error!("terminal socket server stopped: {e}");
        }
    });
    Ok(port)
}

#[derive(Deserialize)]
struct TokenQuery {
    token: String,
}

async fn socket_route(
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::extract::Query(query): axum::extract::Query<TokenQuery>,
    axum::extract::State(shared): axum::extract::State<Shared>,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let attach = shared.pending.lock().ok().and_then(|mut pending| {
        let matches = pending.get(&id).is_some_and(|a| a.token == query.token);
        if matches { pending.remove(&id) } else { None }
    });
    let Some(attach) = attach else {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    };
    ws.on_upgrade(move |socket| pump(socket, id, attach, shared.sessions))
}

#[derive(Deserialize)]
struct Control {
    resize: Option<(u16, u16)>,
    /// A line for the log: the page reports its keystroke-to-echo latency.
    trace: Option<String>,
}

/// Move bytes both ways until the client exits or the page goes away.
async fn pump(
    mut socket: axum::extract::ws::WebSocket,
    id: String,
    attach: Attach,
    sessions: Sessions,
) {
    use axum::extract::ws::Message;
    let Attach { mut output, input, .. } = attach;
    loop {
        tokio::select! {
            out = output.recv() => {
                let Some(first) = out else { break };
                // Whatever is already queued goes in the same frame: a large
                // result is a handful of messages, not one per PTY read.
                let mut data = Vec::new();
                let mut exit = None;
                let mut next = Some(first);
                while let Some(item) = next.take() {
                    match item {
                        Out::Data(bytes) => data.extend_from_slice(&bytes),
                        Out::Exit(code) => {
                            exit = Some(code);
                            break;
                        }
                    }
                    if data.len() >= MAX_FRAME {
                        break;
                    }
                    next = output.try_recv().ok();
                }
                if !data.is_empty() && socket.send(Message::Binary(data)).await.is_err() {
                    break;
                }
                if let Some(code) = exit {
                    let _ = socket.send(Message::Text(serde_json::json!({ "exit": code }).to_string())).await;
                    break;
                }
            }
            message = socket.recv() => match message {
                Some(Ok(Message::Binary(bytes))) => {
                    let _ = input.send(bytes);
                }
                Some(Ok(Message::Text(text))) => {
                    let Ok(control) = serde_json::from_str::<Control>(&text) else { continue };
                    if let Some((cols, rows)) = control.resize {
                        let size = PtySize { rows: rows.max(2), cols: cols.max(2), pixel_width: 0, pixel_height: 0 };
                        if let Some(session) = sessions.lock().ok().as_ref().and_then(|m| m.get(&id)) {
                            let _ = session.master.resize(size);
                        }
                    }
                    if let Some(trace) = control.trace {
                        log::info!("terminal {id}: {}", trace.chars().take(300).collect::<String>());
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            }
        }
    }
    let _ = socket.send(Message::Close(None)).await;
    // The socket is the session's lifeline: a closed tab, a reloaded page and a
    // closed window all end up here, and the client goes with them.
    if let Some(mut session) = sessions.lock().ok().and_then(|mut map| map.remove(&id)) {
        let _ = session.killer.kill();
    }
}

/// A client started in a pseudo-terminal of its own.
struct Spawned {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
}

async fn spawn_client(
    binary: &Path,
    name: &str,
    launch: &Launch,
    cols: u16,
    rows: u16,
) -> Result<Spawned, String> {
    let size = PtySize { rows: rows.max(2), cols: cols.max(2), pixel_width: 0, pixel_height: 0 };
    let pair = native_pty_system()
        .openpty(size)
        .map_err(|e| format!("Could not open a terminal: {e}"))?;
    let mut cmd = CommandBuilder::new(binary);
    cmd.args(&launch.args);
    for key in launch.unset {
        cmd.env_remove(key);
    }
    for (key, value) in &launch.env {
        cmd.env(key, value);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    // What the client starts itself (psql's pager, `\!`) needs the same lookup.
    cmd.env("PATH", crate::omniroute::user_path().await);
    // A .app launched from Finder gets no locale at all, and psql then prints
    // every non-ASCII character as `?`.
    #[cfg(unix)]
    if ["LC_ALL", "LC_CTYPE", "LANG"].iter().all(|k| std::env::var_os(k).is_none()) {
        cmd.env("LANG", if cfg!(target_os = "macos") { "en_US.UTF-8" } else { "C.UTF-8" });
    }
    if let Some(home) = home_dir() {
        cmd.cwd(home);
    }

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("Could not start {name}: {e}"))?;
    // The client now holds the only slave handle, so its exit is what ends the reader.
    drop(pair.slave);
    let streams = pair
        .master
        .try_clone_reader()
        .and_then(|reader| Ok((reader, pair.master.take_writer()?)));
    match streams {
        Ok((reader, writer)) => Ok(Spawned { master: pair.master, child, reader, writer }),
        Err(e) => {
            let _ = child.kill();
            Err(format!("Could not attach to {name}: {e}"))
        }
    }
}

fn read_loop(mut reader: Box<dyn Read + Send>, out: tokio::sync::mpsc::UnboundedSender<Out>) {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if out.send(Out::Data(buf[..n].to_vec())).is_err() {
                    break;
                }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            // EIO is how Linux reports that the client closed its end.
            Err(_) => break,
        }
    }
}

fn write_loop(mut writer: Box<dyn Write + Send>, keystrokes: mpsc::Receiver<Vec<u8>>) {
    for bytes in keystrokes {
        if writer.write_all(&bytes).and_then(|_| writer.flush()).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn install_hint_is_one_command_on_known_distros() {
        let Some(cmd) = linux_install("mysql") else { return };
        assert!(cmd.starts_with("sudo "), "{cmd}");
        assert_eq!(linux_install("sqlcmd"), None);
    }

    #[test]
    fn versions_sort_numerically() {
        let root = std::env::temp_dir().join(format!("stroke-term-{}", std::process::id()));
        for v in ["9.6", "17", "16"] {
            std::fs::create_dir_all(root.join(v)).unwrap();
        }
        let found: Vec<String> = versioned(&root, "", "bin")
            .iter()
            .map(|p| p.parent().unwrap().file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(found, ["17", "16", "9.6"]);
    }

    #[test]
    fn pg_password_and_ssl_travel_in_env_not_argv() {
        let config = PgConfig {
            name: "t".into(),
            host: "db.example.com".into(),
            port: 5433,
            database: "app".into(),
            user: "me".into(),
            password: "s3cret".into(),
            ssl: true,
            ssl_mode: None,
            ssl_root_cert: Some("/ca.pem".into()),
            ssh: None,
            timezone: Some("SYSTEM".into()),
        };
        let launch = tauri::async_runtime::block_on(pg_launch(config)).unwrap();
        assert!(launch.args.iter().all(|a| !a.contains("s3cret")));
        let env: HashMap<_, _> = launch.env.into_iter().collect();
        assert_eq!(env["PGPASSWORD"], "s3cret");
        assert_eq!(env["PGSSLMODE"], "require");
        assert_eq!(env["PGSSLROOTCERT"], "/ca.pem");
        assert_eq!(env["PGPORT"], "5433");
        assert!(!env.contains_key("PGTZ"));
    }

    #[test]
    fn psql_keeps_ascii_borders_on_windows() {
        let unix = psql_display_args(false);
        assert!(unix.contains(&"--pset=linestyle=unicode".to_string()));
        let windows = psql_display_args(true);
        assert!(windows.iter().all(|a| a.is_ascii() && !a.contains("linestyle")), "{windows:?}");
        assert!(windows.contains(&"--pset=null=(null)".to_string()));
    }

    #[test]
    fn sqlite_gets_box_mode_by_version() {
        let c = SqliteConfig { name: "t".into(), file_path: "/tmp/x.db".into() };
        if home_dir().is_some_and(|h| h.join(".sqliterc").exists()) {
            return;
        }
        assert_eq!(sqlite_launch(&c, Some("SQLite 3.53.4")).args[..2], ["-cmd", ".mode box --wrap 60"]);
        assert_eq!(sqlite_launch(&c, Some("SQLite 3.35.0")).args, ["-box", "/tmp/x.db"]);
        assert_eq!(sqlite_launch(&c, Some("SQLite 3.31.1")).args, ["/tmp/x.db"]);
        assert_eq!(sqlite_launch(&c, None).args, ["/tmp/x.db"]);
    }

    #[test]
    fn mariadb_gets_switches_instead_of_ssl_mode() {
        let config = |ssl_mode: &str| MysqlConfig {
            name: "t".into(),
            host: "h".into(),
            port: 3306,
            database: String::new(),
            user: "u".into(),
            password: "p".into(),
            ssl: false,
            ssl_mode: Some(ssl_mode.into()),
            ssl_root_cert: None,
            ssh: None,
            timezone: None,
        };
        let run = |c, mariadb| tauri::async_runtime::block_on(mysql_launch(c, mariadb)).unwrap().args;
        assert!(run(config("required"), false).contains(&"--ssl-mode=REQUIRED".to_string()));
        let maria = run(config("VERIFY_IDENTITY"), true);
        assert!(maria.contains(&"--ssl-verify-server-cert".to_string()));
        assert!(maria.iter().all(|a| !a.starts_with("--ssl-mode")));
        assert!(run(config("DISABLED"), true).contains(&"--skip-ssl".to_string()));
    }

    /// The whole path a session takes: find the client, start it in a PTY,
    /// type into it, read its output, see it exit. Skipped without sqlite3.
    #[test]
    fn sqlite3_round_trips_through_a_pty() {
        let Some(binary) = tauri::async_runtime::block_on(find_binary(&["sqlite3"])) else {
            return;
        };
        let db = std::env::temp_dir().join(format!("stroke-term-{}.db", std::process::id()));
        let launch = sqlite_launch(&SqliteConfig { name: "t".into(), file_path: db.display().to_string() }, None);
        let Spawned { mut child, mut reader, mut writer, master: _master } =
            tauri::async_runtime::block_on(spawn_client(&binary, "sqlite3", &launch, 80, 24)).unwrap();
        writer.write_all(b"select 6*7;\n.quit\n").unwrap();
        writer.flush().unwrap();
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        while let Ok(n @ 1..) = reader.read(&mut buf) {
            out.extend_from_slice(&buf[..n]);
        }
        let status = child.wait().unwrap();
        let _ = std::fs::remove_file(&db);
        assert!(String::from_utf8_lossy(&out).contains("42"), "{}", String::from_utf8_lossy(&out));
        assert!(status.success());
    }

    /// The page's side of a session: open, connect to the socket, type, read
    /// the echo, run a query, quit, get the exit. Skipped without sqlite3.
    #[test]
    fn sqlite3_round_trips_over_the_socket() {
        use futures::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;
        tauri::async_runtime::block_on(async {
            if find_binary(&["sqlite3"]).await.is_none() {
                return;
            }
            let state = TerminalState::default();
            let db = std::env::temp_dir().join(format!("stroke-term-ws-{}.db", std::process::id()));
            let config = AnyConnectionConfig::Sqlite(SqliteConfig {
                name: "t".into(),
                file_path: db.display().to_string(),
            });
            let session = open_session(&state, config, 80, 24).await.unwrap();

            let wrong = session.url.replace("token=", "token=x");
            assert!(tokio_tungstenite::connect_async(&wrong).await.is_err(), "a wrong token must not attach");

            let (mut ws, _) = tokio_tungstenite::connect_async(&session.url).await.unwrap();
            let mut seen = Vec::<u8>::new();
            // Read frames until `want` shows up in the output.
            async fn until(
                ws: &mut (impl StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin),
                seen: &mut Vec<u8>,
                want: &str,
            ) -> Option<String> {
                let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
                loop {
                    if String::from_utf8_lossy(seen).contains(want) {
                        return None;
                    }
                    match tokio::time::timeout_at(deadline, ws.next()).await.ok()?? .ok()? {
                        Message::Binary(b) => seen.extend_from_slice(&b),
                        Message::Text(t) => return Some(t),
                        _ => {}
                    }
                }
            }
            until(&mut ws, &mut seen, "sqlite>").await;

            let mut latencies = Vec::new();
            for ch in "select 6*7;".chars() {
                seen.clear();
                let start = Instant::now();
                ws.send(Message::Binary(ch.to_string().into_bytes())).await.unwrap();
                until(&mut ws, &mut seen, &ch.to_string()).await;
                latencies.push(start.elapsed());
            }
            ws.send(Message::Binary(b"\r".to_vec())).await.unwrap();
            until(&mut ws, &mut seen, "42").await;
            assert!(String::from_utf8_lossy(&seen).contains("42"));

            ws.send(Message::Text(r#"{"resize":[120,40]}"#.into())).await.unwrap();
            ws.send(Message::Binary(b".quit\r".to_vec())).await.unwrap();
            let exit = until(&mut ws, &mut seen, "\u{0}never").await;
            let _ = std::fs::remove_file(&db);
            assert_eq!(exit.as_deref(), Some(r#"{"exit":0}"#));

            latencies.sort();
            println!(
                "keystroke to echo over the socket: p50 {:?}, max {:?}",
                latencies[latencies.len() / 2],
                latencies.last().unwrap()
            );
            assert!(*latencies.last().unwrap() < Duration::from_millis(100));
        });
    }
}
