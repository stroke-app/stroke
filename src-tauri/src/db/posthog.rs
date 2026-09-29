//! PostHog, queried with HogQL over PostHog's query API.
//!
//! PostHog stores events, persons and sessions in ClickHouse, but Cloud gives no
//! direct ClickHouse access. The query API (`POST /api/projects/{id}/query/`)
//! takes HogQL, PostHog's SQL dialect over that ClickHouse, and returns
//! `columns` / `types` / `results`. The schema comes from the same endpoint as a
//! `DatabaseSchemaQuery`. Everything here is read-only: the API runs SELECTs.
//!
//! Limits that shape this module (PostHog docs): 10s per query, 50,000 rows at
//! most, 240 requests a minute and 3 concurrent queries per project. Hence the
//! schema cache, one request per page, and a row cap.

use super::connection::PosthogConfig;
use super::query::{ColumnInfo, ForeignKeyInfo, RowFilter, SqlResult, TableRows};
use super::schema::{ColumnStructureRow, IndexInfo, TableInfo};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The one schema PostHog exposes to the sidebar.
pub const SCHEMA: &str = "posthog";
/// HogQL's hard ceiling on returned rows.
const MAX_ROWS: i64 = 50_000;
/// How long a fetched schema is reused before asking PostHog again.
const SCHEMA_TTL: Duration = Duration::from_secs(300);

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("stroke/1.0")
            .pool_idle_timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            // PostHog stops a query at 10s; leave room for queueing and transfer.
            .timeout(Duration::from_secs(45))
            .build()
            .expect("failed to build PostHog HTTP client")
    })
}

/// POST one query object to the project's query endpoint.
async fn post_query(config: &PosthogConfig, query: Value) -> Result<Value, String> {
    let url = format!(
        "{}/api/projects/{}/query/",
        config.base_url(),
        urlencoding::encode(config.project_id.trim())
    );
    let resp = client()
        .post(&url)
        .bearer_auth(config.api_key.trim())
        .json(&json!({ "query": query, "name": "stroke" }))
        .send()
        .await
        .map_err(|e| format!("PostHog request failed: {e}"))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| format!("PostHog read failed: {e}"))?;
    let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        let detail = body["detail"]
            .as_str()
            .or_else(|| body["error"].as_str())
            .map(String::from)
            .unwrap_or_else(|| text.chars().take(300).collect());
        return Err(match status {
            401 => "PostHog rejected the API key. Check it, and that it has the Query Read scope.".into(),
            403 => format!("PostHog refused access to project {}: {detail}", config.project_id),
            429 => "PostHog's query rate limit was hit (240 a minute, 3 at once). Wait a moment and try again.".into(),
            _ => format!("PostHog error ({status}): {detail}"),
        });
    }
    if let Some(err) = body["error"].as_str().filter(|e| !e.is_empty()) {
        return Err(format!("PostHog error: {err}"));
    }
    Ok(body)
}

/// Only SELECT-shaped HogQL reaches the API: it can't write, so say so up front
/// rather than relaying a parse error.
fn is_read_query(sql: &str) -> bool {
    matches!(super::sql_util::statement_head(sql).as_str(), "select" | "with")
}

/// A `types` entry: `["column", "Nullable(String)"]` or a bare type string.
fn type_of(entry: &Value) -> String {
    let raw = entry
        .as_array()
        .and_then(|pair| pair.get(1))
        .and_then(Value::as_str)
        .or_else(|| entry.as_str())
        .unwrap_or("");
    strip_wrappers(raw)
}

fn strip_wrappers(ty: &str) -> String {
    let mut t = ty;
    for w in ["Nullable(", "LowCardinality("] {
        if let Some(inner) = t.strip_prefix(w).and_then(|s| s.strip_suffix(')')) {
            t = inner;
        }
    }
    t.to_string()
}

/// A HogQL response → the app's result shape.
fn to_sql_result(body: &Value, sql: &str, query_ms: u64) -> SqlResult {
    let names: Vec<String> = body["columns"]
        .as_array()
        .map(|c| c.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect())
        .unwrap_or_default();
    let types = body["types"].as_array().cloned().unwrap_or_default();
    let columns: Vec<ColumnInfo> = names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let raw = types.get(i).cloned().unwrap_or(Value::Null);
            let mut c = ColumnInfo::new(n.clone(), type_of(&raw));
            c.nullable = raw.as_array().and_then(|p| p.get(1)).and_then(Value::as_str).is_some_and(|t| t.starts_with("Nullable("));
            c
        })
        .collect();
    // Cap oversized cells (event `properties` can be large JSON) before they
    // reach the webview, like every other engine.
    let rows: Vec<Vec<Value>> = body["results"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|r| {
                    r.as_array()
                        .map(|cells| cells.iter().cloned().map(|v| super::sql_util::cap_json_value("text", v)).collect())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default();
    SqlResult {
        row_count: Some(rows.len() as i64),
        message: body["hasMore"].as_bool().filter(|m| *m).map(|_| "More rows exist - add a LIMIT to fetch a specific range.".into()),
        columns,
        rows,
        query_ms,
        sql: sql.to_string(),
    }
}

/// Run one HogQL statement.
pub async fn query(config: &PosthogConfig, sql: &str) -> Result<SqlResult, String> {
    let trimmed = sql.trim().trim_end_matches(';');
    if !is_read_query(trimmed) {
        return Err("PostHog is read-only here: HogQL runs SELECT (and WITH) queries.".into());
    }
    let t0 = Instant::now();
    let body = post_query(config, json!({ "kind": "HogQLQuery", "query": trimmed })).await?;
    Ok(to_sql_result(&body, sql, t0.elapsed().as_millis() as u64))
}

// ── Schema ───────────────────────────────────────────────────────────────────

fn schema_cache() -> &'static Mutex<HashMap<String, (Instant, Value)>> {
    static C: OnceLock<Mutex<HashMap<String, (Instant, Value)>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

/// The project's `DatabaseSchemaQuery` result (`tables` map), cached for a few
/// minutes: it is the heaviest call here, and the sidebar, structure view and
/// every filtered page all need it.
async fn schema(config: &PosthogConfig) -> Result<Value, String> {
    let key = format!("{}|{}", config.base_url(), config.project_id.trim());
    if let Some((at, v)) = schema_cache().lock().ok().and_then(|m| m.get(&key).cloned()) {
        if at.elapsed() < SCHEMA_TTL {
            return Ok(v);
        }
    }
    let body = post_query(config, json!({ "kind": "DatabaseSchemaQuery" })).await?;
    let tables = body["tables"].clone();
    if let Ok(mut m) = schema_cache().lock() {
        m.insert(key, (Instant::now(), tables.clone()));
    }
    Ok(tables)
}

/// Field types that are real columns. The rest (`lazy_table`, `virtual_table`,
/// `field_traverser`, `expression`, views) are relations and computed paths
/// that `SELECT *` doesn't return.
fn is_column(ty: &str) -> bool {
    matches!(ty, "integer" | "float" | "decimal" | "string" | "datetime" | "date" | "boolean" | "array" | "json" | "unknown")
}

fn table_fields(tables: &Value, table: &str) -> Vec<(String, String)> {
    tables[table]["fields"]
        .as_object()
        .map(|fields| {
            fields
                .values()
                .filter_map(|f| {
                    let ty = f["type"].as_str().unwrap_or("unknown");
                    is_column(ty).then(|| (f["name"].as_str().unwrap_or("").to_string(), ty.to_string()))
                })
                .filter(|(n, _)| !n.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub async fn list_schemas(_config: &PosthogConfig) -> Result<Vec<String>, String> {
    Ok(vec![SCHEMA.to_string()])
}

pub async fn list_tables(config: &PosthogConfig) -> Result<Vec<TableInfo>, String> {
    let tables = schema(config).await?;
    let mut out: Vec<TableInfo> = tables
        .as_object()
        .map(|m| {
            m.values()
                .filter_map(|t| {
                    let name = t["name"].as_str()?.to_string();
                    let kind = match t["type"].as_str().unwrap_or("") {
                        "view" | "materialized_view" | "managed_view" => "view",
                        _ => "table",
                    };
                    let row_count = t["row_count"].as_f64().map(|n| n as i64).unwrap_or(-1);
                    Some(TableInfo { name, kind: kind.to_string(), row_count, rls_enabled: None })
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub async fn list_indexes(_config: &PosthogConfig) -> Result<Vec<IndexInfo>, String> {
    Ok(vec![])
}

pub async fn get_column_structure(config: &PosthogConfig, table: &str) -> Result<Vec<ColumnStructureRow>, String> {
    let tables = schema(config).await?;
    Ok(table_fields(&tables, table)
        .into_iter()
        .enumerate()
        .map(|(i, (name, ty))| ColumnStructureRow {
            ordinal_position: (i + 1) as i32,
            name,
            data_type: ty,
            is_nullable: true,
            column_default: None,
            foreign_key: None,
            fk_constraint_name: None,
            comment: None,
        })
        .collect())
}

// ── Browsing ─────────────────────────────────────────────────────────────────

/// One page of a table, and its count, as two concurrent HogQL queries.
pub async fn get_table_rows(
    config: &PosthogConfig,
    table: &str,
    limit: i64,
    offset: i64,
    search: Option<String>,
    sort_column: Option<String>,
    sort_direction: Option<String>,
    filters: Option<Vec<RowFilter>>,
) -> Result<TableRows, String> {
    let t0 = Instant::now();
    let tq = super::sql_util::quote_backtick(table);
    let has_search = search.as_deref().map(str::trim).is_some_and(|s| !s.is_empty());
    let has_filters = filters.as_ref().is_some_and(|f| !f.is_empty());
    let cols = if has_search || has_filters { get_column_structure(config, table).await? } else { Vec::new() };
    let where_clause = super::clickhouse::build_where(&cols, search.as_deref(), filters.as_deref());

    let order = match (sort_column.as_deref().map(str::trim), sort_direction.as_deref()) {
        (Some(c), dir) if !c.is_empty() => {
            let d = if dir.is_some_and(|d| d.eq_ignore_ascii_case("desc")) { "DESC" } else { "ASC" };
            format!(" ORDER BY {} {d}", super::sql_util::quote_backtick(c))
        }
        // Events are read newest first: the start of a years-long event table is
        // rarely what anyone opened it for.
        _ if table == "events" => " ORDER BY timestamp DESC".to_string(),
        _ => String::new(),
    };
    let limit = limit.clamp(1, MAX_ROWS);
    let count_sql = format!("SELECT count() FROM {tq}{where_clause}");
    let data_sql = format!("SELECT * FROM {tq}{where_clause}{order} LIMIT {limit} OFFSET {}", offset.max(0));
    let (count, page) = tokio::join!(query(config, &count_sql), query(config, &data_sql));
    let total = count
        .ok()
        .and_then(|r| r.rows.first().and_then(|row| row.first()).and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok())))
        .unwrap_or(-1);
    let page = page?;
    Ok(TableRows {
        preview_columns: Vec::new(),
        columns: page.columns,
        rows: page.rows,
        total,
        query_ms: t0.elapsed().as_millis() as u64,
        // Analytics data: browse-only, no row identity to edit by.
        primary_key: vec![],
        foreign_keys: Vec::<ForeignKeyInfo>::new(),
        sql: format!("{data_sql}\n{count_sql}"),
    })
}

/// A readable definition for the DDL view: PostHog tables have no CREATE
/// statement, so describe the columns instead.
pub async fn get_ddl(config: &PosthogConfig, table: &str) -> Result<String, String> {
    let cols = get_column_structure(config, table).await?;
    let body: Vec<String> = cols.iter().map(|c| format!("  {} {}", super::sql_util::quote_backtick(&c.name), c.data_type)).collect();
    Ok(format!("-- PostHog table (HogQL), read-only\n{} (\n{}\n)", table, body.join(",\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hogql_response_maps_columns_types_and_rows() {
        let body = json!({
            "columns": ["event", "timestamp", "distinct_id"],
            "types": [["event", "String"], ["timestamp", "DateTime64(6, 'UTC')"], ["distinct_id", "Nullable(String)"]],
            "results": [["$pageview", "2026-09-29T10:00:00Z", "u1"]],
            "hasMore": false
        });
        let r = to_sql_result(&body, "SELECT 1", 12);
        let names: Vec<&str> = r.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["event", "timestamp", "distinct_id"]);
        assert_eq!(r.columns[2].data_type, "String");
        assert!(r.columns[2].nullable && !r.columns[0].nullable);
        assert_eq!(r.rows.len(), 1);
        assert!(r.message.is_none());
    }

    #[test]
    fn schema_fields_keep_real_columns_only() {
        let tables = json!({ "events": { "name": "events", "type": "posthog", "fields": {
            "event": { "name": "event", "type": "string" },
            "properties": { "name": "properties", "type": "json" },
            "person": { "name": "person", "type": "lazy_table" },
            "pdi": { "name": "pdi", "type": "field_traverser" }
        }}});
        let names: Vec<String> = table_fields(&tables, "events").into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["event", "properties"]);
    }

    #[test]
    fn only_select_shaped_hogql_is_sent() {
        assert!(is_read_query("SELECT event FROM events"));
        assert!(is_read_query("WITH x AS (SELECT 1) SELECT * FROM x"));
        assert!(!is_read_query("DELETE FROM events"));
    }
}
