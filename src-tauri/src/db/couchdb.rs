//! CouchDB over its HTTP API. Documents are plain JSON with `_id`/`_rev`.
//!
//! The query console takes a small shell-like syntax, since Mango has no text
//! form of its own:
//!   products.find({"price": {"$gt": 50}})            Mango selector
//!   products.find({"type": "product"}, {"limit": 10, "sort": [{"price": "asc"}]})
//!   products.get("p-001")                           one document
//!   products.all()                                  every document (capped)

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::OnceLock;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CouchConfig {
    pub name: String,
    /// Base URL, e.g. `http://localhost:5984`.
    pub url: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default()
    })
}

fn base(cfg: &CouchConfig) -> Result<reqwest::Url, String> {
    let raw = cfg.url.trim().trim_end_matches('/');
    let raw = if raw.contains("://") { raw.to_string() } else { format!("http://{raw}") };
    reqwest::Url::parse(&format!("{raw}/")).map_err(|e| format!("Invalid CouchDB URL \"{raw}\": {e}"))
}

/// Requests the app may make. Server administration (`/_config`, `/_node`,
/// users, replication) is refused, and so is deleting a whole database.
pub fn check_path(method: &str, path: &str) -> Result<(), String> {
    let p = path.trim_start_matches('/');
    let first = p.split(['/', '?']).next().unwrap_or("");
    let ok_method = matches!(method, "GET" | "POST" | "PUT" | "DELETE");
    if !ok_method {
        return Err(format!("HTTP method {method} is not allowed"));
    }
    if first.is_empty() {
        return if method == "GET" { Ok(()) } else { Err("Not allowed on the server root".into()) };
    }
    if first.starts_with('_') {
        return if method == "GET" && matches!(first, "_all_dbs" | "_up") {
            Ok(())
        } else {
            Err(format!("/{first} is not available from Stroke"))
        };
    }
    let is_db_root = !p.trim_end_matches('/').contains('/');
    if is_db_root && method != "GET" {
        return Err("Creating or deleting databases is not available from Stroke".into());
    }
    if p.contains("..") {
        return Err("Invalid path".into());
    }
    Ok(())
}

/// One CouchDB call; `path` is relative to the server, query string included.
pub async fn request(cfg: &CouchConfig, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
    check_path(method, path)?;
    let url = base(cfg)?
        .join(path.trim_start_matches('/'))
        .map_err(|e| format!("Invalid path \"{path}\": {e}"))?;
    let m = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
    let mut req = client().request(m, url.clone());
    if let Some(u) = cfg.user.as_deref().filter(|u| !u.is_empty()) {
        req = req.basic_auth(u, cfg.password.as_deref());
    }
    if let Some(b) = body {
        req = req.json(b);
    }
    let resp = req.send().await.map_err(|e| {
        if e.is_connect() {
            format!("Could not reach CouchDB at {}: {e}", base(cfg).map(|u| u.to_string()).unwrap_or_default())
        } else {
            e.to_string()
        }
    })?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::String(text.clone()));
    if !status.is_success() {
        let err = v.get("error").and_then(|e| e.as_str());
        let reason = v.get("reason").and_then(|e| e.as_str());
        return Err(match (err, reason) {
            (Some("conflict"), _) => "Document update conflict: it changed since it was loaded. Reload and try again.".into(),
            (Some("unauthorized"), r) => format!("CouchDB refused the login: {}", r.unwrap_or("check the user and password")),
            (Some(e), Some(r)) => format!("{e}: {r}"),
            (Some(e), None) => e.to_string(),
            _ => format!("CouchDB returned HTTP {status}"),
        });
    }
    Ok(v)
}

/// Connection test: the server answers and the login works.
pub async fn ping(cfg: &CouchConfig) -> Result<(), String> {
    request(cfg, "GET", "_all_dbs?limit=1", None).await.map(|_| ())
}

pub async fn list_databases(cfg: &CouchConfig) -> Result<Vec<String>, String> {
    let v = request(cfg, "GET", "_all_dbs", None).await?;
    Ok(v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| d.as_str())
        // System databases hold users and replication jobs, not app data.
        .filter(|d| !d.starts_with('_'))
        .map(str::to_string)
        .collect())
}

// ── Console ──────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum Statement {
    Find { db: String, selector: Value, opts: Value },
    Get { db: String, id: String },
    All { db: String },
}

/// Parse `db.find({...}, {...})`, `db.get("id")` or `db.all()`.
pub fn parse(stmt: &str) -> Result<Statement, String> {
    let s = stmt.trim().trim_end_matches(';').trim();
    let usage = "Use db.find({selector}), db.find({selector}, {\"limit\": 50}), db.get(\"id\") or db.all()";
    let open = s.find('(').ok_or(usage)?;
    if !s.ends_with(')') {
        return Err(usage.into());
    }
    let head = &s[..open];
    let (db, method) = head.rsplit_once('.').ok_or(usage)?;
    let db = db.trim().trim_matches('"').to_string();
    if db.is_empty() {
        return Err(usage.into());
    }
    let args_src = s[open + 1..s.len() - 1].trim();
    // The arguments are JSON values separated by a comma: parse them as an array.
    let args: Vec<Value> = if args_src.is_empty() {
        vec![]
    } else {
        serde_json::from_str(&format!("[{args_src}]")).map_err(|e| format!("Arguments aren't valid JSON: {e}"))?
    };
    match method.trim() {
        "find" => {
            let selector = args.first().cloned().unwrap_or_else(|| json!({}));
            if !selector.is_object() {
                return Err("find() takes a selector object, e.g. {\"type\": \"product\"}".into());
            }
            let opts = args.get(1).cloned().unwrap_or_else(|| json!({}));
            if !opts.is_object() {
                return Err("find()'s second argument is an options object, e.g. {\"limit\": 10}".into());
            }
            Ok(Statement::Find { db, selector, opts })
        }
        "get" => match args.first() {
            Some(Value::String(id)) => Ok(Statement::Get { db, id: id.clone() }),
            _ => Err("get() takes a document id string".into()),
        },
        "all" => Ok(Statement::All { db }),
        m => Err(format!("Unknown method \"{m}\". {usage}")),
    }
}

const CONSOLE_CAP: usize = 5_000;

/// Percent-encode one path segment or query value (ids may hold `/`, `?`, `#`).
pub fn enc(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

/// Run one console statement.
pub async fn query(cfg: &CouchConfig, stmt: &str) -> Result<super::query::SqlResult, String> {
    let t0 = std::time::Instant::now();
    let parsed = parse(stmt)?;
    let mut message = None;
    let docs: Vec<Value> = match parsed {
        Statement::Find { db, selector, opts } => {
            let mut body = json!({ "selector": selector, "limit": 1000 });
            if let Some(o) = opts.as_object() {
                for (k, v) in o {
                    body[k] = v.clone();
                }
            }
            let out = request(cfg, "POST", &format!("{}/_find", enc(&db)), Some(&body)).await?;
            if let Some(w) = out.get("warning").and_then(|w| w.as_str()) {
                message = Some(w.to_string());
            }
            out["docs"].as_array().cloned().unwrap_or_default()
        }
        Statement::Get { db, id } => vec![request(cfg, "GET", &format!("{}/{}", enc(&db), enc(&id)), None).await?],
        Statement::All { db } => {
            let out = request(cfg, "GET", &format!("{}/_all_docs?include_docs=true&limit={CONSOLE_CAP}", enc(&db)), None).await?;
            let rows = out["rows"].as_array().cloned().unwrap_or_default();
            if out["total_rows"].as_u64().unwrap_or(0) as usize > rows.len() {
                message = Some(format!("Showing the first {} documents", rows.len()));
            }
            rows.into_iter().filter_map(|r| r.get("doc").cloned()).collect()
        }
    };
    Ok(docs_result(docs, message, stmt, t0))
}

/// A result-grid cell: Extended JSON scalars (`{"$oid": ...}`, `{"$date": ...}`,
/// `{"$numberLong": ...}`, `{"$numberDecimal": ...}`) as their plain value, and
/// the type name for the column header.
pub fn grid_cell(v: &Value) -> (Value, &'static str) {
    match v {
        Value::Null => (Value::Null, ""),
        Value::Bool(_) => (v.clone(), "boolean"),
        Value::Number(n) if n.is_i64() || n.is_u64() => (v.clone(), "integer"),
        Value::Number(_) => (v.clone(), "double"),
        Value::String(_) => (v.clone(), "string"),
        Value::Array(_) => (v.clone(), "array"),
        Value::Object(o) if o.len() == 1 => {
            let (k, x) = o.iter().next().unwrap();
            match (k.as_str(), x) {
                ("$oid", Value::String(_)) => (x.clone(), "objectId"),
                ("$date", Value::String(_)) => (x.clone(), "date"),
                ("$date", Value::Object(d)) => (d.get("$numberLong").cloned().unwrap_or(x.clone()), "date"),
                ("$numberLong", Value::String(_)) => (x.clone(), "long"),
                ("$numberDecimal", Value::String(_)) => (x.clone(), "decimal"),
                ("$uuid", Value::String(_)) => (x.clone(), "uuid"),
                _ => (v.clone(), "object"),
            }
        }
        Value::Object(_) => (v.clone(), "object"),
    }
}

/// Documents as a result grid: `_id` first, then every field in first-seen
/// order. A column's type is its values' type, or `mixed`.
pub fn docs_result(docs: Vec<Value>, message: Option<String>, stmt: &str, t0: std::time::Instant) -> super::query::SqlResult {
    use super::query::{ColumnInfo, SqlResult};
    let mut cols: Vec<String> = vec![];
    let mut seen = std::collections::HashSet::new();
    for d in &docs {
        if let Some(o) = d.as_object() {
            for k in o.keys() {
                if seen.insert(k.clone()) {
                    cols.push(k.clone());
                }
            }
        }
    }
    cols.sort_by_key(|c| match c.as_str() {
        "_id" => 0,
        "_rev" => 1,
        _ => 2,
    });
    let mut types: Vec<&'static str> = vec![""; cols.len()];
    let rows: Vec<Vec<Value>> = docs
        .iter()
        .map(|d| {
            cols.iter()
                .enumerate()
                .map(|(i, c)| {
                    let (v, t) = grid_cell(d.get(c).unwrap_or(&Value::Null));
                    if !t.is_empty() {
                        types[i] = match (types[i], t) {
                            ("", _) => t,
                            (a, b) if a == b => t,
                            ("integer", "double") | ("double", "integer") => "double",
                            _ => "mixed",
                        };
                    }
                    v
                })
                .collect()
        })
        .collect();
    SqlResult {
        columns: cols.iter().zip(&types).map(|(c, t)| ColumnInfo::new(c.clone(), if t.is_empty() { "null" } else { *t })).collect(),
        row_count: Some(rows.len() as i64),
        rows,
        message,
        query_ms: t0.elapsed().as_millis() as u64,
        sql: stmt.to_string(),
    }
}

/// A database's Mango indexes (`GET /{db}/_index`): the built-in `_all_docs`
/// index and every json/text index, with their fields.
pub async fn index_list(cfg: &CouchConfig, db: &str) -> Result<Vec<super::schema::IndexInfo>, String> {
    let v = request(cfg, "GET", &format!("{}/_index", enc(db)), None).await?;
    Ok(indexes_from_json(db, &v))
}

pub fn indexes_from_json(db: &str, v: &Value) -> Vec<super::schema::IndexInfo> {
    v["indexes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|ix| {
            let name = ix["name"].as_str()?.to_string();
            let kind = ix["type"].as_str().unwrap_or("json").to_string();
            let fields = ix["def"]["fields"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|f| {
                    let (field, dir) = f.as_object()?.iter().next()?;
                    Some(if dir.as_str() == Some("desc") { format!("{field} desc") } else { field.clone() })
                })
                .collect::<Vec<_>>()
                .join(", ");
            Some(super::schema::IndexInfo {
                is_primary: kind == "special",
                is_unique: kind == "special",
                name,
                table_name: db.to_string(),
                columns: fields,
                index_type: kind,
                condition: None,
                comment: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mango_indexes_decode() {
        let v = json!({"total_rows": 2, "indexes": [
            {"ddoc": null, "name": "_all_docs", "type": "special", "def": {"fields": [{"_id": "asc"}]}},
            {"ddoc": "_design/x", "name": "by-type-price", "type": "json", "def": {"fields": [{"type": "asc"}, {"price": "desc"}]}}
        ]});
        let idx = indexes_from_json("shop", &v);
        assert_eq!(idx.len(), 2);
        assert!(idx[0].is_primary && idx[0].columns == "_id" && idx[0].index_type == "special");
        assert_eq!(idx[1].columns, "type, price desc");
        assert_eq!(idx[1].table_name, "shop");
    }

    #[test]
    fn parses_console_statements() {
        assert_eq!(
            parse(r#"products.find({"price": {"$gt": 50}}, {"limit": 5});"#).unwrap(),
            Statement::Find { db: "products".into(), selector: json!({"price": {"$gt": 50}}), opts: json!({"limit": 5}) }
        );
        assert_eq!(parse("products.find()").unwrap(), Statement::Find { db: "products".into(), selector: json!({}), opts: json!({}) });
        assert_eq!(parse(r#"my.db.get("a/b")"#).unwrap(), Statement::Get { db: "my.db".into(), id: "a/b".into() });
        assert_eq!(parse("customers.all()").unwrap(), Statement::All { db: "customers".into() });
        assert!(parse("SELECT * FROM x").is_err());
        assert!(parse("x.find([1])").unwrap_err().contains("selector object"));
        assert!(parse("x.drop()").unwrap_err().contains("Unknown method"));
    }

    #[test]
    fn grid_flattens_extended_json_and_types_columns() {
        let docs = vec![
            json!({"_id": {"$oid": "6af2"}, "n": 1, "at": {"$date": "2026-01-14T10:00:00Z"}, "big": {"$numberLong": "9007199254740993"}}),
            json!({"_id": {"$oid": "6af3"}, "n": 2.5, "tags": ["a"]}),
        ];
        let r = docs_result(docs, None, "q", std::time::Instant::now());
        let cols: Vec<(&str, &str)> = r.columns.iter().map(|c| (c.name.as_str(), c.data_type.as_str())).collect();
        assert_eq!(cols, [("_id", "objectId"), ("n", "double"), ("at", "date"), ("big", "long"), ("tags", "array")]);
        assert_eq!(r.rows[0][0], json!("6af2"));
        assert_eq!(r.rows[0][3], json!("9007199254740993"));
        assert_eq!(r.rows[1][2], Value::Null);
    }

    #[test]
    fn path_allowlist() {
        assert!(check_path("GET", "_all_dbs").is_ok());
        assert!(check_path("GET", "products/_all_docs?include_docs=true").is_ok());
        assert!(check_path("POST", "products/_find").is_ok());
        assert!(check_path("PUT", "products/p-1").is_ok());
        assert!(check_path("DELETE", "products/p-1?rev=1-a").is_ok());
        assert!(check_path("DELETE", "products").is_err());
        assert!(check_path("PUT", "products").is_err());
        assert!(check_path("GET", "_config").is_err());
        assert!(check_path("PUT", "_users/org.couchdb.user:x").is_err());
        assert!(check_path("PATCH", "products/p-1").is_err());
        assert!(check_path("GET", "products/../_config").is_err());
    }

    /// Against `dev/nosql` CouchDB: `cargo test --lib couch_live -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn couch_live() {
        let cfg = CouchConfig { name: "t".into(), url: "http://localhost:5984".into(), user: Some("admin".into()), password: Some("admin".into()) };
        ping(&cfg).await.unwrap();
        let dbs = list_databases(&cfg).await.unwrap();
        assert!(dbs.contains(&"products".to_string()), "{dbs:?}");
        let r = query(&cfg, r#"products.find({"price": {"$gt": 50}}, {"sort": [{"price": "asc"}]})"#).await;
        // Sorting needs an index on price alone; the seeded one is (type, price).
        assert!(r.is_err() || r.as_ref().unwrap().rows.len() == 2);
        let r = query(&cfg, r#"products.find({"price": {"$gt": 50}})"#).await.unwrap();
        assert_eq!(r.rows.len(), 2);
        assert_eq!(r.columns[0].name, "_id");
        let g = query(&cfg, r#"products.get("p-002")"#).await.unwrap();
        assert_eq!(g.rows.len(), 1);
        let bad = CouchConfig { password: Some("nope".into()), ..cfg.clone() };
        assert!(ping(&bad).await.unwrap_err().contains("refused the login"));
        // Write, conflict, delete.
        let put = request(&cfg, "PUT", "products/it-1", Some(&json!({"name": "x"}))).await.unwrap();
        let rev = put["rev"].as_str().unwrap().to_string();
        let err = request(&cfg, "PUT", "products/it-1", Some(&json!({"name": "y"}))).await.unwrap_err();
        assert!(err.contains("conflict"), "{err}");
        request(&cfg, "DELETE", &format!("products/it-1?rev={}", enc(&rev)), None).await.unwrap();
    }
}
