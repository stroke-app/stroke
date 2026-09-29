use super::query::{ColumnInfo, ForeignKeyInfo, RowFilter, SqlResult, TableRows};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use futures::TryStreamExt;
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sqlx::{Column, MySqlPool, Row, TypeInfo};
use std::collections::HashMap;
use std::time::Instant;

const EXECUTE_SQL_MAX_ROWS: usize = 1_000_000_000;

/// A text value out of MySQL's `information_schema`.
///
/// Those columns are declared `varchar` but carry a `utf8mb3_bin` collation, so
/// MySQL sets the protocol's BINARY flag on them and sqlx types the column
/// `VARBINARY`. `try_get::<String>` then fails on a column that is plainly
/// text - `mismatched types; Rust type String (as SQL type VARCHAR) is not
/// compatible with SQL type VARBINARY`.
///
/// Every metadata decode in this file read `try_get::<String>(i).ok()?`, which
/// *drops the row* on that error. The result was a MySQL connection whose
/// sidebar said "No tables" for a database that had them, with no error
/// anywhere to say why - and the same silence behind blank index, trigger,
/// function and column metadata. Read the bytes when the string decode is
/// refused, and take the text from them.
pub(crate) fn my_text(row: &sqlx::mysql::MySqlRow, idx: usize) -> Option<String> {
    if let Ok(s) = row.try_get::<Option<String>, _>(idx) {
        return s;
    }
    match row.try_get::<Option<Vec<u8>>, _>(idx) {
        Ok(Some(b)) => Some(String::from_utf8_lossy(&b).into_owned()),
        _ => None,
    }
}

/// The same, by column name.
pub(crate) fn my_text_named(row: &sqlx::mysql::MySqlRow, name: &str) -> Option<String> {
    if let Ok(s) = row.try_get::<Option<String>, _>(name) {
        return s;
    }
    match row.try_get::<Option<Vec<u8>>, _>(name) {
        Ok(Some(b)) => Some(String::from_utf8_lossy(&b).into_owned()),
        _ => None,
    }
}

/// An integer out of `information_schema`, whatever numeric type it arrives as.
///
/// `COALESCE(TABLE_ROWS, 0)` is `decimal(21,0)`, not the `bigint unsigned` the
/// column is declared as - so `try_get::<u64>` fails and the estimate silently
/// read 0 for every table.
pub(crate) fn my_int(row: &sqlx::mysql::MySqlRow, idx: usize) -> Option<i64> {
    if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
        return v;
    }
    if let Ok(v) = row.try_get::<Option<u64>, _>(idx) {
        return v.map(|n| n as i64);
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(idx) {
        return v.map(|n| n as i64);
    }
    my_text(row, idx).and_then(|t| t.trim().parse::<i64>().ok())
}

fn bt(s: &str) -> String {
    super::sql_util::quote_backtick(s)
}

pub fn cell_to_json(row: &sqlx::mysql::MySqlRow, idx: usize) -> Value {
    // DECIMAL/NUMERIC must decode as an exact Decimal *first*. MySQL sends these as
    // text and the generic integer arms below can otherwise consume the value and
    // keep only its integer part (e.g. 9.99 → 9, 0.99 → 0), silently corrupting it.
    // Route by column type before the i64/u64/bool/f64 attempts.
    let type_name = row.column(idx).type_info().name();
    if type_name.eq_ignore_ascii_case("DECIMAL") || type_name.eq_ignore_ascii_case("NEWDECIMAL") {
        if let Ok(v) = row.try_get::<Option<Decimal>, _>(idx) {
            return v.map(|d| json!(d.to_string())).unwrap_or(Value::Null);
        }
    }
    // Fast path: route the common column types by name so a plain text/date cell
    // doesn't pay for a cascade of failed try_get attempts (each mismatch makes
    // sqlx allocate a boxed "mismatched types" error - up to 9 per text cell).
    // A failed route falls through to the full chain below, so unmatched or
    // alias types behave exactly as before.
    match type_name {
        "VARCHAR" | "CHAR" | "TEXT" | "TINYTEXT" | "MEDIUMTEXT" | "LONGTEXT" | "ENUM" | "SET" => {
            if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
                return text_cell(row, idx, v);
            }
        }
        "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "BIGINT" => {
            if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
                return v.map(|n| json!(n)).unwrap_or(Value::Null);
            }
        }
        "TINYINT UNSIGNED" | "SMALLINT UNSIGNED" | "MEDIUMINT UNSIGNED" | "INT UNSIGNED"
        | "BIGINT UNSIGNED" => {
            if let Ok(v) = row.try_get::<Option<u64>, _>(idx) {
                return v.map(|n| json!(n)).unwrap_or(Value::Null);
            }
        }
        "FLOAT" | "DOUBLE" => {
            if let Ok(v) = row.try_get::<Option<f64>, _>(idx) {
                return v.map(|n| json!(n)).unwrap_or(Value::Null);
            }
        }
        "DATETIME" | "TIMESTAMP" => {
            if let Ok(v) = row.try_get::<Option<NaiveDateTime>, _>(idx) {
                return v.map(|d| json!(d.to_string())).unwrap_or(Value::Null);
            }
        }
        "DATE" => {
            if let Ok(v) = row.try_get::<Option<NaiveDate>, _>(idx) {
                return v.map(|d| json!(d.to_string())).unwrap_or(Value::Null);
            }
        }
        "TIME" => {
            if let Ok(v) = row.try_get::<Option<NaiveTime>, _>(idx) {
                return v.map(|t| json!(t.to_string())).unwrap_or(Value::Null);
            }
        }
        "JSON" => {
            if let Ok(v) = row.try_get::<Option<serde_json::Value>, _>(idx) {
                return json_cell(row, idx, v);
            }
        }
        _ => {}
    }
    if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
        return v.map(|n| json!(n)).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<u64>, _>(idx) {
        return v.map(|n| json!(n)).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<bool>, _>(idx) {
        return v.map(|b| json!(b)).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(idx) {
        return v.map(|n| json!(n)).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<Decimal>, _>(idx) {
        return v.map(|d| json!(d.to_string())).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<NaiveDateTime>, _>(idx) {
        return v.map(|d| json!(d.to_string())).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<NaiveDate>, _>(idx) {
        return v.map(|d| json!(d.to_string())).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<NaiveTime>, _>(idx) {
        return v.map(|t| json!(t.to_string())).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<serde_json::Value>, _>(idx) {
        return json_cell(row, idx, v);
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
        return text_cell(row, idx, v);
    }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(idx) {
        return v.map(|b| binary_cell(row, idx, b)).unwrap_or(Value::Null);
    }
    Value::Null
}

/// A binary column that is plainly text comes back as text.
///
/// MySQL types the result columns of `SHOW …` statements (and several
/// `information_schema` views) as VARBINARY even though they hold names, so
/// `try_get::<String>` refuses them - sqlx will not decode binary as UTF-8 - and
/// they fell through to the byte-count placeholder. `SHOW DATABASES` therefore
/// returned `"[18 bytes]"` for every row: the database switcher listed one
/// indistinguishable entry per database, and the sidebar's keyed `{#each}` threw
/// `each_key_duplicate` and took the whole panel down.
///
/// The test is stricter than "valid UTF-8": a real BLOB can be valid UTF-8 by
/// accident, so the bytes must also carry no control characters other than tab,
/// newline and carriage return. Anything else keeps the placeholder, because a
/// grid cell is the wrong place to dump a megabyte of PNG.
fn binary_cell(row: &sqlx::mysql::MySqlRow, idx: usize, bytes: Vec<u8>) -> Value {
    if bytes.len() <= super::sql_util::CELL_VALUE_CAP {
        if let Ok(text) = std::str::from_utf8(&bytes) {
            if is_plain_text(text) {
                return text_cell(row, idx, Some(text.to_string()));
            }
        }
    }
    json!(format!("[{} bytes]", bytes.len()))
}

/// Text a person could read in a table cell: no C0 controls beyond the three
/// whitespace ones, and no NULs.
fn is_plain_text(s: &str) -> bool {
    !s.is_empty()
        && !s.chars().any(|c| {
            (c.is_control() && c != '\t' && c != '\n' && c != '\r') || c == '\u{0}'
        })
}

/// String cell, capped - a multi-MB cell shipped whole freezes the webview
/// (see sql_util::CELL_VALUE_CAP).
fn text_cell(row: &sqlx::mysql::MySqlRow, idx: usize, v: Option<String>) -> Value {
    match v {
        Some(s) if s.len() > super::sql_util::CELL_VALUE_CAP => super::sql_util::oversize_cell(
            &row.column(idx).type_info().name().to_lowercase(),
            s.len(),
            s.as_bytes(),
        ),
        Some(s) => json!(s),
        None => Value::Null,
    }
}

/// JSON document cell, capped the same way as text.
fn json_cell(row: &sqlx::mysql::MySqlRow, idx: usize, v: Option<serde_json::Value>) -> Value {
    match v {
        Some(val) => super::sql_util::cap_json_value(
            &row.column(idx).type_info().name().to_lowercase(),
            val,
        ),
        None => Value::Null,
    }
}

pub async fn fetch_primary_key(pool: &MySqlPool, schema: &str, table: &str) -> Result<Vec<String>, String> {
    let rows = sqlx::query(
        "SELECT COLUMN_NAME FROM information_schema.KEY_COLUMN_USAGE \
         WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? AND CONSTRAINT_NAME = 'PRIMARY' \
         ORDER BY ORDINAL_POSITION",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to load primary key: {e}"))?;
    Ok(rows.iter().filter_map(|r| my_text(r, 0)).collect())
}

fn escape_like(input: &str) -> String {
    super::sql_util::escape_like_backslash(input)
}

struct WhereClause {
    sql: String,
    binds: Vec<String>,
}

fn build_where(columns: &[String], search: Option<&str>, search_is_regex: bool, search_case_sensitive: bool, filters: &[RowFilter]) -> Result<WhereClause, String> {
    // (conjunct - None for first condition, Some("AND"/"OR") for subsequent)
    let mut cond_parts: Vec<(Option<&'static str>, String)> = Vec::new();
    let mut binds: Vec<String> = Vec::new();

    if let Some(term) = search.map(str::trim).filter(|s| !s.is_empty()) {
        // CAST to CHAR/BINARY makes the operand a string LIKE/REGEXP can match;
        // BINARY additionally forces case-sensitivity. Regex uses the ICU
        // REGEXP operator; the pattern arrives pre-built from the frontend.
        let cast = if search_case_sensitive { "BINARY" } else { "CHAR" };
        let pattern = if search_is_regex { term.to_string() } else { format!("%{}%", escape_like(term)) };
        let parts: Vec<String> = columns
            .iter()
            .map(|c| {
                let qc = bt(c);
                if search_is_regex { format!("CAST({qc} AS {cast}) REGEXP ?") }
                else { format!("CAST({qc} AS {cast}) LIKE ? ESCAPE '\\\\'") }
            })
            .collect();
        if !parts.is_empty() {
            cond_parts.push((None, format!("({})", parts.join(" OR "))));
            for _ in columns { binds.push(pattern.clone()); }
        }
    }

    for f in filters {
        let conj: Option<&'static str> = if cond_parts.is_empty() { None }
            else if f.conjunct.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("or")) { Some("OR") }
            else { Some("AND") };

        if f.column == "__any__" {
            let v = f.value.as_deref().unwrap_or("").trim();
            if !v.is_empty() && !columns.is_empty() {
                let (pattern, like_op) = match f.op.as_str() {
                    "contains"    => (format!("%{}%", escape_like(v)), "LIKE"),
                    "starts_with" => (format!("{}%",  escape_like(v)), "LIKE"),
                    "ends_with"   => (format!("%{}",  escape_like(v)), "LIKE"),
                    "eq"          => (v.to_string(), "="),
                    _             => { continue; }
                };
                let parts: Vec<String> = columns.iter().map(|c| {
                    let qc = bt(c);
                    if like_op == "=" { format!("CAST({qc} AS CHAR) = ?") }
                    else              { format!("CAST({qc} AS CHAR) {like_op} ? ESCAPE '\\\\'") }
                }).collect();
                cond_parts.push((conj, format!("({})", parts.join(" OR "))));
                for _ in columns { binds.push(pattern.clone()); }
            }
            continue;
        }

        // Validate the filter column against the fetched columns so an unknown
        // name never reaches the query (mirrors the Postgres ensure_column check).
        if !columns.iter().any(|c| c == &f.column) {
            return Err(format!("Unknown column: {}", f.column));
        }
        let col = bt(&f.column);
        match f.op.as_str() {
            "is_null"     => cond_parts.push((conj, format!("{col} IS NULL"))),
            "is_not_null" => cond_parts.push((conj, format!("{col} IS NOT NULL"))),
            op => {
                let v = f.value.as_deref().unwrap_or("").trim();
                if v.is_empty() { continue; }
                // Comparison operators: NO CAST - MySQL performs implicit type coercion
                // from the string parameter to the column's actual type, which means the
                // database can use indexes on typed columns (INT, DECIMAL, DATETIME, etc.).
                // Only text-search ops (LIKE) need CAST since LIKE is inherently string-only.
                match op {
                    "eq"  => { cond_parts.push((conj, format!("{col} = ?"))); binds.push(v.to_string()); }
                    "neq" => { cond_parts.push((conj, format!("{col} != ?"))); binds.push(v.to_string()); }
                    "gt"  => { cond_parts.push((conj, format!("{col} > ?"))); binds.push(v.to_string()); }
                    "gte" => { cond_parts.push((conj, format!("{col} >= ?"))); binds.push(v.to_string()); }
                    "lt"  => { cond_parts.push((conj, format!("{col} < ?"))); binds.push(v.to_string()); }
                    "lte" => { cond_parts.push((conj, format!("{col} <= ?"))); binds.push(v.to_string()); }
                    "contains" => {
                        cond_parts.push((conj, format!("CAST({col} AS CHAR) LIKE ? ESCAPE '\\\\'")));
                        binds.push(format!("%{}%", escape_like(v)));
                    }
                    "not_contains" => {
                        // A NULL cell contains nothing, so it satisfies "does not contain".
                        cond_parts.push((conj, format!("({col} IS NULL OR CAST({col} AS CHAR) NOT LIKE ? ESCAPE '\\\\')")));
                        binds.push(format!("%{}%", escape_like(v)));
                    }
                    "starts_with" => {
                        cond_parts.push((conj, format!("CAST({col} AS CHAR) LIKE ? ESCAPE '\\\\'")));
                        binds.push(format!("{}%", escape_like(v)));
                    }
                    "ends_with" => {
                        cond_parts.push((conj, format!("CAST({col} AS CHAR) LIKE ? ESCAPE '\\\\'")));
                        binds.push(format!("%{}", escape_like(v)));
                    }
                    "between" => {
                        let mut parts = v.splitn(2, ',');
                        let from = parts.next().unwrap_or("").trim().to_string();
                        let to   = parts.next().unwrap_or("").trim().to_string();
                        cond_parts.push((conj, format!("({col} >= ? AND {col} <= ?)")));
                        binds.push(from);
                        binds.push(to);
                    }
                    _ => return Err(format!("Unsupported filter operator: {op}")),
                }
            }
        }
    }

    let sql = if cond_parts.is_empty() { String::new() } else {
        let mut out = String::from(" WHERE ");
        for (i, (conj, cond)) in cond_parts.into_iter().enumerate() {
            if i > 0 { out.push(' '); out.push_str(conj.unwrap_or("AND")); out.push(' '); }
            out.push_str(&cond);
        }
        out
    };
    Ok(WhereClause { sql, binds })
}

pub async fn get_table_rows(
    pool: &MySqlPool,
    schema: &str,
    table: &str,
    limit: i64,
    offset: i64,
    search: Option<String>,
    // Search-box modifiers: ICU `REGEXP` instead of substring, and case-sensitive
    // matching (BINARY cast). Apply to the global search only, not column filters.
    search_is_regex: bool,
    search_case_sensitive: bool,
    sort_column: Option<String>,
    sort_direction: Option<String>,
    filters: Option<Vec<RowFilter>>,
    // When false, skip the primary-key/foreign-key catalog round-trips - the
    // frontend already holds them for repeat fetches (pagination/sort/filter/live).
    include_meta: bool,
    nulls_order: Option<String>,
) -> Result<TableRows, String> {
    let started = Instant::now();
    // One catalog round-trip per fetch: the WHERE/sort builders need the column
    // names before the count/data queries can be built, so fetch the full
    // name/type/nullability projection upfront and reuse it below for the
    // nullable map and the empty-table column fallback (instead of a second
    // information_schema.COLUMNS query inside the join).
    // Every catalog and data query of a table open, in as few round trips as
    // the dependencies allow. Against a remote MySQL each sequential query costs
    // a full round trip (~300ms to a US region from South Asia), and this used to
    // run four in a row: columns, then count + rows, then primary key, then
    // foreign keys. PK and FK depend on nothing, so they always join the page
    // fetch; and a plain open (no search, filter or sort) doesn't need the column
    // list to build its query, so the column metadata joins it too: one stage.
    let meta_q = sqlx::query(
        "SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, EXTRA, COLUMN_DEFAULT \
         FROM information_schema.COLUMNS \
         WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
    )
    .bind(schema)
    .bind(table);
    let pk_fk = async {
        if include_meta {
            let (pk, fks) = tokio::join!(fetch_primary_key(pool, schema, table), fetch_foreign_keys(pool, schema, table));
            (pk.unwrap_or_default(), fks.unwrap_or_default())
        } else {
            // The frontend keeps the values it already loaded for this table.
            (Vec::new(), Vec::new())
        }
    };
    let filters = filters.unwrap_or_default();
    let sort_col = sort_column.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let needs_columns_first =
        sort_col.is_some() || !filters.is_empty() || search.as_deref().is_some_and(|q| !q.trim().is_empty());

    let table_ref = format!("{}.{}", bt(schema), bt(table));
    // MySQL types COUNT(*) as BIGINT UNSIGNED, but MariaDB types it as signed
    // BIGINT - decoding the wrong signedness is a hard type-mismatch in sqlx.
    // CAST(... AS SIGNED) normalizes both to i64, which comfortably holds any
    // real row count.
    let build = |table_columns: &[String]| -> Result<(String, String, Vec<String>), String> {
        let where_clause = build_where(table_columns, search.as_deref(), search_is_regex, search_case_sensitive, &filters)?;
        let order_by = if let Some(col) = sort_col.as_deref() {
            // Validate the sort column against the fetched columns so an unknown
            // name never reaches the query (mirrors the Postgres ensure_column check).
            if !table_columns.iter().any(|c| c == col) {
                return Err(format!("Unknown column: {col}"));
            }
            let dir = match sort_direction.as_deref().unwrap_or("asc") {
                "desc" => "DESC",
                _ => "ASC",
            };
            // Emulate NULLS FIRST/LAST - real MySQL (unlike MariaDB) rejects the
            // `NULLS FIRST/LAST` syntax. `ISNULL(col)` yields 0 for non-NULLs and
            // 1 for NULLs: ordering it ASC keeps NULLs last, DESC puts NULLs first.
            let qc = bt(col);
            match nulls_order.as_deref() {
                Some("first") => format!(" ORDER BY ISNULL({qc}) DESC, {qc} {dir}"),
                _ => format!(" ORDER BY ISNULL({qc}), {qc} {dir}"),
            }
        } else {
            String::new()
        };
        Ok((
            format!("SELECT CAST(COUNT(*) AS SIGNED) FROM {table_ref}{}", where_clause.sql),
            format!("SELECT * FROM {table_ref}{}{} LIMIT ? OFFSET ?", where_clause.sql, order_by),
            where_clause.binds,
        ))
    };
    let run_page = |count_sql: String, data_sql: String, binds: Vec<String>| async move {
        let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
        let mut data_q = sqlx::query(&data_sql);
        for b in &binds {
            count_q = count_q.bind(b.clone());
            data_q = data_q.bind(b.clone());
        }
        data_q = data_q.bind(limit).bind(offset);
        let (t, r) = tokio::join!(count_q.fetch_one(pool), data_q.fetch_all(pool));
        (t, r, count_sql, data_sql)
    };

    let (meta_rows, (total_res, rows_res, count_sql, data_sql), (pk, fks)) = if needs_columns_first {
        // The WHERE and ORDER BY are built from, and validated against, the
        // column list, so it has to land first.
        let meta_rows = meta_q.fetch_all(pool).await.map_err(|e| format!("Failed to load columns: {e}"))?;
        let names: Vec<String> = meta_rows.iter().filter_map(|r| my_text(r, 0)).collect();
        let (count_sql, data_sql, binds) = build(&names)?;
        let (page, meta) = tokio::join!(run_page(count_sql, data_sql, binds), pk_fk);
        (meta_rows, page, meta)
    } else {
        let (count_sql, data_sql, binds) = build(&[])?;
        let (meta_rows, page, meta) = tokio::join!(meta_q.fetch_all(pool), run_page(count_sql, data_sql, binds), pk_fk);
        (meta_rows.map_err(|e| format!("Failed to load columns: {e}"))?, page, meta)
    };
    let total: i64 = total_res.map_err(|e| format!("Failed to count rows: {e}"))?;
    let rows = rows_res.map_err(|e| format!("Failed to fetch rows: {e}"))?;

    // Column flags from information_schema. EXTRA carries `auto_increment` and
    // the generated-column markers; a column with either of those, or with any
    // DEFAULT, is one the insert row must not demand a value for.
    let flags_map: HashMap<String, super::query::ColumnFlags> = meta_rows
        .iter()
        .filter_map(|r| {
            let name = my_text(r, 0)?;
            let nullable = my_text(r, 2)?;
            let extra = my_text(r, 3).unwrap_or_default().to_ascii_lowercase();
            let default = my_text(r, 4);
            let auto_generated = extra.contains("auto_increment") || extra.contains("generated");
            Some((
                name,
                super::query::ColumnFlags {
                    nullable: nullable.eq_ignore_ascii_case("YES"),
                    auto_generated,
                    has_default: auto_generated || default.is_some(),
                },
            ))
        })
        .collect();

    let mut columns: Vec<ColumnInfo> = if let Some(first) = rows.first() {
        first
            .columns()
            .iter()
            .map(|c| ColumnInfo::new(c.name(), c.type_info().name().to_lowercase()))
            .collect()
    } else {
        // Empty table: derive column definitions from information_schema
        meta_rows
            .iter()
            .filter_map(|r| {
                Some(ColumnInfo::new(my_text(r, 0)?, my_text(r, 1)?.to_lowercase()))
            })
            .collect()
    };

    for col in &mut columns {
        if let Some(f) = flags_map.get(&col.name) {
            col.nullable = f.nullable;
            col.auto_generated = f.auto_generated;
            col.has_default = f.has_default;
        }
    }

    let data: Vec<Vec<Value>> = rows
        .iter()
        .map(|row| (0..row.len()).map(|i| cell_to_json(row, i)).collect())
        .collect();

    Ok(TableRows {
        // Preview fetching is a Postgres path (pg_stats + pg_column_size).
        preview_columns: Vec::new(),
        columns,
        rows: data,
        total,
        query_ms: started.elapsed().as_millis() as u64,
        primary_key: pk,
        foreign_keys: fks,
        sql: format!("{data_sql}\n{count_sql}"),
    })
}

async fn fetch_foreign_keys(pool: &MySqlPool, schema: &str, table: &str) -> Result<Vec<ForeignKeyInfo>, String> {
    let rows = sqlx::query(
        "SELECT kcu.CONSTRAINT_NAME, kcu.COLUMN_NAME, \
                kcu.REFERENCED_TABLE_SCHEMA, kcu.REFERENCED_TABLE_NAME, kcu.REFERENCED_COLUMN_NAME \
         FROM information_schema.KEY_COLUMN_USAGE kcu \
         JOIN information_schema.REFERENTIAL_CONSTRAINTS rc \
           ON rc.CONSTRAINT_NAME = kcu.CONSTRAINT_NAME \
          AND rc.CONSTRAINT_SCHEMA = kcu.TABLE_SCHEMA \
         WHERE kcu.TABLE_SCHEMA = ? AND kcu.TABLE_NAME = ? \
           AND kcu.REFERENCED_TABLE_NAME IS NOT NULL \
         ORDER BY kcu.CONSTRAINT_NAME, kcu.ORDINAL_POSITION",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to load foreign keys: {e}"))?;

    let mut out: Vec<ForeignKeyInfo> = Vec::new();
    let mut current: Option<String> = None;
    for row in &rows {
        // my_text: a refused VARBINARY decode here blanked every foreign key on
        // MySQL 8+, so FK jumps and relation chips silently disappeared.
        let constraint = my_text(row, 0).unwrap_or_default();
        let column = my_text(row, 1).unwrap_or_default();
        let ref_schema = my_text(row, 2).unwrap_or_default();
        let ref_table = my_text(row, 3).unwrap_or_default();
        let ref_col = my_text(row, 4).unwrap_or_default();
        if current.as_deref() == Some(&constraint) {
            if let Some(fk) = out.last_mut() {
                fk.columns.push(column);
                fk.referenced_columns.push(ref_col);
            }
        } else {
            current = Some(constraint);
            out.push(ForeignKeyInfo {
                columns: vec![column],
                referenced_schema: ref_schema,
                referenced_table: ref_table,
                referenced_columns: vec![ref_col],
            });
        }
    }
    Ok(out)
}

pub async fn execute_sql(
    pool: &MySqlPool,
    sql: &str,
    // When `Some`, real cancellation is armed: we capture this connection's id and
    // issue `KILL QUERY <id>` on a separate connection if the receiver fires, so
    // the running statement stops server-side. Callers that can't be cancelled
    // (diff/multi paths) pass `None`.
    cancel_rx: Option<tokio::sync::oneshot::Receiver<()>>,
) -> Result<SqlResult, String> {
    let started = Instant::now();
    let query_ms = || started.elapsed().as_millis() as u64;

    // Run on a dedicated connection so its CONNECTION_ID() identifies the exact
    // backend to cancel.
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| format!("Failed to acquire connection: {e}"))?;
    if let Some(rx) = cancel_rx {
        if let Ok(conn_id) = sqlx::query_scalar::<_, u64>("SELECT CONNECTION_ID()")
            .fetch_one(&mut *conn)
            .await
        {
            let cancel_pool = pool.clone();
            tokio::spawn(async move {
                if rx.await.is_ok() {
                    // conn_id is a numeric id from the server; safe to inline.
                    let _ = sqlx::query(&format!("KILL QUERY {conn_id}"))
                        .execute(&cancel_pool)
                        .await;
                }
            });
        }
    }

    let head = super::sql_util::statement_head(sql);
    let is_select = matches!(head.as_str(), "select" | "show" | "explain" | "describe" | "desc" | "with" | "call");

    if is_select {
        let mut stream = sqlx::query(sql).fetch(&mut *conn);
        // Convert each row to JSON as it streams in and drop the driver row
        // immediately - retaining the full Vec<MySqlRow> alongside the JSON rows
        // would double peak memory on a large result.
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
                    data.push((0..row.len()).map(|i| cell_to_json(&row, i)).collect());
                    if data.len() >= EXECUTE_SQL_MAX_ROWS {
                        capped = true;
                        break;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    drop(stream);
                    return Err(format!("Query failed: {e}"));
                }
            }
        }
        drop(stream);

        let row_count = data.len() as i64;
        return Ok(SqlResult {
            columns,
            rows: data,
            row_count: Some(row_count),
            message: if capped {
                Some(format!("Result capped at {EXECUTE_SQL_MAX_ROWS} rows - add a LIMIT clause to fetch a specific range."))
            } else {
                None
            },
            query_ms: query_ms(),
            sql: sql.to_string(),
        });
    }

    let result = sqlx::query(sql).execute(&mut *conn).await.map_err(|e| format!("Statement failed: {e}"))?;
    let affected = result.rows_affected() as i64;
    Ok(SqlResult {
        columns: vec![],
        rows: vec![],
        row_count: Some(affected),
        message: Some(format!("{affected} row(s) affected")),
        query_ms: query_ms(),
        sql: sql.to_string(),
    })
}

pub async fn update_table_cell(
    pool: &MySqlPool,
    schema: &str,
    table: &str,
    primary_key: HashMap<String, Value>,
    column: &str,
    value: &Value,
) -> Result<(), String> {
    let pk_columns = fetch_primary_key(pool, schema, table).await?;
    if pk_columns.is_empty() {
        return Err("Cannot update row: table has no primary key".into());
    }
    let pk_parts: Vec<String> = pk_columns.iter().map(|c| format!("{} = ?", bt(c))).collect();
    let sql = format!("UPDATE {}.{} SET {} = ? WHERE {}", bt(schema), bt(table), bt(column), pk_parts.join(" AND "));

    let mut q = sqlx::query(&sql);
    q = bind_value(q, value);
    for col in &pk_columns {
        let pk_val = primary_key.get(col).ok_or_else(|| format!("Missing primary key column: {col}"))?;
        q = bind_value(q, pk_val);
    }
    q.execute(pool).await.map_err(|e| format!("Update failed: {e}"))?;
    Ok(())
}

pub async fn insert_table_row(
    pool: &MySqlPool,
    schema: &str,
    table: &str,
    values: HashMap<String, Value>,
) -> Result<Vec<Value>, String> {
    let meta_rows = sqlx::query(
        "SELECT COLUMN_NAME, IS_NULLABLE, COLUMN_DEFAULT, EXTRA \
         FROM information_schema.COLUMNS \
         WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? \
         ORDER BY ORDINAL_POSITION",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to load column metadata: {e}"))?;

    if meta_rows.is_empty() {
        return Err(format!("Table not found: {schema}.{table}"));
    }

    let mut column_order: Vec<String> = Vec::new();
    let mut optional: HashMap<String, bool> = HashMap::new();
    let mut auto_increment_col: Option<String> = None;

    for row in &meta_rows {
        // my_text throughout: on MySQL 8+ these can arrive VARBINARY. A refused
        // decode of EXTRA silently lost AUTO_INCREMENT, so the new id was never
        // read back and the inserted row came back with a NULL key.
        let name = my_text(row, 0).ok_or("Invalid column name in information_schema")?;
        let is_nullable = my_text(row, 1).unwrap_or_else(|| "NO".to_string());
        let default_val = my_text(row, 2);
        let extra = my_text(row, 3).unwrap_or_default();
        let is_auto = extra.to_lowercase().contains("auto_increment");
        let opt = is_auto || default_val.is_some() || is_nullable.eq_ignore_ascii_case("YES");
        if is_auto {
            auto_increment_col = Some(name.clone());
        }
        column_order.push(name.clone());
        optional.insert(name, opt);
    }

    for col in values.keys() {
        if !optional.contains_key(col) {
            return Err(format!("Unknown column: {col}"));
        }
    }
    for (name, &opt) in &optional {
        if !opt && !values.contains_key(name) {
            return Err(format!("Column \"{name}\" is required (NOT NULL, no default)"));
        }
    }

    let mut col_names: Vec<String> = values.keys().cloned().collect();
    col_names.sort();

    let cols: Vec<String> = col_names.iter().map(|c| bt(c)).collect();
    let placeholders: Vec<&str> = col_names.iter().map(|_| "?").collect();
    let sql = format!("INSERT INTO {}.{} ({}) VALUES ({})", bt(schema), bt(table), cols.join(", "), placeholders.join(", "));

    let mut q = sqlx::query(&sql);
    for col in &col_names {
        let value = values
            .get(col)
            .ok_or_else(|| format!("Missing value for column: {col}"))?;
        q = bind_value(q, value);
    }
    // Run the INSERT and the LAST_INSERT_ID()/re-fetch on ONE pinned connection:
    // LAST_INSERT_ID() is connection-scoped, and a second pool acquire can land
    // on a different connection (returning 0 or a stale id) whenever background
    // work is also using the pool.
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| format!("Failed to acquire connection: {e}"))?;
    q.execute(&mut *conn).await.map_err(|e| format!("Insert failed: {e}"))?;

    // Re-fetch the inserted row
    let fetched = if let Some(ai_col) = &auto_increment_col {
        let last_id: u64 = sqlx::query_scalar("SELECT LAST_INSERT_ID()")
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| format!("Failed to get last insert ID: {e}"))?;
        let sel = format!("SELECT * FROM {}.{} WHERE {} = ? LIMIT 1", bt(schema), bt(table), bt(ai_col));
        sqlx::query(&sel).bind(last_id as i64).fetch_optional(&mut *conn).await
            .map_err(|e| format!("Failed to fetch inserted row: {e}"))?
    } else {
        let pk_cols = fetch_primary_key(pool, schema, table).await.unwrap_or_default();
        if !pk_cols.is_empty() && pk_cols.iter().all(|c| values.contains_key(c)) {
            let where_parts: Vec<String> = pk_cols.iter().map(|c| format!("{} = ?", bt(c))).collect();
            let sel = format!("SELECT * FROM {}.{} WHERE {} LIMIT 1", bt(schema), bt(table), where_parts.join(" AND "));
            let mut sel_q = sqlx::query(&sel);
            for pk_col in &pk_cols {
                let value = values
                    .get(pk_col)
                    .ok_or_else(|| format!("Missing value for column: {pk_col}"))?;
                sel_q = bind_value(sel_q, value);
            }
            sel_q.fetch_optional(&mut *conn).await.map_err(|e| format!("Failed to fetch inserted row: {e}"))?
        } else {
            None
        }
    };

    if let Some(row) = fetched {
        Ok(column_order
            .iter()
            .map(|name| {
                let idx = row.columns().iter().position(|c| c.name() == name.as_str()).unwrap_or(0);
                cell_to_json(&row, idx)
            })
            .collect())
    } else {
        Ok(column_order.iter().map(|name| values.get(name).cloned().unwrap_or(Value::Null)).collect())
    }
}

pub async fn delete_table_rows(
    pool: &MySqlPool,
    schema: &str,
    table: &str,
    primary_keys: Vec<HashMap<String, Value>>,
) -> Result<u64, String> {
    if primary_keys.is_empty() {
        return Ok(0);
    }
    let pk_columns = fetch_primary_key(pool, schema, table).await?;
    if pk_columns.is_empty() {
        return Err("Cannot delete rows: table has no primary key".into());
    }

    // Single-column PK: batch into `IN (…)` chunks so deleting N selected rows
    // doesn't cost N round-trips. Composite PKs keep the per-row loop.
    if pk_columns.len() == 1 {
        let col = &pk_columns[0];
        let mut total = 0u64;
        for chunk in primary_keys.chunks(100) {
            let placeholders = vec!["?"; chunk.len()].join(", ");
            let sql = format!("DELETE FROM {}.{} WHERE {} IN ({placeholders})", bt(schema), bt(table), bt(col));
            let mut q = sqlx::query(&sql);
            for pk_map in chunk {
                let val = pk_map.get(col).ok_or_else(|| format!("Missing primary key: {col}"))?;
                q = bind_value(q, val);
            }
            let res = q.execute(pool).await.map_err(|e| format!("Delete failed: {e}"))?;
            total += res.rows_affected();
        }
        return Ok(total);
    }

    let where_parts: Vec<String> = pk_columns.iter().map(|c| format!("{} = ?", bt(c))).collect();
    let sql = format!("DELETE FROM {}.{} WHERE {}", bt(schema), bt(table), where_parts.join(" AND "));

    let mut total = 0u64;
    for pk_map in primary_keys {
        let mut q = sqlx::query(&sql);
        for col in &pk_columns {
            let val = pk_map.get(col).ok_or_else(|| format!("Missing primary key: {col}"))?;
            q = bind_value(q, val);
        }
        let res = q.execute(pool).await.map_err(|e| format!("Delete failed: {e}"))?;
        total += res.rows_affected();
    }
    Ok(total)
}

fn bind_value<'q>(
    q: sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>,
    value: &'q Value,
) -> sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments> {
    match value {
        Value::Null => q.bind(None::<String>),
        Value::Bool(b) => q.bind(*b as i64),
        Value::Number(n) if n.is_i64() => q.bind(n.as_i64().unwrap()),
        Value::Number(n) if n.is_u64() => q.bind(n.as_u64().unwrap() as i64),
        Value::Number(n) => q.bind(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => q.bind(s.as_str()),
        other => q.bind(other.to_string()),
    }
}


#[cfg(test)]
mod tests {
    use super::is_plain_text;

    #[test]
    fn a_database_name_is_plain_text() {
        // The case that broke the switcher: `SHOW DATABASES` types this column
        // as VARBINARY, so it arrived as bytes and was rendered "[18 bytes]".
        assert!(is_plain_text("information_schema"));
        assert!(is_plain_text("shop"));
    }

    #[test]
    fn text_with_ordinary_whitespace_is_still_text() {
        assert!(is_plain_text("two words"));
        assert!(is_plain_text("a\tb\nc\r"));
    }

    #[test]
    fn control_bytes_and_nuls_are_not_text() {
        assert!(!is_plain_text("\u{0}"));
        assert!(!is_plain_text("png\u{1}\u{2}"));
        assert!(!is_plain_text("\u{7}bell"));
    }

    #[test]
    fn empty_is_not_text() {
        // An empty BLOB is not a name; let it report its length instead.
        assert!(!is_plain_text(""));
    }
}

/// Against the dialect-matrix container (`docker compose -f docker/dialects.yml
/// up -d mysql`), like `dialect_matrix`: `cargo test --lib mysql_live -- --ignored`.
#[cfg(test)]
mod mysql_live {
    use super::*;

    async fn pool() -> MySqlPool {
        MySqlPool::connect("mysql://root:stroke@127.0.0.1:53306/shop").await.expect("stroke-test-mysql is running")
    }

    /// A plain open fetches everything in one stage; a sorted one fetches the
    /// columns first. Both must return the same rows, columns, key and count.
    #[tokio::test]
    #[ignore]
    async fn plain_and_sorted_opens_agree() {
        let pool = pool().await;
        let plain = get_table_rows(&pool, "shop", "customers", 50, 0, None, false, false, None, None, None, true, None)
            .await
            .unwrap();
        assert!(!plain.columns.is_empty() && !plain.rows.is_empty());
        assert_eq!(plain.total, plain.rows.len() as i64);
        assert!(!plain.primary_key.is_empty(), "primary key read back");
        let first = plain.columns[0].name.clone();
        let sorted = get_table_rows(&pool, "shop", "customers", 50, 0, None, false, false, Some(first), Some("desc".into()), None, true, None)
            .await
            .unwrap();
        assert_eq!(sorted.columns.len(), plain.columns.len());
        assert_eq!(sorted.total, plain.total);
        assert_eq!(sorted.primary_key, plain.primary_key);
        assert!(get_table_rows(&pool, "shop", "customers", 50, 0, None, false, false, Some("nope".into()), None, None, true, None)
            .await
            .unwrap_err()
            .contains("Unknown column"));
    }

    /// The bug behind "No columns visible": an empty table must still come back
    /// with its columns, from the catalog.
    #[tokio::test]
    #[ignore]
    async fn an_empty_table_still_has_columns() {
        let pool = pool().await;
        sqlx::query("CREATE TABLE IF NOT EXISTS stroke_empty_probe (id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY, name TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        let r = get_table_rows(&pool, "shop", "stroke_empty_probe", 50, 0, None, false, false, None, None, None, true, None)
            .await
            .unwrap();
        let names: Vec<&str> = r.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["id", "name"]);
        assert_eq!((r.total, r.rows.len()), (0, 0));
        assert_eq!(r.primary_key, ["id"]);
        sqlx::query("DROP TABLE stroke_empty_probe").execute(&pool).await.unwrap();
    }
}
