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

// ── Keys table ────────────────────────────────────────────────────────────────
// The keys page is a grid: one row per key with its type, TTL, size, length and
// a short value preview. Everything for a page of keys is fetched in two
// pipelined round trips (metadata, then per-type length + preview) instead of
// several commands per key.

/// A key as the frontend names it: the display string plus whether it was
/// escaped from non-UTF-8 bytes (see `bytes_to_display`), so writes can target
/// the exact original bytes.
#[derive(serde::Deserialize, Clone)]
pub struct KeyRef {
    pub key: String,
    #[serde(default)]
    pub binary: bool,
}

impl KeyRef {
    fn bytes(&self) -> Vec<u8> {
        if self.binary { display_to_bytes(&self.key) } else { self.key.as_bytes().to_vec() }
    }
}

/// One row of the keys table.
#[derive(serde::Serialize)]
pub struct KeyRow {
    pub key: String,
    /// The key name held bytes that aren't clean text; `key` is the escaped form.
    pub binary: bool,
    /// Redis type (`string`, `hash`, …, or a module type such as `ReJSON-RL`);
    /// `none` when the key vanished between SCAN and TYPE.
    pub r#type: String,
    /// Milliseconds to expiry; `-1` no expiry, `-2` gone.
    pub ttl_ms: i64,
    /// `MEMORY USAGE` bytes; `None` where the server refuses it (some hosts).
    pub memory: Option<i64>,
    /// Characters for a string, elements for a collection.
    pub length: Option<i64>,
    pub preview: String,
    /// The preview doesn't show the whole value.
    pub truncated: bool,
}

#[derive(serde::Serialize)]
pub struct KeyPage {
    /// Cursor to continue from; `"0"` once the keyspace is exhausted.
    pub cursor: String,
    pub rows: Vec<KeyRow>,
    /// Keys SCAN looked at this call (before TYPE filtering on old servers).
    pub scanned: usize,
}

const PREVIEW_ITEMS: usize = 6;
const PREVIEW_CHARS: usize = 160;
const ITEM_CHARS: usize = 48;

fn clip(s: &str, max: usize) -> (String, bool) {
    if s.chars().count() <= max {
        return (s.to_string(), false);
    }
    (s.chars().take(max).collect::<String>() + "…", true)
}

fn as_i64(v: &::redis::Value) -> Option<i64> {
    match v {
        ::redis::Value::Int(i) => Some(*i),
        ::redis::Value::BulkString(b) => std::str::from_utf8(b).ok()?.trim().parse().ok(),
        ::redis::Value::SimpleString(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn as_items(v: &::redis::Value) -> &[::redis::Value] {
    match v {
        ::redis::Value::Array(items) | ::redis::Value::Set(items) => items,
        _ => &[],
    }
}

/// HSCAN / SSCAN reply: `[cursor, [elements…]]`.
fn scan_reply(v: &::redis::Value) -> (String, &[::redis::Value]) {
    let parts = as_items(v);
    let cursor = parts.first().map(value_to_string).unwrap_or_else(|| "0".into());
    (cursor, parts.get(1).map(as_items).unwrap_or(&[]))
}

/// Inverse of `bytes_to_display` for keys flagged binary: `\xNN` → byte,
/// `\\` → backslash, everything else verbatim.
fn display_to_bytes(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            if b[i + 1] == b'\\' {
                out.push(b'\\');
                i += 2;
                continue;
            }
            if b[i + 1] == b'x' && i + 3 < b.len() {
                if let Ok(byte) = u8::from_str_radix(&s[i + 2..i + 4], 16) {
                    out.push(byte);
                    i += 4;
                    continue;
                }
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// Whether a key's bytes need escaping to display (so it round-trips as binary).
fn is_binary(bytes: &[u8]) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
        Err(_) => true,
    }
}

fn join_preview(items: impl Iterator<Item = String>, total: Option<i64>, open: &str, close: &str) -> (String, bool) {
    let parts: Vec<String> = items.take(PREVIEW_ITEMS).collect();
    let shown = parts.len() as i64;
    let more = total.map(|t| t > shown).unwrap_or(false);
    let body = parts.join(", ");
    let (body, cut) = clip(&body, PREVIEW_CHARS);
    (format!("{open}{body}{}{close}", if more { ", …" } else { "" }), more || cut)
}

fn item(v: &::redis::Value) -> String {
    clip(&value_to_string(v), ITEM_CHARS).0
}

/// Type, TTL, memory, length and a value preview for each key, in two
/// pipelined round trips. Per-command failures (e.g. `MEMORY USAGE` disabled on
/// a hosted Redis) degrade that field instead of failing the page.
async fn describe(
    conn: &mut ::redis::aio::MultiplexedConnection,
    keys: &[Vec<u8>],
) -> Result<Vec<KeyRow>, String> {
    if keys.is_empty() {
        return Ok(vec![]);
    }
    let mut meta = ::redis::pipe();
    meta.ignore_errors();
    for k in keys {
        meta.cmd("TYPE").arg(k);
        meta.cmd("PTTL").arg(k);
        meta.cmd("MEMORY").arg("USAGE").arg(k);
    }
    let meta: Vec<::redis::RedisResult<::redis::Value>> = meta
        .query_async(conn)
        .await
        .map_err(|e| format!("Redis key lookup failed: {e}"))?;

    let types: Vec<String> = (0..keys.len())
        .map(|i| match &meta[i * 3] {
            Ok(v) => value_to_string(v),
            Err(_) => String::new(),
        })
        .collect();

    // Round two: one length command and one preview command per key.
    let mut detail = ::redis::pipe();
    detail.ignore_errors();
    for (k, t) in keys.iter().zip(&types) {
        match t.as_str() {
            "string" => {
                detail.cmd("STRLEN").arg(k);
                detail.cmd("GETRANGE").arg(k).arg(0).arg(PREVIEW_CHARS as i64 * 4);
            }
            "hash" => {
                detail.cmd("HLEN").arg(k);
                detail.cmd("HSCAN").arg(k).arg(0).arg("COUNT").arg(PREVIEW_ITEMS);
            }
            "list" => {
                detail.cmd("LLEN").arg(k);
                detail.cmd("LRANGE").arg(k).arg(0).arg(PREVIEW_ITEMS as i64 - 1);
            }
            "set" => {
                detail.cmd("SCARD").arg(k);
                detail.cmd("SSCAN").arg(k).arg(0).arg("COUNT").arg(PREVIEW_ITEMS);
            }
            "zset" => {
                detail.cmd("ZCARD").arg(k);
                detail.cmd("ZRANGE").arg(k).arg(0).arg(PREVIEW_ITEMS as i64 - 1).arg("WITHSCORES");
            }
            "stream" => {
                detail.cmd("XLEN").arg(k);
                detail.cmd("XREVRANGE").arg(k).arg("+").arg("-").arg("COUNT").arg(1);
            }
            // Unknown/module types: keep the indexes aligned with two PINGs.
            _ => {
                detail.cmd("PING");
                detail.cmd("PING");
            }
        }
    }
    let detail: Vec<::redis::RedisResult<::redis::Value>> = detail
        .query_async(conn)
        .await
        .map_err(|e| format!("Redis key lookup failed: {e}"))?;

    let mut rows = Vec::with_capacity(keys.len());
    for (i, k) in keys.iter().enumerate() {
        let t = types[i].clone();
        let ttl_ms = meta[i * 3 + 1].as_ref().ok().and_then(as_i64).unwrap_or(-1);
        let memory = meta[i * 3 + 2].as_ref().ok().and_then(as_i64);
        let length = detail[i * 2].as_ref().ok().and_then(as_i64);
        let body = detail[i * 2 + 1].as_ref().ok();
        let (preview, truncated) = match (t.as_str(), body) {
            ("string", Some(v)) => {
                let (p, cut) = clip(&value_to_string(v), PREVIEW_CHARS);
                (p, cut || length.map(|n| n > PREVIEW_CHARS as i64).unwrap_or(false))
            }
            ("hash", Some(v)) => {
                let els = scan_reply(v).1;
                let pairs = els.chunks(2).map(|p| {
                    format!("{}: {}", item(&p[0]), p.get(1).map(item).unwrap_or_default())
                });
                join_preview(pairs, length, "{", "}")
            }
            ("list", Some(v)) => join_preview(as_items(v).iter().map(item), length, "[", "]"),
            ("set", Some(v)) => join_preview(scan_reply(v).1.iter().map(item), length, "{", "}"),
            ("zset", Some(v)) => {
                let pairs = as_items(v).chunks(2).map(|p| {
                    format!("{} ({})", item(&p[0]), p.get(1).map(value_to_string).unwrap_or_default())
                });
                join_preview(pairs, length, "[", "]")
            }
            ("stream", Some(v)) => {
                // XREVRANGE reply: [[id, [f, v, …]]]
                let last = as_items(v).first().map(as_items).unwrap_or(&[]);
                let id = last.first().map(value_to_string).unwrap_or_default();
                let fields = last.get(1).map(as_items).unwrap_or(&[]);
                let pairs = fields.chunks(2).map(|p| {
                    format!("{}: {}", item(&p[0]), p.get(1).map(item).unwrap_or_default())
                });
                let (body, cut) = join_preview(pairs, None, "{", "}");
                if id.is_empty() { (String::new(), false) } else { (format!("{id} {body}"), cut || length.unwrap_or(0) > 1) }
            }
            _ => (String::new(), false),
        };
        rows.push(KeyRow {
            key: bytes_to_display(k),
            binary: is_binary(k),
            r#type: if t.is_empty() { "unknown".into() } else { t },
            ttl_ms,
            memory,
            length,
            preview,
            truncated,
        });
    }
    Ok(rows)
}

/// One page of the keys table: SCAN (with optional `MATCH` and `TYPE`) until
/// about `target` keys are found, the keyspace is exhausted, or a bounded
/// number of SCAN calls have run (sparse matches on a huge keyspace shouldn't
/// pin the server), then describe them.
pub async fn scan_rows(
    cfg: &RedisConfig,
    cursor: &str,
    pattern: Option<String>,
    type_filter: Option<String>,
    target: u32,
) -> Result<KeyPage, String> {
    let mut conn = open(cfg).await?;
    let pattern = pattern
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty() && p != "*");
    let type_filter = type_filter.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let target = target.clamp(1, 2000) as usize;
    let mut cursor = cursor.to_string();
    let mut seen = std::collections::HashSet::new();
    let mut keys: Vec<Vec<u8>> = Vec::new();
    let mut scanned = 0usize;
    for _ in 0..40 {
        let mut cmd = ::redis::cmd("SCAN");
        cmd.arg(&cursor);
        if let Some(p) = &pattern {
            cmd.arg("MATCH").arg(p);
        }
        // COUNT is the work per call; MATCH filters after it, so a sparse
        // pattern needs a bigger stride to fill a page in few round trips.
        cmd.arg("COUNT").arg(if pattern.is_some() || type_filter.is_some() { 1000 } else { 500 });
        if let Some(t) = &type_filter {
            cmd.arg("TYPE").arg(t);
        }
        let (next, raw): (String, Vec<Vec<u8>>) = cmd
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Redis SCAN failed: {e}"))?;
        scanned += raw.len();
        for k in raw {
            // SCAN may return a key more than once across calls.
            if seen.insert(k.clone()) {
                keys.push(k);
            }
        }
        cursor = next;
        if cursor == "0" || keys.len() >= target {
            break;
        }
    }
    let rows = describe(&mut conn, &keys).await?;
    Ok(KeyPage { cursor, rows, scanned })
}

/// Fresh rows for specific keys (after an apply), in the same shape as a page.
pub async fn key_rows(cfg: &RedisConfig, keys: Vec<KeyRef>) -> Result<Vec<KeyRow>, String> {
    let mut conn = open(cfg).await?;
    let raw: Vec<Vec<u8>> = keys.iter().map(KeyRef::bytes).collect();
    describe(&mut conn, &raw).await
}

/// One element of a collection value.
#[derive(serde::Serialize)]
pub struct ValueEntry {
    /// Hash field, or stream entry id.
    pub field: Option<String>,
    pub value: String,
    /// Sorted-set score.
    pub score: Option<String>,
    /// List index.
    pub index: Option<i64>,
}

#[derive(serde::Serialize)]
pub struct KeyValue {
    pub r#type: String,
    pub ttl_ms: i64,
    pub memory: Option<i64>,
    pub encoding: String,
    pub length: Option<i64>,
    /// Whole string value (capped at `STRING_CAP` bytes).
    pub text: Option<String>,
    pub text_truncated: bool,
    pub entries: Vec<ValueEntry>,
    /// Cursor/offset for the next page of `entries`; `None` when done.
    pub next: Option<String>,
}

const STRING_CAP: i64 = 512 * 1024;

/// A key's value for the side panel, paged for collections: `cursor` is an
/// HSCAN/SSCAN cursor, a list/zset offset, or the last stream id seen.
pub async fn get_value(cfg: &RedisConfig, key: KeyRef, cursor: Option<String>, count: u32) -> Result<KeyValue, String> {
    let mut conn = open(cfg).await?;
    let k = key.bytes();
    let count = count.clamp(1, 5000) as i64;
    let mut meta = ::redis::pipe();
    meta.ignore_errors();
    meta.cmd("TYPE").arg(&k);
    meta.cmd("PTTL").arg(&k);
    meta.cmd("MEMORY").arg("USAGE").arg(&k);
    meta.cmd("OBJECT").arg("ENCODING").arg(&k);
    let meta: Vec<::redis::RedisResult<::redis::Value>> = meta
        .query_async(&mut conn)
        .await
        .map_err(|e| format!("Redis read failed: {e}"))?;
    let t = meta[0].as_ref().map(value_to_string).unwrap_or_default();
    if t == "none" {
        return Err("Key does not exist (it may have expired)".into());
    }
    let mut out = KeyValue {
        r#type: t.clone(),
        ttl_ms: meta[1].as_ref().ok().and_then(as_i64).unwrap_or(-1),
        memory: meta[2].as_ref().ok().and_then(as_i64),
        encoding: meta[3].as_ref().map(value_to_string).unwrap_or_default(),
        length: None,
        text: None,
        text_truncated: false,
        entries: vec![],
        next: None,
    };
    let err = |e: ::redis::RedisError| format!("Redis read failed: {e}");
    let offset = cursor.as_deref().and_then(|c| c.parse::<i64>().ok()).unwrap_or(0);
    match t.as_str() {
        "string" => {
            let len: i64 = ::redis::cmd("STRLEN").arg(&k).query_async(&mut conn).await.map_err(err)?;
            let v: ::redis::Value = ::redis::cmd("GETRANGE").arg(&k).arg(0).arg(STRING_CAP - 1).query_async(&mut conn).await.map_err(err)?;
            out.length = Some(len);
            out.text = Some(value_to_string(&v));
            out.text_truncated = len > STRING_CAP;
        }
        "hash" | "set" => {
            let (len_cmd, scan_cmd) = if t == "hash" { ("HLEN", "HSCAN") } else { ("SCARD", "SSCAN") };
            out.length = ::redis::cmd(len_cmd).arg(&k).query_async::<i64>(&mut conn).await.ok();
            let c = cursor.unwrap_or_else(|| "0".into());
            let v: ::redis::Value = ::redis::cmd(scan_cmd).arg(&k).arg(&c).arg("COUNT").arg(count)
                .query_async(&mut conn).await.map_err(err)?;
            let (next, els) = scan_reply(&v);
            if t == "hash" {
                for p in els.chunks(2) {
                    out.entries.push(ValueEntry {
                        field: Some(value_to_string(&p[0])),
                        value: p.get(1).map(value_to_string).unwrap_or_default(),
                        score: None,
                        index: None,
                    });
                }
            } else {
                for e in els {
                    out.entries.push(ValueEntry { field: None, value: value_to_string(e), score: None, index: None });
                }
            }
            out.next = (next != "0").then_some(next);
        }
        "list" => {
            out.length = ::redis::cmd("LLEN").arg(&k).query_async::<i64>(&mut conn).await.ok();
            let v: ::redis::Value = ::redis::cmd("LRANGE").arg(&k).arg(offset).arg(offset + count - 1)
                .query_async(&mut conn).await.map_err(err)?;
            let els = as_items(&v);
            for (i, e) in els.iter().enumerate() {
                out.entries.push(ValueEntry { field: None, value: value_to_string(e), score: None, index: Some(offset + i as i64) });
            }
            let end = offset + els.len() as i64;
            out.next = (out.length.map(|n| end < n).unwrap_or(false)).then(|| end.to_string());
        }
        "zset" => {
            out.length = ::redis::cmd("ZCARD").arg(&k).query_async::<i64>(&mut conn).await.ok();
            let v: ::redis::Value = ::redis::cmd("ZRANGE").arg(&k).arg(offset).arg(offset + count - 1).arg("WITHSCORES")
                .query_async(&mut conn).await.map_err(err)?;
            let els = as_items(&v);
            for p in els.chunks(2) {
                out.entries.push(ValueEntry {
                    field: None,
                    value: value_to_string(&p[0]),
                    score: p.get(1).map(value_to_string),
                    index: None,
                });
            }
            let end = offset + (els.len() / 2) as i64;
            out.next = (out.length.map(|n| end < n).unwrap_or(false)).then(|| end.to_string());
        }
        "stream" => {
            out.length = ::redis::cmd("XLEN").arg(&k).query_async::<i64>(&mut conn).await.ok();
            // Exclusive start after the last id of the previous page.
            let start = cursor.map(|c| format!("({c}")).unwrap_or_else(|| "-".into());
            let v: ::redis::Value = ::redis::cmd("XRANGE").arg(&k).arg(&start).arg("+").arg("COUNT").arg(count)
                .query_async(&mut conn).await.map_err(err)?;
            let els = as_items(&v);
            for e in els {
                let parts = as_items(e);
                let id = parts.first().map(value_to_string).unwrap_or_default();
                let fields = parts.get(1).map(as_items).unwrap_or(&[]);
                let obj: serde_json::Map<String, JsonValue> = fields
                    .chunks(2)
                    .map(|p| (value_to_string(&p[0]), JsonValue::String(p.get(1).map(value_to_string).unwrap_or_default())))
                    .collect();
                out.entries.push(ValueEntry { field: Some(id), value: JsonValue::Object(obj).to_string(), score: None, index: None });
            }
            out.next = (els.len() as i64 == count).then(|| out.entries.last().and_then(|e| e.field.clone())).flatten();
        }
        _ => {}
    }
    Ok(out)
}

/// One staged write: the command and its arguments. `binary_key` marks that
/// `args[1]` is an escaped binary key name to decode back to bytes.
#[derive(serde::Deserialize)]
pub struct RedisOp {
    pub args: Vec<String>,
    #[serde(default)]
    pub binary_key: bool,
}

#[derive(serde::Serialize)]
pub struct OpOutcome {
    pub ok: bool,
    pub error: Option<String>,
}

/// Run staged writes as one `MULTI`/`EXEC` transaction so no other client sees
/// a half-applied set. Redis doesn't roll back a command that fails at run time
/// (e.g. WRONGTYPE), so the outcome of every op is returned: the caller keeps
/// the failed ones staged and reports them.
pub async fn apply(cfg: &RedisConfig, ops: Vec<RedisOp>) -> Result<Vec<OpOutcome>, String> {
    if ops.is_empty() {
        return Ok(vec![]);
    }
    let mut conn = open(cfg).await?;
    let mut pipe = ::redis::pipe();
    pipe.atomic().ignore_errors();
    for op in &ops {
        let Some(name) = op.args.first() else {
            return Err("Empty Redis command".into());
        };
        let mut cmd = ::redis::cmd(name);
        for (i, a) in op.args.iter().enumerate().skip(1) {
            if i == 1 && op.binary_key {
                cmd.arg(display_to_bytes(a));
            } else {
                cmd.arg(a.as_str());
            }
        }
        pipe.add_command(cmd);
    }
    let results: Vec<::redis::RedisResult<::redis::Value>> = pipe
        .query_async(&mut conn)
        .await
        .map_err(|e| format!("Redis transaction failed, nothing was applied: {e}"))?;
    Ok(results
        .into_iter()
        .zip(&ops)
        .map(|(r, op)| match r {
            Ok(::redis::Value::ServerError(e)) => OpOutcome { ok: false, error: Some(format!("{e:?}")) },
            // RENAMENX reports a taken target name as 0, not as an error.
            Ok(::redis::Value::Int(0)) if op.args[0].eq_ignore_ascii_case("RENAMENX") => OpOutcome {
                ok: false,
                error: Some(format!("A key named \"{}\" already exists", op.args.get(2).map(String::as_str).unwrap_or(""))),
            },
            Ok(_) => OpOutcome { ok: true, error: None },
            Err(e) => OpOutcome { ok: false, error: Some(e.to_string()) },
        })
        .collect())
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
/// Run one command (already split into args) and return the reply as JSON,
/// keeping nesting (SLOWLOG GET, CLIENT LIST, INFO...). For the insight views;
/// not recorded in query history.
pub async fn command_json(cfg: &RedisConfig, args: Vec<String>) -> Result<JsonValue, String> {
    let name = args.first().ok_or("Empty Redis command")?;
    let mut conn = open(cfg).await?;
    let mut cmd = ::redis::cmd(name.as_str());
    for a in &args[1..] {
        cmd.arg(a.as_str());
    }
    let reply: ::redis::Value = cmd.query_async(&mut conn).await.map_err(|e| format!("{name} failed: {e}"))?;
    Ok(value_to_json(&reply))
}

// ── Pub/Sub subscriptions ─────────────────────────────────────────────────────
// One background task per subscription id, each on its own dedicated pub/sub
// connection (a subscribed connection can't run normal commands). Messages are
// handed to `emit`; `unsubscribe` aborts the task, which drops the connection.

static SUBS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, tokio::task::AbortHandle>>> =
    std::sync::LazyLock::new(Default::default);

#[derive(serde::Serialize, Clone)]
pub struct PubSubMessage {
    pub channel: String,
    pub pattern: Option<String>,
    pub payload: String,
    pub at: u64,
}

pub async fn subscribe<F>(cfg: &RedisConfig, id: String, channels: Vec<String>, patterns: Vec<String>, emit: F) -> Result<(), String>
where
    F: Fn(PubSubMessage) + Send + 'static,
{
    if channels.is_empty() && patterns.is_empty() {
        return Err("Name a channel or a pattern to subscribe to".into());
    }
    let client = ::redis::Client::open(url_for(cfg)).map_err(|e| format!("Redis connection failed: {e}"))?;
    let mut ps = client.get_async_pubsub().await.map_err(|e| format!("Could not open a pub/sub connection: {e}"))?;
    for c in &channels {
        ps.subscribe(c).await.map_err(|e| format!("SUBSCRIBE {c} failed: {e}"))?;
    }
    for p in &patterns {
        ps.psubscribe(p).await.map_err(|e| format!("PSUBSCRIBE {p} failed: {e}"))?;
    }
    unsubscribe(&id);
    let task = tokio::spawn(async move {
        use futures::StreamExt;
        let mut stream = ps.into_on_message();
        while let Some(msg) = stream.next().await {
            let payload: Vec<u8> = msg.get_payload().unwrap_or_default();
            emit(PubSubMessage {
                channel: msg.get_channel_name().to_string(),
                pattern: msg.get_pattern::<String>().ok(),
                payload: bytes_to_display(&payload),
                at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
            });
        }
    });
    SUBS.lock().map_err(|_| "Subscription registry poisoned")?.insert(id, task.abort_handle());
    Ok(())
}

pub fn unsubscribe(id: &str) {
    if let Ok(mut m) = SUBS.lock() {
        if let Some(h) = m.remove(id) {
            h.abort();
        }
    }
}

fn value_to_json(v: &::redis::Value) -> JsonValue {
    match v {
        ::redis::Value::Nil => JsonValue::Null,
        ::redis::Value::Int(i) => JsonValue::from(*i),
        ::redis::Value::Double(d) => JsonValue::from(*d),
        ::redis::Value::Boolean(b) => JsonValue::Bool(*b),
        ::redis::Value::Array(items) | ::redis::Value::Set(items) => JsonValue::Array(items.iter().map(value_to_json).collect()),
        ::redis::Value::Map(pairs) => {
            let mut m = serde_json::Map::new();
            for (k, val) in pairs {
                m.insert(value_to_string(k), value_to_json(val));
            }
            JsonValue::Object(m)
        }
        other => JsonValue::String(value_to_string(other)),
    }
}

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
        // A literal backslash is doubled so `display_to_bytes` can tell it
        // apart from a `\xNN` escape.
        if b == b'\\' {
            out.push_str("\\\\");
        } else if b == b'\n' || b == b'\r' || b == b'\t' || (0x20..=0x7e).contains(&b) {
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

/// Live tests against `dev/nosql` (`docker compose up -d redis`, port 6391).
/// Run with `cargo test --lib redis_live -- --ignored --test-threads=1`.
#[cfg(test)]
mod redis_live {
    use super::*;

    fn cfg() -> RedisConfig {
        RedisConfig {
            name: "it".into(),
            host: "127.0.0.1".into(),
            port: std::env::var("STROKE_REDIS_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(6391),
            password: None,
            db: 14,
            tls: false,
        }
    }
    fn op(args: &[&str]) -> RedisOp {
        RedisOp { args: args.iter().map(|s| s.to_string()).collect(), binary_key: false }
    }
    fn k(s: &str) -> KeyRef {
        KeyRef { key: s.into(), binary: false }
    }

    #[tokio::test]
    #[ignore]
    async fn staged_edits_round_trip() {
        let c = cfg();
        apply(&c, vec![op(&["FLUSHDB"])]).await.unwrap();

        // Creates, as createOps builds them.
        let out = apply(&c, vec![
            op(&["SET", "s", "hello"]),
            op(&["EXPIRE", "s", "300"]),
            op(&["HSET", "h", "a", "1"]),
            op(&["RPUSH", "l", "x", "y", "z"]),
            op(&["SADD", "st", "m1", "m2"]),
            op(&["ZADD", "z", "2", "two", "1", "one"]),
        ]).await.unwrap();
        assert!(out.iter().all(|o| o.ok), "{:?}", out.iter().map(|o| &o.error).collect::<Vec<_>>());

        let page = scan_rows(&c, "0", None, None, 100).await.unwrap();
        let mut names: Vec<_> = page.rows.iter().map(|r| (r.key.as_str(), r.r#type.as_str())).collect();
        names.sort();
        assert_eq!(names, [("h", "hash"), ("l", "list"), ("s", "string"), ("st", "set"), ("z", "zset")]);

        let v = get_value(&c, k("s"), None, 100).await.unwrap();
        assert_eq!(v.text.as_deref(), Some("hello"));
        assert!(v.ttl_ms > 0 && v.ttl_ms <= 300_000);

        // Entry edits, a list delete via tombstone + LREM, a rename, a delete.
        let tomb = "__stroke_deleted_element__";
        let out = apply(&c, vec![
            op(&["HSET", "h", "b", "2"]),
            op(&["HDEL", "h", "a"]),
            op(&["LSET", "l", "1", tomb]),
            op(&["LSET", "l", "0", "X"]),
            op(&["SREM", "st", "m1"]),
            op(&["ZADD", "z", "5", "one"]),
            op(&["PERSIST", "s"]),
            op(&["LREM", "l", "0", tomb]),
            op(&["RENAMENX", "st", "st2"]),
            op(&["DEL", "z"]),
        ]).await.unwrap();
        assert!(out.iter().all(|o| o.ok), "{:?}", out.iter().map(|o| &o.error).collect::<Vec<_>>());

        let h = get_value(&c, k("h"), None, 100).await.unwrap();
        assert_eq!(h.entries.iter().map(|e| (e.field.clone().unwrap(), e.value.clone())).collect::<Vec<_>>(), [("b".to_string(), "2".to_string())]);
        let l = get_value(&c, k("l"), None, 100).await.unwrap();
        assert_eq!(l.entries.iter().map(|e| e.value.as_str()).collect::<Vec<_>>(), ["X", "z"]);
        assert_eq!(get_value(&c, k("s"), None, 10).await.unwrap().ttl_ms, -1);
        let st = get_value(&c, k("st2"), None, 10).await.unwrap();
        assert_eq!(st.entries.iter().map(|e| e.value.as_str()).collect::<Vec<_>>(), ["m2"]);
        assert!(get_value(&c, k("z"), None, 10).await.is_err(), "deleted key should be gone");

        // A taken rename target and a wrong-type write report per op.
        let out = apply(&c, vec![op(&["RENAMENX", "h", "l"]), op(&["HSET", "l", "f", "v"]), op(&["SET", "ok", "1"])]).await.unwrap();
        assert!(!out[0].ok && out[0].error.as_deref().unwrap().contains("already exists"));
        assert!(!out[1].ok, "WRONGTYPE should surface");
        assert!(out[2].ok);

        // Paging through a collection bigger than one page.
        let mut big = vec!["HSET".to_string(), "big".into()];
        for i in 0..1200 { big.push(format!("f{i}")); big.push(i.to_string()); }
        apply(&c, vec![RedisOp { args: big, binary_key: false }]).await.unwrap();
        let (mut seen, mut cur) = (std::collections::HashSet::new(), None);
        loop {
            let p = get_value(&c, k("big"), cur, 500).await.unwrap();
            seen.extend(p.entries.into_iter().map(|e| e.field.unwrap()));
            match p.next { Some(n) => cur = Some(n), None => break }
        }
        assert_eq!(seen.len(), 1200);

        apply(&c, vec![op(&["FLUSHDB"])]).await.unwrap();
    }
}
