//! Snowflake over its SQL REST API (v2). Beta.
//!
//! The SQL API takes key-pair authentication (a JWT signed with the user's
//! RSA key, whose public half is registered on the user) or a programmatic
//! access token. It does not take passwords.
//!
//! Metadata comes from SHOW and DESCRIBE, which run in Snowflake's cloud
//! services without a warehouse: schemas, tables with their row counts,
//! columns with their primary keys, and foreign keys both ways. Queries and
//! the grid need the connection's warehouse.
//!
//! Every statement is submitted asynchronously and its handle kept, so Stop
//! cancels it on Snowflake; results arrive in partitions, which the console
//! streams. Values in the grid are bound as parameters (`?`).

use super::query::{ColumnInfo, ForeignKeyInfo, RowFilter, SqlResult, TableRows};
use super::schema::{ColumnStructureRow, TableInfo};
use base64::{engine::general_purpose::STANDARD as B64, engine::general_purpose::URL_SAFE_NO_PAD as B64URL, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnowflakeConfig {
    pub name: String,
    /// The account identifier: `myorg-myaccount`, or a locator such as
    /// `xy12345.us-east-2.aws`.
    pub account: String,
    pub user: String,
    /// "keypair" (a private key file) or "token" (a programmatic access token).
    #[serde(default = "default_auth")]
    pub auth: String,
    #[serde(default)]
    pub private_key_file: Option<String>,
    /// The programmatic access token, for `auth = "token"`.
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub warehouse: Option<String>,
    pub database: String,
    /// The schema bare table names mean; PUBLIC when blank.
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    /// The API's base URL, for private link or a test server. Derived from
    /// the account when blank.
    #[serde(default)]
    pub host: Option<String>,
}

fn default_auth() -> String {
    "keypair".into()
}

pub struct SnowflakeConn {
    pub cfg: SnowflakeConfig,
    /// The signed JWT and when it was made; Snowflake takes one for an hour.
    jwt: Mutex<Option<(String, u64)>>,
}

pub type SnowflakeHandle = Arc<SnowflakeConn>;

/// Rows one console statement collects when it does not stream.
const MAX_ROWS: usize = 100_000;

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        crate::proxy::http_builder(crate::proxy::Kind::Database)
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default()
    })
}

fn opt(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn base_url(cfg: &SnowflakeConfig) -> String {
    match opt(&cfg.host) {
        Some(h) if h.contains("://") => h.trim_end_matches('/').to_string(),
        Some(h) => format!("https://{}", h.trim_end_matches('/')),
        None => format!("https://{}.snowflakecomputing.com", cfg.account.trim().to_ascii_lowercase()),
    }
}

// ── Key-pair sign-in ──────────────────────────────────────────────────────────

fn pem_to_der(pem: &str) -> Result<(String, Vec<u8>), String> {
    let label = pem
        .lines()
        .find_map(|l| l.trim().strip_prefix("-----BEGIN ").and_then(|r| r.strip_suffix("-----")))
        .unwrap_or("")
        .to_string();
    let body: String = pem.lines().map(str::trim).filter(|l| !l.starts_with("-----") && !l.is_empty()).collect();
    let der = B64.decode(body).map_err(|e| format!("The private key file is not valid PEM: {e}"))?;
    Ok((label, der))
}

fn der_len(n: usize) -> Vec<u8> {
    if n < 0x80 {
        vec![n as u8]
    } else {
        let bytes: Vec<u8> = n.to_be_bytes().iter().copied().skip_while(|b| *b == 0).collect();
        let mut out = vec![0x80 | bytes.len() as u8];
        out.extend(bytes);
        out
    }
}

/// The SubjectPublicKeyInfo DER of an RSA public key (PKCS#1 RSAPublicKey),
/// which is what Snowflake fingerprints.
fn spki_der(rsa_public_key: &[u8]) -> Vec<u8> {
    // SEQUENCE { OID 1.2.840.113549.1.1.1, NULL }
    let alg: &[u8] = &[0x30, 0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01, 0x05, 0x00];
    let mut bit = vec![0x03];
    bit.extend(der_len(rsa_public_key.len() + 1));
    bit.push(0x00);
    bit.extend_from_slice(rsa_public_key);
    let mut out = vec![0x30];
    out.extend(der_len(alg.len() + bit.len()));
    out.extend_from_slice(alg);
    out.extend(bit);
    out
}

/// The account as the JWT names it: upper case, the locator without its
/// region, and an organization's `org.account` written `org-account`.
fn jwt_account(account: &str) -> String {
    let a = account.trim();
    let a = a.strip_suffix(".snowflakecomputing.com").unwrap_or(a);
    let first = if a.contains('-') && !a.contains('.') { a } else { a.split('.').next().unwrap_or(a) };
    first.replace('.', "-").to_ascii_uppercase()
}

/// A key-pair JWT: `iss` is ACCOUNT.USER.SHA256:<public key fingerprint>.
pub fn sign_jwt(account: &str, user: &str, private_key_pem: &str, iat: u64) -> Result<String, String> {
    let (label, der) = pem_to_der(private_key_pem)?;
    if label.contains("ENCRYPTED") {
        return Err("The private key is encrypted with a passphrase. Export it unencrypted (openssl pkcs8 -topk8 -nocrypt) or use an access token.".into());
    }
    let key = if label.contains("RSA PRIVATE KEY") {
        ring::signature::RsaKeyPair::from_der(&der)
    } else {
        ring::signature::RsaKeyPair::from_pkcs8(&der)
    }
    .map_err(|e| format!("The private key was rejected: {e}. Snowflake key pairs are RSA keys of 2048 bits or more."))?;
    use ring::signature::KeyPair;
    let fp = B64.encode(ring::digest::digest(&ring::digest::SHA256, &spki_der(key.public_key().as_ref())).as_ref());
    let qualified = format!("{}.{}", jwt_account(account), user.trim().to_ascii_uppercase());
    let header = B64URL.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
    let claims = B64URL.encode(json!({ "iss": format!("{qualified}.SHA256:{fp}"), "sub": qualified, "iat": iat, "exp": iat + 3540 }).to_string());
    let input = format!("{header}.{claims}");
    let mut sig = vec![0u8; key.public().modulus_len()];
    key.sign(&ring::signature::RSA_PKCS1_SHA256, &ring::rand::SystemRandom::new(), input.as_bytes(), &mut sig)
        .map_err(|_| "Could not sign with the private key".to_string())?;
    Ok(format!("{input}.{}", B64URL.encode(sig)))
}

/// The Authorization header's token and its type.
fn credentials(h: &SnowflakeConn) -> Result<(String, &'static str), String> {
    if h.cfg.auth == "token" {
        let t = opt(&h.cfg.password).ok_or("Enter the programmatic access token")?;
        return Ok((t.to_string(), "PROGRAMMATIC_ACCESS_TOKEN"));
    }
    let mut g = h.jwt.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((jwt, at)) = g.as_ref() {
        if now() < at + 3000 {
            return Ok((jwt.clone(), "KEYPAIR_JWT"));
        }
    }
    let path = opt(&h.cfg.private_key_file).ok_or("Choose the private key file")?;
    let pem = std::fs::read_to_string(path).map_err(|e| format!("Could not read the private key file {path}: {e}"))?;
    let iat = now();
    let jwt = sign_jwt(&h.cfg.account, &h.cfg.user, &pem, iat)?;
    *g = Some((jwt.clone(), iat));
    Ok((jwt, "KEYPAIR_JWT"))
}

// ── Requests ──────────────────────────────────────────────────────────────────

async fn call(h: &SnowflakeConn, method: reqwest::Method, path: &str, body: Option<&Value>) -> Result<(u16, Value), String> {
    let (token, kind) = credentials(h)?;
    let url = format!("{}{}", base_url(&h.cfg), path);
    let mut req = client()
        .request(method, &url)
        .bearer_auth(token)
        .header("X-Snowflake-Authorization-Token-Type", kind)
        .header("Accept", "application/json")
        .header("User-Agent", "Stroke/1.0");
    if let Some(b) = body {
        req = req.json(b);
    }
    let resp = req.send().await.map_err(|e| {
        if e.is_connect() {
            format!("Could not reach Snowflake at {}: check the account identifier", base_url(&h.cfg))
        } else {
            format!("Snowflake request failed: {e}")
        }
    })?;
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if status >= 400 {
        let msg = v["message"].as_str().filter(|m| !m.is_empty()).map(String::from).unwrap_or_else(|| {
            let t = text.trim();
            if t.is_empty() { format!("Snowflake answered HTTP {status}") } else { t.chars().take(300).collect() }
        });
        return Err(match status {
            401 => format!("Snowflake refused the sign-in: {msg}"),
            403 => format!("No permission: {msg}"),
            _ => msg,
        });
    }
    Ok((status, v))
}

/// A statement Snowflake accepted: its handle, for polling, partitions and Stop.
struct Statement {
    handle: String,
}

fn fresh_request_id() -> String {
    uuid_like()
}

/// A random UUID-shaped request id (Snowflake wants a UUID).
fn uuid_like() -> String {
    let mut b = [0u8; 16];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut b).ok();
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// Bound values, as Snowflake's `bindings` (positional `?`).
#[derive(Default, Clone)]
struct Binds(Map<String, Value>);

impl Binds {
    fn bind(&mut self, ty: &str, value: String) -> &'static str {
        let n = self.0.len() + 1;
        self.0.insert(n.to_string(), json!({ "type": ty, "value": value }));
        "?"
    }
    fn text(&mut self, s: &str) -> &'static str {
        self.bind("TEXT", s.to_string())
    }
}

/// Submit a statement asynchronously; Snowflake answers at once with a handle.
async fn submit(h: &SnowflakeConn, sql: &str, binds: &Binds) -> Result<(Statement, Option<Value>), String> {
    let mut body = json!({ "statement": sql, "timeout": 0, "database": h.cfg.database.trim() });
    if let Some(w) = opt(&h.cfg.warehouse) {
        body["warehouse"] = json!(w);
    }
    if let Some(s) = opt(&h.cfg.schema) {
        body["schema"] = json!(s);
    }
    if let Some(r) = opt(&h.cfg.role) {
        body["role"] = json!(r);
    }
    if !binds.0.is_empty() {
        body["bindings"] = Value::Object(binds.0.clone());
    }
    let path = format!("/api/v2/statements?requestId={}&async=true", fresh_request_id());
    let (status, v) = call(h, reqwest::Method::POST, &path, Some(&body)).await?;
    let handle = v["statementHandle"].as_str().unwrap_or("").to_string();
    if handle.is_empty() {
        return Err(v["message"].as_str().unwrap_or("Snowflake returned no statement handle").to_string());
    }
    // 200: already done (and the first partition is here); 202: running.
    Ok((Statement { handle }, (status == 200).then_some(v)))
}

/// Wait for a statement and return its first partition with the metadata.
async fn wait(h: &SnowflakeConn, st: &Statement) -> Result<Value, String> {
    let mut delay = 100u64;
    loop {
        let (status, v) = call(h, reqwest::Method::GET, &format!("/api/v2/statements/{}", st.handle), None).await?;
        if status == 200 {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(delay)).await;
        delay = (delay * 2).min(1500);
    }
}

async fn partition(h: &SnowflakeConn, st: &Statement, n: usize) -> Result<Value, String> {
    let (_, v) = call(h, reqwest::Method::GET, &format!("/api/v2/statements/{}?partition={n}", st.handle), None).await?;
    Ok(v)
}

async fn cancel(h: &SnowflakeConn, st: &Statement) {
    let _ = tokio::time::timeout(Duration::from_secs(10), call(h, reqwest::Method::POST, &format!("/api/v2/statements/{}/cancel", st.handle), None)).await;
}

async fn run(h: &SnowflakeConn, sql: &str, binds: &Binds) -> Result<(Statement, Value), String> {
    let (st, done) = submit(h, sql, binds).await?;
    let first = match done {
        Some(v) => v,
        None => wait(h, &st).await?,
    };
    Ok((st, first))
}

pub async fn connect(cfg: &SnowflakeConfig) -> Result<SnowflakeHandle, String> {
    if cfg.account.trim().is_empty() || cfg.user.trim().is_empty() {
        return Err("Enter the account identifier and the user".into());
    }
    if cfg.database.trim().is_empty() {
        return Err("Enter the database".into());
    }
    let h = Arc::new(SnowflakeConn { cfg: cfg.clone(), jwt: Mutex::new(None) });
    ping(&h).await?;
    Ok(h)
}

pub async fn ping(h: &SnowflakeConn) -> Result<(), String> {
    run(h, "SELECT 1", &Binds::default()).await.map(|_| ())
}

// ── Values ────────────────────────────────────────────────────────────────────

fn row_types(v: &Value) -> Vec<Value> {
    v["resultSetMetaData"]["rowType"].as_array().cloned().unwrap_or_default()
}

fn type_label(t: &Value) -> String {
    let ty = t["type"].as_str().unwrap_or("text").to_ascii_lowercase();
    match ty.as_str() {
        "fixed" => match (t["precision"].as_u64(), t["scale"].as_u64()) {
            (Some(p), Some(0)) => format!("NUMBER({p},0)"),
            (Some(p), Some(s)) => format!("NUMBER({p},{s})"),
            _ => "NUMBER".into(),
        },
        "real" => "FLOAT".into(),
        "text" => match t["length"].as_u64() {
            Some(l) if l > 0 && l < 16_777_216 => format!("VARCHAR({l})"),
            _ => "VARCHAR".into(),
        },
        other => other.to_ascii_uppercase(),
    }
}

fn days_to_date(days: i64) -> Option<String> {
    chrono::NaiveDate::from_num_days_from_ce_opt(719_163 + days as i32).map(|d| d.format("%Y-%m-%d").to_string())
}

/// "seconds.fraction" → (seconds, nanoseconds).
fn split_secs(s: &str) -> Option<(i64, u32)> {
    let (whole, frac) = s.split_once('.').unwrap_or((s, ""));
    let secs: i64 = whole.trim().parse().ok()?;
    let mut f = frac.trim().to_string();
    f.truncate(9);
    while f.len() < 9 {
        f.push('0');
    }
    let nanos: u32 = f.parse().unwrap_or(0);
    // A negative time before the epoch counts the fraction backwards.
    if secs < 0 && nanos > 0 { Some((secs - 1, 1_000_000_000 - nanos)) } else { Some((secs, nanos)) }
}

fn trim_frac(s: String) -> String {
    match s.split_once('.') {
        Some((a, b)) => {
            let b = b.trim_end_matches('0');
            if b.is_empty() { a.to_string() } else { format!("{a}.{b}") }
        }
        None => s,
    }
}

fn timestamp(s: &str, ty: &str) -> Option<String> {
    let (secs_part, offset) = match s.split_once(' ') {
        Some((a, b)) => (a, b.trim().parse::<i32>().ok().map(|m| m - 1440)),
        None => (s, None),
    };
    let (secs, nanos) = split_secs(secs_part)?;
    let utc = chrono::DateTime::from_timestamp(secs, nanos)?;
    Some(match (ty, offset) {
        ("timestamp_tz", Some(mins)) => {
            let off = chrono::FixedOffset::east_opt(mins * 60)?;
            trim_frac(utc.with_timezone(&off).format("%Y-%m-%d %H:%M:%S%.9f").to_string()) + &utc.with_timezone(&off).format(" %:z").to_string()
        }
        ("timestamp_ltz", _) | ("timestamp_tz", None) => trim_frac(utc.format("%Y-%m-%d %H:%M:%S%.9f").to_string()) + " UTC",
        _ => trim_frac(utc.naive_utc().format("%Y-%m-%d %H:%M:%S%.9f").to_string()),
    })
}

fn time_of_day(s: &str) -> Option<String> {
    let (secs, nanos) = split_secs(s)?;
    let t = chrono::NaiveTime::from_num_seconds_from_midnight_opt(secs.rem_euclid(86_400) as u32, nanos)?;
    Some(trim_frac(t.format("%H:%M:%S%.9f").to_string()))
}

/// A jsonv2 cell (always a string or null) as the grid shows it.
fn cell(t: &Value, v: Value) -> Value {
    let Value::String(s) = v else { return v };
    let ty = t["type"].as_str().unwrap_or("text").to_ascii_lowercase();
    match ty.as_str() {
        "fixed" if t["scale"].as_u64().unwrap_or(0) == 0 => match s.parse::<i64>() {
            Ok(n) if n.unsigned_abs() <= (1u64 << 53) => json!(n),
            _ => Value::String(s),
        },
        "real" => match s.parse::<f64>() {
            Ok(x) if x.is_finite() => json!(x),
            _ => Value::String(s),
        },
        "boolean" => match s.as_str() {
            "true" | "1" => json!(true),
            "false" | "0" => json!(false),
            _ => Value::String(s),
        },
        "date" => s.parse::<i64>().ok().and_then(days_to_date).map(Value::String).unwrap_or(Value::String(s)),
        "time" => time_of_day(&s).map(Value::String).unwrap_or(Value::String(s)),
        "timestamp_ntz" | "timestamp_ltz" | "timestamp_tz" => timestamp(&s, &ty).map(Value::String).unwrap_or(Value::String(s)),
        "variant" | "object" | "array" => serde_json::from_str(&s).unwrap_or(Value::String(s)),
        _ => super::sql_util::cap_json_value("text", Value::String(s)),
    }
}

fn columns_of(types: &[Value]) -> Vec<ColumnInfo> {
    types.iter().map(|t| ColumnInfo::new(t["name"].as_str().unwrap_or("").to_string(), type_label(t))).collect()
}

fn rows_of(types: &[Value], v: &Value) -> Vec<Vec<Value>> {
    v["data"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|r| r.as_array().map(|cells| cells.iter().zip(types).map(|(c, t)| cell(t, c.clone())).collect()).unwrap_or_default())
                .collect()
        })
        .unwrap_or_default()
}

fn partitions(v: &Value) -> usize {
    v["resultSetMetaData"]["partitionInfo"].as_array().map(|p| p.len()).unwrap_or(1).max(1)
}

/// A statement's rows, up to `cap`, across its partitions.
async fn collect(h: &SnowflakeConn, sql: &str, binds: &Binds, cap: usize) -> Result<(Vec<Value>, Vec<Vec<Value>>, Value), String> {
    let (st, first) = run(h, sql, binds).await?;
    let types = row_types(&first);
    let mut rows = rows_of(&types, &first);
    for n in 1..partitions(&first) {
        if rows.len() >= cap {
            break;
        }
        rows.extend(rows_of(&types, &partition(h, &st, n).await?));
    }
    rows.truncate(cap);
    Ok((types, rows, first))
}

fn result_of(types: &[Value], rows: Vec<Vec<Value>>, first: &Value, sql: &str, t0: Instant) -> SqlResult {
    let total = first["resultSetMetaData"]["numRows"].as_i64().unwrap_or(rows.len() as i64);
    let message = (total > rows.len() as i64).then(|| format!("Showing the first {} of {total} rows", rows.len()));
    SqlResult { columns: columns_of(types), row_count: Some(rows.len() as i64), rows, message, query_ms: t0.elapsed().as_millis() as u64, sql: sql.to_string() }
}

pub fn reads(stmt: &str) -> bool {
    matches!(super::sql_util::statement_head(stmt).as_str(), "select" | "with" | "show" | "describe" | "desc" | "explain" | "list" | "ls")
}

pub async fn query(h: &SnowflakeConn, stmt: &str) -> Result<SqlResult, String> {
    let t0 = Instant::now();
    let sql = stmt.trim().trim_end_matches(';').trim();
    if sql.is_empty() {
        return Err("Query is empty".into());
    }
    let (types, rows, first) = collect(h, sql, &Binds::default(), MAX_ROWS).await?;
    Ok(result_of(&types, rows, &first, sql, t0))
}

/// One console statement that Stop cancels on Snowflake.
pub async fn query_cancellable(h: &SnowflakeConn, stmt: &str, stop: &super::stop::Stop) -> Result<SqlResult, String> {
    let t0 = Instant::now();
    let sql = stmt.trim().trim_end_matches(';').trim();
    if sql.is_empty() {
        return Err("Query is empty".into());
    }
    let (st, done) = submit(h, sql, &Binds::default()).await?;
    let work = async {
        let first = match done {
            Some(v) => v,
            None => wait(h, &st).await?,
        };
        let types = row_types(&first);
        let mut rows = rows_of(&types, &first);
        for n in 1..partitions(&first) {
            if rows.len() >= MAX_ROWS {
                break;
            }
            rows.extend(rows_of(&types, &partition(h, &st, n).await?));
        }
        rows.truncate(MAX_ROWS);
        Ok::<_, String>(result_of(&types, rows, &first, sql, t0))
    };
    tokio::pin!(work);
    tokio::select! {
        r = &mut work => r,
        _ = stop.wait() => {
            cancel(h, &st).await;
            Err(super::query::QUERY_CANCELLED.into())
        }
    }
}

/// Stream a statement's partitions into the console's sink; Stop cancels it.
pub async fn stream_into(h: &SnowflakeConn, stmt: &str, sink: &mut super::query::RowSink, stop: &super::stop::Stop) -> Result<SqlResult, String> {
    let t0 = Instant::now();
    let sql = stmt.trim().trim_end_matches(';').trim();
    if sql.is_empty() {
        return Err("Query is empty".into());
    }
    let (st, done) = submit(h, sql, &Binds::default()).await?;
    let work = async {
        let first = match done {
            Some(v) => v,
            None => wait(h, &st).await?,
        };
        let types = row_types(&first);
        let columns = columns_of(&types);
        sink.start(columns.clone())?;
        let mut n_rows = 0usize;
        for r in rows_of(&types, &first) {
            sink.push(r)?;
            n_rows += 1;
        }
        for n in 1..partitions(&first) {
            for r in rows_of(&types, &partition(h, &st, n).await?) {
                sink.push(r)?;
                n_rows += 1;
            }
        }
        sink.finish()?;
        Ok::<_, String>(SqlResult { columns, rows: vec![], row_count: Some(n_rows as i64), message: None, query_ms: t0.elapsed().as_millis() as u64, sql: sql.to_string() })
    };
    tokio::pin!(work);
    tokio::select! {
        r = &mut work => r,
        _ = stop.wait() => {
            cancel(h, &st).await;
            Err(super::query::QUERY_CANCELLED.into())
        }
    }
}

// ── Plans ─────────────────────────────────────────────────────────────────────

/// EXPLAIN USING TABULAR, as the plan view's tree: each operation under its
/// parent, with the tables it reads and the partitions it touches.
pub async fn explain_plan(h: &SnowflakeConn, sql: &str) -> Result<Value, String> {
    let sql = sql.trim().trim_end_matches(';').trim();
    let sql = sql.strip_prefix("EXPLAIN ").or_else(|| sql.strip_prefix("explain ")).unwrap_or(sql);
    let (types, rows, _) = collect(h, &format!("EXPLAIN USING TABULAR {sql}"), &Binds::default(), 10_000).await?;
    Ok(plan_tree(&types, &rows))
}

pub fn plan_tree(types: &[Value], rows: &[Vec<Value>]) -> Value {
    let col = |name: &str| types.iter().position(|t| t["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(name)));
    let get = |r: &[Value], name: &str| -> Value { col(name).and_then(|i| r.get(i).cloned()).unwrap_or(Value::Null) };
    let text = |v: Value| match v {
        Value::String(s) => s,
        Value::Null => String::new(),
        other => other.to_string(),
    };
    let mut nodes: Vec<(String, String, Map<String, Value>)> = Vec::new();
    let mut root_extra = Map::new();
    for r in rows {
        let op = text(get(r, "operation"));
        if op.eq_ignore_ascii_case("GlobalStats") {
            for k in ["partitionsTotal", "partitionsAssigned", "bytesAssigned"] {
                let v = text(get(r, k));
                if !v.is_empty() {
                    root_extra.insert(k.into(), json!(v));
                }
            }
            continue;
        }
        let mut n = Map::new();
        n.insert("Node Type".into(), json!(op));
        let objects = text(get(r, "objects"));
        if !objects.is_empty() {
            n.insert("Relation Name".into(), json!(objects));
        }
        let expr = text(get(r, "expressions"));
        if !expr.is_empty() {
            n.insert("Filter".into(), json!(expr));
        }
        for k in ["partitionsAssigned", "partitionsTotal"] {
            let v = text(get(r, k));
            if !v.is_empty() {
                n.insert(k.into(), json!(v));
            }
        }
        nodes.push((text(get(r, "id")), text(get(r, "parent")), n));
    }
    fn build(id: &str, nodes: &[(String, String, Map<String, Value>)]) -> Value {
        let (_, _, me) = nodes.iter().find(|n| n.0 == id).expect("node exists");
        let mut me = me.clone();
        let kids: Vec<Value> = nodes.iter().filter(|n| n.1 == id).map(|n| build(&n.0, nodes)).collect();
        if !kids.is_empty() {
            me.insert("Plans".into(), Value::Array(kids));
        }
        Value::Object(me)
    }
    let ids: Vec<&String> = nodes.iter().map(|n| &n.0).collect();
    let roots: Vec<Value> = nodes.iter().filter(|n| n.1.is_empty() || !ids.contains(&&n.1)).map(|n| build(&n.0, &nodes)).collect();
    let mut root = match roots.len() {
        0 => Map::from_iter([("Node Type".to_string(), json!("Plan"))]),
        1 => roots[0].as_object().cloned().unwrap_or_default(),
        _ => Map::from_iter([("Node Type".to_string(), json!("Plan")), ("Plans".to_string(), Value::Array(roots))]),
    };
    root.extend(root_extra);
    Value::Object(root)
}

// ── Catalog ───────────────────────────────────────────────────────────────────

pub fn quote(ident: &str) -> String {
    super::sql_util::quote_double(ident)
}

fn database(h: &SnowflakeConn) -> String {
    quote(h.cfg.database.trim())
}

fn table_ref(h: &SnowflakeConn, schema: &str, table: &str) -> String {
    format!("{}.{}.{}", database(h), quote(schema), quote(table))
}

/// A column of a SHOW / DESCRIBE result, by name.
fn field(types: &[Value], row: &[Value], name: &str) -> String {
    types
        .iter()
        .position(|t| t["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(name)))
        .and_then(|i| row.get(i))
        .map(|v| match v {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        })
        .unwrap_or_default()
}

async fn show(h: &SnowflakeConn, sql: &str) -> Result<(Vec<Value>, Vec<Vec<Value>>), String> {
    let (types, rows, _) = collect(h, sql, &Binds::default(), 100_000).await?;
    Ok((types, rows))
}

pub async fn list_schemas(h: &SnowflakeConn) -> Result<Vec<String>, String> {
    let (t, rows) = show(h, &format!("SHOW SCHEMAS IN DATABASE {}", database(h))).await?;
    let mut out: Vec<String> = rows.iter().map(|r| field(&t, r, "name")).filter(|n| !n.is_empty() && n != "INFORMATION_SCHEMA").collect();
    out.sort();
    // The connection's schema first.
    let first = opt(&h.cfg.schema).unwrap_or("PUBLIC").to_string();
    if let Some(i) = out.iter().position(|s| s.eq_ignore_ascii_case(&first)) {
        let s = out.remove(i);
        out.insert(0, s);
    }
    Ok(out)
}

pub async fn list_tables(h: &SnowflakeConn, schema: &str) -> Result<Vec<TableInfo>, String> {
    let scope = format!("{}.{}", database(h), quote(schema));
    let (t, rows) = show(h, &format!("SHOW TABLES IN SCHEMA {scope}")).await?;
    let mut out: Vec<TableInfo> = rows
        .iter()
        .map(|r| TableInfo {
            name: field(&t, r, "name"),
            row_count: field(&t, r, "rows").parse().unwrap_or(-1),
            kind: "table".into(),
            rls_enabled: None,
            created_at: None,
            create_order: None,
            system: None,
        })
        .collect();
    if let Ok((vt, views)) = show(h, &format!("SHOW VIEWS IN SCHEMA {scope}")).await {
        out.extend(views.iter().map(|r| TableInfo {
            name: field(&vt, r, "name"),
            row_count: -1,
            kind: "view".into(),
            rls_enabled: None,
            created_at: None,
            create_order: None,
            system: None,
        }));
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// (column, referenced table, referenced column, constraint) for each
/// foreign key column of `table`; `exported` gives the keys pointing at it,
/// as (referencing table, referencing column, column here, constraint).
async fn keys(h: &SnowflakeConn, schema: &str, table: &str, exported: bool) -> Vec<[String; 4]> {
    let kind = if exported { "EXPORTED" } else { "IMPORTED" };
    let Ok((t, rows)) = show(h, &format!("SHOW {kind} KEYS IN TABLE {}", table_ref(h, schema, table))).await else { return vec![] };
    rows.iter()
        .map(|r| {
            if exported {
                [field(&t, r, "fk_table_name"), field(&t, r, "fk_column_name"), field(&t, r, "pk_column_name"), field(&t, r, "fk_name")]
            } else {
                [field(&t, r, "fk_column_name"), field(&t, r, "pk_table_name"), field(&t, r, "pk_column_name"), field(&t, r, "fk_name")]
            }
        })
        .collect()
}

pub async fn get_column_structure(h: &SnowflakeConn, schema: &str, table: &str) -> Result<Vec<ColumnStructureRow>, String> {
    let (t, rows) = show(h, &format!("DESCRIBE TABLE {}", table_ref(h, schema, table))).await?;
    let fks = keys(h, schema, table, false).await;
    Ok(rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let name = field(&t, r, "name");
            let fk = fks.iter().find(|k| k[0] == name);
            ColumnStructureRow {
                ordinal_position: (i + 1) as i32,
                data_type: field(&t, r, "type"),
                is_nullable: field(&t, r, "null?") != "N",
                column_default: Some(field(&t, r, "default")).filter(|d| !d.is_empty()),
                foreign_key: fk.map(|k| format!("{}.{}", k[1], k[2])),
                fk_constraint_name: fk.map(|k| k[3].clone()).filter(|n| !n.is_empty()),
                comment: Some(field(&t, r, "comment")).filter(|d| !d.is_empty()),
                is_primary_key: field(&t, r, "primary key") == "Y",
                name,
            }
        })
        .collect())
}

pub async fn schema_column_structure(h: &SnowflakeConn, schema: &str) -> Result<Vec<super::schema::TableColumnStructure>, String> {
    use futures::stream::StreamExt;
    let tables = list_tables(h, schema).await?;
    let out: Vec<super::schema::TableColumnStructure> = futures::stream::iter(tables.into_iter().take(300).map(|t| async move {
        get_column_structure(h, schema, &t.name).await.ok().map(|columns| super::schema::TableColumnStructure { table: t.name, columns })
    }))
    .buffered(6)
    .filter_map(|x| async move { x })
    .collect()
    .await;
    Ok(out)
}

pub async fn incoming_foreign_keys(h: &SnowflakeConn, schema: &str, table: &str) -> Result<Vec<super::schema::IncomingForeignKey>, String> {
    let mut out: Vec<super::schema::IncomingForeignKey> = Vec::new();
    for [from_table, col, ref_col, name] in keys(h, schema, table, true).await {
        match out.iter_mut().find(|f| f.from_table == from_table && f.constraint_name == name) {
            Some(f) => {
                f.from_columns.push(col);
                f.to_columns.push(ref_col);
            }
            None => out.push(super::schema::IncomingForeignKey {
                from_schema: schema.to_string(),
                from_table,
                from_columns: vec![col],
                to_columns: vec![ref_col],
                constraint_name: name,
            }),
        }
    }
    Ok(out)
}

pub async fn get_ddl(h: &SnowflakeConn, schema: &str, table: &str) -> Result<String, String> {
    let mut b = Binds::default();
    let name = format!("{}.{}.{}", h.cfg.database.trim(), quote(schema), quote(table));
    b.text(&name);
    let (_, rows, _) = collect(h, "SELECT GET_DDL('TABLE', ?, TRUE)", &b, 1).await?;
    match rows.first().and_then(|r| r.first()) {
        Some(Value::String(s)) => Ok(s.clone()),
        _ => Err("Snowflake returned no DDL for this table".into()),
    }
}

// ── The grid ──────────────────────────────────────────────────────────────────

fn base_type(ty: &str) -> String {
    ty.trim().to_ascii_uppercase().split(['(', ' ']).next().unwrap_or("").to_string()
}

fn as_text(col: &str, ty: &str) -> String {
    match base_type(ty).as_str() {
        "VARCHAR" | "TEXT" | "STRING" | "CHAR" | "CHARACTER" => col.to_string(),
        "VARIANT" | "OBJECT" | "ARRAY" => format!("TO_JSON({col})"),
        _ => format!("TO_VARCHAR({col})"),
    }
}

/// A grid value as a binding for a column, or NULL.
fn bind_value(ty: &str, v: &Value, b: &mut Binds) -> Result<String, String> {
    let base = base_type(ty);
    Ok(match v {
        Value::Null => "NULL".into(),
        Value::Bool(x) => b.bind("BOOLEAN", x.to_string()).into(),
        Value::Number(n) if n.is_i64() || n.is_u64() => b.bind("FIXED", n.to_string()).into(),
        Value::Number(n) => b.bind("REAL", n.to_string()).into(),
        Value::String(s) if matches!(base.as_str(), "VARIANT" | "OBJECT" | "ARRAY") => {
            // Bound as text, parsed on Snowflake's side.
            format!("PARSE_JSON({})", b.text(s))
        }
        Value::String(s) => b.text(s).into(),
        other if matches!(base.as_str(), "VARIANT" | "OBJECT" | "ARRAY") => format!("PARSE_JSON({})", b.text(&other.to_string())),
        other => b.text(&other.to_string()).into(),
    })
}

fn build_where(cols: &[ColumnStructureRow], search: Option<&str>, filters: Option<&[RowFilter]>, b: &mut Binds) -> String {
    let by_name: HashMap<&str, &ColumnStructureRow> = cols.iter().map(|c| (c.name.as_str(), c)).collect();
    let mut clauses: Vec<String> = Vec::new();
    if let Some(q) = search.map(str::trim).filter(|s| !s.is_empty()) {
        let needle = format!("%{q}%");
        let ors: Vec<String> = cols.iter().map(|c| format!("{} ILIKE {}", as_text(&quote(&c.name), &c.data_type), b.text(&needle))).collect();
        if !ors.is_empty() {
            clauses.push(format!("({})", ors.join(" OR ")));
        }
    }
    for f in filters.unwrap_or(&[]) {
        let Some(c) = by_name.get(f.column.as_str()) else { continue };
        let col = quote(&c.name);
        let v = f.value.as_deref().unwrap_or("");
        let text = as_text(&col, &c.data_type);
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
                // Text is bound and Snowflake casts it to the column's type.
                format!("{col} {op} {}", b.text(v))
            }
            "contains" => format!("{text} ILIKE {}", b.text(&format!("%{v}%"))),
            "not_contains" => format!("({col} IS NULL OR {text} NOT ILIKE {})", b.text(&format!("%{v}%"))),
            "starts_with" => format!("{text} ILIKE {}", b.text(&format!("{v}%"))),
            "ends_with" => format!("{text} ILIKE {}", b.text(&format!("%{v}"))),
            "between" => {
                let mut parts = v.splitn(2, ',');
                let (a, z) = (parts.next().unwrap_or("").trim().to_string(), parts.next().unwrap_or("").trim().to_string());
                format!("{col} BETWEEN {} AND {}", b.text(&a), b.text(&z))
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

#[allow(clippy::too_many_arguments)]
pub async fn get_table_rows(
    h: &SnowflakeConn,
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
    let cols = get_column_structure(h, schema, table).await?;
    if cols.is_empty() {
        return Err(format!("No table named {table}"));
    }
    let tref = table_ref(h, schema, table);
    let mut b = Binds::default();
    let where_clause = build_where(&cols, search.as_deref(), filters.as_deref(), &mut b);
    let order = match sort_column.as_deref().filter(|c| cols.iter().any(|x| x.name == *c)) {
        Some(c) => {
            let d = if sort_direction.as_deref().is_some_and(|d| d.eq_ignore_ascii_case("desc")) { "DESC" } else { "ASC" };
            format!(" ORDER BY {} {d}", quote(c))
        }
        None => {
            let pk: Vec<String> = cols.iter().filter(|c| c.is_primary_key).map(|c| quote(&c.name)).collect();
            if pk.is_empty() { String::new() } else { format!(" ORDER BY {}", pk.join(", ")) }
        }
    };
    let (limit, offset) = (limit.max(0), offset.max(0));
    let data_sql = format!("SELECT * FROM {tref}{where_clause}{order} LIMIT {limit} OFFSET {offset}");
    let count_sql = format!("SELECT COUNT(*) FROM {tref}{where_clause}");
    let (data, count) = tokio::join!(collect(h, &data_sql, &b, limit as usize), collect(h, &count_sql, &b, 1));
    let (types, rows, _) = data?;
    let total = count.ok().and_then(|(_, r, _)| r.first().and_then(|r| r.first()).and_then(|v| v.as_i64())).unwrap_or(rows.len() as i64);
    let fks = cols
        .iter()
        .filter_map(|c| {
            let (t, col) = c.foreign_key.as_ref()?.split_once('.')?;
            Some(ForeignKeyInfo { columns: vec![c.name.clone()], referenced_schema: schema.to_string(), referenced_table: t.to_string(), referenced_columns: vec![col.to_string()] })
        })
        .collect();
    Ok(TableRows {
        preview_columns: Vec::new(),
        columns: columns_of(&types),
        rows,
        total,
        query_ms: t0.elapsed().as_millis() as u64,
        primary_key: cols.iter().filter(|c| c.is_primary_key).map(|c| c.name.clone()).collect(),
        foreign_keys: fks,
        sql: format!("{data_sql}\n{count_sql}"),
    })
}

fn pk_where(cols: &[ColumnStructureRow], pk: &HashMap<String, Value>, b: &mut Binds) -> Result<String, String> {
    let keys: Vec<&ColumnStructureRow> = cols.iter().filter(|c| c.is_primary_key).collect();
    if keys.is_empty() {
        return Err("This table declares no primary key, so the grid can't tell its rows apart. Use UPDATE or DELETE in the SQL console, or add a PRIMARY KEY constraint (Snowflake keeps it as metadata).".into());
    }
    let mut parts = Vec::new();
    for c in keys {
        let v = pk.get(&c.name).ok_or_else(|| format!("The row's {} is missing", c.name))?;
        let col = quote(&c.name);
        parts.push(if v.is_null() { format!("{col} IS NULL") } else { format!("{col} = {}", bind_value(&c.data_type, v, b)?) });
    }
    Ok(parts.join(" AND "))
}

/// Rows a DML statement changed, from its one-row result.
async fn dml(h: &SnowflakeConn, sql: &str, b: &Binds) -> Result<Option<i64>, String> {
    let (_, rows, _) = collect(h, sql, b, 1).await?;
    Ok(rows.first().and_then(|r| r.first()).and_then(|v| v.as_i64()))
}

pub async fn update_table_cell(
    h: &SnowflakeConn,
    schema: &str,
    table: &str,
    primary_key: HashMap<String, Value>,
    column: &str,
    value: &Value,
) -> Result<(), String> {
    let cols = get_column_structure(h, schema, table).await?;
    let c = cols.iter().find(|c| c.name == column).ok_or_else(|| format!("No column named {column}"))?;
    let mut b = Binds::default();
    let set = format!("{} = {}", quote(column), bind_value(&c.data_type, value, &mut b)?);
    let w = pk_where(&cols, &primary_key, &mut b)?;
    let sql = format!("UPDATE {} SET {set} WHERE {w}", table_ref(h, schema, table));
    match dml(h, &sql, &b).await? {
        Some(1) | None => Ok(()),
        Some(n) => Err(super::query::row_count_error(&primary_key.keys().cloned().collect::<Vec<_>>(), &primary_key, n.max(0) as u64)),
    }
}

pub async fn insert_table_row(h: &SnowflakeConn, schema: &str, table: &str, values: HashMap<String, Value>) -> Result<Vec<Value>, String> {
    let cols = get_column_structure(h, schema, table).await?;
    let mut b = Binds::default();
    let mut names = Vec::new();
    let mut holes = Vec::new();
    for c in &cols {
        let Some(v) = values.get(&c.name) else { continue };
        names.push(quote(&c.name));
        holes.push(bind_value(&c.data_type, v, &mut b)?);
    }
    if names.is_empty() {
        return Err("Fill in at least one column".into());
    }
    // VALUES can't hold PARSE_JSON; SELECT can, for VARIANT columns.
    let sql = format!("INSERT INTO {} ({}) SELECT {}", table_ref(h, schema, table), names.join(", "), holes.join(", "));
    dml(h, &sql, &b).await?;
    Ok(cols.iter().map(|c| values.get(&c.name).cloned().unwrap_or(Value::Null)).collect())
}

pub async fn delete_table_rows(h: &SnowflakeConn, schema: &str, table: &str, primary_keys: Vec<HashMap<String, Value>>) -> Result<u64, String> {
    if primary_keys.is_empty() {
        return Ok(0);
    }
    let cols = get_column_structure(h, schema, table).await?;
    let mut b = Binds::default();
    let mut ors = Vec::new();
    for pk in &primary_keys {
        ors.push(format!("({})", pk_where(&cols, pk, &mut b)?));
    }
    let sql = format!("DELETE FROM {} WHERE {}", table_ref(h, schema, table), ors.join(" OR "));
    Ok(dml(h, &sql, &b).await?.map(|n| n.max(0) as u64).unwrap_or(primary_keys.len() as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_read_as_the_grid_shows_them() {
        let t = |ty: &str| json!({ "type": ty, "scale": 0, "precision": 38 });
        assert_eq!(cell(&t("fixed"), json!("42")), json!(42));
        assert_eq!(cell(&json!({ "type": "fixed", "scale": 2 }), json!("12.50")), json!("12.50"));
        assert_eq!(cell(&t("real"), json!("9.5")), json!(9.5));
        assert_eq!(cell(&t("boolean"), json!("true")), json!(true));
        assert_eq!(cell(&t("date"), json!("20413")), json!("2025-11-21"));
        assert_eq!(cell(&t("time"), json!("45296.500000000")), json!("12:34:56.5"));
        assert_eq!(cell(&t("timestamp_ntz"), json!("1795237200.000000000")), json!("2026-11-21 05:00:00"));
        assert_eq!(cell(&t("timestamp_ltz"), json!("1795237200.250000000")), json!("2026-11-21 05:00:00.25 UTC"));
        assert_eq!(cell(&t("timestamp_tz"), json!("1795237200.000000000 1500")), json!("2026-11-21 06:00:00 +01:00"));
        assert_eq!(cell(&t("variant"), json!("{\"a\":1}")), json!({ "a": 1 }));
        assert_eq!(cell(&t("text"), Value::Null), Value::Null);
    }

    #[test]
    fn types_label_like_snowflake() {
        assert_eq!(type_label(&json!({ "type": "fixed", "precision": 38, "scale": 0 })), "NUMBER(38,0)");
        assert_eq!(type_label(&json!({ "type": "text", "length": 16777216 })), "VARCHAR");
        assert_eq!(type_label(&json!({ "type": "timestamp_tz" })), "TIMESTAMP_TZ");
    }

    #[test]
    fn jwt_names_the_account_snowflake_s_way() {
        assert_eq!(jwt_account("myorg-myaccount"), "MYORG-MYACCOUNT");
        assert_eq!(jwt_account("xy12345.us-east-2.aws"), "XY12345");
        assert_eq!(jwt_account("xy12345.snowflakecomputing.com"), "XY12345");
    }

    /// A 2048-bit test key, generated for this test only.
    fn test_key() -> String {
        let out = std::process::Command::new("openssl").args(["genpkey", "-algorithm", "RSA", "-pkeyopt", "rsa_keygen_bits:2048"]).output();
        match out {
            Ok(o) if o.status.success() => String::from_utf8(o.stdout).unwrap(),
            _ => String::new(),
        }
    }

    #[test]
    fn key_pair_jwt_carries_the_public_key_fingerprint() {
        let pem = test_key();
        if pem.is_empty() {
            return; // no openssl here
        }
        let jwt = sign_jwt("myorg-acct", "me", &pem, 1_700_000_000).unwrap();
        let claims: Value = serde_json::from_slice(&B64URL.decode(jwt.split('.').nth(1).unwrap()).unwrap()).unwrap();
        assert_eq!(claims["sub"], "MYORG-ACCT.ME");
        // The same fingerprint openssl computes over the public key's DER.
        let dir = std::env::temp_dir().join(format!("stroke-sf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let kp = dir.join("k.pem");
        std::fs::write(&kp, &pem).unwrap();
        let der = std::process::Command::new("openssl").args(["pkey", "-in", kp.to_str().unwrap(), "-pubout", "-outform", "DER"]).output().unwrap().stdout;
        let fp = B64.encode(ring::digest::digest(&ring::digest::SHA256, &der).as_ref());
        assert_eq!(claims["iss"], format!("MYORG-ACCT.ME.SHA256:{fp}"));
        assert_eq!(claims["exp"].as_u64().unwrap() - claims["iat"].as_u64().unwrap(), 3540);
        let _ = std::fs::remove_dir_all(dir);
        assert!(sign_jwt("a", "b", "-----BEGIN ENCRYPTED PRIVATE KEY-----\nAAAA\n-----END ENCRYPTED PRIVATE KEY-----", 0).unwrap_err().contains("encrypted"));
    }

    #[test]
    fn plans_nest_by_parent() {
        let types: Vec<Value> = ["step", "id", "parent", "operation", "objects", "expressions", "partitionsTotal", "partitionsAssigned"]
            .iter()
            .map(|n| json!({ "name": n, "type": "text" }))
            .collect();
        let row = |id: &str, parent: &str, op: &str, obj: &str| -> Vec<Value> {
            vec![json!("1"), json!(id), if parent.is_empty() { Value::Null } else { json!(parent) }, json!(op), json!(obj), Value::Null, json!("4"), json!("2")]
        };
        let rows = vec![
            vec![Value::Null, Value::Null, Value::Null, json!("GlobalStats"), Value::Null, Value::Null, json!("4"), json!("2")],
            row("0", "", "Result", ""),
            row("1", "0", "Filter", ""),
            row("2", "1", "TableScan", "DB.PUBLIC.T"),
        ];
        let p = plan_tree(&types, &rows);
        assert_eq!(p["Node Type"], "Result");
        assert_eq!(p["partitionsTotal"], "4");
        assert_eq!(p["Plans"][0]["Plans"][0]["Relation Name"], "DB.PUBLIC.T");
    }

    #[test]
    fn filters_bind_their_values() {
        let col = |n: &str, t: &str| ColumnStructureRow {
            ordinal_position: 1, name: n.into(), data_type: t.into(), is_nullable: true, column_default: None,
            foreign_key: None, fk_constraint_name: None, comment: None, is_primary_key: false,
        };
        let cols = vec![col("ID", "NUMBER(38,0)"), col("NAME", "VARCHAR(40)"), col("META", "VARIANT")];
        let mut b = Binds::default();
        let f = RowFilter { column: "ID".into(), op: ">=".into(), value: Some("2'; drop".into()), conjunct: None, data_type: None };
        let w = build_where(&cols, Some("an"), Some(&[f]), &mut b);
        assert_eq!(w, " WHERE ((TO_VARCHAR(\"ID\") ILIKE ? OR \"NAME\" ILIKE ? OR TO_JSON(\"META\") ILIKE ?) AND \"ID\" >= ?)");
        assert_eq!(b.0["4"]["value"], "2'; drop");
        let mut b = Binds::default();
        assert_eq!(bind_value("VARIANT", &json!({ "k": 1 }), &mut b).unwrap(), "PARSE_JSON(?)");
        assert_eq!(bind_value("NUMBER(38,0)", &json!(7), &mut b).unwrap(), "?");
        assert_eq!(b.0["2"]["type"], "FIXED");
    }

    /// A stand-in for Snowflake's SQL API: answers a few statements the way
    /// the API documents, with an asynchronous first reply, two partitions,
    /// an error and a cancel.
    #[tokio::test]
    async fn against_a_stand_in_sql_api() {
        use axum::{extract::{Path, Query, State}, http::StatusCode, routing::{get, post}, Json, Router};
        #[derive(Default)]
        struct Mock {
            polls: std::sync::atomic::AtomicUsize,
            cancelled: std::sync::atomic::AtomicBool,
            seen: Mutex<Vec<Value>>,
        }
        let mock = Arc::new(Mock::default());
        let rt = |name: &str, ty: &str| json!({ "name": name, "type": ty, "scale": 0, "precision": 38, "nullable": true });
        async fn submit_h(State(m): State<Arc<Mock>>, Json(body): Json<Value>) -> (StatusCode, Json<Value>) {
            m.seen.lock().unwrap().push(body.clone());
            let sql = body["statement"].as_str().unwrap_or("");
            if sql.contains("nope") {
                return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "code": "002003", "message": "SQL compilation error: Object 'NOPE' does not exist", "statementHandle": "h-err" })));
            }
            let handle = if sql.starts_with("SELECT * FROM") { "h-big" } else if sql.contains("SYSTEM$WAIT") { "h-slow" } else { "h-small" };
            (StatusCode::ACCEPTED, Json(json!({ "statementHandle": handle, "message": "Asynchronous execution in progress." })))
        }
        async fn status_h(State(m): State<Arc<Mock>>, Path(handle): Path<String>, Query(q): Query<HashMap<String, String>>) -> (StatusCode, Json<Value>) {
            let rt = |name: &str, ty: &str| json!({ "name": name, "type": ty, "scale": 0, "precision": 38, "nullable": true });
            if handle == "h-slow" {
                return (StatusCode::ACCEPTED, Json(json!({ "statementHandle": handle })));
            }
            // The first poll says it's still running.
            if m.polls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                return (StatusCode::ACCEPTED, Json(json!({ "statementHandle": handle })));
            }
            match (handle.as_str(), q.get("partition").map(String::as_str)) {
                ("h-big", Some("1")) => (StatusCode::OK, Json(json!({ "data": [["3", "ink", "20413"]] }))),
                ("h-big", _) => (StatusCode::OK, Json(json!({
                    "statementHandle": handle,
                    "resultSetMetaData": { "numRows": 3, "format": "jsonv2", "rowType": [rt("ID", "fixed"), rt("NAME", "text"), rt("D", "date")],
                        "partitionInfo": [{ "rowCount": 2 }, { "rowCount": 1 }] },
                    "data": [["1", "pen", "20413"], ["2", null, null]],
                }))),
                _ => (StatusCode::OK, Json(json!({
                    "statementHandle": handle,
                    "resultSetMetaData": { "numRows": 1, "rowType": [rt("1", "fixed")], "partitionInfo": [{ "rowCount": 1 }] },
                    "data": [["1"]],
                }))),
            }
        }
        async fn cancel_h(State(m): State<Arc<Mock>>, Path(_h): Path<String>) -> Json<Value> {
            m.cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
            Json(json!({ "message": "successfully canceled" }))
        }
        let _ = rt;
        let app = Router::new()
            .route("/api/v2/statements", post(submit_h))
            .route("/api/v2/statements/:handle", get(status_h))
            .route("/api/v2/statements/:handle/cancel", post(cancel_h))
            .with_state(mock.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let cfg = SnowflakeConfig {
            name: "t".into(),
            account: "myorg-acct".into(),
            user: "me".into(),
            auth: "token".into(),
            private_key_file: None,
            password: Some("pat-secret".into()),
            warehouse: Some("WH".into()),
            database: "DB".into(),
            schema: Some("PUBLIC".into()),
            role: None,
            host: Some(format!("http://{addr}")),
        };
        let h = connect(&cfg).await.unwrap();
        let seen = mock.seen.lock().unwrap()[0].clone();
        assert_eq!(seen["warehouse"], "WH");
        assert_eq!(seen["database"], "DB");
        assert_eq!(seen["schema"], "PUBLIC");

        // Async reply, a poll that says running, then two partitions.
        let r = query(&h, "SELECT * FROM T").await.unwrap();
        assert_eq!(r.rows.len(), 3);
        assert_eq!(r.rows[0], vec![json!(1), json!("pen"), json!("2025-11-21")]);
        assert_eq!(r.rows[2][1], json!("ink"));
        assert_eq!(r.columns[0].data_type, "NUMBER(38,0)");

        let e = query(&h, "SELECT nope").await.unwrap_err();
        assert!(e.contains("does not exist"), "{e}");

        // Stop cancels the statement on the server.
        let (tx, rx) = tokio::sync::oneshot::channel();
        let stop = super::super::stop::Stop::from_rx(rx);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let _ = tx.send(());
        });
        let e = query_cancellable(&h, "CALL SYSTEM$WAIT(60)", &stop).await.unwrap_err();
        assert!(e.starts_with(super::super::query::QUERY_CANCELLED));
        assert!(mock.cancelled.load(std::sync::atomic::Ordering::SeqCst));
    }
}
