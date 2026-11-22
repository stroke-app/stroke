//! Neo4j over its HTTP API (the transactional endpoint, Neo4j 4.x and 5.x). Beta.
//!
//! The document browser shows node labels as collections and nodes as
//! documents: each with its element id, its labels and its properties. The
//! console runs Cypher. Every value goes in as a query parameter.
//!
//! Access levels for agents read the Cypher itself: MATCH ... RETURN reads,
//! CREATE / MERGE / SET / DELETE / REMOVE write, index or constraint changes
//! alter the schema, and databases, users and privileges are dangerous.

use super::query::{ColumnInfo, SqlResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Neo4jConfig {
    pub name: String,
    /// The HTTP address: http://localhost:7474, or https:// for Aura and TLS.
    pub url: String,
    /// The database; `neo4j` when blank.
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

/// Rows a console statement returns.
const MAX_ROWS: usize = 10_000;

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| crate::proxy::http_builder(crate::proxy::Kind::Database).timeout(Duration::from_secs(120)).connect_timeout(Duration::from_secs(10)).build().unwrap_or_default())
}

fn database(cfg: &Neo4jConfig) -> String {
    cfg.database.as_deref().map(str::trim).filter(|d| !d.is_empty()).unwrap_or("neo4j").to_string()
}

fn base(cfg: &Neo4jConfig) -> String {
    let u = cfg.url.trim().trim_end_matches('/');
    // A bolt:// or neo4j:// address from a driver config: the HTTP API is on 7474.
    if let Some(rest) = u.strip_prefix("bolt://").or_else(|| u.strip_prefix("neo4j://")) {
        let host = rest.split(':').next().unwrap_or(rest);
        return format!("http://{host}:7474");
    }
    if u.contains("://") { u.to_string() } else { format!("http://{u}") }
}

/// Run statements in one transaction; the results, in order.
async fn run(cfg: &Neo4jConfig, statements: Vec<(String, Value)>) -> Result<Vec<Value>, String> {
    let url = format!("{}/db/{}/tx/commit", base(cfg), urlencoding::encode(&database(cfg)));
    let body = json!({
        "statements": statements
            .into_iter()
            .map(|(s, p)| json!({ "statement": s, "parameters": p, "resultDataContents": ["row"], "includeStats": true }))
            .collect::<Vec<_>>(),
    });
    let mut req = client().post(&url).json(&body).header("Accept", "application/json;charset=UTF-8");
    if let Some(u) = cfg.user.as_deref().filter(|u| !u.is_empty()) {
        req = req.basic_auth(u, cfg.password.as_deref());
    }
    let resp = req.send().await.map_err(|e| {
        if e.is_connect() {
            format!("Could not reach Neo4j at {}: is it running, and is the HTTP port (7474) open?", base(cfg))
        } else {
            format!("Neo4j request failed: {e}")
        }
    })?;
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if status == 401 {
        return Err("Neo4j refused the user name or password".into());
    }
    if let Some(err) = v["errors"].as_array().and_then(|e| e.first()) {
        let code = err["code"].as_str().unwrap_or("");
        let msg = err["message"].as_str().unwrap_or("Neo4j error");
        return Err(if code.is_empty() { msg.to_string() } else { format!("{msg} ({})", code.rsplit('.').next().unwrap_or(code)) });
    }
    if status >= 400 {
        return Err(format!("Neo4j answered HTTP {status}: {}", text.chars().take(300).collect::<String>()));
    }
    Ok(v["results"].as_array().cloned().unwrap_or_default())
}

async fn run_one(cfg: &Neo4jConfig, statement: &str, params: Value) -> Result<Value, String> {
    run(cfg, vec![(statement.to_string(), params)]).await?.into_iter().next().ok_or_else(|| "Neo4j returned no result".to_string())
}

pub async fn connect(cfg: &Neo4jConfig) -> Result<Neo4jConfig, String> {
    if cfg.url.trim().is_empty() {
        return Err("Enter the Neo4j address".into());
    }
    run_one(cfg, "RETURN 1", json!({})).await?;
    Ok(cfg.clone())
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "BOOLEAN",
        Value::Number(n) if n.is_i64() || n.is_u64() => "INTEGER",
        Value::Number(_) => "FLOAT",
        Value::String(_) => "STRING",
        Value::Array(_) => "LIST",
        Value::Object(_) => "MAP",
    }
}

/// A Cypher statement from the console, as a table.
pub async fn query(cfg: &Neo4jConfig, stmt: &str) -> Result<SqlResult, String> {
    let t0 = Instant::now();
    let sql = stmt.trim().trim_end_matches(';').trim();
    if sql.is_empty() {
        return Err("Query is empty".into());
    }
    let r = run_one(cfg, sql, json!({})).await?;
    let names: Vec<String> = r["columns"].as_array().into_iter().flatten().filter_map(|c| c.as_str().map(String::from)).collect();
    let data = r["data"].as_array().cloned().unwrap_or_default();
    let total = data.len();
    let rows: Vec<Vec<Value>> = data
        .into_iter()
        .take(MAX_ROWS)
        .map(|d| d["row"].as_array().cloned().unwrap_or_default().into_iter().map(|v| super::sql_util::cap_json_value("text", v)).collect())
        .collect();
    // Types from the first non-null value in each column.
    let columns = names
        .iter()
        .enumerate()
        .map(|(i, n)| ColumnInfo::new(n.clone(), rows.iter().map(|r| kind_of(&r[i])).find(|k| *k != "null").unwrap_or("ANY")))
        .collect();
    let (row_count, message) = if names.is_empty() {
        let s = &r["stats"];
        let parts: Vec<String> = ["nodes_created", "nodes_deleted", "relationships_created", "relationships_deleted", "properties_set"]
            .iter()
            .filter_map(|k| s[*k].as_i64().filter(|n| *n > 0).map(|n| format!("{n} {}", k.replace('_', " "))))
            .collect();
        (Some(0), Some(if parts.is_empty() { "Done".into() } else { parts.join(", ") }))
    } else {
        let msg = (total > rows.len()).then(|| format!("Showing the first {} of {total} rows", rows.len()));
        (Some(rows.len() as i64), msg)
    };
    Ok(SqlResult { columns, rows, row_count, message, query_ms: t0.elapsed().as_millis() as u64, sql: sql.to_string() })
}

/// The planner's plan for a statement (EXPLAIN: nothing runs), as the
/// Postgres-shaped tree the plan viewer draws.
pub async fn explain_plan(cfg: &Neo4jConfig, stmt: &str) -> Result<Value, String> {
    let s = stmt.trim().trim_end_matches(';').trim();
    let upper = s.to_ascii_uppercase();
    let body = if upper.starts_with("EXPLAIN ") || upper.starts_with("PROFILE ") { &s[8..] } else { s };
    let r = run_one(cfg, &format!("EXPLAIN {body}"), json!({})).await?;
    let root = &r["plan"]["root"];
    if root.is_null() {
        return Err("Neo4j returned no plan for this statement".into());
    }
    Ok(plan_node(root))
}

fn plan_node(op: &Value) -> Value {
    let mut n = Map::new();
    let kind = op["operatorType"].as_str().unwrap_or("Operator");
    n.insert("Node Type".into(), json!(kind.split('@').next().unwrap_or(kind)));
    if let Some(d) = op["Details"].as_str().filter(|d| !d.is_empty()) {
        n.insert("Filter".into(), json!(d));
    }
    if let Some(rows) = op["EstimatedRows"].as_f64() {
        n.insert("Plan Rows".into(), json!(rows.round() as i64));
    }
    if let Some(ids) = op["identifiers"].as_array().filter(|a| !a.is_empty()) {
        n.insert("Output".into(), Value::Array(ids.clone()));
    }
    let kids: Vec<Value> = op["children"].as_array().into_iter().flatten().map(plan_node).collect();
    if !kids.is_empty() {
        n.insert("Plans".into(), Value::Array(kids));
    }
    Value::Object(n)
}

/// The access level a Cypher script needs: 0 read, 1 write, 2 schema,
/// 3 server administration (the levels of `mcp::access`).
pub fn cypher_level(stmt: &str) -> u8 {
    // Strings and comments can't change the level: strip them first.
    let mut clean = String::with_capacity(stmt.len());
    let mut chars = stmt.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' | '"' | '`' => {
                let q = c;
                while let Some(d) = chars.next() {
                    if d == '\\' {
                        chars.next();
                    } else if d == q {
                        break;
                    }
                }
                clean.push(' ');
            }
            '/' if chars.peek() == Some(&'/') => {
                for d in chars.by_ref() {
                    if d == '\n' {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = ' ';
                for d in chars.by_ref() {
                    if prev == '*' && d == '/' {
                        break;
                    }
                    prev = d;
                }
            }
            c => clean.push(c.to_ascii_uppercase()),
        }
    }
    let words: Vec<&str> = clean.split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '.').filter(|w| !w.is_empty()).collect();
    let has = |w: &str| words.contains(&w);
    // Databases, users, roles and privileges: the server's administration.
    let admin = words.windows(2).any(|p| {
        matches!(p[0], "CREATE" | "DROP" | "ALTER" | "START" | "STOP" | "RENAME" | "REPLACE")
            && matches!(p[1], "DATABASE" | "COMPOSITE" | "USER" | "ROLE" | "ALIAS" | "SERVER" | "OR")
    }) || ["GRANT", "REVOKE", "DENY", "ALTER"].iter().any(|w| has(w));
    if admin {
        return 3;
    }
    let schema = words.windows(2).any(|p| {
        matches!(p[0], "CREATE" | "DROP")
            && matches!(p[1], "INDEX" | "CONSTRAINT" | "FULLTEXT" | "RANGE" | "TEXT" | "POINT" | "LOOKUP" | "VECTOR" | "BTREE")
    });
    if schema {
        return 2;
    }
    // Procedures can write (apoc.create.*, db.create.*, ...): any CALL
    // that isn't a known read counts as a write.
    let writing_call = words.windows(2).any(|p| {
        p[0] == "CALL"
            && !(p[1].starts_with("DB.LABELS") || p[1].starts_with("DB.RELATIONSHIPTYPES") || p[1].starts_with("DB.PROPERTYKEYS") || p[1].starts_with("DB.SCHEMA.") || p[1].starts_with("DB.INDEXES") || p[1].starts_with("DB.CONSTRAINTS") || p[1].starts_with("DBMS.COMPONENTS") || p[1].starts_with("SHOW"))
    });
    if ["CREATE", "MERGE", "SET", "DELETE", "DETACH", "REMOVE", "FOREACH", "LOAD"].iter().any(|w| has(w)) || writing_call {
        1
    } else {
        0
    }
}

// ── The document browser ──────────────────────────────────────────────────────

fn label_ident(label: &str) -> String {
    format!("`{}`", label.replace('`', "``"))
}

/// Node labels, with how many nodes carry each.
pub async fn labels(cfg: &Neo4jConfig) -> Result<Vec<(String, i64)>, String> {
    let r = run_one(cfg, "CALL db.labels() YIELD label RETURN label ORDER BY label", json!({})).await?;
    let names: Vec<String> = r["data"].as_array().into_iter().flatten().filter_map(|d| d["row"][0].as_str().map(String::from)).collect();
    if names.is_empty() {
        return Ok(vec![]);
    }
    let stmts = names.iter().map(|n| (format!("MATCH (n:{}) RETURN count(n)", label_ident(n)), json!({}))).collect();
    let counts = run(cfg, stmts).await?;
    Ok(names.into_iter().zip(counts).map(|(n, c)| (n, c["data"][0]["row"][0].as_i64().unwrap_or(-1))).collect())
}

/// Property keys seen on nodes with a label (a sample), for the describe panel.
pub async fn describe(cfg: &Neo4jConfig, label: &str) -> Result<Value, String> {
    let r = run_one(
        cfg,
        &format!("MATCH (n:{}) WITH n LIMIT 200 UNWIND keys(n) AS k RETURN k, count(*) AS c ORDER BY c DESC, k", label_ident(label)),
        json!({}),
    )
    .await?;
    let keys: Vec<Value> = r["data"].as_array().into_iter().flatten().map(|d| d["row"][0].clone()).collect();
    let count = run_one(cfg, &format!("MATCH (n:{}) RETURN count(n)", label_ident(label)), json!({})).await?;
    Ok(json!({ "keys": keys, "count": count["data"][0]["row"][0] }))
}

/// One page of nodes with a label. `filter` is a Cypher WHERE condition on `n`.
pub async fn page(cfg: &Neo4jConfig, label: &str, filter: &str, skip: i64, limit: i64) -> Result<Value, String> {
    let filter = filter.trim();
    if cypher_level(filter) > 0 {
        return Err("The filter is a WHERE condition: it can't write".into());
    }
    let wh = if filter.is_empty() { String::new() } else { format!(" WHERE {filter}") };
    let stmt = format!(
        "MATCH (n:{}){wh} RETURN elementId(n) AS id, labels(n) AS labels, properties(n) AS props ORDER BY elementId(n) SKIP $skip LIMIT $limit",
        label_ident(label)
    );
    let r = run_one(cfg, &stmt, json!({ "skip": skip.max(0), "limit": limit.clamp(1, 5000) })).await?;
    let docs: Vec<Value> = r["data"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|d| {
            let row = &d["row"];
            let mut doc = Map::new();
            doc.insert("_id".into(), row[0].clone());
            doc.insert("_labels".into(), row[1].clone());
            if let Some(props) = row[2].as_object() {
                for (k, v) in props {
                    doc.insert(k.clone(), v.clone());
                }
            }
            Value::Object(doc)
        })
        .collect();
    let next = (docs.len() as i64 >= limit).then_some(skip + docs.len() as i64);
    Ok(json!({ "docs": docs, "next": next }))
}

/// The properties to store: everything but the browser's own fields.
fn props_of(doc: &Value) -> Value {
    let mut out = Map::new();
    if let Some(o) = doc.as_object() {
        for (k, v) in o {
            if k != "_id" && k != "_labels" {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    Value::Object(out)
}

/// Save a node: replace its properties, or create it under `label`.
pub async fn save(cfg: &Neo4jConfig, label: &str, doc: &Value) -> Result<Value, String> {
    let props = props_of(doc);
    let (stmt, params) = match doc["_id"].as_str().filter(|s| !s.is_empty()) {
        Some(id) => (
            "MATCH (n) WHERE elementId(n) = $id SET n = $props RETURN elementId(n), labels(n), properties(n)".to_string(),
            json!({ "id": id, "props": props }),
        ),
        None => (
            format!("CREATE (n:{}) SET n = $props RETURN elementId(n), labels(n), properties(n)", label_ident(label)),
            json!({ "props": props }),
        ),
    };
    let r = run_one(cfg, &stmt, params).await?;
    let row = &r["data"][0]["row"];
    if row.is_null() {
        return Err("That node no longer exists".into());
    }
    let mut out = Map::new();
    out.insert("_id".into(), row[0].clone());
    out.insert("_labels".into(), row[1].clone());
    if let Some(p) = row[2].as_object() {
        out.extend(p.clone());
    }
    Ok(Value::Object(out))
}

/// Delete a node and its relationships.
pub async fn remove(cfg: &Neo4jConfig, id: &str) -> Result<(), String> {
    run_one(cfg, "MATCH (n) WHERE elementId(n) = $id DETACH DELETE n", json!({ "id": id })).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cypher_levels() {
        assert_eq!(cypher_level("MATCH (n:Person) RETURN n LIMIT 10"), 0);
        assert_eq!(cypher_level("MATCH (n) WHERE n.name = 'CREATE me' RETURN n"), 0, "strings don't count");
        assert_eq!(cypher_level("CALL db.labels()"), 0);
        assert_eq!(cypher_level("MATCH (n) SET n.x = 1"), 1);
        assert_eq!(cypher_level("MATCH (n) DETACH DELETE n"), 1);
        assert_eq!(cypher_level("MERGE (a:A {id: 1})"), 1);
        assert_eq!(cypher_level("CALL apoc.create.node(['X'], {})"), 1);
        assert_eq!(cypher_level("LOAD CSV FROM 'file:///x' AS r CREATE (:R)"), 1);
        assert_eq!(cypher_level("CREATE INDEX FOR (n:Person) ON (n.name)"), 2);
        assert_eq!(cypher_level("DROP CONSTRAINT c"), 2);
        assert_eq!(cypher_level("DROP DATABASE shop"), 3);
        assert_eq!(cypher_level("CREATE USER bob SET PASSWORD 'x'"), 3);
        assert_eq!(cypher_level("GRANT ROLE admin TO bob"), 3);
        assert_eq!(cypher_level("MATCH (n:`CREATE`) RETURN n"), 0, "quoted names don't count");
    }

    #[test]
    fn addresses_become_http() {
        let c = |u: &str| Neo4jConfig { name: "t".into(), url: u.into(), database: None, user: None, password: None };
        assert_eq!(base(&c("localhost:7474")), "http://localhost:7474");
        assert_eq!(base(&c("bolt://db.example.com:7687")), "http://db.example.com:7474");
        assert_eq!(base(&c("https://x.databases.neo4j.io/")), "https://x.databases.neo4j.io");
    }

    /// Against a Neo4j server: STROKE_TEST_NEO4J = url|user|password.
    #[tokio::test]
    async fn live_round_trip() {
        let Ok(spec) = std::env::var("STROKE_TEST_NEO4J") else { return };
        let p: Vec<&str> = spec.splitn(3, '|').collect();
        let cfg = Neo4jConfig { name: "t".into(), url: p[0].into(), database: None, user: Some(p[1].into()), password: Some(p[2].into()) };
        let cfg = connect(&cfg).await.unwrap();
        let label = format!("StrokeTest{}", std::process::id());
        query(&cfg, &format!("CREATE (:{label} {{name: 'Ada', born: 1815}}), (:{label} {{name: 'Alan', born: 1912}})")).await.unwrap();
        let ls = labels(&cfg).await.unwrap();
        assert!(ls.iter().any(|(n, c)| n == &label && *c == 2), "{ls:?}");
        let pg = page(&cfg, &label, "n.born > 1900", 0, 50).await.unwrap();
        assert_eq!(pg["docs"].as_array().unwrap().len(), 1);
        let mut doc = pg["docs"][0].clone();
        assert_eq!(doc["name"], "Alan");
        doc["name"] = json!("Alan Turing");
        let saved = save(&cfg, &label, &doc).await.unwrap();
        assert_eq!(saved["name"], "Alan Turing");
        let created = save(&cfg, &label, &json!({ "name": "Grace", "born": 1906 })).await.unwrap();
        assert!(created["_id"].as_str().is_some());
        remove(&cfg, created["_id"].as_str().unwrap()).await.unwrap();
        let d = describe(&cfg, &label).await.unwrap();
        assert_eq!(d["count"], 2);
        let r = query(&cfg, &format!("MATCH (n:{label}) RETURN n.name AS name, n.born AS born ORDER BY born")).await.unwrap();
        assert_eq!(r.rows[1], vec![json!("Alan Turing"), json!(1912)]);
        assert_eq!(r.columns[1].data_type, "INTEGER");
        assert!(page(&cfg, &label, "true DETACH DELETE n", 0, 10).await.is_err(), "a filter can't write");
        let gone = query(&cfg, &format!("MATCH (n:{label}) DETACH DELETE n")).await.unwrap();
        assert!(gone.message.unwrap().contains("2 nodes deleted"));
        let plan = explain_plan(&cfg, &format!("MATCH (n:{label}) WHERE n.born > 1900 RETURN n.name LIMIT 5")).await.unwrap();
        assert_eq!(plan["Node Type"], "ProduceResults");
        assert!(plan.to_string().contains("NodeByLabelScan"), "{plan}");
        assert!(query(&cfg, "MATCH (n RETURN n").await.is_err());
    }
}
