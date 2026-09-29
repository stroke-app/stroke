use super::connection::RedisConfig;
use super::query::{ColumnInfo, RowFilter, SqlResult, TableRows};
use super::schema::{ColumnStructureRow, IndexInfo, TableInfo};
use serde_json::Value as JsonValue;
use std::time::Instant;

// ── Crate-name collision note ────────────────────────────────────────────────
// This module is `crate::db::redis`, but the driver crate is *also* named
// `redis`. Inside this file the bare path `redis::…` would resolve to *this*
// module, so the external crate is always referenced through the leading-colon
// path `::redis::…`.

// ── Connection ────────────────────────────────────────────────────────────────

/// Connections kept for reuse, keyed by URL (host, port, db, TLS and password).
///
/// Every command used to open its own connection. Against a hosted Redis
/// (Upstash, Railway) that is DNS + TCP + TLS + AUTH per key browsed, three or
/// more round trips before the command itself. A multiplexed connection is
/// cheap to clone and safe to share, so one per config serves every command.
struct Cached {
    conn: ::redis::aio::MultiplexedConnection,
    last_used: Instant,
}
static POOL: std::sync::LazyLock<tokio::sync::Mutex<std::collections::HashMap<String, Cached>>> =
    std::sync::LazyLock::new(Default::default);

/// Past this much idle time a cached connection is PINGed before reuse: hosted
/// Redis closes idle clients, and one round trip to find out beats a failed
/// command.
const IDLE_CHECK: std::time::Duration = std::time::Duration::from_secs(30);

fn url_for(cfg: &RedisConfig) -> String {
    let scheme = if cfg.tls { "rediss" } else { "redis" };
    let auth = match cfg.password.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(pw) => format!(":{}@", urlencoding::encode(pw)),
        None => String::new(),
    };
    format!("{scheme}://{auth}{}:{}/{}", cfg.host, cfg.port, cfg.db)
}

async fn dial(url: &str) -> Result<::redis::aio::MultiplexedConnection, String> {
    let client = ::redis::Client::open(url).map_err(|e| format!("Redis connection failed: {e}"))?;
    // Bounded: with no timeout an unreachable host (or a TLS port spoken to in
    // plain TCP) left the connect awaiting until the OS gave up.
    let config = ::redis::AsyncConnectionConfig::new()
        .set_connection_timeout(Some(std::time::Duration::from_secs(10)))
        .set_response_timeout(Some(std::time::Duration::from_secs(30)));
    client
        .get_multiplexed_async_connection_with_config(&config)
        .await
        .map_err(|e| format!("Redis connection failed: {e}"))
}

/// A multiplexed connection for this config: the cached one when it is still
/// alive, otherwise a new one (which then becomes the cached one).
async fn open(cfg: &RedisConfig) -> Result<::redis::aio::MultiplexedConnection, String> {
    let url = url_for(cfg);
    let mut pool = POOL.lock().await;
    if let Some(entry) = pool.get_mut(&url) {
        let fresh = entry.last_used.elapsed() < IDLE_CHECK;
        let alive = fresh
            || ::redis::cmd("PING")
                .query_async::<::redis::Value>(&mut entry.conn)
                .await
                .is_ok();
        if alive {
            entry.last_used = Instant::now();
            return Ok(entry.conn.clone());
        }
        pool.remove(&url);
    }
    // Dial without holding the lock for the whole handshake would let two
    // commands race to open two connections; holding it is the simpler bound,
    // and it only ever waits on the first connect for a given host.
    let conn = dial(&url).await?;
    pool.insert(url, Cached { conn: conn.clone(), last_used: Instant::now() });
    Ok(conn)
}

/// Drop the cached connection for a config (disconnect, or a failed PING in
/// `ping`), so the next command dials fresh.
pub async fn forget(cfg: &RedisConfig) {
    POOL.lock().await.remove(&url_for(cfg));
}

/// Connectivity/credential check - issues a `PING`.
pub async fn ping(cfg: &RedisConfig) -> Result<(), String> {
    let mut conn = open(cfg).await?;
    let r = ::redis::cmd("PING")
        .query_async::<::redis::Value>(&mut conn)
        .await
        .map(|_| ())
        .map_err(|e| format!("Redis PING failed: {e}"));
    if r.is_err() {
        forget(cfg).await;
    }
    r
}

// ── Raw command execution ─────────────────────────────────────────────────────

/// Run a raw Redis command string (e.g. `GET foo`, `KEYS *`, `INFO`) and return
/// the reply as a single-column result set. The reply is expanded so that array
/// replies produce one row per element; scalar replies produce a single row.
pub async fn query(cfg: &RedisConfig, command: &str) -> Result<SqlResult, String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err("Empty Redis command".into());
    }

    // Tokenize with redis-cli-style quoting so values containing spaces work
    // (e.g. `SET greeting "hello world"`, `HSET u:1 bio 'a b c'`).
    let tokens = tokenize(trimmed);
    let cmd_name = tokens.first().ok_or("Empty Redis command")?;

    let t0 = Instant::now();
    let mut conn = open(cfg).await?;
    let mut cmd = ::redis::cmd(cmd_name.as_str());
    for arg in &tokens[1..] {
        cmd.arg(arg.as_str());
    }

    let reply: ::redis::Value = cmd
        .query_async(&mut conn)
        .await
        .map_err(|e| format!("Redis command failed: {e}"))?;
    let elapsed = t0.elapsed().as_millis() as u64;

    let rows = value_to_rows(&reply);
    let row_count = Some(rows.len() as i64);
    Ok(SqlResult {
        columns: vec![ColumnInfo::new("result", "string")],
        rows,
        row_count,
        message: None,
        query_ms: elapsed,
        sql: command.to_string(),
    })
}

/// One page of a `SCAN` iteration: the opaque cursor to resume from (`"0"` once
/// iteration completes) plus the keys found this round.
#[derive(serde::Serialize)]
pub struct ScanReply {
    pub cursor: String,
    pub keys: Vec<String>,
}

/// Non-blocking incremental key iteration via `SCAN` - unlike `KEYS *`, which
/// blocks the whole server on a large keyspace. Binary/control bytes in key
/// names stay legible via `bytes_to_display`. Pass the returned `cursor` back to
/// fetch the next page; `"0"` means iteration is complete.
pub async fn scan(
    cfg: &RedisConfig,
    cursor: &str,
    pattern: Option<String>,
    count: u32,
) -> Result<ScanReply, String> {
    let mut conn = open(cfg).await?;
    let mut cmd = ::redis::cmd("SCAN");
    cmd.arg(cursor);
    if let Some(p) = pattern
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty() && *p != "*")
    {
        cmd.arg("MATCH").arg(p);
    }
    cmd.arg("COUNT").arg(count.max(1));
    let (next, raw): (String, Vec<Vec<u8>>) = cmd
        .query_async(&mut conn)
        .await
        .map_err(|e| format!("Redis SCAN failed: {e}"))?;
    Ok(ScanReply {
        cursor: next,
        keys: raw.iter().map(|b| bytes_to_display(b)).collect(),
    })
}

/// Split a command line into tokens, honoring single/double quotes and
/// backslash escapes inside double quotes (mirrors how redis-cli parses input).
/// Unquoted runs of whitespace separate tokens; quotes group a value even when
/// it contains spaces. Empty quoted strings (`""`) yield an empty token.
fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut has_token = false;
    let mut in_single = false;
    let mut in_double = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double => {
                in_single = !in_single;
                has_token = true;
            }
            '"' if !in_single => {
                in_double = !in_double;
                has_token = true;
            }
            '\\' if in_double => {
                if let Some(&next) = chars.peek() {
                    cur.push(next);
                    chars.next();
                }
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if has_token {
                    tokens.push(std::mem::take(&mut cur));
                    has_token = false;
                }
            }
            c => {
                cur.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        tokens.push(cur);
    }
    tokens
}

/// Expand a top-level reply into rows for the single "result" column:
/// `Nil` → no rows; array/set → one row per element; anything else → one row.
fn value_to_rows(v: &::redis::Value) -> Vec<Vec<JsonValue>> {
    match v {
        ::redis::Value::Nil => Vec::new(),
        ::redis::Value::Array(items) | ::redis::Value::Set(items) => items
            .iter()
            .map(|item| vec![JsonValue::String(value_to_string(item))])
            .collect(),
        other => vec![vec![JsonValue::String(value_to_string(other))]],
    }
}

/// Render a bulk-string reply so binary / control bytes stay legible instead of
/// collapsing into tofu boxes (the old `from_utf8_lossy` turned every non-text
/// byte into U+FFFD). Clean UTF-8 text is returned verbatim; anything with
/// non-whitespace control bytes or invalid UTF-8 keeps its printable ASCII and
/// escapes the rest as `\xNN` (redis-cli style), so `celery\x00\x00\x00celery`
/// reads as exactly that.
fn bytes_to_display(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        if !s
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            return s.to_string();
        }
    }
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        if b == b'\n' || b == b'\r' || b == b'\t' || (0x20..=0x7e).contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("\\x{b:02x}"));
        }
    }
    out
}

/// Stringify a single Redis reply value (recursing into nested containers).
fn value_to_string(v: &::redis::Value) -> String {
    match v {
        ::redis::Value::Nil => String::new(),
        ::redis::Value::Int(i) => i.to_string(),
        ::redis::Value::BulkString(bytes) => bytes_to_display(bytes),
        ::redis::Value::SimpleString(s) => s.clone(),
        ::redis::Value::Okay => "OK".to_string(),
        ::redis::Value::Double(d) => d.to_string(),
        ::redis::Value::Boolean(b) => b.to_string(),
        ::redis::Value::VerbatimString { text, .. } => text.clone(),
        ::redis::Value::Array(items) | ::redis::Value::Set(items) => {
            let inner: Vec<String> = items.iter().map(value_to_string).collect();
            format!("[{}]", inner.join(", "))
        }
        ::redis::Value::Map(pairs) => {
            let inner: Vec<String> = pairs
                .iter()
                .map(|(k, val)| format!("{}: {}", value_to_string(k), value_to_string(val)))
                .collect();
            format!("{{{}}}", inner.join(", "))
        }
        // Push, Attribute, BigNumber, ServerError, and any future (non_exhaustive)
        // variants fall back to the crate's Debug rendering.
        other => format!("{other:?}"),
    }
}

// ── SQL-shaped stubs (not supported on Redis in phase 1) ──────────────────────
// Redis is a key/value store, not a relational engine. These satisfy the shared
// data contract by returning empty/not-supported results so the dispatch enums
// compile. Keyspace browsing is intentionally deferred to a later phase.

pub async fn list_tables(_cfg: &RedisConfig) -> Result<Vec<TableInfo>, String> {
    Ok(vec![])
}

pub async fn list_indexes(_cfg: &RedisConfig) -> Result<Vec<IndexInfo>, String> {
    Ok(vec![])
}

pub async fn get_column_structure(
    _cfg: &RedisConfig,
    _table: &str,
) -> Result<Vec<ColumnStructureRow>, String> {
    Ok(vec![])
}

pub async fn get_ddl(_cfg: &RedisConfig, _table: &str) -> Result<String, String> {
    Ok(String::new())
}

#[allow(clippy::too_many_arguments)]
pub async fn get_table_rows(
    _cfg: &RedisConfig,
    _table: &str,
    _limit: i64,
    _offset: i64,
    _search: Option<String>,
    _sort_column: Option<String>,
    _sort_direction: Option<String>,
    _filters: Option<Vec<RowFilter>>,
    _include_meta: bool,
) -> Result<TableRows, String> {
    Err("Row browsing is not supported on Redis yet".into())
}
