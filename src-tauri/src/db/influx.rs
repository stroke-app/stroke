//! InfluxDB 1.x, 2.x and 3.x over HTTP. Beta, and read-only.
//!
//! Measurements are the collections and points the documents (one row per
//! timestamp and series, fields and tags as columns, newest first).
//!
//! - 3.x (Core, Enterprise, Cloud Dedicated/Serverless): SQL through
//!   `/api/v3/query_sql`. The console takes SQL, or InfluxQL after `influxql:`.
//! - 2.x and Cloud TSM: the console takes Flux or InfluxQL; the browser uses
//!   InfluxQL through the 1.x compatibility API (`/query`).
//! - 1.x: InfluxQL.
//!
//! Nothing here writes. The console refuses InfluxQL other than SELECT, SHOW
//! and EXPLAIN, and Flux that writes (`to()`) or reaches out of the database
//! (`http.post`, `sql`, notification packages).

use super::query::{ColumnInfo, SqlResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfluxConfig {
    pub name: String,
    pub url: String,
    /// "1", "2" or "3".
    #[serde(default = "default_version")]
    pub version: String,
    /// The bucket (2.x) or database (1.x, 3.x).
    pub database: String,
    /// The organization (2.x only).
    #[serde(default)]
    pub org: Option<String>,
    /// API token (2.x, 3.x).
    #[serde(default)]
    pub token: Option<String>,
    /// User and password (1.x).
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

fn default_version() -> String {
    "2".into()
}

const MAX_ROWS: usize = 10_000;

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| reqwest::Client::builder().timeout(Duration::from_secs(120)).connect_timeout(Duration::from_secs(10)).build().unwrap_or_default())
}

fn base(cfg: &InfluxConfig) -> String {
    let u = cfg.url.trim().trim_end_matches('/');
    if u.contains("://") { u.to_string() } else { format!("http://{u}") }
}

fn v3(cfg: &InfluxConfig) -> bool {
    cfg.version.trim() == "3"
}

fn auth(cfg: &InfluxConfig, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    if let Some(t) = cfg.token.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        // 2.x reads "Token", 3.x "Bearer"; both take the other too.
        return req.header("Authorization", if v3(cfg) { format!("Bearer {t}") } else { format!("Token {t}") });
    }
    match cfg.user.as_deref().filter(|u| !u.is_empty()) {
        Some(u) => req.basic_auth(u, cfg.password.as_deref()),
        None => req,
    }
}

async fn send(cfg: &InfluxConfig, req: reqwest::RequestBuilder) -> Result<String, String> {
    let resp = auth(cfg, req).send().await.map_err(|e| {
        if e.is_connect() {
            format!("Could not reach InfluxDB at {}: is it running?", base(cfg))
        } else {
            format!("InfluxDB request failed: {e}")
        }
    })?;
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if status == 401 || status == 403 {
        return Err("InfluxDB refused the credentials (check the token, or the user and password)".into());
    }
    if status >= 400 {
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let msg = v["message"].as_str().or_else(|| v["error"].as_str()).map(String::from).unwrap_or_else(|| text.chars().take(300).collect());
        return Err(format!("InfluxDB: {msg}"));
    }
    Ok(text)
}

// ── The three query paths ─────────────────────────────────────────────────────

/// A table: column names, then rows.
type Table = (Vec<String>, Vec<Vec<Value>>);

/// InfluxQL through `/query` (1.x, and 2.x's compatibility API); series are
/// flattened, their tags becoming columns.
async fn influxql(cfg: &InfluxConfig, q: &str) -> Result<Table, String> {
    if v3(cfg) {
        let req = client().post(format!("{}/api/v3/query_influxql", base(cfg))).json(&json!({ "db": cfg.database.trim(), "q": q, "format": "json" }));
        return json_rows(&send(cfg, req).await?);
    }
    // Times come back as RFC 3339 strings (no `epoch`).
    let req = client().get(format!("{}/query", base(cfg))).query(&[("db", cfg.database.trim()), ("q", q)]);
    let text = send(cfg, req).await?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("InfluxDB answered something unexpected: {e}"))?;
    let mut cols: Vec<String> = Vec::new();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    for res in v["results"].as_array().into_iter().flatten() {
        if let Some(e) = res["error"].as_str() {
            return Err(format!("InfluxDB: {e}"));
        }
        for s in res["series"].as_array().into_iter().flatten() {
            let tags: Vec<(String, Value)> = s["tags"].as_object().map(|t| t.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
            let names: Vec<String> = s["columns"].as_array().into_iter().flatten().filter_map(|c| c.as_str().map(String::from)).collect();
            for n in names.iter().chain(tags.iter().map(|(k, _)| k)) {
                if !cols.contains(n) {
                    cols.push(n.clone());
                }
            }
            for r in s["values"].as_array().into_iter().flatten() {
                let mut row = vec![Value::Null; cols.len()];
                for (i, n) in names.iter().enumerate() {
                    let at = cols.iter().position(|c| c == n).unwrap();
                    row[at] = r[i].clone();
                }
                for (k, v) in &tags {
                    let at = cols.iter().position(|c| c == k).unwrap();
                    row[at] = v.clone();
                }
                rows.push(row);
            }
        }
    }
    for r in &mut rows {
        r.resize(cols.len(), Value::Null);
    }
    Ok((cols, rows))
}

/// Rows of JSON objects (3.x), `time` first.
fn json_rows(text: &str) -> Result<Table, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("InfluxDB answered something unexpected: {e}"))?;
    let objs = v.as_array().cloned().unwrap_or_default();
    let mut cols: Vec<String> = Vec::new();
    for o in &objs {
        for k in o.as_object().into_iter().flat_map(|m| m.keys()) {
            if !cols.contains(k) {
                cols.push(k.clone());
            }
        }
    }
    if let Some(i) = cols.iter().position(|c| c == "time") {
        let t = cols.remove(i);
        cols.insert(0, t);
    }
    let rows = objs.iter().map(|o| cols.iter().map(|c| o.get(c).cloned().unwrap_or(Value::Null)).collect()).collect();
    Ok((cols, rows))
}

async fn sql(cfg: &InfluxConfig, q: &str) -> Result<Table, String> {
    let req = client().post(format!("{}/api/v3/query_sql", base(cfg))).json(&json!({ "db": cfg.database.trim(), "q": q, "format": "json" }));
    json_rows(&send(cfg, req).await?)
}

/// Flux (2.x): annotated CSV, every table of the result stacked.
async fn flux(cfg: &InfluxConfig, q: &str) -> Result<Table, String> {
    let org = cfg.org.as_deref().map(str::trim).unwrap_or("");
    let req = client()
        .post(format!("{}/api/v2/query", base(cfg)))
        .query(&[("org", org)])
        .header("Accept", "application/csv")
        .json(&json!({ "query": q, "type": "flux", "dialect": { "header": true, "annotations": ["datatype"], "delimiter": "," } }));
    Ok(flux_csv(&send(cfg, req).await?))
}

/// Split one CSV line, honouring double quotes.
fn csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Split CSV text into records: a newline inside quotes stays in the field.
fn csv_records(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                cur.push(c);
            }
            '\n' if !quoted => out.push(std::mem::take(&mut cur).trim_end_matches('\r').to_string()),
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn typed(kind: &str, s: &str) -> Value {
    if s.is_empty() {
        return Value::Null;
    }
    match kind {
        "long" => s.parse::<i64>().map(Value::from).unwrap_or_else(|_| json!(s)),
        "unsignedLong" => s.parse::<u64>().map(Value::from).unwrap_or_else(|_| json!(s)),
        "double" => s.parse::<f64>().ok().and_then(|f| serde_json::Number::from_f64(f).map(Value::Number)).unwrap_or_else(|| json!(s)),
        "boolean" => json!(s == "true"),
        _ => json!(s),
    }
}

/// Annotated CSV: each table brings its `#datatype` row and header. The
/// `result` and `table` bookkeeping columns are left out.
pub fn flux_csv(text: &str) -> Table {
    let mut cols: Vec<String> = Vec::new();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut kinds: Vec<String> = Vec::new();
    let mut header: Option<Vec<String>> = None;
    for line in csv_records(text) {
        if line.trim().is_empty() {
            header = None;
            continue;
        }
        let cells = csv_line(&line);
        if cells.first().is_some_and(|c| c == "#datatype") {
            kinds = cells;
            header = None;
            continue;
        }
        if cells.first().is_some_and(|c| c.starts_with('#')) {
            continue;
        }
        let Some(h) = &header else {
            for n in cells.iter().skip(1) {
                if !n.is_empty() && n != "result" && n != "table" && !cols.contains(n) {
                    cols.push(n.clone());
                }
            }
            header = Some(cells);
            continue;
        };
        let mut row = vec![Value::Null; cols.len()];
        for (i, n) in h.iter().enumerate().skip(1) {
            if let Some(at) = cols.iter().position(|c| c == n) {
                row[at] = typed(kinds.get(i).map(String::as_str).unwrap_or("string"), cells.get(i).map(String::as_str).unwrap_or(""));
            }
        }
        rows.push(row);
    }
    for r in &mut rows {
        r.resize(cols.len(), Value::Null);
    }
    (cols, rows)
}

// ── Reads only ────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum Lang {
    Sql,
    InfluxQl,
    Flux,
}

/// Which language a console statement is in, for this server.
pub fn lang_of(cfg: &InfluxConfig, stmt: &str) -> (Lang, String) {
    let s = stmt.trim();
    if let Some(rest) = s.strip_prefix("influxql:").or_else(|| s.strip_prefix("INFLUXQL:")) {
        return (Lang::InfluxQl, rest.trim().to_string());
    }
    if s.contains("|>") || s.starts_with("from(") || s.starts_with("import ") || s.starts_with("buckets(") {
        return (Lang::Flux, s.to_string());
    }
    (if v3(cfg) { Lang::Sql } else { Lang::InfluxQl }, s.to_string())
}

/// Strings and comments out, so they can't hide or fake a keyword.
fn bare(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' | '\'' => {
                while let Some(d) = chars.next() {
                    if d == '\\' {
                        chars.next();
                    } else if d == c {
                        break;
                    }
                }
                out.push(' ');
            }
            '/' if chars.peek() == Some(&'/') => {
                for d in chars.by_ref() {
                    if d == '\n' {
                        break;
                    }
                }
                out.push('\n');
            }
            '-' if chars.peek() == Some(&'-') => {
                for d in chars.by_ref() {
                    if d == '\n' {
                        break;
                    }
                }
                out.push('\n');
            }
            c => out.push(c),
        }
    }
    out
}

/// Flux packages that only compute or read this database.
const FLUX_IMPORTS: &[&str] = &[
    "influxdata/influxdb/schema", "influxdata/influxdb/v1", "strings", "math", "date", "regexp", "array", "join", "types",
    "timezone", "interpolate", "experimental", "experimental/aggregate", "experimental/array", "experimental/table", "dict", "universe",
];

/// Why the console won't run this, if it won't: the engine is read-only.
pub fn refusal(lang: &Lang, stmt: &str) -> Option<String> {
    match lang {
        Lang::Flux => {
            for line in stmt.lines().map(str::trim).filter(|l| l.starts_with("import")) {
                let pkg = line.trim_start_matches("import").trim().trim_matches('"');
                if !FLUX_IMPORTS.contains(&pkg) {
                    return Some(format!("The Flux package \"{pkg}\" can write or reach other systems; InfluxDB is read-only here"));
                }
            }
            let b = bare(stmt);
            let writes = regex_lite_to(&b);
            writes.then(|| "This Flux writes (to()); InfluxDB is read-only here".to_string())
        }
        Lang::InfluxQl | Lang::Sql => {
            let b = bare(stmt).to_ascii_uppercase();
            let parts: Vec<&str> = b.split(';').map(str::trim).filter(|p| !p.is_empty()).collect();
            if parts.len() > 1 && matches!(lang, Lang::Sql) {
                return Some("Run one statement at a time".into());
            }
            for p in parts {
                let head = p.split_whitespace().next().unwrap_or("");
                if !matches!(head, "SELECT" | "SHOW" | "EXPLAIN" | "WITH" | "DESCRIBE") {
                    return Some(format!("{head} changes data; InfluxDB is read-only here (SELECT, SHOW and EXPLAIN run)"));
                }
                if matches!(lang, Lang::InfluxQl) && head == "SELECT" && p.split_whitespace().any(|w| w == "INTO") {
                    return Some("SELECT ... INTO writes; InfluxDB is read-only here".into());
                }
            }
            None
        }
    }
}

/// A call to `to(`, `wideTo(` or `*.to(` in Flux.
fn regex_lite_to(b: &str) -> bool {
    let bytes = b.as_bytes();
    let mut i = 0;
    while let Some(off) = b[i..].find("to") {
        let at = i + off;
        let before = if at == 0 { b' ' } else { bytes[at - 1] };
        let word_start = !(before.is_ascii_alphanumeric() || before == b'_') || b[..at].ends_with("wide");
        let after = b[at + 2..].trim_start();
        if word_start && after.starts_with('(') {
            return true;
        }
        i = at + 2;
    }
    false
}

pub async fn connect(cfg: &InfluxConfig) -> Result<InfluxConfig, String> {
    if cfg.url.trim().is_empty() {
        return Err("Enter the InfluxDB address".into());
    }
    if cfg.database.trim().is_empty() {
        return Err(if cfg.version.trim() == "2" { "Enter the bucket".into() } else { "Enter the database".into() });
    }
    if cfg.version.trim() == "2" && cfg.org.as_deref().map(str::trim).unwrap_or("").is_empty() {
        return Err("Enter the organization".into());
    }
    measurements(cfg).await?;
    Ok(cfg.clone())
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "float",
        _ => "string",
    }
}

fn to_result(cols: Vec<String>, mut rows: Vec<Vec<Value>>, sql: &str, t0: Instant) -> SqlResult {
    let total = rows.len();
    rows.truncate(MAX_ROWS);
    let columns = cols
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let k = if n == "time" || n == "_time" { "timestamp" } else { rows.iter().map(|r| kind_of(&r[i])).find(|k| *k != "null").unwrap_or("string") };
            ColumnInfo::new(n.clone(), k)
        })
        .collect();
    let message = (total > rows.len()).then(|| format!("Showing the first {} of {total} rows", rows.len()));
    SqlResult { row_count: Some(rows.len() as i64), columns, rows, message, query_ms: t0.elapsed().as_millis() as u64, sql: sql.to_string() }
}

/// A console statement: SQL (3.x), Flux (2.x) or InfluxQL.
pub async fn query(cfg: &InfluxConfig, stmt: &str) -> Result<SqlResult, String> {
    let t0 = Instant::now();
    let (lang, text) = lang_of(cfg, stmt.trim().trim_end_matches(';'));
    if text.trim().is_empty() {
        return Err("Query is empty".into());
    }
    if let Some(why) = refusal(&lang, &text) {
        return Err(why);
    }
    let (cols, rows) = match lang {
        Lang::Sql => sql(cfg, &text).await?,
        Lang::InfluxQl => influxql(cfg, &text).await?,
        Lang::Flux if v3(cfg) => return Err("InfluxDB 3 doesn't run Flux: use SQL, or InfluxQL after \"influxql:\"".into()),
        Lang::Flux => flux(cfg, &text).await?,
    };
    Ok(to_result(cols, rows, &text, t0))
}

// ── The browser ───────────────────────────────────────────────────────────────

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
}

fn sql_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Measurement names.
pub async fn measurements(cfg: &InfluxConfig) -> Result<Vec<String>, String> {
    let (cols, rows) = if v3(cfg) {
        sql(cfg, "SELECT table_name FROM information_schema.tables WHERE table_schema = 'iox' ORDER BY table_name").await?
    } else {
        influxql(cfg, "SHOW MEASUREMENTS").await?
    };
    let at = cols.iter().position(|c| c == "table_name" || c == "name").unwrap_or(0);
    Ok(rows.into_iter().filter_map(|r| r.get(at).and_then(|v| v.as_str().map(String::from))).collect())
}

/// Tags and fields of a measurement, with their types.
pub async fn describe(cfg: &InfluxConfig, m: &str) -> Result<Value, String> {
    if v3(cfg) {
        let (_, rows) = sql(
            cfg,
            &format!(
                "SELECT column_name, data_type FROM information_schema.columns WHERE table_schema = 'iox' AND table_name = '{}' ORDER BY ordinal_position",
                m.replace('\'', "''")
            ),
        )
        .await?;
        let mut tags = vec![];
        let mut fields = vec![];
        for r in rows {
            let name = r[0].as_str().unwrap_or("").to_string();
            let ty = r[1].as_str().unwrap_or("");
            if name == "time" {
                continue;
            }
            if ty.starts_with("Dictionary") {
                tags.push(json!(name));
            } else {
                fields.push(json!({ "name": name, "type": ty }));
            }
        }
        return Ok(json!({ "tags": tags, "fields": fields }));
    }
    let (_, tag_rows) = influxql(cfg, &format!("SHOW TAG KEYS FROM {}", quote_ident(m))).await?;
    let (_, field_rows) = influxql(cfg, &format!("SHOW FIELD KEYS FROM {}", quote_ident(m))).await?;
    Ok(json!({
        "tags": tag_rows.iter().map(|r| r[0].clone()).collect::<Vec<_>>(),
        "fields": field_rows.iter().map(|r| json!({ "name": r[0], "type": r.get(1).cloned().unwrap_or(Value::Null) })).collect::<Vec<_>>(),
    }))
}

/// A WHERE condition from the browser's filter box: one read-only expression.
fn check_filter(filter: &str) -> Result<(), String> {
    let b = bare(filter);
    if b.contains(';') {
        return Err("The filter is one condition: no \";\"".into());
    }
    let mut depth = 0i32;
    for c in b.chars() {
        depth += match c {
            '(' => 1,
            ')' => -1,
            _ => 0,
        };
        if depth < 0 {
            return Err("The filter's parentheses don't match".into());
        }
    }
    if depth != 0 {
        return Err("The filter's parentheses don't match".into());
    }
    let up = b.to_ascii_uppercase();
    if up.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').any(|w| matches!(w, "INTO" | "UNION" | "LIMIT" | "OFFSET" | "SLIMIT" | "SOFFSET" | "ORDER" | "GROUP")) {
        return Err("The filter is a WHERE condition only".into());
    }
    Ok(())
}

/// One page of points, newest first.
pub async fn page(cfg: &InfluxConfig, m: &str, filter: &str, skip: i64, limit: i64) -> Result<Value, String> {
    let filter = filter.trim();
    if !filter.is_empty() {
        check_filter(filter)?;
    }
    let wh = if filter.is_empty() { String::new() } else { format!(" WHERE {filter}") };
    let limit = limit.clamp(1, 5000);
    let skip = skip.max(0);
    let (cols, rows) = if v3(cfg) {
        sql(cfg, &format!("SELECT * FROM {}{wh} ORDER BY time DESC LIMIT {limit} OFFSET {skip}", sql_ident(m))).await?
    } else {
        influxql(cfg, &format!("SELECT * FROM {}{wh} ORDER BY time DESC LIMIT {limit} OFFSET {skip}", quote_ident(m))).await?
    };
    let docs: Vec<Value> = rows
        .into_iter()
        .map(|r| Value::Object(cols.iter().cloned().zip(r).collect::<Map<String, Value>>()))
        .collect();
    let next = (docs.len() as i64 >= limit).then_some(skip + docs.len() as i64);
    Ok(json!({ "docs": docs, "next": next }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(version: &str) -> InfluxConfig {
        InfluxConfig { name: "t".into(), url: "localhost:8086".into(), version: version.into(), database: "db".into(), org: Some("o".into()), token: None, user: None, password: None }
    }

    #[test]
    fn languages() {
        assert_eq!(lang_of(&cfg("2"), "from(bucket: \"b\") |> range(start: -1h)").0, Lang::Flux);
        assert_eq!(lang_of(&cfg("2"), "SELECT * FROM cpu").0, Lang::InfluxQl);
        assert_eq!(lang_of(&cfg("3"), "SELECT * FROM cpu").0, Lang::Sql);
        assert_eq!(lang_of(&cfg("3"), "influxql: SHOW MEASUREMENTS"), (Lang::InfluxQl, "SHOW MEASUREMENTS".into()));
    }

    #[test]
    fn writes_are_refused() {
        assert!(refusal(&Lang::InfluxQl, "SELECT * FROM cpu").is_none());
        assert!(refusal(&Lang::InfluxQl, "SHOW MEASUREMENTS").is_none());
        assert!(refusal(&Lang::InfluxQl, "DROP MEASUREMENT cpu").is_some());
        assert!(refusal(&Lang::InfluxQl, "SELECT * INTO copy FROM cpu").is_some());
        assert!(refusal(&Lang::InfluxQl, "SELECT * FROM cpu; DELETE FROM cpu").is_some());
        assert!(refusal(&Lang::InfluxQl, "SELECT * FROM cpu WHERE host = 'DROP x; INTO'").is_none(), "strings don't count");
        assert!(refusal(&Lang::Sql, "SELECT 1; SELECT 2").is_some());
        assert!(refusal(&Lang::Flux, "from(bucket: \"b\") |> range(start: -1h) |> filter(fn: (r) => r.to == \"x\")").is_none());
        assert!(refusal(&Lang::Flux, "from(bucket: \"b\") |> range(start: -1h) |> to(bucket: \"c\")").is_some());
        assert!(refusal(&Lang::Flux, "import \"experimental\"\nfrom(bucket: \"b\") |> range(start: -1h) |> experimental.to(bucket: \"c\")").is_some());
        assert!(refusal(&Lang::Flux, "import \"influxdata/influxdb/schema\"\nschema.measurements(bucket: \"b\")").is_none());
        assert!(refusal(&Lang::Flux, "import \"http\"\nhttp.post(url: \"https://x\")").is_some());
        assert!(refusal(&Lang::Flux, "import \"sql\"\nsql.from(driverName: \"postgres\", dataSourceName: \"x\", query: \"y\")").is_some());
    }

    #[test]
    fn filters() {
        assert!(check_filter("host = 'a' AND usage > 50").is_ok());
        assert!(check_filter("time > now() - 1h").is_ok());
        assert!(check_filter("1=1) LIMIT 1 (").is_err());
        assert!(check_filter("host = 'a'; DROP MEASUREMENT cpu").is_err());
        assert!(check_filter("host = 'a' GROUP BY host").is_err());
        assert!(check_filter("note = 'limit; (('").is_ok());
    }

    #[test]
    fn annotated_csv() {
        let text = "#datatype,string,long,dateTime:RFC3339,string,double\r\n,result,table,_time,host,usage\r\n,_result,0,2026-01-01T00:00:00Z,a,1.5\r\n,_result,0,2026-01-01T00:00:01Z,\"b,\"\"c\"\"\",2\r\n\r\n#datatype,string,long,string,long\r\n,result,table,host,cores\r\n,_result,1,a,4\r\n";
        let (cols, rows) = flux_csv(text);
        assert_eq!(cols, vec!["_time", "host", "usage", "cores"]);
        assert_eq!(rows[0], vec![json!("2026-01-01T00:00:00Z"), json!("a"), json!(1.5), Value::Null]);
        assert_eq!(rows[1][1], json!("b,\"c\""));
        assert_eq!(rows[2], vec![Value::Null, json!("a"), Value::Null, json!(4)]);
    }

    /// STROKE_TEST_INFLUX2 = url|org|bucket|token (with cpu and mem points),
    /// STROKE_TEST_INFLUX3 = url|database (no auth).
    #[tokio::test]
    async fn live_round_trip() {
        let mut ran = false;
        if let Ok(spec) = std::env::var("STROKE_TEST_INFLUX2") {
            let p: Vec<&str> = spec.split('|').collect();
            let c = InfluxConfig { name: "t".into(), url: p[0].into(), version: "2".into(), org: Some(p[1].into()), database: p[2].into(), token: Some(p[3].into()), user: None, password: None };
            check(&c).await;
            let r = query(&c, &format!("from(bucket: \"{}\") |> range(start: 0) |> filter(fn: (r) => r._measurement == \"cpu\" and r._field == \"usage\")", p[2])).await.unwrap();
            assert_eq!(r.rows.len(), 2);
            assert!(r.columns.iter().any(|c| c.name == "_value"));
            assert!(query(&c, &format!("from(bucket: \"{}\") |> range(start: 0) |> to(bucket: \"x\")", p[2])).await.is_err());
            let bad = InfluxConfig { token: Some("nope".into()), ..c.clone() };
            assert!(connect(&bad).await.unwrap_err().contains("refused"));
            ran = true;
        }
        if let Ok(spec) = std::env::var("STROKE_TEST_INFLUX3") {
            let p: Vec<&str> = spec.split('|').collect();
            let c = InfluxConfig { name: "t".into(), url: p[0].into(), version: "3".into(), org: None, database: p[1].into(), token: None, user: None, password: None };
            check(&c).await;
            let r = query(&c, "SELECT host, avg(usage) AS u FROM cpu GROUP BY host ORDER BY host").await.unwrap();
            assert_eq!(r.rows.len(), 2);
            assert!(query(&c, "influxql: SHOW MEASUREMENTS").await.unwrap().rows.len() >= 2);
            assert!(query(&c, "DELETE FROM cpu").await.is_err());
            ran = true;
        }
        let _ = ran;
    }

    async fn check(c: &InfluxConfig) {
        let c = connect(c).await.unwrap();
        let ms = measurements(&c).await.unwrap();
        assert!(ms.contains(&"cpu".to_string()) && ms.contains(&"mem".to_string()), "{ms:?}");
        let d = describe(&c, "cpu").await.unwrap();
        assert!(d["tags"].as_array().unwrap().iter().any(|t| t == "host"), "{d}");
        assert!(d["fields"].as_array().unwrap().iter().any(|f| f["name"] == "usage"), "{d}");
        let pg = page(&c, "cpu", "host = 'b'", 0, 50).await.unwrap();
        let docs = pg["docs"].as_array().unwrap();
        assert_eq!(docs.len(), 1, "{pg}");
        assert_eq!(docs[0]["usage"], json!(80.1));
        assert_eq!(docs[0]["region"], json!("us"));
        let all = page(&c, "cpu", "", 0, 1).await.unwrap();
        assert_eq!(all["next"], json!(1));
        assert!(page(&c, "cpu", "1=1; DROP MEASUREMENT cpu", 0, 1).await.is_err());
        let r = query(&c, "SELECT * FROM cpu").await.unwrap();
        assert_eq!(r.rows.len(), 2);
        assert_eq!(r.columns[0].name, "time");
        assert!(query(&c, "DROP MEASUREMENT cpu").await.is_err());
    }
}
