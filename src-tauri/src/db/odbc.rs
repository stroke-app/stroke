//! The ODBC bridge: any database with an ODBC driver installed. Beta.
//!
//! One driver for many engines (Db2, SAP HANA, Teradata, Vertica, Exasol,
//! Firebird, Redshift, Databricks, Hive, Trino, Dremio, IRIS, Access, ...).
//! The app ships the driver manager; the database's own ODBC driver does the
//! talking. On Windows that manager is the system's `odbc32`. Elsewhere it is
//! unixODBC, built into the app, so nothing has to be installed for the app
//! to start, only the driver for the database you connect to.
//!
//! The bundled unixODBC reads its driver list from a merged copy of the
//! system's: `/etc/odbcinst.ini`, Homebrew's, and macOS's `/Library/ODBC`
//! (where iODBC-registered drivers live), with bare library names resolved to
//! full paths. See [`registry`].
//!
//! Everything runs through the ODBC catalog functions (SQLTables, SQLColumns,
//! SQLPrimaryKeys, SQLForeignKeys), so browsing works the same on every
//! driver. Paging adapts to the database: LIMIT/OFFSET where it is spoken,
//! OFFSET/FETCH, SKIP/FIRST or TOP elsewhere, and skipping rows on this side
//! when nothing else is known to work.
//!
//! ODBC calls block, so each one runs on a blocking thread holding the
//! connection. Stop calls SQLCancel on the running statement from another
//! thread, which the ODBC spec allows for exactly this.

use super::query::{ColumnInfo, ForeignKeyInfo, RowFilter, SqlResult, TableRows};
use super::schema::{ColumnStructureRow, TableInfo};
use odbc_api::handles::{AnyHandle, AsStatementRef};
use odbc_api::{parameter::InputParameter, Connection, ConnectionOptions, Cursor, DataType, IntoParameter, ResultSetMetadata};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OdbcConfig {
    pub name: String,
    /// The engine this connection is for (`db2`, `hana`, ... or `generic`).
    /// Only labels and hints depend on it; the driver decides the rest.
    #[serde(default)]
    pub preset: Option<String>,
    /// A data source name from odbc.ini / the ODBC Data Source Administrator.
    #[serde(default)]
    pub dsn: Option<String>,
    /// A whole connection string (`Driver={...};Server=...;`), when no DSN.
    #[serde(default)]
    pub connection_string: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    /// An SSH tunnel to the database's host and port. Only a preset's fields
    /// say where that is (`target_host`, `target_port`); the string is
    /// pointed at the tunnel's local end.
    #[serde(default)]
    pub ssh: Option<super::ssh_tunnel::SshConfig>,
    #[serde(default)]
    pub target_host: Option<String>,
    #[serde(default)]
    pub target_port: Option<u16>,
}

/// How the database names what the app calls a schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Namespace {
    /// `schema.table` (Db2, HANA, Postgres, Oracle, SQL Server, Trino, ...).
    Schema,
    /// `catalog.table`, no schemas (MySQL and MariaDB call databases catalogs).
    Catalog,
    /// Bare table names (SQLite, Access, a CSV driver).
    Flat,
}

/// How a page of rows is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paging {
    LimitOffset,
    /// Trino and Presto: `OFFSET n LIMIT m`.
    OffsetLimit,
    /// `OFFSET n ROWS FETCH NEXT m ROWS ONLY` (Db2, Oracle, Firebird 3+).
    OffsetFetch,
    /// SQL Server and Sybase IQ want an ORDER BY with OFFSET/FETCH.
    OffsetFetchOrdered,
    /// Informix: `SELECT SKIP n FIRST m`.
    SkipFirst,
    /// `SELECT TOP n+m`, then skip n here (Teradata, SAP ASE, Access, IRIS).
    Top,
    /// No clause: read and skip here.
    Client,
}

#[derive(Debug, Clone)]
pub struct OdbcInfo {
    /// What SQL_DBMS_NAME says (`DB2/LINUXX8664`, `HDB`, `PostgreSQL`, ...).
    pub dbms: String,
    pub quote: Option<char>,
    pub namespace: Namespace,
    /// The connection's current catalog, for catalog calls in Schema mode.
    pub catalog: String,
    pub paging: Paging,
}

/// What the window needs to write SQL for this connection.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OdbcSummary {
    pub dbms: String,
    /// The identifier quote: `"`, `` ` `` or `[`. None when the driver has none.
    pub quote: Option<String>,
    /// "schema", "catalog" or "flat".
    pub namespace: &'static str,
}

impl OdbcInfo {
    pub fn summary(&self) -> OdbcSummary {
        OdbcSummary {
            dbms: self.dbms.clone(),
            quote: self.quote.map(String::from),
            namespace: match self.namespace {
                Namespace::Schema => "schema",
                Namespace::Catalog => "catalog",
                Namespace::Flat => "flat",
            },
        }
    }
}

pub struct OdbcConn {
    pub info: OdbcInfo,
    /// The connection string, to open a transaction's own connection beside
    /// this one. Held in memory only.
    cs: String,
    conn: Mutex<Connection<'static>>,
}

pub type OdbcHandle = Arc<OdbcConn>;

/// What the schema list shows for a database without schemas.
pub const FLAT_SCHEMA: &str = "default";

// ── Driver registry (unixODBC) ────────────────────────────────────────────────

/// The merged driver and data source list the bundled unixODBC reads.
///
/// unixODBC resolves a driver registered by a bare file name (`Driver =
/// psqlodbcw.so`, as Debian and Ubuntu packages do) against its own build's
/// library directory, which for a copy built into this app is nowhere. So the
/// app reads every registry it knows of, writes the paths out in full, and
/// points ODBCSYSINI at the result. A user's own ODBCSYSINI is read first.
#[cfg(not(windows))]
pub mod registry {
    use std::path::{Path, PathBuf};

    fn home() -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }

    /// Where registries live, most specific first. The user's own data
    /// sources (`~/.odbc.ini`, or ODBCINI) come before the system's: unixODBC
    /// built into the app does not read them itself, so they ride in the
    /// merged file.
    fn sources(file: &str, user_sysini: Option<&Path>) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if file == "odbc.ini" {
            if let Some(p) = std::env::var_os("ODBCINI") {
                out.push(PathBuf::from(p));
            }
            if let Some(h) = home() {
                out.push(h.join(".odbc.ini"));
            }
        }
        if let Some(dir) = user_sysini {
            out.push(dir.join(file));
        }
        for dir in ["/etc", "/etc/unixODBC", "/usr/local/etc", "/opt/homebrew/etc", "/Library/ODBC"] {
            out.push(Path::new(dir).join(file));
        }
        if let Some(h) = home() {
            out.push(h.join("Library/ODBC").join(file));
            if file == "odbcinst.ini" {
                out.push(h.join(".odbcinst.ini"));
            }
        }
        out
    }

    /// Directories a bare driver file name may be in.
    fn lib_dirs() -> Vec<PathBuf> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir("/usr/lib") {
            for e in rd.flatten() {
                let p = e.path();
                if p.join("odbc").is_dir() {
                    out.push(p.join("odbc"));
                }
                if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.contains("-linux-gnu")) {
                    out.push(p);
                }
            }
        }
        for d in ["/usr/lib/odbc", "/usr/lib64", "/usr/lib", "/usr/local/lib", "/opt/homebrew/lib", "/usr/lib64/odbc"] {
            out.push(PathBuf::from(d));
        }
        out
    }

    /// `[section]` → ordered (key, value) pairs.
    pub fn parse(text: &str) -> Vec<(String, Vec<(String, String)>)> {
        let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                out.push((name.trim().to_string(), Vec::new()));
            } else if let (Some((k, v)), Some(last)) = (line.split_once('='), out.last_mut()) {
                last.1.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
        out
    }

    /// Merge registries: the first file to define a section wins it.
    pub fn merge(files: &[String], resolve: &dyn Fn(&str) -> Option<String>) -> String {
        let mut seen = std::collections::HashSet::new();
        let mut out = String::new();
        for text in files {
            for (name, keys) in parse(text) {
                if !seen.insert(name.to_ascii_lowercase()) {
                    continue;
                }
                out.push_str(&format!("[{name}]\n"));
                for (k, v) in keys {
                    let v = if matches!(k.to_ascii_lowercase().as_str(), "driver" | "setup" | "driver64" | "setup64")
                        && !v.contains('/')
                        && !v.is_empty()
                    {
                        resolve(&v).unwrap_or(v)
                    } else {
                        v
                    };
                    out.push_str(&format!("{k} = {v}\n"));
                }
                out.push('\n');
            }
        }
        out
    }

    fn resolve_lib(name: &str) -> Option<String> {
        lib_dirs().into_iter().map(|d| d.join(name)).find(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned())
    }

    /// Write the merged registry and point the driver manager at it. Runs
    /// before every driver list, data source list and connect, so a driver
    /// installed while the app is open shows up without a restart: unixODBC
    /// reads the files again when they change.
    pub fn install() {
        // The user's own ODBCSYSINI, as it was before this app replaced it.
        static USER_SYSINI: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
        let user_sysini = USER_SYSINI.get_or_init(|| std::env::var_os("ODBCSYSINI").map(PathBuf::from)).clone();
        let read = |file: &str| -> Vec<String> {
            sources(file, user_sysini.as_deref()).iter().filter_map(|p| std::fs::read_to_string(p).ok()).collect()
        };
        let drivers = merge(&read("odbcinst.ini"), &resolve_lib);
        let dsns = merge(&read("odbc.ini"), &resolve_lib);
        let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
        let dir = std::env::temp_dir().join(format!("stroke-odbc-{user}"));
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        {
            use std::os::unix::fs::PermissionsExt;
            // System DSNs can carry passwords: this copy is the user's alone.
            let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        }
        let write = |name: &str, body: &str| -> bool {
            let path = dir.join(name);
            let ok = std::fs::write(&path, body).is_ok();
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }
            ok
        };
        if write("odbcinst.ini", &drivers) && write("odbc.ini", &dsns) && std::env::var_os("ODBCSYSINI").as_deref() != Some(dir.as_os_str()) {
            // Set once, before the first ODBC call; nothing else in the app
            // reads this variable.
            std::env::set_var("ODBCSYSINI", &dir);
        }
    }
}

fn environment() -> Result<&'static odbc_api::Environment, String> {
    // Serialised: two refreshes at once would write the same files together.
    static REFRESH: Mutex<()> = Mutex::new(());
    {
        let _g = REFRESH.lock().unwrap_or_else(|e| e.into_inner());
        #[cfg(not(windows))]
        registry::install();
    }
    odbc_api::environment().map_err(|e| format!("The ODBC driver manager could not start: {e}"))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OdbcDriver {
    pub name: String,
    /// The driver's library, where the registry says.
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OdbcDataSource {
    pub name: String,
    pub driver: String,
}

/// Installed ODBC drivers.
pub async fn drivers() -> Result<Vec<OdbcDriver>, String> {
    tokio::task::spawn_blocking(|| {
        let env = environment()?;
        let mut out: Vec<OdbcDriver> = env
            .drivers()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|d| OdbcDriver { path: d.attributes.get("Driver").cloned(), name: d.description })
            .collect();
        out.sort_by_key(|d| d.name.to_ascii_lowercase());
        out.dedup_by(|a, b| a.name == b.name);
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Configured data sources (DSNs), user and system.
pub async fn data_sources() -> Result<Vec<OdbcDataSource>, String> {
    tokio::task::spawn_blocking(|| {
        let env = environment()?;
        let mut out: Vec<OdbcDataSource> = env
            .data_sources()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|d| OdbcDataSource { name: d.server_name, driver: d.driver })
            .collect();
        out.sort_by_key(|d| d.name.to_ascii_lowercase());
        out.dedup_by(|a, b| a.name == b.name);
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Connecting ────────────────────────────────────────────────────────────────

/// A value as an ODBC connection string writes it: braced when it holds
/// anything the string's own syntax uses.
pub fn attr_value(v: &str) -> String {
    if v.contains([';', '{', '}', '=']) || v.starts_with(' ') || v.ends_with(' ') {
        format!("{{{}}}", v.replace('}', "}}"))
    } else {
        v.to_string()
    }
}

/// Is `key` already set in the string (case-insensitive)?
fn has_key(s: &str, keys: &[&str]) -> bool {
    s.split(';').filter_map(|part| part.split_once('=')).any(|(k, _)| keys.iter().any(|w| k.trim().eq_ignore_ascii_case(w)))
}

fn trimmed(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// The connection string handed to the driver manager.
pub fn connection_string(cfg: &OdbcConfig) -> Result<String, String> {
    let mut s = match (trimmed(&cfg.dsn), trimmed(&cfg.connection_string)) {
        (Some(dsn), _) => format!("DSN={};", attr_value(dsn)),
        (None, Some(cs)) => cs.to_string(),
        (None, None) => return Err("Choose a data source (DSN) or enter a connection string".into()),
    };
    if !s.ends_with(';') {
        s.push(';');
    }
    if let Some(u) = trimmed(&cfg.user) {
        if !has_key(&s, &["UID", "User", "Username"]) {
            s.push_str(&format!("UID={};", attr_value(u)));
        }
    }
    if let Some(p) = cfg.password.as_deref().filter(|p| !p.is_empty()) {
        if !has_key(&s, &["PWD", "Password"]) {
            s.push_str(&format!("PWD={};", attr_value(p)));
        }
    }
    Ok(s)
}

/// Point a connection string's host and port at a tunnel's local end:
/// `host:port` and `host/port` inside a value, and values that are exactly
/// the host or the port. Values are compared, never substrings of other words.
pub fn retarget(cs: &str, host: &str, port: u16, local_port: u16) -> String {
    let (host, port_s, local) = (host.trim(), port.to_string(), local_port.to_string());
    let mut out = Vec::new();
    // Split on ';' outside braces.
    let mut part = String::new();
    let mut depth = 0;
    let mut parts = Vec::new();
    for c in cs.chars() {
        match c {
            '{' => { depth += 1; part.push(c) }
            '}' => { depth -= 1; part.push(c) }
            ';' if depth <= 0 => parts.push(std::mem::take(&mut part)),
            c => part.push(c),
        }
    }
    if !part.is_empty() {
        parts.push(part);
    }
    for p in parts {
        let Some((k, v)) = p.split_once('=') else {
            out.push(p);
            continue;
        };
        let v = if v == host {
            "127.0.0.1".to_string()
        } else if v == port_s {
            local.clone()
        } else {
            v.replace(&format!("{host}:{port_s}"), &format!("127.0.0.1:{local}")).replace(&format!("{host}/{port_s}"), &format!("127.0.0.1/{local}"))
        };
        out.push(format!("{k}={v}"));
    }
    let mut s = out.join(";");
    if cs.ends_with(';') {
        s.push(';');
    }
    s
}

/// Driver errors arrive as diagnostics records; keep the message, drop the
/// state codes unless that is all there is.
fn odbc_err(e: odbc_api::Error) -> String {
    let text = e.to_string();
    let msgs: Vec<&str> = text.split("Message: ").skip(1).map(|m| m.split(", function:").next().unwrap_or(m).trim()).collect();
    if msgs.is_empty() { text } else { msgs.join(" ") }
}

fn paging_for(dbms: &str) -> Paging {
    let d = dbms.to_ascii_lowercase();
    let has = |w: &str| d.contains(w);
    if has("trino") || has("presto") {
        Paging::OffsetLimit
    } else if has("microsoft sql server") || has("sybase iq") || has("adaptive server iq") {
        Paging::OffsetFetchOrdered
    } else if has("db2") || has("oracle") || has("firebird") || has("interbase") || has("derby") {
        Paging::OffsetFetch
    } else if has("informix") || d.starts_with("ids") {
        Paging::SkipFirst
    } else if has("teradata") || has("adaptive server") || has("sybase") || has("access") || has("iris") || has("cache") {
        Paging::Top
    } else if has("postgres") || has("mysql") || has("mariadb") || has("sqlite") || has("vertica") || has("exasol") || has("hdb") || has("hana")
        || has("redshift") || has("snowflake") || has("bigquery") || has("google") || has("clickhouse") || has("duckdb") || has("dremio")
        || has("spark") || has("databricks") || has("impala") || has("greenplum") || has("cockroach")
    {
        Paging::LimitOffset
    } else {
        Paging::Client
    }
}

pub async fn connect(cfg: &OdbcConfig) -> Result<OdbcHandle, String> {
    connect_string(connection_string(cfg)?).await
}

async fn connect_string(cs: String) -> Result<OdbcHandle, String> {
    tokio::task::spawn_blocking(move || {
        let env = environment()?;
        let options = ConnectionOptions { login_timeout_sec: Some(20), ..Default::default() };
        let conn = env.connect_with_connection_string(&cs, options).map_err(odbc_err)?;
        let dbms = conn.database_management_system_name().unwrap_or_default();
        let quote = conn.identifier_quote_char().ok().flatten().filter(|c| !c.is_whitespace());
        let schema_len = conn.max_schema_name_len().unwrap_or(0);
        let catalog_len = conn.max_catalog_name_len().unwrap_or(0);
        let catalog = if catalog_len > 0 { conn.current_catalog().unwrap_or_default() } else { String::new() };
        let guess = if schema_len > 0 {
            Namespace::Schema
        } else if catalog_len > 0 {
            Namespace::Catalog
        } else {
            Namespace::Flat
        };
        let mut info = OdbcInfo { paging: paging_for(&dbms), dbms, quote, namespace: guess, catalog };
        // What the driver says it supports and what its catalog holds can
        // differ (SQLite's driver reports schemas and has none): ask.
        if guess == Namespace::Schema && schema_names(&conn, &info).is_empty() {
            info.namespace = if catalog_len > 0 { Namespace::Catalog } else { Namespace::Flat };
        }
        // The same for catalogs: SQLite's driver reports them and has none.
        if info.namespace == Namespace::Catalog && catalog_names(&conn).is_empty() {
            info.namespace = Namespace::Flat;
        }
        Ok(Arc::new(OdbcConn { info, cs, conn: Mutex::new(conn) }))
    })
    .await
    .map_err(|e| format!("ODBC task failed: {e}"))?
}

/// Run `f` on a blocking thread with the connection.
async fn run<T, F>(h: &OdbcHandle, f: F) -> Result<T, String>
where
    F: FnOnce(&Connection<'static>, &OdbcInfo) -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    let h = h.clone();
    tokio::task::spawn_blocking(move || {
        let conn = h.conn.lock().map_err(|_| "ODBC connection lock poisoned".to_string())?;
        f(&conn, &h.info)
    })
    .await
    .map_err(|e| format!("ODBC task failed: {e}"))?
}

/// A connection of its own for an explicit transaction, autocommit off: on
/// the shared one, every grid page and sidebar query would run inside it.
pub async fn open_tx(h: &OdbcHandle) -> Result<OdbcHandle, String> {
    let tx = connect_string(h.cs.clone()).await?;
    run(&tx, |conn, _| conn.set_autocommit(false).map_err(odbc_err)).await?;
    Ok(tx)
}

pub async fn commit(h: &OdbcHandle) -> Result<(), String> {
    run(h, |conn, _| conn.commit().map_err(odbc_err)).await
}

pub async fn rollback(h: &OdbcHandle) -> Result<(), String> {
    run(h, |conn, _| conn.rollback().map_err(odbc_err)).await
}

pub async fn ping(h: &OdbcHandle) -> Result<(), String> {
    run(h, |conn, _| match conn.is_dead() {
        Ok(true) => Err("The connection to the database was lost".into()),
        _ => Ok(()),
    })
    .await
}

// ── Values ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Int,
    Float,
    Bool,
    Binary,
    Text,
}

fn kind_of(dt: &DataType) -> Kind {
    match dt {
        DataType::Integer | DataType::SmallInt | DataType::BigInt | DataType::TinyInt => Kind::Int,
        DataType::Real | DataType::Double | DataType::Float { .. } => Kind::Float,
        DataType::Bit => Kind::Bool,
        DataType::Binary { .. } | DataType::Varbinary { .. } | DataType::LongVarbinary { .. } => Kind::Binary,
        DataType::Other { data_type, .. } if data_type.0 == 16 => Kind::Bool,
        _ => Kind::Text,
    }
}

/// A result column's type as a label: `INTEGER`, `VARCHAR(40)`, `DECIMAL(10,2)`.
fn type_label(dt: &DataType) -> String {
    let len = |l: &Option<std::num::NonZeroUsize>, base: &str| match l {
        Some(n) if n.get() < 1 << 30 => format!("{base}({n})"),
        _ => base.to_string(),
    };
    match dt {
        DataType::Unknown => "UNKNOWN".into(),
        DataType::Char { length } => len(length, "CHAR"),
        DataType::WChar { length } => len(length, "NCHAR"),
        DataType::Varchar { length } => len(length, "VARCHAR"),
        DataType::WVarchar { length } => len(length, "NVARCHAR"),
        DataType::LongVarchar { .. } => "TEXT".into(),
        DataType::WLongVarchar { .. } => "NTEXT".into(),
        DataType::Numeric { precision, scale } => format!("NUMERIC({precision},{scale})"),
        DataType::Decimal { precision, scale } => format!("DECIMAL({precision},{scale})"),
        DataType::Integer => "INTEGER".into(),
        DataType::SmallInt => "SMALLINT".into(),
        DataType::TinyInt => "TINYINT".into(),
        DataType::BigInt => "BIGINT".into(),
        DataType::Float { .. } => "FLOAT".into(),
        DataType::Real => "REAL".into(),
        DataType::Double => "DOUBLE".into(),
        DataType::Date => "DATE".into(),
        DataType::Time { .. } => "TIME".into(),
        DataType::Timestamp { .. } => "TIMESTAMP".into(),
        DataType::Bit => "BIT".into(),
        DataType::Binary { length } => len(length, "BINARY"),
        DataType::Varbinary { length } => len(length, "VARBINARY"),
        DataType::LongVarbinary { .. } => "BLOB".into(),
        DataType::Other { data_type, .. } => match data_type.0 {
            16 => "BOOLEAN".into(),
            -11 => "UUID".into(),
            n => format!("TYPE {n}"),
        },
    }
}

/// One cell as text into `buf`, or None for NULL.
fn text_cell(row: &mut odbc_api::CursorRow<'_>, col: u16, buf: &mut Vec<u8>, wbuf: &mut Vec<u16>) -> Result<Option<String>, String> {
    if cfg!(windows) {
        // The narrow API speaks the system code page there, not UTF-8.
        let some = row.get_wide_text(col, wbuf).map_err(odbc_err)?;
        Ok(some.then(|| String::from_utf16_lossy(wbuf)))
    } else {
        let some = row.get_text(col, buf).map_err(odbc_err)?;
        Ok(some.then(|| String::from_utf8_lossy(buf).into_owned()))
    }
}

fn cell(kind: Kind, row: &mut odbc_api::CursorRow<'_>, col: u16, buf: &mut Vec<u8>, wbuf: &mut Vec<u16>) -> Result<Value, String> {
    if kind == Kind::Binary {
        let some = row.get_binary(col, buf).map_err(odbc_err)?;
        if !some {
            return Ok(Value::Null);
        }
        let mut hex = String::with_capacity(2 + buf.len() * 2);
        hex.push_str("0x");
        for b in buf.iter() {
            hex.push_str(&format!("{b:02x}"));
        }
        return Ok(super::sql_util::cap_json_value("text", Value::String(hex)));
    }
    let Some(s) = text_cell(row, col, buf, wbuf)? else { return Ok(Value::Null) };
    Ok(match kind {
        Kind::Int => match s.trim().parse::<i64>() {
            Ok(n) if n.unsigned_abs() <= (1u64 << 53) => json!(n),
            _ => Value::String(s),
        },
        Kind::Float => match s.trim().parse::<f64>() {
            Ok(x) if x.is_finite() => json!(x),
            _ => Value::String(s),
        },
        Kind::Bool => match s.trim().to_ascii_lowercase().as_str() {
            "1" | "t" | "true" | "y" | "yes" => json!(true),
            "0" | "f" | "false" | "n" | "no" => json!(false),
            _ => Value::String(s),
        },
        _ => super::sql_util::cap_json_value("text", Value::String(s)),
    })
}

/// Columns of a cursor, with how to read each.
fn describe(cursor: &mut impl ResultSetMetadata) -> Result<(Vec<ColumnInfo>, Vec<Kind>), String> {
    let n = cursor.num_result_cols().map_err(odbc_err)?.max(0) as u16;
    let mut cols = Vec::with_capacity(n as usize);
    let mut kinds = Vec::with_capacity(n as usize);
    for i in 1..=n {
        let name = cursor.col_name(i).map_err(odbc_err)?;
        let dt = cursor.col_data_type(i).unwrap_or(DataType::Unknown);
        kinds.push(kind_of(&dt));
        cols.push(ColumnInfo::new(if name.is_empty() { format!("column{i}") } else { name }, type_label(&dt)));
    }
    Ok((cols, kinds))
}

/// Read a cursor's rows into `on_row` until it returns false.
fn read_rows(cursor: &mut impl Cursor, kinds: &[Kind], mut on_row: impl FnMut(Vec<Value>) -> Result<bool, String>) -> Result<(), String> {
    let mut buf = Vec::new();
    let mut wbuf = Vec::new();
    while let Some(mut row) = cursor.next_row().map_err(odbc_err)? {
        let mut out = Vec::with_capacity(kinds.len());
        for (i, k) in kinds.iter().enumerate() {
            out.push(cell(*k, &mut row, i as u16 + 1, &mut buf, &mut wbuf)?);
        }
        if !on_row(out)? {
            break;
        }
    }
    Ok(())
}

/// Parameters for one statement: bound as typed values, never spliced.
#[derive(Default, Debug, Clone)]
struct Params(Vec<Param>);

#[derive(Debug, Clone)]
enum Param {
    Text(String),
    Int(i64),
    Float(f64),
}

impl Params {
    fn text(&mut self, s: impl Into<String>) -> &'static str {
        self.0.push(Param::Text(s.into()));
        "?"
    }
    fn boxed(&self) -> Vec<Box<dyn InputParameter>> {
        self.0
            .iter()
            .map(|p| -> Box<dyn InputParameter> {
                match p {
                    Param::Int(n) => Box::new(*n),
                    Param::Float(x) => Box::new(*x),
                    // Wide (UTF-16) text on Windows, where narrow means the code page.
                    #[cfg(windows)]
                    Param::Text(s) => Box::new(s.as_str().into_parameter()),
                    #[cfg(not(windows))]
                    Param::Text(s) => Box::new(odbc_api::Narrow(s.clone()).into_parameter()),
                }
            })
            .collect()
    }
}

/// The running statement's handle, for Stop. Cleared under the lock before
/// the statement is freed, so a late Stop never reaches a freed handle.
#[derive(Default)]
struct CancelSlot {
    stopped: bool,
    handle: Option<usize>,
}

type Slot = Arc<Mutex<CancelSlot>>;

fn cancel_now(slot: &Slot) {
    let mut g = slot.lock().unwrap_or_else(|e| e.into_inner());
    g.stopped = true;
    if let Some(h) = g.handle {
        // SAFETY: the handle is live while it is in the slot (see above), and
        // SQLCancel is the one call ODBC allows from another thread.
        unsafe {
            let _ = odbc_api::sys::SQLCancel(odbc_api::sys::HStmt(h as *mut std::ffi::c_void));
        }
    }
}

/// One statement, its rows (or affected count) through `on_cols`/`on_row`.
/// Returns affected rows for a statement without a result set.
fn execute_on(
    conn: &Connection<'static>,
    sql: &str,
    params: &Params,
    slot: Option<&Slot>,
    mut on_cols: impl FnMut(Vec<ColumnInfo>) -> Result<(), String>,
    on_row: impl FnMut(Vec<Value>) -> Result<bool, String>,
) -> Result<Option<usize>, String> {
    let mut stmt = conn.preallocate().map_err(odbc_err)?;
    if let Some(s) = slot {
        let mut g = s.lock().unwrap_or_else(|e| e.into_inner());
        if g.stopped {
            return Err(super::query::QUERY_CANCELLED.into());
        }
        g.handle = Some(stmt.as_stmt_ref().as_handle().0 as usize);
    }
    let boxed = params.boxed();
    // Ok(true): the statement returned rows.
    let out: Result<bool, String> = (|| {
        let cursor = stmt.execute(sql, &boxed[..]).map_err(odbc_err)?;
        match cursor {
            Some(mut cursor) => {
                let (cols, kinds) = describe(&mut cursor)?;
                on_cols(cols)?;
                read_rows(&mut cursor, &kinds, on_row)?;
                Ok(true)
            }
            None => Ok(false),
        }
    })();
    let affected = if matches!(out, Ok(false)) { stmt.row_count().ok().flatten() } else { None };
    if let Some(s) = slot {
        s.lock().unwrap_or_else(|e| e.into_inner()).handle = None;
    }
    out.map(|_| affected)
}

// ── Console ───────────────────────────────────────────────────────────────────

pub fn reads(stmt: &str) -> bool {
    matches!(
        super::sql_util::statement_head(stmt).as_str(),
        "select" | "with" | "values" | "show" | "describe" | "desc" | "explain"
    )
}

fn collect(conn: &Connection<'static>, sql: &str, params: &Params, slot: Option<&Slot>, t0: Instant) -> Result<SqlResult, String> {
    let mut columns = Vec::new();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut budget = super::query::CollectBudget::default();
    let mut capped = false;
    let affected = execute_on(conn, sql, params, slot, |c| {
        columns = c;
        Ok(())
    }, |r| {
        let fits = budget.take(&r);
        rows.push(r);
        capped = !fits;
        Ok(fits)
    })?;
    if columns.is_empty() {
        let n = affected.unwrap_or(0) as i64;
        return Ok(SqlResult {
            columns,
            rows: vec![],
            row_count: Some(n),
            message: Some(format!("{n} row{} affected", if n == 1 { "" } else { "s" })),
            query_ms: t0.elapsed().as_millis() as u64,
            sql: sql.to_string(),
        });
    }
    let message = capped.then(|| super::query::budget_message(rows.len()));
    Ok(SqlResult { columns, row_count: Some(rows.len() as i64), rows, message, query_ms: t0.elapsed().as_millis() as u64, sql: sql.to_string() })
}

pub async fn query(h: &OdbcHandle, stmt: &str) -> Result<SqlResult, String> {
    let sql = stmt.trim().trim_end_matches(';').trim().to_string();
    if sql.is_empty() {
        return Err("Query is empty".into());
    }
    let t0 = Instant::now();
    run(h, move |conn, _| collect(conn, &sql, &Params::default(), None, t0)).await
}

/// `query`, with Stop cancelling the statement on the server.
pub async fn query_cancellable(h: &OdbcHandle, stmt: &str, cancel: tokio::sync::oneshot::Receiver<()>) -> Result<SqlResult, String> {
    let sql = stmt.trim().trim_end_matches(';').trim().to_string();
    if sql.is_empty() {
        return Err("Query is empty".into());
    }
    let slot: Slot = Arc::default();
    let s = slot.clone();
    let arm = super::stop::Arm::new(cancel, move || cancel_now(&s));
    let t0 = Instant::now();
    let out = run(h, move |conn, _| collect(conn, &sql, &Params::default(), Some(&slot), t0)).await;
    let stopped = arm.disarm();
    match out {
        Err(_) if stopped => Err(super::query::QUERY_CANCELLED.into()),
        other => other,
    }
}

/// Stream a statement's rows into the console's sink as the driver fetches
/// them, through a small bounded channel; Stop cancels the statement.
pub async fn stream_into(
    h: &OdbcHandle,
    stmt: &str,
    sink: &mut super::query::RowSink,
    cancel: tokio::sync::oneshot::Receiver<()>,
) -> Result<SqlResult, String> {
    enum Msg {
        Columns(Vec<ColumnInfo>),
        Rows(Vec<Vec<Value>>),
    }
    const BATCH: usize = 1_000;
    let sql = stmt.trim().trim_end_matches(';').trim().to_string();
    let slot: Slot = Arc::default();
    let s = slot.clone();
    let arm = super::stop::Arm::new(cancel, move || cancel_now(&s));
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Msg>(8);
    let sql_run = sql.clone();
    let reader = run(h, move |conn, _| {
        let mut batch = Vec::with_capacity(BATCH);
        let txc = tx.clone();
        let affected = execute_on(
            conn,
            &sql_run,
            &Params::default(),
            Some(&slot),
            |c| {
                let _ = txc.blocking_send(Msg::Columns(c));
                Ok(())
            },
            |r| {
                batch.push(r);
                if batch.len() < BATCH {
                    return Ok(true);
                }
                Ok(tx.blocking_send(Msg::Rows(std::mem::replace(&mut batch, Vec::with_capacity(BATCH)))).is_ok())
            },
        )?;
        if !batch.is_empty() {
            let _ = tx.blocking_send(Msg::Rows(batch));
        }
        Ok(affected)
    });
    tokio::pin!(reader);
    let t0 = Instant::now();
    let mut columns: Option<Vec<ColumnInfo>> = None;
    let mut total = 0usize;
    let mut sink_err: Option<String> = None;
    let mut reader_out = None;
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Some(Msg::Columns(c)) => {
                    if let Err(e) = sink.start(c.clone()) { sink_err = Some(e); rx.close(); }
                    columns = Some(c);
                }
                Some(Msg::Rows(rows)) => {
                    if sink_err.is_some() { continue; }
                    for r in rows {
                        if let Err(e) = sink.push(r) { sink_err = Some(e); break; }
                        total += 1;
                    }
                    if sink_err.is_some() { rx.close(); }
                }
                None => break,
            },
            out = &mut reader, if reader_out.is_none() => reader_out = Some(out),
        }
    }
    let out = match reader_out {
        Some(o) => o,
        None => reader.await,
    };
    let stopped = arm.disarm();
    if let Some(e) = sink_err {
        return Err(e);
    }
    let affected = match out {
        Err(_) if stopped => return Err(super::query::QUERY_CANCELLED.into()),
        Err(e) => return Err(e),
        Ok(a) => a,
    };
    match columns {
        Some(columns) => {
            sink.finish()?;
            Ok(SqlResult { columns, rows: vec![], row_count: Some(total as i64), message: None, query_ms: t0.elapsed().as_millis() as u64, sql })
        }
        None => {
            let n = affected.unwrap_or(0) as i64;
            Ok(SqlResult {
                columns: vec![],
                rows: vec![],
                row_count: Some(n),
                message: Some(format!("{n} row{} affected", if n == 1 { "" } else { "s" })),
                query_ms: t0.elapsed().as_millis() as u64,
                sql,
            })
        }
    }
}

// ── Names ─────────────────────────────────────────────────────────────────────

pub fn quote(info: &OdbcInfo, ident: &str) -> String {
    match info.quote {
        Some('[') => format!("[{}]", ident.replace(']', "]]")),
        Some(q) => format!("{q}{}{q}", ident.replace(q, &format!("{q}{q}"))),
        None => ident.to_string(),
    }
}

/// `schema.table`, `catalog.table` or `table`, by how the database names things.
pub(crate) fn table_ref(info: &OdbcInfo, schema: &str, table: &str) -> String {
    let t = quote(info, table);
    if schema.is_empty() || schema == FLAT_SCHEMA && info.namespace == Namespace::Flat {
        return t;
    }
    match info.namespace {
        Namespace::Flat => t,
        _ => format!("{}.{t}", quote(info, schema)),
    }
}

/// (catalog, schema) arguments for a catalog call about `schema`.
fn scope(info: &OdbcInfo, schema: &str) -> (String, String) {
    match info.namespace {
        Namespace::Schema => (info.catalog.clone(), schema.to_string()),
        Namespace::Catalog => (schema.to_string(), String::new()),
        Namespace::Flat => (String::new(), String::new()),
    }
}

/// Read a catalog cursor's columns (1-based) as text.
fn catalog_rows(cursor: &mut impl Cursor, cols: &[u16]) -> Result<Vec<Vec<Option<String>>>, String> {
    let mut out = Vec::new();
    let mut buf = Vec::new();
    let mut wbuf = Vec::new();
    while let Some(mut row) = cursor.next_row().map_err(odbc_err)? {
        let mut r = Vec::with_capacity(cols.len());
        for c in cols {
            r.push(text_cell(&mut row, *c, &mut buf, &mut wbuf)?);
        }
        out.push(r);
    }
    Ok(out)
}

fn s(v: &Option<String>) -> String {
    v.clone().unwrap_or_default()
}

/// Schemas the database itself keeps, hidden from the list.
fn is_system_schema(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    matches!(
        n.as_str(),
        "INFORMATION_SCHEMA" | "PG_CATALOG" | "PG_TOAST" | "SYS" | "SYSIBM" | "SYSCAT" | "SYSSTAT" | "SYSFUN" | "SYSIBMADM" | "SYSPROC"
            | "SYSTOOLS" | "SYSIBMINTERNAL" | "SYSIBMTS" | "SYSPUBLIC" | "NULLID" | "SQLJ" | "_SYS_BI" | "_SYS_BIC" | "_SYS_REPO"
            | "_SYS_STATISTICS" | "_SYS_XS" | "_SYS_AFL" | "_SYS_EPM" | "_SYS_TASK" | "_SYS_SQL_ANALYZER" | "_SYS_PLAN_STABILITY"
            | "_SYS_TELEMETRY" | "_SYS_AUDIT" | "_SYS_DATA_ANONYMIZATION" | "_SYS_LDB" | "_SYS_RT" | "_SYS_SECURITY" | "_SYS_SPS"
            | "_SYS_WORKLOAD_REPLAY" | "_SYS_DI" | "_SYS_DI_TO" | "_SYS_DI_SEPARATE_CATALOG" | "_SYS_ADVISOR" | "_SYS_XB"
            | "SYS_SPATIAL" | "UIS" | "HANA_XS_BASE" | "%LIBRARY" | "%SYS" | "INFORMATION_SCHEMA_VIEWS" | "V_CATALOG" | "V_MONITOR"
            | "V_INTERNAL" | "V_FUNC" | "V_TXTINDEX" | "DBC" | "SYSLIB" | "SYSUDTLIB" | "SYSSPATIAL" | "TD_SYSFNLIB" | "TD_SYSXML"
            | "SYSBAR" | "SYSJDBC" | "SYSXML" | "SYS_CALENDAR" | "TDSTATS" | "TD_SYSGPL" | "TDQCD" | "SQLJ_SYS" | "EXA_STATISTICS"
            | "PERFORMANCE_SCHEMA" | "MYSQL"
    )
}

/// Every schema name: SQL_ALL_SCHEMAS (catalog "", schema "%", table ""),
/// or, where a driver answers that with nothing, the schemas its tables are in.
fn schema_names(conn: &Connection<'static>, info: &OdbcInfo) -> Vec<String> {
    let ask = |cat: &str, table: &str, types: &str| -> Vec<String> {
        let Ok(mut stmt) = conn.preallocate() else { return vec![] };
        let Ok(mut cur) = stmt.tables_cursor(cat, "%", table, types) else { return vec![] };
        catalog_rows(&mut cur, &[2]).unwrap_or_default().into_iter().filter_map(|r| r[0].clone()).filter(|s| !s.is_empty()).collect()
    };
    let mut names = ask("", "", "");
    if names.is_empty() {
        names = ask(&info.catalog, "%", "TABLE,VIEW");
    }
    names.sort();
    names.dedup();
    names
}

/// Every catalog name: SQL_ALL_CATALOGS (catalog "%", schema "", table "").
fn catalog_names(conn: &Connection<'static>) -> Vec<String> {
    let Ok(mut stmt) = conn.preallocate() else { return vec![] };
    let Ok(mut cur) = stmt.tables_cursor("%", "", "", "") else { return vec![] };
    catalog_rows(&mut cur, &[1]).unwrap_or_default().into_iter().filter_map(|r| r[0].clone()).filter(|s| !s.is_empty()).collect()
}

pub async fn list_schemas(h: &OdbcHandle) -> Result<Vec<String>, String> {
    run(h, |conn, info| {
        let mut names: Vec<String> = match info.namespace {
            Namespace::Flat => return Ok(vec![FLAT_SCHEMA.to_string()]),
            Namespace::Schema => schema_names(conn, info),
            Namespace::Catalog => catalog_names(conn),
        };
        names.sort();
        names.dedup();
        let mine: Vec<String> = names.iter().filter(|n| !is_system_schema(n)).cloned().collect();
        let mut out = if mine.is_empty() { names } else { mine };
        // The connection's own schema or catalog first.
        let current = if info.namespace == Namespace::Catalog { info.catalog.clone() } else { String::new() };
        if let Some(i) = out.iter().position(|n| *n == current) {
            let c = out.remove(i);
            out.insert(0, c);
        }
        if out.is_empty() {
            out.push(FLAT_SCHEMA.to_string());
        }
        Ok(out)
    })
    .await
}

fn tables_on(conn: &Connection<'static>, info: &OdbcInfo, schema: &str) -> Result<Vec<TableInfo>, String> {
    let (cat, sch) = scope(info, schema);
    let mut stmt = conn.preallocate().map_err(odbc_err)?;
    let mut cur = stmt.tables_cursor(&cat, &sch, "%", "TABLE,VIEW").map_err(odbc_err)?;
    let mut out: Vec<TableInfo> = catalog_rows(&mut cur, &[3, 4])?
        .into_iter()
        .filter_map(|r| {
            let name = r[0].clone()?;
            let ty = s(&r[1]).to_ascii_uppercase();
            if ty.contains("SYSTEM") {
                return None;
            }
            Some(TableInfo {
                name,
                row_count: -1,
                kind: if ty.contains("VIEW") { "view".into() } else { "table".into() },
                rls_enabled: None,
                created_at: None,
                create_order: None,
                system: None,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    Ok(out)
}

pub async fn list_tables(h: &OdbcHandle, schema: &str) -> Result<Vec<TableInfo>, String> {
    let schema = schema.to_string();
    run(h, move |conn, info| tables_on(conn, info, &schema)).await
}

/// One column of a foreign key: `fk_table.fk_column` references `pk_table.pk_column`.
struct FkRow {
    pk_table: String,
    pk_column: String,
    fk_table: String,
    fk_column: String,
    name: String,
}

/// The foreign keys of `table` (outgoing), or of the tables pointing at it.
fn foreign_keys_on(conn: &Connection<'static>, info: &OdbcInfo, schema: &str, table: &str, incoming: bool) -> Result<Vec<FkRow>, String> {
    let (cat, sch) = scope(info, schema);
    let mut stmt = conn.preallocate().map_err(odbc_err)?;
    let cur = if incoming {
        stmt.foreign_keys_cursor(&cat, &sch, table, "", "", "")
    } else {
        stmt.foreign_keys_cursor("", "", "", &cat, &sch, table)
    };
    let mut cur = match cur {
        Ok(c) => c,
        // Drivers without SQLForeignKeys (Hive, some flat-file ones).
        Err(_) => return Ok(vec![]),
    };
    // 3 PKTABLE_NAME, 4 PKCOLUMN_NAME, 7 FKTABLE_NAME, 8 FKCOLUMN_NAME, 12 FK_NAME
    Ok(catalog_rows(&mut cur, &[3, 4, 7, 8, 12])
        .unwrap_or_default()
        .into_iter()
        .map(|r| FkRow { pk_table: s(&r[0]), pk_column: s(&r[1]), fk_table: s(&r[2]), fk_column: s(&r[3]), name: s(&r[4]) })
        .collect())
}

fn columns_on(conn: &Connection<'static>, info: &OdbcInfo, schema: &str, table: &str) -> Result<Vec<ColumnStructureRow>, String> {
    let (cat, sch) = scope(info, schema);
    let rows = {
        let mut stmt = conn.preallocate().map_err(odbc_err)?;
        let mut cur = stmt.columns_cursor(&cat, &sch, table, "%").map_err(odbc_err)?;
        // 4 COLUMN_NAME, 5 DATA_TYPE, 6 TYPE_NAME, 7 COLUMN_SIZE, 9 DECIMAL_DIGITS,
        // 11 NULLABLE, 12 REMARKS, 13 COLUMN_DEF, 17 ORDINAL_POSITION
        let n = cur.num_result_cols().map_err(odbc_err)?;
        let wanted: Vec<u16> = [4u16, 5, 6, 7, 9, 11, 12, 13, 17].into_iter().filter(|c| (*c as i16) <= n).collect();
        let raw = catalog_rows(&mut cur, &wanted)?;
        raw.into_iter()
            .map(|r| {
                let mut full: Vec<Option<String>> = vec![None; 9];
                for (i, c) in wanted.iter().enumerate() {
                    let slot = [4u16, 5, 6, 7, 9, 11, 12, 13, 17].iter().position(|x| x == c).unwrap_or(0);
                    full[slot] = r[i].clone();
                }
                full
            })
            .collect::<Vec<_>>()
    };
    let pk: Vec<String> = {
        let mut stmt = conn.preallocate().map_err(odbc_err)?;
        let cat_opt = (!cat.is_empty()).then_some(cat.as_str());
        let sch_opt = (!sch.is_empty()).then_some(sch.as_str());
        let keys: Vec<(i32, String)> = match stmt.primary_keys_cursor(cat_opt, sch_opt, table) {
            Ok(mut cur) => catalog_rows(&mut cur, &[4, 5])
                .unwrap_or_default()
                .into_iter()
                .map(|r| (s(&r[1]).parse().unwrap_or(0), s(&r[0])))
                .collect(),
            Err(_) => vec![],
        };
        let mut keys = keys;
        keys.sort();
        keys.into_iter().map(|k| k.1).collect()
    };
    let fks = foreign_keys_on(conn, info, schema, table, false).unwrap_or_default();
    Ok(rows
        .into_iter()
        .enumerate()
        .map(|(i, r)| {
            let name = s(&r[0]);
            let ty = s(&r[2]);
            let size = s(&r[3]);
            let digits = s(&r[4]);
            let code: i32 = s(&r[1]).parse().unwrap_or(0);
            // Only a character or binary type takes a length: some drivers
            // report other types as text (psqlODBC's bool arrives as VARCHAR
            // of 5), and `bool(5)` is not a type.
            let sized_name = { let t = ty.to_ascii_lowercase(); t.contains("char") || t.contains("binary") || t.contains("string") };
            let shows_size = matches!(code, 1 | 12 | -8 | -9 | -2 | -3) && sized_name && !size.is_empty() && size != "0" && !ty.contains('(');
            let shows_scale = matches!(code, 2 | 3) && !size.is_empty() && !ty.contains('(');
            let data_type = if shows_size {
                format!("{ty}({size})")
            } else if shows_scale {
                format!("{ty}({size},{})", if digits.is_empty() { "0" } else { &digits })
            } else {
                ty
            };
            let fk = fks.iter().find(|f| f.fk_column == name);
            ColumnStructureRow {
                ordinal_position: s(&r[8]).parse().unwrap_or((i + 1) as i32),
                data_type,
                is_nullable: s(&r[5]) != "0",
                column_default: r[7].clone().filter(|d| !d.is_empty()),
                foreign_key: fk.map(|f| format!("{}.{}", f.pk_table, f.pk_column)),
                fk_constraint_name: fk.map(|f| f.name.clone()).filter(|n| !n.is_empty()),
                comment: r[6].clone().filter(|d| !d.is_empty()),
                is_primary_key: pk.contains(&name),
                name,
            }
        })
        .collect())
}

pub async fn get_column_structure(h: &OdbcHandle, schema: &str, table: &str) -> Result<Vec<ColumnStructureRow>, String> {
    let (schema, table) = (schema.to_string(), table.to_string());
    run(h, move |conn, info| columns_on(conn, info, &schema, &table)).await
}

pub async fn schema_column_structure(h: &OdbcHandle, schema: &str) -> Result<Vec<super::schema::TableColumnStructure>, String> {
    let schema = schema.to_string();
    run(h, move |conn, info| {
        let tables = tables_on(conn, info, &schema)?;
        Ok(tables
            .into_iter()
            .take(500)
            .filter_map(|t| columns_on(conn, info, &schema, &t.name).ok().map(|columns| super::schema::TableColumnStructure { table: t.name, columns }))
            .collect())
    })
    .await
}

pub async fn incoming_foreign_keys(h: &OdbcHandle, schema: &str, table: &str) -> Result<Vec<super::schema::IncomingForeignKey>, String> {
    let (schema, table) = (schema.to_string(), table.to_string());
    run(h, move |conn, info| {
        let mut out: Vec<super::schema::IncomingForeignKey> = Vec::new();
        for f in foreign_keys_on(conn, info, &schema, &table, true)? {
            let (col, from_table, ref_col, name) = (f.fk_column, f.fk_table, f.pk_column, f.name);
            match out.iter_mut().find(|f| f.from_table == from_table && f.constraint_name == name) {
                Some(f) => {
                    f.from_columns.push(col);
                    f.to_columns.push(ref_col);
                }
                None => out.push(super::schema::IncomingForeignKey {
                    from_schema: schema.clone(),
                    from_table,
                    from_columns: vec![col],
                    to_columns: vec![ref_col],
                    constraint_name: name,
                }),
            }
        }
        Ok(out)
    })
    .await
}

/// A CREATE TABLE written from the catalog: columns, NOT NULL, defaults, the
/// primary key. ODBC has no standard way to ask for the real DDL.
pub async fn get_ddl(h: &OdbcHandle, schema: &str, table: &str) -> Result<String, String> {
    let cols = get_column_structure(h, schema, table).await?;
    let info = &h.info;
    let mut lines: Vec<String> = cols
        .iter()
        .map(|c| {
            let mut l = format!("  {} {}", quote(info, &c.name), c.data_type);
            if !c.is_nullable {
                l.push_str(" NOT NULL");
            }
            if let Some(d) = &c.column_default {
                l.push_str(&format!(" DEFAULT {d}"));
            }
            l
        })
        .collect();
    let pk: Vec<String> = cols.iter().filter(|c| c.is_primary_key).map(|c| quote(info, &c.name)).collect();
    if !pk.is_empty() {
        lines.push(format!("  PRIMARY KEY ({})", pk.join(", ")));
    }
    Ok(format!(
        "-- Written from the ODBC catalog ({}); the database's own DDL may differ.\nCREATE TABLE {} (\n{}\n);",
        info.dbms,
        table_ref(info, schema, table),
        lines.join(",\n")
    ))
}

#[cfg_attr(not(test), allow(dead_code))]
pub async fn count_rows(h: &OdbcHandle, schema: &str, table: &str) -> Result<i64, String> {
    let sql = format!("SELECT COUNT(*) FROM {}", table_ref(&h.info, schema, table));
    let r = query(h, &sql).await?;
    Ok(r.rows.first().and_then(|r| r.first()).and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok())).unwrap_or(-1))
}

// ── The grid ──────────────────────────────────────────────────────────────────

fn is_numeric_type(ty: &str) -> bool {
    let t = ty.to_ascii_lowercase();
    ["int", "serial", "decimal", "numeric", "number", "float", "double", "real", "money"].iter().any(|w| t.contains(w))
        && !t.contains("interval")
        && !t.contains("point")
}

fn is_text_type(ty: &str) -> bool {
    let t = ty.to_ascii_lowercase();
    ["char", "text", "string", "clob", "memo", "uuid", "json", "xml"].iter().any(|w| t.contains(w))
}

/// A column as text, for search and contains-filters on non-text columns.
fn as_text(info: &OdbcInfo, col: &str, ty: &str) -> String {
    if is_text_type(ty) {
        return col.to_string();
    }
    let d = info.dbms.to_ascii_lowercase();
    if d.contains("mysql") || d.contains("mariadb") {
        format!("CAST({col} AS CHAR)")
    } else if d.contains("access") {
        format!("CStr({col})")
    } else {
        format!("CAST({col} AS VARCHAR(4000))")
    }
}

/// `col op value`, with numbers written in (after checking they are numbers)
/// and everything else bound as a text parameter.
fn compare(col: &str, ty: &str, op: &str, raw: &str, p: &mut Params) -> Option<String> {
    if is_numeric_type(ty) {
        let t = raw.trim();
        if let Ok(n) = t.parse::<i64>() {
            p.0.push(Param::Int(n));
            return Some(format!("{col} {op} ?"));
        }
        if let Ok(x) = t.parse::<f64>() {
            if x.is_finite() {
                p.0.push(Param::Float(x));
                return Some(format!("{col} {op} ?"));
            }
        }
        return None;
    }
    Some(format!("{col} {op} {}", p.text(raw)))
}

fn build_where(info: &OdbcInfo, cols: &[ColumnStructureRow], search: Option<&str>, filters: Option<&[RowFilter]>, p: &mut Params) -> String {
    let by_name: HashMap<&str, &ColumnStructureRow> = cols.iter().map(|c| (c.name.as_str(), c)).collect();
    let mut clauses: Vec<String> = Vec::new();
    if let Some(q) = search.map(str::trim).filter(|s| !s.is_empty()) {
        let needle = format!("%{}%", q.to_lowercase());
        let ors: Vec<String> = cols
            .iter()
            .filter(|c| is_text_type(&c.data_type))
            .map(|c| format!("LOWER({}) LIKE {}", quote(info, &c.name), p.text(needle.clone())))
            .collect();
        if !ors.is_empty() {
            clauses.push(format!("({})", ors.join(" OR ")));
        }
    }
    for f in filters.unwrap_or(&[]) {
        let Some(c) = by_name.get(f.column.as_str()) else { continue };
        let col = quote(info, &c.name);
        let v = f.value.as_deref().unwrap_or("");
        let text = as_text(info, &col, &c.data_type);
        let clause = match f.op.as_str() {
            "=" | "eq" | "!=" | "ne" | "neq" | ">" | "gt" | ">=" | "gte" | "<" | "lt" | "<=" | "lte" => {
                let op = match f.op.as_str() {
                    "=" | "eq" => "=",
                    "!=" | "ne" | "neq" => "<>",
                    ">" | "gt" => ">",
                    ">=" | "gte" => ">=",
                    "<" | "lt" => "<",
                    _ => "<=",
                };
                match compare(&col, &c.data_type, op, v, p) {
                    Some(c) => c,
                    None => continue,
                }
            }
            "contains" => format!("LOWER({text}) LIKE {}", p.text(format!("%{}%", v.to_lowercase()))),
            "not_contains" => format!("({col} IS NULL OR NOT LOWER({text}) LIKE {})", p.text(format!("%{}%", v.to_lowercase()))),
            "starts_with" => format!("LOWER({text}) LIKE {}", p.text(format!("{}%", v.to_lowercase()))),
            "ends_with" => format!("LOWER({text}) LIKE {}", p.text(format!("%{}", v.to_lowercase()))),
            "between" => {
                let mut parts = v.splitn(2, ',');
                let (a, b) = (parts.next().unwrap_or("").trim().to_string(), parts.next().unwrap_or("").trim().to_string());
                let mut tmp = p.clone();
                match (compare(&col, &c.data_type, ">=", &a, &mut tmp), compare(&col, &c.data_type, "<=", &b, &mut tmp)) {
                    (Some(x), Some(y)) => {
                        *p = tmp;
                        format!("({x} AND {y})")
                    }
                    _ => continue,
                }
            }
            "is_null" => format!("{col} IS NULL"),
            "is_not_null" => format!("{col} IS NOT NULL"),
            _ => continue,
        };
        let joiner = if clauses.is_empty() { "" } else if f.conjunct.as_deref() == Some("or") { " OR " } else { " AND " };
        if joiner.is_empty() {
            clauses.push(clause);
        } else {
            let last = clauses.pop().unwrap_or_default();
            clauses.push(format!("({last}{joiner}{clause})"));
        }
    }
    if clauses.is_empty() { String::new() } else { format!(" WHERE {}", clauses.join(" AND ")) }
}

/// The page query for a paging style, and how many leading rows to skip here.
pub(crate) fn page_sql(paging: Paging, from: &str, order: &str, limit: i64, offset: i64) -> (String, i64) {
    match paging {
        Paging::LimitOffset => (format!("SELECT * FROM {from}{order} LIMIT {limit} OFFSET {offset}"), 0),
        Paging::OffsetLimit => (format!("SELECT * FROM {from}{order} OFFSET {offset} LIMIT {limit}"), 0),
        Paging::OffsetFetch => (format!("SELECT * FROM {from}{order} OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY"), 0),
        Paging::OffsetFetchOrdered => {
            let order = if order.is_empty() { " ORDER BY (SELECT NULL)" } else { order };
            (format!("SELECT * FROM {from}{order} OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY"), 0)
        }
        Paging::SkipFirst => (format!("SELECT SKIP {offset} FIRST {limit} * FROM {from}{order}"), 0),
        Paging::Top => (format!("SELECT TOP {} * FROM {from}{order}", limit + offset), offset),
        Paging::Client => (format!("SELECT * FROM {from}{order}"), offset),
    }
}

/// Read one page: skip `skip` rows, keep `limit`, then let the cursor go.
fn read_page(conn: &Connection<'static>, sql: &str, p: &Params, skip: i64, limit: i64) -> Result<(Vec<ColumnInfo>, Vec<Vec<Value>>), String> {
    let mut columns = Vec::new();
    let mut rows = Vec::new();
    let mut seen = 0i64;
    execute_on(conn, sql, p, None, |c| {
        columns = c;
        Ok(())
    }, |r| {
        seen += 1;
        if seen > skip {
            rows.push(r);
        }
        Ok((rows.len() as i64) < limit)
    })?;
    Ok((columns, rows))
}

#[allow(clippy::too_many_arguments)]
pub async fn get_table_rows(
    h: &OdbcHandle,
    schema: &str,
    table: &str,
    limit: i64,
    offset: i64,
    search: Option<String>,
    sort_column: Option<String>,
    sort_direction: Option<String>,
    filters: Option<Vec<RowFilter>>,
) -> Result<TableRows, String> {
    let t0 = Instant::now();
    let (schema, table) = (schema.to_string(), table.to_string());
    run(h, move |conn, info| {
        let cols = columns_on(conn, info, &schema, &table)?;
        if cols.is_empty() {
            return Err(format!("No table named {table}"));
        }
        let from = table_ref(info, &schema, &table);
        let mut p = Params::default();
        let where_clause = build_where(info, &cols, search.as_deref(), filters.as_deref(), &mut p);
        let order = match sort_column.as_deref().filter(|c| cols.iter().any(|x| x.name == *c)) {
            Some(c) => {
                let d = if sort_direction.as_deref().is_some_and(|d| d.eq_ignore_ascii_case("desc")) { "DESC" } else { "ASC" };
                format!(" ORDER BY {} {d}", quote(info, c))
            }
            None => {
                let pk: Vec<String> = cols.iter().filter(|c| c.is_primary_key).map(|c| quote(info, &c.name)).collect();
                if pk.is_empty() { String::new() } else { format!(" ORDER BY {}", pk.join(", ")) }
            }
        };
        let (limit, offset) = (limit.max(0), offset.max(0));
        let from_where = format!("{from}{where_clause}");
        let (sql, skip) = page_sql(info.paging, &from_where, &order, limit, offset);
        // A paging clause the database turns out not to speak: read and skip here.
        let (sql, page) = match read_page(conn, &sql, &p, skip, limit) {
            Ok(page) => (sql, page),
            Err(e) if info.paging != Paging::Client => {
                let (fallback, skip) = page_sql(Paging::Client, &from_where, &order, limit, offset);
                match read_page(conn, &fallback, &p, skip, limit) {
                    Ok(page) => (fallback, page),
                    Err(_) => return Err(e),
                }
            }
            Err(e) => return Err(e),
        };
        let (columns, rows) = page;
        let count_sql = format!("SELECT COUNT(*) FROM {from_where}");
        let total = collect(conn, &count_sql, &p, None, Instant::now())
            .ok()
            .and_then(|r| r.rows.first().and_then(|r| r.first()).and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok())))
            .unwrap_or(offset + rows.len() as i64);
        let fks = cols
            .iter()
            .filter_map(|c| {
                let (t, col) = c.foreign_key.as_ref()?.split_once('.')?;
                Some(ForeignKeyInfo {
                    columns: vec![c.name.clone()],
                    referenced_schema: schema.clone(),
                    referenced_table: t.to_string(),
                    referenced_columns: vec![col.to_string()],
                })
            })
            .collect();
        Ok(TableRows {
            preview_columns: Vec::new(),
            columns,
            rows,
            total,
            query_ms: t0.elapsed().as_millis() as u64,
            primary_key: cols.iter().filter(|c| c.is_primary_key).map(|c| c.name.clone()).collect(),
            foreign_keys: fks,
            sql: format!("{sql}\n{count_sql}"),
        })
    })
    .await
}

/// A grid value as a typed parameter for a column, or NULL.
fn bind_value(ty: &str, v: &Value, p: &mut Params) -> String {
    match v {
        Value::Null => "NULL".into(),
        Value::Bool(b) => {
            // Booleans as 1/0: every SQL engine's BIT, SMALLINT or BOOLEAN takes it.
            p.0.push(Param::Int(*b as i64));
            "?".into()
        }
        Value::Number(n) if n.is_i64() => {
            p.0.push(Param::Int(n.as_i64().unwrap_or(0)));
            "?".into()
        }
        Value::Number(n) => {
            p.0.push(Param::Float(n.as_f64().unwrap_or(0.0)));
            "?".into()
        }
        Value::String(s) if is_numeric_type(ty) => {
            if let Ok(n) = s.trim().parse::<i64>() {
                p.0.push(Param::Int(n));
            } else {
                p.text(s.clone());
            }
            "?".into()
        }
        Value::String(s) => p.text(s.clone()).into(),
        other => p.text(other.to_string()).into(),
    }
}

fn pk_where(info: &OdbcInfo, cols: &[ColumnStructureRow], pk: &HashMap<String, Value>, p: &mut Params) -> Result<String, String> {
    let keys: Vec<&ColumnStructureRow> = cols.iter().filter(|c| c.is_primary_key).collect();
    if keys.is_empty() {
        return Err("This table reports no primary key through its ODBC driver, so the grid can't tell rows apart. \
                    Use UPDATE or DELETE in the SQL console."
            .into());
    }
    let mut parts = Vec::new();
    for c in keys {
        let v = pk.get(&c.name).ok_or_else(|| format!("The row's {} is missing", c.name))?;
        let col = quote(info, &c.name);
        if v.is_null() {
            parts.push(format!("{col} IS NULL"));
        } else {
            parts.push(format!("{col} = {}", bind_value(&c.data_type, v, p)));
        }
    }
    Ok(parts.join(" AND "))
}

fn write(conn: &Connection<'static>, sql: &str, p: &Params) -> Result<Option<usize>, String> {
    execute_on(conn, sql, p, None, |_| Ok(()), |_| Ok(false))
}

pub async fn update_table_cell(
    h: &OdbcHandle,
    schema: &str,
    table: &str,
    primary_key: HashMap<String, Value>,
    column: &str,
    value: &Value,
) -> Result<(), String> {
    let (schema, table, column, value) = (schema.to_string(), table.to_string(), column.to_string(), value.clone());
    run(h, move |conn, info| {
        let cols = columns_on(conn, info, &schema, &table)?;
        let c = cols.iter().find(|c| c.name == column).ok_or_else(|| format!("No column named {column}"))?;
        let mut p = Params::default();
        let set = format!("{} = {}", quote(info, &column), bind_value(&c.data_type, &value, &mut p));
        let w = pk_where(info, &cols, &primary_key, &mut p)?;
        let sql = format!("UPDATE {} SET {set} WHERE {w}", table_ref(info, &schema, &table));
        match write(conn, &sql, &p)? {
            Some(1) | None => Ok(()),
            Some(n) => Err(super::query::row_count_error(&primary_key.keys().cloned().collect::<Vec<_>>(), &primary_key, n as u64)),
        }
    })
    .await
}

pub async fn insert_table_row(h: &OdbcHandle, schema: &str, table: &str, values: HashMap<String, Value>) -> Result<Vec<Value>, String> {
    let (schema, table) = (schema.to_string(), table.to_string());
    run(h, move |conn, info| {
        let cols = columns_on(conn, info, &schema, &table)?;
        let mut p = Params::default();
        let mut names = Vec::new();
        let mut holes = Vec::new();
        for c in &cols {
            let Some(v) = values.get(&c.name) else { continue };
            names.push(quote(info, &c.name));
            holes.push(bind_value(&c.data_type, v, &mut p));
        }
        if names.is_empty() {
            return Err("Fill in at least one column".into());
        }
        let sql = format!("INSERT INTO {} ({}) VALUES ({})", table_ref(info, &schema, &table), names.join(", "), holes.join(", "));
        write(conn, &sql, &p)?;
        Ok(cols.iter().map(|c| values.get(&c.name).cloned().unwrap_or(Value::Null)).collect())
    })
    .await
}

pub async fn delete_table_rows(h: &OdbcHandle, schema: &str, table: &str, primary_keys: Vec<HashMap<String, Value>>) -> Result<u64, String> {
    if primary_keys.is_empty() {
        return Ok(0);
    }
    let (schema, table) = (schema.to_string(), table.to_string());
    run(h, move |conn, info| {
        let cols = columns_on(conn, info, &schema, &table)?;
        let mut p = Params::default();
        let mut ors = Vec::new();
        for pk in &primary_keys {
            ors.push(format!("({})", pk_where(info, &cols, pk, &mut p)?));
        }
        let sql = format!("DELETE FROM {} WHERE {}", table_ref(info, &schema, &table), ors.join(" OR "));
        Ok(write(conn, &sql, &p)?.map(|n| n as u64).unwrap_or(primary_keys.len() as u64))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(cs: Option<&str>, dsn: Option<&str>, user: Option<&str>, pw: Option<&str>) -> OdbcConfig {
        OdbcConfig {
            name: "t".into(),
            preset: None,
            dsn: dsn.map(String::from),
            connection_string: cs.map(String::from),
            user: user.map(String::from),
            password: pw.map(String::from),
            ssh: None,
            target_host: None,
            target_port: None,
        }
    }

    #[test]
    fn connection_strings_carry_credentials_once() {
        assert_eq!(
            connection_string(&cfg(Some("Driver={Db2};Hostname=h;Port=50000"), None, Some("me"), Some("p;w}d"))).unwrap(),
            "Driver={Db2};Hostname=h;Port=50000;UID=me;PWD={p;w}}d};"
        );
        assert_eq!(connection_string(&cfg(None, Some("Sales DW"), None, None)).unwrap(), "DSN=Sales DW;");
        // A string that already says who: left alone.
        assert_eq!(connection_string(&cfg(Some("DSN=x;UID=a;PWD=b;"), None, Some("me"), Some("pw"))).unwrap(), "DSN=x;UID=a;PWD=b;");
        assert!(connection_string(&cfg(None, None, None, None)).is_err());
    }

    /// Through a real SSH server (STROKE_TEST_ODBC_SSH = key|ssh port|db host|db port|connection string).
    #[tokio::test]
    async fn live_through_ssh() {
        let Ok(spec) = std::env::var("STROKE_TEST_ODBC_SSH") else { return };
        let p: Vec<&str> = spec.splitn(5, '|').collect();
        let mut c = cfg(Some(p[4]), None, None, None);
        c.ssh = Some(super::super::ssh_tunnel::SshConfig {
            host: "127.0.0.1".into(),
            port: p[1].parse().unwrap(),
            username: "tester".into(),
            private_key_path: p[0].into(),
            keepalive: 0,
        });
        c.target_host = Some(p[2].into());
        c.target_port = Some(p[3].parse().unwrap());
        let (c, tunnel) = crate::db::connection::resolve_odbc_ssh(c).await.unwrap();
        let tunnel = tunnel.expect("a tunnel");
        let cs = c.connection_string.clone().unwrap();
        assert!(cs.contains("127.0.0.1") && cs.contains(&tunnel.local_port.to_string()) && !cs.contains(p[2]), "{cs}");
        let h = connect(&c).await.unwrap();
        assert_eq!(query(&h, "SELECT 41 + 1").await.unwrap().rows[0][0], json!(42));
    }

    #[test]
    fn tunnels_retarget_host_and_port_only() {
        let db2 = "Driver={IBM DB2 ODBC DRIVER};Database=SAMPLE;Hostname=db.internal;Port=50000;Protocol=TCPIP;";
        assert_eq!(retarget(db2, "db.internal", 50000, 41234), "Driver={IBM DB2 ODBC DRIVER};Database=SAMPLE;Hostname=127.0.0.1;Port=41234;Protocol=TCPIP;");
        assert_eq!(retarget("Driver={HDBODBC};ServerNode=hana.internal:30015;", "hana.internal", 30015, 5000), "Driver={HDBODBC};ServerNode=127.0.0.1:5000;");
        assert_eq!(retarget("DBQ=//ora:1521/FREEPDB1;", "ora", 1521, 7000), "DBQ=//127.0.0.1:7000/FREEPDB1;");
        assert_eq!(retarget("Dbname=fb/3050:/data/x.fdb;", "fb", 3050, 7001), "Dbname=127.0.0.1/7001:/data/x.fdb;");
        // A database named like the port, or a driver brace with ';', stays.
        assert_eq!(retarget("Driver={A;B};Database=50000x;Port=50000", "h", 50000, 1), "Driver={A;B};Database=50000x;Port=1");
    }

    #[test]
    fn paging_follows_the_database() {
        assert_eq!(paging_for("PostgreSQL"), Paging::LimitOffset);
        assert_eq!(paging_for("HDB"), Paging::LimitOffset);
        assert_eq!(paging_for("DB2/LINUXX8664"), Paging::OffsetFetch);
        assert_eq!(paging_for("Trino"), Paging::OffsetLimit);
        assert_eq!(paging_for("Microsoft SQL Server"), Paging::OffsetFetchOrdered);
        assert_eq!(paging_for("Teradata"), Paging::Top);
        assert_eq!(paging_for("Informix Dynamic Server"), Paging::SkipFirst);
        assert_eq!(paging_for("Something New"), Paging::Client);
        assert_eq!(page_sql(Paging::Top, "t", "", 50, 100), ("SELECT TOP 150 * FROM t".to_string(), 100));
        assert_eq!(page_sql(Paging::OffsetFetchOrdered, "t", "", 5, 0).0, "SELECT * FROM t ORDER BY (SELECT NULL) OFFSET 0 ROWS FETCH NEXT 5 ROWS ONLY");
    }

    #[test]
    fn identifiers_quote_the_drivers_way() {
        let mut info = OdbcInfo { dbms: "x".into(), quote: Some('"'), namespace: Namespace::Schema, catalog: String::new(), paging: Paging::Client };
        assert_eq!(table_ref(&info, "s", "a\"b"), "\"s\".\"a\"\"b\"");
        info.quote = Some('`');
        info.namespace = Namespace::Catalog;
        assert_eq!(table_ref(&info, "shop", "t"), "`shop`.`t`");
        info.namespace = Namespace::Flat;
        assert_eq!(table_ref(&info, FLAT_SCHEMA, "t"), "`t`");
        info.quote = Some('[');
        assert_eq!(quote(&info, "a]b"), "[a]]b]");
    }

    #[cfg(not(windows))]
    #[test]
    fn registries_merge_with_full_driver_paths() {
        let a = "[PostgreSQL Unicode]\nDriver=psqlodbcw.so\nSetup = libodbcpsqlS.so\n\n[Db2]\nDriver=/opt/ibm/libdb2o.so\n".to_string();
        let b = "# brew\n[Db2]\nDriver=/elsewhere.so\n[Vertica]\nDriver=/opt/vertica/lib64/libverticaodbc.so\n".to_string();
        let out = registry::merge(&[a, b], &|name| Some(format!("/usr/lib/x/odbc/{name}")));
        assert!(out.contains("[PostgreSQL Unicode]\nDriver = /usr/lib/x/odbc/psqlodbcw.so\nSetup = /usr/lib/x/odbc/libodbcpsqlS.so"));
        assert!(out.contains("[Db2]\nDriver = /opt/ibm/libdb2o.so"));
        assert!(!out.contains("/elsewhere.so"), "the first registry to name a driver wins");
        assert!(out.contains("[Vertica]"));
    }

    #[test]
    fn filters_bind_values_and_write_numbers_checked() {
        let info = OdbcInfo { dbms: "PostgreSQL".into(), quote: Some('"'), namespace: Namespace::Schema, catalog: String::new(), paging: Paging::LimitOffset };
        let col = |n: &str, t: &str| ColumnStructureRow {
            ordinal_position: 1, name: n.into(), data_type: t.into(), is_nullable: true, column_default: None,
            foreign_key: None, fk_constraint_name: None, comment: None, is_primary_key: false,
        };
        let cols = vec![col("id", "int4"), col("name", "varchar(40)")];
        let mut p = Params::default();
        let f = |c: &str, op: &str, v: &str| RowFilter { column: c.into(), op: op.into(), value: Some(v.into()), conjunct: None, data_type: None };
        let w = build_where(&info, &cols, Some("Ann"), Some(&[f("id", ">=", "2"), f("id", "=", "2; DROP TABLE x")]), &mut p);
        assert_eq!(w, " WHERE ((LOWER(\"name\") LIKE ?) AND \"id\" >= ?)");
        assert!(matches!(p.0[0], Param::Text(ref s) if s == "%ann%"));
        assert!(matches!(p.0[1], Param::Int(2)));
    }

    /// Live, through a real driver: STROKE_TEST_ODBC is a connection string,
    /// e.g. `Driver={SQLite3};Database=/tmp/t.db` or
    /// `Driver={PostgreSQL Unicode};Server=localhost;Port=5432;Database=postgres;UID=postgres;PWD=pw`.
    #[tokio::test]
    async fn live_round_trip() {
        let Ok(cs) = std::env::var("STROKE_TEST_ODBC") else { return };
        let h = connect(&cfg(Some(&cs), None, None, None)).await.unwrap();
        ping(&h).await.unwrap();
        let t = format!("stroke_odbc_{}", std::process::id());
        let schemas = list_schemas(&h).await.unwrap();
        let schema = match h.info.namespace {
            Namespace::Schema => schemas.iter().find(|s| s.eq_ignore_ascii_case("public") || s.eq_ignore_ascii_case("dbo")).cloned().unwrap_or(schemas[0].clone()),
            Namespace::Catalog => h.info.catalog.clone(),
            Namespace::Flat => FLAT_SCHEMA.to_string(),
        };
        let tref = table_ref(&h.info, &schema, &t);
        if query(&h, &format!("CREATE TABLE {tref} (id INTEGER NOT NULL PRIMARY KEY, label VARCHAR(40), price DOUBLE PRECISION, ok SMALLINT)"))
            .await
            .is_err()
        {
            query(&h, &format!("CREATE TABLE {tref} (id INTEGER NOT NULL PRIMARY KEY, label VARCHAR(40), price DOUBLE, ok SMALLINT)")).await.unwrap();
        }
        // A second table pointing at the first, for the foreign key jumps.
        let child = format!("{t}_c");
        let cref = table_ref(&h.info, &schema, &child);
        let made_child = query(&h, &format!("CREATE TABLE {cref} (cid INTEGER NOT NULL PRIMARY KEY, item_id INTEGER, FOREIGN KEY (item_id) REFERENCES {tref} (id))")).await.is_ok();
        let ins = query(&h, &format!("INSERT INTO {tref} (id, label, price, ok) VALUES (1, 'pen', 2.5, 1), (2, 'cup', 4, 0)")).await.unwrap();
        assert_eq!(ins.row_count, Some(2), "{ins:?}");
        assert!(list_tables(&h, &schema).await.unwrap().iter().any(|x| x.name == t));
        let cols = get_column_structure(&h, &schema, &t).await.unwrap();
        assert_eq!(cols.len(), 4);
        assert!(cols[0].is_primary_key, "{cols:?}");
        assert!(!cols[0].is_nullable);

        let page = get_table_rows(&h, &schema, &t, 1, 1, None, None, None, None).await.unwrap();
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0][0], json!(2));
        assert_eq!(page.total, 2);
        let f = RowFilter { column: "price".into(), op: ">".into(), value: Some("3".into()), conjunct: None, data_type: None };
        let page = get_table_rows(&h, &schema, &t, 50, 0, Some("CU".into()), Some("id".into()), Some("desc".into()), Some(vec![f])).await.unwrap();
        assert_eq!(page.rows.len(), 1, "{}", page.sql);
        assert_eq!(page.rows[0][1], json!("cup"));
        assert_eq!(page.rows[0][2], json!(4.0));

        update_table_cell(&h, &schema, &t, HashMap::from([("id".into(), json!(1))]), "label", &json!("pencil")).await.unwrap();
        update_table_cell(&h, &schema, &t, HashMap::from([("id".into(), json!(2))]), "price", &Value::Null).await.unwrap();
        insert_table_row(&h, &schema, &t, HashMap::from([("id".into(), json!(3)), ("label".into(), json!("ink"))])).await.unwrap();
        let r = query(&h, &format!("SELECT id, label, price FROM {tref} ORDER BY id")).await.unwrap();
        assert_eq!(r.rows.len(), 3);
        assert_eq!(r.rows[0][1], json!("pencil"));
        assert_eq!(r.rows[1][2], Value::Null);
        assert_eq!(delete_table_rows(&h, &schema, &t, vec![HashMap::from([("id".into(), json!(3))])]).await.unwrap(), 1);
        assert_eq!(count_rows(&h, &schema, &t).await.unwrap(), 2);
        assert!(get_ddl(&h, &schema, &t).await.unwrap().contains("PRIMARY KEY"));
        assert!(query(&h, "SELECT nope FROM no_such_table_here").await.is_err());
        if made_child {
            let ccols = get_column_structure(&h, &schema, &child).await.unwrap();
            let item = ccols.iter().find(|c| c.name == "item_id").unwrap();
            // SQLite's ODBC driver reports no foreign keys; the others do.
            if !h.info.dbms.to_ascii_lowercase().contains("sqlite") {
                assert_eq!(item.foreign_key.as_deref(), Some(format!("{t}.id").as_str()), "{ccols:?}");
                let inc = incoming_foreign_keys(&h, &schema, &t).await.unwrap();
                assert!(inc.iter().any(|f| f.from_table == child && f.from_columns == ["item_id"] && f.to_columns == ["id"]), "{}", inc.len());
            }
            query(&h, &format!("DROP TABLE {cref}")).await.unwrap();
        }
        query(&h, &format!("DROP TABLE {tref}")).await.unwrap();

        // Stop: SQLCancel ends a long statement on the server.
        let d = h.info.dbms.to_ascii_lowercase();
        let slow = if d.contains("postgres") {
            Some("SELECT pg_sleep(30)")
        } else if d.contains("mysql") || d.contains("mariadb") {
            Some("SELECT SLEEP(30)")
        } else {
            None
        };
        if let Some(slow) = slow {
            let (tx, rx) = tokio::sync::oneshot::channel();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let _ = tx.send(());
            });
            let t0 = Instant::now();
            let r = query_cancellable(&h, slow, rx).await;
            let took = t0.elapsed();
            // MariaDB answers a cancelled SLEEP with its row (1) rather than an error.
            assert!(took < std::time::Duration::from_secs(10), "Stop took {took:?}: {r:?}");
            // The connection is usable afterwards.
            assert_eq!(query(&h, "SELECT 1").await.unwrap().rows[0][0], json!(1));
        }
    }
}
