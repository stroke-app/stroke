use super::query::{ColumnInfo, ForeignKeyInfo, SqlResult, TableRows};
use futures::TryStreamExt;
use serde_json::{json, Value};
use sqlx::{Column, Row, SqlitePool, TypeInfo};
use std::time::Instant;

const EXECUTE_SQL_MAX_ROWS: usize = 1_000_000_000;

// ── Cell conversion ───────────────────────────────────────────────────────────

pub fn cell_to_json(row: &sqlx::sqlite::SqliteRow, idx: usize) -> Value {
    // Try types in order matching SQLite's type affinity rules
    if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
        return v.map(|n| json!(n)).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(idx) {
        return v.map(|n| json!(n)).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
        return match v {
            // Cap oversized text/JSON - a multi-MB cell shipped whole freezes
            // the webview (see sql_util::CELL_VALUE_CAP).
            Some(s) if s.len() > super::sql_util::CELL_VALUE_CAP => {
                super::sql_util::oversize_cell(&col_type(row, idx), s.len(), s.as_bytes())
            }
            Some(s) => json!(s),
            None => Value::Null,
        };
    }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(idx) {
        return match v {
            // Hex display doubles the byte count - cap blobs the same way so a
            // file stored in a blob column can't ship megabytes of hex.
            Some(b) if b.len() * 2 > super::sql_util::CELL_VALUE_CAP => {
                let head: String = b
                    .iter()
                    .take(super::sql_util::CELL_PREVIEW_BYTES / 2)
                    .map(|x| format!("{x:02x}"))
                    .collect();
                super::sql_util::oversize_cell("blob", b.len(), head.as_bytes())
            }
            Some(b) => Value::String(b.iter().map(|x| format!("{x:02x}")).collect()),
            None => Value::Null,
        };
    }
    Value::Null
}

fn col_type(row: &sqlx::sqlite::SqliteRow, idx: usize) -> String {
    row.column(idx).type_info().name().to_lowercase()
}

// ── Primary-key helpers ───────────────────────────────────────────────────────

/// Returns PK column names in key-sequence order using PRAGMA table_info.
pub async fn fetch_primary_key(pool: &SqlitePool, table: &str) -> Result<Vec<String>, String> {
    let sql = format!("PRAGMA table_info(\"{}\")", table.replace('"', "\"\""));
    let rows = sqlx::query(&sql)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("PRAGMA table_info failed: {e}"))?;

    let mut pk: Vec<(i64, String)> = rows
        .iter()
        .filter_map(|r| {
            let pk_pos: i64 = r.try_get::<Option<i64>, _>(5).ok().flatten().unwrap_or(0);
            if pk_pos == 0 {
                return None;
            }
            let name: String = r.try_get::<Option<String>, _>(1).ok().flatten()?;
            Some((pk_pos, name))
        })
        .collect();

    pk.sort_by_key(|(pos, _)| *pos);
    Ok(pk.into_iter().map(|(_, n)| n).collect())
}

/// Returns FK info using PRAGMA foreign_key_list.
pub async fn fetch_foreign_keys(pool: &SqlitePool, table: &str) -> Result<Vec<ForeignKeyInfo>, String> {
    let sql = format!("PRAGMA foreign_key_list(\"{}\")", table.replace('"', "\"\""));
    let rows = sqlx::query(&sql)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("PRAGMA foreign_key_list failed: {e}"))?;

    // Group by FK id (first column)
    let mut map: std::collections::BTreeMap<i64, ForeignKeyInfo> = Default::default();
    for r in &rows {
        let id: i64 = r.try_get::<Option<i64>, _>(0).ok().flatten().unwrap_or(0);
        let ref_table: String = r.try_get::<Option<String>, _>(2).ok().flatten().unwrap_or_default();
        let from_col: String = r.try_get::<Option<String>, _>(3).ok().flatten().unwrap_or_default();
        let to_col: String = r.try_get::<Option<String>, _>(4).ok().flatten().unwrap_or_default();

        let entry = map.entry(id).or_insert(ForeignKeyInfo {
            columns: vec![],
            referenced_schema: "main".to_string(),
            referenced_table: ref_table,
            referenced_columns: vec![],
        });
        entry.columns.push(from_col);
        entry.referenced_columns.push(to_col);
    }
    Ok(map.into_values().collect())
}

// ── execute_sql ───────────────────────────────────────────────────────────────

pub async fn execute_sql(pool: &SqlitePool, sql: &str) -> Result<SqlResult, String> {
    let t0 = Instant::now();
    let sql = sql.trim();

    let head = super::sql_util::statement_head(sql);

    if matches!(head.as_str(), "select" | "with" | "pragma" | "explain" | "values") {
        let mut stream = sqlx::query(sql).fetch(pool);
        // Convert each row to JSON as it streams in and drop the driver row
        // immediately - retaining the full Vec<SqliteRow> alongside the JSON
        // rows would double peak memory on a large result.
        let mut columns: Vec<ColumnInfo> = Vec::new();
        let mut data: Vec<Vec<Value>> = Vec::new();
        let mut capped = false;

        loop {
            match stream.try_next().await {
                Ok(Some(row)) => {
                    if data.is_empty() {
                        columns = row
                            .columns()
                            .iter()
                            .map(|c| ColumnInfo::new(c.name(), c.type_info().name().to_lowercase()))
                            .collect();
                    }
                    data.push((0..columns.len()).map(|i| cell_to_json(&row, i)).collect());
                    if data.len() >= EXECUTE_SQL_MAX_ROWS {
                        capped = true;
                        break;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    drop(stream);
                    return Err(format!("{e}"));
                }
            }
        }
        drop(stream);

        let n = data.len() as i64;
        Ok(SqlResult {
            columns,
            rows: data,
            row_count: Some(n),
            message: if capped {
                Some(format!(
                    "Result capped at {EXECUTE_SQL_MAX_ROWS} rows - add a LIMIT clause to fetch a specific range."
                ))
            } else {
                None
            },
            query_ms: t0.elapsed().as_millis() as u64,
            sql: sql.to_string(),
        })
    } else {
        let res = sqlx::query(sql)
            .execute(pool)
            .await
            .map_err(|e| format!("{e}"))?;

        let affected = res.rows_affected() as i64;
        Ok(SqlResult {
            columns: vec![],
            rows: vec![],
            row_count: Some(affected),
            message: Some(format!("{affected} row(s) affected")),
            query_ms: t0.elapsed().as_millis() as u64,
            sql: sql.to_string(),
        })
    }
}

// ── get_table_rows ────────────────────────────────────────────────────────────

pub async fn get_table_rows(
    pool: &SqlitePool,
    table: &str,
    limit: i64,
    offset: i64,
    search: Option<String>,
    // Case-sensitive substring search (drops the LOWER() case-folding).
    search_case_sensitive: bool,
    sort_column: Option<String>,
    sort_direction: Option<String>,
    filters: Option<Vec<crate::db::RowFilter>>,
    // When false, skip the primary-key/foreign-key PRAGMA round-trips - the
    // frontend already holds them for repeat fetches (pagination/sort/filter/live).
    include_meta: bool,
    // Null placement ("first"/"last"); None keeps the historical NULLS LAST default.
    nulls_order: Option<String>,
) -> Result<TableRows, String> {
    let t0 = Instant::now();
    let tq = format!("\"{}\"", table.replace('"', "\"\""));

    // Column info from PRAGMA
    let pragma_sql = format!("PRAGMA table_info({tq})");
    let pragma_rows = sqlx::query(&pragma_sql)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("PRAGMA table_info: {e}"))?;

    // col_name -> flags. `INTEGER PRIMARY KEY` is the rowid alias SQLite fills in
    // itself, which is the closest thing it has to AUTO_INCREMENT and is exactly
    // the column the insert row must not demand a value for.
    let pragma_flags: std::collections::HashMap<String, super::query::ColumnFlags> = pragma_rows
        .iter()
        .filter_map(|r| {
            let name = r.try_get::<Option<String>, _>(1).ok().flatten()?;
            let col_type = r.try_get::<Option<String>, _>(2).ok().flatten().unwrap_or_default();
            let notnull: i64 = r.try_get(3).ok()?;
            let dflt = r.try_get::<Option<String>, _>(4).ok().flatten();
            let pk: i64 = r.try_get(5).ok().unwrap_or(0);
            let auto_generated = pk > 0 && col_type.to_ascii_lowercase().contains("int");
            Some((
                name,
                super::query::ColumnFlags {
                    nullable: notnull == 0,
                    auto_generated,
                    has_default: auto_generated || dflt.is_some(),
                },
            ))
        })
        .collect();

    // col_name -> declared type (used as fallback when the table is empty)
    let pragma_types: std::collections::HashMap<String, String> = pragma_rows
        .iter()
        .filter_map(|r| {
            let name = r.try_get::<Option<String>, _>(1).ok().flatten()?;
            let col_type = r.try_get::<Option<String>, _>(2).ok().flatten()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "text".into());
            Some((name, col_type.to_lowercase()))
        })
        .collect();

    let col_names: Vec<String> = pragma_rows
        .iter()
        .filter_map(|r| r.try_get::<Option<String>, _>(1).ok().flatten())
        .collect();

    // Build WHERE clause (using ? placeholders)
    // Each entry: (conjunct - None for first, Some("AND"/"OR") for rest, condition SQL, binds)
    let mut cond_parts: Vec<(Option<&'static str>, String)> = vec![];
    let mut binds: Vec<String> = vec![];

    if let Some(ref s) = search {
        if !s.is_empty() && !col_names.is_empty() {
            // Case-insensitive folds both operands with LOWER(); case-sensitive
            // compares the raw text. instr() has no LIKE pattern-length cap.
            let parts: Vec<String> = col_names
                .iter()
                .map(|c| {
                    let qc = c.replace('"', "\"\"");
                    if search_case_sensitive {
                        format!("instr(CAST(\"{qc}\" AS TEXT), ?) > 0")
                    } else {
                        format!("instr(LOWER(CAST(\"{qc}\" AS TEXT)), LOWER(?)) > 0")
                    }
                })
                .collect();
            cond_parts.push((None, format!("({})", parts.join(" OR "))));
            for _ in &col_names { binds.push(s.clone()); }
        }
    }

    if let Some(ref fs) = filters {
        for f in fs {
            let conj: Option<&'static str> = if cond_parts.is_empty() { None }
                else if f.conjunct.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("or")) { Some("OR") }
                else { Some("AND") };

            if f.op == super::sql_util::RAW_SQL_FILTER_OP {
                let raw = f.value.as_deref().unwrap_or("");
                if let Some(expr) = super::sql_util::raw_where_expr(raw, super::sql_util::RawFilterDialect::Sqlite)? {
                    cond_parts.push((conj, expr));
                }
                continue;
            }

            if f.column == "__any__" {
                if let Some(ref v) = f.value {
                    let v = v.trim();
                    if !v.is_empty() && !col_names.is_empty() {
                        let mut parts = Vec::new();
                        let mut extra = Vec::new();
                        for col in &col_names {
                            let qcol = format!("\"{}\"", col.replace('"', "\"\""));
                            let (cond, eb) = build_filter_condition(&qcol, &f.op, v);
                            parts.push(cond);
                            extra.extend(eb);
                        }
                        if !parts.is_empty() {
                            cond_parts.push((conj, format!("({})", parts.join(" OR "))));
                            binds.extend(extra);
                        }
                    }
                }
                continue;
            }

            let qcol = format!("\"{}\"", f.column.replace('"', "\"\""));
            match f.op.as_str() {
                "is_null"     => cond_parts.push((conj, format!("{qcol} IS NULL"))),
                "is_not_null" => cond_parts.push((conj, format!("{qcol} IS NOT NULL"))),
                _ => if let Some(ref v) = f.value {
                    let (cond, extra_binds) = build_filter_condition(&qcol, &f.op, v);
                    cond_parts.push((conj, cond));
                    binds.extend(extra_binds);
                },
            }
        }
    }

    let where_clause = if cond_parts.is_empty() { String::new() } else {
        let mut out = String::from("WHERE ");
        for (i, (conj, cond)) in cond_parts.into_iter().enumerate() {
            if i > 0 { out.push(' '); out.push_str(conj.unwrap_or("AND")); out.push(' '); }
            out.push_str(&cond);
        }
        out
    };

    // ORDER BY - an explicit NULLS clause keeps ordering consistent when a column
    // has NULLs; defaults to NULLS LAST when the caller doesn't specify placement.
    let order_clause = if let Some(col) = sort_column {
        let dir = sort_direction.as_deref().unwrap_or("asc").to_ascii_uppercase();
        let dir = if dir == "DESC" { "DESC" } else { "ASC" };
        let nulls = match nulls_order.as_deref() {
            Some("first") => "NULLS FIRST",
            _ => "NULLS LAST",
        };
        format!("ORDER BY \"{}\" {dir} {nulls}", col.replace('"', "\"\""))
    } else {
        String::new()
    };

    // COUNT
    let count_sql = format!("SELECT COUNT(*) FROM {tq} {where_clause}");
    let mut count_q = sqlx::query(&count_sql);
    for b in &binds {
        count_q = count_q.bind(b.clone());
    }
    let count_row = count_q
        .fetch_one(pool)
        .await
        .map_err(|e| format!("Count failed: {e}"))?;
    let total: i64 = count_row.try_get::<Option<i64>, _>(0).ok().flatten().unwrap_or(0);

    // ROWS
    let rows_sql = format!("SELECT * FROM {tq} {where_clause} {order_clause} LIMIT ? OFFSET ?");
    let mut rows_q = sqlx::query(&rows_sql);
    for b in &binds {
        rows_q = rows_q.bind(b.clone());
    }
    rows_q = rows_q.bind(limit).bind(offset);

    let fetched = rows_q
        .fetch_all(pool)
        .await
        .map_err(|e| format!("Query failed: {e}"))?;

    let columns: Vec<ColumnInfo> = fetched
        .first()
        .map(|r| {
            r.columns()
                .iter()
                .map(|c| {
                    let mut col = ColumnInfo::new(c.name(), c.type_info().name().to_lowercase());
                    if let Some(f) = pragma_flags.get(c.name()) {
                        col.nullable = f.nullable;
                        col.auto_generated = f.auto_generated;
                        col.has_default = f.has_default;
                    }
                    col
                })
                .collect()
        })
        .unwrap_or_else(|| {
            col_names
                .iter()
                .map(|n| {
                    let dt = pragma_types.get(n).cloned().unwrap_or_else(|| "text".into());
                    let mut col = ColumnInfo::new(n.clone(), dt);
                    if let Some(f) = pragma_flags.get(n.as_str()) {
                        col.nullable = f.nullable;
                        col.auto_generated = f.auto_generated;
                        col.has_default = f.has_default;
                    }
                    col
                })
                .collect()
        });

    let rows: Vec<Vec<Value>> = fetched
        .iter()
        .map(|r| (0..columns.len()).map(|i| cell_to_json(r, i)).collect())
        .collect();

    // Two extra PRAGMA statements serialized on the single-connection pool -
    // skip them on metadata-skipping fetches; the frontend keeps its cached values.
    let (primary_key, foreign_keys) = if include_meta {
        (
            fetch_primary_key(pool, table).await.unwrap_or_default(),
            fetch_foreign_keys(pool, table).await.unwrap_or_default(),
        )
    } else {
        (Vec::new(), Vec::new())
    };

    Ok(TableRows {
        // Preview fetching is a Postgres path (pg_stats + pg_column_size).
        preview_columns: Vec::new(),
        columns,
        rows,
        total,
        query_ms: t0.elapsed().as_millis() as u64,
        primary_key,
        foreign_keys,
        sql: format!("{rows_sql}\n{count_sql}"),
    })
}

/// Build OR-across-all-columns conditions for the `__any__` sentinel (D1 / JSON params).
pub fn build_any_column_d1(col_names: &[String], op: &str, val: &str) -> (Vec<String>, Vec<serde_json::Value>) {
    let mut parts = Vec::new();
    let mut params = Vec::new();
    for col in col_names {
        let qcol = format!("\"{}\"", col.replace('"', "\"\""));
        let (cond, binds) = build_filter_condition(&qcol, op, val);
        parts.push(cond);
        for b in binds { params.push(serde_json::Value::String(b)); }
    }
    (parts, params)
}

/// Same logic as build_filter_condition but returns JSON Values for D1 HTTP params.
pub fn build_d1_filter(qcol: &str, op: &str, val: &str) -> (String, Vec<serde_json::Value>) {
    let (cond, binds) = build_filter_condition(qcol, op, val);
    (cond, binds.into_iter().map(serde_json::Value::String).collect())
}

fn build_filter_condition(qcol: &str, op: &str, val: &str) -> (String, Vec<String>) {
    match op {
        // Comparison operators: SQLite is dynamically typed - no cast needed.
        // Direct comparison works correctly for integers, reals, and text alike.
        "eq"  => (format!("{qcol} = ?"),  vec![val.to_string()]),
        "neq" => (format!("{qcol} != ?"), vec![val.to_string()]),
        "gt"  => (format!("{qcol} > ?"),  vec![val.to_string()]),
        "gte" => (format!("{qcol} >= ?"), vec![val.to_string()]),
        "lt"  => (format!("{qcol} < ?"),  vec![val.to_string()]),
        "lte" => (format!("{qcol} <= ?"), vec![val.to_string()]),
        // Text-search operators: use instr()/substr() instead of LIKE. D1 caps
        // the LIKE pattern length ("LIKE or GLOB pattern too complex" for long
        // terms); instr() does a literal, case-folded substring match with no
        // pattern-length limit and no wildcard escaping needed.
        "contains" => (
            format!("instr(LOWER(CAST({qcol} AS TEXT)), LOWER(?)) > 0"),
            vec![val.to_string()],
        ),
        // A NULL cell contains nothing, so it satisfies "does not contain".
        "not_contains" => (
            format!("({qcol} IS NULL OR instr(LOWER(CAST({qcol} AS TEXT)), LOWER(?)) = 0)"),
            vec![val.to_string()],
        ),
        "starts_with" => (
            format!("instr(LOWER(CAST({qcol} AS TEXT)), LOWER(?)) = 1"),
            vec![val.to_string()],
        ),
        "ends_with" => (
            format!("substr(LOWER(CAST({qcol} AS TEXT)), -length(?)) = LOWER(?)"),
            vec![val.to_string(), val.to_string()],
        ),
        "between" => {
            let mut parts = val.splitn(2, ',');
            let from = parts.next().unwrap_or("").trim().to_string();
            let to   = parts.next().unwrap_or("").trim().to_string();
            (format!("({qcol} >= ? AND {qcol} <= ?)"), vec![from, to])
        }
        _ => (format!("{qcol} = ?"), vec![val.to_string()]),
    }
}

// ── insert_table_row ──────────────────────────────────────────────────────────

pub fn sqlite_column_optional_when_omitted(
    notnull: bool,
    dflt_value: Option<&str>,
    pk: i64,
    col_type: &str,
) -> bool {
    if dflt_value.is_some() {
        return true;
    }
    let t = col_type.to_ascii_lowercase();
    if t.contains("serial") || t.ends_with("serial") {
        return true;
    }
    if pk > 0 && t.contains("int") {
        return true;
    }
    !notnull
}

pub async fn insert_table_row(
    pool: &SqlitePool,
    table: &str,
    values: std::collections::HashMap<String, Value>,
) -> Result<Vec<Value>, String> {
    let mut conn = pool.acquire().await.map_err(|e| format!("Failed to acquire connection: {e}"))?;
    insert_row_on(&mut conn, table, &values).await
}

/// Insert one row on the given connection and return it (RETURNING *). Shared
/// by the single-row insert and the transactional apply.
async fn insert_row_on(
    conn: &mut sqlx::SqliteConnection,
    table: &str,
    values: &std::collections::HashMap<String, Value>,
) -> Result<Vec<Value>, String> {
    let tq = format!("\"{}\"", table.replace('"', "\"\""));
    let pragma_sql = format!("PRAGMA table_info({tq})");
    let pragma_rows = sqlx::query(&pragma_sql)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| format!("PRAGMA table_info: {e}"))?;

    let mut column_order: Vec<String> = Vec::new();
    let mut optional: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    let mut col_type_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for r in &pragma_rows {
        let name = r
            .try_get::<Option<String>, _>(1)
            .ok()
            .flatten()
            .ok_or("Invalid PRAGMA column name")?;
        let col_type = r
            .try_get::<Option<String>, _>(2)
            .ok()
            .flatten()
            .unwrap_or_else(|| "text".into());
        let notnull: i64 = r.try_get(3).ok().unwrap_or(0);
        let dflt: Option<String> = r.try_get(4).ok().flatten();
        let pk: i64 = r.try_get(5).ok().unwrap_or(0);
        let opt = sqlite_column_optional_when_omitted(notnull != 0, dflt.as_deref(), pk, &col_type);
        column_order.push(name.clone());
        col_type_map.insert(name.clone(), col_type.to_lowercase());
        optional.insert(name, opt);
    }

    for col in values.keys() {
        if !optional.contains_key(col) {
            return Err(format!("Unknown column: {col}"));
        }
    }

    for (name, opt) in &optional {
        if !opt && !values.contains_key(name) {
            return Err(format!(
                "Column \"{name}\" is required (NOT NULL, no default)"
            ));
        }
    }

    let mut col_names: Vec<String> = values.keys().cloned().collect();
    col_names.sort();

    let cols: Vec<String> = col_names
        .iter()
        .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
        .collect();
    let placeholders: Vec<String> = (0..col_names.len()).map(|_| "?".to_string()).collect();
    let sql = format!(
        "INSERT INTO {tq} ({}) VALUES ({}) RETURNING *",
        cols.join(", "),
        placeholders.join(", ")
    );

    let mut q = sqlx::query(&sql);
    for col in &col_names {
        let v = values.get(col).ok_or_else(|| format!("Missing value for {col}"))?;
        let ct = col_type_map.get(col).map(|s| s.as_str()).unwrap_or("");
        q = bind_value_typed(q, v, ct);
    }

    let inserted = q
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| format!("Insert failed: {e}"))?;

    let returning_cols: Vec<String> = inserted
        .columns()
        .iter()
        .map(|c| c.name().to_string())
        .collect();

    Ok(column_order
        .iter()
        .map(|name| {
            let idx = returning_cols
                .iter()
                .position(|n| n == name)
                .unwrap_or(0);
            cell_to_json(&inserted, idx)
        })
        .collect())
}

// ── update_table_cell ─────────────────────────────────────────────────────────

pub async fn update_table_cell(
    pool: &SqlitePool,
    table: &str,
    primary_key: std::collections::HashMap<String, Value>,
    column: &str,
    value: &Value,
) -> Result<(), String> {
    let pk_columns = fetch_primary_key(pool, table).await?;
    if pk_columns.is_empty() {
        return Err("Cannot update row: table has no primary key".into());
    }

    let tq = format!("\"{}\"", table.replace('"', "\"\""));
    let set_col = format!("\"{}\"", column.replace('"', "\"\""));

    let where_parts: Vec<String> = pk_columns
        .iter()
        .map(|c| format!("\"{}\" = ?", c.replace('"', "\"\"")))
        .collect();

    let sql = format!("UPDATE {tq} SET {set_col} = ? WHERE {}", where_parts.join(" AND "));
    let mut q = sqlx::query(&sql);
    q = bind_value(q, value);
    for pk_col in &pk_columns {
        let v = primary_key.get(pk_col).ok_or_else(|| format!("Missing PK: {pk_col}"))?;
        q = bind_value(q, v);
    }

    q.execute(pool)
        .await
        .map_err(|e| format!("Update failed: {e}"))?;
    Ok(())
}

// ── delete_table_rows ─────────────────────────────────────────────────────────

pub async fn delete_table_rows(
    pool: &SqlitePool,
    table: &str,
    primary_keys: Vec<std::collections::HashMap<String, Value>>,
) -> Result<u64, String> {
    if primary_keys.is_empty() {
        return Ok(0);
    }
    let pk_columns = fetch_primary_key(pool, table).await?;
    if pk_columns.is_empty() {
        return Err("Cannot delete rows: table has no primary key".into());
    }

    let tq = format!("\"{}\"", table.replace('"', "\"\""));
    let mut total: u64 = 0;

    for pk_map in primary_keys {
        let where_parts: Vec<String> = pk_columns
            .iter()
            .map(|c| format!("\"{}\" = ?", c.replace('"', "\"\"")))
            .collect();
        let sql = format!("DELETE FROM {tq} WHERE {}", where_parts.join(" AND "));
        let mut q = sqlx::query(&sql);
        for col in &pk_columns {
            let v = pk_map.get(col).ok_or_else(|| format!("Missing PK: {col}"))?;
            q = bind_value(q, v);
        }
        let res = q.execute(pool).await.map_err(|e| format!("Delete failed: {e}"))?;
        total += res.rows_affected();
    }
    Ok(total)
}

// ── apply_table_changes ───────────────────────────────────────────────────────

/// Apply a grid's staged edits, deletions and insert as one transaction - the
/// SQLite side of `query::apply_table_changes`. Each edited row is one UPDATE
/// of all its changed columns, bound with the column's declared type (so a
/// STRICT table gets an INTEGER, not the numeric string the grid holds), and
/// every UPDATE/DELETE must match exactly one row or the whole apply rolls back.
pub async fn apply_table_changes(
    pool: &SqlitePool,
    table: &str,
    updates: Vec<super::query::RowUpdate>,
    deletes: Vec<std::collections::HashMap<String, Value>>,
    insert: Option<std::collections::HashMap<String, Value>>,
) -> Result<super::query::ApplyChangesResult, String> {
    use super::query::{describe_pk, row_count_error, ApplyChangesResult};

    let quote = |name: &str| format!("\"{}\"", name.replace('"', "\"\""));
    let tq = quote(table);
    // One PRAGMA gives both the declared types and the key, in key order.
    let pragma_rows = sqlx::query(&format!("PRAGMA table_info({tq})"))
        .fetch_all(pool)
        .await
        .map_err(|e| format!("PRAGMA table_info: {e}"))?;
    if pragma_rows.is_empty() {
        return Err(format!("Table not found: {table}"));
    }
    let mut col_types: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut pk: Vec<(i64, String)> = Vec::new();
    for r in &pragma_rows {
        let Some(name) = r.try_get::<Option<String>, _>(1).ok().flatten() else { continue };
        let col_type = r.try_get::<Option<String>, _>(2).ok().flatten().unwrap_or_default();
        let pk_pos: i64 = r.try_get::<Option<i64>, _>(5).ok().flatten().unwrap_or(0);
        if pk_pos > 0 {
            pk.push((pk_pos, name.clone()));
        }
        col_types.insert(name, col_type.to_lowercase());
    }
    pk.sort_by_key(|(pos, _)| *pos);
    let pk_columns: Vec<String> = pk.into_iter().map(|(_, n)| n).collect();
    if pk_columns.is_empty() && !(updates.is_empty() && deletes.is_empty()) {
        return Err("Cannot apply changes: table has no primary key".into());
    }
    let col_type = |c: &str| -> Result<&str, String> {
        col_types.get(c).map(|s| s.as_str()).ok_or_else(|| format!("Unknown column: {c}"))
    };
    let pk_value = |primary_key: &std::collections::HashMap<String, Value>, c: &str| -> Result<Value, String> {
        primary_key.get(c).cloned().ok_or_else(|| format!("Missing primary key column: {c}"))
    };
    let where_sql = pk_columns.iter().map(|c| format!("{} = ?", quote(c))).collect::<Vec<_>>().join(" AND ");

    let mut tx = pool.begin().await.map_err(|e| format!("Could not start a transaction: {e}"))?;
    let mut updated = 0_u64;
    let mut deleted = 0_u64;

    for row in &updates {
        if row.values.is_empty() {
            continue;
        }
        let pk_vals = pk_columns.iter().map(|c| pk_value(&row.primary_key, c)).collect::<Result<Vec<_>, _>>()?;
        let mut cols: Vec<&String> = row.values.keys().collect();
        cols.sort();
        let sets = cols.iter().map(|c| format!("{} = ?", quote(c))).collect::<Vec<_>>().join(", ");
        let sql = format!("UPDATE {tq} SET {sets} WHERE {where_sql}");
        let mut q = sqlx::query(&sql);
        for c in &cols {
            q = bind_value_typed(q, &row.values[*c], col_type(c)?);
        }
        for (c, v) in pk_columns.iter().zip(&pk_vals) {
            q = bind_value_typed(q, v, col_type(c)?);
        }
        let n = q
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("Update failed ({}): {e}", describe_pk(&pk_columns, &row.primary_key)))?
            .rows_affected();
        if n != 1 {
            return Err(row_count_error(&pk_columns, &row.primary_key, n));
        }
        updated += 1;
    }

    if !deletes.is_empty() {
        let sql = format!("DELETE FROM {tq} WHERE {where_sql}");
        for primary_key in &deletes {
            let pk_vals = pk_columns.iter().map(|c| pk_value(primary_key, c)).collect::<Result<Vec<_>, _>>()?;
            let mut q = sqlx::query(&sql);
            for (c, v) in pk_columns.iter().zip(&pk_vals) {
                q = bind_value_typed(q, v, col_type(c)?);
            }
            let n = q
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("Delete failed ({}): {e}", describe_pk(&pk_columns, primary_key)))?
                .rows_affected();
            if n != 1 {
                return Err(row_count_error(&pk_columns, primary_key, n));
            }
            deleted += 1;
        }
    }

    let inserted = match insert.filter(|v| !v.is_empty()) {
        Some(values) => Some(insert_row_on(&mut tx, table, &values).await?),
        None => None,
    };

    tx.commit().await.map_err(|e| format!("Could not commit: {e}"))?;
    Ok(ApplyChangesResult { updated, deleted, inserted })
}

// ── value binding ─────────────────────────────────────────────────────────────

fn bind_value<'a>(
    q: sqlx::query::Query<'a, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'a>>,
    value: &Value,
) -> sqlx::query::Query<'a, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'a>> {
    bind_value_typed(q, value, "")
}

/// Type-aware binding: coerces string values to integers/floats when the
/// declared column affinity requires it. Prevents SQLITE_MISMATCH (code 20)
/// when the frontend sends numeric strings for INTEGER/REAL columns.
fn bind_value_typed<'a>(
    q: sqlx::query::Query<'a, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'a>>,
    value: &Value,
    col_type: &str,
) -> sqlx::query::Query<'a, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'a>> {
    let t = col_type.to_ascii_lowercase();
    match value {
        Value::Null => q.bind(None::<String>),
        Value::Bool(b) => q.bind(*b as i64),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                q.bind(i)
            } else if let Some(f) = n.as_f64() {
                q.bind(f)
            } else {
                q.bind(n.to_string())
            }
        }
        Value::String(s) => {
            let trimmed = s.trim();
            // Coerce numeric strings based on declared column affinity so that
            // INTEGER PRIMARY KEY never receives a TEXT or REAL binding.
            if t.contains("int") || t.ends_with("serial") {
                if let Ok(i) = trimmed.parse::<i64>() {
                    return q.bind(i);
                }
            } else if t.contains("real") || t.contains("float") || t.contains("double")
                || t.contains("numeric") || t.contains("decimal") || t.contains("number")
            {
                if let Ok(f) = trimmed.parse::<f64>() {
                    return q.bind(f);
                }
            }
            q.bind(s.clone())
        }
        other => q.bind(other.to_string()),
    }
}
