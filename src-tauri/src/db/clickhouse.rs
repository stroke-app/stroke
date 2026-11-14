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
    query_in_with(config, database, sql, false).await
}

/// `query` with ClickHouse's own `readonly=1` setting on the request: the
/// server itself refuses any write, DDL or settings change. For read-only
/// callers (the MCP server at "Read only").
pub async fn query_readonly(config: &ClickhouseConfig, sql: &str) -> Result<SqlResult, String> {
    query_in_with(config, &config.database, sql, true).await
}

async fn query_in_with(
    config: &ClickhouseConfig,
    database: &str,
    sql: &str,
    readonly: bool,
) -> Result<SqlResult, String> {
    let trimmed = sql.trim().trim_end_matches(';');
    let read = is_read_query(trimmed);
    let body = if read {
        format!("{trimmed}\nFORMAT JSONCompact")
    } else {
        trimmed.to_string()
    };

    let url = format!(
        "{}/?database={}{}",
        config.base_url(),
        urlencoding::encode(database),
        if readonly { "&readonly=1" } else { "" }
    );
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
            // system.tables keeps a metadata change time, not a creation time.
            Some(TableInfo { name, kind: kind.to_string(), row_count, rls_enabled: None, created_at: None, create_order: None, system: None })
        })
        .collect())
}

/// What stands in for indexes in ClickHouse: the primary key (the sparse
/// index every MergeTree table has), the sorting key when it is longer than the
/// primary key, and the data-skipping indices. One statement for the database.
const CH_INDEXES_SQL: &str = "\
    SELECT name AS tbl, 'primary key' AS index_name, primary_key AS expr, 'primary' AS kind, 1 AS is_primary \
    FROM system.tables WHERE database = currentDatabase() AND primary_key != '' \
    UNION ALL \
    SELECT name, 'sorting key', sorting_key, 'sorting', 0 \
    FROM system.tables WHERE database = currentDatabase() AND sorting_key != '' AND sorting_key != primary_key";
const CH_SKIP_INDEXES_SQL: &str = "\
    UNION ALL \
    SELECT table, name, expr, type, 0 FROM system.data_skipping_indices WHERE database = currentDatabase()";

pub async fn list_indexes(config: &ClickhouseConfig, schema: &str) -> Result<Vec<IndexInfo>, String> {
    let db = database_for(config, schema);
    // system.data_skipping_indices arrived in 20.x; an older server still has keys.
    let r = match query_in(config, &db, &format!("{CH_INDEXES_SQL} {CH_SKIP_INDEXES_SQL}")).await {
        Ok(r) => r,
        Err(_) => query_in(config, &db, CH_INDEXES_SQL).await?,
    };
    Ok(indexes_from_result(&r))
}

fn indexes_from_result(r: &SqlResult) -> Vec<IndexInfo> {
    let (t_i, n_i, e_i, k_i, p_i) = (col_index(r, "tbl"), col_index(r, "index_name"), col_index(r, "expr"), col_index(r, "kind"), col_index(r, "is_primary"));
    r.rows
        .iter()
        .filter_map(|row| {
            let kind = row.get(k_i).and_then(|v| v.as_str()).unwrap_or("").to_string();
            Some(IndexInfo {
                name: row.get(n_i)?.as_str()?.to_string(),
                table_name: row.get(t_i)?.as_str()?.to_string(),
                columns: row.get(e_i).and_then(|v| v.as_str()).unwrap_or("").to_string(),
                is_unique: false,
                is_primary: row.get(p_i).and_then(json_to_i64).unwrap_or(0) != 0,
                index_type: kind,
                condition: None,
                comment: None,
            })
        })
        .collect()
}

/// Columns of one table, or of every table in the database when `table` is None.
/// The type is the declared one, wrappers included (`Nullable(String)`,
/// `LowCardinality(String)`): that is the column's type in ClickHouse.
const CH_COLUMNS_SQL: &str = "\
    SELECT table, position, name, type, default_expression, comment, is_in_primary_key \
    FROM system.columns WHERE database = currentDatabase()";

fn columns_from_result(r: &SqlResult) -> Vec<(String, ColumnStructureRow)> {
    let (tbl_i, pos_i, name_i, type_i, def_i, com_i, pk_i) = (
        col_index(r, "table"),
        col_index(r, "position"),
        col_index(r, "name"),
        col_index(r, "type"),
        col_index(r, "default_expression"),
        col_index(r, "comment"),
        col_index(r, "is_in_primary_key"),
    );
    r.rows
        .iter()
        .enumerate()
        .filter_map(|(i, row)| {
            let table = row.get(tbl_i)?.as_str()?.to_string();
            let name = row.get(name_i)?.as_str()?.to_string();
            let ty = row.get(type_i).and_then(|v| v.as_str()).unwrap_or("");
            let default = row.get(def_i).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from);
            let comment = row.get(com_i).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from);
            let ordinal = row.get(pos_i).and_then(json_to_i64).unwrap_or((i + 1) as i64) as i32;
            Some((
                table,
                ColumnStructureRow {
                    ordinal_position: ordinal,
                    name,
                    data_type: ty.to_string(),
                    is_nullable: ty.starts_with("Nullable(") || ty.starts_with("LowCardinality(Nullable("),
                    column_default: default,
                    foreign_key: None,
                    fk_constraint_name: None,
                    is_primary_key: row.get(pk_i).and_then(json_to_i64).unwrap_or(0) != 0,
                    comment,
                },
            ))
        })
        .collect()
}

pub async fn get_column_structure(
    config: &ClickhouseConfig,
    schema: &str,
    table: &str,
) -> Result<Vec<ColumnStructureRow>, String> {
    let sql = format!(
        "{CH_COLUMNS_SQL} AND table = '{}' ORDER BY position",
        table.replace('\\', "\\\\").replace('\'', "\\'")
    );
    let r = query_in(config, &database_for(config, schema), &sql).await?;
    Ok(columns_from_result(&r).into_iter().map(|(_, c)| c).collect())
}

/// Every table's columns in the database, in one statement.
pub async fn schema_column_structure(
    config: &ClickhouseConfig,
    schema: &str,
) -> Result<Vec<super::schema::TableColumnStructure>, String> {
    let r = query_in(config, &database_for(config, schema), &format!("{CH_COLUMNS_SQL} ORDER BY table, position")).await?;
    let mut out: Vec<super::schema::TableColumnStructure> = Vec::new();
    for (table, col) in columns_from_result(&r) {
        match out.last_mut() {
            Some(last) if last.table == table => last.columns.push(col),
            _ => out.push(super::schema::TableColumnStructure { table, columns: vec![col] }),
        }
    }
    Ok(out)
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
        // Typed SQL filter. Already checked in `query::validate_raw_filters`
        // before this builder runs; an error here can't happen, and nothing is
        // dropped silently.
        if f.op == super::sql_util::RAW_SQL_FILTER_OP {
            let raw = f.value.as_deref().unwrap_or("");
            if let Ok(Some(expr)) = super::sql_util::raw_where_expr(raw, super::sql_util::RawFilterDialect::Literal) {
                clauses.push(expr);
            }
            continue;
        }
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

#[cfg(test)]
mod tree_tests {
    use super::*;

    fn result(cols: &[&str], rows: Vec<Vec<Value>>) -> SqlResult {
        SqlResult {
            columns: cols.iter().map(|c| ColumnInfo::new(c.to_string(), String::new())).collect(),
            rows,
            row_count: None,
            message: None,
            query_ms: 0,
            sql: String::new(),
        }
    }

    #[test]
    fn columns_keep_the_declared_type_and_primary_key() {
        let r = result(
            &["table", "position", "name", "type", "default_expression", "comment", "is_in_primary_key"],
            vec![
                vec!["hits".into(), "1".into(), "id".into(), "UInt64".into(), "".into(), "".into(), 1.into()],
                vec!["hits".into(), "2".into(), "ref".into(), "Nullable(String)".into(), "".into(), "".into(), 0.into()],
                vec!["hits".into(), "3".into(), "tag".into(), "LowCardinality(Nullable(String))".into(), "''".into(), "kind".into(), 0.into()],
            ],
        );
        let cols = columns_from_result(&r);
        assert_eq!(cols.iter().map(|(_, c)| c.data_type.as_str()).collect::<Vec<_>>(), ["UInt64", "Nullable(String)", "LowCardinality(Nullable(String))"]);
        assert!(cols[0].1.is_primary_key && !cols[0].1.is_nullable);
        assert!(cols[1].1.is_nullable && cols[2].1.is_nullable);
        assert_eq!(cols[2].1.column_default.as_deref(), Some("''"));
    }

    #[test]
    fn keys_and_skip_indices_are_indexes() {
        let r = result(
            &["tbl", "index_name", "expr", "kind", "is_primary"],
            vec![
                vec!["hits".into(), "primary key".into(), "id".into(), "primary".into(), 1.into()],
                vec!["hits".into(), "sorting key".into(), "id, ts".into(), "sorting".into(), 0.into()],
                vec!["hits".into(), "ref_bf".into(), "ref".into(), "bloom_filter".into(), 0.into()],
            ],
        );
        let idx = indexes_from_result(&r);
        assert!(idx[0].is_primary && idx[0].columns == "id");
        assert_eq!((idx[1].name.as_str(), idx[1].columns.as_str()), ("sorting key", "id, ts"));
        assert_eq!(idx[2].index_type, "bloom_filter");
    }
}
