use super::connection::ClickhouseConfig;
use super::query::{ColumnInfo, ForeignKeyInfo, RowFilter, SqlResult, TableRows};
use super::schema::{ColumnStructureRow, IndexInfo, TableInfo};
use serde::Deserialize;
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Instant;

// ── Shared HTTP client ─────────────────────────────────────────────────────────
// One client for the whole process lifetime. reqwest keeps an internal connection
// pool so subsequent requests to the same host reuse the TCP/TLS session.

static CH_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn client() -> &'static reqwest::Client {
    CH_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            // Bounded, but generously: this carries queries, not just metadata.
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("failed to build ClickHouse HTTP client")
    })
}

// ── JSONCompact response shape ───────────────────────────────────────────────
// ClickHouse returns:
//   { "meta": [{"name": "...", "type": "..."}], "data": [[...], ...], "rows": N }

#[derive(Deserialize)]
struct JsonCompact {
    #[serde(default)]
    meta: Vec<MetaCol>,
    #[serde(default)]
    data: Vec<Vec<Value>>,
}

#[derive(Deserialize)]
struct MetaCol {
    name: String,
    #[serde(rename = "type")]
    ty: String,
}

/// A SQL statement that returns a result set (so we can ask for JSONCompact).
fn is_read_query(sql: &str) -> bool {
    let head = super::sql_util::statement_head(sql);
    matches!(
        head.as_str(),
        "select" | "with" | "show" | "describe" | "desc" | "explain" | "values" | "exists"
    )
}

// ── Core HTTP query ──────────────────────────────────────────────────────────

/// POST a single statement to the ClickHouse HTTP interface. Read queries are
/// wrapped with `FORMAT JSONCompact` and parsed into columns + rows; write/DDL
/// statements return an empty result with a success message.
pub async fn query(config: &ClickhouseConfig, sql: &str) -> Result<SqlResult, String> {
    query_in(config, &config.database, sql).await
}

/// Which database a statement runs against: the schema the sidebar has selected,
/// or the connection's own when that is blank.
///
/// ClickHouse databases are what the app calls schemas, and everything here used
/// to be pinned to the one named on the connection - `list_schemas` returned
/// `[config.database]` and nothing else, and every query resolved
/// `currentDatabase()` to it. A connection opened on `default` therefore showed
/// "Nothing in default yet" with no way to reach the databases that did have
/// tables, because the switcher had a single entry.
fn database_for(config: &ClickhouseConfig, schema: &str) -> String {
    let s = schema.trim();
    if s.is_empty() { config.database.clone() } else { s.to_string() }
}

/// `query`, against a named database. The database goes on the URL rather than
/// into the SQL, so `currentDatabase()` and unqualified table names both resolve
/// to it and no statement here needs to be rewritten.
pub async fn query_in(
    config: &ClickhouseConfig,
    database: &str,
    sql: &str,
) -> Result<SqlResult, String> {
    let trimmed = sql.trim().trim_end_matches(';');
    let read = is_read_query(trimmed);
    let body = if read {
        format!("{trimmed}\nFORMAT JSONCompact")
    } else {
        trimmed.to_string()
    };

    let url = format!("{}/?database={}", config.base_url(), urlencoding::encode(database));
    let t0 = Instant::now();
    let res = client()
        .post(&url)
        .header("X-ClickHouse-User", &config.user)
        .header("X-ClickHouse-Key", &config.password)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("ClickHouse request failed: {e}"))?;

    let status = res.status();
    let elapsed = t0.elapsed().as_millis() as u64;

    if !status.is_success() {
        let msg = res.text().await.unwrap_or_default();
        // ClickHouse error bodies start with "Code: NN. DB::Exception: ...".
        return Err(format!("ClickHouse error: {}", msg.trim().chars().take(500).collect::<String>()));
    }

    if !read {
        // INSERT / CREATE / ALTER / DROP - success, no result set.
        return Ok(SqlResult {
            columns: vec![],
            rows: vec![],
            row_count: Some(0),
            message: Some("Statement executed".to_string()),
            query_ms: elapsed,
            sql: sql.to_string(),
        });
    }

    let text = res.text().await.map_err(|e| format!("Failed to read ClickHouse response: {e}"))?;
    if text.trim().is_empty() {
        return Ok(SqlResult { columns: vec![], rows: vec![], row_count: Some(0), message: None, query_ms: elapsed, sql: sql.to_string() });
    }
    let parsed: JsonCompact =
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse ClickHouse response: {e}"))?;

    let columns: Vec<ColumnInfo> = parsed
        .meta
        .iter()
        .map(|m| {
            let mut c = ColumnInfo::new(m.name.clone(), ch_type_label(&m.ty));
            c.nullable = m.ty.starts_with("Nullable(");
            c
        })
        .collect();

    let row_count = Some(parsed.data.len() as i64);
    // Cap oversized cells - a multi-MB String/JSON value shipped whole freezes
    // the webview (see sql_util::CELL_VALUE_CAP); small scalars pass through.
    let rows: Vec<Vec<Value>> = parsed
        .data
        .into_iter()
        .map(|row| row.into_iter().map(|v| super::sql_util::cap_json_value("text", v)).collect())
        .collect();
    Ok(SqlResult { columns, rows, row_count, message: None, query_ms: elapsed, sql: sql.to_string() })
}

/// Trim ClickHouse `Nullable(...)`/`LowCardinality(...)` wrappers for display.
fn ch_type_label(ty: &str) -> String {
    let mut t = ty;
    for wrapper in ["Nullable(", "LowCardinality("] {
        if let Some(inner) = t.strip_prefix(wrapper).and_then(|s| s.strip_suffix(')')) {
            t = inner;
        }
    }
    t.to_string()
}

/// Quote a ClickHouse identifier with backticks.
fn quote_ident(ident: &str) -> String {
    super::sql_util::quote_backtick(ident)
}

// ── Schema introspection ───────────────────────────────────────────────────────

/// Every database on the server, which is what the sidebar's schema switcher
/// offers. `system` is included deliberately - it is where ClickHouse keeps the
/// tables worth looking at when something is wrong, and the switcher marks a
/// system schema with its own icon.
pub async fn list_schemas(config: &ClickhouseConfig) -> Result<Vec<String>, String> {
    let r = query(config, "SELECT name FROM system.databases ORDER BY name").await?;
    let name_i = col_index(&r, "name");
    let mut names: Vec<String> = r
        .rows
        .iter()
        .filter_map(|row| Some(row.get(name_i)?.as_str()?.to_string()))
        .collect();
    // A server that answers but lists nothing still has the one we are on.
    if names.is_empty() {
        names.push(config.database.clone());
    }
    Ok(names)
}

pub async fn list_tables(config: &ClickhouseConfig, schema: &str) -> Result<Vec<TableInfo>, String> {
    let sql = "SELECT name, engine, coalesce(total_rows, 0) AS rows \
               FROM system.tables WHERE database = currentDatabase() ORDER BY name";
    let r = query_in(config, &database_for(config, schema), sql).await?;
    let name_i = col_index(&r, "name");
    let eng_i = col_index(&r, "engine");
    let rows_i = col_index(&r, "rows");
    Ok(r
        .rows
        .iter()
        .filter_map(|row| {
            let name = row.get(name_i)?.as_str()?.to_string();
            let engine = row.get(eng_i).and_then(|v| v.as_str()).unwrap_or("");
            let kind = if engine.contains("View") { "view" } else { "table" };
            let row_count = row.get(rows_i).and_then(json_to_i64).unwrap_or(-1);
            Some(TableInfo { name, kind: kind.to_string(), row_count, rls_enabled: None })
        })
        .collect())
}

pub async fn list_indexes(_config: &ClickhouseConfig) -> Result<Vec<IndexInfo>, String> {
    // ClickHouse has no classic secondary indexes; sorting keys / data-skipping
    // indices don't map onto the IndexInfo model, so report none.
    Ok(vec![])
}

pub async fn get_column_structure(
    config: &ClickhouseConfig,
    schema: &str,
    table: &str,
) -> Result<Vec<ColumnStructureRow>, String> {
    let sql = format!(
        "SELECT position, name, type, default_expression, comment \
         FROM system.columns WHERE database = currentDatabase() AND table = '{}' ORDER BY position",
        table.replace('\'', "\\'")
    );
    let r = query_in(config, &database_for(config, schema), &sql).await?;
    let pos_i = col_index(&r, "position");
    let name_i = col_index(&r, "name");
    let type_i = col_index(&r, "type");
    let def_i = col_index(&r, "default_expression");
    let com_i = col_index(&r, "comment");
    Ok(r
        .rows
        .iter()
        .enumerate()
        .filter_map(|(i, row)| {
            let name = row.get(name_i)?.as_str()?.to_string();
            let ty = row.get(type_i).and_then(|v| v.as_str()).unwrap_or("");
            let default = row.get(def_i).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from);
            let comment = row.get(com_i).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from);
            let ordinal = row.get(pos_i).and_then(json_to_i64).unwrap_or((i + 1) as i64) as i32;
            Some(ColumnStructureRow {
                ordinal_position: ordinal,
                name,
                data_type: ch_type_label(ty),
                is_nullable: ty.starts_with("Nullable("),
                column_default: default,
                foreign_key: None,
                fk_constraint_name: None,
                is_primary_key: false,
                comment,
            })
        })
        .collect())
}

pub async fn get_ddl(config: &ClickhouseConfig, schema: &str, table: &str) -> Result<String, String> {
    let sql = format!("SHOW CREATE TABLE {}", quote_ident(table));
    let r = query_in(config, &database_for(config, schema), &sql).await?;
    Ok(r.rows
        .first()
        .and_then(|row| row.first())
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string())
}

// ── Data browsing ──────────────────────────────────────────────────────────────

pub async fn get_table_rows(
    config: &ClickhouseConfig,
    schema: &str,
    table: &str,
    limit: i64,
    offset: i64,
    search: Option<String>,
    sort_column: Option<String>,
    sort_direction: Option<String>,
    filters: Option<Vec<RowFilter>>,
    // ClickHouse is OLAP/browse-only (no PK/FK to return), so the column-structure
    // round-trip is only needed to build a WHERE clause. Skip it on plain
    // pagination where there is no search/filter. `include_meta` forces the fetch
    // so a first load still validates against the real column list.
    include_meta: bool,
) -> Result<TableRows, String> {
    let t0 = Instant::now();
    let tq = quote_ident(table);

    let has_search = search.as_deref().map(str::trim).is_some_and(|s| !s.is_empty());
    let has_filters = filters.as_ref().is_some_and(|f| !f.is_empty());
    let cols = if include_meta || has_search || has_filters {
        get_column_structure(config, schema, table).await?
    } else {
        Vec::new()
    };
    let where_clause = build_where(&cols, search.as_deref(), filters.as_deref());

    let order = match (sort_column.as_deref(), sort_direction.as_deref()) {
        (Some(c), dir) if !c.trim().is_empty() => {
            let d = if dir.map(|d| d.eq_ignore_ascii_case("desc")).unwrap_or(false) { "DESC" } else { "ASC" };
            format!(" ORDER BY {} {d}", quote_ident(c))
        }
        _ => String::new(),
    };

    // Count (respecting filters) and page are independent - run the two HTTP
    // round-trips concurrently, matching the pg and D1 paths.
    let count_sql = format!("SELECT count() FROM {tq}{where_clause}");
    let data_sql = format!("SELECT * FROM {tq}{where_clause}{order} LIMIT {limit} OFFSET {offset}");
    let db = database_for(config, schema);
    let (count_res, data_res) =
        tokio::join!(query_in(config, &db, &count_sql), query_in(config, &db, &data_sql));
    let total = count_res?
        .rows
        .first()
        .and_then(|r| r.first())
        .and_then(json_to_i64)
        .unwrap_or(0);
    let result = data_res?;

    Ok(TableRows {
        // Preview fetching is a Postgres path (pg_stats + pg_column_size).
        preview_columns: Vec::new(),
        columns: result.columns,
        rows: result.rows,
        total,
        query_ms: t0.elapsed().as_millis() as u64,
        // ClickHouse is OLAP: no primary-key based row editing, so the grid is browse-only.
        primary_key: vec![],
        foreign_keys: Vec::<ForeignKeyInfo>::new(),
        sql: format!("{data_sql}\n{count_sql}"),
    })
}

/// Build a `WHERE` clause from the global search box + structured filters.
/// Values are escaped into single-quoted literals (ClickHouse HTTP has no bound
/// params here); identifiers are validated against the known column list.
pub(crate) fn build_where(cols: &[ColumnStructureRow], search: Option<&str>, filters: Option<&[RowFilter]>) -> String {
    let known: std::collections::HashSet<&str> = cols.iter().map(|c| c.name.as_str()).collect();
    let mut clauses: Vec<String> = Vec::new();

    if let Some(q) = search.map(str::trim).filter(|s| !s.is_empty()) {
        let needle = esc_literal(q);
        let ors: Vec<String> = cols
            .iter()
            .map(|c| format!("positionCaseInsensitive(toString({}), '{needle}') > 0", quote_ident(&c.name)))
            .collect();
        if !ors.is_empty() {
            clauses.push(format!("({})", ors.join(" OR ")));
        }
    }

    for f in filters.unwrap_or(&[]) {
        if !known.contains(f.column.as_str()) {
            continue;
        }
        let col = quote_ident(&f.column);
        let v = f.value.as_deref().unwrap_or("");
        let lit = esc_literal(v);
        let clause = match f.op.as_str() {
            "=" | "eq" => format!("{col} = '{lit}'"),
            "!=" | "ne" | "neq" => format!("{col} != '{lit}'"),
            ">" | "gt" => format!("{col} > '{lit}'"),
            ">=" | "gte" => format!("{col} >= '{lit}'"),
            "<" | "lt" => format!("{col} < '{lit}'"),
            "<=" | "lte" => format!("{col} <= '{lit}'"),
            "contains" => format!("positionCaseInsensitive(toString({col}), '{lit}') > 0"),
            // A NULL cell contains nothing, so it satisfies "does not contain".
            "not_contains" => format!("({col} IS NULL OR NOT (positionCaseInsensitive(toString({col}), '{lit}') > 0))"),
            "starts_with" => format!("positionCaseInsensitive(toString({col}), '{lit}') = 1"),
            "ends_with" => format!("endsWith(lowerUTF8(toString({col})), lowerUTF8('{lit}'))"),
            "between" => {
                let mut parts = v.splitn(2, ',');
                let from = esc_literal(parts.next().unwrap_or("").trim());
                let to = esc_literal(parts.next().unwrap_or("").trim());
                format!("{col} BETWEEN '{from}' AND '{to}'")
            }
            "is_null" => format!("{col} IS NULL"),
            "is_not_null" => format!("{col} IS NOT NULL"),
            _ => continue,
        };
        clauses.push(clause);
    }

    if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    }
}

fn esc_literal(s: &str) -> String {
    super::sql_util::esc_backslash_quote(s)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn col_index(r: &SqlResult, name: &str) -> usize {
    r.columns.iter().position(|c| c.name == name).unwrap_or(0)
}

/// ClickHouse JSONCompact returns 64-bit ints as quoted strings by default;
/// accept both number and string forms.
fn json_to_i64(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
}
