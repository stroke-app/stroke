//! Elasticsearch and OpenSearch over their REST API. They share one wire
//! format for everything Stroke uses (indices, `_doc`, `_search`, mappings),
//! so one module serves both; `flavor` only picks the SQL endpoint and the
//! name in messages.
//!
//! The query console takes three forms:
//!   GET logs-*/_search                               Dev Tools style: a request
//!   { "query": { "match": { "msg": "timeout" } } }   line, then a JSON body
//!   logs.search({"query": {"term": {"level": "error"}}})   shell style
//!   logs.get("id-1")                                 one document
//!   logs.count({"query": {...}})                     a count
//!   SELECT level, COUNT(*) FROM logs GROUP BY level  SQL (x-pack / SQL plugin)
//! The console runs reads and document writes; deleting an index is not offered.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EsConfig {
    pub name: String,
    /// Base URL, e.g. `http://localhost:9200` or an Elastic Cloud endpoint.
    pub url: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    /// An API key (the base64 `id:key` form Elastic shows once). Used instead
    /// of the user and password when set.
    #[serde(default)]
    pub api_key: Option<String>,
    /// `elasticsearch` or `opensearch`. Saved connections pick it; the server's
    /// own answer to `GET /` corrects it on connect.
    #[serde(default)]
    pub flavor: Option<String>,
    /// Accept a self-signed certificate (a local OpenSearch's demo TLS).
    #[serde(default)]
    pub insecure: bool,
    /// Reach the server through an SSH tunnel (connection::tunnel_to and friends).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<super::ssh_tunnel::SshConfig>,
    /// The tunnel's local end, when there is one: the URL keeps the server's
    /// real name (TLS checks it) and the client resolves that name to here.
    #[serde(skip)]
    pub via: Option<std::net::SocketAddr>,
}

impl EsConfig {
    pub fn is_opensearch(&self) -> bool {
        self.flavor.as_deref() == Some("opensearch")
    }
    fn label(&self) -> &'static str {
        if self.is_opensearch() { "OpenSearch" } else { "Elasticsearch" }
    }
}

fn client(insecure: bool) -> &'static reqwest::Client {
    static SAFE: OnceLock<reqwest::Client> = OnceLock::new();
    static LAX: OnceLock<reqwest::Client> = OnceLock::new();
    let cell = if insecure { &LAX } else { &SAFE };
    cell.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            .danger_accept_invalid_certs(insecure)
            .build()
            .unwrap_or_default()
    })
}

fn base(cfg: &EsConfig) -> Result<reqwest::Url, String> {
    let raw = cfg.url.trim().trim_end_matches('/');
    let raw = if raw.contains("://") { raw.to_string() } else { format!("http://{raw}") };
    reqwest::Url::parse(&format!("{raw}/"))
        .map(|u| super::ssh_tunnel::url_via(u, cfg.via))
        .map_err(|e| format!("Invalid {} URL \"{raw}\": {e}", cfg.label()))
}

/// Endpoints (`_name` path segments) that only read. The console may call
/// these with POST, since search bodies go in a POST.
const READ_ENDPOINTS: &[&str] = &[
    "_search", "_count", "_msearch", "_mget", "_explain", "_field_caps", "_validate", "_analyze", "_sql", "_mapping",
    "_mappings", "_settings", "_stats", "_alias", "_aliases", "_cat", "_cluster", "_nodes", "_doc", "_source",
    "_termvectors", "_resolve", "_data_stream", "_plugins", "_opendistro", "_pit", "_search_shards",
];

/// Read endpoints that take a POST body. `POST /{index}/_mapping` and
/// `POST /_aliases` change things, so they're GET only.
const POST_READ_ENDPOINTS: &[&str] = &[
    "_search", "_count", "_msearch", "_mget", "_explain", "_field_caps", "_validate", "_analyze", "_sql", "_plugins",
    "_opendistro", "_pit", "_search_shards", "_termvectors",
];

/// Endpoints the browser writes documents through.
const DOC_WRITE_ENDPOINTS: &[&str] = &["_doc", "_create", "_update", "_refresh", "_bulk"];

/// `_plugins` / `_opendistro` paths allowed: SQL and PPL only.
fn plugin_ok(p: &str) -> bool {
    let rest = p.split_once('/').map(|(_, r)| r).unwrap_or("");
    rest.starts_with("_sql") || rest.starts_with("_ppl")
}

/// The request's `_endpoint` segments, path only.
fn endpoints(p: &str) -> Vec<&str> {
    p.split('?').next().unwrap_or("").split('/').filter(|s| s.starts_with('_')).collect()
}

/// Requests the app may make. Cluster settings, security, snapshots, index
/// deletion and `_delete_by_query` are refused; documents can be written.
pub fn check_path(method: &str, path: &str) -> Result<(), String> {
    let p = path.trim_start_matches('/');
    if p.contains("..") {
        return Err("Invalid path".into());
    }
    if !matches!(method, "GET" | "HEAD" | "POST" | "PUT" | "DELETE") {
        return Err(format!("HTTP method {method} is not allowed"));
    }
    let path_only = p.split('?').next().unwrap_or("").trim_end_matches('/');
    if path_only.is_empty() {
        return if method == "GET" || method == "HEAD" { Ok(()) } else { Err("Not allowed on the server root".into()) };
    }
    let eps = endpoints(p);
    if eps.is_empty() {
        // `/{index}` alone: reading its definition is fine; creating or
        // deleting an index is not available from Stroke.
        return if method == "GET" || method == "HEAD" {
            Ok(())
        } else {
            Err("Creating or deleting indices is not available from Stroke".into())
        };
    }
    for e in &eps {
        if matches!(*e, "_plugins" | "_opendistro") && !plugin_ok(path_only.trim_start_matches(|c| c != '_')) {
            return Err(format!("/{path_only} is not available from Stroke"));
        }
    }
    let first = eps[0];
    let read = READ_ENDPOINTS.contains(&first);
    let write = DOC_WRITE_ENDPOINTS.contains(&first);
    match method {
        "GET" | "HEAD" if read || write => Ok(()),
        // Search bodies travel in a POST; `_doc`/`_create`/`_update`/`_bulk`/
        // `_refresh` write documents.
        "POST" if POST_READ_ENDPOINTS.contains(&first) => Ok(()),
        "POST" | "PUT" if write => Ok(()),
        "DELETE" if first == "_doc" && path_only.split('/').count() == 3 => Ok(()),
        "DELETE" if first == "_pit" => Ok(()),
        _ => Err(format!("{method} /{path_only} is not available from Stroke")),
    }
}

/// What an agent's request needs from the MCP access gate: reads are READ,
/// writing documents is WRITE, creating an index or adding to its mapping is
/// MODIFY, anything else (deleting an index, cluster settings) is DANGEROUS,
/// which `agent_may` refuses whatever the level.
pub fn request_level(method: &str, path: &str) -> u8 {
    use crate::mcp::access::{DANGEROUS, MODIFY, READ, WRITE};
    if is_read(method, path) {
        READ
    } else if check_path(method, path).is_ok() {
        WRITE
    } else if creates_schema(method, path) {
        MODIFY
    } else {
        DANGEROUS
    }
}

/// `PUT /{index}` (with optional settings and mappings) or `PUT|POST
/// /{index}/_mapping`: the schema changes an agent at MODIFY may make.
fn creates_schema(method: &str, path: &str) -> bool {
    let p = path.trim_start_matches('/');
    let path_only = p.split('?').next().unwrap_or("").trim_end_matches('/');
    if path_only.is_empty() || path_only.starts_with('_') || p.contains("..") {
        return false;
    }
    let eps = endpoints(p);
    (method == "PUT" && eps.is_empty() && !path_only.contains('/'))
        || (matches!(method, "PUT" | "POST") && eps.first() == Some(&"_mapping"))
}

/// Requests an agent may send once the gate has passed it: documents, plus
/// creating an index or its mapping. Index deletion stays unavailable.
fn agent_may(method: &str, path: &str) -> bool {
    check_path(method, path).is_ok() || creates_schema(method, path)
}

/// Whether a request only reads (the console and MCP accept only these).
pub fn is_read(method: &str, path: &str) -> bool {
    if check_path(method, path).is_err() {
        return false;
    }
    let eps = endpoints(path.trim_start_matches('/'));
    match method {
        "GET" | "HEAD" => true,
        "POST" => eps.first().is_some_and(|e| POST_READ_ENDPOINTS.contains(e)),
        _ => false,
    }
}

fn error_text(cfg: &EsConfig, status: reqwest::StatusCode, v: &Value) -> String {
    let err = &v["error"];
    // `{"error": {"root_cause": [...], "type": ..., "reason": ...}}`, or a string.
    let reason = err["root_cause"][0]["reason"]
        .as_str()
        .or_else(|| err["reason"].as_str())
        .or_else(|| err.as_str())
        .or_else(|| v["message"].as_str());
    let kind = err["root_cause"][0]["type"].as_str().or_else(|| err["type"].as_str());
    match (status.as_u16(), kind, reason) {
        (401, _, _) => format!("{} refused the login: check the user and password, or the API key", cfg.label()),
        (403, _, Some(r)) => format!("Not allowed: {r}"),
        (404, Some("index_not_found_exception"), Some(r)) => r.to_string(),
        (409, _, _) => "Version conflict: the document changed since it was loaded. Reload and try again.".into(),
        (_, Some(k), Some(r)) => format!("{k}: {r}"),
        (_, None, Some(r)) => r.to_string(),
        _ => format!("{} returned HTTP {status}", cfg.label()),
    }
}

/// One REST call; `path` is relative to the server, query string included.
pub async fn request(cfg: &EsConfig, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
    check_path(method, path)?;
    send(cfg, method, path, body.map(Body::Json)).await
}

enum Body<'a> {
    Json(&'a Value),
    /// `_bulk` / `_msearch`: newline-delimited JSON.
    Ndjson(String),
}

tokio::task_local! {
    /// The console run's id, sent as X-Opaque-Id so Stop can find its tasks.
    static OPAQUE_ID: String;
}

/// `query`, stoppable on the server. Every request the statement makes
/// carries an X-Opaque-Id; Stop drops the request and cancels the tasks that
/// carry that id. Searches already end when their connection closes, but
/// `_sql`, `_delete_by_query`, `_update_by_query` and the like run on.
pub async fn query_cancellable(cfg: &EsConfig, stmt: &str, stop: &super::stop::Stop) -> Result<super::query::SqlResult, String> {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = format!("stroke-{:x}-{:x}", std::process::id(), SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    // Boxed, so `drop(run)` below really ends the future and lets go of
    // what it holds (a connection, a stream); a stack pin outlived it.
    let mut run = Box::pin(OPAQUE_ID.scope(id.clone(), query(cfg, stmt)));
    tokio::select! {
        r = &mut run => r,
        _ = stop.wait() => {
            drop(run);
            let _ = tokio::time::timeout(Duration::from_secs(10), cancel_tasks(cfg, &id)).await;
            Err(super::query::QUERY_CANCELLED.into())
        }
    }
}

/// Cancel every task whose X-Opaque-Id is `id`. Returns how many.
pub async fn cancel_tasks(cfg: &EsConfig, id: &str) -> Result<usize, String> {
    let v = send(cfg, "GET", "_tasks?detailed=true&group_by=none", None).await?;
    let ids: Vec<String> = v["tasks"]
        .as_array()
        .map(|t| {
            t.iter()
                .filter(|t| t["headers"]["X-Opaque-Id"].as_str() == Some(id) && t["cancellable"].as_bool() != Some(false))
                .filter_map(|t| Some(format!("{}:{}", t["node"].as_str()?, t["id"].as_i64()?)))
                .collect()
        })
        .unwrap_or_default();
    for t in &ids {
        let _ = send(cfg, "POST", &format!("_tasks/{t}/_cancel"), None).await;
    }
    Ok(ids.len())
}

async fn send(cfg: &EsConfig, method: &str, path: &str, body: Option<Body<'_>>) -> Result<Value, String> {
    let url = base(cfg)?.join(path.trim_start_matches('/')).map_err(|e| format!("Invalid path \"{path}\": {e}"))?;
    let m = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
    let host = url.host_str().unwrap_or_default().to_string();
    let mut req = super::ssh_tunnel::http_client(&host, cfg.via, cfg.insecure)
        .unwrap_or_else(|| client(cfg.insecure).clone())
        .request(m, url);
    if let Some(k) = cfg.api_key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        req = req.header("Authorization", format!("ApiKey {k}"));
    } else if let Some(u) = cfg.user.as_deref().filter(|u| !u.is_empty()) {
        req = req.basic_auth(u, cfg.password.as_deref());
    }
    if let Ok(id) = OPAQUE_ID.try_with(Clone::clone) {
        req = req.header("X-Opaque-Id", id);
    }
    match body {
        Some(Body::Json(b)) => req = req.json(b),
        Some(Body::Ndjson(s)) => req = req.header("Content-Type", "application/x-ndjson").body(s),
        None => {}
    }
    let resp = req.send().await.map_err(|e| {
        if e.is_connect() {
            let mut msg = format!("Could not reach {} at {}", cfg.label(), base(cfg).map(|u| u.to_string()).unwrap_or_default());
            // A local OpenSearch serves HTTPS with a demo certificate.
            if format!("{e:?}").contains("certificate") {
                msg.push_str(": the server's certificate isn't trusted. Turn on \"Accept self-signed certificate\" for a local server.");
            } else {
                msg.push_str(&format!(": {e}"));
            }
            msg
        } else if e.is_timeout() {
            // A red cluster (no shard can take the write, often a full disk)
            // holds requests until they time out.
            format!("{} didn't answer within 60 seconds. Check the cluster's health (GET _cluster/health): a red cluster holds writes and searches.", cfg.label())
        } else {
            e.to_string()
        }
    })?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::String(text));
    if !status.is_success() {
        return Err(error_text(cfg, status, &v));
    }
    Ok(v)
}

/// A search's plan: the statement run once with `"profile": true`, and the
/// profile drawn as the plan view's tree (search, then each shard, then its
/// Lucene queries and aggregations with their times). Only searches have one.
pub async fn explain_plan(cfg: &EsConfig, stmt: &str) -> Result<Value, String> {
    let (index, mut body) = match parse(stmt)? {
        Statement::Search { index, body } => (index, body),
        Statement::Count { index, body } => (index, json!({ "query": body.get("query").cloned().unwrap_or(json!({"match_all": {}})), "size": 0 })),
        Statement::Raw { method, path, body } if (method == "GET" || method == "POST") && path.trim_end_matches('/').ends_with("_search") => {
            let index = path.trim_start_matches('/').split('/').next().unwrap_or("").to_string();
            (index, body.unwrap_or(json!({})))
        }
        _ => return Err("Explain works on searches: GET index/_search, index.search({...}) or index.count({...})".into()),
    };
    if !body.is_object() {
        body = json!({});
    }
    body["profile"] = json!(true);
    let path = if index.is_empty() || index == "_search" { "_search".to_string() } else { format!("{}/_search", enc(&index)) };
    let v = request(cfg, "POST", &path, Some(&body)).await?;
    Ok(profile_tree(&v, &index))
}

/// A profiled search response → plan tree. Pure, so it is tested on recorded output.
pub fn profile_tree(v: &Value, index: &str) -> Value {
    fn ms(nanos: &Value) -> Value {
        nanos.as_f64().map(|n| json!((n / 1e6 * 1000.0).round() / 1000.0)).unwrap_or(Value::Null)
    }
    fn part(q: &Value) -> Value {
        let mut n = json!({
            "Node Type": q["type"].as_str().unwrap_or("Query"),
            "Filter": q["description"].as_str().unwrap_or(""),
            "Actual Total Time": ms(&q["time_in_nanos"]),
        });
        if let Some(kids) = q["children"].as_array().filter(|k| !k.is_empty()) {
            n["Plans"] = Value::Array(kids.iter().map(part).collect());
        }
        n
    }
    let shards: Vec<Value> = v["profile"]["shards"]
        .as_array()
        .map(|sh| {
            sh.iter()
                .map(|shard| {
                    let mut plans: Vec<Value> = shard["searches"]
                        .as_array()
                        .map(|ss| ss.iter().flat_map(|s| s["query"].as_array().cloned().unwrap_or_default()).map(|q| part(&q)).collect())
                        .unwrap_or_default();
                    plans.extend(shard["aggregations"].as_array().map(|a| a.iter().map(part).collect::<Vec<_>>()).unwrap_or_default());
                    let total: f64 = plans.iter().filter_map(|p| p["Actual Total Time"].as_f64()).sum();
                    let id = shard["id"].as_str().unwrap_or("");
                    // `[node][index][shard]`: the last two say which.
                    let label = id.split(']').filter(|p| !p.is_empty()).map(|p| p.trim_start_matches('[')).skip(1).collect::<Vec<_>>().join(" shard ");
                    json!({
                        "Node Type": "Shard",
                        "Relation Name": if label.is_empty() { id.to_string() } else { label },
                        "Actual Total Time": (total * 1000.0).round() / 1000.0,
                        "Plans": plans,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let hits = v["hits"]["total"]["value"].as_i64().or_else(|| v["hits"]["total"].as_i64());
    json!({
        "Node Type": "Search",
        "Relation Name": index,
        "Actual Rows": hits,
        "Actual Total Time": v["took"].as_f64(),
        "Plans": shards,
    })
}

/// What `GET /` says about the server.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub flavor: String,
    pub version: String,
    pub cluster: String,
}

pub fn server_info(v: &Value) -> ServerInfo {
    let flavor = if v["version"]["distribution"].as_str() == Some("opensearch") { "opensearch" } else { "elasticsearch" };
    ServerInfo {
        flavor: flavor.into(),
        version: v["version"]["number"].as_str().unwrap_or("").into(),
        cluster: v["cluster_name"].as_str().unwrap_or("").into(),
    }
}

/// Connection test: the server answers, the login works, and which engine it is.
pub async fn ping(cfg: &EsConfig) -> Result<ServerInfo, String> {
    let v = request(cfg, "GET", "", None).await?;
    if !v.is_object() || v.get("version").is_none() {
        return Err(format!("{} didn't answer at {}: is this the HTTP port (usually 9200)?", cfg.label(), cfg.url));
    }
    Ok(server_info(&v))
}

/// Connect: ping, then keep the flavor the server reported.
pub async fn connect(mut cfg: EsConfig) -> Result<EsConfig, String> {
    let info = ping(&cfg).await?;
    cfg.flavor = Some(info.flavor);
    Ok(cfg)
}

// ── Catalog ──────────────────────────────────────────────────────────────────

/// One index from `_cat/indices`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexRow {
    pub name: String,
    pub docs: i64,
    pub size: String,
    pub health: String,
}

pub fn index_rows(v: &Value) -> Vec<IndexRow> {
    let mut out: Vec<IndexRow> = v
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let name = r["index"].as_str()?.to_string();
            // Dot-prefixed indices are the server's own (security, Kibana,
            // task results) and data stream backing indices.
            if name.starts_with('.') {
                return None;
            }
            let docs = r["docs.count"].as_str().and_then(|s| s.parse().ok()).or_else(|| r["docs.count"].as_i64()).unwrap_or(-1);
            Some(IndexRow {
                name,
                docs,
                size: r["store.size"].as_str().unwrap_or("").into(),
                health: r["health"].as_str().unwrap_or("").into(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub async fn list_indices(cfg: &EsConfig) -> Result<Vec<IndexRow>, String> {
    let v = request(cfg, "GET", "_cat/indices?format=json&h=index,docs.count,store.size,health&s=index", None).await?;
    Ok(index_rows(&v))
}

/// Field paths and types from an index mapping, nested objects flattened to
/// dotted paths (`user.name`), multi-fields (`title.keyword`) included.
pub fn mapping_fields(mapping: &Value) -> Vec<(String, String)> {
    fn walk(props: &Value, prefix: &str, out: &mut Vec<(String, String)>) {
        let Some(map) = props.as_object() else { return };
        for (name, def) in map {
            let path = if prefix.is_empty() { name.clone() } else { format!("{prefix}.{name}") };
            let ty = def["type"].as_str().unwrap_or(if def.get("properties").is_some() { "object" } else { "" });
            if !ty.is_empty() {
                out.push((path.clone(), ty.to_string()));
            }
            if let Some(p) = def.get("properties") {
                walk(p, &path, out);
            }
            if let Some(f) = def.get("fields") {
                walk(f, &path, out);
            }
        }
    }
    let mut out = vec![];
    // `{index: {mappings: {properties: ...}}}`; one entry per concrete index
    // when the name is an alias or pattern. The first one's fields are used.
    if let Some((_, idx)) = mapping.as_object().and_then(|m| m.iter().next()) {
        walk(&idx["mappings"]["properties"], "", &mut out);
    }
    out
}

pub async fn index_columns(cfg: &EsConfig, index: &str) -> Result<Vec<super::schema::ColumnStructureRow>, String> {
    let v = request(cfg, "GET", &format!("{}/_mapping", enc(index)), None).await?;
    let mut rows = vec![super::schema::ColumnStructureRow {
        ordinal_position: 1,
        name: "_id".into(),
        data_type: "keyword".into(),
        is_nullable: false,
        column_default: None,
        foreign_key: None,
        fk_constraint_name: None,
        comment: None,
        is_primary_key: true,
    }];
    for (i, (name, ty)) in mapping_fields(&v).into_iter().enumerate() {
        rows.push(super::schema::ColumnStructureRow {
            ordinal_position: i as i32 + 2,
            name,
            data_type: ty,
            is_nullable: true,
            column_default: None,
            foreign_key: None,
            fk_constraint_name: None,
            comment: None,
            is_primary_key: false,
        });
    }
    Ok(rows)
}

/// Percent-encode one path segment (ids may hold `/`, `?`, `#`). Commas and
/// `*` stay, since index lists and patterns use them.
pub fn enc(s: &str) -> String {
    urlencoding::encode(s).replace("%2C", ",").replace("%2A", "*")
}

// ── Console ──────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum Statement {
    /// `METHOD path` plus an optional JSON body (Kibana Dev Tools form).
    Raw { method: String, path: String, body: Option<Value> },
    Search { index: String, body: Value },
    Get { index: String, id: String },
    Count { index: String, body: Value },
    Sql(String),
}

const USAGE: &str = "Use GET index/_search with a JSON body, index.search({...}), index.get(\"id\"), index.count({...}) or SQL";

pub fn parse(stmt: &str) -> Result<Statement, String> {
    let s = stmt.trim().trim_end_matches(';').trim();
    if s.is_empty() {
        return Err(USAGE.into());
    }
    let first_word = s.split_whitespace().next().unwrap_or("").to_ascii_uppercase();
    if matches!(first_word.as_str(), "SELECT" | "SHOW" | "DESCRIBE" | "DESC" | "WITH") {
        return Ok(Statement::Sql(s.to_string()));
    }
    if matches!(first_word.as_str(), "GET" | "POST" | "PUT" | "DELETE" | "HEAD") {
        let (line, rest) = s.split_once('\n').unwrap_or((s, ""));
        let mut parts = line.split_whitespace();
        let method = parts.next().unwrap_or("").to_ascii_uppercase();
        let path = parts.next().unwrap_or("").trim_start_matches('/').to_string();
        let rest = rest.trim();
        let body = if rest.is_empty() {
            None
        } else {
            Some(serde_json::from_str(rest).map_err(|e| format!("The request body isn't valid JSON: {e}"))?)
        };
        return Ok(Statement::Raw { method, path, body });
    }
    let open = s.find('(').ok_or(USAGE)?;
    if !s.ends_with(')') {
        return Err(USAGE.into());
    }
    let (index, method) = s[..open].rsplit_once('.').ok_or(USAGE)?;
    let index = index.trim().trim_matches('"').to_string();
    if index.is_empty() {
        return Err(USAGE.into());
    }
    let args_src = s[open + 1..s.len() - 1].trim();
    let args: Vec<Value> = if args_src.is_empty() {
        vec![]
    } else {
        serde_json::from_str(&format!("[{args_src}]")).map_err(|e| format!("Arguments aren't valid JSON: {e}"))?
    };
    let body = || -> Result<Value, String> {
        let b = args.first().cloned().unwrap_or_else(|| json!({}));
        if b.is_object() { Ok(b) } else { Err(format!("{}() takes a request body object, e.g. {{\"query\": {{\"match_all\": {{}}}}}}", method.trim())) }
    };
    match method.trim() {
        "search" | "find" => Ok(Statement::Search { index, body: body()? }),
        "count" => Ok(Statement::Count { index, body: body()? }),
        "get" => match args.first() {
            Some(Value::String(id)) => Ok(Statement::Get { index, id: id.clone() }),
            _ => Err("get() takes a document id string".into()),
        },
        m => Err(format!("Unknown method \"{m}\". {USAGE}")),
    }
}

/// Default page of hits when the body doesn't say.
const DEFAULT_SIZE: u64 = 100;

/// A search hit as a grid row: `_id` and `_index`, then the source fields.
fn hit_doc(h: &Value, many_indices: bool) -> Value {
    let mut doc = serde_json::Map::new();
    doc.insert("_id".into(), h["_id"].clone());
    if many_indices {
        doc.insert("_index".into(), h["_index"].clone());
    }
    if let Some(src) = h["_source"].as_object() {
        for (k, v) in src {
            doc.insert(k.clone(), v.clone());
        }
    }
    if let Some(f) = h["fields"].as_object() {
        for (k, v) in f {
            // `fields` values are arrays; one value reads better unwrapped.
            let v = match v.as_array() {
                Some(a) if a.len() == 1 => a[0].clone(),
                _ => v.clone(),
            };
            doc.entry(k.clone()).or_insert(v);
        }
    }
    Value::Object(doc)
}

/// Aggregation buckets as rows: one row per bucket of each top-level
/// bucket aggregation, single values (avg, sum, cardinality) as one row.
fn agg_rows(aggs: &Value) -> Vec<Value> {
    let mut rows = vec![];
    for (name, a) in aggs.as_object().into_iter().flatten() {
        if let Some(buckets) = a["buckets"].as_array() {
            for b in buckets {
                let mut row = serde_json::Map::new();
                row.insert("aggregation".into(), json!(name));
                row.insert("key".into(), b.get("key_as_string").cloned().unwrap_or_else(|| b["key"].clone()));
                row.insert("doc_count".into(), b["doc_count"].clone());
                // Sub-aggregations with a single value become their own columns.
                for (k, v) in b.as_object().into_iter().flatten() {
                    if let Some(x) = v.get("value") {
                        row.insert(k.clone(), x.clone());
                    }
                }
                rows.push(Value::Object(row));
            }
        } else if let Some(x) = a.get("value") {
            rows.push(json!({ "aggregation": name, "value": x }));
        } else {
            rows.push(json!({ "aggregation": name, "result": a }));
        }
    }
    rows
}

/// A `_search` response as grid documents and a summary line.
pub fn search_docs(out: &Value) -> (Vec<Value>, Option<String>) {
    let hits = out["hits"]["hits"].as_array().cloned().unwrap_or_default();
    let total = out["hits"]["total"]["value"].as_u64().or_else(|| out["hits"]["total"].as_u64());
    let exact = out["hits"]["total"]["relation"].as_str() != Some("gte");
    let many = {
        let first = hits.first().and_then(|h| h["_index"].as_str());
        hits.iter().any(|h| h["_index"].as_str() != first)
    };
    if hits.is_empty() {
        if let Some(aggs) = out.get("aggregations") {
            let rows = agg_rows(aggs);
            let msg = total.map(|t| format!("{t}{} matching documents", if exact { "" } else { "+" }));
            return (rows, msg);
        }
    }
    let docs: Vec<Value> = hits.iter().map(|h| hit_doc(h, many)).collect();
    let msg = match total {
        Some(t) if t as usize > docs.len() => Some(format!("{} of {t}{} hits", docs.len(), if exact { "" } else { "+" })),
        _ => None,
    };
    (docs, msg)
}

/// A SQL response as a result grid. Elasticsearch answers `{columns, rows}`;
/// OpenSearch's SQL plugin `{schema, datarows}`.
pub fn sql_result(v: &Value, stmt: &str, t0: Instant) -> super::query::SqlResult {
    use super::query::{ColumnInfo, SqlResult};
    let cols = v["columns"].as_array().or_else(|| v["schema"].as_array()).cloned().unwrap_or_default();
    let rows: Vec<Vec<Value>> = v["rows"]
        .as_array()
        .or_else(|| v["datarows"].as_array())
        .into_iter()
        .flatten()
        .map(|r| r.as_array().cloned().unwrap_or_default())
        .collect();
    let message = v["cursor"].as_str().filter(|c| !c.is_empty()).map(|_| format!("Showing the first {} rows", rows.len()));
    SqlResult {
        columns: cols
            .iter()
            .map(|c| ColumnInfo::new(c["name"].as_str().or_else(|| c["alias"].as_str()).unwrap_or("").to_string(), c["type"].as_str().unwrap_or("")))
            .collect(),
        row_count: Some(rows.len() as i64),
        rows,
        message,
        query_ms: t0.elapsed().as_millis() as u64,
        sql: stmt.to_string(),
    }
}

/// Any JSON answer as grid documents: arrays as rows (`_cat` with
/// `format=json`), a search as hits, anything else as one row.
fn raw_docs(v: &Value) -> (Vec<Value>, Option<String>) {
    if v.get("hits").is_some() {
        return search_docs(v);
    }
    if v.get("_source").is_some() && v.get("_id").is_some() {
        return (vec![hit_doc(v, false)], None);
    }
    match v {
        Value::Array(a) => (a.clone(), None),
        Value::Object(_) => (vec![v.clone()], None),
        other => (vec![json!({ "response": other })], None),
    }
}

/// Run one console statement I typed: reads, document writes, and creating
/// an index or its mapping. A read-only connection is stopped by the page
/// before it gets here.
pub async fn query(cfg: &EsConfig, stmt: &str) -> Result<super::query::SqlResult, String> {
    query_as(cfg, stmt, true).await
}

/// Run one statement for an agent. With `allow_write` (the MCP gate already
/// checked the session's level) documents can be written and an index or its
/// mapping created; without it this is the read-only console.
pub async fn query_as(cfg: &EsConfig, stmt: &str, allow_write: bool) -> Result<super::query::SqlResult, String> {
    let t0 = Instant::now();
    let (docs, message) = match parse(stmt)? {
        Statement::Sql(sql) => {
            let (path, body) = if cfg.is_opensearch() {
                ("_plugins/_sql?format=jdbc", json!({ "query": sql }))
            } else {
                ("_sql?format=json", json!({ "query": sql, "fetch_size": 1000 }))
            };
            let v = request(cfg, "POST", path, Some(&body)).await.map_err(|e| {
                if e.contains("no handler") || e.contains("HTTP 404") || e.contains("HTTP 400") && e.contains("_plugins") {
                    format!("{e}. SQL needs the SQL plugin (OpenSearch) or a license that includes SQL (Elasticsearch).")
                } else {
                    e
                }
            })?;
            return Ok(sql_result(&v, stmt, t0));
        }
        Statement::Raw { method, path, body } => {
            if !is_read(&method, &path) {
                if allow_write && agent_may(&method, &path) {
                    let v = send(cfg, &method, &path, body.as_ref().map(Body::Json)).await?;
                    return Ok(super::couchdb::docs_result(vec![v], None, stmt, t0));
                }
                return Err(match check_path(&method, &path) {
                    Err(why) => why,
                    Ok(()) if !allow_write => format!(
                        "Agents only read on this connection. {method} /{path} changes data; allow writes for the agent, or run it in the editor."
                    ),
                    Ok(()) => format!("{method} /{path} isn't available from Stroke."),
                });
            }
            let v = request(cfg, &method, &path, body.as_ref()).await?;
            raw_docs(&v)
        }
        Statement::Search { index, mut body } => {
            if body.get("size").is_none() {
                body["size"] = json!(DEFAULT_SIZE);
            }
            let v = request(cfg, "POST", &format!("{}/_search", enc(&index)), Some(&body)).await?;
            search_docs(&v)
        }
        Statement::Count { index, body } => {
            let v = request(cfg, "POST", &format!("{}/_count", enc(&index)), Some(&body)).await?;
            (vec![json!({ "count": v["count"] })], None)
        }
        Statement::Get { index, id } => {
            let v = request(cfg, "GET", &format!("{}/_doc/{}", enc(&index), urlencoding::encode(&id)), None).await?;
            (vec![hit_doc(&v, false)], None)
        }
    };
    Ok(super::couchdb::docs_result(docs, message, stmt, t0))
}

/// Import a JSON, JSON Lines or CSV file into an index, 500 documents per
/// `_bulk` request. A document's `_id` field becomes its id.
pub async fn import_file(cfg: &EsConfig, index: &str, path: &str, format: &str, infer: bool) -> Result<super::mongo::ImportResult, String> {
    let (name, text) = super::mongo::read_import(path)?;
    let (_, docs) = super::mongo::parse_import(&name, &text, format, infer)?;
    drop(text);
    let mut res = super::mongo::ImportResult { inserted: 0, failed: 0, errors: vec![] };
    for (b, chunk) in docs.chunks(500).enumerate() {
        let mut nd = String::new();
        for d in chunk {
            let id = match d.get("_id") {
                Some(Value::String(s)) => Some(s.clone()),
                Some(Value::Number(n)) => Some(n.to_string()),
                _ => None,
            };
            let mut src = d.clone();
            if let Some(o) = src.as_object_mut() {
                o.remove("_id");
            }
            let action = match id {
                Some(id) => json!({ "index": { "_index": index, "_id": id } }),
                None => json!({ "index": { "_index": index } }),
            };
            nd.push_str(&action.to_string());
            nd.push('\n');
            nd.push_str(&src.to_string());
            nd.push('\n');
        }
        let v = send(cfg, "POST", "_bulk?refresh=wait_for", Some(Body::Ndjson(nd)))
            .await
            .map_err(|e| format!("Import stopped after {} documents: {e}", res.inserted))?;
        for (i, item) in v["items"].as_array().into_iter().flatten().enumerate() {
            let r = &item["index"];
            if r.get("error").is_some() {
                res.failed += 1;
                if res.errors.len() < 5 {
                    let reason = r["error"]["reason"].as_str().unwrap_or("failed");
                    res.errors.push(format!("Document {}: {reason}", b * 500 + i + 1));
                }
            } else {
                res.inserted += 1;
            }
        }
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_allowlist() {
        assert!(check_path("GET", "").is_ok());
        assert!(check_path("GET", "_cat/indices?format=json").is_ok());
        assert!(check_path("POST", "logs/_search").is_ok());
        assert!(check_path("POST", "logs-*,metrics/_search?size=5").is_ok());
        assert!(check_path("POST", "_sql?format=json").is_ok());
        assert!(check_path("POST", "_plugins/_sql").is_ok());
        assert!(check_path("GET", "logs/_mapping").is_ok());
        assert!(check_path("PUT", "logs/_doc/a%2Fb?refresh=wait_for").is_ok());
        assert!(check_path("POST", "logs/_doc").is_ok());
        assert!(check_path("POST", "logs/_update/1").is_ok());
        assert!(check_path("DELETE", "logs/_doc/1").is_ok());
        assert!(check_path("POST", "_bulk").is_ok());

        assert!(check_path("DELETE", "logs").is_err());
        assert!(check_path("PUT", "logs").is_err());
        assert!(check_path("POST", "logs/_delete_by_query").is_err());
        assert!(check_path("POST", "logs/_mapping").is_err());
        assert!(check_path("POST", "_aliases").is_err());
        assert!(check_path("PUT", "_cluster/settings").is_err());
        assert!(check_path("POST", "_snapshot/repo").is_err());
        assert!(check_path("PUT", "_security/user/x").is_err());
        assert!(check_path("GET", "_plugins/_security/api/internalusers").is_err());
        assert!(check_path("DELETE", "logs/_doc").is_err());
        assert!(check_path("PATCH", "logs/_doc/1").is_err());
        assert!(check_path("GET", "logs/../_security").is_err());
    }

    #[test]
    fn reads_and_writes() {
        assert!(is_read("GET", "logs/_doc/1"));
        assert!(is_read("POST", "logs/_search"));
        assert!(is_read("POST", "_plugins/_sql"));
        assert!(!is_read("POST", "logs/_doc"));
        assert!(!is_read("PUT", "logs/_doc/1"));
        assert!(!is_read("POST", "_bulk"));
        assert!(!is_read("DELETE", "logs/_doc/1"));
    }

    #[test]
    fn parses_console_statements() {
        assert_eq!(
            parse("GET logs/_search\n{\"query\": {\"match_all\": {}}}").unwrap(),
            Statement::Raw { method: "GET".into(), path: "logs/_search".into(), body: Some(json!({"query": {"match_all": {}}})) }
        );
        assert_eq!(
            parse("get /_cat/indices?v").unwrap(),
            Statement::Raw { method: "GET".into(), path: "_cat/indices?v".into(), body: None }
        );
        assert_eq!(
            parse(r#"logs.search({"query": {"term": {"level": "error"}}})"#).unwrap(),
            Statement::Search { index: "logs".into(), body: json!({"query": {"term": {"level": "error"}}}) }
        );
        assert_eq!(parse("logs-2026.11.search()").unwrap(), Statement::Search { index: "logs-2026.11".into(), body: json!({}) });
        assert_eq!(parse(r#"logs.get("a/b")"#).unwrap(), Statement::Get { index: "logs".into(), id: "a/b".into() });
        assert_eq!(parse("logs.count()").unwrap(), Statement::Count { index: "logs".into(), body: json!({}) });
        assert_eq!(parse("select * from logs limit 5;").unwrap(), Statement::Sql("select * from logs limit 5".into()));
        assert!(parse("GET logs/_search\n{bad").unwrap_err().contains("valid JSON"));
        assert!(parse("logs.drop()").unwrap_err().contains("Unknown method"));
        assert!(parse("logs.search([1])").unwrap_err().contains("body object"));
    }

    #[test]
    fn search_hits_become_rows() {
        let out = json!({"hits": {"total": {"value": 42, "relation": "eq"}, "hits": [
            {"_index": "logs", "_id": "1", "_source": {"level": "error", "msg": "boom"}},
            {"_index": "logs", "_id": "2", "_source": {"level": "info"}, "fields": {"host": ["a"]}}
        ]}});
        let (docs, msg) = search_docs(&out);
        assert_eq!(docs[0], json!({"_id": "1", "level": "error", "msg": "boom"}));
        assert_eq!(docs[1], json!({"_id": "2", "level": "info", "host": "a"}));
        assert_eq!(msg.as_deref(), Some("2 of 42 hits"));
        let r = super::super::couchdb::docs_result(docs, msg, "q", Instant::now());
        assert_eq!(r.columns[0].name, "_id");
    }

    #[test]
    fn aggregations_become_rows() {
        let out = json!({"hits": {"total": {"value": 10000, "relation": "gte"}, "hits": []}, "aggregations": {
            "by_level": {"buckets": [{"key": "error", "doc_count": 3, "avg_ms": {"value": 12.5}}, {"key": "info", "doc_count": 9, "avg_ms": {"value": 2.0}}]},
            "total_ms": {"value": 99.0}
        }});
        let (rows, msg) = search_docs(&out);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], json!({"aggregation": "by_level", "key": "error", "doc_count": 3, "avg_ms": 12.5}));
        assert_eq!(rows[2], json!({"aggregation": "total_ms", "value": 99.0}));
        assert_eq!(msg.as_deref(), Some("10000+ matching documents"));
    }

    #[test]
    fn sql_answers_from_both_engines() {
        let es = json!({"columns": [{"name": "level", "type": "keyword"}, {"name": "n", "type": "long"}], "rows": [["error", 3]]});
        let r = sql_result(&es, "q", Instant::now());
        assert_eq!(r.columns[1].name, "n");
        assert_eq!(r.rows, vec![vec![json!("error"), json!(3)]]);
        let os = json!({"schema": [{"name": "level", "type": "keyword"}], "datarows": [["info"]], "total": 1, "size": 1, "status": 200});
        let r = sql_result(&os, "q", Instant::now());
        assert_eq!(r.columns[0].name, "level");
        assert_eq!(r.rows.len(), 1);
    }

    #[test]
    fn mapping_flattens_nested_and_multi_fields() {
        let m = json!({"logs": {"mappings": {"properties": {
            "msg": {"type": "text", "fields": {"keyword": {"type": "keyword"}}},
            "user": {"properties": {"name": {"type": "keyword"}, "age": {"type": "integer"}}},
            "@timestamp": {"type": "date"}
        }}}});
        let f = mapping_fields(&m);
        assert!(f.contains(&("msg".into(), "text".into())));
        assert!(f.contains(&("msg.keyword".into(), "keyword".into())));
        assert!(f.contains(&("user".into(), "object".into())));
        assert!(f.contains(&("user.age".into(), "integer".into())));
        assert!(f.contains(&("@timestamp".into(), "date".into())));
    }

    #[test]
    fn indices_skip_system_ones() {
        let v = json!([
            {"index": ".kibana_1", "docs.count": "5", "store.size": "1kb", "health": "green"},
            {"index": "logs", "docs.count": "1200", "store.size": "2mb", "health": "yellow"}
        ]);
        let rows = index_rows(&v);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "logs");
        assert_eq!(rows[0].docs, 1200);
    }

    #[test]
    fn detects_the_engine() {
        let os = json!({"cluster_name": "c", "version": {"distribution": "opensearch", "number": "2.17.0"}});
        assert_eq!(server_info(&os).flavor, "opensearch");
        let es = json!({"cluster_name": "c", "version": {"number": "8.15.0", "build_flavor": "default"}});
        assert_eq!(server_info(&es).flavor, "elasticsearch");
        assert_eq!(server_info(&es).version, "8.15.0");
    }

    #[test]
    fn encodes_ids_but_keeps_patterns() {
        assert_eq!(enc("logs-*,metrics"), "logs-*,metrics");
        assert_eq!(urlencoding::encode("a/b"), "a%2Fb");
    }

    /// Stop on a slow search: while it runs the server lists a task with our
    /// X-Opaque-Id, and after Stop none is left.
    //   STROKE_TEST_ES=http://127.0.0.1:19200 cargo test --lib elastic::tests::live_stop
    #[tokio::test]
    async fn live_stop_cancels_the_task() {
        let Ok(url) = std::env::var("STROKE_TEST_ES") else { return };
        let cfg = EsConfig { ssh: None, via: None, name: "t".into(), url, user: None, password: None, api_key: None, flavor: None, insecure: true };
        let cfg = connect(cfg).await.unwrap();
        let _ = send(&cfg, "DELETE", "stroke_slow", None).await;
        let mut nd = String::new();
        for i in 0..20_000 {
            nd.push_str(&format!("{{\"index\":{{\"_index\":\"stroke_slow\"}}}}\n{{\"n\":{i}}}\n"));
        }
        send(&cfg, "POST", "_bulk?refresh=true", Some(Body::Ndjson(nd))).await.unwrap();
        let ours = |v: &Value| -> usize {
            v["tasks"].as_array().map(|t| t.iter().filter(|t| t["headers"]["X-Opaque-Id"].as_str().is_some_and(|h| h.starts_with("stroke-"))).count()).unwrap_or(0)
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        let stop = super::super::stop::Stop::from_rx(rx);
        let probe = cfg.clone();
        let during = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            let v = send(&probe, "GET", "_tasks?detailed=true&group_by=none", None).await.unwrap();
            let n = ours(&v);
            let _ = tx.send(());
            n
        });
        let slow = "GET stroke_slow/_search\n{\"query\": {\"script\": {\"script\": \"long x = 0; for (int i = 0; i < 900000; i++) { x += i; } return x > 0;\"}}}";
        let t0 = Instant::now();
        let e = query_cancellable(&cfg, slow, &stop).await.unwrap_err();
        assert_eq!(e, super::super::query::QUERY_CANCELLED);
        assert!(during.await.unwrap() > 0, "the search carried our X-Opaque-Id");
        assert!(t0.elapsed() < Duration::from_secs(20));
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let v = send(&cfg, "GET", "_tasks?detailed=true&group_by=none", None).await.unwrap();
        assert_eq!(ours(&v), 0, "a task is still running after Stop");
        let _ = send(&cfg, "DELETE", "stroke_slow", None).await;
    }

    /// Against a local server: `cargo test --lib es_live -- --ignored`, with
    /// ES_URL (default http://localhost:9200).
    #[tokio::test]
    #[ignore]
    async fn es_live() {
        let url = std::env::var("ES_URL").unwrap_or_else(|_| "http://localhost:9200".into());
        let cfg = EsConfig { ssh: None, via: None, name: "t".into(), url, user: None, password: None, api_key: None, flavor: None, insecure: true };
        let cfg = connect(cfg).await.unwrap();
        request(&cfg, "PUT", "stroke_it/_doc/1?refresh=wait_for", Some(&json!({"level": "error", "ms": 12}))).await.unwrap();
        request(&cfg, "PUT", "stroke_it/_doc/2?refresh=wait_for", Some(&json!({"level": "info", "ms": 3}))).await.unwrap();
        let names: Vec<String> = list_indices(&cfg).await.unwrap().into_iter().map(|i| i.name).collect();
        assert!(names.contains(&"stroke_it".to_string()), "{names:?}");
        let r = query(&cfg, r#"stroke_it.search({"query": {"term": {"level": "error"}}})"#).await.unwrap();
        assert_eq!(r.rows.len(), 1);
        let r = query(&cfg, "GET stroke_it/_search\n{\"size\": 0, \"aggs\": {\"l\": {\"terms\": {\"field\": \"level.keyword\"}}}}").await.unwrap();
        assert_eq!(r.rows.len(), 2);
        let r = query(&cfg, "SELECT level, ms FROM stroke_it ORDER BY ms").await.unwrap();
        assert_eq!(r.rows.len(), 2, "{:?}", r.rows);
        let cols = index_columns(&cfg, "stroke_it").await.unwrap();
        assert!(cols.iter().any(|c| c.name == "level.keyword"));
        assert!(query(&cfg, "DELETE stroke_it/_doc/1").await.unwrap_err().contains("only reads"));
        request(&cfg, "DELETE", "stroke_it/_doc/1", None).await.unwrap();
        request(&cfg, "DELETE", "stroke_it/_doc/2", None).await.unwrap();
    }
}
