use super::connection::DuckdbHandle;
use super::query::{ColumnInfo, ForeignKeyInfo, RowFilter, SqlResult, TableRows};
use super::schema::{ColumnStructureRow, IndexInfo, TableInfo};
use ::duckdb::types::{TimeUnit, Value as DuckValue, ValueRef};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Instant;

// DuckDB's driver is synchronous; every entry point hops onto a blocking thread
// and holds the connection mutex only for that one statement.

/// Validate a freshly-opened handle.
pub async fn ping(handle: &DuckdbHandle) -> Result<(), String> {
    let h = handle.clone();
    run_blocking(h, move |conn| {
        conn.execute_batch("SELECT 1").map_err(|e| format!("DuckDB ping failed: {e}"))
    })
    .await
}

/// Run a closure against the locked connection on a blocking thread.
async fn run_blocking<T, F>(handle: DuckdbHandle, f: F) -> Result<T, String>
where
    F: FnOnce(&::duckdb::Connection) -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let conn = handle.lock().map_err(|_| "DuckDB connection lock poisoned".to_string())?;
        f(&conn)
    })
    .await
    .map_err(|e| format!("DuckDB task failed: {e}"))?
}

// ── Type conversion ──────────────────────────────────────────────────────────

fn valueref_to_json(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Boolean(b) => Value::Bool(b),
        ValueRef::TinyInt(n) => Value::from(n),
        ValueRef::SmallInt(n) => Value::from(n),
        ValueRef::Int(n) => Value::from(n),
        ValueRef::BigInt(n) => Value::from(n),
        ValueRef::HugeInt(n) => Value::String(n.to_string()),
        ValueRef::UTinyInt(n) => Value::from(n),
        ValueRef::USmallInt(n) => Value::from(n),
        ValueRef::UInt(n) => Value::from(n),
        ValueRef::UBigInt(n) => Value::from(n),
        ValueRef::Float(f) => serde_json::Number::from_f64(f as f64).map(Value::Number).unwrap_or(Value::Null),
        ValueRef::Double(f) => serde_json::Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null),
        ValueRef::Decimal(d) => Value::String(d.to_string()),
        ValueRef::Text(bytes) => {
            // Cap oversized text/JSON - a multi-MB cell shipped whole freezes
            // the webview (see sql_util::CELL_VALUE_CAP), same guard as the
            // other engines.
            let s = String::from_utf8_lossy(bytes);
            if s.len() > super::sql_util::CELL_VALUE_CAP {
                super::sql_util::oversize_cell("text", s.len(), s.as_bytes())
            } else {
                Value::String(s.into_owned())
            }
        }
        ValueRef::Blob(bytes) => Value::String(format!("<blob: {} bytes>", bytes.len())),
        ValueRef::Date32(days) => Value::String(fmt_date(days as i64)),
        ValueRef::Timestamp(unit, v) => Value::String(fmt_timestamp(unit, v)),
        ValueRef::Time64(unit, v) => Value::String(fmt_time(unit, v)),
        other => Value::String(format!("{other:?}")),
    }
}

fn json_to_duck(v: &Value) -> DuckValue {
    match v {
        Value::Null => DuckValue::Null,
        Value::Bool(b) => DuckValue::Boolean(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                DuckValue::BigInt(i)
            } else {
                DuckValue::Double(n.as_f64().unwrap_or(0.0))
            }
        }
        Value::String(s) => DuckValue::Text(s.clone()),
        other => DuckValue::Text(other.to_string()),
    }
}

// ── Calendar helpers (chrono-free; days/secs since the Unix epoch) ─────────────

/// Howard Hinnant's civil-from-days algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn fmt_date(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Split a count of `unit`s into whole seconds + leftover nanoseconds.
fn to_secs_nanos(unit: TimeUnit, v: i64) -> (i64, i64) {
    match unit {
        TimeUnit::Second => (v, 0),
        TimeUnit::Millisecond => (v.div_euclid(1_000), v.rem_euclid(1_000) * 1_000_000),
        TimeUnit::Microsecond => (v.div_euclid(1_000_000), v.rem_euclid(1_000_000) * 1_000),
        TimeUnit::Nanosecond => (v.div_euclid(1_000_000_000), v.rem_euclid(1_000_000_000)),
    }
}

fn fmt_timestamp(unit: TimeUnit, v: i64) -> String {
    let (secs, ns) = to_secs_nanos(unit, v);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (y, mo, d) = civil_from_days(days);
    let (h, mi, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    if ns > 0 {
        format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}.{:06}", ns / 1000)
    } else {
        format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}")
    }
}

fn fmt_time(unit: TimeUnit, v: i64) -> String {
    let (secs, ns) = to_secs_nanos(unit, v);
    let tod = secs.rem_euclid(86_400);
    let (h, mi, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    if ns > 0 {
        format!("{h:02}:{mi:02}:{s:02}.{:06}", ns / 1000)
    } else {
        format!("{h:02}:{mi:02}:{s:02}")
    }
}

// ── Core query helpers (run inside the lock) ──────────────────────────────────

/// Run a row-returning statement and collect columns + rows.
fn collect_rows(
    conn: &::duckdb::Connection,
    sql: &str,
    params: &[DuckValue],
) -> Result<(Vec<ColumnInfo>, Vec<Vec<Value>>), String> {
    let mut stmt = conn.prepare(sql).map_err(|e| format!("DuckDB error: {e}"))?;

    // column_names()/column_count() panic before the statement is executed, so we
    // run query() first (it executes eagerly) and read metadata via rows.as_ref().
    let mut rows = stmt
        .query(::duckdb::params_from_iter(params.iter()))
        .map_err(|e| format!("DuckDB error: {e}"))?;

    let (col_names, col_count): (Vec<String>, usize) = match rows.as_ref() {
        Some(s) => (s.column_names(), s.column_count()),
        None => (Vec::new(), 0),
    };

    let mut out: Vec<Vec<Value>> = Vec::new();
    while let Some(row) = rows.next().map_err(|e| format!("DuckDB row error: {e}"))? {
        let mut r = Vec::with_capacity(col_count);
        for i in 0..col_count {
            let vref = row.get_ref(i).map_err(|e| format!("DuckDB cell error: {e}"))?;
            r.push(valueref_to_json(vref));
        }
        out.push(r);
    }

    let columns = col_names.into_iter().map(|n| ColumnInfo::new(n, "")).collect();
    Ok((columns, out))
}

fn is_read_query(sql: &str) -> bool {
    let head = super::sql_util::statement_head(sql);
    matches!(
        head.as_str(),
        "select" | "with" | "show" | "describe" | "desc" | "pragma" | "explain" | "values" | "table" | "from"
    )
}

fn quote_ident(ident: &str) -> String {
    super::sql_util::quote_double(ident)
}

/// Escape LIKE/ILIKE wildcards (`%`, `_`, `\`) so user input matches literally
/// (used with `ESCAPE '\'`).
fn escape_like(input: &str) -> String {
    super::sql_util::escape_like_backslash(input)
}

// ── Public API ─────────────────────────────────────────────────────────────────

pub async fn execute_sql(handle: &DuckdbHandle, sql: &str) -> Result<SqlResult, String> {
    let h = handle.clone();
    let sql = sql.trim().to_string();
    let t0 = Instant::now();
    let read = is_read_query(&sql);

    run_blocking(h, move |conn| {
        if read {
            let (columns, rows) = collect_rows(conn, &sql, &[])?;
            let row_count = Some(rows.len() as i64);
            Ok(SqlResult { columns, rows, row_count, message: None, query_ms: t0.elapsed().as_millis() as u64, sql: sql.clone() })
        } else {
            let affected = conn.execute(&sql, []).map_err(|e| format!("DuckDB error: {e}"))?;
            Ok(SqlResult {
                columns: vec![],
                rows: vec![],
                row_count: Some(affected as i64),
                message: Some(format!("{affected} row(s) affected")),
                query_ms: t0.elapsed().as_millis() as u64,
                sql: sql.clone(),
            })
        }
    })
    .await
}

pub async fn list_tables(handle: &DuckdbHandle) -> Result<Vec<TableInfo>, String> {
    let h = handle.clone();
    run_blocking(h, move |conn| {
        let (_c, rows) = collect_rows(
            conn,
            "SELECT table_name, table_type FROM information_schema.tables \
             WHERE table_schema = 'main' ORDER BY table_name",
            &[],
        )?;
        let mut tables = Vec::new();
        for row in &rows {
            let name = match row.first().and_then(|v| v.as_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            let ttype = row.get(1).and_then(|v| v.as_str()).unwrap_or("BASE TABLE");
            let kind = if ttype.eq_ignore_ascii_case("VIEW") { "view" } else { "table" };
            // views → 0; base tables get -1 as a "needs count" sentinel, filled below.
            let row_count = if kind == "view" { 0 } else { -1 };
            tables.push(TableInfo { name, kind: kind.to_string(), row_count, rls_enabled: None });
        }

        // Batch all COUNT(*)s into one UNION ALL statement per chunk instead of a
        // separate query per table (the whole list runs under a single lock).
        const COUNT_BATCH: usize = 200;
        let count_idx: Vec<usize> = tables
            .iter()
            .enumerate()
            .filter(|(_, t)| t.row_count < 0)
            .map(|(i, _)| i)
            .collect();
        for chunk in count_idx.chunks(COUNT_BATCH) {
            let sql = chunk
                .iter()
                .map(|&i| format!("SELECT count(*) FROM {}", quote_ident(&tables[i].name)))
                .collect::<Vec<_>>()
                .join(" UNION ALL ");
            match collect_rows(conn, &sql, &[]) {
                Ok((_, r)) if r.len() == chunk.len() => {
                    for (&i, row) in chunk.iter().zip(r.iter()) {
                        tables[i].row_count =
                            row.first().and_then(json_to_i64).unwrap_or(0);
                    }
                }
                _ => {
                    for &i in chunk {
                        let one = format!("SELECT count(*) FROM {}", quote_ident(&tables[i].name));
                        tables[i].row_count = collect_rows(conn, &one, &[])
                            .ok()
                            .and_then(|(_, r)| r.first().and_then(|row| row.first()).and_then(json_to_i64))
                            .unwrap_or(0);
                    }
                }
            }
        }
        Ok(tables)
    })
    .await
}

pub async fn list_indexes(handle: &DuckdbHandle) -> Result<Vec<IndexInfo>, String> {
    let h = handle.clone();
    run_blocking(h, move |conn| {
        let (_c, rows) = collect_rows(
            conn,
            "SELECT index_name, table_name, is_unique FROM duckdb_indexes() ORDER BY table_name, index_name",
            &[],
        )?;
        Ok(rows
            .iter()
            .filter_map(|r| {
                let name = r.first()?.as_str()?.to_string();
                let table_name = r.get(1)?.as_str()?.to_string();
                let is_unique = r.get(2).and_then(|v| v.as_bool()).unwrap_or(false);
                Some(IndexInfo {
                    name,
                    table_name,
                    columns: String::new(),
                    index_type: "art".to_string(),
                    is_unique,
                    is_primary: false,
                    condition: None,
                    comment: None,
                })
            })
            .collect())
    })
    .await
}

pub async fn get_column_structure(handle: &DuckdbHandle, table: &str) -> Result<Vec<ColumnStructureRow>, String> {
    let h = handle.clone();
    let table = table.to_string();
    run_blocking(h, move |conn| {
        let (_c, rows) = collect_rows(
            conn,
            "SELECT column_index, column_name, data_type, is_nullable, column_default \
             FROM duckdb_columns() WHERE table_name = ? AND schema_name = 'main' ORDER BY column_index",
            &[DuckValue::Text(table.clone())],
        )?;
        Ok(rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| {
                let name = r.get(1)?.as_str()?.to_string();
                let data_type = r.get(2).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let is_nullable = r.get(3).and_then(|v| v.as_bool()).unwrap_or(true);
                let column_default = r.get(4).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from);
                let ordinal = r.first().and_then(json_to_i64).unwrap_or((i + 1) as i64) as i32;
                Some(ColumnStructureRow {
                    ordinal_position: ordinal,
                    name,
                    data_type,
                    is_nullable,
                    column_default,
                    foreign_key: None,
                    fk_constraint_name: None,
                    is_primary_key: false,
                    comment: None,
                })
            })
            .collect())
    })
    .await
}

pub async fn get_ddl(handle: &DuckdbHandle, table: &str) -> Result<String, String> {
    let h = handle.clone();
    let table = table.to_string();
    run_blocking(h, move |conn| {
        let ddl = collect_rows(
            conn,
            "SELECT sql FROM duckdb_tables() WHERE table_name = ? AND schema_name = 'main'",
            &[DuckValue::Text(table.clone())],
        )
        .ok()
        .and_then(|(_, r)| r.first().and_then(|row| row.first()).and_then(|v| v.as_str()).map(String::from))
        .unwrap_or_default();
        Ok(ddl)
    })
    .await
}

/// Primary-key columns for a table (empty if none).
/// `constraint_column_names` is a LIST, so unnest it to one scalar row per column.
fn primary_key(conn: &::duckdb::Connection, table: &str) -> Vec<String> {
    let res = collect_rows(
        conn,
        "SELECT unnest(constraint_column_names) AS col FROM duckdb_constraints() \
         WHERE table_name = ? AND schema_name = 'main' AND constraint_type = 'PRIMARY KEY'",
        &[DuckValue::Text(table.to_string())],
    );
    match res {
        Ok((_, rows)) => rows.iter().filter_map(|r| r.first().and_then(|v| v.as_str()).map(String::from)).collect(),
        Err(_) => Vec::new(),
    }
}

pub async fn get_table_rows(
    handle: &DuckdbHandle,
    table: &str,
    limit: i64,
    offset: i64,
    search: Option<String>,
    sort_column: Option<String>,
    sort_direction: Option<String>,
    filters: Option<Vec<RowFilter>>,
    // When false, skip the primary-key lookup - the frontend keeps it across
    // repeat fetches (pagination/sort/filter). The column list is still fetched
    // because sort-column validation depends on it.
    include_meta: bool,
) -> Result<TableRows, String> {
    let h = handle.clone();
    let table = table.to_string();
    let t0 = Instant::now();

    run_blocking(h, move |conn| {
        let tq = quote_ident(&table);
        let cols = {
            let (_c, rows) = collect_rows(
                conn,
                "SELECT column_name FROM duckdb_columns() WHERE table_name = ? AND schema_name = 'main' ORDER BY column_index",
                &[DuckValue::Text(table.clone())],
            )?;
            rows.iter().filter_map(|r| r.first().and_then(|v| v.as_str()).map(String::from)).collect::<Vec<_>>()
        };

        let (where_clause, params) = build_where(&cols, search.as_deref(), filters.as_deref());

        let count_sql = format!("SELECT count(*) FROM {tq}{where_clause}");
        let total = collect_rows(conn, &count_sql, &params)?
            .1
            .first()
            .and_then(|r| r.first())
            .and_then(json_to_i64)
            .unwrap_or(0);

        let order = match sort_column.as_deref() {
            Some(c) if !c.trim().is_empty() && cols.iter().any(|k| k == c) => {
                let dir = if sort_direction.as_deref().map(|d| d.eq_ignore_ascii_case("desc")).unwrap_or(false) {
                    "DESC"
                } else {
                    "ASC"
                };
                format!(" ORDER BY {} {dir}", quote_ident(c))
            }
            _ => String::new(),
        };

        let data_sql = format!("SELECT * FROM {tq}{where_clause}{order} LIMIT {limit} OFFSET {offset}");
        let (columns, rows) = collect_rows(conn, &data_sql, &params)?;

        Ok(TableRows {
        // Preview fetching is a Postgres path (pg_stats + pg_column_size).
        preview_columns: Vec::new(),
            columns,
            rows,
            total,
            query_ms: t0.elapsed().as_millis() as u64,
            primary_key: if include_meta { primary_key(conn, &table) } else { Vec::new() },
            foreign_keys: Vec::<ForeignKeyInfo>::new(),
            sql: format!("{data_sql}\n{count_sql}"),
        })
    })
    .await
}

/// Build a parameterized WHERE clause from the global search box + structured filters.
fn build_where(
    cols: &[String],
    search: Option<&str>,
    filters: Option<&[RowFilter]>,
) -> (String, Vec<DuckValue>) {
    let known: std::collections::HashSet<&str> = cols.iter().map(|s| s.as_str()).collect();
    let mut clauses: Vec<String> = Vec::new();
    let mut params: Vec<DuckValue> = Vec::new();

    if let Some(q) = search.map(str::trim).filter(|s| !s.is_empty()) {
        let ors: Vec<String> = cols
            .iter()
            .map(|c| {
                params.push(DuckValue::Text(format!("%{}%", escape_like(q))));
                format!("CAST({} AS VARCHAR) ILIKE ? ESCAPE '\\'", quote_ident(c))
            })
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
        let v = f.value.clone().unwrap_or_default();
        let clause = match f.op.as_str() {
            // No CAST for eq/ne: bind the value and let DuckDB coerce types, so numeric
            // equality compares natively (05 == 5) and typed indexes stay usable.
            "=" | "eq" => { params.push(DuckValue::Text(v)); format!("{col} = ?") }
            "!=" | "ne" | "neq" => { params.push(DuckValue::Text(v)); format!("{col} != ?") }
            ">" | "gt" => { params.push(DuckValue::Text(v)); format!("{col} > ?") }
            ">=" | "gte" => { params.push(DuckValue::Text(v)); format!("{col} >= ?") }
            "<" | "lt" => { params.push(DuckValue::Text(v)); format!("{col} < ?") }
            "<=" | "lte" => { params.push(DuckValue::Text(v)); format!("{col} <= ?") }
            "contains" => { params.push(DuckValue::Text(format!("%{}%", escape_like(&v)))); format!("CAST({col} AS VARCHAR) ILIKE ? ESCAPE '\\'") }
            "not_contains" => { params.push(DuckValue::Text(format!("%{}%", escape_like(&v)))); format!("({col} IS NULL OR NOT (CAST({col} AS VARCHAR) ILIKE ? ESCAPE '\\'))") }
            "starts_with" => { params.push(DuckValue::Text(format!("{}%", escape_like(&v)))); format!("CAST({col} AS VARCHAR) ILIKE ? ESCAPE '\\'") }
            "ends_with" => { params.push(DuckValue::Text(format!("%{}", escape_like(&v)))); format!("CAST({col} AS VARCHAR) ILIKE ? ESCAPE '\\'") }
            "between" => {
                let mut parts = v.splitn(2, ',');
                let from = parts.next().unwrap_or("").trim().to_string();
                let to = parts.next().unwrap_or("").trim().to_string();
                params.push(DuckValue::Text(from));
                params.push(DuckValue::Text(to));
                format!("{col} BETWEEN ? AND ?")
            }
            "is_null" => format!("{col} IS NULL"),
            "is_not_null" => format!("{col} IS NOT NULL"),
            _ => continue,
        };
        clauses.push(clause);
    }

    if clauses.is_empty() {
        (String::new(), params)
    } else {
        (format!(" WHERE {}", clauses.join(" AND ")), params)
    }
}

// ── Row editing ────────────────────────────────────────────────────────────────

pub async fn update_table_cell(
    handle: &DuckdbHandle,
    table: &str,
    primary_key: HashMap<String, Value>,
    column: &str,
    value: &Value,
) -> Result<(), String> {
    if primary_key.is_empty() {
        return Err("Cannot update row: table has no primary key".into());
    }
    let h = handle.clone();
    let (table, column, value) = (table.to_string(), column.to_string(), value.clone());
    run_blocking(h, move |conn| {
        let mut params: Vec<DuckValue> = vec![json_to_duck(&value)];
        let (where_sql, pk_params) = pk_where(&primary_key);
        params.extend(pk_params);
        let sql = format!("UPDATE {} SET {} = ?{}", quote_ident(&table), quote_ident(&column), where_sql);
        let n = conn
            .execute(&sql, ::duckdb::params_from_iter(params.iter()))
            .map_err(|e| format!("DuckDB update failed: {e}"))?;
        if n == 0 {
            return Err("No row updated (row may have changed)".into());
        }
        Ok(())
    })
    .await
}

pub async fn insert_table_row(
    handle: &DuckdbHandle,
    table: &str,
    values: HashMap<String, Value>,
) -> Result<Vec<Value>, String> {
    let h = handle.clone();
    let table = table.to_string();
    run_blocking(h, move |conn| {
        let cols: Vec<String> = values.keys().cloned().collect();
        let placeholders = vec!["?"; cols.len()].join(", ");
        let col_list = cols.iter().map(|c| quote_ident(c)).collect::<Vec<_>>().join(", ");
        let params: Vec<DuckValue> = cols.iter().map(|c| json_to_duck(&values[c])).collect();
        let sql = format!(
            "INSERT INTO {} ({col_list}) VALUES ({placeholders}) RETURNING *",
            quote_ident(&table)
        );
        let (_c, rows) = collect_rows(conn, &sql, &params)?;
        Ok(rows.into_iter().next().unwrap_or_default())
    })
    .await
}

pub async fn delete_table_rows(
    handle: &DuckdbHandle,
    table: &str,
    primary_keys: Vec<HashMap<String, Value>>,
) -> Result<u64, String> {
    if primary_keys.is_empty() {
        return Ok(0);
    }
    let h = handle.clone();
    let table = table.to_string();
    run_blocking(h, move |conn| {
        let mut deleted = 0u64;
        for pk in &primary_keys {
            if pk.is_empty() {
                return Err("Cannot delete rows: table has no primary key".into());
            }
            let (where_sql, params) = pk_where(pk);
            let sql = format!("DELETE FROM {}{}", quote_ident(&table), where_sql);
            deleted += conn
                .execute(&sql, ::duckdb::params_from_iter(params.iter()))
                .map_err(|e| format!("DuckDB delete failed: {e}"))? as u64;
        }
        Ok(deleted)
    })
    .await
}

/// Build a ` WHERE col1 = ? AND col2 = ?` clause + params from a primary-key map.
fn pk_where(pk: &HashMap<String, Value>) -> (String, Vec<DuckValue>) {
    let mut clauses = Vec::new();
    let mut params = Vec::new();
    for (col, val) in pk {
        clauses.push(format!("{} = ?", quote_ident(col)));
        params.push(json_to_duck(val));
    }
    (format!(" WHERE {}", clauses.join(" AND ")), params)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn json_to_i64(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn handle() -> DuckdbHandle {
        Arc::new(Mutex::new(::duckdb::Connection::open_in_memory().unwrap()))
    }

    #[tokio::test]
    async fn introspection_roundtrip() {
        let h = handle();
        execute_sql(&h, "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, born DATE)")
            .await
            .unwrap();
        execute_sql(&h, "INSERT INTO users VALUES (1, 'ada', DATE '1815-12-10'), (2, 'alan', DATE '1912-06-23')")
            .await
            .unwrap();

        // list_tables must surface the new table with a real row count.
        let tables = list_tables(&h).await.unwrap();
        assert!(tables.iter().any(|t| t.name == "users" && t.row_count == 2), "tables: {tables:?}");

        // columns
        let cols = get_column_structure(&h, "users").await.unwrap();
        assert_eq!(cols.len(), 3, "cols: {cols:?}");
        assert_eq!(cols[0].name, "id");

        // primary key + row browsing
        let rows = get_table_rows(&h, "users", 10, 0, None, None, None, None, true).await.unwrap();
        assert_eq!(rows.total, 2);
        assert_eq!(rows.rows.len(), 2);
        assert_eq!(rows.primary_key, vec!["id".to_string()], "pk: {:?}", rows.primary_key);
        // the DATE column must render as an ISO string, not a debug blob
        let date_cell = &rows.rows[0][2];
        assert!(date_cell.as_str().map(|s| s.starts_with("18")).unwrap_or(false), "date: {date_cell:?}");

        // indexes (PK shows up as an ART index in duckdb_indexes or not at all - must not error)
        let _ = list_indexes(&h).await.unwrap();

        // editing
        update_table_cell(&h, "users", HashMap::from([("id".into(), Value::from(1))]), "name", &Value::from("ada2"))
            .await
            .unwrap();
        let deleted = delete_table_rows(&h, "users", vec![HashMap::from([("id".into(), Value::from(2))])])
            .await
            .unwrap();
        assert_eq!(deleted, 1);
        let after = get_table_rows(&h, "users", 10, 0, None, None, None, None, true).await.unwrap();
        assert_eq!(after.total, 1);
    }
}
