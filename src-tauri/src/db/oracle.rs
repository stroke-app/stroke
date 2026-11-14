//! Oracle Database, through the `oracle` crate (rust-oracle).
//!
//! The crate compiles ODPI-C from source, so building Stroke needs no Oracle
//! files. At runtime ODPI-C loads the Oracle Client library (Instant Client)
//! the first time an Oracle connection opens; when it is missing the user gets
//! `INSTANT_CLIENT_HELP` instead of a crash, and every other engine is
//! unaffected.
//!
//! The driver is synchronous. Every entry point hops onto a blocking thread and
//! holds the session mutex only inside it, never across an `.await`.

use super::connection::{OracleConfig, OracleHandle};
use super::query::{ColumnInfo, ForeignKeyInfo, RowFilter, SqlResult, TableRows};
use super::schema::{ColumnStructureRow, IncomingForeignKey, IndexInfo, TableColumnStructure, TableInfo};
use oracle::sql_type::{OracleType, Timestamp, ToSql};
use oracle::{Connection, Connector, InitParams, SqlValue};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

/// One Oracle session. `lock` serializes statements; `conn` is reached
/// without it only to interrupt the running call (`OCIBreak` is safe from
/// another thread while the session is busy). The driver's autocommit stays
/// off: `run_commit` commits after each write, so a read-only run and a
/// multi-row delete can hold one transaction open.
pub struct OracleSession {
    conn: Connection,
    lock: Mutex<()>,
    /// What the grid needs to know about a table besides its rows, by
    /// (schema, table). The ALL_* dictionary views behind it are the slow part
    /// of a page fetch (seconds for a user who sees every schema), and none of
    /// it changes when you page, sort or filter.
    meta: Mutex<HashMap<(String, String), (Instant, std::sync::Arc<TableMeta>)>>,
}

struct TableMeta {
    structure: Vec<ColumnStructureRow>,
    is_table: bool,
    foreign_keys: Vec<ForeignKeyInfo>,
}

/// A table changed by another client shows up after at most this long.
const META_TTL: std::time::Duration = std::time::Duration::from_secs(60);

impl OracleSession {
    fn cached_meta(&self, key: &(String, String)) -> Option<std::sync::Arc<TableMeta>> {
        let map = self.meta.lock().ok()?;
        map.get(key).filter(|(at, _)| at.elapsed() < META_TTL).map(|(_, m)| m.clone())
    }

    fn store_meta(&self, key: (String, String), meta: std::sync::Arc<TableMeta>) {
        if let Ok(mut map) = self.meta.lock() {
            map.insert(key, (Instant::now(), meta));
        }
    }

    /// Forget every table's catalog facts, after a statement that may have
    /// changed them.
    fn forget_meta(&self) {
        if let Ok(mut map) = self.meta.lock() {
            map.clear();
        }
    }
}

/// Whether a statement only reads: anything else (DDL, DML, PL/SQL) may
/// change a table's columns, keys or comments.
fn reads_only(stmt: &str) -> bool {
    let head = stmt.trim_start().split(|c: char| !c.is_ascii_alphabetic()).next().unwrap_or("");
    head.eq_ignore_ascii_case("select") || head.eq_ignore_ascii_case("with")
}

// ── Oracle Client library ────────────────────────────────────────────────────

/// Shown when ODPI-C cannot load the Oracle Client library (DPI-1047).
pub const INSTANT_CLIENT_HELP: &str = "Oracle Instant Client is required to connect to Oracle, and it was not found. \
Download the Basic or Basic Light package for your platform from \
https://www.oracle.com/database/technologies/instant-client/downloads.html, unzip it, \
then set \"Instant Client folder\" on this connection (or the STROKE_ORACLE_CLIENT_DIR \
environment variable) to the unzipped folder. On Linux, also install libaio \
(libaio1 / libaio1t64).";

#[cfg(target_os = "windows")]
const CLIENT_LIB: &str = "oci.dll";
#[cfg(target_os = "macos")]
const CLIENT_LIB: &str = "libclntsh.dylib";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const CLIENT_LIB: &str = "libclntsh.so";

/// Folders an unzipped Instant Client usually lands in, newest first.
pub(crate) fn client_dir_candidates(explicit: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let explicit = explicit.trim();
    if !explicit.is_empty() {
        out.push(PathBuf::from(explicit));
    }
    if let Ok(dir) = std::env::var("STROKE_ORACLE_CLIENT_DIR") {
        if !dir.trim().is_empty() {
            out.push(PathBuf::from(dir.trim()));
        }
    }
    if let Ok(home) = std::env::var("ORACLE_HOME") {
        let home = PathBuf::from(home);
        out.push(home.join("lib"));
        out.push(home.join("bin"));
        out.push(home);
    }
    let mut parents: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs_home() {
        parents.push(home.clone());
        parents.push(home.join("Downloads"));
        parents.push(home.join("oracle"));
    }
    if cfg!(target_os = "windows") {
        parents.push(PathBuf::from("C:\\oracle"));
        parents.push(PathBuf::from("C:\\"));
    } else {
        parents.push(PathBuf::from("/opt/oracle"));
        parents.push(PathBuf::from("/usr/local/lib"));
        parents.push(PathBuf::from("/opt/homebrew/lib"));
    }
    for parent in parents {
        let Ok(rd) = std::fs::read_dir(&parent) else { continue };
        let mut found: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.to_ascii_lowercase().starts_with("instantclient"))
            })
            .collect();
        // instantclient_23_26 before instantclient_19_8.
        found.sort_by(|a, b| version_key(b).cmp(&version_key(a)));
        out.extend(found);
    }
    // Linux RPM installs.
    if let Ok(rd) = std::fs::read_dir("/usr/lib/oracle") {
        for e in rd.flatten() {
            out.push(e.path().join("client64").join("lib"));
        }
    }
    out
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(target_os = "windows") { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

fn version_key(p: &Path) -> Vec<u32> {
    p.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse().ok())
        .collect()
}

/// Load the Oracle Client library once per process. A failed load is not
/// cached, so installing Instant Client and connecting again works without a
/// restart.
fn ensure_client(explicit_dir: &str) -> Result<(), String> {
    if InitParams::is_initialized() {
        return Ok(());
    }
    let explicit = explicit_dir.trim();
    let mut first_err: Option<String> = None;
    let mut candidates = client_dir_candidates(explicit).into_iter();
    // A folder set on the connection comes first; then the platform's own
    // search path (ldconfig / LD_LIBRARY_PATH, PATH, DYLD paths); then the
    // usual unzip locations.
    let try_dir = |dir: &Path, first_err: &mut Option<String>| -> bool {
        if !dir.join(CLIENT_LIB).is_file() {
            if !explicit.is_empty() && dir == Path::new(explicit) && first_err.is_none() {
                *first_err = Some(format!(
                    "No {CLIENT_LIB} in the Instant Client folder \"{}\". Point it at the folder you unzipped Instant Client into.",
                    dir.display()
                ));
            }
            return false;
        }
        let mut params = InitParams::new();
        match params.oracle_client_lib_dir(dir.as_os_str().to_owned()).and_then(|p| p.init()) {
            Ok(_) => true,
            Err(e) => {
                log::warn!("oracle: could not load the client from {}: {e}", dir.display());
                if first_err.is_none() {
                    *first_err = Some(load_error(dir, &clean_err(&e.to_string())));
                }
                false
            }
        }
    };
    if !explicit.is_empty() {
        if let Some(dir) = candidates.next() {
            if try_dir(&dir, &mut first_err) {
                return Ok(());
            }
        }
    }
    let default_err = match InitParams::new().init() {
        Ok(_) => return Ok(()),
        Err(e) => clean_err(&e.to_string()),
    };
    for dir in candidates {
        if try_dir(&dir, &mut first_err) {
            return Ok(());
        }
    }
    Err(match first_err {
        Some(found) => found,
        None if default_err.contains("DPI-1047") => format!("{INSTANT_CLIENT_HELP} ({default_err})"),
        None => format!("Oracle client initialization failed: {default_err}"),
    })
}

/// Why Instant Client in `dir` would not load, and what fixes it.
fn load_error(dir: &Path, msg: &str) -> String {
    let hint = if msg.contains("libaio") {
        " Instant Client needs libaio: install it (libaio1, or libaio1t64 on Ubuntu 24.04 and later, where \
         libaio.so.1 also needs a link: sudo ln -s /usr/lib/x86_64-linux-gnu/libaio.so.1t64 /usr/lib/x86_64-linux-gnu/libaio.so.1)."
            .to_string()
    } else if cfg!(target_os = "linux") && msg.contains("cannot open shared object file") {
        // Linux Instant Client libraries carry no $ORIGIN run path, so their
        // own dependencies (libnnz.so) resolve only through the linker's path.
        format!(
            " On Linux the Instant Client folder must also be on the library path: run \
             sudo sh -c 'echo {} > /etc/ld.so.conf.d/oracle-instantclient.conf' && sudo ldconfig, then restart Stroke \
             (or start Stroke with LD_LIBRARY_PATH set to that folder).",
            dir.display()
        )
    } else {
        String::new()
    };
    format!("Could not load Oracle Instant Client from \"{}\": {msg}{hint}", dir.display())
}

// ── Connect ──────────────────────────────────────────────────────────────────

/// The connect string ODPI-C gets: the connection's own Easy Connect / TNS
/// string when set, else one built from host, port and service name or SID.
pub fn connect_string(cfg: &OracleConfig) -> String {
    let custom = cfg.connect_string.trim();
    if !custom.is_empty() {
        return custom.to_string();
    }
    let host = cfg.host.trim();
    let port = if cfg.port == 0 { 1521 } else { cfg.port };
    let sid = cfg.sid.trim();
    if !sid.is_empty() {
        // Easy Connect has no SID form, so a SID goes in a full descriptor.
        return format!(
            "(DESCRIPTION=(ADDRESS=(PROTOCOL=TCP)(HOST={host})(PORT={port}))(CONNECT_DATA=(SID={sid})))"
        );
    }
    let service = cfg.service_name.trim();
    if service.is_empty() {
        format!("//{host}:{port}")
    } else {
        format!("//{host}:{port}/{service}")
    }
}

/// Host and port to probe before the real connect, when the connect string
/// names them. A tnsnames.ora alias names neither, and is not probed.
pub fn probe_target(cfg: &OracleConfig) -> Option<(String, u16)> {
    let cs = connect_string(cfg);
    let upper = cs.to_ascii_uppercase();
    if upper.contains("(HOST=") || upper.contains("(HOST =") {
        let host = descriptor_value(&cs, "HOST")?;
        let port = descriptor_value(&cs, "PORT").and_then(|p| p.parse().ok()).unwrap_or(1521);
        return Some((host, port));
    }
    // Easy Connect: [tcp[s]://]host[:port][/service][:server][/instance][?params]
    let mut rest = cs.as_str();
    for prefix in ["tcps://", "tcp://", "TCPS://", "TCP://"] {
        if let Some(r) = rest.strip_prefix(prefix) {
            rest = r;
        }
    }
    let rest = rest.trim_start_matches("//");
    if rest.contains('(') || rest.contains(' ') {
        return None;
    }
    let addr = rest.split(['/', '?']).next()?;
    if addr.is_empty() {
        return None;
    }
    // [::1]:1521
    if let Some(v6) = addr.strip_prefix('[') {
        let (host, tail) = v6.split_once(']')?;
        let port = tail.strip_prefix(':').and_then(|p| p.parse().ok()).unwrap_or(1521);
        return Some((host.to_string(), port));
    }
    match addr.split_once(':') {
        Some((h, p)) => Some((h.to_string(), p.parse().ok()?)),
        // No port and no slash: a bare word is a tnsnames alias, not a host.
        None if rest.contains('/') => Some((addr.to_string(), 1521)),
        None => None,
    }
}

fn descriptor_value(desc: &str, key: &str) -> Option<String> {
    let upper = desc.to_ascii_uppercase();
    let at = upper.find(&format!("({key}"))?;
    let after = &desc[at + key.len() + 1..];
    let after = after.trim_start().strip_prefix('=')?;
    let end = after.find(')')?;
    Some(after[..end].trim().to_string())
}

/// Session settings every Oracle connection starts with: dates and timestamps
/// read and written as ISO text (what the grid shows is what an edit binds
/// back), and `.` as the decimal separator whatever the client locale.
const SESSION_SETUP: &str = "ALTER SESSION SET \
    NLS_DATE_FORMAT = 'YYYY-MM-DD HH24:MI:SS' \
    NLS_TIMESTAMP_FORMAT = 'YYYY-MM-DD HH24:MI:SS.FF' \
    NLS_TIMESTAMP_TZ_FORMAT = 'YYYY-MM-DD HH24:MI:SS.FF TZH:TZM' \
    NLS_NUMERIC_CHARACTERS = '.,'";

/// Load the Oracle Client library before anything touches the network, so a
/// missing Instant Client is the error the user sees - not a reachability
/// verdict about a server it could never have talked to anyway.
pub async fn prepare_client(explicit_dir: &str) -> Result<(), String> {
    let dir = explicit_dir.to_string();
    tokio::task::spawn_blocking(move || ensure_client(&dir))
        .await
        .map_err(|e| format!("Oracle client task failed: {e}"))?
}

/// Open a session (blocking). Callers bound it with `connect_racing_probe`.
pub async fn connect(cfg: &OracleConfig) -> Result<OracleHandle, String> {
    let cfg = cfg.clone();
    tokio::task::spawn_blocking(move || {
        ensure_client(&cfg.client_lib_dir)?;
        let cs = connect_string(&cfg);
        let conn = Connector::new(cfg.user.trim(), cfg.password.as_str(), cs.as_str())
            .driver_name("Stroke")
            .connect()
            .map_err(|e| explain_connect_error(&e.to_string()))?;
        conn.execute(SESSION_SETUP, &[]).map_err(|e| format!("Oracle session setup failed: {}", clean_err(&e.to_string())))?;
        Ok(std::sync::Arc::new(OracleSession { conn, lock: Mutex::new(()), meta: Mutex::new(HashMap::new()) }))
    })
    .await
    .map_err(|e| format!("Oracle connect task failed: {e}"))?
}

fn explain_connect_error(err: &str) -> String {
    let e = clean_err(err);
    if e.contains("DPI-1047") {
        return format!("{INSTANT_CLIENT_HELP} ({e})");
    }
    if e.contains("ORA-12514") {
        return format!("Oracle connection failed: the listener does not know that service name. Check the service name (e.g. FREEPDB1, XEPDB1, ORCLPDB1), or connect by SID. ({e})");
    }
    if e.contains("ORA-12505") {
        return format!("Oracle connection failed: the listener does not know that SID. Check the SID, or connect by service name. ({e})");
    }
    if e.contains("ORA-01017") {
        return format!("Oracle connection failed: invalid username or password. ({e})");
    }
    if e.contains("ORA-12541") {
        return format!("Oracle connection failed: no listener at that host and port. ({e})");
    }
    format!("Oracle connection failed: {e}")
}

/// The driver's error text without its "OCI Error: " / "DPI Error: " prefix.
fn clean_err(e: &str) -> String {
    e.trim_start_matches("OCI Error: ").trim_start_matches("DPI Error: ").trim().to_string()
}

fn ora_err(e: oracle::Error) -> String {
    let msg = clean_err(&e.to_string());
    if msg.contains("ORA-01013") {
        return super::query::QUERY_CANCELLED.to_string();
    }
    format!("Oracle error: {msg}")
}

/// Run a closure against the locked session on a blocking thread.
async fn run_blocking<T, F>(handle: &OracleHandle, f: F) -> Result<T, String>
where
    F: FnOnce(&Connection) -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    let h = handle.clone();
    tokio::task::spawn_blocking(move || {
        let _guard = h.lock.lock().map_err(|_| "Oracle session lock poisoned".to_string())?;
        f(&h.conn)
    })
    .await
    .map_err(|e| format!("Oracle task failed: {e}"))?
}

/// The container this session is in: a pluggable database (FREEPDB1) or the
/// root (CDB$ROOT). On 11g, which has no containers, the database name.
pub async fn container_name(handle: &OracleHandle) -> Result<String, String> {
    let r = match query_binds(handle, "SELECT SYS_CONTEXT('USERENV', 'CON_NAME') FROM DUAL".into(), vec![]).await {
        Ok(r) => r,
        Err(_) => query_binds(handle, "SELECT SYS_CONTEXT('USERENV', 'DB_NAME') FROM DUAL".into(), vec![]).await?,
    };
    Ok(r.rows.first().and_then(|row| text(row, 0)).unwrap_or_default())
}

pub async fn ping(handle: &OracleHandle) -> Result<(), String> {
    run_blocking(handle, |conn| {
        conn.query_row("SELECT 1 FROM DUAL", &[]).map(|_| ()).map_err(|e| format!("Oracle ping failed: {}", clean_err(&e.to_string())))
    })
    .await
}

/// Interrupt whatever the session is running (Stop in the SQL editor). The
/// interrupted call fails with ORA-01013, which `ora_err` reports as a cancel.
pub async fn cancel(handle: &OracleHandle) {
    let h = handle.clone();
    let _ = tokio::task::spawn_blocking(move || h.conn.break_execution()).await;
}

// ── Identifiers and literals ─────────────────────────────────────────────────

/// `"name"`: Oracle folds unquoted names to upper case, so catalog names are
/// always quoted exactly as stored.
pub fn quote_ident(ident: &str) -> String {
    super::sql_util::quote_double(ident)
}

fn table_ref(schema: &str, table: &str) -> String {
    if schema.is_empty() {
        quote_ident(table)
    } else {
        format!("{}.{}", quote_ident(schema), quote_ident(table))
    }
}

// ── Statement splitting ──────────────────────────────────────────────────────

/// Whether a statement is PL/SQL (or a stored unit's DDL), which keeps its
/// `;` and runs to a `/` line instead of ending at the first `;`.
fn is_plsql_head(words: &[String]) -> bool {
    let w: Vec<&str> = words.iter().map(|s| s.as_str()).collect();
    match w.first().copied() {
        Some("BEGIN") | Some("DECLARE") => true,
        Some("CREATE") => {
            let mut i = 1;
            if w.get(i) == Some(&"OR") && w.get(i + 1) == Some(&"REPLACE") {
                i += 2;
            }
            if matches!(w.get(i).copied(), Some("EDITIONABLE") | Some("NONEDITIONABLE") | Some("EDITIONING")) {
                i += 1;
            }
            matches!(w.get(i).copied(), Some("FUNCTION") | Some("PROCEDURE") | Some("PACKAGE") | Some("TRIGGER") | Some("TYPE") | Some("LIBRARY"))
        }
        _ => false,
    }
}

/// The first few upper-cased words at `from`, skipping whitespace and comments.
fn leading_words(chars: &[char], from: usize, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = from;
    while i < chars.len() && out.len() < max {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '-' && chars.get(i + 1) == Some(&'-') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else if c.is_alphanumeric() || c == '_' || c == '$' || c == '#' {
            let s = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$' || chars[i] == '#') {
                i += 1;
            }
            out.push(chars[s..i].iter().collect::<String>().to_ascii_uppercase());
        } else {
            break;
        }
    }
    out
}

/// Whether the line holding `i` is a lone `/` (SQL*Plus's "run the buffer").
fn slash_line_at(chars: &[char], i: usize) -> Option<usize> {
    if chars[i] != '/' {
        return None;
    }
    let mut s = i;
    while s > 0 && chars[s - 1] != '\n' {
        s -= 1;
        if !chars[s].is_whitespace() {
            return None;
        }
    }
    let mut e = i + 1;
    while e < chars.len() && chars[e] != '\n' {
        if !chars[e].is_whitespace() {
            return None;
        }
        e += 1;
    }
    Some(e)
}

/// Split a script the way SQL*Plus / SQL Developer do: SQL ends at `;` or a
/// `/` line (the `;` is dropped - OCI rejects it), PL/SQL blocks and stored
/// unit DDL run to a `/` line or the end and keep their final `;`. Strings
/// (including `q'[...]'` quoting), quoted names and comments never split.
pub fn split_statements(sql: &str) -> Vec<String> {
    let chars: Vec<char> = sql.chars().collect();
    let n = chars.len();
    let mut out: Vec<String> = Vec::new();
    let mut start = 0usize;
    let mut plsql: Option<bool> = None;
    let mut i = 0usize;

    let flush = |out: &mut Vec<String>, from: usize, to: usize, plsql: bool| {
        let text: String = chars[from..to].iter().collect();
        let mut t = text.trim().to_string();
        if !plsql {
            while t.ends_with(';') {
                t.pop();
                t = t.trim_end().to_string();
            }
        }
        let meaningful = !leading_words(&t.chars().collect::<Vec<_>>(), 0, 1).is_empty()
            || t.trim_start().starts_with('(');
        if !t.is_empty() && meaningful {
            out.push(t);
        }
    };

    while i < n {
        let c = chars[i];
        if plsql.is_none() && !c.is_whitespace() {
            let words = leading_words(&chars, i, 6);
            if !words.is_empty() {
                plsql = Some(is_plsql_head(&words));
            }
        }
        match c {
            '-' if chars.get(i + 1) == Some(&'-') => {
                while i < n && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i + 1 < n && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i = (i + 2).min(n);
            }
            '/' => {
                if let Some(end) = slash_line_at(&chars, i) {
                    flush(&mut out, start, i, plsql.unwrap_or(false));
                    start = end;
                    i = end;
                    plsql = None;
                } else {
                    i += 1;
                }
            }
            'q' | 'Q' if chars.get(i + 1) == Some(&'\'') && (i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_')) => {
                // q'X...X' with ([{< closed by their pair.
                let open = chars.get(i + 2).copied().unwrap_or('\'');
                let close = match open {
                    '(' => ')',
                    '[' => ']',
                    '{' => '}',
                    '<' => '>',
                    other => other,
                };
                i += 3;
                while i < n && !(chars[i] == close && chars.get(i + 1) == Some(&'\'')) {
                    i += 1;
                }
                i = (i + 2).min(n);
            }
            '\'' | '"' => {
                i += 1;
                while i < n {
                    if chars[i] == c {
                        if chars.get(i + 1) == Some(&c) {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                i = (i + 1).min(n);
            }
            ';' if plsql != Some(true) => {
                flush(&mut out, start, i, false);
                i += 1;
                start = i;
                plsql = None;
            }
            _ => i += 1,
        }
    }
    flush(&mut out, start, n, plsql.unwrap_or(false));
    out
}

/// The statement with its string literals (`'...'`, `q'[...]'`) and comments
/// blanked out, for the access classifier: words inside a literal are data.
pub fn strip_literals(stmt: &str) -> String {
    let chars: Vec<char> = stmt.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(stmt.len());
    let mut i = 0;
    while i < n {
        let c = chars[i];
        if c == '-' && chars.get(i + 1) == Some(&'-') {
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            out.push(' ');
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < n && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(n);
            out.push(' ');
        } else if (c == 'q' || c == 'Q')
            && chars.get(i + 1) == Some(&'\'')
            && (i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_'))
        {
            let open = chars.get(i + 2).copied().unwrap_or('\'');
            let close = match open {
                '(' => ')',
                '[' => ']',
                '{' => '}',
                '<' => '>',
                other => other,
            };
            i += 3;
            while i < n && !(chars[i] == close && chars.get(i + 1) == Some(&'\'')) {
                i += 1;
            }
            i = (i + 2).min(n);
            out.push_str(" '' ");
        } else if c == '\'' {
            i += 1;
            while i < n {
                if chars[i] == '\'' {
                    if chars.get(i + 1) == Some(&'\'') {
                        i += 2;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            i = (i + 1).min(n);
            out.push_str(" '' ");
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

// ── Cell conversion ──────────────────────────────────────────────────────────

/// Grid label for a result column's type. Oracle's DATE holds a time of day
/// too, so the grid edits it as a date-time - labelled `DATE` it would get a
/// date-only editor that refuses (or drops) the time.
fn type_label(t: &OracleType) -> String {
    match t {
        OracleType::Date => "DATETIME".to_string(),
        other => other.to_string(),
    }
}

/// NUMBER arrives as exact decimal text. Integers within JSON's safe range and
/// decimals that survive the round trip become JSON numbers; anything wider
/// stays text so no digit is lost.
fn number_json(text: &str) -> Value {
    let t = text.trim();
    if let Ok(i) = t.parse::<i64>() {
        if i.unsigned_abs() <= (1u64 << 53) {
            return Value::from(i);
        }
        return Value::String(t.to_string());
    }
    if let Ok(f) = t.parse::<f64>() {
        if f.is_finite() {
            let back = f.to_string();
            let norm = |s: &str| s.trim_start_matches('+').replace("-.", "-0.").to_string();
            let mut canon = norm(t);
            if canon.starts_with('.') {
                canon.insert(0, '0');
            }
            if back == canon {
                if let Some(n) = serde_json::Number::from_f64(f) {
                    return Value::Number(n);
                }
            }
        }
    }
    Value::String(t.to_string())
}

fn fmt_timestamp(ts: &Timestamp, date_only_precision: bool, with_tz: bool) -> String {
    let mut s = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        ts.year(),
        ts.month(),
        ts.day(),
        ts.hour(),
        ts.minute(),
        ts.second()
    );
    if !date_only_precision && ts.nanosecond() > 0 {
        let frac = format!("{:09}", ts.nanosecond());
        s.push('.');
        s.push_str(frac.trim_end_matches('0'));
    }
    if with_tz {
        let h = ts.tz_hour_offset();
        let m = ts.tz_minute_offset();
        let sign = if h < 0 || m < 0 { '-' } else { '+' };
        s.push_str(&format!(" {sign}{:02}:{:02}", h.abs(), m.abs()));
    }
    s
}

fn capped_text(s: String, ty: &str) -> Value {
    if s.len() > super::sql_util::CELL_VALUE_CAP {
        super::sql_util::oversize_cell(ty, s.len(), s.as_bytes())
    } else {
        Value::String(s)
    }
}

fn hex_upper(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02X}"));
    }
    s
}

fn cell_to_json(v: &SqlValue, ty: &OracleType) -> Value {
    if v.is_null().unwrap_or(true) {
        return Value::Null;
    }
    match ty {
        OracleType::Number(_, _) | OracleType::Float(_) => v.get::<String>().map(|s| number_json(&s)).unwrap_or(Value::Null),
        OracleType::Int64 => v.get::<i64>().map(Value::from).unwrap_or(Value::Null),
        OracleType::UInt64 => v.get::<u64>().map(Value::from).unwrap_or(Value::Null),
        OracleType::BinaryFloat | OracleType::BinaryDouble => v
            .get::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        OracleType::Boolean => v.get::<bool>().map(Value::Bool).unwrap_or(Value::Null),
        OracleType::Date => v.get::<Timestamp>().map(|t| Value::String(fmt_timestamp(&t, true, false))).unwrap_or(Value::Null),
        OracleType::Timestamp(_) | OracleType::TimestampLTZ(_) => {
            v.get::<Timestamp>().map(|t| Value::String(fmt_timestamp(&t, false, false))).unwrap_or(Value::Null)
        }
        OracleType::TimestampTZ(_) => v.get::<Timestamp>().map(|t| Value::String(fmt_timestamp(&t, false, true))).unwrap_or(Value::Null),
        OracleType::Raw(_) | OracleType::LongRaw => match v.get::<Vec<u8>>() {
            Ok(b) if b.len() * 2 > super::sql_util::CELL_VALUE_CAP => Value::String(format!("<binary: {} bytes>", b.len())),
            Ok(b) => Value::String(hex_upper(&b)),
            Err(_) => Value::Null,
        },
        // Without lob_locator() the driver fetches a LOB inline, as bytes.
        OracleType::BLOB => match v.get::<Vec<u8>>() {
            Ok(b) => Value::String(format!("<blob: {} bytes>", b.len())),
            Err(_) => Value::String("<blob>".into()),
        },
        OracleType::BFILE => Value::String("<bfile>".into()),
        OracleType::RefCursor => Value::String("<cursor>".into()),
        OracleType::Object(o) => Value::String(format!("<{}>", o.name())),
        OracleType::CLOB | OracleType::NCLOB | OracleType::Long => {
            v.get::<String>().map(|s| capped_text(s, "clob")).unwrap_or(Value::Null)
        }
        _ => v.get::<String>().map(|s| capped_text(s, "varchar2")).unwrap_or(Value::Null),
    }
}

// ── Binds ────────────────────────────────────────────────────────────────────

/// A value bound to a `:n` placeholder.
#[derive(Debug, Clone, PartialEq)]
pub enum Bind {
    Null,
    Int(i64),
    Text(String),
}

impl Bind {
    /// From a grid value. Non-integer numbers bind as their decimal text so
    /// NUMBER gets exactly the digits shown (a binary double would not).
    pub fn from_json(v: &Value) -> Bind {
        match v {
            Value::Null => Bind::Null,
            Value::Bool(b) => Bind::Int(i64::from(*b)),
            Value::Number(n) => match n.as_i64() {
                Some(i) => Bind::Int(i),
                None => Bind::Text(n.to_string()),
            },
            Value::String(s) => Bind::Text(s.clone()),
            other => Bind::Text(other.to_string()),
        }
    }

    /// `from_json` for a column of a known catalog type: date-time text is
    /// put in the shape the session's NLS formats read back exactly.
    pub fn for_column(v: &Value, data_type: Option<&str>) -> Bind {
        match (Bind::from_json(v), data_type) {
            (Bind::Text(s), Some(t)) => Bind::Text(normalize_temporal(t, &s)),
            (b, _) => b,
        }
    }
}

/// Date-time text as the session's NLS formats read it: a `T` separator
/// becomes a space, and for zoned types `Z`/`UTC` becomes `+00:00` and a
/// value without fractional seconds gets `.0`. Without that last step
/// `... 04:05:06 -07:00` is read as `+07:00` - Oracle's lenient matching of the
/// missing `.FF` swallows the sign.
pub fn normalize_temporal(data_type: &str, s: &str) -> String {
    let t = data_type.to_ascii_uppercase();
    let trimmed = s.trim();
    if !(t.starts_with("DATE") || t.starts_with("TIMESTAMP")) || !trimmed.is_ascii() {
        return s.to_string();
    }
    let mut v = trimmed.to_string();
    let b = v.as_bytes();
    let date_like = b.len() >= 10 && b[4] == b'-' && b[7] == b'-' && b[..4].iter().all(u8::is_ascii_digit);
    if !date_like {
        return s.to_string();
    }
    if b.len() > 10 && b[10] == b'T' {
        v.replace_range(10..11, " ");
    }
    if t.contains("TIME ZONE") {
        for suffix in ["Z", " UTC", " GMT", "UTC", "GMT"] {
            if let Some(rest) = v.strip_suffix(suffix) {
                v = format!("{} +00:00", rest.trim_end());
                break;
            }
        }
        let b = v.as_bytes();
        let time_like = b.len() >= 19 && b[13] == b':' && b[16] == b':';
        if time_like {
            let after = v[19..].trim_start();
            if after.starts_with('+') || after.starts_with('-') {
                v = format!("{}.0 {}", &v[..19], after);
            }
        }
    }
    v
}

fn bind_refs(binds: &[Bind]) -> Vec<Box<dyn ToSql>> {
    binds
        .iter()
        .map(|b| -> Box<dyn ToSql> {
            match b {
                Bind::Null => Box::new(Option::<String>::None),
                Bind::Int(i) => Box::new(*i),
                Bind::Text(s) => Box::new(s.clone()),
            }
        })
        .collect()
}

// ── Execute ──────────────────────────────────────────────────────────────────

/// Run one statement (no terminator, unless PL/SQL) on a locked session.
fn run_one(conn: &Connection, sql: &str, binds: &[Bind]) -> Result<SqlResult, String> {
    let t0 = Instant::now();
    let boxed = bind_refs(binds);
    let params: Vec<&dyn ToSql> = boxed.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.statement(sql).fetch_array_size(500).build().map_err(ora_err)?;
    if stmt.is_query() {
        let rs = stmt.query(&params).map_err(ora_err)?;
        let infos = rs.column_info().to_vec();
        let columns: Vec<ColumnInfo> = infos
            .iter()
            .map(|c| {
                let mut ci = ColumnInfo::new(c.name().to_string(), type_label(c.oracle_type()));
                ci.nullable = c.nullable();
                ci
            })
            .collect();
        let mut rows: Vec<Vec<Value>> = Vec::new();
        for row in rs {
            let row = row.map_err(ora_err)?;
            rows.push(row.sql_values().iter().zip(infos.iter()).map(|(v, c)| cell_to_json(v, c.oracle_type())).collect());
        }
        let row_count = Some(rows.len() as i64);
        return Ok(SqlResult { columns, rows, row_count, message: None, query_ms: t0.elapsed().as_millis() as u64, sql: sql.to_string() });
    }
    stmt.execute(&params).map_err(ora_err)?;
    let (row_count, message) = if stmt.is_dml() {
        let n = stmt.row_count().unwrap_or(0) as i64;
        (Some(n), Some(format!("{n} row(s) affected")))
    } else if stmt.is_plsql() {
        (None, Some("PL/SQL procedure successfully completed".to_string()))
    } else {
        (None, Some("Statement executed".to_string()))
    };
    Ok(SqlResult { columns: vec![], rows: vec![], row_count, message, query_ms: t0.elapsed().as_millis() as u64, sql: sql.to_string() })
}

/// `run_one`, committed when it wrote (a query has nothing to commit).
fn run_commit(conn: &Connection, sql: &str, binds: &[Bind]) -> Result<SqlResult, String> {
    let r = run_one(conn, sql, binds);
    match &r {
        Ok(res) if res.message.is_some() => conn.commit().map_err(ora_err)?,
        Ok(_) => {}
        // Oracle undoes a failed statement itself; nothing else is open.
        Err(_) => {
            let _ = conn.rollback();
        }
    }
    r
}

/// Run a script: each statement in turn, the last one's result returned.
pub async fn execute_sql(handle: &OracleHandle, sql: &str) -> Result<SqlResult, String> {
    let stmts = split_statements(sql);
    if stmts.is_empty() {
        return Err("Query is empty".into());
    }
    let original = sql.trim().to_string();
    let writes = !stmts.iter().all(|s| reads_only(s));
    let out = run_blocking(handle, move |conn| {
        let mut last = None;
        for s in &stmts {
            last = Some(run_commit(conn, s, &[])?);
        }
        let mut out = last.expect("non-empty");
        out.sql = original;
        Ok(out)
    })
    .await;
    // Even a failed script may have run its first statements.
    if writes {
        handle.forget_meta();
    }
    out
}

/// `execute_sql` for read-only callers (the MCP server at "Read only"): the
/// statements run inside `SET TRANSACTION READ ONLY`, so any DML fails with
/// ORA-01456, and the transaction is rolled back after. Oracle DDL commits on
/// its own and is not covered by this - the access classifier refuses DDL at
/// that level before it gets here.
pub async fn execute_sql_readonly(handle: &OracleHandle, sql: &str) -> Result<SqlResult, String> {
    let stmts = split_statements(sql);
    if stmts.is_empty() {
        return Err("Query is empty".into());
    }
    let original = sql.trim().to_string();
    run_blocking(handle, move |conn| {
        let run = |conn: &Connection| -> Result<Option<SqlResult>, String> {
            conn.rollback().map_err(ora_err)?;
            conn.execute("SET TRANSACTION READ ONLY", &[]).map_err(ora_err)?;
            let mut last = None;
            for s in &stmts {
                last = Some(run_one(conn, s, &[])?);
            }
            Ok(last)
        };
        let out = run(conn);
        let _ = conn.rollback();
        let mut r = out?.expect("non-empty");
        r.sql = original;
        Ok(r)
    })
    .await
}

/// One bound query, for introspection.
async fn query_binds(handle: &OracleHandle, sql: String, binds: Vec<Bind>) -> Result<SqlResult, String> {
    run_blocking(handle, move |conn| run_one(conn, &sql, &binds)).await
}

fn text(row: &[Value], i: usize) -> Option<String> {
    match row.get(i)? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn int(row: &[Value], i: usize) -> Option<i64> {
    match row.get(i)? {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

// ── Explain ──────────────────────────────────────────────────────────────────

/// `EXPLAIN PLAN` + `DBMS_XPLAN.DISPLAY`, as text lines. Does not run the query.
pub async fn explain_lines(handle: &OracleHandle, sql: &str) -> Result<Vec<String>, String> {
    let stmts = split_statements(sql);
    let target = stmts.into_iter().next().ok_or("Query is empty")?;
    let target = match target.get(..8) {
        Some(p) if p.eq_ignore_ascii_case("EXPLAIN ") => target[8..].trim_start().to_string(),
        _ => target,
    };
    let id = format!("STROKE{}", std::process::id());
    run_blocking(handle, move |conn| {
        conn.execute(&format!("EXPLAIN PLAN SET STATEMENT_ID = '{id}' FOR {target}"), &[])
            .map_err(|e| format!("EXPLAIN failed: {}", clean_err(&e.to_string())))?;
        let r = run_one(conn, "SELECT PLAN_TABLE_OUTPUT FROM TABLE(DBMS_XPLAN.DISPLAY(NULL, :1, 'TYPICAL'))", &[Bind::Text(id.clone())])?;
        let _ = conn.execute("DELETE FROM PLAN_TABLE WHERE STATEMENT_ID = :1", &[&id]);
        let _ = conn.commit();
        Ok(r.rows.iter().filter_map(|row| text(row, 0)).collect())
    })
    .await
}

// ── Schema introspection ─────────────────────────────────────────────────────

/// Schemas (users) that own a table or view, plus the connected user, which
/// comes first. Oracle-maintained users (SYS, XDB, MDSYS, ...) sort last; the
/// UI keeps them behind its system-schema toggle.
pub async fn list_schemas(handle: &OracleHandle) -> Result<Vec<String>, String> {
    // ORACLE_MAINTAINED is 12c+; 11g falls back to the plain list.
    let modern = "SELECT u.USERNAME FROM ALL_USERS u \
        WHERE u.USERNAME = USER OR u.USERNAME IN (SELECT OWNER FROM ALL_TABLES UNION SELECT OWNER FROM ALL_VIEWS) \
        ORDER BY CASE WHEN u.USERNAME = USER THEN 0 WHEN u.ORACLE_MAINTAINED = 'Y' THEN 2 ELSE 1 END, u.USERNAME";
    let legacy = "SELECT u.USERNAME FROM ALL_USERS u \
        WHERE u.USERNAME = USER OR u.USERNAME IN (SELECT OWNER FROM ALL_TABLES UNION SELECT OWNER FROM ALL_VIEWS) \
        ORDER BY CASE WHEN u.USERNAME = USER THEN 0 ELSE 1 END, u.USERNAME";
    let r = match query_binds(handle, modern.into(), vec![]).await {
        Ok(r) => r,
        Err(_) => query_binds(handle, legacy.into(), vec![]).await?,
    };
    Ok(r.rows.iter().filter_map(|row| text(row, 0)).collect())
}

/// Below this many rows (by the optimizer statistics) a table gets an exact
/// count instead: statistics are often stale or missing on small tables. Same
/// threshold as the Postgres and MySQL paths.
const ESTIMATE_THRESHOLD: i64 = 100_000;

pub async fn list_tables(handle: &OracleHandle, schema: &str) -> Result<Vec<TableInfo>, String> {
    // ORACLE_MAINTAINED (12c+) marks Oracle's own objects - the AQ$_, LOGMNR,
    // MVIEW$ tables that fill SYSTEM - so "Hide system tables" can hide them.
    // 11g has no such column and gets the list without it.
    let modern = TABLES_SQL.replace("{maintained}", "o.ORACLE_MAINTAINED");
    let legacy = TABLES_SQL.replace("{maintained}", "NULL");
    let binds = vec![Bind::Text(schema.to_string())];
    let r = match query_binds(handle, modern, binds.clone()).await {
        Ok(r) => r,
        Err(_) => query_binds(handle, legacy, binds).await?,
    };
    Ok(r.rows
        .iter()
        .filter_map(|row| {
            let name = text(row, 0)?;
            let is_view = text(row, 1).as_deref() == Some("VIEW");
            // NUM_ROWS is the optimizer's estimate from the last statistics
            // run. Trusted only for big tables; the rest (-1) get an exact
            // COUNT(*) from `table_row_counts`. Views report 0, as on MySQL
            // and SQL Server.
            let row_count = if is_view {
                0
            } else {
                match int(row, 2) {
                    Some(n) if n >= ESTIMATE_THRESHOLD => n,
                    _ => -1,
                }
            };
            Some(TableInfo {
                name,
                kind: if is_view { "view" } else { "table" }.to_string(),
                row_count,
                rls_enabled: None,
                created_at: text(row, 3),
                create_order: int(row, 4),
                system: text(row, 5).map(|m| m == "Y"),
            })
        })
        .collect())
}

const TABLES_SQL: &str = "SELECT o.OBJECT_NAME, o.OBJECT_TYPE, t.NUM_ROWS, \
            TO_CHAR(o.CREATED, 'YYYY-MM-DD HH24:MI:SS'), o.OBJECT_ID, {maintained} \
        FROM ALL_OBJECTS o \
        LEFT JOIN ALL_TABLES t ON t.OWNER = o.OWNER AND t.TABLE_NAME = o.OBJECT_NAME \
        WHERE o.OWNER = NVL(:1, SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA')) AND o.OBJECT_TYPE IN ('TABLE', 'VIEW') \
          AND o.SECONDARY = 'N' AND o.OBJECT_NAME NOT LIKE 'BIN$%' \
          AND (t.NESTED IS NULL OR t.NESTED = 'NO') \
          AND (t.IOT_TYPE IS NULL OR t.IOT_TYPE = 'IOT') \
        ORDER BY o.OBJECT_NAME";

/// Exact `COUNT(*)` of one table.
pub async fn exact_row_count(handle: &OracleHandle, schema: &str, table: &str) -> Result<i64, String> {
    let r = query_binds(handle, format!("SELECT COUNT(*) FROM {}", table_ref(schema, table)), vec![]).await?;
    Ok(r.rows.first().and_then(|row| int(row, 0)).unwrap_or(0))
}

pub async fn list_indexes(handle: &OracleHandle, schema: &str) -> Result<Vec<IndexInfo>, String> {
    let sql = "SELECT i.INDEX_NAME, i.TABLE_NAME, i.UNIQUENESS, i.INDEX_TYPE, \
            CASE WHEN EXISTS (SELECT 1 FROM ALL_CONSTRAINTS c WHERE c.OWNER = i.TABLE_OWNER \
                AND c.TABLE_NAME = i.TABLE_NAME AND c.CONSTRAINT_TYPE = 'P' AND c.INDEX_NAME = i.INDEX_NAME) \
                THEN 1 ELSE 0 END, \
            (SELECT LISTAGG(ic.COLUMN_NAME, ',') WITHIN GROUP (ORDER BY ic.COLUMN_POSITION) \
               FROM ALL_IND_COLUMNS ic WHERE ic.INDEX_OWNER = i.OWNER AND ic.INDEX_NAME = i.INDEX_NAME) \
        FROM ALL_INDEXES i \
        WHERE i.TABLE_OWNER = NVL(:1, SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA')) AND i.INDEX_TYPE <> 'LOB' AND i.TABLE_NAME NOT LIKE 'BIN$%' \
        ORDER BY i.TABLE_NAME, i.INDEX_NAME";
    let r = query_binds(handle, sql.into(), vec![Bind::Text(schema.to_string())]).await?;
    Ok(r.rows
        .iter()
        .filter_map(|row| {
            Some(IndexInfo {
                name: text(row, 0)?,
                table_name: text(row, 1)?,
                columns: text(row, 5).unwrap_or_default(),
                index_type: text(row, 3).unwrap_or_default().to_lowercase(),
                is_unique: text(row, 2).as_deref() == Some("UNIQUE"),
                is_primary: int(row, 4) == Some(1),
                condition: None,
                comment: None,
            })
        })
        .collect())
}

/// Every column of a schema (or one table/view of it): the declared type with
/// its length, precision and scale, nullability, default, primary key
/// membership, the foreign key it belongs to and its comment.
///
/// The key and foreign-key facts come from two derived tables filtered to the
/// same owner (and table), joined once - a correlated subquery per column
/// against the ALL_* dictionary views took most of a second for one table.
/// Each filter gets its own bind, since a positional bind is per occurrence.
fn columns_query(schema: &str, table: Option<&str>) -> (String, Vec<Bind>) {
    const OWNER: &str = "NVL(:{n}, SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA'))";
    let mut binds: Vec<Bind> = Vec::new();
    let filter = |alias: &str, binds: &mut Vec<Bind>| -> String {
        binds.push(Bind::Text(schema.to_string()));
        let mut f = format!("{alias}.OWNER = {}", OWNER.replace("{n}", &binds.len().to_string()));
        if let Some(t) = table {
            binds.push(Bind::Text(t.to_string()));
            f.push_str(&format!(" AND {alias}.TABLE_NAME = :{}", binds.len()));
        }
        f
    };
    let main = filter("c", &mut binds);
    let pk = filter("p", &mut binds);
    let fk = filter("f", &mut binds);
    let sql = format!(
        "SELECT c.TABLE_NAME, c.COLUMN_ID, c.COLUMN_NAME, c.DATA_TYPE, c.DATA_LENGTH, c.CHAR_LENGTH, \
            c.CHAR_USED, c.DATA_PRECISION, c.DATA_SCALE, c.NULLABLE, c.DATA_DEFAULT, \
            NVL2(pk.COLUMN_NAME, 1, 0), fk.REF, cm.COMMENTS \
        FROM ALL_TAB_COLUMNS c \
        LEFT JOIN ( \
            SELECT DISTINCT pc.TABLE_NAME, pc.COLUMN_NAME FROM ALL_CONSTRAINTS p \
            JOIN ALL_CONS_COLUMNS pc ON pc.OWNER = p.OWNER AND pc.CONSTRAINT_NAME = p.CONSTRAINT_NAME \
            WHERE p.CONSTRAINT_TYPE = 'P' AND {pk} \
        ) pk ON pk.TABLE_NAME = c.TABLE_NAME AND pk.COLUMN_NAME = c.COLUMN_NAME \
        LEFT JOIN ( \
            SELECT fc.TABLE_NAME, fc.COLUMN_NAME, \
                MIN(r.OWNER || '.' || rc.TABLE_NAME || '.' || rc.COLUMN_NAME || CHR(1) || f.CONSTRAINT_NAME) AS REF \
            FROM ALL_CONSTRAINTS f \
            JOIN ALL_CONS_COLUMNS fc ON fc.OWNER = f.OWNER AND fc.CONSTRAINT_NAME = f.CONSTRAINT_NAME \
            JOIN ALL_CONSTRAINTS r ON r.OWNER = f.R_OWNER AND r.CONSTRAINT_NAME = f.R_CONSTRAINT_NAME \
            JOIN ALL_CONS_COLUMNS rc ON rc.OWNER = r.OWNER AND rc.CONSTRAINT_NAME = r.CONSTRAINT_NAME AND rc.POSITION = fc.POSITION \
            WHERE f.CONSTRAINT_TYPE = 'R' AND {fk} \
            GROUP BY fc.TABLE_NAME, fc.COLUMN_NAME \
        ) fk ON fk.TABLE_NAME = c.TABLE_NAME AND fk.COLUMN_NAME = c.COLUMN_NAME \
        LEFT JOIN ALL_COL_COMMENTS cm ON cm.OWNER = c.OWNER AND cm.TABLE_NAME = c.TABLE_NAME AND cm.COLUMN_NAME = c.COLUMN_NAME \
        WHERE {main} AND c.TABLE_NAME NOT LIKE 'BIN$%' \
        ORDER BY c.TABLE_NAME, c.COLUMN_ID"
    );
    (sql, binds)
}

/// `VARCHAR2(50 CHAR)`, `NUMBER(10,2)`, `RAW(16)` from the catalog's parts.
pub fn format_column_type(data_type: &str, length: Option<i64>, char_length: Option<i64>, char_used: Option<&str>, precision: Option<i64>, scale: Option<i64>) -> String {
    match data_type {
        "VARCHAR2" | "CHAR" => {
            if char_used == Some("C") {
                format!("{data_type}({} CHAR)", char_length.unwrap_or(0))
            } else {
                format!("{data_type}({})", length.unwrap_or(0))
            }
        }
        "NVARCHAR2" | "NCHAR" => format!("{data_type}({})", char_length.unwrap_or(0)),
        "RAW" | "UROWID" => format!("{data_type}({})", length.unwrap_or(0)),
        "NUMBER" => match (precision, scale) {
            (Some(p), Some(0)) => format!("NUMBER({p})"),
            (Some(p), Some(s)) => format!("NUMBER({p},{s})"),
            (None, Some(0)) => "INTEGER".to_string(),
            _ => "NUMBER".to_string(),
        },
        "FLOAT" => match precision {
            Some(p) if p != 126 => format!("FLOAT({p})"),
            _ => "FLOAT".to_string(),
        },
        other => other.to_string(),
    }
}

fn columns_from_rows(rows: &[Vec<Value>]) -> Vec<(String, ColumnStructureRow)> {
    rows.iter()
        .enumerate()
        .filter_map(|(idx, row)| {
            let table = text(row, 0)?;
            let name = text(row, 2)?;
            let data_type = format_column_type(
                &text(row, 3).unwrap_or_default(),
                int(row, 4),
                int(row, 5),
                text(row, 6).as_deref(),
                int(row, 7),
                int(row, 8),
            );
            let (foreign_key, fk_constraint_name) = match text(row, 12) {
                Some(s) => match s.split_once('\u{1}') {
                    Some((r, c)) => (Some(r.to_string()), Some(c.to_string())),
                    None => (Some(s), None),
                },
                None => (None, None),
            };
            Some((
                table,
                ColumnStructureRow {
                    ordinal_position: int(row, 1).unwrap_or((idx + 1) as i64) as i32,
                    name,
                    data_type,
                    is_nullable: text(row, 9).as_deref() != Some("N"),
                    // DATA_DEFAULT keeps the text as typed, trailing newline included.
                    column_default: text(row, 10).map(|d| d.trim().to_string()).filter(|d| !d.is_empty()),
                    is_primary_key: int(row, 11) == Some(1),
                    foreign_key,
                    fk_constraint_name,
                    comment: text(row, 13),
                },
            ))
        })
        .collect()
}

pub async fn get_column_structure(handle: &OracleHandle, schema: &str, table: &str) -> Result<Vec<ColumnStructureRow>, String> {
    let (sql, binds) = columns_query(schema, Some(table));
    let r = query_binds(handle, sql, binds).await?;
    Ok(columns_from_rows(&r.rows).into_iter().map(|(_, c)| c).collect())
}

pub async fn schema_column_structure(handle: &OracleHandle, schema: &str) -> Result<Vec<TableColumnStructure>, String> {
    let (sql, binds) = columns_query(schema, None);
    let r = query_binds(handle, sql, binds).await?;
    let mut out: Vec<TableColumnStructure> = Vec::new();
    for (table, col) in columns_from_rows(&r.rows) {
        match out.last_mut() {
            Some(last) if last.table == table => last.columns.push(col),
            _ => out.push(TableColumnStructure { table, columns: vec![col] }),
        }
    }
    Ok(out)
}

/// Foreign keys of one table, for the grid's jump-to-referenced-row links.
pub async fn foreign_keys(handle: &OracleHandle, schema: &str, table: &str) -> Result<Vec<ForeignKeyInfo>, String> {
    let sql = "SELECT f.CONSTRAINT_NAME, fc.COLUMN_NAME, r.OWNER, rc.TABLE_NAME, rc.COLUMN_NAME \
        FROM ALL_CONSTRAINTS f \
        JOIN ALL_CONS_COLUMNS fc ON fc.OWNER = f.OWNER AND fc.CONSTRAINT_NAME = f.CONSTRAINT_NAME \
        JOIN ALL_CONSTRAINTS r ON r.OWNER = f.R_OWNER AND r.CONSTRAINT_NAME = f.R_CONSTRAINT_NAME \
        JOIN ALL_CONS_COLUMNS rc ON rc.OWNER = r.OWNER AND rc.CONSTRAINT_NAME = r.CONSTRAINT_NAME AND rc.POSITION = fc.POSITION \
        WHERE f.CONSTRAINT_TYPE = 'R' AND f.OWNER = NVL(:1, SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA')) AND f.TABLE_NAME = :2 \
        ORDER BY f.CONSTRAINT_NAME, fc.POSITION";
    let r = query_binds(handle, sql.into(), vec![Bind::Text(schema.into()), Bind::Text(table.into())]).await?;
    let mut out: Vec<(String, ForeignKeyInfo)> = Vec::new();
    for row in &r.rows {
        let (Some(name), Some(col), Some(rs), Some(rt), Some(rcol)) = (text(row, 0), text(row, 1), text(row, 2), text(row, 3), text(row, 4)) else {
            continue;
        };
        match out.last_mut() {
            Some((n, fk)) if *n == name => {
                fk.columns.push(col);
                fk.referenced_columns.push(rcol);
            }
            _ => out.push((name, ForeignKeyInfo { columns: vec![col], referenced_schema: rs, referenced_table: rt, referenced_columns: vec![rcol] })),
        }
    }
    Ok(out.into_iter().map(|(_, fk)| fk).collect())
}

/// Foreign keys elsewhere that point at this table.
pub async fn incoming_foreign_keys(handle: &OracleHandle, schema: &str, table: &str) -> Result<Vec<IncomingForeignKey>, String> {
    let sql = "SELECT f.OWNER, f.TABLE_NAME, fc.COLUMN_NAME, rc.COLUMN_NAME, f.CONSTRAINT_NAME \
        FROM ALL_CONSTRAINTS r \
        JOIN ALL_CONSTRAINTS f ON f.R_OWNER = r.OWNER AND f.R_CONSTRAINT_NAME = r.CONSTRAINT_NAME AND f.CONSTRAINT_TYPE = 'R' \
        JOIN ALL_CONS_COLUMNS fc ON fc.OWNER = f.OWNER AND fc.CONSTRAINT_NAME = f.CONSTRAINT_NAME \
        JOIN ALL_CONS_COLUMNS rc ON rc.OWNER = r.OWNER AND rc.CONSTRAINT_NAME = r.CONSTRAINT_NAME AND rc.POSITION = fc.POSITION \
        WHERE r.OWNER = NVL(:1, SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA')) AND r.TABLE_NAME = :2 AND r.CONSTRAINT_TYPE IN ('P', 'U') \
        ORDER BY f.OWNER, f.TABLE_NAME, f.CONSTRAINT_NAME, fc.POSITION";
    let r = query_binds(handle, sql.into(), vec![Bind::Text(schema.into()), Bind::Text(table.into())]).await?;
    let mut map: std::collections::BTreeMap<(String, String, String), (Vec<String>, Vec<String>)> = Default::default();
    for row in &r.rows {
        let key = (text(row, 0).unwrap_or_default(), text(row, 1).unwrap_or_default(), text(row, 4).unwrap_or_default());
        let e = map.entry(key).or_default();
        e.0.push(text(row, 2).unwrap_or_default());
        e.1.push(text(row, 3).unwrap_or_default());
    }
    Ok(map
        .into_iter()
        .map(|((fs, ft, cn), (fc, tc))| IncomingForeignKey { from_schema: fs, from_table: ft, from_columns: fc, to_columns: tc, constraint_name: cn })
        .collect())
}

/// `DBMS_METADATA.GET_DDL` for a table or view; a synthesized CREATE TABLE
/// when the user may not call it on that object.
pub async fn get_ddl(handle: &OracleHandle, schema: &str, table: &str) -> Result<String, String> {
    let kind = query_binds(
        handle,
        "SELECT OBJECT_TYPE FROM ALL_OBJECTS WHERE OWNER = NVL(:1, SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA')) AND OBJECT_NAME = :2 AND OBJECT_TYPE IN ('TABLE', 'VIEW')".into(),
        vec![Bind::Text(schema.into()), Bind::Text(table.into())],
    )
    .await?
    .rows
    .first()
    .and_then(|row| text(row, 0))
    .unwrap_or_else(|| "TABLE".to_string());
    let ddl = query_binds(
        handle,
        "SELECT DBMS_METADATA.GET_DDL(:1, :2, :3) FROM DUAL".into(),
        vec![Bind::Text(kind), Bind::Text(table.into()), Bind::Text(schema.into())],
    )
    .await;
    if let Ok(r) = ddl {
        if let Some(d) = r.rows.first().and_then(|row| text(row, 0)) {
            let d = d.trim();
            if !d.is_empty() {
                return Ok(format!("{d};"));
            }
        }
    }
    let cols = get_column_structure(handle, schema, table).await?;
    if cols.is_empty() {
        return Ok(String::new());
    }
    let mut lines: Vec<String> = cols
        .iter()
        .map(|c| {
            let def = c.column_default.as_deref().map(|d| format!(" DEFAULT {d}")).unwrap_or_default();
            let null = if c.is_nullable { "" } else { " NOT NULL" };
            format!("    {} {}{def}{null}", quote_ident(&c.name), c.data_type)
        })
        .collect();
    let pk: Vec<String> = cols.iter().filter(|c| c.is_primary_key).map(|c| quote_ident(&c.name)).collect();
    if !pk.is_empty() {
        lines.push(format!("    PRIMARY KEY ({})", pk.join(", ")));
    }
    Ok(format!("CREATE TABLE {} (\n{}\n);", table_ref(schema, table), lines.join(",\n")))
}

// ── Data browsing ────────────────────────────────────────────────────────────

/// A column the WHERE builder knows by name and catalog type.
#[derive(Debug, Clone)]
pub struct FilterColumn {
    pub name: String,
    pub data_type: String,
}

impl FilterColumn {
    fn base_type(&self) -> String {
        self.data_type.split('(').next().unwrap_or("").trim().to_ascii_uppercase()
    }

    /// The column as text for LIKE, or None when it has no text form to match
    /// (BLOB, LONG, BFILE, object types).
    fn as_text(&self) -> Option<String> {
        let q = quote_ident(&self.name);
        let t = self.base_type();
        match t.as_str() {
            "VARCHAR2" | "NVARCHAR2" | "CHAR" | "NCHAR" | "CLOB" | "NCLOB" => Some(q),
            "RAW" => Some(format!("RAWTOHEX({q})")),
            "BLOB" | "LONG" | "LONG RAW" | "BFILE" | "XMLTYPE" | "JSON" => None,
            _ if t.contains('.') || t.starts_with("SDO_") || t.starts_with("ANYDATA") => None,
            _ => Some(format!("TO_CHAR({q})")),
        }
    }

    /// `col = :n`, comparing the way the column's type allows.
    fn equals(&self, ph: &str) -> Option<String> {
        let q = quote_ident(&self.name);
        match self.base_type().as_str() {
            "CLOB" | "NCLOB" => Some(format!("DBMS_LOB.COMPARE({q}, TO_CLOB({ph})) = 0")),
            "RAW" => Some(format!("{q} = HEXTORAW({ph})")),
            "BLOB" | "LONG" | "LONG RAW" | "BFILE" => None,
            _ => Some(format!("{q} = {ph}")),
        }
    }
}

fn escape_like(s: &str) -> String {
    super::sql_util::escape_like_backslash(s)
}

/// WHERE clause with `:n` binds numbered from `first_bind`. Search and the
/// text filters are case-insensitive, like ILIKE on Postgres.
pub fn build_where(cols: &[FilterColumn], search: Option<&str>, filters: &[RowFilter], first_bind: usize) -> Result<(String, Vec<Bind>), String> {
    let mut parts: Vec<(Option<&'static str>, String)> = Vec::new();
    let mut binds: Vec<Bind> = Vec::new();
    let mut ph = |binds: &mut Vec<Bind>, v: String| -> String {
        binds.push(Bind::Text(v));
        format!(":{}", first_bind + binds.len() - 1)
    };

    let any_text = |binds: &mut Vec<Bind>, ph: &mut dyn FnMut(&mut Vec<Bind>, String) -> String, op: &str, v: &str| -> Option<String> {
        let ors: Vec<String> = cols
            .iter()
            .filter_map(|c| {
                let t = c.as_text()?;
                Some(match op {
                    "eq" => format!("{t} = {}", ph(binds, v.to_string())),
                    "starts_with" => format!("UPPER({t}) LIKE UPPER({}) ESCAPE '\\'", ph(binds, format!("{}%", escape_like(v)))),
                    "ends_with" => format!("UPPER({t}) LIKE UPPER({}) ESCAPE '\\'", ph(binds, format!("%{}", escape_like(v)))),
                    _ => format!("UPPER({t}) LIKE UPPER({}) ESCAPE '\\'", ph(binds, format!("%{}%", escape_like(v)))),
                })
            })
            .collect();
        if ors.is_empty() {
            None
        } else {
            Some(format!("({})", ors.join(" OR ")))
        }
    };

    if let Some(term) = search.map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(c) = any_text(&mut binds, &mut ph, "contains", term) {
            parts.push((None, c));
        }
    }

    for f in filters {
        let conj: Option<&'static str> = if parts.is_empty() {
            None
        } else if f.conjunct.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("or")) {
            Some("OR")
        } else {
            Some("AND")
        };
        if f.op == super::sql_util::RAW_SQL_FILTER_OP {
            let raw = f.value.as_deref().unwrap_or("");
            if let Some(expr) = super::sql_util::raw_where_expr(raw, super::sql_util::RawFilterDialect::Oracle)? {
                parts.push((conj, expr));
            }
            continue;
        }
        if f.column == "__any__" {
            let v = f.value.as_deref().unwrap_or("").trim();
            if !v.is_empty() {
                if let Some(c) = any_text(&mut binds, &mut ph, f.op.as_str(), v) {
                    parts.push((conj, c));
                }
            }
            continue;
        }
        let Some(col) = cols.iter().find(|c| c.name == f.column) else {
            return Err(format!("Unknown column: {}", f.column));
        };
        let q = quote_ident(&col.name);
        let v = normalize_temporal(&col.data_type, f.value.as_deref().unwrap_or("").trim());
        let op = match f.op.as_str() {
            "=" => "eq",
            "!=" | "<>" | "ne" => "neq",
            ">" => "gt",
            ">=" => "gte",
            "<" => "lt",
            "<=" => "lte",
            other => other,
        };
        let text_col = || col.as_text().ok_or_else(|| format!("Column {} can't be searched as text", col.name));
        let clause = match op {
            "is_null" => format!("{q} IS NULL"),
            "is_not_null" => format!("{q} IS NOT NULL"),
            _ if v.is_empty() => continue,
            "eq" => {
                let p = ph(&mut binds, v);
                col.equals(&p).ok_or_else(|| format!("Column {} can't be compared", col.name))?
            }
            "neq" => {
                let p = ph(&mut binds, v);
                let eq = col.equals(&p).ok_or_else(|| format!("Column {} can't be compared", col.name))?;
                format!("({q} IS NULL OR NOT ({eq}))")
            }
            "gt" => format!("{q} > {}", ph(&mut binds, v)),
            "gte" => format!("{q} >= {}", ph(&mut binds, v)),
            "lt" => format!("{q} < {}", ph(&mut binds, v)),
            "lte" => format!("{q} <= {}", ph(&mut binds, v)),
            "contains" => format!("UPPER({}) LIKE UPPER({}) ESCAPE '\\'", text_col()?, ph(&mut binds, format!("%{}%", escape_like(&v)))),
            // A NULL cell contains nothing, so it satisfies "does not contain".
            "not_contains" => {
                let t = text_col()?;
                format!("({q} IS NULL OR UPPER({t}) NOT LIKE UPPER({}) ESCAPE '\\')", ph(&mut binds, format!("%{}%", escape_like(&v))))
            }
            "starts_with" => format!("UPPER({}) LIKE UPPER({}) ESCAPE '\\'", text_col()?, ph(&mut binds, format!("{}%", escape_like(&v)))),
            "ends_with" => format!("UPPER({}) LIKE UPPER({}) ESCAPE '\\'", text_col()?, ph(&mut binds, format!("%{}", escape_like(&v)))),
            "between" => {
                let mut it = v.splitn(2, ',');
                let from = it.next().unwrap_or("").trim().to_string();
                let to = it.next().unwrap_or("").trim().to_string();
                let a = ph(&mut binds, from);
                let b = ph(&mut binds, to);
                format!("({q} >= {a} AND {q} <= {b})")
            }
            other => return Err(format!("Unsupported filter operator: {other}")),
        };
        parts.push((conj, clause));
    }

    if parts.is_empty() {
        return Ok((String::new(), binds));
    }
    let mut out = String::from(" WHERE ");
    for (i, (conj, cond)) in parts.into_iter().enumerate() {
        if i > 0 {
            out.push(' ');
            out.push_str(conj.unwrap_or("AND"));
            out.push(' ');
        }
        out.push_str(&cond);
    }
    Ok((out, binds))
}

/// `ORDER BY` for a page: the chosen sort with the key as tie-breaker, else
/// the primary key, else ROWID for a table, so pages never overlap or skip.
pub fn order_by(sort: Option<(&str, bool)>, pk: &[String], is_table: bool) -> String {
    let mut terms: Vec<String> = Vec::new();
    if let Some((c, desc)) = sort {
        terms.push(format!("{} {}", quote_ident(c), if desc { "DESC" } else { "ASC" }));
        terms.extend(pk.iter().filter(|k| k.as_str() != c).map(|k| quote_ident(k)));
        if pk.is_empty() && is_table {
            terms.push("ROWID".into());
        }
    } else if !pk.is_empty() {
        terms.extend(pk.iter().map(|k| quote_ident(k)));
    } else if is_table {
        terms.push("ROWID".into());
    }
    if terms.is_empty() {
        String::new()
    } else {
        format!(" ORDER BY {}", terms.join(", "))
    }
}

/// `OFFSET n ROWS FETCH NEXT m ROWS ONLY` (Oracle 12c and later).
pub fn page_clause(limit: i64, offset: i64) -> String {
    format!(" OFFSET {} ROWS FETCH NEXT {} ROWS ONLY", offset.max(0), limit.max(0))
}

#[allow(clippy::too_many_arguments)]
pub async fn get_table_rows(
    handle: &OracleHandle,
    schema: &str,
    table: &str,
    limit: i64,
    offset: i64,
    search: Option<String>,
    sort_column: Option<String>,
    sort_direction: Option<String>,
    filters: Option<Vec<RowFilter>>,
    include_meta: bool,
) -> Result<TableRows, String> {
    let t0 = Instant::now();
    let meta = table_meta(handle, schema, table).await?;
    let structure = &meta.structure;
    let is_table = meta.is_table;
    let cols: Vec<FilterColumn> = structure.iter().map(|c| FilterColumn { name: c.name.clone(), data_type: c.data_type.clone() }).collect();
    let pk: Vec<String> = structure.iter().filter(|c| c.is_primary_key).map(|c| c.name.clone()).collect();

    let filters = filters.unwrap_or_default();
    let (where_sql, binds) = build_where(&cols, search.as_deref(), &filters, 1)?;
    let tq = table_ref(schema, table);
    let sort = sort_column
        .as_deref()
        .filter(|c| !c.trim().is_empty() && cols.iter().any(|k| k.name == *c))
        .map(|c| (c, sort_direction.as_deref().is_some_and(|d| d.eq_ignore_ascii_case("desc"))));
    let order = order_by(sort, &pk, is_table);

    let count_sql = format!("SELECT COUNT(*) FROM {tq}{where_sql}");
    let data_sql = format!("SELECT * FROM {tq}{where_sql}{order}{}", page_clause(limit, offset));
    let (count_q, data_q) = (count_sql.clone(), data_sql.clone());
    let (total, mut result) = run_blocking(handle, move |conn| {
        let total = run_one(conn, &count_q, &binds)?.rows.first().and_then(|r| int(r, 0)).unwrap_or(0);
        let data = run_one(conn, &data_q, &binds)?;
        Ok((total, data))
    })
    .await?;

    // Catalog facts the insert row needs: NOT NULL and defaults.
    let by_name: HashMap<&str, &ColumnStructureRow> = structure.iter().map(|c| (c.name.as_str(), c)).collect();
    for c in result.columns.iter_mut() {
        if let Some(s) = by_name.get(c.name.as_str()) {
            c.nullable = s.is_nullable;
            c.has_default = s.column_default.is_some();
            c.auto_generated = s.column_default.as_deref().is_some_and(|d| d.contains(".nextval") || d.contains("ISEQ$$"));
        }
    }
    let fks = if include_meta { meta.foreign_keys.clone() } else { Vec::new() };

    Ok(TableRows {
        preview_columns: Vec::new(),
        columns: result.columns,
        rows: result.rows,
        total,
        query_ms: t0.elapsed().as_millis() as u64,
        primary_key: if include_meta { pk } else { Vec::new() },
        foreign_keys: fks,
        sql: format!("{data_sql}\n{count_sql}"),
    })
}

/// A table's columns, whether it is a table (not a view) and its foreign keys:
/// from the session's cache, or the catalog on a miss.
async fn table_meta(handle: &OracleHandle, schema: &str, table: &str) -> Result<std::sync::Arc<TableMeta>, String> {
    let key = (schema.to_string(), table.to_string());
    if let Some(meta) = handle.cached_meta(&key) {
        return Ok(meta);
    }
    let structure = get_column_structure(handle, schema, table).await?;
    if structure.is_empty() {
        return Err(format!("Table {schema}.{table} not found, or you have no access to it"));
    }
    let is_table = query_binds(
        handle,
        "SELECT 1 FROM ALL_TABLES WHERE OWNER = NVL(:1, SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA')) AND TABLE_NAME = :2".into(),
        vec![Bind::Text(schema.into()), Bind::Text(table.into())],
    )
    .await
    .map(|r| !r.rows.is_empty())
    .unwrap_or(false);
    let foreign_keys = foreign_keys(handle, schema, table).await.unwrap_or_default();
    let meta = std::sync::Arc::new(TableMeta { structure, is_table, foreign_keys });
    handle.store_meta(key, meta.clone());
    Ok(meta)
}

// ── Row editing ──────────────────────────────────────────────────────────────

/// `WHERE "K1" = :n AND ...` for one row's key, binds numbered from `first`.
pub fn key_where(pk: &HashMap<String, Value>, first: usize) -> (String, Vec<Bind>) {
    let mut keys: Vec<&String> = pk.keys().collect();
    keys.sort();
    let mut binds = Vec::new();
    let mut parts = Vec::new();
    for k in keys {
        let v = &pk[k];
        if v.is_null() {
            parts.push(format!("{} IS NULL", quote_ident(k)));
        } else {
            parts.push(format!("{} = :{}", quote_ident(k), first + binds.len()));
            binds.push(Bind::from_json(v));
        }
    }
    (format!(" WHERE {}", parts.join(" AND ")), binds)
}

/// Column name -> catalog type, for shaping bound values. Empty on failure:
/// values then bind as given.
async fn column_types(handle: &OracleHandle, schema: &str, table: &str) -> HashMap<String, String> {
    let sql = "SELECT COLUMN_NAME, DATA_TYPE FROM ALL_TAB_COLUMNS \
        WHERE OWNER = NVL(:1, SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA')) AND TABLE_NAME = :2";
    match query_binds(handle, sql.into(), vec![Bind::Text(schema.into()), Bind::Text(table.into())]).await {
        Ok(r) => r.rows.iter().filter_map(|row| Some((text(row, 0)?, text(row, 1)?))).collect(),
        Err(_) => HashMap::new(),
    }
}

pub async fn update_table_cell(
    handle: &OracleHandle,
    schema: &str,
    table: &str,
    primary_key: HashMap<String, Value>,
    column: &str,
    value: &Value,
) -> Result<(), String> {
    if primary_key.is_empty() {
        return Err("Cannot update row: table has no primary key".into());
    }
    let types = column_types(handle, schema, table).await;
    let (where_sql, key_binds) = key_where(&primary_key, 2);
    let sql = format!("UPDATE {} SET {} = :1{where_sql}", table_ref(schema, table), quote_ident(column));
    let mut binds = vec![Bind::for_column(value, types.get(column).map(String::as_str))];
    binds.extend(key_binds);
    let res = run_blocking(handle, move |conn| run_commit(conn, &sql, &binds)).await?;
    if res.row_count == Some(0) {
        return Err("No row updated (row may have changed)".into());
    }
    Ok(())
}

/// Insert one row and return it as stored (defaults, identity and trigger
/// values filled in), read back by the ROWID the insert produced.
pub async fn insert_table_row(
    handle: &OracleHandle,
    schema: &str,
    table: &str,
    values: HashMap<String, Value>,
) -> Result<Vec<Value>, String> {
    let mut cols: Vec<String> = values.keys().cloned().collect();
    cols.sort();
    let tq = table_ref(schema, table);
    let sql = if cols.is_empty() {
        format!("INSERT INTO {tq} VALUES (DEFAULT)")
    } else {
        let names = cols.iter().map(|c| quote_ident(c)).collect::<Vec<_>>().join(", ");
        let phs = (1..=cols.len()).map(|i| format!(":{i}")).collect::<Vec<_>>().join(", ");
        format!("INSERT INTO {tq} ({names}) VALUES ({phs})")
    };
    let types = column_types(handle, schema, table).await;
    let binds: Vec<Bind> = cols.iter().map(|c| Bind::for_column(&values[c], types.get(c).map(String::as_str))).collect();
    run_blocking(handle, move |conn| {
        let boxed = bind_refs(&binds);
        let params: Vec<&dyn ToSql> = boxed.iter().map(|b| b.as_ref()).collect();
        let mut stmt = conn.statement(&sql).build().map_err(ora_err)?;
        if let Err(e) = stmt.execute(&params) {
            let _ = conn.rollback();
            return Err(ora_err(e));
        }
        conn.commit().map_err(ora_err)?;
        let rowid = stmt.last_row_id().ok().flatten();
        if let Some(rowid) = rowid {
            if let Ok(r) = run_one(conn, &format!("SELECT * FROM {tq} WHERE ROWID = CHARTOROWID(:1)"), &[Bind::Text(rowid)]) {
                if let Some(row) = r.rows.into_iter().next() {
                    return Ok(row);
                }
            }
        }
        Ok(Vec::new())
    })
    .await
}

pub async fn delete_table_rows(
    handle: &OracleHandle,
    schema: &str,
    table: &str,
    primary_keys: Vec<HashMap<String, Value>>,
) -> Result<u64, String> {
    if primary_keys.is_empty() {
        return Ok(0);
    }
    if primary_keys.iter().any(|pk| pk.is_empty()) {
        return Err("Cannot delete rows: table has no primary key".into());
    }
    let tq = table_ref(schema, table);
    run_blocking(handle, move |conn| {
        // All or nothing: one transaction, one commit at the end.
        let run = |conn: &Connection| -> Result<u64, String> {
            let mut deleted = 0u64;
            for pk in &primary_keys {
                let (where_sql, binds) = key_where(pk, 1);
                let boxed = bind_refs(&binds);
                let params: Vec<&dyn ToSql> = boxed.iter().map(|b| b.as_ref()).collect();
                let mut stmt = conn.statement(&format!("DELETE FROM {tq}{where_sql}")).build().map_err(ora_err)?;
                stmt.execute(&params).map_err(ora_err)?;
                deleted += stmt.row_count().unwrap_or(0);
            }
            conn.commit().map_err(ora_err)?;
            Ok(deleted)
        };
        let out = run(conn);
        if out.is_err() {
            let _ = conn.rollback();
        }
        out
    })
    .await
}

/// Against a real server: `STROKE_ORACLE_TEST=user/password@//host:1521/SERVICE
/// cargo test --lib oracle_live -- --ignored --nocapture`, with a CUSTOMERS
/// table in that user's schema.
#[cfg(test)]
mod live_tests {
    use super::*;

    fn live_cfg() -> Option<OracleConfig> {
        let url = std::env::var("STROKE_ORACLE_TEST").ok()?;
        let (creds, cs) = url.split_once('@')?;
        let (user, password) = creds.split_once('/')?;
        Some(OracleConfig {
            name: "live".into(),
            host: String::new(),
            port: 1521,
            service_name: String::new(),
            sid: String::new(),
            connect_string: cs.into(),
            user: user.into(),
            password: password.into(),
            client_lib_dir: std::env::var("STROKE_ORACLE_CLIENT_DIR").unwrap_or_default(),
        })
    }

    #[tokio::test]
    #[ignore]
    async fn oracle_live_meta_cache_and_flags() {
        let Some(cfg) = live_cfg() else { return };
        let h = connect(&cfg).await.expect("connect");
        println!("container: {}", container_name(&h).await.unwrap());

        let t = Instant::now();
        let first = get_table_rows(&h, "", "CUSTOMERS", 100, 0, None, None, None, None, true).await.unwrap();
        let first_ms = t.elapsed().as_millis();
        let t = Instant::now();
        let second = get_table_rows(&h, "", "CUSTOMERS", 100, 0, None, Some("NAME".into()), Some("desc".into()), None, true).await.unwrap();
        let second_ms = t.elapsed().as_millis();
        println!("first fetch {first_ms}ms, cached {second_ms}ms, rows {}", second.rows.len());
        assert_eq!(first.rows.len(), second.rows.len());
        assert_eq!(second.primary_key, vec!["ID".to_string()]);

        // DDL forgets the cached columns.
        execute_sql(&h, "ALTER TABLE CUSTOMERS ADD (NOTE VARCHAR2(10))").await.unwrap();
        let after = get_table_rows(&h, "", "CUSTOMERS", 100, 0, None, None, None, None, true).await.unwrap();
        execute_sql(&h, "ALTER TABLE CUSTOMERS DROP COLUMN NOTE").await.unwrap();
        assert!(after.columns.iter().any(|c| c.name == "NOTE"));

        let sql_filter = |v: &str| Some(vec![RowFilter { column: String::new(), op: "sql".into(), value: Some(v.into()), conjunct: None, data_type: None }]);
        let filtered = get_table_rows(&h, "", "CUSTOMERS", 100, 0, None, None, None, sql_filter("ID < 3"), false).await.unwrap();
        assert_eq!(filtered.total, 2, "{:?}", filtered.rows);
        assert_eq!(filtered.rows.len(), 2);

        let tables = list_tables(&h, "").await.unwrap();
        let mine = tables.iter().find(|t| t.name == "CUSTOMERS").unwrap();
        assert_eq!(mine.system, Some(false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> OracleConfig {
        OracleConfig {
            name: "t".into(),
            host: "db.example.com".into(),
            port: 1521,
            service_name: "FREEPDB1".into(),
            sid: String::new(),
            connect_string: String::new(),
            user: "scott".into(),
            password: "tiger".into(),
            client_lib_dir: String::new(),
        }
    }

    #[test]
    fn connect_string_forms() {
        assert_eq!(connect_string(&cfg()), "//db.example.com:1521/FREEPDB1");
        let mut c = cfg();
        c.sid = "XE".into();
        assert_eq!(
            connect_string(&c),
            "(DESCRIPTION=(ADDRESS=(PROTOCOL=TCP)(HOST=db.example.com)(PORT=1521))(CONNECT_DATA=(SID=XE)))"
        );
        let mut c = cfg();
        c.connect_string = "  tcps://h:2484/svc?wallet_location=/w ".into();
        assert_eq!(connect_string(&c), "tcps://h:2484/svc?wallet_location=/w");
    }

    #[test]
    fn probe_targets() {
        assert_eq!(probe_target(&cfg()), Some(("db.example.com".into(), 1521)));
        let mut c = cfg();
        c.sid = "XE".into();
        c.port = 1522;
        assert_eq!(probe_target(&c), Some(("db.example.com".into(), 1522)));
        let mut c = cfg();
        c.connect_string = "tcps://h:2484/svc".into();
        assert_eq!(probe_target(&c), Some(("h".into(), 2484)));
        c.connect_string = "h2/svc".into();
        assert_eq!(probe_target(&c), Some(("h2".into(), 1521)));
        c.connect_string = "MYALIAS".into();
        assert_eq!(probe_target(&c), None);
        c.connect_string = "(DESCRIPTION = (ADDRESS = (PROTOCOL = TCP)(HOST = 10.0.0.5)(PORT = 1530))(CONNECT_DATA = (SERVICE_NAME = x)))".into();
        assert_eq!(probe_target(&c), Some(("10.0.0.5".into(), 1530)));
    }

    #[test]
    fn splits_sql_and_drops_terminators() {
        assert_eq!(split_statements("SELECT 1 FROM dual;"), vec!["SELECT 1 FROM dual"]);
        assert_eq!(split_statements("select 1 from dual; select 'a;b' from dual ;"), vec!["select 1 from dual", "select 'a;b' from dual"]);
        assert_eq!(split_statements("select q'[it's; fine]' from dual; select 2 from dual"), vec!["select q'[it's; fine]' from dual", "select 2 from dual"]);
        assert_eq!(split_statements("select \"A;B\" from t -- c; d\n;"), vec!["select \"A;B\" from t -- c; d"]);
        assert_eq!(split_statements("  ;; -- nothing\n"), Vec::<String>::new());
        assert_eq!(split_statements("select 1 from dual\n/\nselect 2 from dual\n/\n"), vec!["select 1 from dual", "select 2 from dual"]);
        assert_eq!(split_statements("SELECT 10 / 2 FROM dual"), vec!["SELECT 10 / 2 FROM dual"]);
    }

    #[test]
    fn splits_plsql_blocks_at_slash() {
        let script = "BEGIN\n  UPDATE t SET a = 1;\n  COMMIT;\nEND;\n/\nSELECT * FROM t;";
        assert_eq!(split_statements(script), vec!["BEGIN\n  UPDATE t SET a = 1;\n  COMMIT;\nEND;", "SELECT * FROM t"]);
        let proc_ = "CREATE OR REPLACE PROCEDURE p AS\nBEGIN\n  NULL;\nEND;\n/";
        assert_eq!(split_statements(proc_), vec!["CREATE OR REPLACE PROCEDURE p AS\nBEGIN\n  NULL;\nEND;"]);
        // A block at the end with no slash still runs whole.
        assert_eq!(split_statements("DECLARE x NUMBER; BEGIN x := 1; END;"), vec!["DECLARE x NUMBER; BEGIN x := 1; END;"]);
        assert_eq!(split_statements("-- lead\nbegin null; end;"), vec!["-- lead\nbegin null; end;"]);
        // CREATE TABLE is plain SQL even though it starts with CREATE.
        assert_eq!(split_statements("CREATE TABLE t (a NUMBER); INSERT INTO t VALUES (1);"), vec!["CREATE TABLE t (a NUMBER)", "INSERT INTO t VALUES (1)"]);
    }

    #[test]
    fn numbers_keep_their_digits() {
        assert_eq!(number_json("42"), Value::from(42));
        assert_eq!(number_json("-7"), Value::from(-7));
        assert_eq!(number_json("12.5"), serde_json::json!(12.5));
        assert_eq!(number_json(".5"), serde_json::json!(0.5));
        assert_eq!(number_json("-.25"), serde_json::json!(-0.25));
        assert_eq!(number_json("12345678901234567890"), Value::String("12345678901234567890".into()));
        assert_eq!(number_json("0.1000000000000000000001"), Value::String("0.1000000000000000000001".into()));
        assert_eq!(number_json("9007199254740993"), Value::String("9007199254740993".into()));
    }

    #[test]
    fn column_types() {
        assert_eq!(format_column_type("VARCHAR2", Some(200), Some(50), Some("C"), None, None), "VARCHAR2(50 CHAR)");
        assert_eq!(format_column_type("VARCHAR2", Some(50), Some(50), Some("B"), None, None), "VARCHAR2(50)");
        assert_eq!(format_column_type("NVARCHAR2", Some(100), Some(50), Some("C"), None, None), "NVARCHAR2(50)");
        assert_eq!(format_column_type("NUMBER", Some(22), None, None, Some(10), Some(2)), "NUMBER(10,2)");
        assert_eq!(format_column_type("NUMBER", Some(22), None, None, Some(9), Some(0)), "NUMBER(9)");
        assert_eq!(format_column_type("NUMBER", Some(22), None, None, None, None), "NUMBER");
        assert_eq!(format_column_type("NUMBER", Some(22), None, None, None, Some(0)), "INTEGER");
        assert_eq!(format_column_type("RAW", Some(16), None, None, None, None), "RAW(16)");
        assert_eq!(format_column_type("TIMESTAMP(6) WITH TIME ZONE", Some(13), None, None, None, Some(6)), "TIMESTAMP(6) WITH TIME ZONE");
    }

    fn fc(name: &str, ty: &str) -> FilterColumn {
        FilterColumn { name: name.into(), data_type: ty.into() }
    }

    #[test]
    fn where_uses_numbered_binds() {
        let cols = vec![fc("ID", "NUMBER(9)"), fc("NAME", "VARCHAR2(50)"), fc("DOC", "BLOB"), fc("NOTE", "CLOB")];
        let filters = vec![
            RowFilter { column: "ID".into(), op: "gt".into(), value: Some("5".into()), conjunct: None, data_type: None },
            RowFilter { column: "NAME".into(), op: "contains".into(), value: Some("a_b".into()), conjunct: Some("or".into()), data_type: None },
            RowFilter { column: "NOTE".into(), op: "eq".into(), value: Some("x".into()), conjunct: None, data_type: None },
        ];
        let (sql, binds) = build_where(&cols, Some("hi"), &filters, 1).unwrap();
        assert_eq!(
            sql,
            " WHERE (UPPER(TO_CHAR(\"ID\")) LIKE UPPER(:1) ESCAPE '\\' OR UPPER(\"NAME\") LIKE UPPER(:2) ESCAPE '\\' OR UPPER(\"NOTE\") LIKE UPPER(:3) ESCAPE '\\') \
             AND \"ID\" > :4 OR UPPER(\"NAME\") LIKE UPPER(:5) ESCAPE '\\' AND DBMS_LOB.COMPARE(\"NOTE\", TO_CLOB(:6)) = 0"
        );
        assert_eq!(
            binds,
            vec![
                Bind::Text("%hi%".into()),
                Bind::Text("%hi%".into()),
                Bind::Text("%hi%".into()),
                Bind::Text("5".into()),
                Bind::Text("%a\\_b%".into()),
                Bind::Text("x".into()),
            ]
        );
        let bad = vec![RowFilter { column: "NOPE".into(), op: "eq".into(), value: Some("1".into()), conjunct: None, data_type: None }];
        assert!(build_where(&cols, None, &bad, 1).is_err());
        let raw = vec![RowFilter { column: String::new(), op: "sql".into(), value: Some("ID = :x".into()), conjunct: None, data_type: None }];
        assert!(build_where(&cols, None, &raw, 1).is_err());
        let raw = vec![RowFilter { column: String::new(), op: "sql".into(), value: Some("ID > 3 AND NAME LIKE 'a%'".into()), conjunct: None, data_type: None }];
        assert_eq!(build_where(&cols, None, &raw, 1).unwrap().0, " WHERE (ID > 3 AND NAME LIKE 'a%')");
    }

    #[test]
    fn page_and_order() {
        assert_eq!(page_clause(50, 100), " OFFSET 100 ROWS FETCH NEXT 50 ROWS ONLY");
        assert_eq!(order_by(None, &["ID".into()], true), " ORDER BY \"ID\"");
        assert_eq!(order_by(None, &[], true), " ORDER BY ROWID");
        assert_eq!(order_by(None, &[], false), "");
        assert_eq!(order_by(Some(("NAME", true)), &["ID".into()], true), " ORDER BY \"NAME\" DESC, \"ID\"");
        assert_eq!(order_by(Some(("NAME", false)), &[], true), " ORDER BY \"NAME\" ASC, ROWID");
    }

    #[test]
    fn key_where_binds_in_order() {
        let pk = HashMap::from([("B".to_string(), Value::from("x")), ("A".to_string(), Value::from(1))]);
        let (sql, binds) = key_where(&pk, 2);
        assert_eq!(sql, " WHERE \"A\" = :2 AND \"B\" = :3");
        assert_eq!(binds, vec![Bind::Int(1), Bind::Text("x".into())]);
        let pk = HashMap::from([("A".to_string(), Value::Null), ("B".to_string(), Value::from(2.5))]);
        let (sql, binds) = key_where(&pk, 1);
        assert_eq!(sql, " WHERE \"A\" IS NULL AND \"B\" = :1");
        assert_eq!(binds, vec![Bind::Text("2.5".into())]);
    }

    #[test]
    fn temporal_text_is_shaped_for_nls() {
        let tz = "TIMESTAMP(6) WITH TIME ZONE";
        assert_eq!(normalize_temporal(tz, "2001-02-03 04:05:06 -07:00"), "2001-02-03 04:05:06.0 -07:00");
        assert_eq!(normalize_temporal(tz, "2001-02-03T04:05:06-07:00"), "2001-02-03 04:05:06.0 -07:00");
        assert_eq!(normalize_temporal(tz, "2001-02-03 04:05:06.25 +05:30"), "2001-02-03 04:05:06.25 +05:30");
        assert_eq!(normalize_temporal(tz, "2001-02-03 04:05:06Z"), "2001-02-03 04:05:06.0 +00:00");
        assert_eq!(normalize_temporal(tz, "2001-02-03 04:05:06 UTC"), "2001-02-03 04:05:06.0 +00:00");
        assert_eq!(normalize_temporal(tz, "2001-02-03"), "2001-02-03");
        assert_eq!(normalize_temporal("DATE", "2001-02-03T04:05"), "2001-02-03 04:05");
        assert_eq!(normalize_temporal("VARCHAR2", "2001-02-03 04:05:06 -07:00"), "2001-02-03 04:05:06 -07:00");
        assert_eq!(normalize_temporal(tz, "not a date"), "not a date");
    }

    #[test]
    fn quoting() {
        assert_eq!(quote_ident("EMP"), "\"EMP\"");
        assert_eq!(quote_ident("we\"ird"), "\"we\"\"ird\"");
        assert_eq!(table_ref("HR", "EMP"), "\"HR\".\"EMP\"");
    }

    // ── Live tests ───────────────────────────────────────────────────────────
    //
    // Need a real Oracle and Instant Client. With the gvenzl/oracle-free image
    // (`-e ORACLE_PASSWORD=... -e APP_USER=stroke -e APP_USER_PASSWORD=strokepass`):
    //
    //   STROKE_ORACLE_TEST_URL=stroke/strokepass@//127.0.0.1:1521/FREEPDB1 \
    //   STROKE_ORACLE_CLIENT_DIR=/path/to/instantclient_23_x \
    //   cargo test --lib db::oracle::tests -- --ignored --test-threads=1

    fn live_config() -> Option<OracleConfig> {
        let url = std::env::var("STROKE_ORACLE_TEST_URL").ok()?;
        let (creds, cs) = url.split_once('@')?;
        let (user, password) = creds.split_once('/')?;
        Some(OracleConfig {
            name: "live".into(),
            host: String::new(),
            port: 1521,
            service_name: String::new(),
            sid: String::new(),
            connect_string: cs.to_string(),
            user: user.into(),
            password: password.into(),
            client_lib_dir: String::new(),
        })
    }

    async fn live() -> Option<OracleHandle> {
        let cfg = live_config()?;
        Some(crate::db::connection::open_oracle(&cfg).await.expect("connect"))
    }

    async fn exec(h: &OracleHandle, sql: &str) -> SqlResult {
        execute_sql(h, sql).await.unwrap_or_else(|e| panic!("{sql}: {e}"))
    }

    #[tokio::test]
    #[ignore]
    async fn live_browse_edit_roundtrip() {
        let Some(h) = live().await else { return };
        let _ = execute_sql(&h, "DROP TABLE stroke_child PURGE").await;
        let _ = execute_sql(&h, "DROP TABLE stroke_types PURGE").await;
        exec(
            &h,
            "CREATE TABLE stroke_types (\
               id NUMBER(9) PRIMARY KEY, \
               name VARCHAR2(50 CHAR) NOT NULL, \
               nname NVARCHAR2(20), \
               price NUMBER(10,2), \
               ratio BINARY_DOUBLE, \
               born DATE, \
               seen TIMESTAMP(6), \
               seen_tz TIMESTAMP WITH TIME ZONE, \
               note CLOB, \
               doc BLOB, \
               tag RAW(4), \
               created DATE DEFAULT SYSDATE)",
        )
        .await;
        exec(&h, "CREATE TABLE stroke_child (id NUMBER PRIMARY KEY, parent_id NUMBER REFERENCES stroke_types(id))").await;
        exec(&h, "COMMENT ON COLUMN stroke_types.name IS 'display name'").await;
        // A script: PL/SQL block ended by `/`, then plain SQL with `;`.
        let script = "BEGIN\n  FOR i IN 1..30 LOOP\n    INSERT INTO stroke_types (id, name, nname, price, ratio, born, seen, seen_tz, note, doc, tag) \
            VALUES (i, 'name ' || i, N'ñ' || i, i + 0.5, i / 4, DATE '2024-01-02' + i, TIMESTAMP '2024-01-02 03:04:05.123456', \
            TIMESTAMP '2024-01-02 03:04:05 +05:30', 'clob ' || i, HEXTORAW('DEADBEEF'), HEXTORAW('0A0B0C0D'));\n  END LOOP;\nEND;\n/\n\
            INSERT INTO stroke_child VALUES (1, 1);\n";
        for s in split_statements(script) {
            exec(&h, &s).await;
        }

        let schemas = list_schemas(&h).await.unwrap();
        assert_eq!(schemas.first().map(String::as_str), Some("STROKE"), "{schemas:?}");

        let tables = list_tables(&h, "STROKE").await.unwrap();
        let t = tables.iter().find(|t| t.name == "STROKE_TYPES").expect("table listed");
        assert_eq!(t.kind, "table");
        assert_eq!(t.row_count, -1, "small tables get an exact count later");
        assert_eq!(exact_row_count(&h, "STROKE", "STROKE_TYPES").await.unwrap(), 30);

        let cols = get_column_structure(&h, "STROKE", "STROKE_TYPES").await.unwrap();
        let col = |n: &str| cols.iter().find(|c| c.name == n).unwrap_or_else(|| panic!("{n}: {cols:?}"));
        assert_eq!(col("ID").data_type, "NUMBER(9)");
        assert!(col("ID").is_primary_key);
        assert_eq!(col("NAME").data_type, "VARCHAR2(50 CHAR)");
        assert!(!col("NAME").is_nullable);
        assert_eq!(col("NAME").comment.as_deref(), Some("display name"));
        assert_eq!(col("PRICE").data_type, "NUMBER(10,2)");
        assert_eq!(col("SEEN_TZ").data_type, "TIMESTAMP(6) WITH TIME ZONE");
        assert_eq!(col("CREATED").column_default.as_deref(), Some("SYSDATE"));
        let child = get_column_structure(&h, "STROKE", "STROKE_CHILD").await.unwrap();
        assert_eq!(child[1].foreign_key.as_deref(), Some("STROKE.STROKE_TYPES.ID"), "{child:?}");
        let inc = incoming_foreign_keys(&h, "STROKE", "STROKE_TYPES").await.unwrap();
        assert_eq!(inc.len(), 1);
        assert_eq!(inc[0].from_table, "STROKE_CHILD");
        let all = schema_column_structure(&h, "STROKE").await.unwrap();
        assert!(all.iter().any(|t| t.table == "STROKE_TYPES" && t.columns.len() == 12));
        let idx = list_indexes(&h, "STROKE").await.unwrap();
        assert!(idx.iter().any(|i| i.table_name == "STROKE_TYPES" && i.is_primary && i.columns == "ID"), "{idx:?}");
        let ddl = get_ddl(&h, "STROKE", "STROKE_TYPES").await.unwrap();
        assert!(ddl.contains("CREATE TABLE") && ddl.contains("\"STROKE_TYPES\""), "{ddl}");

        // Page 2 of 10, sorted by price descending.
        let page = get_table_rows(&h, "STROKE", "STROKE_TYPES", 10, 10, None, Some("PRICE".into()), Some("desc".into()), None, true)
            .await
            .unwrap();
        assert_eq!(page.total, 30);
        assert_eq!(page.rows.len(), 10);
        assert_eq!(page.primary_key, vec!["ID".to_string()]);
        let names: Vec<&str> = page.columns.iter().map(|c| c.name.as_str()).collect();
        let at = |n: &str| names.iter().position(|c| *c == n).unwrap();
        let first = &page.rows[0];
        assert_eq!(first[at("ID")], Value::from(20));
        assert_eq!(first[at("PRICE")], serde_json::json!(20.5));
        assert_eq!(first[at("RATIO")], serde_json::json!(5.0));
        assert_eq!(first[at("NNAME")], Value::from("ñ20"));
        assert_eq!(first[at("BORN")], Value::from("2024-01-22 00:00:00"));
        assert_eq!(first[at("SEEN")], Value::from("2024-01-02 03:04:05.123456"));
        assert_eq!(first[at("SEEN_TZ")], Value::from("2024-01-02 03:04:05 +05:30"));
        assert_eq!(first[at("NOTE")], Value::from("clob 20"));
        assert_eq!(first[at("DOC")], Value::from("<blob: 4 bytes>"));
        assert_eq!(first[at("TAG")], Value::from("0A0B0C0D"));
        assert_eq!(page.columns[at("BORN")].data_type, "DATETIME");
        assert!(!page.columns[at("NAME")].nullable);
        assert!(page.columns[at("CREATED")].has_default);

        // Search + filters, case-insensitively.
        let f = |c: &str, op: &str, v: &str| RowFilter { column: c.into(), op: op.into(), value: Some(v.into()), conjunct: None, data_type: None };
        let r = get_table_rows(&h, "STROKE", "STROKE_TYPES", 50, 0, Some("NAME 1".into()), None, None, None, false).await.unwrap();
        assert_eq!(r.total, 11, "name 1, name 10..19");
        let r = get_table_rows(&h, "STROKE", "STROKE_TYPES", 50, 0, None, None, None, Some(vec![f("PRICE", "gte", "25.5"), f("NOTE", "contains", "CLOB 2")]), false)
            .await
            .unwrap();
        assert_eq!(r.total, 5, "25..29");
        let r = get_table_rows(&h, "STROKE", "STROKE_TYPES", 50, 0, None, None, None, Some(vec![f("BORN", "between", "2024-01-05,2024-01-07")]), false)
            .await
            .unwrap();
        assert_eq!(r.total, 3);
        let r = get_table_rows(&h, "STROKE", "STROKE_TYPES", 50, 0, None, None, None, Some(vec![f("TAG", "eq", "0a0b0c0d")]), false).await;
        assert_eq!(r.unwrap().total, 30);
        let r = get_table_rows(&h, "STROKE", "STROKE_TYPES", 50, 0, None, None, None, Some(vec![f("", "sql", "ID < 4 AND name LIKE 'name%'")]), false)
            .await
            .unwrap();
        assert_eq!(r.total, 3);

        // Edits, with values in the formats the grid shows.
        let pk = |id: i64| HashMap::from([("ID".to_string(), Value::from(id))]);
        update_table_cell(&h, "STROKE", "STROKE_TYPES", pk(3), "NAME", &Value::from("it's three")).await.unwrap();
        update_table_cell(&h, "STROKE", "STROKE_TYPES", pk(3), "PRICE", &serde_json::json!(0.1)).await.unwrap();
        update_table_cell(&h, "STROKE", "STROKE_TYPES", pk(3), "BORN", &Value::from("1999-12-31 23:59:58")).await.unwrap();
        update_table_cell(&h, "STROKE", "STROKE_TYPES", pk(3), "SEEN", &Value::from("2001-02-03 04:05:06.5")).await.unwrap();
        update_table_cell(&h, "STROKE", "STROKE_TYPES", pk(3), "SEEN_TZ", &Value::from("2001-02-03 04:05:06 -07:00")).await.unwrap();
        update_table_cell(&h, "STROKE", "STROKE_TYPES", pk(3), "TAG", &Value::from("FFEE")).await.unwrap();
        update_table_cell(&h, "STROKE", "STROKE_TYPES", pk(3), "NNAME", &Value::Null).await.unwrap();
        assert!(update_table_cell(&h, "STROKE", "STROKE_TYPES", pk(999), "NAME", &Value::from("x")).await.is_err());
        let row = exec(&h, "SELECT name, price, born, seen, seen_tz, tag, nname FROM stroke_types WHERE id = 3").await.rows.remove(0);
        assert_eq!(
            row,
            vec![
                Value::from("it's three"),
                serde_json::json!(0.1),
                Value::from("1999-12-31 23:59:58"),
                Value::from("2001-02-03 04:05:06.5"),
                Value::from("2001-02-03 04:05:06 -07:00"),
                Value::from("FFEE"),
                Value::Null
            ]
        );

        let inserted = insert_table_row(
            &h,
            "STROKE",
            "STROKE_TYPES",
            HashMap::from([("ID".to_string(), Value::from(100)), ("NAME".to_string(), Value::from("new")), ("PRICE".to_string(), Value::from("12.34"))]),
        )
        .await
        .unwrap();
        assert_eq!(inserted.len(), 12, "the whole stored row comes back");
        assert_eq!(inserted[0], Value::from(100));
        assert!(inserted[11].as_str().is_some_and(|s| s.len() == 19), "DEFAULT SYSDATE filled in: {:?}", inserted[11]);

        let deleted = delete_table_rows(&h, "STROKE", "STROKE_TYPES", vec![pk(100), pk(30), pk(29)]).await.unwrap();
        assert_eq!(deleted, 3);
        // A failing batch deletes nothing: row 1 is referenced by STROKE_CHILD.
        assert!(delete_table_rows(&h, "STROKE", "STROKE_TYPES", vec![pk(28), pk(1)]).await.is_err());
        assert_eq!(exact_row_count(&h, "STROKE", "STROKE_TYPES").await.unwrap(), 28);

        // Read only: queries run, writes are refused and leave nothing behind.
        let r = execute_sql_readonly(&h, "SELECT COUNT(*) AS n FROM stroke_types").await.unwrap();
        assert_eq!(r.rows, vec![vec![Value::from(28)]]);
        let e = execute_sql_readonly(&h, "DELETE FROM stroke_types").await.unwrap_err();
        assert!(e.contains("ORA-01456"), "{e}");
        assert_eq!(exact_row_count(&h, "STROKE", "STROKE_TYPES").await.unwrap(), 28);

        // Messages, the explain plan and an error the UI can show.
        assert_eq!(exec(&h, "UPDATE stroke_types SET ratio = 1 WHERE id < 5;").await.message.as_deref(), Some("4 row(s) affected"));
        assert_eq!(exec(&h, "begin null; end;").await.message.as_deref(), Some("PL/SQL procedure successfully completed"));
        let plan = explain_lines(&h, "SELECT * FROM stroke_types WHERE id = 5").await.unwrap();
        assert!(plan.iter().any(|l| l.contains("INDEX UNIQUE SCAN") || l.contains("TABLE ACCESS")), "{plan:?}");
        let e = execute_sql(&h, "SELECT * FROM no_such_table").await.unwrap_err();
        assert!(e.starts_with("Oracle error: ORA-00942"), "{e}");

        // A view (no ROWID to page by) and a table without a key (ROWID order, read-only grid).
        exec(&h, "CREATE OR REPLACE VIEW stroke_v AS SELECT id, name FROM stroke_types").await;
        let _ = execute_sql(&h, "DROP TABLE stroke_nokey PURGE").await;
        exec(&h, "CREATE TABLE stroke_nokey (a NUMBER)").await;
        exec(&h, "INSERT INTO stroke_nokey SELECT LEVEL FROM dual CONNECT BY LEVEL <= 3").await;
        let tables = list_tables(&h, "").await.unwrap();
        assert!(tables.iter().any(|t| t.name == "STROKE_V" && t.kind == "view" && t.row_count == 0), "{tables:?}");
        let v = get_table_rows(&h, "STROKE", "STROKE_V", 5, 0, None, None, None, None, true).await.unwrap();
        assert_eq!((v.total, v.rows.len(), v.primary_key.len()), (28, 5, 0));
        let k = get_table_rows(&h, "STROKE", "STROKE_NOKEY", 2, 1, None, None, None, None, true).await.unwrap();
        assert!(k.sql.contains("ORDER BY ROWID"), "{}", k.sql);
        assert_eq!(k.rows, vec![vec![Value::from(2)], vec![Value::from(3)]]);
        assert!(update_table_cell(&h, "STROKE", "STROKE_NOKEY", HashMap::new(), "A", &Value::from(9)).await.is_err());

        exec(&h, "DROP VIEW stroke_v").await;
        exec(&h, "DROP TABLE stroke_nokey PURGE").await;
        exec(&h, "DROP TABLE stroke_child PURGE").await;
        exec(&h, "DROP TABLE stroke_types PURGE").await;
    }

    #[tokio::test]
    #[ignore]
    async fn live_cancel_interrupts_the_call() {
        let Some(h) = live().await else { return };
        let slow = "SELECT COUNT(*) FROM all_objects a, all_objects b, all_objects c";
        let run = {
            let h = h.clone();
            tokio::spawn(async move { execute_sql(&h, slow).await })
        };
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        let t0 = Instant::now();
        cancel(&h).await;
        let r = run.await.unwrap();
        assert_eq!(r.unwrap_err(), super::super::query::QUERY_CANCELLED);
        assert!(t0.elapsed() < std::time::Duration::from_secs(10));
        // The session is still usable.
        ping(&h).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn live_connect_forms_and_errors() {
        // The other ways to say where: fields, and a full descriptor.
        let Some(mut cfg) = live_config() else { return };
        cfg.connect_string = String::new();
        cfg.host = "127.0.0.1".into();
        cfg.service_name = "FREEPDB1".into();
        ping(&crate::db::connection::open_oracle(&cfg).await.expect("fields")).await.unwrap();
        cfg.connect_string = "(DESCRIPTION=(ADDRESS=(PROTOCOL=TCP)(HOST=127.0.0.1)(PORT=1521))(CONNECT_DATA=(SERVICE_NAME=FREEPDB1)))".into();
        ping(&crate::db::connection::open_oracle(&cfg).await.expect("descriptor")).await.unwrap();
        cfg.connect_string = String::new();
        cfg.sid = "NO_SUCH_SID".into();
        let e = crate::db::connection::open_oracle(&cfg).await.err().expect("must fail");
        assert!(e.contains("ORA-12505"), "{e}");

        let Some(mut cfg) = live_config() else { return };
        cfg.connect_string = "//127.0.0.1:1521/NO_SUCH_SERVICE".into();
        let e = crate::db::connection::open_oracle(&cfg).await.err().expect("must fail");
        assert!(e.contains("ORA-12514") && e.contains("service name"), "{e}");
        let Some(mut cfg) = live_config() else { return };
        cfg.password = "wrong".into();
        let e = crate::db::connection::open_oracle(&cfg).await.err().expect("must fail");
        assert!(e.contains("ORA-01017"), "{e}");
    }

    /// Run alone, with no Instant Client anywhere the app looks (no
    /// STROKE_ORACLE_CLIENT_DIR, no LD_LIBRARY_PATH entry):
    ///   STROKE_ORACLE_EXPECT_NO_CLIENT=1 cargo test --lib live_missing_client -- --ignored
    #[tokio::test]
    #[ignore]
    async fn live_missing_client_is_explained() {
        if std::env::var("STROKE_ORACLE_EXPECT_NO_CLIENT").is_err() {
            return;
        }
        let e = crate::db::connection::test_oracle_connection(cfg()).await.unwrap_err();
        assert!(e.starts_with("Oracle Instant Client is required"), "{e}");
        let mut c = cfg();
        c.client_lib_dir = "/nonexistent/instantclient".into();
        let e = crate::db::connection::test_oracle_connection(c).await.unwrap_err();
        assert!(e.contains("/nonexistent/instantclient"), "{e}");
    }
}
