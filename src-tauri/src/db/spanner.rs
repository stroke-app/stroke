//! Google Cloud Spanner over its REST API (v1). Beta.
//!
//! Sign-in comes from `google_auth`: a service account key, the gcloud login,
//! or the local emulator (no auth, `SPANNER_EMULATOR_HOST`-style host).
//!
//! - Reads run as single-use read-only queries (strong reads).
//! - INSERT, UPDATE and DELETE run in a read-write transaction that commits
//!   when the statement succeeds.
//! - CREATE, ALTER and DROP go through the database's schema update, a
//!   long-running operation this waits for.
//!
//! Both dialects work: GoogleSQL quotes identifiers with backticks and keeps
//! its tables in the unnamed schema (shown as `default`); PostgreSQL-dialect
//! databases quote with double quotes and use `public`. `information_schema`
//! answers the same lowercase column names in both.
//!
//! Values in the grid are bound as typed query parameters, never spliced into
//! the SQL text.

use super::query::{ColumnInfo, ForeignKeyInfo, RowFilter, SqlResult, TableRows};
use super::schema::{ColumnStructureRow, IndexInfo, TableInfo};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpannerConfig {
    pub name: String,
    pub project_id: String,
    pub instance_id: String,
    pub database_id: String,
    /// "serviceAccount", "gcloud" or "emulator".
    #[serde(default = "default_auth")]
    pub auth: String,
    #[serde(default)]
    pub key_file: Option<String>,
    /// The emulator's REST address, `localhost:9020` when blank.
    #[serde(default)]
    pub emulator_host: Option<String>,
}

fn default_auth() -> String {
    "gcloud".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    GoogleSql,
    Postgres,
}

pub struct SpannerConn {
    pub cfg: SpannerConfig,
    pub dialect: Dialect,
    /// One session for the connection, made on first use and made again when
    /// Spanner forgets it (idle sessions expire after an hour).
    session: tokio::sync::Mutex<Option<String>>,
}

pub type SpannerHandle = Arc<SpannerConn>;

/// What the schema switcher calls GoogleSQL's unnamed schema.
pub const DEFAULT_SCHEMA: &str = "default";

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default()
    })
}

fn base_url(cfg: &SpannerConfig) -> String {
    if cfg.auth == "emulator" {
        let h = cfg.emulator_host.as_deref().map(str::trim).filter(|h| !h.is_empty()).unwrap_or("localhost:9020");
        let h = h.trim_end_matches('/');
        if h.contains("://") { format!("{h}/v1") } else { format!("http://{h}/v1") }
    } else {
        "https://spanner.googleapis.com/v1".into()
    }
}

pub fn database_path(cfg: &SpannerConfig) -> String {
    format!(
        "projects/{}/instances/{}/databases/{}",
        cfg.project_id.trim(),
        cfg.instance_id.trim(),
        cfg.database_id.trim()
    )
}

/// One REST call. `path` is relative to `/v1`.
async fn call(cfg: &SpannerConfig, method: reqwest::Method, path: &str, body: Option<&Value>) -> Result<Value, String> {
    let url = format!("{}/{}", base_url(cfg), path.trim_start_matches('/'));
    let token = super::google_auth::token(&cfg.auth, cfg.key_file.as_deref(), super::google_auth::SCOPE_CLOUD_PLATFORM).await?;
    let build = || {
        let mut req = client().request(method.clone(), &url).bearer_auth(&token);
        if let Some(b) = body {
            req = req.json(b);
        }
        req
    };
    // One retry when the request never got an answer (a dropped keep-alive
    // connection). Safe to resend: DML carries its seqno and a commit its
    // transaction id, so Spanner applies neither twice.
    let sent = match build().send().await {
        Err(e) if e.is_request() || e.is_connect() => {
            tokio::time::sleep(Duration::from_millis(150)).await;
            build().send().await
        }
        other => other,
    };
    let resp = sent.map_err(|e| {
        if e.is_connect() && cfg.auth == "emulator" {
            format!("Could not reach the Spanner emulator at {}: is it running?", base_url(cfg))
        } else {
            format!("Spanner request failed: {e}")
        }
    })?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !status.is_success() || v.get("error").is_some() {
        return Err(error_text(status.as_u16(), &v, &text));
    }
    Ok(v)
}

/// Google's error body (`{"error": {"message", "status"}}`), or the emulator's
/// flatter one (`{"code", "message"}`).
fn error_text(status: u16, v: &Value, raw: &str) -> String {
    let e = v.get("error").unwrap_or(v);
    let msg = e.get("message").and_then(|m| m.as_str()).unwrap_or("");
    let kind = e.get("status").and_then(|s| s.as_str()).unwrap_or("");
    match status {
        401 => "Google refused the credentials. Sign in again with gcloud, or check the key file.".into(),
        403 => format!("Not allowed on this database: {msg}"),
        _ if !msg.is_empty() => {
            if kind.is_empty() { format!("Spanner: {msg}") } else { format!("Spanner ({kind}): {msg}") }
        }
        _ => format!("Spanner error (HTTP {status}): {}", raw.chars().take(300).collect::<String>()),
    }
}

fn is_session_gone(err: &str) -> bool {
    err.contains("Session not found") || err.contains("NOT_FOUND") && err.contains("ession")
}

pub async fn connect(cfg: &SpannerConfig) -> Result<SpannerHandle, String> {
    for (what, v) in [("project", &cfg.project_id), ("instance", &cfg.instance_id), ("database", &cfg.database_id)] {
        if v.trim().is_empty() {
            return Err(format!("Enter the Spanner {what}"));
        }
    }
    let db = call(cfg, reqwest::Method::GET, &database_path(cfg), None).await?;
    let dialect = match db.get("databaseDialect").and_then(|d| d.as_str()) {
        Some("POSTGRESQL") => Dialect::Postgres,
        _ => Dialect::GoogleSql,
    };
    let h = Arc::new(SpannerConn { cfg: cfg.clone(), dialect, session: tokio::sync::Mutex::new(None) });
    session(&h).await?;
    Ok(h)
}

async fn session(h: &SpannerConn) -> Result<String, String> {
    let mut s = h.session.lock().await;
    if let Some(name) = s.as_ref() {
        return Ok(name.clone());
    }
    let v = call(&h.cfg, reqwest::Method::POST, &format!("{}/sessions", database_path(&h.cfg)), Some(&json!({}))).await?;
    let name = v.get("name").and_then(|n| n.as_str()).ok_or("Spanner did not return a session")?.to_string();
    *s = Some(name.clone());
    Ok(name)
}

async fn forget_session(h: &SpannerConn) {
    *h.session.lock().await = None;
}

/// `session:executeSql`, made once more on a fresh session if Spanner forgot ours.
async fn execute(h: &SpannerConn, body: &Value) -> Result<Value, String> {
    for attempt in 0..2 {
        let s = session(h).await?;
        match call(&h.cfg, reqwest::Method::POST, &format!("{s}:executeSql"), Some(body)).await {
            Err(e) if attempt == 0 && is_session_gone(&e) => forget_session(h).await,
            other => return other,
        }
    }
    unreachable_err()
}

fn unreachable_err<T>() -> Result<T, String> {
    Err("Spanner kept losing the session".into())
}

pub async fn ping(h: &SpannerConn) -> Result<(), String> {
    execute(h, &json!({ "sql": "SELECT 1" })).await.map(|_| ())
}

// ── Values ────────────────────────────────────────────────────────────────────

/// `ARRAY<STRING>`, `STRUCT`, `INT64`: a column's type for the grid header.
fn type_label(t: &Value) -> String {
    let code = t.get("code").and_then(|c| c.as_str()).unwrap_or("UNKNOWN");
    match code {
        "ARRAY" => format!("ARRAY<{}>", t.get("arrayElementType").map(type_label).unwrap_or_default()),
        other => other.to_string(),
    }
}

/// A Spanner value as the grid shows it. INT64 arrives as a string; it becomes
/// a number when it fits a double exactly. JSON arrives as text; it becomes
/// the object. Everything else stays as Spanner sent it.
fn cell(t: &Value, v: Value) -> Value {
    if v.is_null() {
        return v;
    }
    match t.get("code").and_then(|c| c.as_str()).unwrap_or("") {
        "INT64" => match v.as_str().and_then(|s| s.parse::<i64>().ok()) {
            Some(n) if n.unsigned_abs() <= (1u64 << 53) => json!(n),
            _ => v,
        },
        "JSON" => v.as_str().and_then(|s| serde_json::from_str(s).ok()).unwrap_or(v),
        "ARRAY" => match (t.get("arrayElementType"), v) {
            (Some(et), Value::Array(items)) => Value::Array(items.into_iter().map(|x| cell(et, x)).collect()),
            (_, other) => other,
        },
        _ => v,
    }
}

fn to_result(v: &Value, sql: &str, t0: Instant) -> SqlResult {
    let fields = v["metadata"]["rowType"]["fields"].as_array().cloned().unwrap_or_default();
    let columns: Vec<ColumnInfo> = fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let name = f.get("name").and_then(|n| n.as_str()).filter(|n| !n.is_empty()).map(String::from).unwrap_or_else(|| format!("_{i}"));
            ColumnInfo::new(name, type_label(&f["type"]))
        })
        .collect();
    let rows: Vec<Vec<Value>> = v["rows"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|r| {
                    r.as_array()
                        .map(|cells| {
                            cells
                                .iter()
                                .zip(fields.iter())
                                .map(|(c, f)| super::sql_util::cap_json_value("text", cell(&f["type"], c.clone())))
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default();
    let changed = v["stats"]["rowCountExact"].as_str().and_then(|s| s.parse::<i64>().ok());
    let (row_count, message) = if columns.is_empty() {
        let n = changed.unwrap_or(0);
        (Some(n), Some(format!("{n} row{} affected", if n == 1 { "" } else { "s" })))
    } else {
        (Some(rows.len() as i64), None)
    };
    SqlResult { columns, rows, row_count, message, query_ms: t0.elapsed().as_millis() as u64, sql: sql.to_string() }
}

/// The parameter type Spanner wants for a column type (`STRING(MAX)` →
/// `STRING`). Arrays and structs can't be edited from a cell.
fn base_type(spanner_type: &str) -> String {
    let t = spanner_type.trim().to_ascii_uppercase();
    let t = t.split('(').next().unwrap_or(&t).trim().to_string();
    // PostgreSQL-dialect spellings.
    match t.as_str() {
        "BIGINT" | "INT8" => "INT64".into(),
        "DOUBLE PRECISION" | "FLOAT8" => "FLOAT64".into(),
        "REAL" | "FLOAT4" => "FLOAT32".into(),
        "BOOLEAN" => "BOOL".into(),
        "CHARACTER VARYING" | "VARCHAR" | "TEXT" => "STRING".into(),
        "BYTEA" => "BYTES".into(),
        "TIMESTAMP WITH TIME ZONE" | "TIMESTAMPTZ" => "TIMESTAMP".into(),
        "JSONB" => "JSON".into(),
        _ => t,
    }
}

/// A cell's value as a Spanner query parameter: the JSON encoding Spanner
/// expects for its type (INT64, NUMERIC, dates and bytes as strings).
fn param(ty: &str, v: &Value) -> Result<(Value, Value), String> {
    let code = base_type(ty);
    if code.starts_with("ARRAY") || code.starts_with("STRUCT") {
        return Err(format!("{ty} values can't be edited from the grid yet; use an UPDATE in the SQL console"));
    }
    let t = json!({ "code": code });
    if v.is_null() {
        return Ok((Value::Null, t));
    }
    let text = match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let encoded = match code.as_str() {
        "BOOL" => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "t" | "1" | "yes" => json!(true),
            "false" | "f" | "0" | "no" => json!(false),
            _ => return Err(format!("\"{text}\" is not a BOOL")),
        },
        "FLOAT64" | "FLOAT32" => match text.trim().parse::<f64>() {
            Ok(f) if f.is_finite() => json!(f),
            _ => json!(text.trim()),
        },
        "INT64" => {
            text.trim().parse::<i64>().map_err(|_| format!("\"{text}\" is not an INT64"))?;
            json!(text.trim())
        }
        "JSON" => match v {
            Value::String(s) => json!(s),
            other => json!(other.to_string()),
        },
        _ => json!(text),
    };
    Ok((encoded, t))
}

// ── Statements ────────────────────────────────────────────────────────────────

fn kind(stmt: &str) -> &'static str {
    match super::sql_util::statement_head(stmt).as_str() {
        "select" | "with" | "show" | "graph" | "@" => "read",
        "insert" | "update" | "delete" => "dml",
        "create" | "alter" | "drop" | "grant" | "revoke" | "rename" | "analyze" => "ddl",
        _ => "read",
    }
}

/// Whether a console statement returns rows.
pub fn reads(stmt: &str) -> bool {
    kind(stmt) == "read"
}

async fn read(h: &SpannerConn, sql: &str, params: Option<(Map<String, Value>, Map<String, Value>)>) -> Result<Value, String> {
    let mut body = json!({ "sql": sql });
    if let Some((p, t)) = params {
        body["params"] = Value::Object(p);
        body["paramTypes"] = Value::Object(t);
    }
    execute(h, &body).await
}

/// A DML statement in its own read-write transaction, committed on success.
async fn dml(h: &SpannerConn, sql: &str, params: Option<(Map<String, Value>, Map<String, Value>)>) -> Result<Value, String> {
    for attempt in 0..2 {
        let s = session(h).await?;
        let begun = call(&h.cfg, reqwest::Method::POST, &format!("{s}:beginTransaction"), Some(&json!({ "options": { "readWrite": {} } }))).await;
        let tx = match begun {
            Err(e) if attempt == 0 && is_session_gone(&e) => {
                forget_session(h).await;
                continue;
            }
            other => other?,
        };
        let id = tx.get("id").and_then(|i| i.as_str()).ok_or("Spanner did not start a transaction")?.to_string();
        let mut body = json!({ "sql": sql, "transaction": { "id": id }, "seqno": "1" });
        if let Some((p, t)) = params.clone() {
            body["params"] = Value::Object(p);
            body["paramTypes"] = Value::Object(t);
        }
        let out = call(&h.cfg, reqwest::Method::POST, &format!("{s}:executeSql"), Some(&body)).await;
        match out {
            Ok(v) => {
                call(&h.cfg, reqwest::Method::POST, &format!("{s}:commit"), Some(&json!({ "transactionId": id }))).await?;
                return Ok(v);
            }
            Err(e) => {
                let _ = call(&h.cfg, reqwest::Method::POST, &format!("{s}:rollback"), Some(&json!({ "transactionId": id }))).await;
                return Err(e);
            }
        }
    }
    unreachable_err()
}

/// Schema statements through the database's DDL update; waits for the
/// long-running operation (seconds on Spanner, at once on the emulator).
async fn ddl(h: &SpannerConn, statements: Vec<String>) -> Result<(), String> {
    let mut op = call(
        &h.cfg,
        reqwest::Method::PATCH,
        &format!("{}/ddl", database_path(&h.cfg)),
        Some(&json!({ "statements": statements })),
    )
    .await?;
    let deadline = Instant::now() + Duration::from_secs(600);
    while op.get("done").and_then(|d| d.as_bool()) != Some(true) {
        if Instant::now() > deadline {
            return Err("The schema change is still running on Spanner; check the database in a minute".into());
        }
        tokio::time::sleep(Duration::from_millis(800)).await;
        let name = op.get("name").and_then(|n| n.as_str()).ok_or("Spanner did not name the schema operation")?.to_string();
        op = call(&h.cfg, reqwest::Method::GET, &name, None).await?;
    }
    if let Some(e) = op.get("error") {
        return Err(format!("Spanner rejected the schema change: {}", e.get("message").and_then(|m| m.as_str()).unwrap_or("unknown error")));
    }
    Ok(())
}

/// Run one statement, the console's way: reads come back as rows, DML says
/// how many rows changed, DDL says it was applied.
/// Backtick-quoted names → double-quoted ones, for a PostgreSQL-dialect
/// database. The app quotes Spanner names GoogleSQL's way (it can't know the
/// dialect before connecting), and a backtick is never valid PostgreSQL, so
/// outside strings, comments and quoted names this only ever fixes a name.
fn pg_quotes(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut it = sql.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\'' | '"' => {
                out.push(c);
                while let Some(d) = it.next() {
                    out.push(d);
                    if d == c {
                        if it.peek() == Some(&c) {
                            out.push(it.next().unwrap());
                        } else {
                            break;
                        }
                    }
                }
            }
            '-' if it.peek() == Some(&'-') => {
                out.push(c);
                for d in it.by_ref() {
                    out.push(d);
                    if d == '\n' {
                        break;
                    }
                }
            }
            '/' if it.peek() == Some(&'*') => {
                out.push(c);
                out.push(it.next().unwrap());
                let mut prev = ' ';
                for d in it.by_ref() {
                    out.push(d);
                    if prev == '*' && d == '/' {
                        break;
                    }
                    prev = d;
                }
            }
            '`' => {
                out.push('"');
                while let Some(d) = it.next() {
                    match d {
                        '`' if it.peek() == Some(&'`') => {
                            it.next();
                            out.push('`');
                        }
                        '`' => break,
                        '"' => out.push_str("\"\""),
                        _ => out.push(d),
                    }
                }
                out.push('"');
            }
            _ => out.push(c),
        }
    }
    out
}

/// The statement as this database's dialect reads it.
fn dialect_sql(h: &SpannerConn, sql: &str) -> String {
    if h.dialect == Dialect::Postgres && sql.contains('`') {
        pg_quotes(sql)
    } else {
        sql.to_string()
    }
}

pub async fn query(h: &SpannerConn, stmt: &str) -> Result<SqlResult, String> {
    let t0 = Instant::now();
    let fixed = dialect_sql(h, stmt);
    let sql = fixed.trim().trim_end_matches(';').trim();
    if sql.is_empty() {
        return Err("Query is empty".into());
    }
    match kind(sql) {
        "dml" => Ok(to_result(&dml(h, sql, None).await?, sql, t0)),
        "ddl" => {
            ddl(h, vec![sql.to_string()]).await?;
            Ok(SqlResult { columns: vec![], rows: vec![], row_count: Some(0), message: Some("Schema updated".into()), query_ms: t0.elapsed().as_millis() as u64, sql: sql.to_string() })
        }
        _ => Ok(to_result(&read(h, sql, None).await?, sql, t0)),
    }
}

// ── Plans ─────────────────────────────────────────────────────────────────────

/// A query's plan in PLAN mode (nothing runs), as the plan view's tree.
pub async fn explain_plan(h: &SpannerConn, sql: &str) -> Result<Value, String> {
    let fixed = dialect_sql(h, sql);
    let sql = fixed.trim().trim_end_matches(';').trim();
    let sql = sql.strip_prefix("EXPLAIN ").or_else(|| sql.strip_prefix("explain ")).unwrap_or(sql);
    let v = execute(h, &json!({ "sql": sql, "queryMode": "PLAN" })).await?;
    let nodes = v["stats"]["queryPlan"]["planNodes"].as_array().cloned().unwrap_or_default();
    if nodes.is_empty() {
        return Err("Spanner returned no plan for this statement".into());
    }
    Ok(plan_tree(&nodes, 0, 0))
}

/// Spanner's flat plan nodes (indexed, with child links) → nested nodes.
/// Scalar nodes (expressions) fold into their parent's details.
pub fn plan_tree(nodes: &[Value], index: usize, depth: usize) -> Value {
    let n = &nodes[index.min(nodes.len().saturating_sub(1))];
    let name = n.get("displayName").and_then(|d| d.as_str()).unwrap_or("Step");
    let meta = n.get("metadata").cloned().unwrap_or(Value::Null);
    let mut out = Map::new();
    let label = match meta.get("scan_type").and_then(|s| s.as_str()) {
        Some(scan) => format!("{name} ({scan})"),
        None => match meta.get("call_type").and_then(|s| s.as_str()) {
            Some(call) => format!("{name} ({call})"),
            None => name.to_string(),
        },
    };
    out.insert("Node Type".into(), json!(label));
    if let Some(t) = meta.get("scan_target").and_then(|s| s.as_str()) {
        out.insert("Relation Name".into(), json!(t));
    }
    let mut conditions: Vec<String> = Vec::new();
    let mut kids: Vec<Value> = Vec::new();
    if depth < 64 {
        for link in n.get("childLinks").and_then(|c| c.as_array()).into_iter().flatten() {
            let Some(ci) = link.get("childIndex").and_then(|c| c.as_u64()).map(|c| c as usize) else { continue };
            let Some(child) = nodes.get(ci) else { continue };
            if child.get("kind").and_then(|k| k.as_str()) == Some("SCALAR") {
                if let Some(desc) = child.get("shortRepresentation").and_then(|s| s.get("description")).and_then(|d| d.as_str()) {
                    let role = link.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    conditions.push(if role.is_empty() { desc.to_string() } else { format!("{role}: {desc}") });
                }
                continue;
            }
            kids.push(plan_tree(nodes, ci, depth + 1));
        }
    }
    if !conditions.is_empty() {
        out.insert("Filter".into(), json!(conditions.join(", ")));
    }
    if !kids.is_empty() {
        out.insert("Plans".into(), Value::Array(kids));
    }
    Value::Object(out)
}

// ── Identifiers and schemas ───────────────────────────────────────────────────

pub fn quote_ident(h: &SpannerConn, ident: &str) -> String {
    match h.dialect {
        Dialect::GoogleSql => super::sql_util::quote_backtick(ident),
        Dialect::Postgres => super::sql_util::quote_double(ident),
    }
}

/// The schema as `information_schema` names it: GoogleSQL's unnamed one is ''.
fn schema_key(h: &SpannerConn, schema: &str) -> String {
    let s = schema.trim();
    match h.dialect {
        Dialect::GoogleSql if s.is_empty() || s == DEFAULT_SCHEMA => String::new(),
        Dialect::Postgres if s.is_empty() => "public".into(),
        _ => s.to_string(),
    }
}

pub(crate) fn table_ref(h: &SpannerConn, schema: &str, table: &str) -> String {
    let key = schema_key(h, schema);
    if key.is_empty() || (h.dialect == Dialect::Postgres && key == "public") {
        quote_ident(h, table)
    } else {
        format!("{}.{}", quote_ident(h, &key), quote_ident(h, table))
    }
}

/// Parameters as `@p` (GoogleSQL) or `$1` (PostgreSQL dialect), with types.
struct Params {
    dialect: Dialect,
    values: Map<String, Value>,
    types: Map<String, Value>,
}

impl Params {
    fn new(dialect: Dialect) -> Self {
        Self { dialect, values: Map::new(), types: Map::new() }
    }
    /// Bind `v`, returning the placeholder to put in the SQL.
    fn bind(&mut self, value: Value, ty: Value) -> String {
        let n = self.values.len() + 1;
        let (key, placeholder) = match self.dialect {
            Dialect::GoogleSql => (format!("p{n}"), format!("@p{n}")),
            Dialect::Postgres => (format!("p{n}"), format!("${n}")),
        };
        self.values.insert(key.clone(), value);
        self.types.insert(key, ty);
        placeholder
    }
    fn string(&mut self, s: &str) -> String {
        self.bind(json!(s), json!({ "code": "STRING" }))
    }
    fn take(self) -> Option<(Map<String, Value>, Map<String, Value>)> {
        if self.values.is_empty() { None } else { Some((self.values, self.types)) }
    }
}

fn text_of(r: &SqlResult, row: &[Value], name: &str) -> String {
    r.columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(name))
        .and_then(|i| row.get(i))
        .map(|v| match v {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        })
        .unwrap_or_default()
}

async fn catalog(h: &SpannerConn, sql: &str, p: Params) -> Result<SqlResult, String> {
    let t0 = Instant::now();
    Ok(to_result(&read(h, sql, p.take()).await?, sql, t0))
}

pub async fn list_schemas(h: &SpannerConn) -> Result<Vec<String>, String> {
    let r = catalog(
        h,
        "SELECT schema_name FROM information_schema.schemata ORDER BY schema_name",
        Params::new(h.dialect),
    )
    .await?;
    let hidden = ["information_schema", "spanner_sys", "pg_catalog"];
    let mut out: Vec<String> = r
        .rows
        .iter()
        .map(|row| text_of(&r, row, "schema_name"))
        .filter(|s| !hidden.contains(&s.to_ascii_lowercase().as_str()))
        .map(|s| if s.is_empty() { DEFAULT_SCHEMA.to_string() } else { s })
        .collect();
    if out.is_empty() {
        out.push(if h.dialect == Dialect::Postgres { "public".into() } else { DEFAULT_SCHEMA.into() });
    }
    Ok(out)
}

pub async fn list_tables(h: &SpannerConn, schema: &str) -> Result<Vec<TableInfo>, String> {
    let mut p = Params::new(h.dialect);
    let s = p.string(&schema_key(h, schema));
    let sql = format!("SELECT table_name, table_type FROM information_schema.tables WHERE table_schema = {s} ORDER BY table_name");
    let r = catalog(h, &sql, p).await?;
    Ok(r.rows
        .iter()
        .map(|row| {
            let ty = text_of(&r, row, "table_type").to_ascii_uppercase();
            TableInfo {
                name: text_of(&r, row, "table_name"),
                row_count: -1,
                kind: if ty.contains("VIEW") { "view".into() } else { "table".into() },
                rls_enabled: None,
                created_at: None,
                create_order: None,
                system: None,
            }
        })
        .collect())
}

/// Columns of one table, or of every table in the schema, with primary and
/// foreign keys filled in. Returns (table, column) pairs in table order.
async fn columns_in(h: &SpannerConn, schema: &str, table: Option<&str>) -> Result<Vec<(String, ColumnStructureRow)>, String> {
    let key = schema_key(h, schema);
    let mut p = Params::new(h.dialect);
    let s = p.string(&key);
    let mut cols_sql = format!(
        "SELECT table_name, column_name, ordinal_position, spanner_type, is_nullable, column_default \
         FROM information_schema.columns WHERE table_schema = {s}"
    );
    if let Some(t) = table {
        let tp = p.string(t);
        cols_sql.push_str(&format!(" AND table_name = {tp}"));
    }
    cols_sql.push_str(" ORDER BY table_name, ordinal_position");
    let cols = catalog(h, &cols_sql, p).await?;

    let mut p = Params::new(h.dialect);
    let s = p.string(&key);
    let pk_sql = format!(
        "SELECT table_name, column_name FROM information_schema.index_columns \
         WHERE table_schema = {s} AND index_type = 'PRIMARY_KEY'"
    );
    let pks = catalog(h, &pk_sql, p).await.unwrap_or_else(|_| SqlResult { columns: vec![], rows: vec![], row_count: None, message: None, query_ms: 0, sql: String::new() });
    let pk_set: std::collections::HashSet<(String, String)> =
        pks.rows.iter().map(|r| (text_of(&pks, r, "table_name"), text_of(&pks, r, "column_name"))).collect();

    let fks = foreign_keys_in(h, schema).await.unwrap_or_default();

    Ok(cols
        .rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let t = text_of(&cols, row, "table_name");
            let name = text_of(&cols, row, "column_name");
            let ty = text_of(&cols, row, "spanner_type");
            let default = Some(text_of(&cols, row, "column_default")).filter(|d| !d.is_empty());
            let ordinal = text_of(&cols, row, "ordinal_position").parse::<i32>().unwrap_or((i + 1) as i32);
            let fk = fks.iter().find(|f| f.0 == t && f.1 == name);
            (
                t.clone(),
                ColumnStructureRow {
                    ordinal_position: ordinal,
                    name: name.clone(),
                    data_type: ty,
                    is_nullable: text_of(&cols, row, "is_nullable").eq_ignore_ascii_case("YES"),
                    column_default: default,
                    foreign_key: fk.map(|f| format!("{}.{}", f.2, f.3)),
                    fk_constraint_name: fk.map(|f| f.4.clone()),
                    is_primary_key: pk_set.contains(&(t, name)),
                    comment: None,
                },
            )
        })
        .collect())
}

/// (table, column, referenced table, referenced column, constraint) per FK column.
async fn foreign_keys_in(h: &SpannerConn, schema: &str) -> Result<Vec<(String, String, String, String, String)>, String> {
    let mut p = Params::new(h.dialect);
    let s = p.string(&schema_key(h, schema));
    let sql = format!(
        "SELECT kcu.table_name, kcu.column_name, ccu.table_name AS ref_table, ccu.column_name AS ref_column, tc.constraint_name \
         FROM information_schema.table_constraints AS tc \
         JOIN information_schema.key_column_usage AS kcu \
           ON kcu.constraint_name = tc.constraint_name AND kcu.constraint_schema = tc.constraint_schema \
         JOIN information_schema.constraint_column_usage AS ccu \
           ON ccu.constraint_name = tc.constraint_name AND ccu.constraint_schema = tc.constraint_schema \
         WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = {s}"
    );
    let r = catalog(h, &sql, p).await?;
    let mut out: Vec<(String, String, String, String, String)> = r
        .rows
        .iter()
        .map(|row| {
            (
                text_of(&r, row, "table_name"),
                text_of(&r, row, "column_name"),
                text_of(&r, row, "ref_table"),
                text_of(&r, row, "ref_column"),
                text_of(&r, row, "constraint_name"),
            )
        })
        .collect();
    out.extend(interleave_links(h, schema).await.unwrap_or_default());
    Ok(out)
}

/// Interleaved tables as relations. Spanner keeps a child's rows inside its
/// parent's (`INTERLEAVE IN PARENT`) instead of declaring a foreign key, so a
/// schema built that way, the usual Spanner design, showed no relations at
/// all. A child's primary key starts with its parent's key columns under the
/// same names, which makes each of them a link to the parent.
async fn interleave_links(h: &SpannerConn, schema: &str) -> Result<Vec<(String, String, String, String, String)>, String> {
    let mut p = Params::new(h.dialect);
    let s = p.string(&schema_key(h, schema));
    let parents = catalog(
        h,
        &format!("SELECT table_name, parent_table_name FROM information_schema.tables WHERE table_schema = {s} AND parent_table_name IS NOT NULL"),
        p,
    )
    .await?;
    if parents.rows.is_empty() {
        return Ok(vec![]);
    }
    let mut p = Params::new(h.dialect);
    let s = p.string(&schema_key(h, schema));
    let keys = catalog(
        h,
        &format!("SELECT table_name, column_name, ordinal_position FROM information_schema.index_columns WHERE table_schema = {s} AND index_type = 'PRIMARY_KEY' ORDER BY table_name, ordinal_position"),
        p,
    )
    .await?;
    let key_of = |t: &str| -> Vec<String> {
        keys.rows.iter().filter(|r| text_of(&keys, r, "table_name") == t).map(|r| text_of(&keys, r, "column_name")).collect()
    };
    let mut out = Vec::new();
    for row in &parents.rows {
        let child = text_of(&parents, row, "table_name");
        let parent = text_of(&parents, row, "parent_table_name");
        if child.is_empty() || parent.is_empty() {
            continue;
        }
        let child_key = key_of(&child);
        for col in key_of(&parent) {
            if child_key.contains(&col) {
                out.push((child.clone(), col.clone(), parent.clone(), col, format!("INTERLEAVE_{child}")));
            }
        }
    }
    Ok(out)
}

/// Foreign keys in this schema that point at `table`, for the grid's
/// "referenced by" jumps.
pub async fn incoming_foreign_keys(h: &SpannerConn, schema: &str, table: &str) -> Result<Vec<super::schema::IncomingForeignKey>, String> {
    let mut by_fk: Vec<super::schema::IncomingForeignKey> = Vec::new();
    for (from_table, col, ref_table, ref_col, name) in foreign_keys_in(h, schema).await? {
        if ref_table != table {
            continue;
        }
        let i = match by_fk.iter().position(|f| f.from_table == from_table && f.constraint_name == name) {
            Some(i) => i,
            None => {
                by_fk.push(super::schema::IncomingForeignKey {
                    from_schema: schema.to_string(),
                    from_table,
                    from_columns: vec![],
                    to_columns: vec![],
                    constraint_name: name,
                });
                by_fk.len() - 1
            }
        };
        let fk = &mut by_fk[i];
        if !fk.from_columns.contains(&col) {
            fk.from_columns.push(col);
            fk.to_columns.push(ref_col);
        }
    }
    Ok(by_fk)
}

pub async fn get_column_structure(h: &SpannerConn, schema: &str, table: &str) -> Result<Vec<ColumnStructureRow>, String> {
    Ok(columns_in(h, schema, Some(table)).await?.into_iter().map(|(_, c)| c).collect())
}

pub async fn schema_column_structure(h: &SpannerConn, schema: &str) -> Result<Vec<super::schema::TableColumnStructure>, String> {
    let mut out: Vec<super::schema::TableColumnStructure> = Vec::new();
    for (table, col) in columns_in(h, schema, None).await? {
        match out.last_mut() {
            Some(last) if last.table == table => last.columns.push(col),
            _ => out.push(super::schema::TableColumnStructure { table, columns: vec![col] }),
        }
    }
    Ok(out)
}

pub async fn list_indexes(h: &SpannerConn, schema: &str) -> Result<Vec<IndexInfo>, String> {
    let key = schema_key(h, schema);
    let mut p = Params::new(h.dialect);
    let s = p.string(&key);
    let sql = format!(
        "SELECT table_name, index_name, index_type, is_unique FROM information_schema.indexes \
         WHERE table_schema = {s} ORDER BY table_name, index_name"
    );
    let idx = catalog(h, &sql, p).await?;
    let mut p = Params::new(h.dialect);
    let s = p.string(&key);
    let sql = format!(
        "SELECT table_name, index_name, column_name, ordinal_position FROM information_schema.index_columns \
         WHERE table_schema = {s} AND ordinal_position IS NOT NULL ORDER BY table_name, index_name, ordinal_position"
    );
    let cols = catalog(h, &sql, p).await?;
    let mut by_index: HashMap<(String, String), Vec<String>> = HashMap::new();
    for row in &cols.rows {
        by_index
            .entry((text_of(&cols, row, "table_name"), text_of(&cols, row, "index_name")))
            .or_default()
            .push(text_of(&cols, row, "column_name"));
    }
    Ok(idx
        .rows
        .iter()
        .map(|row| {
            let table = text_of(&idx, row, "table_name");
            let name = text_of(&idx, row, "index_name");
            let kind = text_of(&idx, row, "index_type");
            let unique = matches!(text_of(&idx, row, "is_unique").to_ascii_lowercase().as_str(), "true" | "yes");
            let columns = by_index.get(&(table.clone(), name.clone())).cloned().unwrap_or_default().join(", ");
            IndexInfo {
                is_primary: kind == "PRIMARY_KEY",
                name: if kind == "PRIMARY_KEY" { "PRIMARY_KEY".into() } else { name },
                table_name: table,
                columns,
                index_type: kind.to_ascii_lowercase(),
                is_unique: unique || kind == "PRIMARY_KEY",
                condition: None,
                comment: None,
            }
        })
        .collect())
}

/// The table's DDL as Spanner keeps it: its CREATE TABLE and its indexes.
pub async fn get_ddl(h: &SpannerConn, _schema: &str, table: &str) -> Result<String, String> {
    let v = call(&h.cfg, reqwest::Method::GET, &format!("{}/ddl", database_path(&h.cfg)), None).await?;
    let lower = table.to_ascii_lowercase();
    let names = [format!(" {lower} "), format!(" {lower}("), format!("`{lower}`"), format!("\"{lower}\""), format!(" {lower}\n")];
    let mine: Vec<String> = v["statements"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s.as_str())
        .filter(|s| {
            let l = format!("{} ", s.to_ascii_lowercase());
            let head = l.split('(').next().unwrap_or(&l).to_string();
            names.iter().any(|n| head.contains(n.as_str()) || l.contains(&format!(" on {lower}")) || l.contains(&format!(" on `{lower}`")))
        })
        .map(|s| format!("{};", s.trim_end_matches(';')))
        .collect();
    Ok(mine.join("\n\n"))
}

// ── The grid ──────────────────────────────────────────────────────────────────

/// A column as text for search and contains-filters.
fn as_text(h: &SpannerConn, col: &str, ty: &str) -> String {
    let base = base_type(ty);
    match h.dialect {
        Dialect::Postgres => format!("CAST({col} AS text)"),
        Dialect::GoogleSql if base == "STRING" => col.to_string(),
        Dialect::GoogleSql if base.starts_with("ARRAY") || base.starts_with("STRUCT") || base == "JSON" || base == "BYTES" => {
            format!("TO_JSON_STRING({col})")
        }
        Dialect::GoogleSql => format!("CAST({col} AS STRING)"),
    }
}

fn build_where(h: &SpannerConn, cols: &[ColumnStructureRow], search: Option<&str>, filters: Option<&[RowFilter]>, p: &mut Params) -> String {
    let by_name: HashMap<&str, &ColumnStructureRow> = cols.iter().map(|c| (c.name.as_str(), c)).collect();
    let mut clauses: Vec<String> = Vec::new();
    if let Some(q) = search.map(str::trim).filter(|s| !s.is_empty()) {
        let needle = p.string(&format!("%{}%", q.to_lowercase()));
        let ors: Vec<String> =
            cols.iter().map(|c| format!("LOWER({}) LIKE {needle}", as_text(h, &quote_ident(h, &c.name), &c.data_type))).collect();
        if !ors.is_empty() {
            clauses.push(format!("({})", ors.join(" OR ")));
        }
    }
    for f in filters.unwrap_or(&[]) {
        let Some(c) = by_name.get(f.column.as_str()) else { continue };
        let col = quote_ident(h, &c.name);
        let v = f.value.as_deref().unwrap_or("");
        let typed = |p: &mut Params, raw: &str| -> Option<String> { param(&c.data_type, &json!(raw)).ok().map(|(val, t)| p.bind(val, t)) };
        let text = as_text(h, &col, &c.data_type);
        let clause = match f.op.as_str() {
            "=" | "eq" | "!=" | "ne" | "neq" | ">" | "gt" | ">=" | "gte" | "<" | "lt" | "<=" | "lte" => {
                let op = match f.op.as_str() {
                    "=" | "eq" => "=",
                    "!=" | "ne" | "neq" => "!=",
                    ">" | "gt" => ">",
                    ">=" | "gte" => ">=",
                    "<" | "lt" => "<",
                    _ => "<=",
                };
                match typed(p, v) {
                    Some(ph) => format!("{col} {op} {ph}"),
                    None => continue,
                }
            }
            "contains" => format!("LOWER({text}) LIKE {}", p.string(&format!("%{}%", v.to_lowercase()))),
            "not_contains" => format!("({col} IS NULL OR NOT LOWER({text}) LIKE {})", p.string(&format!("%{}%", v.to_lowercase()))),
            "starts_with" => format!("LOWER({text}) LIKE {}", p.string(&format!("{}%", v.to_lowercase()))),
            "ends_with" => format!("LOWER({text}) LIKE {}", p.string(&format!("%{}", v.to_lowercase()))),
            "between" => {
                let mut parts = v.splitn(2, ',');
                let (a, b) = (parts.next().unwrap_or("").trim().to_string(), parts.next().unwrap_or("").trim().to_string());
                match (typed(p, &a), typed(p, &b)) {
                    (Some(x), Some(y)) => format!("{col} BETWEEN {x} AND {y}"),
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

#[allow(clippy::too_many_arguments)]
pub async fn get_table_rows(
    h: &SpannerConn,
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
    let mut p = Params::new(h.dialect);
    let where_clause = build_where(h, &cols, search.as_deref(), filters.as_deref(), &mut p);
    let order = match sort_column.as_deref().filter(|c| cols.iter().any(|x| x.name == *c)) {
        Some(c) => {
            let d = if sort_direction.as_deref().is_some_and(|d| d.eq_ignore_ascii_case("desc")) { "DESC" } else { "ASC" };
            format!(" ORDER BY {} {d}", quote_ident(h, c))
        }
        None => {
            // A stable page order: the primary key.
            let pk: Vec<String> = cols.iter().filter(|c| c.is_primary_key).map(|c| quote_ident(h, &c.name)).collect();
            if pk.is_empty() { String::new() } else { format!(" ORDER BY {}", pk.join(", ")) }
        }
    };
    let (limit, offset) = (limit.max(0), offset.max(0));
    let data_sql = format!("SELECT * FROM {tref}{where_clause}{order} LIMIT {limit} OFFSET {offset}");
    let count_sql = format!("SELECT COUNT(*) AS n FROM {tref}{where_clause}");
    let params = p.take();
    let (data, count) = tokio::join!(read(h, &data_sql, params.clone()), read(h, &count_sql, params));
    let result = to_result(&data?, &data_sql, t0);
    let total = count?["rows"][0][0].as_str().and_then(|s| s.parse::<i64>().ok()).unwrap_or(result.rows.len() as i64);
    let fks: Vec<ForeignKeyInfo> = cols
        .iter()
        .filter_map(|c| {
            let target = c.foreign_key.as_ref()?;
            let (t, col) = target.split_once('.')?;
            Some(ForeignKeyInfo {
                columns: vec![c.name.clone()],
                referenced_schema: schema.to_string(),
                referenced_table: t.to_string(),
                referenced_columns: vec![col.to_string()],
            })
        })
        .collect();
    Ok(TableRows {
        preview_columns: Vec::new(),
        columns: result.columns,
        rows: result.rows,
        total,
        query_ms: t0.elapsed().as_millis() as u64,
        primary_key: cols.iter().filter(|c| c.is_primary_key).map(|c| c.name.clone()).collect(),
        foreign_keys: fks,
        sql: format!("{data_sql}\n{count_sql}"),
    })
}

pub async fn count_rows(h: &SpannerConn, schema: &str, table: &str) -> Result<i64, String> {
    let sql = format!("SELECT COUNT(*) FROM {}", table_ref(h, schema, table));
    let v = read(h, &sql, None).await?;
    Ok(v["rows"][0][0].as_str().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0))
}

fn column_types(cols: &[ColumnStructureRow]) -> HashMap<String, String> {
    cols.iter().map(|c| (c.name.clone(), c.data_type.clone())).collect()
}

fn pk_where(h: &SpannerConn, types: &HashMap<String, String>, pk: &HashMap<String, Value>, p: &mut Params) -> Result<String, String> {
    if pk.is_empty() {
        return Err("Spanner needs the row's primary key to change it".into());
    }
    let mut parts = Vec::new();
    for (k, v) in pk {
        let ty = types.get(k).ok_or_else(|| format!("No column named {k}"))?;
        let (val, t) = param(ty, v)?;
        parts.push(format!("{} = {}", quote_ident(h, k), p.bind(val, t)));
    }
    Ok(parts.join(" AND "))
}

pub async fn update_table_cell(
    h: &SpannerConn,
    schema: &str,
    table: &str,
    primary_key: HashMap<String, Value>,
    column: &str,
    value: &Value,
) -> Result<(), String> {
    let cols = get_column_structure(h, schema, table).await?;
    let types = column_types(&cols);
    let ty = types.get(column).ok_or_else(|| format!("No column named {column}"))?;
    let mut p = Params::new(h.dialect);
    let (val, t) = param(ty, value)?;
    let set = format!("{} = {}", quote_ident(h, column), p.bind(val, t));
    let w = pk_where(h, &types, &primary_key, &mut p)?;
    let sql = format!("UPDATE {} SET {set} WHERE {w}", table_ref(h, schema, table));
    let v = dml(h, &sql, p.take()).await?;
    match v["stats"]["rowCountExact"].as_str() {
        Some("1") => Ok(()),
        Some(n) => Err(super::query::row_count_error(&primary_key.keys().cloned().collect::<Vec<_>>(), &primary_key, n.parse().unwrap_or(0))),
        None => Ok(()),
    }
}

pub async fn insert_table_row(h: &SpannerConn, schema: &str, table: &str, values: HashMap<String, Value>) -> Result<Vec<Value>, String> {
    let cols = get_column_structure(h, schema, table).await?;
    let types = column_types(&cols);
    let mut p = Params::new(h.dialect);
    let mut names = Vec::new();
    let mut holes = Vec::new();
    for c in &cols {
        let Some(v) = values.get(&c.name) else { continue };
        let (val, t) = param(&c.data_type, v)?;
        names.push(quote_ident(h, &c.name));
        holes.push(p.bind(val, t));
    }
    if names.is_empty() {
        return Err("Fill in at least the primary key".into());
    }
    let sql = format!("INSERT INTO {} ({}) VALUES ({})", table_ref(h, schema, table), names.join(", "), holes.join(", "));
    dml(h, &sql, p.take()).await?;
    // Read the row back by its key, so defaults and commit timestamps show.
    let pk: HashMap<String, Value> =
        cols.iter().filter(|c| c.is_primary_key).filter_map(|c| Some((c.name.clone(), values.get(&c.name)?.clone()))).collect();
    if pk.len() != cols.iter().filter(|c| c.is_primary_key).count() || pk.is_empty() {
        return Ok(cols.iter().map(|c| values.get(&c.name).cloned().unwrap_or(Value::Null)).collect());
    }
    let mut p = Params::new(h.dialect);
    let w = pk_where(h, &types, &pk, &mut p)?;
    let sql = format!("SELECT * FROM {} WHERE {w}", table_ref(h, schema, table));
    let r = to_result(&read(h, &sql, p.take()).await?, &sql, Instant::now());
    Ok(r.rows.into_iter().next().unwrap_or_default())
}

pub async fn delete_table_rows(h: &SpannerConn, schema: &str, table: &str, primary_keys: Vec<HashMap<String, Value>>) -> Result<u64, String> {
    if primary_keys.is_empty() {
        return Ok(0);
    }
    let cols = get_column_structure(h, schema, table).await?;
    let types = column_types(&cols);
    let mut p = Params::new(h.dialect);
    let mut ors = Vec::new();
    for pk in &primary_keys {
        ors.push(format!("({})", pk_where(h, &types, pk, &mut p)?));
    }
    let sql = format!("DELETE FROM {} WHERE {}", table_ref(h, schema, table), ors.join(" OR "));
    let v = dml(h, &sql, p.take()).await?;
    Ok(v["stats"]["rowCountExact"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    #[test]
    fn backticks_become_pg_quotes() {
        assert_eq!(pg_quotes("SELECT * FROM `public`.`items` WHERE a = 'x`y'"), "SELECT * FROM \"public\".\"items\" WHERE a = 'x`y'");
        assert_eq!(pg_quotes("SELECT `a``b`, `c\"d` -- `keep`\nFROM t /* `no` */"), "SELECT \"a`b\", \"c\"\"d\" -- `keep`\nFROM t /* `no` */");
        assert_eq!(pg_quotes("SELECT \"x`\" FROM t"), "SELECT \"x`\" FROM t");
    }

    use super::*;

    #[test]
    fn values_come_back_as_the_grid_shows_them() {
        assert_eq!(cell(&json!({"code": "INT64"}), json!("42")), json!(42));
        assert_eq!(cell(&json!({"code": "INT64"}), json!("9223372036854775807")), json!("9223372036854775807"));
        assert_eq!(cell(&json!({"code": "JSON"}), json!("{\"a\":1}")), json!({"a": 1}));
        assert_eq!(
            cell(&json!({"code": "ARRAY", "arrayElementType": {"code": "INT64"}}), json!(["1", "2"])),
            json!([1, 2])
        );
        assert_eq!(type_label(&json!({"code": "ARRAY", "arrayElementType": {"code": "STRING"}})), "ARRAY<STRING>");
    }

    #[test]
    fn cells_bind_as_typed_parameters() {
        assert_eq!(param("INT64", &json!("7")).unwrap(), (json!("7"), json!({"code": "INT64"})));
        assert!(param("INT64", &json!("seven")).is_err());
        assert_eq!(param("BOOL", &json!("true")).unwrap().0, json!(true));
        assert_eq!(param("STRING(MAX)", &json!("x")).unwrap(), (json!("x"), json!({"code": "STRING"})));
        assert_eq!(param("FLOAT64", &json!("1.5")).unwrap().0, json!(1.5));
        assert_eq!(param("TIMESTAMP", &Value::Null).unwrap().0, Value::Null);
        assert!(param("ARRAY<STRING(MAX)>", &json!("[]")).is_err());
        assert_eq!(base_type("character varying(20)"), "STRING");
    }

    #[test]
    fn statements_are_classified() {
        assert!(reads("SELECT 1"));
        assert!(reads("WITH a AS (SELECT 1) SELECT * FROM a"));
        assert_eq!(kind("INSERT INTO t (a) VALUES (1)"), "dml");
        assert_eq!(kind("CREATE INDEX i ON t(a)"), "ddl");
    }

    #[test]
    fn plans_nest_and_fold_scalars() {
        let nodes = json!([
            {"index": 0, "kind": "RELATIONAL", "displayName": "Distributed Union", "childLinks": [{"childIndex": 1}]},
            {"index": 1, "kind": "RELATIONAL", "displayName": "Scan", "metadata": {"scan_type": "TableScan", "scan_target": "users"},
             "childLinks": [{"childIndex": 2, "type": "Condition"}]},
            {"index": 2, "kind": "SCALAR", "displayName": "Function", "shortRepresentation": {"description": "($id > 1)"}}
        ]);
        let t = plan_tree(nodes.as_array().unwrap(), 0, 0);
        assert_eq!(t["Node Type"], "Distributed Union");
        assert_eq!(t["Plans"][0]["Node Type"], "Scan (TableScan)");
        assert_eq!(t["Plans"][0]["Relation Name"], "users");
        assert_eq!(t["Plans"][0]["Filter"], "Condition: ($id > 1)");
    }

    /// Interleaved tables show as relations (STROKE_TEST_SPANNER).
    #[tokio::test]
    async fn live_interleaved_relations() {
        let Ok(spec) = std::env::var("STROKE_TEST_SPANNER") else { return };
        let mut parts = spec.splitn(4, '/');
        let (host, project, instance, database) =
            (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap());
        let cfg = SpannerConfig {
            name: "t".into(),
            project_id: project.into(),
            instance_id: instance.into(),
            database_id: database.into(),
            auth: "emulator".into(),
            key_file: None,
            emulator_host: Some(host.into()),
        };
        let h = connect(&cfg).await.unwrap();
        let _ = query(&h, "DROP TABLE StrokeOrders").await;
        let _ = query(&h, "DROP TABLE StrokeCustomers").await;
        query(&h, "CREATE TABLE StrokeCustomers (CustomerId STRING(36) NOT NULL, Name STRING(100)) PRIMARY KEY (CustomerId)").await.unwrap();
        query(&h, "CREATE TABLE StrokeOrders (CustomerId STRING(36) NOT NULL, OrderId STRING(36) NOT NULL, Status STRING(20)) PRIMARY KEY (CustomerId, OrderId), INTERLEAVE IN PARENT StrokeCustomers ON DELETE CASCADE").await.unwrap();
        let cols = get_column_structure(&h, DEFAULT_SCHEMA, "StrokeOrders").await.unwrap();
        assert_eq!(cols.iter().find(|c| c.name == "CustomerId").unwrap().foreign_key.as_deref(), Some("StrokeCustomers.CustomerId"));
        assert!(cols.iter().find(|c| c.name == "OrderId").unwrap().foreign_key.is_none());
        let inc = incoming_foreign_keys(&h, DEFAULT_SCHEMA, "StrokeCustomers").await.unwrap();
        assert!(inc.iter().any(|f| f.from_table == "StrokeOrders" && f.from_columns == vec!["CustomerId".to_string()]), "{inc:?}");
        query(&h, "DROP TABLE StrokeOrders").await.unwrap();
        query(&h, "DROP TABLE StrokeCustomers").await.unwrap();
    }

    /// Against the Spanner emulator:
    ///   docker run -p 9010:9010 -p 9020:9020 gcr.io/cloud-spanner-emulator/emulator
    ///   STROKE_TEST_SPANNER=localhost:9020/test/inst/shop cargo test --lib spanner::tests::live
    #[tokio::test]
    async fn live_emulator_round_trip() {
        let Ok(spec) = std::env::var("STROKE_TEST_SPANNER") else { return };
        let mut parts = spec.splitn(4, '/');
        let (host, project, instance, database) =
            (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap());
        let cfg = SpannerConfig {
            name: "t".into(),
            project_id: project.into(),
            instance_id: instance.into(),
            database_id: database.into(),
            auth: "emulator".into(),
            key_file: None,
            emulator_host: Some(host.into()),
        };
        let h = connect(&cfg).await.unwrap();
        let _ = query(&h, "DROP TABLE stroke_t").await;
        query(&h, "CREATE TABLE stroke_t (id INT64 NOT NULL, name STRING(50), score FLOAT64, ok BOOL, born DATE) PRIMARY KEY (id)").await.unwrap();
        let r = query(&h, "INSERT INTO stroke_t (id, name, score, ok) VALUES (1, 'a', 1.5, true), (2, 'b', NULL, false)").await.unwrap();
        assert_eq!(r.row_count, Some(2));

        assert!(list_schemas(&h).await.unwrap().contains(&DEFAULT_SCHEMA.to_string()));
        let tables = list_tables(&h, DEFAULT_SCHEMA).await.unwrap();
        assert!(tables.iter().any(|t| t.name == "stroke_t"));
        let cols = get_column_structure(&h, DEFAULT_SCHEMA, "stroke_t").await.unwrap();
        assert!(cols.iter().find(|c| c.name == "id").unwrap().is_primary_key);

        let page = get_table_rows(&h, DEFAULT_SCHEMA, "stroke_t", 50, 0, None, None, None, None).await.unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.primary_key, vec!["id".to_string()]);
        let filtered = get_table_rows(
            &h, DEFAULT_SCHEMA, "stroke_t", 50, 0, None, None, None,
            Some(vec![RowFilter { column: "id".into(), op: ">".into(), value: Some("1".into()), conjunct: None, data_type: None }]),
        ).await.unwrap();
        assert_eq!(filtered.total, 1);
        let searched = get_table_rows(&h, DEFAULT_SCHEMA, "stroke_t", 50, 0, Some("B".into()), None, None, None).await.unwrap();
        assert_eq!(searched.total, 1);

        let pk = |id: i64| HashMap::from([("id".to_string(), json!(id))]);
        update_table_cell(&h, DEFAULT_SCHEMA, "stroke_t", pk(2), "name", &json!("bee")).await.unwrap();
        update_table_cell(&h, DEFAULT_SCHEMA, "stroke_t", pk(2), "born", &json!("2020-02-02")).await.unwrap();
        let row = insert_table_row(&h, DEFAULT_SCHEMA, "stroke_t", HashMap::from([("id".to_string(), json!("3")), ("name".to_string(), json!("c"))])).await.unwrap();
        assert_eq!(row[0], json!(3));
        assert_eq!(delete_table_rows(&h, DEFAULT_SCHEMA, "stroke_t", vec![pk(1), pk(3)]).await.unwrap(), 2);
        let r = query(&h, "SELECT name, born FROM stroke_t").await.unwrap();
        assert_eq!(r.rows, vec![vec![json!("bee"), json!("2020-02-02")]]);

        let plan = explain_plan(&h, "SELECT * FROM stroke_t WHERE id > 1").await.unwrap();
        assert!(plan["Node Type"].is_string(), "{plan}");
        assert!(get_ddl(&h, DEFAULT_SCHEMA, "stroke_t").await.unwrap().contains("CREATE TABLE"));
        assert!(query(&h, "SELECT * FROM no_such_table").await.is_err());
        query(&h, "DROP TABLE stroke_t").await.unwrap();

        // The seeded tables: a foreign key and a secondary index.
        if list_tables(&h, DEFAULT_SCHEMA).await.unwrap().iter().any(|t| t.name == "orders") {
            let cols = get_column_structure(&h, DEFAULT_SCHEMA, "orders").await.unwrap();
            assert_eq!(cols.iter().find(|c| c.name == "user_id").unwrap().foreign_key.as_deref(), Some("users.id"));
            let idx = list_indexes(&h, DEFAULT_SCHEMA).await.unwrap();
            assert!(idx.iter().any(|i| i.name == "orders_by_user" && i.columns == "user_id"), "{idx:?}");
            let inc = incoming_foreign_keys(&h, DEFAULT_SCHEMA, "users").await.unwrap();
            assert!(inc.iter().any(|f| f.from_table == "orders" && f.from_columns == ["user_id"] && f.to_columns == ["id"]), "{inc:?}");
            assert!(count_rows(&h, DEFAULT_SCHEMA, "users").await.unwrap() >= 0);
        }
    }

    /// A PostgreSQL-dialect database on the emulator:
    ///   STROKE_TEST_SPANNER_PG=localhost:9020/test/inst/pgshop (with an `items` table)
    #[tokio::test]
    async fn live_emulator_postgres_dialect() {
        let Ok(spec) = std::env::var("STROKE_TEST_SPANNER_PG") else { return };
        let mut parts = spec.splitn(4, '/');
        let (host, project, instance, database) =
            (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap());
        let cfg = SpannerConfig {
            name: "t".into(), project_id: project.into(), instance_id: instance.into(), database_id: database.into(),
            auth: "emulator".into(), key_file: None, emulator_host: Some(host.into()),
        };
        let h = connect(&cfg).await.unwrap();
        assert_eq!(h.dialect, Dialect::Postgres);
        assert!(list_schemas(&h).await.unwrap().contains(&"public".to_string()));
        assert!(list_tables(&h, "public").await.unwrap().iter().any(|t| t.name == "items"));
        query(&h, "DELETE FROM items WHERE true").await.unwrap();
        query(&h, "INSERT INTO items (id, label, price) VALUES (1, 'pen', 2.5), (2, 'cup', 4)").await.unwrap();
        let page = get_table_rows(
            &h, "public", "items", 50, 0, None, Some("price".into()), Some("desc".into()),
            Some(vec![RowFilter { column: "id".into(), op: ">=".into(), value: Some("1".into()), conjunct: None, data_type: None }]),
        ).await.unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.rows[0][1], json!("cup"));
        update_table_cell(&h, "public", "items", HashMap::from([("id".to_string(), json!(1))]), "label", &json!("pencil")).await.unwrap();
        let r = query(&h, "SELECT label FROM items WHERE id = 1").await.unwrap();
        assert_eq!(r.rows[0][0], json!("pencil"));
        // The app's GoogleSQL-style quoting still reads here.
        let r = query(&h, "SELECT `label` FROM `public`.`items` WHERE `id` = 1").await.unwrap();
        assert_eq!(r.rows[0][0], json!("pencil"));
    }
}
