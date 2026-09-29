/*!
Bulk row import into an existing table.

The frontend parses the file (CSV/TSV/JSON/JSONL) and maps its fields onto
columns; this module is handed a column list and a block of rows and gets them
into the table. Values are bound as parameters, never interpolated into SQL.

Rows go in batched multi-row INSERTs. When a batch fails and the caller asked to
keep going, the batch is replayed one row at a time so the report can name the
rows that were actually bad instead of blaming all 500.
*/

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use super::connection::{require_conn, ActiveConnection, DbState};
use super::query::{bind_typed_value, is_bytea_type, pg_insert_meta, validate_typed_value};
use super::schema::validate_ident;

/// Cooperative cancel flag for the in-flight import, set by `import_cancel`.
/// Only one import runs at a time in the UI, so a single flag suffices.
static IMPORT_CANCELLED: AtomicBool = AtomicBool::new(false);

fn reset_cancel() { IMPORT_CANCELLED.store(false, Ordering::SeqCst); }
fn is_cancelled() -> bool { IMPORT_CANCELLED.load(Ordering::SeqCst) }

/// Request cancellation of the running import. The loop stops at the next batch
/// boundary; whether the rows already written survive depends on `on_error`
/// (see `ImportOptions::abort_on_error`).
#[tauri::command]
pub fn import_cancel() {
    IMPORT_CANCELLED.store(true, Ordering::SeqCst);
}

// ── Public types ──────────────────────────────────────────────────────────────

/// What to do when a row collides with an existing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConflictMode {
    /// Let the constraint violation surface as an error for that row.
    #[default]
    Error,
    /// Keep the existing row.
    Skip,
    /// Overwrite the existing row's non-key columns.
    Update,
}

fn default_batch_size() -> usize { 500 }
fn default_true() -> bool { true }

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOptions {
    #[serde(default)]
    pub conflict: ConflictMode,
    /// Columns that decide a conflict. Required by `ConflictMode::Update`;
    /// `Skip` falls back to "any constraint" when this is empty.
    #[serde(default)]
    pub conflict_columns: Vec<String>,
    /// True (the default) runs the whole import as one transaction that is
    /// rolled back on the first failure - the file either lands completely or
    /// not at all. False commits batch by batch and reports the rows it could
    /// not place.
    #[serde(default = "default_true")]
    pub abort_on_error: bool,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            conflict: ConflictMode::default(),
            conflict_columns: Vec::new(),
            abort_on_error: true,
            batch_size: default_batch_size(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowError {
    /// 1-based index into the rows as submitted, so the UI can point at the
    /// line the user is looking at.
    pub row: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRowsResult {
    pub inserted: usize,
    pub failed: usize,
    /// Rows never attempted, because the run was cancelled or aborted early.
    pub skipped: usize,
    pub cancelled: bool,
    /// True when nothing was committed - an aborted or cancelled transactional
    /// run. Lets the UI say "nothing changed" rather than leaving the user to
    /// guess what landed.
    pub rolled_back: bool,
    pub errors: Vec<RowError>,
}

#[derive(Debug, Clone, Serialize)]
struct ImportLog {
    level: &'static str,
    message: String,
}

fn emit_log(app: &AppHandle, level: &'static str, message: impl Into<String>) {
    app.emit("import-log", ImportLog { level, message: message.into() }).ok();
}

// ── Tauri command ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn import_rows(
    app: AppHandle,
    state: State<'_, DbState>,
    schema: Option<String>,
    table: String,
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
    options: Option<ImportOptions>,
) -> Result<ImportRowsResult, String> {
    let opts = options.unwrap_or_default();
    if columns.is_empty() {
        return Err("Select at least one column to import into".into());
    }
    if rows.is_empty() {
        return Err("There are no rows to import".into());
    }
    if let Some(bad) = rows.iter().position(|r| r.len() != columns.len()) {
        return Err(format!(
            "Row {} has {} value(s) but {} column(s) were selected",
            bad + 1,
            rows[bad].len(),
            columns.len()
        ));
    }
    if opts.conflict == ConflictMode::Update && opts.conflict_columns.is_empty() {
        return Err("Choose the column(s) that identify an existing row before updating on conflict".into());
    }
    for col in columns.iter().chain(opts.conflict_columns.iter()) {
        validate_ident(col)?;
    }
    validate_ident(&table)?;

    reset_cancel();
    emit_log(&app, "info", format!("Importing {} row(s) into {table}…", rows.len()));

    let result = match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => {
            let schema = schema.unwrap_or_else(|| "public".to_string());
            import_postgres(&app, &pool, &schema, &table, &columns, &rows, &opts).await
        }
        ActiveConnection::Mysql(pool) => {
            import_mysql(&app, &pool, schema.as_deref(), &table, &columns, &rows, &opts).await
        }
        ActiveConnection::Sqlite(pool) => {
            import_sqlite(&app, &pool, &table, &columns, &rows, &opts).await
        }
        ActiveConnection::Duckdb(_) => Err("Importing rows is not supported for DuckDB yet".into()),
        ActiveConnection::Mssql(_) => Err("Importing rows is not supported for SQL Server yet".into()),
        ActiveConnection::D1(_) => Err("Importing rows is not supported for Cloudflare D1 yet".into()),
        ActiveConnection::LibSql(_) => Err("Importing rows is not supported for LibSQL/Turso yet".into()),
        ActiveConnection::Clickhouse(_) => {
            Err("Importing rows is not supported for ClickHouse. Use INSERT INTO … in the SQL console.".into())
        }
        ActiveConnection::Posthog(_) => Err("PostHog is read-only: its data comes from HogQL queries.".into()),
        ActiveConnection::Redis(_) => Err("Importing rows is not supported on Redis".into()),
    }?;

    let level = if result.failed > 0 || result.cancelled { "warn" } else { "ok" };
    emit_log(
        &app,
        level,
        if result.rolled_back {
            format!("Rolled back - nothing was imported ({} error(s))", result.errors.len())
        } else {
            format!("Imported {} row(s), {} failed", result.inserted, result.failed)
        },
    );
    Ok(result)
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// How many rows to put in one statement.
///
/// Postgres and MySQL both cap a statement's parameters at 65535, so a wide
/// table needs a smaller batch than a narrow one or the statement is rejected
/// outright.
fn rows_per_batch(requested: usize, columns: usize) -> usize {
    let cap = if columns == 0 { 1 } else { 65535 / columns.max(1) };
    requested.clamp(1, cap.max(1))
}

/// Progress every few batches, so a long import shows movement without
/// flooding the log with one line per batch.
fn emit_progress(app: &AppHandle, done: usize, total: usize) {
    emit_log(app, "info", format!("  {done}/{total} rows…"));
}

// ── PostgreSQL ────────────────────────────────────────────────────────────────

async fn import_postgres(
    app: &AppHandle,
    pool: &sqlx::PgPool,
    schema: &str,
    table: &str,
    columns: &[String],
    rows: &[Vec<Value>],
    opts: &ImportOptions,
) -> Result<ImportRowsResult, String> {
    let (_, meta) = pg_insert_meta(pool, schema, table).await?;

    for col in columns {
        let m = meta
            .get(col)
            .ok_or_else(|| format!("Column \"{col}\" does not exist in {schema}.{table}"))?;
        if is_bytea_type(&m.data_type) {
            return Err(format!("Cannot import into bytea column \"{col}\""));
        }
    }
    for col in &opts.conflict_columns {
        if !meta.contains_key(col) {
            return Err(format!("Conflict column \"{col}\" does not exist in {schema}.{table}"));
        }
    }
    // A required column left out of the mapping fails on every single row, so
    // say so once, up front, instead of returning one error per row.
    for m in meta.values() {
        if !m.optional_when_omitted && !columns.iter().any(|c| c == &m.name) {
            return Err(format!(
                "Column \"{}\" is required (NOT NULL, no default) - map a field to it",
                m.name
            ));
        }
    }

    let col_sql = columns.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    let conflict_sql = pg_conflict_sql(columns, opts)?;
    let per_batch = rows_per_batch(opts.batch_size, columns.len());

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let mut inserted = 0usize;
    let mut errors: Vec<RowError> = Vec::new();
    let mut attempted = 0usize;

    for (batch_no, batch) in rows.chunks(per_batch).enumerate() {
        if is_cancelled() { break; }
        let base = batch_no * per_batch;

        let sql = pg_batch_sql(schema, table, &col_sql, columns, &meta, batch.len(), &conflict_sql)?;
        // A whole batch is one savepoint: on failure it rolls back to here and
        // the rows are retried individually, so one bad row doesn't discard the
        // 499 good ones beside it.
        sqlx::query("SAVEPOINT stroke_import").execute(&mut *tx).await.ok();
        let mut q = sqlx::query(&sql);
        for row in batch {
            for (value, col) in row.iter().zip(columns) {
                let m = &meta[col];
                q = bind_typed_value(q, &m.data_type, value)?;
            }
        }

        match q.execute(&mut *tx).await {
            Ok(_) => {
                sqlx::query("RELEASE SAVEPOINT stroke_import").execute(&mut *tx).await.ok();
                inserted += batch.len();
                attempted += batch.len();
            }
            Err(batch_err) => {
                sqlx::query("ROLLBACK TO SAVEPOINT stroke_import").execute(&mut *tx).await.ok();
                if opts.abort_on_error {
                    tx.rollback().await.ok();
                    return Ok(ImportRowsResult {
                        inserted: 0,
                        failed: 1,
                        skipped: rows.len(),
                        cancelled: false,
                        rolled_back: true,
                        errors: vec![RowError {
                            row: base + 1,
                            message: format!("{batch_err} (rows {}-{})", base + 1, base + batch.len()),
                        }],
                    });
                }

                // Replay row by row to find which ones are actually bad.
                for (i, row) in batch.iter().enumerate() {
                    if is_cancelled() { break; }
                    attempted += 1;
                    let one = pg_batch_sql(schema, table, &col_sql, columns, &meta, 1, &conflict_sql)?;
                    sqlx::query("SAVEPOINT stroke_import_row").execute(&mut *tx).await.ok();
                    let mut rq = sqlx::query(&one);
                    let mut bind_err = None;
                    for (value, col) in row.iter().zip(columns) {
                        let m = &meta[col];
                        match validate_typed_value(&m.data_type, value)
                            .and_then(|_| bind_typed_value(rq, &m.data_type, value))
                        {
                            Ok(next) => rq = next,
                            Err(e) => { bind_err = Some(e); rq = sqlx::query(&one); break; }
                        }
                    }
                    if let Some(e) = bind_err {
                        sqlx::query("ROLLBACK TO SAVEPOINT stroke_import_row").execute(&mut *tx).await.ok();
                        errors.push(RowError { row: base + i + 1, message: e });
                        continue;
                    }
                    match rq.execute(&mut *tx).await {
                        Ok(_) => {
                            sqlx::query("RELEASE SAVEPOINT stroke_import_row").execute(&mut *tx).await.ok();
                            inserted += 1;
                        }
                        Err(e) => {
                            sqlx::query("ROLLBACK TO SAVEPOINT stroke_import_row").execute(&mut *tx).await.ok();
                            errors.push(RowError { row: base + i + 1, message: e.to_string() });
                        }
                    }
                }
            }
        }

        if (batch_no + 1) % 10 == 0 {
            emit_progress(app, attempted.min(rows.len()), rows.len());
        }
    }

    let cancelled = is_cancelled();
    if cancelled && opts.abort_on_error {
        // Everything so far is inside this transaction, so the cancel can still
        // undo all of it - which is what "all or nothing" promised.
        tx.rollback().await.ok();
        return Ok(ImportRowsResult {
            inserted: 0,
            failed: errors.len(),
            skipped: rows.len(),
            cancelled: true,
            rolled_back: true,
            errors,
        });
    }
    tx.commit().await.map_err(|e| format!("Could not commit the import: {e}"))?;

    Ok(ImportRowsResult {
        skipped: rows.len().saturating_sub(inserted + errors.len()),
        inserted,
        failed: errors.len(),
        cancelled,
        rolled_back: false,
        errors,
    })
}

/// `VALUES ($1,$2),($3,$4)…` with each placeholder carrying the cast its column
/// needs (enum, array, json, timestamp), then the ON CONFLICT clause.
fn pg_batch_sql(
    schema: &str,
    table: &str,
    col_sql: &str,
    columns: &[String],
    meta: &HashMap<String, super::query::PgInsertColumnMeta>,
    row_count: usize,
    conflict_sql: &str,
) -> Result<String, String> {
    let mut bind_idx = 1u32;
    let mut tuples: Vec<String> = Vec::with_capacity(row_count);
    for _ in 0..row_count {
        let mut placeholders: Vec<String> = Vec::with_capacity(columns.len());
        for col in columns {
            let m = meta
                .get(col)
                .ok_or_else(|| format!("Unknown column: {col}"))?;
            placeholders.push(m.pg.insert_value_sql(bind_idx)?);
            bind_idx += 1;
        }
        tuples.push(format!("({})", placeholders.join(", ")));
    }
    Ok(format!(
        "INSERT INTO \"{schema}\".\"{table}\" ({col_sql}) VALUES {}{conflict_sql}",
        tuples.join(", ")
    ))
}

fn pg_conflict_sql(columns: &[String], opts: &ImportOptions) -> Result<String, String> {
    let target = || {
        opts.conflict_columns
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Ok(match opts.conflict {
        ConflictMode::Error => String::new(),
        ConflictMode::Skip => {
            if opts.conflict_columns.is_empty() {
                " ON CONFLICT DO NOTHING".to_string()
            } else {
                format!(" ON CONFLICT ({}) DO NOTHING", target())
            }
        }
        ConflictMode::Update => {
            // Every mapped column except the key ones takes the incoming value.
            let sets: Vec<String> = columns
                .iter()
                .filter(|c| !opts.conflict_columns.contains(c))
                .map(|c| format!("\"{c}\" = EXCLUDED.\"{c}\""))
                .collect();
            if sets.is_empty() {
                return Err(
                    "Every mapped column is a conflict key, so there is nothing left to update"
                        .into(),
                );
            }
            format!(" ON CONFLICT ({}) DO UPDATE SET {}", target(), sets.join(", "))
        }
    })
}

// ── MySQL ─────────────────────────────────────────────────────────────────────

async fn import_mysql(
    app: &AppHandle,
    pool: &sqlx::MySqlPool,
    schema: Option<&str>,
    table: &str,
    columns: &[String],
    rows: &[Vec<Value>],
    opts: &ImportOptions,
) -> Result<ImportRowsResult, String> {
    let qualified = match schema {
        Some(s) if !s.is_empty() => { validate_ident(s)?; format!("`{s}`.`{table}`") }
        _ => format!("`{table}`"),
    };
    let col_sql = columns.iter().map(|c| format!("`{c}`")).collect::<Vec<_>>().join(", ");
    let suffix = match opts.conflict {
        ConflictMode::Error => String::new(),
        // MySQL has no conflict target: IGNORE covers any unique constraint.
        ConflictMode::Skip => String::new(),
        ConflictMode::Update => {
            let sets: Vec<String> = columns
                .iter()
                .filter(|c| !opts.conflict_columns.contains(c))
                .map(|c| format!("`{c}` = VALUES(`{c}`)"))
                .collect();
            if sets.is_empty() {
                return Err("Every mapped column is a conflict key, so there is nothing left to update".into());
            }
            format!(" ON DUPLICATE KEY UPDATE {}", sets.join(", "))
        }
    };
    let verb = if opts.conflict == ConflictMode::Skip { "INSERT IGNORE" } else { "INSERT" };
    let per_batch = rows_per_batch(opts.batch_size, columns.len());

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let mut inserted = 0usize;
    let mut errors: Vec<RowError> = Vec::new();

    for (batch_no, batch) in rows.chunks(per_batch).enumerate() {
        if is_cancelled() { break; }
        let base = batch_no * per_batch;
        let tuple = format!("({})", vec!["?"; columns.len()].join(", "));
        let sql = format!(
            "{verb} INTO {qualified} ({col_sql}) VALUES {}{suffix}",
            vec![tuple.as_str(); batch.len()].join(", ")
        );
        let mut q = sqlx::query(&sql);
        for row in batch {
            for value in row {
                q = bind_mysql_value(q, value);
            }
        }
        match q.execute(&mut *tx).await {
            Ok(_) => inserted += batch.len(),
            Err(batch_err) => {
                if opts.abort_on_error {
                    tx.rollback().await.ok();
                    return Ok(ImportRowsResult {
                        inserted: 0,
                        failed: 1,
                        skipped: rows.len(),
                        cancelled: false,
                        rolled_back: true,
                        errors: vec![RowError {
                            row: base + 1,
                            message: format!("{batch_err} (rows {}-{})", base + 1, base + batch.len()),
                        }],
                    });
                }
                // MySQL aborts only the failing statement, so the rows can be
                // replayed individually without unwinding the transaction.
                let one = format!("{verb} INTO {qualified} ({col_sql}) VALUES {tuple}{suffix}");
                for (i, row) in batch.iter().enumerate() {
                    if is_cancelled() { break; }
                    let mut rq = sqlx::query(&one);
                    for value in row {
                        rq = bind_mysql_value(rq, value);
                    }
                    match rq.execute(&mut *tx).await {
                        Ok(_) => inserted += 1,
                        Err(e) => errors.push(RowError { row: base + i + 1, message: e.to_string() }),
                    }
                }
            }
        }
        if (batch_no + 1) % 10 == 0 {
            emit_progress(app, (inserted + errors.len()).min(rows.len()), rows.len());
        }
    }

    let cancelled = is_cancelled();
    if cancelled && opts.abort_on_error {
        tx.rollback().await.ok();
        return Ok(ImportRowsResult {
            inserted: 0,
            failed: errors.len(),
            skipped: rows.len(),
            cancelled: true,
            rolled_back: true,
            errors,
        });
    }
    tx.commit().await.map_err(|e| format!("Could not commit the import: {e}"))?;

    Ok(ImportRowsResult {
        skipped: rows.len().saturating_sub(inserted + errors.len()),
        inserted,
        failed: errors.len(),
        cancelled,
        rolled_back: false,
        errors,
    })
}

fn bind_mysql_value<'q>(
    q: sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>,
    value: &'q Value,
) -> sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments> {
    match value {
        Value::Null => q.bind(None::<String>),
        Value::Bool(b) => q.bind(*b as i64),
        Value::Number(n) if n.is_i64() => q.bind(n.as_i64().unwrap_or_default()),
        Value::Number(n) if n.is_u64() => q.bind(n.as_u64().unwrap_or_default() as i64),
        Value::Number(n) => q.bind(n.as_f64().unwrap_or_default()),
        Value::String(s) => q.bind(s.as_str()),
        // Objects and arrays go in as their JSON text, which is what a JSON
        // column wants and what a text column would have held anyway.
        other => q.bind(other.to_string()),
    }
}

// ── SQLite ────────────────────────────────────────────────────────────────────

async fn import_sqlite(
    app: &AppHandle,
    pool: &sqlx::SqlitePool,
    table: &str,
    columns: &[String],
    rows: &[Vec<Value>],
    opts: &ImportOptions,
) -> Result<ImportRowsResult, String> {
    let col_sql = columns.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    let suffix = match opts.conflict {
        ConflictMode::Error => String::new(),
        ConflictMode::Skip => {
            if opts.conflict_columns.is_empty() {
                " ON CONFLICT DO NOTHING".to_string()
            } else {
                format!(
                    " ON CONFLICT ({}) DO NOTHING",
                    opts.conflict_columns.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ")
                )
            }
        }
        ConflictMode::Update => {
            let sets: Vec<String> = columns
                .iter()
                .filter(|c| !opts.conflict_columns.contains(c))
                .map(|c| format!("\"{c}\" = excluded.\"{c}\""))
                .collect();
            if sets.is_empty() {
                return Err("Every mapped column is a conflict key, so there is nothing left to update".into());
            }
            format!(
                " ON CONFLICT ({}) DO UPDATE SET {}",
                opts.conflict_columns.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", "),
                sets.join(", ")
            )
        }
    };
    // SQLite's compiled-in ceiling is 999 parameters by default.
    let per_batch = opts.batch_size.clamp(1, (999 / columns.len().max(1)).max(1));

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let mut inserted = 0usize;
    let mut errors: Vec<RowError> = Vec::new();

    for (batch_no, batch) in rows.chunks(per_batch).enumerate() {
        if is_cancelled() { break; }
        let base = batch_no * per_batch;
        let tuple = format!("({})", vec!["?"; columns.len()].join(", "));
        let sql = format!(
            "INSERT INTO \"{table}\" ({col_sql}) VALUES {}{suffix}",
            vec![tuple.as_str(); batch.len()].join(", ")
        );
        let mut q = sqlx::query(&sql);
        for row in batch {
            for value in row {
                q = bind_sqlite_value(q, value);
            }
        }
        match q.execute(&mut *tx).await {
            Ok(_) => inserted += batch.len(),
            Err(batch_err) => {
                if opts.abort_on_error {
                    tx.rollback().await.ok();
                    return Ok(ImportRowsResult {
                        inserted: 0,
                        failed: 1,
                        skipped: rows.len(),
                        cancelled: false,
                        rolled_back: true,
                        errors: vec![RowError {
                            row: base + 1,
                            message: format!("{batch_err} (rows {}-{})", base + 1, base + batch.len()),
                        }],
                    });
                }
                let one = format!("INSERT INTO \"{table}\" ({col_sql}) VALUES {tuple}{suffix}");
                for (i, row) in batch.iter().enumerate() {
                    if is_cancelled() { break; }
                    let mut rq = sqlx::query(&one);
                    for value in row {
                        rq = bind_sqlite_value(rq, value);
                    }
                    match rq.execute(&mut *tx).await {
                        Ok(_) => inserted += 1,
                        Err(e) => errors.push(RowError { row: base + i + 1, message: e.to_string() }),
                    }
                }
            }
        }
        if (batch_no + 1) % 10 == 0 {
            emit_progress(app, (inserted + errors.len()).min(rows.len()), rows.len());
        }
    }

    let cancelled = is_cancelled();
    if cancelled && opts.abort_on_error {
        tx.rollback().await.ok();
        return Ok(ImportRowsResult {
            inserted: 0,
            failed: errors.len(),
            skipped: rows.len(),
            cancelled: true,
            rolled_back: true,
            errors,
        });
    }
    tx.commit().await.map_err(|e| format!("Could not commit the import: {e}"))?;

    Ok(ImportRowsResult {
        skipped: rows.len().saturating_sub(inserted + errors.len()),
        inserted,
        failed: errors.len(),
        cancelled,
        rolled_back: false,
        errors,
    })
}

fn bind_sqlite_value<'a>(
    q: sqlx::query::Query<'a, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'a>>,
    value: &Value,
) -> sqlx::query::Query<'a, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'a>> {
    match value {
        Value::Null => q.bind(None::<String>),
        Value::Bool(b) => q.bind(*b as i64),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() { q.bind(i) }
            else if let Some(f) = n.as_f64() { q.bind(f) }
            else { q.bind(n.to_string()) }
        }
        Value::String(s) => q.bind(s.clone()),
        other => q.bind(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_size_stays_under_the_parameter_ceiling() {
        // 60 columns x 500 rows would be 30,000 binds - fine. 60 x 2000 would
        // be 120,000, past the 65535 a statement can carry.
        assert_eq!(rows_per_batch(500, 60), 500);
        assert_eq!(rows_per_batch(2000, 60), 65535 / 60);
        // A single row must always be allowed through, however wide.
        assert_eq!(rows_per_batch(500, 70_000), 1);
        assert_eq!(rows_per_batch(0, 10), 1);
    }

    fn opts(conflict: ConflictMode, keys: &[&str]) -> ImportOptions {
        ImportOptions {
            conflict,
            conflict_columns: keys.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn conflict_clause_matches_the_chosen_mode() {
        let cols = vec!["id".to_string(), "name".to_string()];
        assert_eq!(pg_conflict_sql(&cols, &opts(ConflictMode::Error, &[])).unwrap(), "");
        assert_eq!(
            pg_conflict_sql(&cols, &opts(ConflictMode::Skip, &[])).unwrap(),
            " ON CONFLICT DO NOTHING"
        );
        assert_eq!(
            pg_conflict_sql(&cols, &opts(ConflictMode::Skip, &["id"])).unwrap(),
            " ON CONFLICT (\"id\") DO NOTHING"
        );
        assert_eq!(
            pg_conflict_sql(&cols, &opts(ConflictMode::Update, &["id"])).unwrap(),
            " ON CONFLICT (\"id\") DO UPDATE SET \"name\" = EXCLUDED.\"name\""
        );
    }

    #[test]
    fn updating_on_conflict_needs_a_non_key_column() {
        let cols = vec!["id".to_string()];
        assert!(pg_conflict_sql(&cols, &opts(ConflictMode::Update, &["id"])).is_err());
    }
}
