//! Convex, over a deployment's HTTP API with its admin (deploy) key.
//!
//! Browsing and editing use the same system functions the Convex dashboard
//! calls (`_system/frontend/*`): the table list, a page of documents with the
//! dashboard's filter expression, and insert/replace/delete. They run as
//! ordinary queries and mutations on `POST /api/query` and `/api/mutation`
//! with `Authorization: Convex <admin key>`. Only the calls in `SYSTEM` go out
//! from the browser; the console runs the app's own deployed functions.
//!
//! Values travel in Convex's encoded JSON, where an int64 is
//! `{"$integer": <base64 of 8 little-endian bytes>}`. The app sees it as
//! `{"$integer": "<decimal>"}` so the grid shows and edits the number; `encode`
//! turns it back on the way in.

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvexConfig {
    pub name: String,
    /// The deployment URL, `https://happy-animal-123.convex.cloud`.
    pub url: String,
    /// An admin key or deploy key (`prod:happy-animal-123|ey...`).
    pub admin_key: String,
}

impl ConvexConfig {
    pub fn base_url(&self) -> String {
        let u = self.url.trim().trim_end_matches('/');
        if u.starts_with("http://") || u.starts_with("https://") { u.to_string() } else { format!("https://{u}") }
    }
}

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        crate::proxy::http_builder(crate::proxy::Kind::Database)
            .user_agent("stroke/1.0")
            .connect_timeout(Duration::from_secs(10))
            // Convex stops a query at 1s of database time and a mutation at
            // 1s too; actions run up to 10 minutes.
            .timeout(Duration::from_secs(120))
            .build()
            .expect("failed to build Convex HTTP client")
    })
}

/// The system functions the browser may call, and whether each one writes.
pub const SYSTEM: &[(&str, bool)] = &[
    ("_system/frontend/getTableMapping", false),
    ("_system/frontend/tableSize", false),
    ("_system/frontend/paginatedTableDocuments", false),
    ("_system/frontend/getById", false),
    ("_system/frontend/indexes", false),
    ("_system/frontend/addDocument", true),
    ("_system/frontend/replaceDocument", true),
    ("_system/frontend/deleteDocuments", true),
    ("_system/frontend/patchDocumentsFields", true),
    ("_system/frontend/createTable", true),
];

/// Tables the dashboard lists although their names start with `_`.
const SHOWN_SYSTEM_TABLES: &[&str] = &["_scheduled_jobs", "_file_storage"];

// ── Values ───────────────────────────────────────────────────────────────────

/// Convex encoded JSON → the app's JSON: `$integer` as a decimal string.
pub fn decode(v: Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.into_iter().map(decode).collect()),
        Value::Object(o) => {
            if o.len() == 1 {
                if let Some(Value::String(b)) = o.get("$integer") {
                    if let Some(n) = B64.decode(b).ok().and_then(|x| <[u8; 8]>::try_from(x).ok()).map(i64::from_le_bytes) {
                        return json!({ "$integer": n.to_string() });
                    }
                }
            }
            Value::Object(o.into_iter().map(|(k, x)| (k, decode(x))).collect())
        }
        other => other,
    }
}

/// The app's JSON → Convex encoded JSON. A `$integer` that is already base64
/// (as a pasted document might carry) passes through.
pub fn encode(v: Value) -> Result<Value, String> {
    Ok(match v {
        Value::Array(a) => Value::Array(a.into_iter().map(encode).collect::<Result<_, _>>()?),
        Value::Object(o) => {
            if o.len() == 1 {
                match o.get("$integer") {
                    // Eight bytes in base64 always end in one `=`; a decimal never does.
                    Some(Value::String(s)) if s.ends_with('=') => {
                        return Ok(Value::Object(o));
                    }
                    Some(Value::String(s)) => {
                        let n: i64 = s.trim().parse().map_err(|_| format!("\"{s}\" is not a 64-bit integer"))?;
                        return Ok(json!({ "$integer": B64.encode(n.to_le_bytes()) }));
                    }
                    Some(Value::Number(n)) if n.is_i64() => {
                        return Ok(json!({ "$integer": B64.encode(n.as_i64().unwrap_or(0).to_le_bytes()) }));
                    }
                    _ => {}
                }
            }
            Value::Object(o.into_iter().map(|(k, x)| encode(x).map(|x| (k, x))).collect::<Result<_, _>>()?)
        }
        other => other,
    })
}

// ── Calls ────────────────────────────────────────────────────────────────────

/// `query` | `mutation` | `action` | `function` (any kind, for the console).
async fn call_raw(cfg: &ConvexConfig, kind: &str, path: &str, args: &Value) -> Result<Value, String> {
    if cfg.url.trim().is_empty() {
        return Err("Enter the deployment URL".into());
    }
    if cfg.admin_key.trim().is_empty() {
        return Err("Enter a deploy key or admin key".into());
    }
    let url = format!("{}/api/{kind}", cfg.base_url());
    let resp = client()
        .post(&url)
        .header("Authorization", format!("Convex {}", cfg.admin_key.trim()))
        .json(&json!({ "path": path, "args": args, "format": "convex_encoded_json" }))
        .send()
        .await
        .map_err(|e| if e.is_connect() { format!("Could not reach Convex at {}: {e}", cfg.base_url()) } else { e.to_string() })?;
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if status == 401 || body["code"] == "BadAdminKey" || body["code"] == "BadDeployKey" {
        // "401" lets the app's auth-failure check mint a fresh deploy key for
        // a provider connection whose key was revoked.
        return Err("Convex refused the key (401). Use a deploy key or admin key for this deployment.".into());
    }
    match body["status"].as_str() {
        Some("success") => Ok(decode(body["value"].clone())),
        Some("error") => Err(error_text(&body)),
        _ if !(200..300).contains(&status) => {
            let msg = body["message"].as_str().map(String::from).unwrap_or_else(|| text.chars().take(300).collect());
            Err(format!("Convex error ({status}): {msg}"))
        }
        _ => Err(format!("Unexpected Convex response: {}", text.chars().take(300).collect::<String>())),
    }
}

/// `errorMessage` without the request id prefix and the `Server Error` line.
fn error_text(body: &Value) -> String {
    let raw = body["errorMessage"].as_str().unwrap_or("Convex function failed");
    let mut s = raw.trim();
    if s.starts_with("[Request ID:") {
        s = s.split_once(']').map(|(_, r)| r.trim_start()).unwrap_or(s);
    }
    let s = s.strip_prefix("Server Error").map(str::trim_start).unwrap_or(s);
    let s = s.strip_prefix("Uncaught ").unwrap_or(s);
    s.trim().to_string()
}

/// One system function from the browser, checked against `SYSTEM`.
pub async fn system_call(cfg: &ConvexConfig, path: &str, args: Value) -> Result<Value, String> {
    let (_, writes) = SYSTEM.iter().find(|(p, _)| *p == path).ok_or_else(|| format!("{path} is not available from Stroke"))?;
    // Without `ids` this one patches every document in the table.
    if path == "_system/frontend/patchDocumentsFields" && !args["ids"].as_array().is_some_and(|ids| !ids.is_empty()) {
        return Err("patchDocumentsFields needs the ids of the documents to change".into());
    }
    call_raw(cfg, if *writes { "mutation" } else { "query" }, path, &encode(args)?).await
}

/// Connection test: the table list.
pub async fn ping(cfg: &ConvexConfig) -> Result<(), String> {
    list_tables(cfg).await.map(|_| ())
}

/// Table names, user tables first, then the two system tables the dashboard shows.
pub async fn list_tables(cfg: &ConvexConfig) -> Result<Vec<String>, String> {
    let m = call_raw(cfg, "query", "_system/frontend/getTableMapping", &json!({})).await?;
    let mut names: Vec<String> = m.as_object().into_iter().flatten().filter_map(|(_, n)| n.as_str().map(String::from)).collect();
    names.retain(|n| !n.starts_with('_') || SHOWN_SYSTEM_TABLES.contains(&n.as_str()));
    names.sort_by(|a, b| a.starts_with('_').cmp(&b.starts_with('_')).then_with(|| a.cmp(b)));
    names.dedup();
    Ok(names)
}

pub async fn table_size(cfg: &ConvexConfig, table: &str) -> Result<i64, String> {
    let v = call_raw(cfg, "query", "_system/frontend/tableSize", &json!({ "tableName": table })).await?;
    Ok(v.as_f64().map(|n| n as i64).unwrap_or(-1))
}

/// One page of a table, newest first, as the dashboard pages it.
pub async fn page(cfg: &ConvexConfig, table: &str, limit: u64, cursor: Option<&str>) -> Result<(Vec<Value>, Option<String>), String> {
    let v = call_raw(
        cfg,
        "query",
        "_system/frontend/paginatedTableDocuments",
        &json!({ "table": table, "filters": null, "paginationOpts": { "numItems": limit, "cursor": cursor } }),
    )
    .await?;
    let docs = v["page"].as_array().cloned().unwrap_or_default();
    let next = if v["isDone"].as_bool().unwrap_or(true) { None } else { v["continueCursor"].as_str().map(String::from) };
    Ok((docs, next))
}

// ── Console ──────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum Statement {
    /// `users`, `users.limit(20)`: documents, newest first.
    Table { table: String, limit: u64 },
    /// `users.get("j57...")`: one document by id.
    Get { id: String },
    /// `messages:list {"limit": 5}`: a deployed function and its arguments.
    Call { path: String, args: Value },
}

const USAGE: &str = r#"Run a deployed function as messages:list {"limit": 5}, or read a table with users, users.limit(20) or users.get("<id>")"#;

fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A function path: `module:name`, `dir/module:name`, or `module` (its default export).
fn is_function_path(s: &str) -> bool {
    let (module, name) = s.split_once(':').unwrap_or((s, "default"));
    is_ident(name) && !module.is_empty() && module.split('/').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')))
}

pub fn parse(stmt: &str) -> Result<Statement, String> {
    let s = stmt.trim().trim_end_matches(';').trim();
    if s.is_empty() {
        return Err(USAGE.into());
    }
    // `module:fn` then optional JSON args.
    let head_end = s.find(|c: char| c.is_whitespace() || c == '(' || c == '{').unwrap_or(s.len());
    let head = &s[..head_end];
    if head.contains(':') {
        if !is_function_path(head) {
            return Err(format!("\"{head}\" is not a function path. {USAGE}"));
        }
        if head.starts_with("_system") {
            return Err("System functions are not available from the console".into());
        }
        let mut rest = s[head_end..].trim();
        if rest.starts_with('(') && rest.ends_with(')') {
            rest = rest[1..rest.len() - 1].trim();
        }
        let args = if rest.is_empty() { json!({}) } else { serde_json::from_str(rest).map_err(|e| format!("The arguments aren't valid JSON: {e}"))? };
        if !args.is_object() {
            return Err("Function arguments are one JSON object, e.g. {\"limit\": 5}".into());
        }
        return Ok(Statement::Call { path: head.to_string(), args });
    }
    // `table`, `table.limit(n)`, `table.get("id")`.
    let (table, call) = match s.split_once('.') {
        Some((t, c)) => (t.trim(), Some(c.trim())),
        None => (s, None),
    };
    if !is_ident(table) {
        return Err(USAGE.into());
    }
    let arg = |c: &str, name: &str| -> Option<String> { c.strip_prefix(name)?.strip_prefix('(')?.strip_suffix(')').map(|a| a.trim().to_string()) };
    match call {
        None => Ok(Statement::Table { table: table.into(), limit: 100 }),
        Some(c) => {
            if let Some(n) = arg(c, "limit") {
                let limit = n.parse::<u64>().map_err(|_| format!("limit() takes a number. {USAGE}"))?;
                Ok(Statement::Table { table: table.into(), limit: limit.clamp(1, 1000) })
            } else if let Some(id) = arg(c, "get") {
                let id: String = serde_json::from_str(&id).map_err(|_| format!("get() takes the id in quotes. {USAGE}"))?;
                Ok(Statement::Get { id })
            } else {
                Err(USAGE.into())
            }
        }
    }
}

/// Whether a statement runs on its own without writing: table reads do; a
/// function call may be a mutation or an action.
pub fn reads(stmt: &str) -> bool {
    matches!(parse(stmt), Ok(Statement::Table { .. } | Statement::Get { .. }))
}

/// The kind of each deployed function, `module:name` → `Query` | `Mutation` | `Action`.
async fn function_kind(cfg: &ConvexConfig, path: &str) -> Result<Option<String>, String> {
    let (module, name) = path.split_once(':').unwrap_or((path, "default"));
    let mods = call_raw(cfg, "query", "_system/frontend/modules:list", &json!({})).await?;
    for m in mods.as_array().into_iter().flatten() {
        let file = m[0].as_str().unwrap_or("");
        let stem = file.strip_suffix(".js").or_else(|| file.strip_suffix(".ts")).unwrap_or(file);
        if stem != module {
            continue;
        }
        for f in m[1]["functions"].as_array().into_iter().flatten() {
            if f["name"] == name {
                return Ok(f["udfType"].as_str().map(String::from));
            }
        }
    }
    Ok(None)
}

pub async fn query(cfg: &ConvexConfig, stmt: &str) -> Result<super::query::SqlResult, String> {
    query_as(cfg, stmt, true).await
}

/// Like `query`; with `allow_write` false only query functions run.
pub async fn query_as(cfg: &ConvexConfig, stmt: &str, allow_write: bool) -> Result<super::query::SqlResult, String> {
    let t0 = Instant::now();
    match parse(stmt)? {
        Statement::Table { table, limit } => {
            let (docs, _) = page(cfg, &table, limit, None).await?;
            Ok(super::couchdb::docs_result(docs, None, stmt, t0))
        }
        Statement::Get { id } => {
            let d = call_raw(cfg, "query", "_system/frontend/getById", &json!({ "id": id })).await?;
            Ok(super::couchdb::docs_result(if d.is_null() { vec![] } else { vec![d] }, None, stmt, t0))
        }
        Statement::Call { path, args } => {
            if !allow_write {
                match function_kind(cfg, &path).await?.as_deref() {
                    Some("Query") => {}
                    Some(k) => return Err(format!("{path} is a {}; this connection only runs queries.", k.to_lowercase())),
                    None => return Err(format!("No deployed function {path}")),
                }
            }
            let v = call_raw(cfg, "function", &path, &encode(args)?).await?;
            Ok(value_result(v, stmt, t0))
        }
    }
}

/// A function's return value as a grid: documents as rows, anything else as one cell.
fn value_result(v: Value, stmt: &str, t0: Instant) -> super::query::SqlResult {
    let rows = match v {
        Value::Array(a) if a.iter().all(Value::is_object) => a,
        // A paginated query's result.
        Value::Object(ref o) if o.get("page").is_some_and(Value::is_array) && o.contains_key("isDone") => o["page"].as_array().cloned().unwrap_or_default(),
        Value::Object(_) => vec![v],
        other => vec![json!({ "value": other })],
    };
    super::couchdb::docs_result(rows, None, stmt, t0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int64_round_trips() {
        let wire = json!({ "a": { "$integer": "BwAAAAAAAAA=" }, "b": [{ "$integer": "//////////8=" }], "c": 1.5 });
        let app = decode(wire.clone());
        assert_eq!(app, json!({ "a": { "$integer": "7" }, "b": [{ "$integer": "-1" }], "c": 1.5 }));
        assert_eq!(encode(app).unwrap(), wire);
        // Already base64: unchanged.
        assert_eq!(encode(json!({ "$integer": "BwAAAAAAAAA=" })).unwrap(), json!({ "$integer": "BwAAAAAAAAA=" }));
        assert_eq!(encode(json!({ "$integer": 7 })).unwrap(), json!({ "$integer": "BwAAAAAAAAA=" }));
        assert!(encode(json!({ "$integer": "seven" })).is_err());
        // Other objects with one key are left alone.
        assert_eq!(decode(json!({ "$bytes": "AQI=" })), json!({ "$bytes": "AQI=" }));
    }

    #[test]
    fn console_parses() {
        assert_eq!(parse("users").unwrap(), Statement::Table { table: "users".into(), limit: 100 });
        assert_eq!(parse("users.limit(5);").unwrap(), Statement::Table { table: "users".into(), limit: 5 });
        assert_eq!(parse(r#"users.get("j57abc")"#).unwrap(), Statement::Get { id: "j57abc".into() });
        assert_eq!(parse(r#"messages:list {"limit": 5}"#).unwrap(), Statement::Call { path: "messages:list".into(), args: json!({ "limit": 5 }) });
        assert_eq!(parse(r#"admin/users:byEmail({"email": "a@b.c"})"#).unwrap(), Statement::Call { path: "admin/users:byEmail".into(), args: json!({ "email": "a@b.c" }) });
        assert_eq!(parse("messages:count").unwrap(), Statement::Call { path: "messages:count".into(), args: json!({}) });
        assert!(parse("messages:list [1]").is_err());
        assert!(parse("_system/frontend/addDocument:default {}").is_err());
        assert!(parse("SELECT * FROM users").is_err());
        assert!(parse("users.drop()").is_err());
        assert!(reads("users.limit(3)"));
        assert!(reads(r#"users.get("x")"#));
        assert!(!reads("messages:send {\"body\": \"x\"}"));
    }

    #[test]
    fn errors_lose_the_request_id() {
        let b = json!({ "errorMessage": "[Request ID: 8fa7] Server Error\nUncaught Error: Object is missing `x`" });
        assert_eq!(error_text(&b), "Error: Object is missing `x`");
        assert_eq!(error_text(&json!({ "errorMessage": "Could not find function" })), "Could not find function");
    }

    #[test]
    fn only_listed_system_functions() {
        assert!(SYSTEM.iter().any(|(p, w)| *p == "_system/frontend/replaceDocument" && *w));
        assert!(!SYSTEM.iter().any(|(p, _)| p.contains("snapshotImport") || p.contains("clearTablePage")));
    }

    fn live() -> Option<ConvexConfig> {
        Some(ConvexConfig { name: "t".into(), url: std::env::var("CONVEX_TEST_URL").ok()?, admin_key: std::env::var("CONVEX_TEST_ADMIN_KEY").ok()? })
    }

    /// Against a local backend (`convex-local-backend`) with the functions in
    /// `messages` deployed and a seeded `users` table:
    /// `CONVEX_TEST_URL=http://127.0.0.1:3210 CONVEX_TEST_ADMIN_KEY=... cargo test --lib convex_live -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn convex_live() {
        let Some(c) = live() else { return };
        ping(&c).await.unwrap();
        let tables = list_tables(&c).await.unwrap();
        assert!(tables.contains(&"users".to_string()), "{tables:?}");
        assert!(table_size(&c, "users").await.unwrap() >= 1);
        let r = query(&c, "users.limit(1)").await.unwrap();
        assert_eq!(r.rows.len(), 1);
        let r = query(&c, r#"messages:list {"limit": 1}"#).await.unwrap();
        assert_eq!(r.rows.len(), 1);
        let err = query_as(&c, r#"messages:send {"body": "x"}"#, false).await.unwrap_err();
        assert!(err.contains("mutation"), "{err}");
        assert!(query(&c, "nope:missing").await.unwrap_err().contains("Could not find"));
        let bad = ConvexConfig { admin_key: "nope".into(), ..c.clone() };
        assert!(ping(&bad).await.unwrap_err().contains("refused the key"));
        assert!(system_call(&c, "_system/frontend/clearTablePage", json!({})).await.is_err());
        let all = system_call(&c, "_system/frontend/patchDocumentsFields", json!({ "table": "users", "fields": { "x": 1 } })).await.unwrap_err();
        assert!(all.contains("needs the ids"), "{all}");
    }
}
