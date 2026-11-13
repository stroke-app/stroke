//! Cassandra / ScyllaDB over the CQL native protocol (`scylla` driver).
//!
//! The browser reads rows with `SELECT JSON`, so each row arrives as one JSON
//! object in Cassandra's own JSON mapping, and writes them back with
//! `INSERT ... JSON ? DEFAULT UNSET`: no per-type marshalling in the app, and
//! columns the user didn't touch are left alone. The query console runs any
//! CQL statement and maps the typed values to JSON for the grid.

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use scylla::client::session::Session;
use scylla::client::session_builder::SessionBuilder;
use scylla::response::PagingState;
use scylla::statement::unprepared::Statement;
use scylla::value::{CqlValue, Row};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CassandraConfig {
    pub name: String,
    /// One or more contact points, `host[:port]`, comma separated.
    pub hosts: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    /// Default keyspace for unqualified table names in the console.
    #[serde(default)]
    pub keyspace: Option<String>,
}

pub type CassandraHandle = Arc<Session>;

/// Maps every node address to the one endpoint the user gave.
struct SingleEndpoint(std::net::SocketAddr);

#[async_trait::async_trait]
impl scylla::policies::address_translator::AddressTranslator for SingleEndpoint {
    async fn translate_address(
        &self,
        _peer: &scylla::policies::address_translator::UntranslatedPeer,
    ) -> Result<std::net::SocketAddr, scylla::errors::TranslationError> {
        Ok(self.0)
    }
}

const SYSTEM_KEYSPACES: &[&str] = &[
    "system", "system_auth", "system_schema", "system_distributed", "system_traces", "system_views", "system_virtual_schema",
];

fn contact_points(cfg: &CassandraConfig) -> Vec<String> {
    cfg.hosts
        .split(',')
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .map(|h| if h.rsplit_once(':').is_some_and(|(_, p)| p.parse::<u16>().is_ok()) { h.to_string() } else { format!("{h}:9042") })
        .collect()
}

pub async fn connect(cfg: &CassandraConfig) -> Result<CassandraHandle, String> {
    let nodes = contact_points(cfg);
    if nodes.is_empty() {
        return Err("Enter at least one host".into());
    }
    let mut b = SessionBuilder::new().known_nodes(&nodes).connection_timeout(Duration::from_secs(10));
    // One contact point: every node the cluster reports is reached through it.
    // A node in Docker, behind NAT or a port forward advertises an address
    // this machine can't reach (the container's 172.x), and the driver then
    // times out reaching the very node it just talked to.
    if let [only] = nodes.as_slice() {
        if let Some(addr) = tokio::net::lookup_host(only.as_str()).await.ok().and_then(|mut a| a.next()) {
            b = b.address_translator(Arc::new(SingleEndpoint(addr)));
        }
    }
    if let Some(u) = cfg.user.as_deref().filter(|u| !u.is_empty()) {
        b = b.user(u, cfg.password.as_deref().unwrap_or(""));
    }
    let session = tokio::time::timeout(Duration::from_secs(20), b.build())
        .await
        .map_err(|_| format!("Cassandra at {} did not answer in time", nodes.join(", ")))?
        .map_err(|e| format!("Could not connect to Cassandra at {}: {e}", nodes.join(", ")))?;
    if let Some(ks) = cfg.keyspace.as_deref().filter(|k| !k.is_empty()) {
        session.use_keyspace(ks, false).await.map_err(|e| format!("Keyspace \"{ks}\": {e}"))?;
    }
    Ok(Arc::new(session))
}

/// `"name"` with inner quotes doubled.
pub fn ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn table_ref(ks: &str, table: &str) -> String {
    format!("{}.{}", ident(ks), ident(table))
}

/// Rows of a result as dynamic `Row`s, with the column names.
async fn run(s: &Session, stmt: Statement, values: impl scylla::serialize::row::SerializeRow, state: PagingState)
    -> Result<(Vec<String>, Vec<Row>, Option<String>), String> {
    let (res, paging) = s.query_single_page(stmt, values, state).await.map_err(|e| e.to_string())?;
    let next = match paging.into_paging_control_flow() {
        ControlFlow::Continue(p) => p.as_bytes_slice().map(|b| B64.encode(&b[..])),
        ControlFlow::Break(()) => None,
    };
    match res.into_rows_result() {
        Ok(rows) => {
            let names = rows.column_specs().iter().map(|c| c.name().to_string()).collect();
            let rows = rows.rows::<Row>().map_err(|e| e.to_string())?.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
            Ok((names, rows, next))
        }
        // DDL/DML: no rows.
        Err(_) => Ok((vec![], vec![], None)),
    }
}

fn text(v: &Option<CqlValue>) -> String {
    match v {
        Some(CqlValue::Text(s)) | Some(CqlValue::Ascii(s)) => s.clone(),
        _ => String::new(),
    }
}

/// User keyspaces, sorted.
pub async fn list_keyspaces(s: &Session) -> Result<Vec<String>, String> {
    let (_, rows, _) = run(s, Statement::new("SELECT keyspace_name FROM system_schema.keyspaces").with_page_size(5000), (), PagingState::start()).await?;
    let mut out: Vec<String> = rows.iter().map(|r| text(&r.columns[0])).filter(|k| !SYSTEM_KEYSPACES.contains(&k.as_str())).collect();
    out.sort();
    Ok(out)
}

/// One keyspace's tables, sorted.
pub async fn tables_in(s: &Session, ks: &str) -> Result<Vec<String>, String> {
    let (_, rows, _) = run(
        s,
        Statement::new("SELECT table_name FROM system_schema.tables WHERE keyspace_name = ?").with_page_size(5000),
        (ks,),
        PagingState::start(),
    )
    .await?;
    let mut out: Vec<String> = rows.iter().map(|r| text(&r.columns[0])).collect();
    out.sort();
    Ok(out)
}

/// User keyspaces' tables as `keyspace.table`.
pub async fn list_tables(s: &Session) -> Result<Vec<String>, String> {
    let (_, rows, _) = run(s, Statement::new("SELECT keyspace_name, table_name FROM system_schema.tables").with_page_size(5000), (), PagingState::start()).await?;
    let mut out: Vec<String> = rows
        .iter()
        .map(|r| (text(&r.columns[0]), text(&r.columns[1])))
        .filter(|(ks, _)| !SYSTEM_KEYSPACES.contains(&ks.as_str()))
        .map(|(ks, t)| format!("{ks}.{t}"))
        .collect();
    out.sort();
    Ok(out)
}

#[derive(Serialize)]
pub struct TableShape {
    pub keyspace: String,
    pub table: String,
    /// Partition key columns, then clustering columns, in key order.
    pub keys: Vec<String>,
    pub partition_keys: Vec<String>,
    /// Every column with its CQL type.
    pub columns: Vec<(String, String)>,
}

fn split(full: &str) -> Result<(String, String), String> {
    full.split_once('.').map(|(a, b)| (a.to_string(), b.to_string())).ok_or_else(|| format!("\"{full}\" is not keyspace.table"))
}

pub async fn describe(s: &Session, full: &str) -> Result<TableShape, String> {
    let (ks, table) = split(full)?;
    let (_, rows, _) = run(
        s,
        Statement::new("SELECT column_name, kind, position, type FROM system_schema.columns WHERE keyspace_name = ? AND table_name = ?").with_page_size(5000),
        (ks.as_str(), table.as_str()),
        PagingState::start(),
    )
    .await?;
    if rows.is_empty() {
        return Err(format!("Table {full} not found"));
    }
    let mut cols: Vec<(String, String, i32, String)> = rows
        .iter()
        .map(|r| {
            let pos = match &r.columns[2] { Some(CqlValue::Int(p)) => *p, _ => 0 };
            (text(&r.columns[0]), text(&r.columns[1]), pos, text(&r.columns[3]))
        })
        .collect();
    let rank = |k: &str| match k { "partition_key" => 0, "clustering" => 1, _ => 2 };
    cols.sort_by(|a, b| rank(&a.1).cmp(&rank(&b.1)).then(a.2.cmp(&b.2)).then(a.0.cmp(&b.0)));
    Ok(TableShape {
        keyspace: ks,
        table,
        keys: cols.iter().filter(|c| rank(&c.1) < 2).map(|c| c.0.clone()).collect(),
        partition_keys: cols.iter().filter(|c| c.1 == "partition_key").map(|c| c.0.clone()).collect(),
        columns: cols.into_iter().map(|c| (c.0, c.3)).collect(),
    })
}

/// One column of `system_schema.columns`.
pub struct CatalogColumn {
    pub table: String,
    pub name: String,
    /// `partition_key`, `clustering`, `regular` or `static`.
    pub kind: String,
    pub position: i32,
    pub cql_type: String,
    /// `asc` / `desc` for a clustering column.
    pub order: String,
}

/// Every column in a keyspace, one statement.
pub async fn keyspace_columns(s: &Session, ks: &str) -> Result<Vec<CatalogColumn>, String> {
    let (_, rows, _) = run(
        s,
        Statement::new("SELECT table_name, column_name, kind, position, type, clustering_order FROM system_schema.columns WHERE keyspace_name = ?").with_page_size(50_000),
        (ks,),
        PagingState::start(),
    )
    .await?;
    Ok(rows
        .iter()
        .map(|r| CatalogColumn {
            table: text(&r.columns[0]),
            name: text(&r.columns[1]),
            kind: text(&r.columns[2]),
            position: match &r.columns[3] { Some(CqlValue::Int(p)) => *p, _ => 0 },
            cql_type: text(&r.columns[4]),
            order: text(&r.columns[5]),
        })
        .collect())
}

/// Per-table column lists in CQL's own order: partition key, clustering key,
/// then the rest by name.
pub fn columns_by_table(mut cols: Vec<CatalogColumn>) -> Vec<super::schema::TableColumnStructure> {
    let rank = |k: &str| match k { "partition_key" => 0, "clustering" => 1, "static" => 2, _ => 3 };
    cols.sort_by(|a, b| a.table.cmp(&b.table).then(rank(&a.kind).cmp(&rank(&b.kind))).then(a.position.cmp(&b.position)).then(a.name.cmp(&b.name)));
    let mut out: Vec<super::schema::TableColumnStructure> = Vec::new();
    for c in cols {
        let col = super::schema::ColumnStructureRow {
            ordinal_position: out.last().filter(|t| t.table == c.table).map(|t| t.columns.len() as i32 + 1).unwrap_or(1),
            is_primary_key: rank(&c.kind) < 2,
            is_nullable: rank(&c.kind) >= 2,
            name: c.name,
            data_type: c.cql_type,
            column_default: None,
            foreign_key: None,
            fk_constraint_name: None,
            comment: if c.kind == "static" { Some("static".into()) } else { None },
        };
        match out.last_mut() {
            Some(last) if last.table == c.table => last.columns.push(col),
            _ => out.push(super::schema::TableColumnStructure { table: c.table, columns: vec![col] }),
        }
    }
    out
}

/// Every table's partition and clustering key (as key rows, see
/// `list_indexes_for`) and secondary indexes in a keyspace: two statements.
pub async fn keyspace_indexes(s: &Session, ks: &str) -> Result<Vec<super::schema::IndexInfo>, String> {
    let cols = keyspace_columns(s, ks).await?;
    let (_, rows, _) = run(
        s,
        Statement::new("SELECT table_name, index_name, kind, options FROM system_schema.indexes WHERE keyspace_name = ?").with_page_size(50_000),
        (ks,),
        PagingState::start(),
    )
    .await?;
    let secondary = rows
        .iter()
        .map(|r| {
            let target = match &r.columns[3] {
                Some(CqlValue::Map(m)) => m
                    .iter()
                    .find(|(k, _)| matches!(k, CqlValue::Text(t) if t == "target"))
                    .map(|(_, v)| text(&Some(v.clone())))
                    .unwrap_or_default(),
                _ => String::new(),
            };
            (text(&r.columns[0]), text(&r.columns[1]), text(&r.columns[2]).to_lowercase(), target)
        })
        .collect();
    Ok(key_and_index_rows(cols, secondary))
}

/// Key rows from the catalog columns, then the secondary indexes
/// (`(table, name, kind, target)`).
pub fn key_and_index_rows(mut cols: Vec<CatalogColumn>, secondary: Vec<(String, String, String, String)>) -> Vec<super::schema::IndexInfo> {
    use super::schema::IndexInfo;
    cols.retain(|c| c.kind == "partition_key" || c.kind == "clustering");
    cols.sort_by(|a, b| a.table.cmp(&b.table).then(a.kind.cmp(&b.kind).reverse()).then(a.position.cmp(&b.position)));
    let mut out: Vec<IndexInfo> = Vec::new();
    let mut i = 0;
    while i < cols.len() {
        let (table, kind) = (cols[i].table.clone(), cols[i].kind.clone());
        let mut names = Vec::new();
        while i < cols.len() && cols[i].table == table && cols[i].kind == kind {
            let c = &cols[i];
            names.push(if c.order == "desc" { format!("{} desc", c.name) } else { c.name.clone() });
            i += 1;
        }
        out.push(IndexInfo {
            name: if kind == "partition_key" { "partition key".into() } else { "clustering key".into() },
            table_name: table,
            columns: names.join(", "),
            index_type: "key".into(),
            is_unique: false,
            is_primary: kind == "partition_key",
            condition: None,
            comment: None,
        });
    }
    for (table, name, kind, target) in secondary {
        out.push(IndexInfo {
            name,
            table_name: table,
            columns: target,
            index_type: if kind.is_empty() { "secondary".into() } else { kind },
            is_unique: false,
            is_primary: false,
            condition: None,
            comment: None,
        });
    }
    out
}

#[derive(Debug, Serialize)]
pub struct JsonPage {
    pub rows: Vec<Value>,
    /// Base64 paging state for the next page, `None` at the end.
    pub next: Option<String>,
}

/// One page of `SELECT JSON *`, optionally with a CQL `WHERE` clause.
pub async fn page(s: &Session, full: &str, filter: &str, cursor: Option<String>, size: i32) -> Result<JsonPage, String> {
    let (ks, table) = split(full)?;
    let mut cql = format!("SELECT JSON * FROM {}", table_ref(&ks, &table));
    let f = filter.trim().trim_end_matches(';').trim();
    let f = f.strip_prefix("WHERE ").or_else(|| f.strip_prefix("where ")).unwrap_or(f);
    if !f.is_empty() {
        if f.contains(';') {
            return Err("The filter is one WHERE clause".into());
        }
        cql.push_str(" WHERE ");
        cql.push_str(f);
    }
    let state = match cursor {
        Some(c) => PagingState::new_from_raw_bytes(B64.decode(c).map_err(|_| "Invalid page cursor")?),
        None => PagingState::start(),
    };
    let (_, rows, next) = run(s, Statement::new(cql).with_page_size(size.clamp(1, 1000)), (), state).await.map_err(|e| {
        if e.contains("ALLOW FILTERING") {
            format!("{e}\nAdd ALLOW FILTERING to the end of the filter to scan anyway.")
        } else {
            e
        }
    })?;
    let rows = rows
        .iter()
        .map(|r| serde_json::from_str(&text(&r.columns[0])).map_err(|e| e.to_string()))
        .collect::<Result<Vec<Value>, _>>()?;
    Ok(JsonPage { rows, next })
}

/// `WHERE "k1" = fromJson(?) AND ...` for a row's primary key.
fn key_where(keys: &[String], row: &Value) -> Result<(String, Vec<String>), String> {
    let mut parts = vec![];
    let mut vals = vec![];
    for k in keys {
        let v = row.get(k).ok_or_else(|| format!("The row needs its key column \"{k}\""))?;
        if v.is_null() {
            return Err(format!("Key column \"{k}\" can't be null"));
        }
        parts.push(format!("{} = fromJson(?)", ident(k)));
        vals.push(v.to_string());
    }
    Ok((parts.join(" AND "), vals))
}

/// Write a row from JSON. With `prev` and a changed key, the old row is
/// deleted in the same logged batch.
pub async fn save(s: &Session, full: &str, row: &Value, prev: Option<&Value>) -> Result<(), String> {
    if !row.is_object() {
        return Err("A row is a JSON object".into());
    }
    let shape = describe(s, full).await?;
    let tref = table_ref(&shape.keyspace, &shape.table);
    let json = row.to_string();
    let insert = format!("INSERT INTO {tref} JSON ? DEFAULT UNSET");
    let key_changed = prev.is_some_and(|p| shape.keys.iter().any(|k| p.get(k) != row.get(k)));
    if !key_changed {
        return run(s, Statement::new(insert), (json.as_str(),), PagingState::start()).await.map(|_| ());
    }
    let (w, vals) = key_where(&shape.keys, prev.unwrap())?;
    let cql = format!("BEGIN BATCH {insert}; DELETE FROM {tref} WHERE {w}; APPLY BATCH");
    let mut all = vec![json];
    all.extend(vals);
    run(s, Statement::new(cql), all, PagingState::start()).await.map(|_| ())
}

pub async fn delete(s: &Session, full: &str, row: &Value) -> Result<(), String> {
    let shape = describe(s, full).await?;
    let (w, vals) = key_where(&shape.keys, row)?;
    let cql = format!("DELETE FROM {} WHERE {w}", table_ref(&shape.keyspace, &shape.table));
    run(s, Statement::new(cql), vals, PagingState::start()).await.map(|_| ())
}

// ── Console ──────────────────────────────────────────────────────────────────

/// A CQL value as JSON for the result grid.
pub fn to_json(v: &Option<CqlValue>) -> Value {
    let Some(v) = v else { return Value::Null };
    match v {
        CqlValue::Ascii(s) | CqlValue::Text(s) => Value::String(s.clone()),
        CqlValue::Boolean(b) => Value::Bool(*b),
        CqlValue::Int(i) => json!(i),
        CqlValue::BigInt(i) => json!(i),
        CqlValue::SmallInt(i) => json!(i),
        CqlValue::TinyInt(i) => json!(i),
        CqlValue::Float(f) => json!(f),
        CqlValue::Double(f) => json!(f),
        CqlValue::Uuid(u) => Value::String(u.to_string()),
        CqlValue::Timeuuid(u) => Value::String(u.to_string()),
        CqlValue::Inet(a) => Value::String(a.to_string()),
        CqlValue::Blob(b) => Value::String(format!("0x{}", hex::encode(b))),
        CqlValue::Timestamp(t) => chrono::DateTime::from_timestamp_millis(t.0)
            .map(|d| Value::String(d.to_rfc3339()))
            .unwrap_or_else(|| json!(t.0)),
        CqlValue::List(xs) | CqlValue::Set(xs) => Value::Array(xs.iter().map(|x| to_json(&Some(x.clone()))).collect()),
        CqlValue::Map(kv) => {
            let mut m = serde_json::Map::new();
            for (k, val) in kv {
                let key = match to_json(&Some(k.clone())) { Value::String(s) => s, other => other.to_string() };
                m.insert(key, to_json(&Some(val.clone())));
            }
            Value::Object(m)
        }
        CqlValue::UserDefinedType { fields, .. } => {
            Value::Object(fields.iter().map(|(n, val)| (n.clone(), to_json(val))).collect())
        }
        CqlValue::Tuple(xs) => Value::Array(xs.iter().map(to_json).collect()),
        CqlValue::Empty => Value::Null,
        // Counter, decimal, varint, date, time, duration: their Debug text.
        other => Value::String(format!("{other:?}")),
    }
}

const CONSOLE_CAP: usize = 5_000;

/// Run one CQL statement from the console; follows pages up to a cap.
pub async fn query(s: &Session, stmt: &str) -> Result<super::query::SqlResult, String> {
    use super::query::{ColumnInfo, SqlResult};
    let t0 = std::time::Instant::now();
    let cql = stmt.trim().trim_end_matches(';').trim();
    if cql.is_empty() {
        return Err("Empty statement".into());
    }
    let mut names = vec![];
    let mut rows: Vec<Vec<Value>> = vec![];
    let mut state = PagingState::start();
    let mut capped = false;
    loop {
        let (n, page, next) = run(s, Statement::new(cql).with_page_size(1000), (), state).await?;
        if names.is_empty() {
            names = n;
        }
        rows.extend(page.iter().map(|r| r.columns.iter().map(to_json).collect()));
        match next {
            Some(n) if rows.len() < CONSOLE_CAP => state = PagingState::new_from_raw_bytes(B64.decode(n).unwrap_or_default()),
            Some(_) => {
                capped = true;
                break;
            }
            None => break,
        }
    }
    let is_select = cql.split_whitespace().next().is_some_and(|w| w.eq_ignore_ascii_case("select"));
    Ok(SqlResult {
        columns: names.iter().map(|n| ColumnInfo::new(n.clone(), "json")).collect(),
        row_count: Some(rows.len() as i64),
        message: if capped {
            Some(format!("Showing the first {} rows; add LIMIT or a WHERE on the key", rows.len()))
        } else if !is_select {
            Some("Statement applied".into())
        } else {
            None
        },
        rows,
        query_ms: t0.elapsed().as_millis() as u64,
        sql: stmt.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(table: &str, name: &str, kind: &str, position: i32, ty: &str, order: &str) -> CatalogColumn {
        CatalogColumn { table: table.into(), name: name.into(), kind: kind.into(), position, cql_type: ty.into(), order: order.into() }
    }

    #[test]
    fn keyspace_catalog_becomes_columns_keys_and_indexes() {
        let cols = || vec![
            col("events", "payload", "regular", -1, "map<text, int>", "none"),
            col("events", "day", "partition_key", 1, "date", "none"),
            col("events", "tenant", "partition_key", 0, "uuid", "none"),
            col("events", "at", "clustering", 0, "timestamp", "desc"),
            col("events", "seq", "clustering", 1, "int", "asc"),
            col("users", "id", "partition_key", 0, "uuid", "none"),
            col("users", "email", "regular", -1, "text", "none"),
        ];
        let tables = columns_by_table(cols());
        assert_eq!(tables[0].columns.iter().map(|c| (c.name.as_str(), c.data_type.as_str(), c.is_primary_key)).collect::<Vec<_>>(),
            [("tenant", "uuid", true), ("day", "date", true), ("at", "timestamp", true), ("seq", "int", true), ("payload", "map<text, int>", false)]);
        let idx = key_and_index_rows(cols(), vec![("users".into(), "users_email_idx".into(), "composites".into(), "email".into())]);
        let rows: Vec<_> = idx.iter().map(|i| (i.table_name.as_str(), i.name.as_str(), i.columns.as_str(), i.index_type.as_str())).collect();
        assert_eq!(rows, [
            ("events", "partition key", "tenant, day", "key"),
            ("events", "clustering key", "at desc, seq", "key"),
            ("users", "partition key", "id", "key"),
            ("users", "users_email_idx", "email", "composites"),
        ]);
    }

    #[test]
    fn contact_points_default_port() {
        let cfg = CassandraConfig { name: "t".into(), hosts: "a, b:9142 ,,10.0.0.1".into(), user: None, password: None, keyspace: None };
        assert_eq!(contact_points(&cfg), ["a:9042", "b:9142", "10.0.0.1:9042"]);
    }

    #[test]
    fn identifiers_and_key_where() {
        assert_eq!(ident(r#"we"ird"#), r#""we""ird""#);
        let (w, v) = key_where(&["user_id".into(), "ts".into()], &json!({"user_id": "u1", "ts": 5, "x": 1})).unwrap();
        assert_eq!(w, r#""user_id" = fromJson(?) AND "ts" = fromJson(?)"#);
        assert_eq!(v, [r#""u1""#, "5"]);
        assert!(key_where(&["k".into()], &json!({"k": null})).is_err());
        assert!(key_where(&["k".into()], &json!({})).is_err());
    }

    /// Against `dev/nosql` Cassandra: `cargo test --lib cassandra_live -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn cassandra_live() {
        let cfg = CassandraConfig { name: "t".into(), hosts: "127.0.0.1".into(), user: None, password: None, keyspace: None };
        let s = connect(&cfg).await.unwrap();
        let tables = list_tables(&s).await.unwrap();
        assert!(tables.iter().any(|t| t.starts_with("shop.")), "{tables:?}");
        let users = tables.iter().find(|t| *t == "shop.users").expect("shop.users");
        let shape = describe(&s, users).await.unwrap();
        assert!(!shape.keys.is_empty());
        let p = page(&s, users, "", None, 2).await.unwrap();
        assert!(!p.rows.is_empty() && p.rows.len() <= 2);
        let r = query(&s, "SELECT * FROM shop.users").await.unwrap();
        assert!(r.rows.len() >= p.rows.len());
        // Round trip a row through JSON: insert a copy under a new key, edit,
        // move its key, delete.
        let mut row = p.rows[0].clone();
        let k = &shape.keys[0];
        let is_uuid = |v: &Value| v.as_str().is_some_and(|x| x.len() == 36 && x.matches('-').count() == 4);
        let (k1, k2) = match &row[k] {
            v if is_uuid(v) => (json!("00000000-0000-4000-8000-0000000000a1"), json!("00000000-0000-4000-8000-0000000000a2")),
            Value::String(_) => (json!("it-row-1"), json!("it-row-2")),
            Value::Number(_) => (json!(987654321), json!(987654322)),
            other => panic!("unexpected key type {other}"),
        };
        let lit = |v: &Value| match v { Value::String(x) if is_uuid(v) => x.clone(), Value::String(x) => format!("'{x}'"), n => n.to_string() };
        let new_key = k1.clone();
        row[k] = new_key.clone();
        save(&s, users, &row, None).await.unwrap();
        let got = page(&s, users, &format!("{} = {}", ident(k), lit(&new_key)), None, 10).await.unwrap();
        assert_eq!(got.rows.len(), 1, "inserted row readable");
        let mut moved = got.rows[0].clone();
        moved[k] = k2.clone();
        save(&s, users, &moved, Some(&got.rows[0])).await.unwrap();
        let old = query(&s, &format!("SELECT * FROM shop.users WHERE {} = {}", ident(k), lit(&new_key))).await.unwrap();
        assert_eq!(old.rows.len(), 0, "old key removed in the batch");
        delete(&s, users, &moved).await.unwrap();
        let err = page(&s, users, "nope = 1", None, 10).await.unwrap_err();
        assert!(!err.is_empty());
    }
}
