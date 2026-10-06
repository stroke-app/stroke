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

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

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
            "--pset=linestyle=unicode".to_string(),
            // NULL apart from the empty string.
            "--pset=null=\u{2205}".to_string(),
        ],
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

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

}
