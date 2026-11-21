//! Google BigQuery over its REST API (v2). Beta.
//!
//! Sign-in comes from `google_auth`: a service account key, the gcloud login,
//! or a local emulator (no auth).
//!
//! BigQuery bills by bytes scanned, so the app is careful with what it runs:
//!
//! - Browsing a table with no filter, search or sort reads it through
//!   `tabledata.list`, which is free. A `SELECT * ... LIMIT` would scan (and
//!   bill) every column of the whole table to show fifty rows.
//! - Row counts come from table metadata, also free.
//! - Plans are dry runs: they say how many bytes the query would read
//!   without running it.
//!
//! Every query is a job with an id made here, so Stop can cancel it on the
//! server (`jobs.cancel`) rather than leave it running and billing.
//!
//! Datasets are the schemas. Tables have no row identity in BigQuery, so the
//! grid edits rows only where the table declares a primary key (BigQuery keeps
//! it unenforced, as metadata); without one, edits go through the SQL console.

use super::query::{ColumnInfo, ForeignKeyInfo, RowFilter, SqlResult, TableRows};
use super::schema::{ColumnStructureRow, TableInfo};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BigQueryConfig {
    pub name: String,
    /// The project queries run (and bill) in, and whose datasets are listed.
    pub project_id: String,
    /// The dataset an unqualified table name means in the SQL console.
    #[serde(default)]
    pub dataset: Option<String>,
    /// Where jobs run (`US`, `EU`, `europe-west2`, ...). BigQuery picks it
    /// from the tables when blank.
    #[serde(default)]
    pub location: Option<String>,
    /// "serviceAccount", "gcloud" or "emulator".
    #[serde(default = "default_auth")]
    pub auth: String,
    #[serde(default)]
    pub key_file: Option<String>,
    /// The emulator's REST address, `localhost:9050` when blank.
    #[serde(default)]
    pub emulator_host: Option<String>,
}

fn default_auth() -> String {
    "gcloud".into()
}

pub type BigQueryHandle = Arc<BigQueryConfig>;

/// Rows one console statement returns when it does not stream.
const MAX_ROWS: usize = 100_000;
/// Rows per `getQueryResults` page.
const PAGE_ROWS: usize = 10_000;

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

fn opt(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

fn base_url(cfg: &BigQueryConfig) -> String {
    if cfg.auth == "emulator" {
        let h = opt(&cfg.emulator_host).unwrap_or("localhost:9050").trim_end_matches('/');
        if h.contains("://") { format!("{h}/bigquery/v2") } else { format!("http://{h}/bigquery/v2") }
    } else {
        "https://bigquery.googleapis.com/bigquery/v2".into()
    }
}

fn project(cfg: &BigQueryConfig) -> &str {
    cfg.project_id.trim()
}

/// One REST call. `path` is relative to `/bigquery/v2/projects/{project}`.
async fn call(cfg: &BigQueryConfig, method: reqwest::Method, path: &str, body: Option<&Value>) -> Result<Value, String> {
    let url = format!(
        "{}/projects/{}/{}",
        base_url(cfg),
        urlencoding::encode(project(cfg)),
        path.trim_start_matches('/')
    );
    let token = super::google_auth::token(&cfg.auth, cfg.key_file.as_deref(), super::google_auth::SCOPE_CLOUD_PLATFORM).await?;
    let build = || {
        let mut req = client().request(method.clone(), &url).bearer_auth(&token);
        if let Some(b) = body {
            req = req.json(b);
        }
        req
    };
    // One retry when the request never got an answer (a dropped keep-alive
    // connection). Safe: reads are reads, and a job insert carries its own id,
    // so a second one is refused as a duplicate rather than run twice.
    let sent = match build().send().await {
        Err(e) if e.is_request() || e.is_connect() => {
            tokio::time::sleep(Duration::from_millis(150)).await;
            build().send().await
        }
        other => other,
    };
    let resp = sent.map_err(|e| {
        if e.is_connect() && cfg.auth == "emulator" {
            format!("Could not reach the BigQuery emulator at {}: is it running?", base_url(cfg))
        } else {
            format!("BigQuery request failed: {e}")
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

fn error_text(status: u16, v: &Value, raw: &str) -> String {
    let e = v.get("error").unwrap_or(v);
    let msg = e.get("message").and_then(|m| m.as_str()).unwrap_or("");
    if msg.is_empty() {
        let raw = raw.trim();
        return if raw.is_empty() {
            format!("BigQuery answered HTTP {status}")
        } else {
            format!("BigQuery answered HTTP {status}: {}", raw.chars().take(300).collect::<String>())
        };
    }
    match status {
        401 => format!("BigQuery refused the credentials: {msg}"),
        403 => format!("No permission: {msg}"),
        404 => format!("Not found: {msg}"),
        _ => msg.to_string(),
    }
}

/// Connect: the project answers and lists its datasets.
pub async fn connect(cfg: &BigQueryConfig) -> Result<BigQueryHandle, String> {
    if project(cfg).is_empty() {
        return Err("Enter the Google Cloud project ID".into());
    }
    call(cfg, reqwest::Method::GET, "datasets?maxResults=1", None).await?;
    Ok(Arc::new(cfg.clone()))
}

pub async fn ping(h: &BigQueryConfig) -> Result<(), String> {
    call(h, reqwest::Method::GET, "datasets?maxResults=1", None).await.map(|_| ())
}

// ── Values ────────────────────────────────────────────────────────────────────

/// BigQuery's legacy type names (what the REST API sends) as GoogleSQL's.
fn standard_type(t: &str) -> String {
    match t.to_ascii_uppercase().as_str() {
        "INTEGER" => "INT64".into(),
        "FLOAT" => "FLOAT64".into(),
        "BOOLEAN" => "BOOL".into(),
        "RECORD" => "STRUCT".into(),
        other => other.to_string(),
    }
}

/// A schema field's type as GoogleSQL writes it: `ARRAY<STRING>`,
/// `STRUCT<a INT64, b STRING>`.
fn type_label(f: &Value) -> String {
    let base = standard_type(f.get("type").and_then(|t| t.as_str()).unwrap_or("STRING"));
    let inner = if base == "STRUCT" {
        let parts: Vec<String> = f["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|sub| format!("{} {}", sub["name"].as_str().unwrap_or(""), type_label(sub)))
            .collect();
        format!("STRUCT<{}>", parts.join(", "))
    } else {
        base
    };
    if f.get("mode").and_then(|m| m.as_str()) == Some("REPEATED") {
        format!("ARRAY<{inner}>")
    } else {
        inner
    }
}

/// A TIMESTAMP cell: microseconds since the epoch (`useInt64Timestamp`), or
/// seconds as a float (what an emulator sends), as RFC 3339 in UTC.
fn timestamp(s: &str) -> Option<String> {
    let micros = if s.contains(['.', 'e', 'E']) {
        (s.parse::<f64>().ok()? * 1_000_000.0).round() as i64
    } else {
        s.parse::<i64>().ok()?
    };
    let dt = chrono::DateTime::from_timestamp_micros(micros)?;
    Some(dt.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true))
}

/// One cell (`{"v": ...}`'s value) as the grid shows it.
fn cell(f: &Value, v: Value) -> Value {
    if v.is_null() {
        return v;
    }
    if f.get("mode").and_then(|m| m.as_str()) == Some("REPEATED") {
        let mut one = f.clone();
        if let Some(o) = one.as_object_mut() {
            o.insert("mode".into(), json!("NULLABLE"));
        }
        return match v {
            Value::Array(items) => Value::Array(items.into_iter().map(|x| cell(&one, x.get("v").cloned().unwrap_or(Value::Null))).collect()),
            other => other,
        };
    }
    let ty = standard_type(f.get("type").and_then(|t| t.as_str()).unwrap_or(""));
    match ty.as_str() {
        "STRUCT" => {
            let subs = f["fields"].as_array().cloned().unwrap_or_default();
            let vals = v["f"].as_array().cloned().unwrap_or_default();
            let mut out = serde_json::Map::new();
            for (sub, val) in subs.iter().zip(vals) {
                let name = sub["name"].as_str().unwrap_or("").to_string();
                out.insert(name, cell(sub, val.get("v").cloned().unwrap_or(Value::Null)));
            }
            Value::Object(out)
        }
        "INT64" => match v.as_str().and_then(|s| s.parse::<i64>().ok()) {
            Some(n) if n.unsigned_abs() <= (1u64 << 53) => json!(n),
            _ => v,
        },
        "FLOAT64" => match v.as_str().and_then(|s| s.parse::<f64>().ok()) {
            Some(x) if x.is_finite() => json!(x),
            _ => v,
        },
        "BOOL" => match v.as_str() {
            Some("true") => json!(true),
            Some("false") => json!(false),
            _ => v,
        },
        "TIMESTAMP" => v.as_str().and_then(timestamp).map(Value::String).unwrap_or(v),
        "JSON" => v.as_str().and_then(|s| serde_json::from_str(s).ok()).unwrap_or(v),
        _ => v,
    }
}

fn columns_of(fields: &[Value]) -> Vec<ColumnInfo> {
    fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let name = f["name"].as_str().filter(|n| !n.is_empty()).map(String::from).unwrap_or_else(|| format!("f{i}_"));
            ColumnInfo::new(name, type_label(f))
        })
        .collect()
}

fn row_values(fields: &[Value], row: &Value) -> Vec<Value> {
    row["f"]
        .as_array()
        .map(|cells| {
            cells
                .iter()
                .zip(fields.iter())
                .map(|(c, f)| super::sql_util::cap_json_value("text", cell(f, c.get("v").cloned().unwrap_or(Value::Null))))
                .collect()
        })
        .unwrap_or_default()
}

fn plural(n: i64) -> &'static str {
    if n == 1 { "" } else { "s" }
}

// ── Jobs ──────────────────────────────────────────────────────────────────────

/// A query job this app started: its id and where it runs.
#[derive(Debug, Clone)]
struct Job {
    id: String,
    location: Option<String>,
}

fn fresh_job_id() -> String {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "stroke_{:x}_{:x}_{:x}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

/// One of this app's job ids (`stroke_<pid>_<time>_<seq>`, hex).
fn is_job_id(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("stroke_") else { return false };
    let parts: Vec<&str> = rest.split('_').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Named query parameters: `@p1`, `@p2`, ... with their types.
#[derive(Default, Clone)]
struct Params(Vec<Value>);

impl Params {
    fn bind(&mut self, ty: &str, value: String) -> String {
        let name = format!("p{}", self.0.len() + 1);
        self.0.push(json!({
            "name": name,
            "parameterType": { "type": ty },
            "parameterValue": { "value": value },
        }));
        format!("@{name}")
    }
    fn string(&mut self, s: &str) -> String {
        self.bind("STRING", s.to_string())
    }
}

fn location_query(job: &Job) -> String {
    job.location.as_deref().map(|l| format!("&location={}", urlencoding::encode(l))).unwrap_or_default()
}

/// Start a query job. Returns once BigQuery has accepted it.
async fn start(h: &BigQueryConfig, sql: &str, params: &Params, dry_run: bool) -> Result<(Job, Value), String> {
    let id = fresh_job_id();
    let mut query = json!({ "query": sql, "useLegacySql": false });
    if let Some(ds) = opt(&h.dataset) {
        query["defaultDataset"] = json!({ "projectId": project(h), "datasetId": ds });
    }
    if !params.0.is_empty() {
        query["parameterMode"] = json!("NAMED");
        query["queryParameters"] = Value::Array(params.0.clone());
    }
    let mut job_ref = json!({ "projectId": project(h), "jobId": id });
    if let Some(loc) = opt(&h.location) {
        job_ref["location"] = json!(loc);
    }
    let body = json!({
        "jobReference": job_ref,
        "configuration": { "query": query, "dryRun": dry_run },
    });
    let v = call(h, reqwest::Method::POST, "jobs", Some(&body)).await?;
    if let Some(err) = v["status"]["errorResult"]["message"].as_str() {
        return Err(err.to_string());
    }
    let location = v["jobReference"]["location"].as_str().map(String::from).or_else(|| opt(&h.location).map(String::from));
    Ok((Job { id, location }, v))
}

/// One page of a job's results, waiting (in 10-second polls) until the job
/// is done. A failed job comes back as its error.
async fn page(h: &BigQueryConfig, job: &Job, token: Option<&str>, max: usize) -> Result<Value, String> {
    loop {
        let mut path = format!(
            "queries/{}?maxResults={max}&timeoutMs=10000&formatOptions.useInt64Timestamp=true{}",
            urlencoding::encode(&job.id),
            location_query(job)
        );
        if let Some(t) = token {
            path.push_str(&format!("&pageToken={}", urlencoding::encode(t)));
        }
        let v = call(h, reqwest::Method::GET, &path, None).await?;
        if v["jobComplete"].as_bool() == Some(false) {
            continue;
        }
        if let Some(err) = v["errors"].as_array().and_then(|e| e.first()).and_then(|e| e["message"].as_str()) {
            return Err(err.to_string());
        }
        return Ok(v);
    }
}

async fn cancel(h: &BigQueryConfig, job: &Job) {
    let path = format!("jobs/{}/cancel?{}", urlencoding::encode(&job.id), location_query(job).trim_start_matches('&'));
    let _ = tokio::time::timeout(Duration::from_secs(10), call(h, reqwest::Method::POST, &path, None)).await;
}

/// The message for a statement that returns no rows: rows changed, or done.
fn done_message(v: &Value) -> (Option<i64>, String) {
    match v["numDmlAffectedRows"].as_str().and_then(|s| s.parse::<i64>().ok()) {
        Some(n) => (Some(n), format!("{n} row{} affected", plural(n))),
        None => (Some(0), "Done".into()),
    }
}

/// Run a job to the end and collect up to `cap` rows.
async fn collect(h: &BigQueryConfig, sql: &str, params: &Params, cap: usize) -> Result<(Vec<Value>, Vec<Vec<Value>>, Value, Job), String> {
    let (job, _) = start(h, sql, params, false).await?;
    let first = page(h, &job, None, PAGE_ROWS.min(cap.max(1))).await?;
    let fields = first["schema"]["fields"].as_array().cloned().unwrap_or_default();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut v = first.clone();
    loop {
        for r in v["rows"].as_array().into_iter().flatten() {
            if rows.len() >= cap {
                break;
            }
            rows.push(row_values(&fields, r));
        }
        let next = v["pageToken"].as_str().map(String::from);
        match next {
            Some(t) if rows.len() < cap => v = page(h, &job, Some(&t), PAGE_ROWS.min(cap - rows.len())).await?,
            _ => break,
        }
    }
    Ok((fields, rows, first, job))
}

fn result_of(fields: &[Value], rows: Vec<Vec<Value>>, first: &Value, sql: &str, t0: Instant) -> SqlResult {
    let columns = columns_of(fields);
    if columns.is_empty() {
        let (n, msg) = done_message(first);
        return SqlResult { columns, rows: vec![], row_count: n, message: Some(msg), query_ms: t0.elapsed().as_millis() as u64, sql: sql.to_string() };
    }
    let total = first["totalRows"].as_str().and_then(|s| s.parse::<i64>().ok()).unwrap_or(rows.len() as i64);
    let message = (total > rows.len() as i64).then(|| format!("Showing the first {} of {total} rows", rows.len()));
    SqlResult { columns, row_count: Some(rows.len() as i64), rows, message, query_ms: t0.elapsed().as_millis() as u64, sql: sql.to_string() }
}

/// Does this statement only read? (SELECT, WITH, or a parenthesised query.)
pub fn reads(stmt: &str) -> bool {
    let head = super::sql_util::statement_head(stmt);
    matches!(head.as_str(), "select" | "with") || stmt.trim_start().starts_with('(')
}

/// One console statement.
pub async fn query(h: &BigQueryConfig, stmt: &str) -> Result<SqlResult, String> {
    let t0 = Instant::now();
    let sql = stmt.trim().trim_end_matches(';').trim();
    if sql.is_empty() {
        return Err("Query is empty".into());
    }
    let (fields, rows, first, _) = collect(h, sql, &Params::default(), MAX_ROWS).await?;
    Ok(result_of(&fields, rows, &first, sql, t0))
}

/// One console statement that Stop cancels on the server.
pub async fn query_cancellable(h: &BigQueryConfig, stmt: &str, stop: &super::stop::Stop) -> Result<SqlResult, String> {
    let t0 = Instant::now();
    let sql = stmt.trim().trim_end_matches(';').trim();
    if sql.is_empty() {
        return Err("Query is empty".into());
    }
    let (job, _) = start(h, sql, &Params::default(), false).await?;
    let run = async {
        let first = page(h, &job, None, PAGE_ROWS).await?;
        let fields = first["schema"]["fields"].as_array().cloned().unwrap_or_default();
        let mut rows = Vec::new();
        let mut v = first.clone();
        loop {
            for r in v["rows"].as_array().into_iter().flatten() {
                if rows.len() < MAX_ROWS {
                    rows.push(row_values(&fields, r));
                }
            }
            match v["pageToken"].as_str().map(String::from) {
                Some(t) if rows.len() < MAX_ROWS => v = page(h, &job, Some(&t), PAGE_ROWS).await?,
                _ => break,
            }
        }
        Ok::<_, String>(result_of(&fields, rows, &first, sql, t0))
    };
    tokio::pin!(run);
    tokio::select! {
        r = &mut run => r,
        _ = stop.wait() => {
            cancel(h, &job).await;
            Err(super::query::QUERY_CANCELLED.into())
        }
    }
}

/// Stream a statement's rows into the console's sink, page by page, with
/// Stop cancelling the job. No row cap: the sink spools to disk.
pub async fn stream_into(h: &BigQueryConfig, stmt: &str, sink: &mut super::query::RowSink, stop: &super::stop::Stop) -> Result<SqlResult, String> {
    let t0 = Instant::now();
    let sql = stmt.trim().trim_end_matches(';').trim();
    if sql.is_empty() {
        return Err("Query is empty".into());
    }
    let (job, _) = start(h, sql, &Params::default(), false).await?;
    let run = async {
        let first = page(h, &job, None, PAGE_ROWS).await?;
        let fields = first["schema"]["fields"].as_array().cloned().unwrap_or_default();
        if fields.is_empty() {
            return Ok(result_of(&fields, vec![], &first, sql, t0));
        }
        let columns = columns_of(&fields);
        sink.start(columns.clone())?;
        let mut n = 0usize;
        let mut v = first;
        loop {
            for r in v["rows"].as_array().into_iter().flatten() {
                sink.push(row_values(&fields, r))?;
                n += 1;
            }
            match v["pageToken"].as_str().map(String::from) {
                Some(t) => v = page(h, &job, Some(&t), PAGE_ROWS).await?,
                None => break,
            }
        }
        sink.finish()?;
        Ok::<_, String>(SqlResult { columns, rows: vec![], row_count: Some(n as i64), message: None, query_ms: t0.elapsed().as_millis() as u64, sql: sql.to_string() })
    };
    tokio::pin!(run);
    tokio::select! {
        r = &mut run => r,
        _ = stop.wait() => {
            cancel(h, &job).await;
            Err(super::query::QUERY_CANCELLED.into())
        }
    }
}

// ── Plans ─────────────────────────────────────────────────────────────────────

fn bytes_label(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut x = n as f64;
    let mut u = 0;
    while x >= 1024.0 && u < UNITS.len() - 1 {
        x /= 1024.0;
        u += 1;
    }
    if u == 0 { format!("{n} B") } else { format!("{x:.1} {}", UNITS[u]) }
}

/// A dry run: what the query would read, without running it, as the plan
/// view's tree (the statement, then each table it reads).
pub async fn explain_plan(h: &BigQueryConfig, sql: &str) -> Result<Value, String> {
    let sql = sql.trim().trim_end_matches(';').trim();
    let sql = sql.strip_prefix("EXPLAIN ").or_else(|| sql.strip_prefix("explain ")).unwrap_or(sql);
    // An emulator ignores dryRun and would run the statement: only reads there.
    if h.auth == "emulator" && !reads(sql) {
        return Err("The emulator can't dry-run a statement that writes".into());
    }
    let (_, v) = start(h, sql, &Params::default(), true).await?;
    let stats = &v["statistics"];
    let bytes = stats["totalBytesProcessed"]
        .as_str()
        .or_else(|| stats["query"]["totalBytesProcessed"].as_str())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let kind = stats["query"]["statementType"].as_str().unwrap_or("QUERY");
    let tables: Vec<Value> = stats["query"]["referencedTables"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| {
            json!({
                "Node Type": "Table read",
                "Relation Name": format!("{}.{}", t["datasetId"].as_str().unwrap_or(""), t["tableId"].as_str().unwrap_or("")),
            })
        })
        .collect();
    let mut root = json!({
        "Node Type": format!("{kind} (dry run)"),
        "Bytes processed": bytes_label(bytes),
        "Detail": format!("BigQuery would read {} running this", bytes_label(bytes)),
    });
    if !tables.is_empty() {
        root["Plans"] = Value::Array(tables);
    }
    Ok(root)
}

// ── Catalog ───────────────────────────────────────────────────────────────────

/// Every page of a list call, items under `key`.
async fn list_all(h: &BigQueryConfig, path: &str, key: &str) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    let mut token: Option<String> = None;
    loop {
        let sep = if path.contains('?') { '&' } else { '?' };
        let mut p = format!("{path}{sep}maxResults=1000");
        if let Some(t) = &token {
            p.push_str(&format!("&pageToken={}", urlencoding::encode(t)));
        }
        let v = call(h, reqwest::Method::GET, &p, None).await?;
        out.extend(v[key].as_array().cloned().unwrap_or_default());
        match v["nextPageToken"].as_str() {
            Some(t) if !t.is_empty() && out.len() < 50_000 => token = Some(t.to_string()),
            _ => break,
        }
    }
    Ok(out)
}

pub async fn list_schemas(h: &BigQueryConfig) -> Result<Vec<String>, String> {
    let mut names: Vec<String> = list_all(h, "datasets", "datasets")
        .await?
        .iter()
        .filter_map(|d| d["datasetReference"]["datasetId"].as_str().map(String::from))
        // Query results live in hidden `_...` datasets; an emulator instead
        // makes one per job, named after the job id this app gave it.
        .filter(|name| !name.starts_with('_') && !(h.auth == "emulator" && is_job_id(name)))
        .collect();
    names.sort();
    Ok(names)
}

pub async fn list_tables(h: &BigQueryConfig, dataset: &str) -> Result<Vec<TableInfo>, String> {
    let path = format!("datasets/{}/tables", urlencoding::encode(dataset));
    let mut out: Vec<TableInfo> = list_all(h, &path, "tables")
        .await?
        .iter()
        .filter_map(|t| {
            let name = t["tableReference"]["tableId"].as_str()?.to_string();
            let ty = t["type"].as_str().unwrap_or("TABLE");
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
    Ok(out)
}

/// One table's metadata: schema, constraints, size, view text.
async fn table_meta(h: &BigQueryConfig, dataset: &str, table: &str) -> Result<Value, String> {
    let path = format!("datasets/{}/tables/{}", urlencoding::encode(dataset), urlencoding::encode(table));
    call(h, reqwest::Method::GET, &path, None).await
}

fn fields_of(meta: &Value) -> Vec<Value> {
    meta["schema"]["fields"].as_array().cloned().unwrap_or_default()
}

fn primary_key_of(meta: &Value) -> Vec<String> {
    meta["tableConstraints"]["primaryKey"]["columns"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c.as_str().map(String::from))
        .collect()
}

/// (column, referenced table, referenced column, constraint) per FK column.
fn foreign_keys_of(meta: &Value) -> Vec<(String, String, String, String)> {
    let mut out = Vec::new();
    for fk in meta["tableConstraints"]["foreignKeys"].as_array().into_iter().flatten() {
        let name = fk["name"].as_str().unwrap_or("").to_string();
        let target = fk["referencedTable"]["tableId"].as_str().unwrap_or("").to_string();
        for c in fk["columnReferences"].as_array().into_iter().flatten() {
            out.push((
                c["referencingColumn"].as_str().unwrap_or("").to_string(),
                target.clone(),
                c["referencedColumn"].as_str().unwrap_or("").to_string(),
                name.clone(),
            ));
        }
    }
    out
}

fn structure_of(meta: &Value) -> Vec<ColumnStructureRow> {
    let pk = primary_key_of(meta);
    let fks = foreign_keys_of(meta);
    fields_of(meta)
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let name = f["name"].as_str().unwrap_or("").to_string();
            let fk = fks.iter().find(|k| k.0 == name);
            ColumnStructureRow {
                ordinal_position: (i + 1) as i32,
                data_type: type_label(f),
                is_nullable: f["mode"].as_str() != Some("REQUIRED"),
                column_default: f["defaultValueExpression"].as_str().map(String::from),
                foreign_key: fk.map(|k| format!("{}.{}", k.1, k.2)),
                fk_constraint_name: fk.map(|k| k.3.clone()),
                comment: f["description"].as_str().filter(|d| !d.is_empty()).map(String::from),
                is_primary_key: pk.contains(&name),
                name,
            }
        })
        .collect()
}

pub async fn get_column_structure(h: &BigQueryConfig, dataset: &str, table: &str) -> Result<Vec<ColumnStructureRow>, String> {
    Ok(structure_of(&table_meta(h, dataset, table).await?))
}

/// Every table's metadata in a dataset, eight requests at a time.
async fn all_meta(h: &BigQueryConfig, dataset: &str) -> Result<Vec<(String, Value)>, String> {
    use futures::stream::StreamExt;
    let tables = list_tables(h, dataset).await?;
    let metas: Vec<(String, Result<Value, String>)> = futures::stream::iter(tables.into_iter().map(|t| async move {
        let m = table_meta(h, dataset, &t.name).await;
        (t.name, m)
    }))
    .buffered(8)
    .collect()
    .await;
    Ok(metas.into_iter().filter_map(|(n, m)| m.ok().map(|m| (n, m))).collect())
}

pub async fn schema_column_structure(h: &BigQueryConfig, dataset: &str) -> Result<Vec<super::schema::TableColumnStructure>, String> {
    Ok(all_meta(h, dataset)
        .await?
        .into_iter()
        .map(|(table, meta)| super::schema::TableColumnStructure { table, columns: structure_of(&meta) })
        .collect())
}

/// Foreign keys in this dataset that point at `table`.
pub async fn incoming_foreign_keys(h: &BigQueryConfig, dataset: &str, table: &str) -> Result<Vec<super::schema::IncomingForeignKey>, String> {
    let mut out: Vec<super::schema::IncomingForeignKey> = Vec::new();
    for (from_table, meta) in all_meta(h, dataset).await? {
        for (col, target, ref_col, name) in foreign_keys_of(&meta) {
            if target != table {
                continue;
            }
            match out.iter_mut().find(|f| f.from_table == from_table && f.constraint_name == name) {
                Some(f) => {
                    f.from_columns.push(col);
                    f.to_columns.push(ref_col);
                }
                None => out.push(super::schema::IncomingForeignKey {
                    from_schema: dataset.to_string(),
                    from_table: from_table.clone(),
                    from_columns: vec![col],
                    to_columns: vec![ref_col],
                    constraint_name: name,
                }),
            }
        }
    }
    Ok(out)
}

/// Rows in a table, from its metadata (free). Views have no stored count.
pub async fn count_rows(h: &BigQueryConfig, dataset: &str, table: &str) -> Result<i64, String> {
    let meta = table_meta(h, dataset, table).await?;
    Ok(meta["numRows"].as_str().and_then(|s| s.parse::<i64>().ok()).unwrap_or(-1))
}

fn table_ref(h: &BigQueryConfig, dataset: &str, table: &str) -> String {
    use super::sql_util::quote_backtick;
    format!("{}.{}.{}", quote_backtick(project(h)), quote_backtick(dataset), quote_backtick(table))
}

/// The table's DDL as BigQuery writes it (`INFORMATION_SCHEMA.TABLES.ddl`),
/// or built from its schema where that column isn't there (an emulator).
pub async fn get_ddl(h: &BigQueryConfig, dataset: &str, table: &str) -> Result<String, String> {
    use super::sql_util::quote_backtick;
    let mut p = Params::default();
    let t = p.string(table);
    let sql = format!(
        "SELECT ddl FROM {}.{}.INFORMATION_SCHEMA.TABLES WHERE table_name = {t}",
        quote_backtick(project(h)),
        quote_backtick(dataset)
    );
    if let Ok((_, rows, _, _)) = collect(h, &sql, &p, 1).await {
        if let Some(Value::String(ddl)) = rows.first().and_then(|r| r.first()) {
            return Ok(ddl.clone());
        }
    }
    let meta = table_meta(h, dataset, table).await?;
    let name = table_ref(h, dataset, table);
    if let Some(q) = meta["view"]["query"].as_str() {
        return Ok(format!("CREATE VIEW {name} AS\n{q};"));
    }
    let mut lines: Vec<String> = fields_of(&meta)
        .iter()
        .map(|f| {
            let mut l = format!("  {} {}", quote_backtick(f["name"].as_str().unwrap_or("")), type_label(f));
            if f["mode"].as_str() == Some("REQUIRED") {
                l.push_str(" NOT NULL");
            }
            if let Some(d) = f["defaultValueExpression"].as_str() {
                l.push_str(&format!(" DEFAULT {d}"));
            }
            l
        })
        .collect();
    let pk = primary_key_of(&meta);
    if !pk.is_empty() {
        let cols: Vec<String> = pk.iter().map(|c| quote_backtick(c)).collect();
        lines.push(format!("  PRIMARY KEY ({}) NOT ENFORCED", cols.join(", ")));
    }
    Ok(format!("CREATE TABLE {name} (\n{}\n);", lines.join(",\n")))
}

// ── The grid ──────────────────────────────────────────────────────────────────

fn base_type(ty: &str) -> String {
    let t = ty.trim().to_ascii_uppercase();
    standard_type(t.split(['(', '<']).next().unwrap_or(&t).trim())
}

/// A cell's value as a typed query parameter's text.
fn param_text(ty: &str, v: &Value) -> Result<(String, String), String> {
    let code = base_type(ty);
    if code == "ARRAY" || code == "STRUCT" || code == "RANGE" {
        return Err(format!("{ty} values can't be edited from the grid yet; use an UPDATE in the SQL console"));
    }
    let text = match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => return Err("null".into()),
        other => other.to_string(),
    };
    let text = if code == "BOOL" {
        match text.trim().to_ascii_lowercase().as_str() {
            "true" | "t" | "1" | "yes" => "true".into(),
            "false" | "f" | "0" | "no" => "false".into(),
            _ => return Err(format!("{text:?} is not a BOOL")),
        }
    } else {
        text
    };
    Ok((code, text))
}

/// `col = @p` for a value, `col IS NULL` for null.
fn eq(col: &str, ty: &str, v: &Value, p: &mut Params) -> Result<String, String> {
    if v.is_null() {
        return Ok(format!("{col} IS NULL"));
    }
    let (code, text) = param_text(ty, v)?;
    Ok(format!("{col} = {}", p.bind(&code, text)))
}

fn as_text(col: &str, ty: &str) -> String {
    match base_type(ty).as_str() {
        "STRING" => col.to_string(),
        "ARRAY" | "STRUCT" | "JSON" | "BYTES" | "RANGE" => format!("TO_JSON_STRING({col})"),
        _ => format!("CAST({col} AS STRING)"),
    }
}

fn build_where(cols: &[ColumnStructureRow], search: Option<&str>, filters: Option<&[RowFilter]>, p: &mut Params) -> String {
    use super::sql_util::quote_backtick;
    let by_name: HashMap<&str, &ColumnStructureRow> = cols.iter().map(|c| (c.name.as_str(), c)).collect();
    let mut clauses: Vec<String> = Vec::new();
    if let Some(q) = search.map(str::trim).filter(|s| !s.is_empty()) {
        let needle = p.string(&format!("%{}%", q.to_lowercase()));
        let ors: Vec<String> =
            cols.iter().map(|c| format!("LOWER({}) LIKE {needle}", as_text(&quote_backtick(&c.name), &c.data_type))).collect();
        if !ors.is_empty() {
            clauses.push(format!("({})", ors.join(" OR ")));
        }
    }
    for f in filters.unwrap_or(&[]) {
        let Some(c) = by_name.get(f.column.as_str()) else { continue };
        let col = quote_backtick(&c.name);
        let v = f.value.as_deref().unwrap_or("");
        let typed = |p: &mut Params, raw: &str| -> Option<String> { param_text(&c.data_type, &json!(raw)).ok().map(|(code, t)| p.bind(&code, t)) };
        let text = as_text(&col, &c.data_type);
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

fn foreign_key_infos(dataset: &str, cols: &[ColumnStructureRow]) -> Vec<ForeignKeyInfo> {
    cols.iter()
        .filter_map(|c| {
            let (t, col) = c.foreign_key.as_ref()?.split_once('.')?;
            Some(ForeignKeyInfo {
                columns: vec![c.name.clone()],
                referenced_schema: dataset.to_string(),
                referenced_table: t.to_string(),
                referenced_columns: vec![col.to_string()],
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub async fn get_table_rows(
    h: &BigQueryConfig,
    dataset: &str,
    table: &str,
    limit: i64,
    offset: i64,
    search: Option<String>,
    sort_column: Option<String>,
    sort_direction: Option<String>,
    filters: Option<Vec<RowFilter>>,
) -> Result<TableRows, String> {
    let t0 = Instant::now();
    let meta = table_meta(h, dataset, table).await?;
    let fields = fields_of(&meta);
    let cols = structure_of(&meta);
    if cols.is_empty() {
        return Err(format!("No table named {table}"));
    }
    let (limit, offset) = (limit.max(0), offset.max(0));
    let is_table = meta["type"].as_str().is_none_or(|t| t == "TABLE");
    let plain = search.as_deref().is_none_or(|s| s.trim().is_empty())
        && filters.as_deref().is_none_or(|f| f.is_empty())
        && sort_column.as_deref().is_none_or(|s| s.is_empty());
    let pk: Vec<String> = cols.iter().filter(|c| c.is_primary_key).map(|c| c.name.clone()).collect();

    // A plain page of a table: tabledata.list reads it without a query.
    if plain && is_table {
        let path = format!(
            "datasets/{}/tables/{}/data?maxResults={limit}&startIndex={offset}&formatOptions.useInt64Timestamp=true",
            urlencoding::encode(dataset),
            urlencoding::encode(table)
        );
        let v = call(h, reqwest::Method::GET, &path, None).await?;
        let all: Vec<&Value> = v["rows"].as_array().map(|r| r.iter().collect()).unwrap_or_default();
        // A server that ignored the paging (an emulator) sent every row.
        let page: Vec<&Value> =
            if all.len() > limit as usize { all.into_iter().skip(offset as usize).take(limit as usize).collect() } else { all };
        let rows = page.into_iter().map(|r| row_values(&fields, r)).collect();
        let total = v["totalRows"]
            .as_str()
            .or_else(|| meta["numRows"].as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(-1);
        return Ok(TableRows {
            preview_columns: Vec::new(),
            columns: columns_of(&fields),
            rows,
            total,
            query_ms: t0.elapsed().as_millis() as u64,
            primary_key: pk,
            foreign_keys: foreign_key_infos(dataset, &cols),
            sql: format!("-- read with tabledata.list (no query, nothing billed)\nSELECT * FROM {} LIMIT {limit} OFFSET {offset}", table_ref(h, dataset, table)),
        });
    }

    let tref = table_ref(h, dataset, table);
    let mut p = Params::default();
    let where_clause = build_where(&cols, search.as_deref(), filters.as_deref(), &mut p);
    let order = match sort_column.as_deref().filter(|c| cols.iter().any(|x| x.name == *c)) {
        Some(c) => {
            let d = if sort_direction.as_deref().is_some_and(|d| d.eq_ignore_ascii_case("desc")) { "DESC" } else { "ASC" };
            format!(" ORDER BY {} {d}", super::sql_util::quote_backtick(c))
        }
        None => String::new(),
    };
    let data_sql = format!("SELECT * FROM {tref}{where_clause}{order} LIMIT {limit} OFFSET {offset}");
    let count_sql = format!("SELECT COUNT(*) AS n FROM {tref}{where_clause}");
    let (data, count) = tokio::join!(collect(h, &data_sql, &p, limit as usize), collect(h, &count_sql, &p, 1));
    let (data_fields, rows, _, _) = data?;
    let total = count
        .ok()
        .and_then(|(_, r, _, _)| r.first().and_then(|row| row.first()).and_then(|v| v.as_i64()))
        .unwrap_or(rows.len() as i64);
    Ok(TableRows {
        preview_columns: Vec::new(),
        columns: columns_of(&data_fields),
        rows,
        total,
        query_ms: t0.elapsed().as_millis() as u64,
        primary_key: pk,
        foreign_keys: foreign_key_infos(dataset, &cols),
        sql: format!("{data_sql}\n{count_sql}"),
    })
}

fn no_key_error() -> String {
    "BigQuery rows have no identity, and this table declares no primary key, so the grid can't tell which row to change. \
     Use an UPDATE or DELETE in the SQL console, or add PRIMARY KEY (...) NOT ENFORCED to the table."
        .into()
}

fn pk_where(cols: &[ColumnStructureRow], pk: &HashMap<String, Value>, p: &mut Params) -> Result<String, String> {
    let keys: Vec<&ColumnStructureRow> = cols.iter().filter(|c| c.is_primary_key).collect();
    if keys.is_empty() {
        return Err(no_key_error());
    }
    let mut parts = Vec::new();
    for c in keys {
        let v = pk.get(&c.name).ok_or_else(|| format!("The row's {} is missing", c.name))?;
        parts.push(eq(&super::sql_util::quote_backtick(&c.name), &c.data_type, v, p)?);
    }
    Ok(parts.join(" AND "))
}

async fn dml(h: &BigQueryConfig, sql: &str, p: &Params) -> Result<Option<i64>, String> {
    let (_, _, first, _) = collect(h, sql, p, 0).await?;
    Ok(first["numDmlAffectedRows"].as_str().and_then(|s| s.parse::<i64>().ok()))
}

pub async fn update_table_cell(
    h: &BigQueryConfig,
    dataset: &str,
    table: &str,
    primary_key: HashMap<String, Value>,
    column: &str,
    value: &Value,
) -> Result<(), String> {
    let cols = get_column_structure(h, dataset, table).await?;
    let c = cols.iter().find(|c| c.name == column).ok_or_else(|| format!("No column named {column}"))?;
    let mut p = Params::default();
    let target = super::sql_util::quote_backtick(column);
    let set = if value.is_null() {
        format!("{target} = NULL")
    } else {
        let (code, text) = param_text(&c.data_type, value)?;
        format!("{target} = {}", p.bind(&code, text))
    };
    let w = pk_where(&cols, &primary_key, &mut p)?;
    let sql = format!("UPDATE {} SET {set} WHERE {w}", table_ref(h, dataset, table));
    match dml(h, &sql, &p).await? {
        Some(1) | None => Ok(()),
        Some(n) => Err(super::query::row_count_error(&primary_key.keys().cloned().collect::<Vec<_>>(), &primary_key, n as u64)),
    }
}

pub async fn insert_table_row(h: &BigQueryConfig, dataset: &str, table: &str, values: HashMap<String, Value>) -> Result<Vec<Value>, String> {
    let cols = get_column_structure(h, dataset, table).await?;
    let mut p = Params::default();
    let mut names = Vec::new();
    let mut holes = Vec::new();
    for c in &cols {
        let Some(v) = values.get(&c.name) else { continue };
        names.push(super::sql_util::quote_backtick(&c.name));
        holes.push(if v.is_null() {
            "NULL".to_string()
        } else {
            let (code, text) = param_text(&c.data_type, v)?;
            p.bind(&code, text)
        });
    }
    if names.is_empty() {
        return Err("Fill in at least one column".into());
    }
    let sql = format!("INSERT INTO {} ({}) VALUES ({})", table_ref(h, dataset, table), names.join(", "), holes.join(", "));
    dml(h, &sql, &p).await?;
    Ok(cols.iter().map(|c| values.get(&c.name).cloned().unwrap_or(Value::Null)).collect())
}

pub async fn delete_table_rows(h: &BigQueryConfig, dataset: &str, table: &str, primary_keys: Vec<HashMap<String, Value>>) -> Result<u64, String> {
    if primary_keys.is_empty() {
        return Ok(0);
    }
    let cols = get_column_structure(h, dataset, table).await?;
    let mut p = Params::default();
    let mut ors = Vec::new();
    for pk in &primary_keys {
        ors.push(format!("({})", pk_where(&cols, pk, &mut p)?));
    }
    let sql = format!("DELETE FROM {} WHERE {}", table_ref(h, dataset, table), ors.join(" OR "));
    Ok(dml(h, &sql, &p).await?.map(|n| n.max(0) as u64).unwrap_or(primary_keys.len() as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, ty: &str, mode: &str) -> Value {
        json!({ "name": name, "type": ty, "mode": mode })
    }

    #[test]
    fn types_read_as_googlesql() {
        assert_eq!(type_label(&field("a", "INTEGER", "NULLABLE")), "INT64");
        assert_eq!(type_label(&field("a", "STRING", "REPEATED")), "ARRAY<STRING>");
        let rec = json!({ "name": "m", "type": "RECORD", "mode": "NULLABLE",
            "fields": [field("a", "INTEGER", "NULLABLE"), field("b", "STRING", "NULLABLE")] });
        assert_eq!(type_label(&rec), "STRUCT<a INT64, b STRING>");
    }

    #[test]
    fn cells_come_back_as_the_grid_shows_them() {
        assert_eq!(cell(&field("a", "INTEGER", "NULLABLE"), json!("42")), json!(42));
        assert_eq!(cell(&field("a", "INTEGER", "NULLABLE"), json!("9007199254740993")), json!("9007199254740993"));
        assert_eq!(cell(&field("a", "FLOAT", "NULLABLE"), json!("9.5")), json!(9.5));
        assert_eq!(cell(&field("a", "BOOLEAN", "NULLABLE"), json!("false")), json!(false));
        assert_eq!(cell(&field("a", "TIMESTAMP", "NULLABLE"), json!("1795237200.000000")), json!("2026-11-21T05:00:00Z"));
        assert_eq!(cell(&field("a", "TIMESTAMP", "NULLABLE"), json!("1795237200123456")), json!("2026-11-21T05:00:00.123456Z"));
        assert_eq!(cell(&field("a", "STRING", "REPEATED"), json!([{ "v": "x" }, { "v": "y" }])), json!(["x", "y"]));
        let rec = json!({ "name": "m", "type": "RECORD", "mode": "NULLABLE",
            "fields": [field("a", "INTEGER", "NULLABLE"), field("b", "STRING", "NULLABLE")] });
        assert_eq!(cell(&rec, json!({ "f": [{ "v": "1" }, { "v": "x" }] })), json!({ "a": 1, "b": "x" }));
        assert_eq!(cell(&field("a", "JSON", "NULLABLE"), json!("{\"k\":1}")), json!({ "k": 1 }));
        assert_eq!(cell(&field("a", "STRING", "NULLABLE"), Value::Null), Value::Null);
    }

    #[test]
    fn grid_values_bind_as_typed_parameters() {
        let mut p = Params::default();
        assert_eq!(eq("`id`", "INT64", &json!(7), &mut p).unwrap(), "`id` = @p1");
        assert_eq!(eq("`ok`", "BOOL", &json!("yes"), &mut p).unwrap(), "`ok` = @p2");
        assert_eq!(eq("`x`", "STRING", &Value::Null, &mut p).unwrap(), "`x` IS NULL");
        assert_eq!(p.0[0]["parameterType"]["type"], "INT64");
        assert_eq!(p.0[0]["parameterValue"]["value"], "7");
        assert_eq!(p.0[1]["parameterValue"]["value"], "true");
        assert!(param_text("ARRAY<STRING>", &json!("x")).is_err());
    }

    #[test]
    fn only_reads_count_as_reads() {
        assert!(reads("SELECT 1"));
        assert!(reads("  with t as (select 1) select * from t"));
        assert!(!reads("DELETE FROM t WHERE true"));
        assert!(!reads("CREATE TABLE t (a INT64)"));
    }

    #[test]
    fn job_ids_are_recognised() {
        assert!(is_job_id(&fresh_job_id()));
        assert!(!is_job_id("stroke_sales"));
        assert!(!is_job_id("analytics"));
    }

    #[test]
    fn byte_counts_read_well() {
        assert_eq!(bytes_label(10), "10 B");
        assert_eq!(bytes_label(1536), "1.5 KB");
        assert_eq!(bytes_label(5 * 1024 * 1024 * 1024), "5.0 GB");
    }

    /// Against the BigQuery emulator, when STROKE_TEST_BIGQUERY is
    /// `host:port/project/dataset`, e.g.
    /// `docker run -p 9050:9050 ghcr.io/goccy/bigquery-emulator --project=test --dataset=shop`
    /// then `STROKE_TEST_BIGQUERY=localhost:9050/test/shop`.
    #[tokio::test]
    async fn live_emulator_round_trip() {
        let Ok(spec) = std::env::var("STROKE_TEST_BIGQUERY") else { return };
        let mut parts = spec.splitn(3, '/');
        let (host, project, dataset) = (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap());
        let cfg = BigQueryConfig {
            name: "t".into(),
            project_id: project.into(),
            dataset: Some(dataset.into()),
            location: None,
            auth: "emulator".into(),
            key_file: None,
            emulator_host: Some(host.into()),
        };
        let h = connect(&cfg).await.unwrap();
        assert!(list_schemas(&h).await.unwrap().contains(&dataset.to_string()));

        // A table with a primary key, made over REST (constraints included).
        let tid = format!("stroke_t_{}", std::process::id());
        let body = json!({
            "tableReference": { "projectId": project, "datasetId": dataset, "tableId": tid },
            "schema": { "fields": [
                { "name": "id", "type": "INTEGER", "mode": "REQUIRED" },
                { "name": "label", "type": "STRING" },
                { "name": "price", "type": "FLOAT" },
                { "name": "seen", "type": "TIMESTAMP" },
                { "name": "tags", "type": "STRING", "mode": "REPEATED" },
            ] },
            "tableConstraints": { "primaryKey": { "columns": ["id"] } },
        });
        call(&h, reqwest::Method::POST, &format!("datasets/{dataset}/tables"), Some(&body)).await.unwrap();
        query(&h, &format!("INSERT INTO {tid} (id, label, price, seen, tags) VALUES (1, 'pen', 2.5, TIMESTAMP '2026-11-21 05:00:00 UTC', ['a']), (2, 'cup', 4.0, NULL, [])"))
            .await
            .unwrap();

        let tables = list_tables(&h, dataset).await.unwrap();
        assert!(tables.iter().any(|t| t.name == tid && t.kind == "table"));
        let cols = get_column_structure(&h, dataset, &tid).await.unwrap();
        assert_eq!(cols[0].data_type, "INT64");
        assert!(cols[0].is_primary_key && !cols[0].is_nullable);
        assert_eq!(cols[4].data_type, "ARRAY<STRING>");

        // Plain page: tabledata.list, no query.
        let page = get_table_rows(&h, dataset, &tid, 1, 1, None, None, None, None).await.unwrap();
        assert_eq!(page.rows.len(), 1);
        assert!(page.sql.contains("tabledata.list"));
        assert_eq!(page.primary_key, vec!["id".to_string()]);

        // Filtered and sorted: a query with parameters.
        let page = get_table_rows(
            &h, dataset, &tid, 50, 0, None, Some("price".into()), Some("desc".into()),
            Some(vec![RowFilter { column: "id".into(), op: ">=".into(), value: Some("1".into()), conjunct: None, data_type: None }]),
        )
        .await
        .unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.rows[0][1], json!("cup"));
        assert_eq!(page.rows[1][3], json!("2026-11-21T05:00:00Z"));
        assert_eq!(page.rows[1][4], json!(["a"]));
        let page = get_table_rows(&h, dataset, &tid, 50, 0, Some("PEN".into()), None, None, None).await.unwrap();
        assert_eq!(page.rows.len(), 1);

        // Edits by primary key.
        update_table_cell(&h, dataset, &tid, HashMap::from([("id".to_string(), json!(1))]), "label", &json!("pencil")).await.unwrap();
        update_table_cell(&h, dataset, &tid, HashMap::from([("id".to_string(), json!(2))]), "price", &Value::Null).await.unwrap();
        insert_table_row(&h, dataset, &tid, HashMap::from([("id".to_string(), json!(3)), ("label".to_string(), json!("ink"))])).await.unwrap();
        let r = query(&h, &format!("SELECT id, label, price FROM {tid} ORDER BY id")).await.unwrap();
        assert_eq!(r.rows.len(), 3);
        assert_eq!(r.rows[0][1], json!("pencil"));
        assert_eq!(r.rows[1][2], Value::Null);
        delete_table_rows(&h, dataset, &tid, vec![HashMap::from([("id".to_string(), json!(3))])]).await.unwrap();
        let r = query(&h, &format!("SELECT COUNT(*) FROM {tid}")).await.unwrap();
        assert_eq!(r.rows[0][0], json!(2));

        assert!(get_ddl(&h, dataset, &tid).await.unwrap().contains("PRIMARY KEY (`id`) NOT ENFORCED"));
        let plan = explain_plan(&h, &format!("SELECT label FROM {tid}")).await.unwrap();
        assert!(plan["Node Type"].as_str().unwrap().contains("dry run"), "{plan}");
        assert!(explain_plan(&h, &format!("DELETE FROM {tid} WHERE true")).await.is_err());
        // Hidden and per-job datasets stay out of the schema list.
        assert!(list_schemas(&h).await.unwrap().iter().all(|d| !is_job_id(d)));
        assert!(query(&h, "SELECT nope FROM no_such_table").await.is_err());
        // Qualified: the emulator resolves DROP against the project, not the default dataset.
        query(&h, &format!("DROP TABLE {dataset}.{tid}")).await.unwrap();
        assert!(!list_tables(&h, dataset).await.unwrap().iter().any(|t| t.name == tid));
    }
}
