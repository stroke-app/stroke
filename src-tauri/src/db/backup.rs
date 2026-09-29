/*!
Backup and restore for all supported database engines.

Export produces a plain SQL script that can be re-executed to restore.
Options control which object types are included (schema DDL, data, sequences,
enums, functions, triggers, views).
*/

use std::sync::atomic::{AtomicBool, Ordering};

use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use sqlx::{Column, Row, TypeInfo};
use tauri::{AppHandle, Emitter, State};

use super::connection::{require_conn, ActiveConnection, DbState};

/// Cooperative cancel flag for the in-flight backup/restore. The frontend sets
/// it via `backup_cancel`; the export/import loops poll it and stop early.
/// Only one backup/restore runs at a time in the UI, so a single flag suffices.
static BACKUP_CANCELLED: AtomicBool = AtomicBool::new(false);

fn reset_cancel() { BACKUP_CANCELLED.store(false, Ordering::SeqCst); }
fn is_cancelled() -> bool { BACKUP_CANCELLED.load(Ordering::SeqCst) }

/// Request cancellation of the running backup/restore. The loops stop at the
/// next table/statement boundary and return the work completed so far.
#[tauri::command]
pub fn backup_cancel() {
    BACKUP_CANCELLED.store(true, Ordering::SeqCst);
}

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub sql: String,
    pub table_count: usize,
    pub row_count: usize,
    /// True when the run stopped at a cancel request, so `sql` covers only the
    /// objects reached before it. The script is a partial dump and must not be
    /// offered as a usable backup.
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub statements_ok: usize,
    pub statements_err: usize,
    /// Statements never attempted because the run was cancelled. Counting these
    /// as failures (the old `total - ok`) reported a restore stopped after 10 of
    /// 1000 statements as "990 failed" with no errors to show for it.
    pub statements_skipped: usize,
    pub cancelled: bool,
    pub errors: Vec<String>,
}

fn default_true() -> bool { true }

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    #[serde(default = "default_true")]
    pub include_schema: bool,
    #[serde(default = "default_true")]
    pub include_data: bool,
    #[serde(default = "default_true")]
    pub include_sequences: bool,
    #[serde(default = "default_true")]
    pub include_enums: bool,
    #[serde(default = "default_true")]
    pub include_functions: bool,
    #[serde(default = "default_true")]
    pub include_triggers: bool,
    #[serde(default = "default_true")]
    pub include_views: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            include_schema: true,
            include_data: true,
            include_sequences: true,
            include_enums: true,
            include_functions: true,
            include_triggers: true,
            include_views: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct BackupLog {
    level: &'static str,
    message: String,
}

fn emit_log(app: &AppHandle, event: &str, level: &'static str, message: impl Into<String>) {
    app.emit(event, BackupLog { level, message: message.into() }).ok();
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn backup_export(
    app: AppHandle,
    state: State<'_, DbState>,
    schema: Option<String>,
    tables: Option<Vec<String>>,
    options: Option<ExportOptions>,
) -> Result<ExportResult, String> {
    let opts = options.unwrap_or_default();
    reset_cancel();
    let result = export_one(&app, &state, schema, tables, &opts).await?;
    if result.cancelled {
        // A cancelled export stops at whatever object it had reached, so the
        // script is a prefix of the real dump. Say so in the file itself: it
        // may well outlive this window, and a truncated backup that looks
        // complete is worse than no backup at all.
        emit_log(&app, "backup-log", "warn", "Cancelled - this dump is incomplete");
        let mut sql = String::with_capacity(result.sql.len() + 128);
        sql.push_str("-- !! INCOMPLETE BACKUP - cancelled before it finished. Do not restore from this file. !!\n");
        sql.push_str(&result.sql);
        sql.push_str("\n-- !! END OF INCOMPLETE BACKUP - objects after this point were never exported. !!\n");
        return Ok(ExportResult { sql, ..result });
    }
    Ok(result)
}

async fn export_one(
    app: &AppHandle,
    state: &State<'_, DbState>,
    schema: Option<String>,
    tables: Option<Vec<String>>,
    opts: &ExportOptions,
) -> Result<ExportResult, String> {
    match require_conn(state)? {
        ActiveConnection::Sqlite(pool) => export_sqlite(app, &pool, tables.as_deref(), opts).await,
        ActiveConnection::Postgres(pool) => export_postgres(app, &pool, schema.as_deref(), tables.as_deref(), opts).await,
        ActiveConnection::Mysql(pool) => export_mysql(app, &pool, schema.as_deref(), tables.as_deref(), opts).await,
        ActiveConnection::D1(cfg) => export_d1(app, &cfg, tables.as_deref(), opts).await,
        ActiveConnection::LibSql(_) => Err("Backup export is not supported for LibSQL/Turso connections".to_string()),
        ActiveConnection::Clickhouse(_) => Err("Backup export is not supported for ClickHouse connections".to_string()),
        ActiveConnection::Redis(_) => Err("Backup is not supported on Redis".to_string()),
        ActiveConnection::Duckdb(h) => export_duckdb(app, &h).await,
        ActiveConnection::Mssql(h) => export_mssql(app, &h).await,
    }
}

#[tauri::command]
pub async fn backup_import(
    app: AppHandle,
    state: State<'_, DbState>,
    sql: String,
) -> Result<ImportResult, String> {
    reset_cancel();
    match require_conn(&state)? {
        ActiveConnection::Sqlite(pool) => import_sqlite(&app, &pool, &sql).await,
        ActiveConnection::Postgres(pool) => import_postgres(&app, &pool, &sql).await,
        ActiveConnection::Mysql(pool) => import_mysql(&app, &pool, &sql).await,
        ActiveConnection::D1(cfg) => import_d1(&app, &cfg, &sql).await,
        ActiveConnection::LibSql(_) => Err("Backup import is not supported for LibSQL/Turso connections".to_string()),
        ActiveConnection::Clickhouse(_) => Err("Backup import is not supported for ClickHouse connections".to_string()),
        ActiveConnection::Redis(_) => Err("Backup is not supported on Redis".to_string()),
        ActiveConnection::Duckdb(h) => import_duckdb(&app, &h, &sql).await,
        ActiveConnection::Mssql(h) => import_mssql(&app, &h, &sql).await,
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

fn backup_header(engine: &str, schema: Option<&str>) -> String {
    let schema_line = schema.map_or_else(String::new, |s| format!("-- Schema   : {s}\n"));
    format!(
        "-- Stroke Backup\n-- Engine   : {engine}\n{schema_line}-- Restore  : execute this file against a {engine} database\n\n"
    )
}

/// Truncate a string to at most `max` characters on a UTF-8 boundary.
/// (Byte-index slicing panics when a boundary lands mid-character.)
fn truncate_chars(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Build and log the result of a finished restore.
///
/// `total` is how many statements the script held; anything neither applied nor
/// failed was never attempted, which only happens when the user cancelled.
fn finish_import(app: &AppHandle, total: usize, ok: usize, errors: Vec<String>) -> ImportResult {
    let cancelled = is_cancelled();
    let failed = errors.len();
    let skipped = total.saturating_sub(ok + failed);
    let msg = if cancelled {
        format!("Restore cancelled: {ok} applied, {failed} failed, {skipped} not run")
    } else {
        format!("Restore complete: {ok} ok, {failed} failed")
    };
    let level = if cancelled || failed > 0 { "warn" } else { "ok" };
    emit_log(app, "restore-log", level, &msg);
    ImportResult { statements_ok: ok, statements_err: failed, statements_skipped: skipped, cancelled, errors }
}

/// True when byte offset `i` sits at the start of a line (only whitespace since
/// the previous newline). Used to recognise line-level directives like `DELIMITER`.
fn at_line_start(chars: &[char], i: usize) -> bool {
    chars[..i].iter().rev().take_while(|c| **c != '\n').all(|c| c.is_whitespace())
}

/// How a dialect escapes a quote inside a single-quoted string literal.
///
/// This has to be explicit: the two rules are mutually incompatible. Under
/// `Standard`, `'a\'` is a complete string whose contents end in a backslash;
/// under `Backslash` the same text is an unterminated string. Guessing wrong in
/// either direction splits a statement in the middle of a literal and corrupts
/// every statement after it in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Escapes {
    /// SQL standard: only a doubled `''` embeds a quote, and a backslash is an
    /// ordinary character. Postgres (with `standard_conforming_strings = on`,
    /// the default since 9.1), SQLite, D1, DuckDB and SQL Server.
    Standard,
    /// MySQL's default (`NO_BACKSLASH_ESCAPES` off): `\'`, `\\` and friends
    /// escape, alongside `''`. This is what our own `mysql_val` emits.
    Backslash,
}

/// Split a SQL script into individual statements.
///
/// Beyond simple `;` splitting this understands the constructs our exporters
/// emit, so bodies containing embedded semicolons survive a round-trip:
///   - single-quoted strings (`'…''…'`, plus `\'` when `esc` is `Backslash`)
///   - double-quoted (`"…"`) and backtick (`` `…` ``) identifiers
///   - PostgreSQL dollar-quoted strings (`$$ … $$`, `$tag$ … $tag$`) - used by
///     `pg_get_functiondef`, trigger defs, and enum `DO $$ … $$` blocks
///   - line comments (`-- …`, stripped) and block comments (`/* … */`,
///     stripped, nesting like Postgres); mysqldump's executable comments
///     (`/*!40000 … */`) are dropped with them, which only skips tuning hints
///   - MySQL `DELIMITER` directives (change the active terminator, e.g. `//`),
///     so routine/trigger bodies aren't split at their internal `;`
fn split_statements(sql: &str, esc: Escapes) -> Vec<String> {
    let chars: Vec<char> = sql.chars().collect();
    let n = chars.len();
    let mut stmts: Vec<String> = Vec::new();
    let mut current = String::with_capacity(512);
    let mut i = 0usize;
    // Active statement terminator; MySQL `DELIMITER` swaps this out.
    let mut delimiter: Vec<char> = vec![';'];

    let flush = |current: &mut String, stmts: &mut Vec<String>| {
        let s = current.trim().to_string();
        if !s.is_empty() { stmts.push(s); }
        current.clear();
    };

    while i < n {
        let ch = chars[i];

        // ── MySQL DELIMITER directive (line-level, not inside a body) ──
        if (ch == 'D' || ch == 'd')
            && at_line_start(&chars, i)
            && i + 9 <= n
            && chars[i..i + 9].iter().collect::<String>().eq_ignore_ascii_case("DELIMITER")
            && chars.get(i + 9).is_some_and(|c| c.is_whitespace())
        {
            i += 9;
            let mut delim: Vec<char> = Vec::new();
            while i < n && chars[i] != '\n' {
                if chars[i].is_whitespace() {
                    if !delim.is_empty() { break; }
                } else {
                    delim.push(chars[i]);
                }
                i += 1;
            }
            while i < n && chars[i] != '\n' { i += 1; } // skip rest of line
            if i < n { i += 1; }                        // consume newline
            if !delim.is_empty() { delimiter = delim; }
            continue;
        }

        // ── Statement terminator (the active delimiter) ──
        if i + delimiter.len() <= n && chars[i..i + delimiter.len()] == delimiter[..] {
            flush(&mut current, &mut stmts);
            i += delimiter.len();
            continue;
        }

        match ch {
            // Single-quoted string literal ('' escapes an embedded quote, and
            // under Backslash rules so does \').
            '\'' => {
                current.push(ch); i += 1;
                while i < n {
                    let c = chars[i];
                    current.push(c);
                    // A backslash consumes whatever follows it, so an escaped
                    // quote can't be mistaken for the end of the literal.
                    if esc == Escapes::Backslash && c == '\\' {
                        if let Some(&next) = chars.get(i + 1) { current.push(next); i += 2; } else { i += 1; }
                        continue;
                    }
                    if c == '\'' {
                        if chars.get(i + 1) == Some(&'\'') { current.push('\''); i += 2; continue; }
                        i += 1; break;
                    }
                    i += 1;
                }
            }
            // Double-quoted identifier ("" escapes an embedded quote).
            '"' => {
                current.push(ch); i += 1;
                while i < n {
                    current.push(chars[i]);
                    if chars[i] == '"' {
                        if chars.get(i + 1) == Some(&'"') { current.push('"'); i += 2; continue; }
                        i += 1; break;
                    }
                    i += 1;
                }
            }
            // Backtick identifier (MySQL).
            '`' => {
                current.push(ch); i += 1;
                while i < n {
                    current.push(chars[i]);
                    if chars[i] == '`' { i += 1; break; }
                    i += 1;
                }
            }
            // Dollar-quoted string ($tag$ … $tag$). Only a real tag opens one;
            // a bare `$` (or `$1` param) is treated as an ordinary character.
            '$' => {
                let mut j = i + 1;
                while j < n && (chars[j].is_alphanumeric() || chars[j] == '_') { j += 1; }
                if j < n && chars[j] == '$' {
                    let tag: Vec<char> = chars[i..=j].to_vec();
                    current.extend(tag.iter());
                    i = j + 1;
                    while i < n {
                        if i + tag.len() <= n && chars[i..i + tag.len()] == tag[..] {
                            current.extend(tag.iter());
                            i += tag.len();
                            break;
                        }
                        current.push(chars[i]);
                        i += 1;
                    }
                } else {
                    current.push(ch); i += 1;
                }
            }
            // Line comment - stripped (replaced by a newline separator).
            '-' if chars.get(i + 1) == Some(&'-') => {
                while i < n && chars[i] != '\n' { i += 1; }
                current.push('\n');
            }
            // Block comment - stripped. Its contents are not SQL, so a `;` in
            // there must not end the statement. Postgres nests these; the depth
            // counter is harmless for the dialects that don't.
            '/' if chars.get(i + 1) == Some(&'*') => {
                let mut depth = 1usize;
                i += 2;
                while i < n && depth > 0 {
                    if chars[i] == '/' && chars.get(i + 1) == Some(&'*') { depth += 1; i += 2; }
                    else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') { depth -= 1; i += 2; }
                    else { i += 1; }
                }
                current.push(' ');
            }
            _ => { current.push(ch); i += 1; }
        }
    }
    flush(&mut current, &mut stmts);
    stmts
}

// ── SQLite export ─────────────────────────────────────────────────────────────

async fn export_sqlite(
    app: &AppHandle,
    pool: &sqlx::SqlitePool,
    filter: Option<&[String]>,
    opts: &ExportOptions,
) -> Result<ExportResult, String> {
    emit_log(app, "backup-log", "info", "Starting SQLite export…");
    let mut out = backup_header("SQLite", None);
    out.push_str("PRAGMA foreign_keys=OFF;\nBEGIN TRANSACTION;\n\n");

    let ddl_rows = sqlx::query(
        "SELECT type, name, tbl_name, sql FROM sqlite_master \
         WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' \
         ORDER BY CASE type WHEN 'table' THEN 0 WHEN 'index' THEN 1 WHEN 'view' THEN 2 ELSE 3 END, name"
    )
    .fetch_all(pool).await.map_err(|e| e.to_string())?;

    // A trigger/index belongs to `tbl_name`; only emit it when that table is in
    // the selection, or the restore would reference a table that was never created.
    let in_filter = |t: &str| filter.map_or(true, |f| f.iter().any(|x| x == t));

    let mut all_tables: Vec<String> = Vec::new();
    for row in &ddl_rows {
        let obj_type: String = row.try_get(0).unwrap_or_default();
        let name: String = row.try_get(1).unwrap_or_default();
        let tbl_name: String = row.try_get(2).unwrap_or_default();
        let sql: String = row.try_get(3).unwrap_or_default();

        match obj_type.as_str() {
            "table" => {
                all_tables.push(name.clone());
                if opts.include_schema && in_filter(&name) {
                    out.push_str(sql.trim()); out.push_str(";\n");
                }
            }
            "view" => {
                if opts.include_views {
                    out.push_str(sql.trim()); out.push_str(";\n");
                }
            }
            "trigger" => {
                if opts.include_triggers && in_filter(&tbl_name) {
                    out.push_str(sql.trim()); out.push_str(";\n");
                }
            }
            // index (and any other table-bound object): gated by schema + filter
            _ => {
                if opts.include_schema && in_filter(&tbl_name) {
                    out.push_str(sql.trim()); out.push_str(";\n");
                }
            }
        }
    }

    let tables_to_dump: Vec<&String> = all_tables.iter()
        .filter(|t| filter.map_or(true, |f| f.iter().any(|ft| ft == *t)))
        .collect();

    if opts.include_data {
        emit_log(app, "backup-log", "info", format!("Exporting {} table(s)…", tables_to_dump.len()));
        let mut total_rows = 0usize;
        for table in &tables_to_dump {
            if is_cancelled() { break; }
            emit_log(app, "backup-log", "info", format!("  → {table}"));
            let table_esc = table.replace('"', "\"\"");

            // Column names first, so the value query can ask SQLite to render
            // each one as a SQL literal.
            let col_names: Vec<String> = sqlx::query_scalar::<_, String>(
                &format!("SELECT name FROM pragma_table_info('{}')", table.replace('\'', "''"))
            ).fetch_all(pool).await.map_err(|e| e.to_string())?;
            if col_names.is_empty() { continue; }

            // `quote()` is SQLite's own value-to-literal function: it returns
            // X'…' for a blob, an escaped 'string' for text, a bare number, or
            // the four characters NULL. Letting the engine do this removes the
            // guesswork a Rust-side decoder would need - and with it the chance
            // of a value it doesn't recognise quietly becoming NULL.
            let select_list = col_names.iter()
                .map(|c| format!("quote(\"{}\")", c.replace('"', "\"\"")))
                .collect::<Vec<_>>().join(", ");
            let col_list = col_names.iter()
                .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
                .collect::<Vec<_>>().join(", ");
            let q = format!("SELECT {select_list} FROM \"{table_esc}\"");

            // Stream rows one at a time - avoids loading the entire table into memory.
            let mut stream = sqlx::query(&q).fetch(pool);
            let mut n = 0usize;
            while let Some(row) = stream.try_next().await.map_err(|e| e.to_string())? {
                if n == 0 { out.push('\n'); }
                n += 1;
                let mut vals: Vec<String> = Vec::with_capacity(col_names.len());
                for (i, name) in col_names.iter().enumerate() {
                    let lit: Option<String> = row.try_get(i).map_err(|e| {
                        format!("Could not read {table}.\"{name}\" at row {n}: {e}")
                    })?;
                    vals.push(lit.unwrap_or_else(|| "NULL".to_string()));
                }
                out.push_str(&format!(
                    "INSERT OR REPLACE INTO \"{table_esc}\" ({col_list}) VALUES ({});\n",
                    vals.join(", ")
                ));
            }
            total_rows += n;
            emit_log(app, "backup-log", "ok", format!("  ✓ {table} - {n} rows"));
        }
        out.push_str("\nCOMMIT;\nPRAGMA foreign_keys=ON;\n");
        emit_log(app, "backup-log", "ok", format!("Export complete: {} tables, {} rows", tables_to_dump.len(), total_rows));
        Ok(ExportResult { sql: out, table_count: tables_to_dump.len(), row_count: total_rows, cancelled: is_cancelled() })
    } else {
        out.push_str("\nCOMMIT;\nPRAGMA foreign_keys=ON;\n");
        emit_log(app, "backup-log", "ok", format!("Export complete: {} tables (schema only)", tables_to_dump.len()));
        Ok(ExportResult { sql: out, table_count: tables_to_dump.len(), row_count: 0, cancelled: is_cancelled() })
    }
}

// ── SQLite import ─────────────────────────────────────────────────────────────

async fn import_sqlite(app: &AppHandle, pool: &sqlx::SqlitePool, sql: &str) -> Result<ImportResult, String> {
    let stmts = split_statements(sql, Escapes::Standard);
    let total = stmts.len();
    emit_log(app, "restore-log", "info", format!("Starting restore: {} statements…", total));
    let mut ok = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for stmt in &stmts {
        if is_cancelled() { break; }
        let low = stmt.trim_start().to_lowercase();
        if low.starts_with("pragma foreign_keys") { ok += 1; continue; }
        match sqlx::query(stmt).execute(pool).await {
            Ok(_) => { ok += 1; }
            Err(e) => errors.push(format!("{e} - near: {}…", truncate_chars(stmt, 60))),
        }
    }

    Ok(finish_import(app, total, ok, errors))
}

// ── PostgreSQL export ─────────────────────────────────────────────────────────

/// Unwrap a secondary-object catalog query for the Postgres export. On failure
/// (permission error, catalog incompatibility) the export continues without
/// those objects, but says so - both in the log and as a WARNING line in the
/// dump - instead of silently producing an incomplete backup.
fn catalog_or_warn<T>(
    res: Result<Vec<T>, sqlx::Error>,
    app: &AppHandle,
    out: &mut String,
    kind: &str,
    schema: &str,
) -> Vec<T> {
    match res {
        Ok(v) => v,
        Err(e) => {
            emit_log(app, "backup-log", "warn", format!("  ! could not export {kind} from {schema}: {e}"));
            out.push_str(&format!("-- WARNING: could not export {kind} from {schema}: {e}\n\n"));
            Vec::new()
        }
    }
}

async fn export_postgres(
    app: &AppHandle,
    pool: &sqlx::PgPool,
    schema_filter: Option<&str>,
    table_filter: Option<&[String]>,
    opts: &ExportOptions,
) -> Result<ExportResult, String> {
    let schemas: Vec<String> = if let Some(s) = schema_filter {
        vec![s.to_string()]
    } else {
        sqlx::query_scalar(
            "SELECT nspname::text FROM pg_catalog.pg_namespace \
             WHERE nspname NOT IN ('pg_catalog','information_schema','pg_toast') \
               AND nspname NOT LIKE 'pg_temp_%' \
             ORDER BY nspname"
        )
        .fetch_all(pool).await.map_err(|e| e.to_string())?
    };

    emit_log(app, "backup-log", "info", format!("Starting PostgreSQL export ({} schema(s))…", schemas.len()));

    let mut out = backup_header("PostgreSQL", schema_filter);
    out.push_str("SET client_encoding = 'UTF8';\n");
    out.push_str("SET standard_conforming_strings = on;\n");
    out.push_str("SET session_replication_role = replica;\n\n");

    let mut total_tables = 0usize;
    let mut total_rows = 0usize;

    for schema in &schemas {
        out.push_str(&format!("-- ── Schema: {schema} ──────────────────────────────────────\n\n"));

        // ── Enums ──
        if opts.include_enums {
            let enums_res = sqlx::query_as::<_, (String, String)>(
                "SELECT t.typname::text, \
                        string_agg(e.enumlabel::text, ',' ORDER BY e.enumsortorder) \
                 FROM pg_catalog.pg_type t \
                 JOIN pg_catalog.pg_enum e ON e.enumtypid = t.oid \
                 JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace \
                 WHERE n.nspname = $1 \
                 GROUP BY t.typname ORDER BY t.typname"
            )
            .bind(schema)
            .fetch_all(pool).await;
            let enums = catalog_or_warn(enums_res, app, &mut out, "enum types", schema);

            if !enums.is_empty() {
                emit_log(app, "backup-log", "info", format!("  Exporting {} enum(s) from {schema}…", enums.len()));
                out.push_str(&format!("-- Custom enum types - {schema}\n"));
                for (name, labels) in &enums {
                    let quoted: Vec<String> = labels.split(',').map(|l| format!("'{}'", l.replace('\'', "''"))).collect();
                    out.push_str(&format!(
                        "DO $$ BEGIN\n  CREATE TYPE \"{schema}\".\"{name}\" AS ENUM ({});\nEXCEPTION WHEN duplicate_object THEN NULL;\nEND $$;\n",
                        quoted.join(", ")
                    ));
                }
                out.push('\n');
            }
        }

        // ── Sequences ──
        if opts.include_sequences {
            let seqs_res = sqlx::query_as::<_, (String, String, i64, i64, i64, i64, bool)>(
                "SELECT sequence_name::text, data_type::text, \
                        start_value::bigint, minimum_value::bigint, \
                        maximum_value::bigint, increment::bigint, \
                        (cycle_option = 'YES') \
                 FROM information_schema.sequences \
                 WHERE sequence_schema = $1 \
                 ORDER BY sequence_name"
            )
            .bind(schema)
            .fetch_all(pool).await;
            let seqs = catalog_or_warn(seqs_res, app, &mut out, "sequences", schema);

            if !seqs.is_empty() {
                emit_log(app, "backup-log", "info", format!("  Exporting {} sequence(s) from {schema}…", seqs.len()));
                out.push_str(&format!("-- Sequences - {schema}\n"));
                for (name, dtype, start, min, max, inc, cycle) in &seqs {
                    let cycle_str = if *cycle { "CYCLE" } else { "NO CYCLE" };
                    out.push_str(&format!(
                        "CREATE SEQUENCE IF NOT EXISTS \"{schema}\".\"{name}\"\n    AS {dtype}\n    START WITH {start}\n    INCREMENT BY {inc}\n    MINVALUE {min}\n    MAXVALUE {max}\n    {cycle_str};\n"
                    ));
                }
                out.push('\n');
            }
        }

        // ── Schema / Tables DDL ──
        if opts.include_schema {
            let all_tables: Vec<String> = sqlx::query_scalar(
                "SELECT c.relname::text FROM pg_catalog.pg_class c \
                 JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND c.relkind = 'r' AND NOT c.relispartition \
                 ORDER BY c.relname"
            )
            .bind(schema).fetch_all(pool).await.map_err(|e| e.to_string())?;

            let tables_to_export: Vec<&String> = all_tables.iter()
                .filter(|t| table_filter.map_or(true, |f| f.iter().any(|ft| ft == *t)))
                .collect();

            if !tables_to_export.is_empty() {
                emit_log(app, "backup-log", "info", format!("  Schema {schema}: {} table(s)", tables_to_export.len()));

                for table in &tables_to_export {
                    if is_cancelled() { break; }
                    emit_log(app, "backup-log", "info", format!("  → {schema}.{table}"));
                    match pg_dump_table(pool, schema, table, opts.include_data).await {
                        Ok((ddl, rows)) => {
                            out.push_str(&ddl);
                            total_rows += rows;
                            total_tables += 1;
                            emit_log(app, "backup-log", "ok", format!("  ✓ {table} - {rows} rows"));
                        }
                        Err(e) => {
                            out.push_str(&format!("-- ERROR exporting {schema}.{table}: {e}\n\n"));
                            emit_log(app, "backup-log", "error", format!("  ✗ {table}: {e}"));
                        }
                    }
                }

                // Foreign keys
                let fk_res = sqlx::query_scalar::<_, String>(
                    "SELECT format('ALTER TABLE %I.%I ADD CONSTRAINT %I %s',\
                            n.nspname, t.relname, c.conname, pg_get_constraintdef(c.oid))\
                     FROM pg_catalog.pg_constraint c\
                     JOIN pg_catalog.pg_class t ON t.oid = c.conrelid\
                     JOIN pg_catalog.pg_namespace n ON n.oid = t.relnamespace\
                     WHERE c.contype = 'f' AND n.nspname = $1\
                     ORDER BY t.relname, c.conname"
                )
                .bind(schema).fetch_all(pool).await;
                let fk_defs = catalog_or_warn(fk_res, app, &mut out, "foreign keys", schema);

                if !fk_defs.is_empty() {
                    out.push_str(&format!("-- Foreign keys - {schema}\n"));
                    for fk in &fk_defs { out.push_str(fk); out.push_str(";\n"); }
                    out.push('\n');
                }
            }
        }

        // ── Views ──
        if opts.include_views {
            let views_res = sqlx::query_as::<_, (String, String)>(
                "SELECT c.relname::text, pg_get_viewdef(c.oid, true) \
                 FROM pg_catalog.pg_class c \
                 JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND c.relkind = 'v' \
                 ORDER BY c.relname"
            )
            .bind(schema).fetch_all(pool).await;
            let views = catalog_or_warn(views_res, app, &mut out, "views", schema);

            if !views.is_empty() {
                emit_log(app, "backup-log", "info", format!("  Exporting {} view(s) from {schema}…", views.len()));
                out.push_str(&format!("-- Views - {schema}\n"));
                for (name, def) in &views {
                    out.push_str(&format!(
                        "CREATE OR REPLACE VIEW \"{schema}\".\"{name}\" AS\n{def};\n"
                    ));
                }
                out.push('\n');
            }
        }

        // ── Functions & Procedures ──
        if opts.include_functions {
            let funcs_res = sqlx::query_as::<_, (String, String)>(
                "SELECT p.proname::text, pg_get_functiondef(p.oid) \
                 FROM pg_catalog.pg_proc p \
                 JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace \
                 WHERE n.nspname = $1 AND p.prokind IN ('f', 'p') \
                 ORDER BY p.proname"
            )
            .bind(schema).fetch_all(pool).await;
            let funcs = catalog_or_warn(funcs_res, app, &mut out, "functions/procedures", schema);

            if !funcs.is_empty() {
                emit_log(app, "backup-log", "info", format!("  Exporting {} function(s)/procedure(s) from {schema}…", funcs.len()));
                out.push_str(&format!("-- Functions & Procedures - {schema}\n"));
                for (_, def) in &funcs {
                    // pg_get_functiondef already includes CREATE OR REPLACE FUNCTION
                    out.push_str(def.trim());
                    if !def.trim().ends_with(';') { out.push(';'); }
                    out.push_str("\n\n");
                }
            }
        }

        // ── Triggers ──
        if opts.include_triggers {
            let triggers_res = sqlx::query_scalar::<_, String>(
                "SELECT pg_get_triggerdef(t.oid) \
                 FROM pg_catalog.pg_trigger t \
                 JOIN pg_catalog.pg_class c ON c.oid = t.tgrelid \
                 JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND NOT t.tgisinternal \
                 ORDER BY c.relname, t.tgname"
            )
            .bind(schema).fetch_all(pool).await;
            let triggers = catalog_or_warn(triggers_res, app, &mut out, "triggers", schema);

            if !triggers.is_empty() {
                emit_log(app, "backup-log", "info", format!("  Exporting {} trigger(s) from {schema}…", triggers.len()));
                out.push_str(&format!("-- Triggers - {schema}\n"));
                for trig in &triggers {
                    out.push_str(trig.trim());
                    out.push_str(";\n");
                }
                out.push('\n');
            }
        }
    }

    out.push_str("SET session_replication_role = DEFAULT;\n");
    emit_log(app, "backup-log", "ok", format!("Export complete: {total_tables} tables, {total_rows} rows"));
    Ok(ExportResult { sql: out, table_count: total_tables, row_count: total_rows, cancelled: is_cancelled() })
}

async fn pg_dump_table(
    pool: &sqlx::PgPool,
    schema: &str,
    table: &str,
    include_data: bool,
) -> Result<(String, usize), String> {
    let mut out = String::new();

    let col_rows = sqlx::query(r#"
        SELECT
            a.attname::text,
            format_type(a.atttypid, a.atttypmod),
            NOT a.attnotnull AS nullable,
            pg_get_expr(d.adbin, d.adrelid) AS col_default,
            a.attidentity IN ('a','d') AS is_identity,
            a.attgenerated <> '' AS is_generated,
            format_type(a.atttypid, NULL) AS base_type
        FROM pg_catalog.pg_attribute a
        LEFT JOIN pg_catalog.pg_attrdef d
            ON d.adrelid = a.attrelid AND d.adnum = a.attnum
        JOIN pg_catalog.pg_class c ON c.oid = a.attrelid
        JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = $1 AND c.relname = $2
          AND a.attnum > 0 AND NOT a.attisdropped
        ORDER BY a.attnum
    "#)
    .bind(schema).bind(table).fetch_all(pool).await.map_err(|e| e.to_string())?;

    if col_rows.is_empty() { return Ok((String::new(), 0)); }

    let pk_cols: Vec<String> = sqlx::query_scalar(
        "SELECT a.attname::text \
         FROM pg_catalog.pg_constraint c \
         JOIN pg_catalog.pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = ANY(c.conkey) \
         JOIN pg_catalog.pg_class t ON t.oid = c.conrelid \
         JOIN pg_catalog.pg_namespace n ON n.oid = t.relnamespace \
         WHERE c.contype = 'p' AND n.nspname = $1 AND t.relname = $2 \
         ORDER BY array_position(c.conkey, a.attnum)"
    )
    .bind(schema).bind(table).fetch_all(pool).await.unwrap_or_default();

    out.push_str(&format!("CREATE TABLE IF NOT EXISTS \"{schema}\".\"{table}\" (\n"));
    let mut col_defs: Vec<String> = Vec::new();

    for row in &col_rows {
        let name: String = row.try_get(0).unwrap_or_default();
        let typ: String = row.try_get(1).unwrap_or_default();
        let nullable: bool = row.try_get(2).unwrap_or(true);
        let default: Option<String> = row.try_get(3).ok().flatten();
        let is_identity: bool = row.try_get(4).unwrap_or(false);
        let is_generated: bool = row.try_get(5).unwrap_or(false);

        let mut def = format!("    \"{}\" {}", name, typ);
        if is_generated {
            // A generated column computes itself. Emitting its expression as a
            // DEFAULT instead would make the column writable and leave the
            // restored table holding whatever the dump inserted.
            if let Some(d) = &default {
                def.push_str(&format!(" GENERATED ALWAYS AS ({d}) STORED"));
            }
            if !nullable { def.push_str(" NOT NULL"); }
        } else {
            if !nullable { def.push_str(" NOT NULL"); }
            if is_identity {
                def.push_str(" GENERATED BY DEFAULT AS IDENTITY");
            } else if let Some(d) = &default {
                def.push_str(&format!(" DEFAULT {}", d));
            }
        }
        col_defs.push(def);
    }

    if !pk_cols.is_empty() {
        let pk_quoted = pk_cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
        col_defs.push(format!("    PRIMARY KEY ({})", pk_quoted));
    }

    out.push_str(&col_defs.join(",\n"));
    out.push_str("\n);\n");

    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT pg_get_indexdef(i.oid) \
         FROM pg_catalog.pg_index ix \
         JOIN pg_catalog.pg_class i ON i.oid = ix.indexrelid \
         JOIN pg_catalog.pg_class t ON t.oid = ix.indrelid \
         JOIN pg_catalog.pg_namespace n ON n.oid = t.relnamespace \
         WHERE n.nspname = $1 AND t.relname = $2 AND NOT ix.indisprimary"
    )
    .bind(schema).bind(table).fetch_all(pool).await.unwrap_or_default();

    for idx_def in &indexes {
        let safe = idx_def
            .replacen("CREATE INDEX ", "CREATE INDEX IF NOT EXISTS ", 1)
            .replacen("CREATE UNIQUE INDEX ", "CREATE UNIQUE INDEX IF NOT EXISTS ", 1);
        out.push_str(&safe);
        out.push_str(";\n");
    }

    if !include_data {
        out.push('\n');
        return Ok((out, 0));
    }

    // Columns to write rows for. A generated column rejects an explicit value,
    // so listing it would make every INSERT in the dump fail.
    let data_cols: Vec<(String, String)> = col_rows
        .iter()
        .filter(|r| !r.try_get::<bool, _>(5).unwrap_or(false))
        .map(|r| {
            (
                r.try_get::<String, _>(0).unwrap_or_default(),
                r.try_get::<String, _>(6).unwrap_or_default(),
            )
        })
        .collect();

    if data_cols.is_empty() {
        out.push('\n');
        return Ok((out, 0));
    }

    // Render every value with a server-side `::text` cast rather than decoding
    // each Postgres type in Rust. sqlx speaks the binary protocol, so the old
    // per-type decoder silently fell through to NULL for everything it had no
    // arm for - uuid, timestamptz, date, interval, arrays, enums, inet - and
    // the dump then carried NULL where real data had been. That is the worst
    // way for a backup to fail: it looks like it worked, and the restore dies
    // on the first NOT NULL column. A cast has an arm for every type there is,
    // including ones a user defined after this code was written.
    let select_list = data_cols
        .iter()
        .map(|(name, base)| {
            if base.eq_ignore_ascii_case("money") {
                // `money::text` is locale-formatted ("$1,234.56"), which only
                // reads back under the same lc_monetary. numeric is portable.
                format!("(\"{name}\"::numeric)::text")
            } else {
                format!("\"{name}\"::text")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let col_list = data_cols
        .iter()
        .map(|(name, _)| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");

    let data_sql = format!("SELECT {select_list} FROM \"{schema}\".\"{table}\"");

    // One connection for the whole table so the float setting below applies to
    // the query that follows it. `extra_float_digits = 3` asks for the shortest
    // representation that reads back bit-identical (the default on PostgreSQL
    // 12+, but not before it).
    let mut conn = pool.acquire().await.map_err(|e| e.to_string())?;
    sqlx::query("SET extra_float_digits = 3").execute(&mut *conn).await.ok();

    // Stream rows one at a time to avoid loading an entire table into memory.
    let mut stream = sqlx::query(&data_sql).fetch(&mut *conn);
    let mut row_count = 0usize;
    let conflict_target = if pk_cols.is_empty() {
        "DO NOTHING".to_string()
    } else {
        let pk_q = pk_cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
        format!("({pk_q}) DO NOTHING")
    };

    while let Some(row) = stream.try_next().await.map_err(|e| e.to_string())? {
        if row_count == 0 { out.push('\n'); }
        row_count += 1;

        let mut vals: Vec<String> = Vec::with_capacity(data_cols.len());
        for (i, (name, _)) in data_cols.iter().enumerate() {
            // Every column arrives as text or NULL. A decode error here is a
            // real failure, not a NULL: report it instead of writing a hole
            // into the backup.
            let cell: Option<String> = row.try_get(i).map_err(|e| {
                format!("Could not read {schema}.{table}.\"{name}\" at row {row_count}: {e}")
            })?;
            vals.push(match cell {
                None => "NULL".to_string(),
                Some(text) => format!("'{}'", text.replace('\'', "''")),
            });
        }

        out.push_str(&format!(
            "INSERT INTO \"{schema}\".\"{table}\" ({col_list}) VALUES ({}) ON CONFLICT {conflict_target};\n",
            vals.join(", ")
        ));
    }
    out.push('\n');

    Ok((out, row_count))
}

// ── PostgreSQL import ─────────────────────────────────────────────────────────

async fn import_postgres(app: &AppHandle, pool: &sqlx::PgPool, sql: &str) -> Result<ImportResult, String> {
    let stmts = split_statements(sql, Escapes::Standard);
    let total = stmts.len();
    emit_log(app, "restore-log", "info", format!("Starting restore: {} statements…", total));

    let mut ok = 0usize;
    let mut errors: Vec<String> = Vec::new();

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("SET session_replication_role = replica").execute(&mut *tx).await.ok();

    for (i, stmt) in stmts.iter().enumerate() {
        if is_cancelled() { break; }
        let low = stmt.trim_start().to_lowercase();
        if low.starts_with("set session_replication_role") { ok += 1; continue; }
        if low.starts_with("set client_encoding") || low.starts_with("set standard_conforming") { ok += 1; continue; }

        // Wrap each statement in a savepoint. In Postgres the first error aborts
        // the whole transaction, so without this a single bad statement would
        // make every later one fail and the final COMMIT roll everything back.
        // Rolling back to the savepoint on error keeps prior successes and lets
        // the restore continue (partial-apply, matching the SQLite/MySQL paths).
        sqlx::query("SAVEPOINT stroke_sp").execute(&mut *tx).await.ok();
        match sqlx::query(stmt).execute(&mut *tx).await {
            Ok(_) => {
                sqlx::query("RELEASE SAVEPOINT stroke_sp").execute(&mut *tx).await.ok();
                ok += 1;
                // Emit progress every 50 statements
                if (i + 1) % 50 == 0 {
                    emit_log(app, "restore-log", "info", format!("  {}/{} statements…", i + 1, total));
                }
            }
            Err(e) => {
                sqlx::query("ROLLBACK TO SAVEPOINT stroke_sp").execute(&mut *tx).await.ok();
                errors.push(format!("{e} - near: {}…", truncate_chars(stmt, 80)));
            }
        }
    }

    sqlx::query("SET session_replication_role = DEFAULT").execute(&mut *tx).await.ok();
    if is_cancelled() {
        // Everything so far is inside this transaction, so cancelling can still
        // undo it in full. Committing here would leave the database holding an
        // arbitrary prefix of the dump - the one outcome a cancel must not have.
        tx.rollback().await.map_err(|e| e.to_string())?;
        emit_log(app, "restore-log", "warn", "Cancelled - rolled back, the database is unchanged");
        return Ok(finish_import(app, total, 0, errors));
    }
    tx.commit().await.map_err(|e| e.to_string())?;

    Ok(finish_import(app, total, ok, errors))
}

// ── MySQL export ──────────────────────────────────────────────────────────────

async fn export_mysql(
    app: &AppHandle,
    pool: &sqlx::MySqlPool,
    schema_filter: Option<&str>,
    table_filter: Option<&[String]>,
    opts: &ExportOptions,
) -> Result<ExportResult, String> {
    let schemas: Vec<String> = if let Some(s) = schema_filter {
        vec![s.to_string()]
    } else {
        sqlx::query_scalar::<_, String>(
            "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA \
             WHERE SCHEMA_NAME NOT IN ('information_schema','performance_schema','mysql','sys') \
             ORDER BY SCHEMA_NAME"
        )
        .fetch_all(pool).await.map_err(|e| e.to_string())?
    };

    emit_log(app, "backup-log", "info", format!("Starting MySQL export ({} schema(s))…", schemas.len()));

    let mut out = backup_header("MySQL", schema_filter);
    out.push_str("SET FOREIGN_KEY_CHECKS=0;\nSET UNIQUE_CHECKS=0;\nSET AUTOCOMMIT=0;\n\n");

    let mut total_tables = 0usize;
    let mut total_rows = 0usize;

    for schema in &schemas {
        out.push_str(&format!("USE `{schema}`;\n\n"));

        // ── Tables ──
        if opts.include_schema {
            let all_tables: Vec<String> = sqlx::query_scalar::<_, String>(
                "SELECT TABLE_NAME FROM information_schema.TABLES \
                 WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'BASE TABLE' \
                 ORDER BY TABLE_NAME"
            )
            .bind(schema).fetch_all(pool).await.map_err(|e| e.to_string())?;

            let tables_to_export: Vec<&String> = all_tables.iter()
                .filter(|t| table_filter.map_or(true, |f| f.iter().any(|ft| ft == *t)))
                .collect();

            if !tables_to_export.is_empty() {
                emit_log(app, "backup-log", "info", format!("Schema {schema}: {} table(s)", tables_to_export.len()));

                for table in &tables_to_export {
                    if is_cancelled() { break; }
                    emit_log(app, "backup-log", "info", format!("  → {schema}.{table}"));
                    let create_row = sqlx::query(&format!("SHOW CREATE TABLE `{schema}`.`{table}`"))
                        .fetch_one(pool).await
                        .map_err(|e| format!("SHOW CREATE TABLE `{table}` failed: {e}"))?;
                    let create_sql = super::mysql::my_text(&create_row, 1).unwrap_or_default();
                    out.push_str(&create_sql.replace("CREATE TABLE ", "CREATE TABLE IF NOT EXISTS "));
                    out.push_str(";\n\n");

                    if opts.include_data {
                        // Stream rows to avoid loading an entire table into memory.
                        let data_q = format!("SELECT * FROM `{schema}`.`{table}`");
                        let mut stream = sqlx::query(&data_q).fetch(pool);
                        let mut n = 0usize;
                        let mut col_list = String::new();
                        while let Some(row) = stream.try_next().await.map_err(|e| e.to_string())? {
                            if n == 0 {
                                col_list = row.columns().iter().map(|c| format!("`{}`", c.name())).collect::<Vec<_>>().join(", ");
                            }
                            n += 1;
                            let vals: Vec<String> = (0..row.len()).map(|i| mysql_val(&row, i)).collect();
                            out.push_str(&format!("INSERT IGNORE INTO `{schema}`.`{table}` ({col_list}) VALUES ({});\n", vals.join(", ")));
                        }
                        if n > 0 { out.push('\n'); }
                        total_rows += n;
                        emit_log(app, "backup-log", "ok", format!("  ✓ {table} - {n} rows"));
                    } else {
                        emit_log(app, "backup-log", "ok", format!("  ✓ {table} (schema only)"));
                    }
                    total_tables += 1;
                }
            }
        }

        // ── Views ──
        if opts.include_views {
            let view_names: Vec<String> = sqlx::query_scalar::<_, String>(
                "SELECT TABLE_NAME FROM information_schema.VIEWS WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME"
            )
            .bind(schema).fetch_all(pool).await.unwrap_or_default();

            if !view_names.is_empty() {
                emit_log(app, "backup-log", "info", format!("  Exporting {} view(s) from {schema}…", view_names.len()));
                out.push_str(&format!("-- Views - {schema}\n"));
                for view in &view_names {
                    if let Ok(row) = sqlx::query(&format!("SHOW CREATE VIEW `{schema}`.`{view}`")).fetch_one(pool).await {
                        let create = super::mysql::my_text(&row, 1).unwrap_or_default();
                        out.push_str(&create.replace("CREATE ", "CREATE OR REPLACE "));
                        out.push_str(";\n");
                    }
                }
                out.push('\n');
            }
        }

        // ── Functions & Procedures ──
        if opts.include_functions {
            let routines: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
                "SELECT ROUTINE_NAME, ROUTINE_TYPE FROM information_schema.ROUTINES \
                 WHERE ROUTINE_SCHEMA = ? \
                 ORDER BY ROUTINE_TYPE, ROUTINE_NAME"
            )
            .bind(schema).fetch_all(pool).await.unwrap_or_default();

            if !routines.is_empty() {
                emit_log(app, "backup-log", "info", format!("  Exporting {} function(s)/procedure(s) from {schema}…", routines.len()));
                out.push_str(&format!("-- Functions & Procedures - {schema}\nDELIMITER //\n"));
                for (name, rtype) in &routines {
                    let keyword = if rtype == "FUNCTION" { "FUNCTION" } else { "PROCEDURE" };
                    if let Ok(row) = sqlx::query(&format!("SHOW CREATE {keyword} `{schema}`.`{name}`")).fetch_one(pool).await {
                        let col_idx: usize = if rtype == "FUNCTION" { 2 } else { 2 };
                        let create = super::mysql::my_text(&row, col_idx).unwrap_or_default();
                        out.push_str(&create);
                        out.push_str("//\n\n");
                    }
                }
                out.push_str("DELIMITER ;\n\n");
            }
        }

        // ── Triggers ──
        if opts.include_triggers {
            let trigger_names: Vec<String> = sqlx::query_scalar::<_, String>(
                "SELECT TRIGGER_NAME FROM information_schema.TRIGGERS \
                 WHERE TRIGGER_SCHEMA = ? ORDER BY TRIGGER_NAME"
            )
            .bind(schema).fetch_all(pool).await.unwrap_or_default();

            if !trigger_names.is_empty() {
                emit_log(app, "backup-log", "info", format!("  Exporting {} trigger(s) from {schema}…", trigger_names.len()));
                out.push_str(&format!("-- Triggers - {schema}\nDELIMITER //\n"));
                for trig in &trigger_names {
                    if let Ok(row) = sqlx::query(&format!("SHOW CREATE TRIGGER `{schema}`.`{trig}`")).fetch_one(pool).await {
                        let create = super::mysql::my_text(&row, 2).unwrap_or_default();
                        out.push_str(&create);
                        out.push_str("//\n\n");
                    }
                }
                out.push_str("DELIMITER ;\n\n");
            }
        }
    }

    out.push_str("\nSET FOREIGN_KEY_CHECKS=1;\nSET UNIQUE_CHECKS=1;\nCOMMIT;\n");
    emit_log(app, "backup-log", "ok", format!("Export complete: {total_tables} tables, {total_rows} rows"));
    Ok(ExportResult { sql: out, table_count: total_tables, row_count: total_rows, cancelled: is_cancelled() })
}

fn mysql_val(row: &sqlx::mysql::MySqlRow, idx: usize) -> String {
    // DECIMAL/NEWDECIMAL must decode as an exact Decimal *first*: the generic
    // integer arm below would otherwise consume the value and keep only its
    // integer part (9.99 → 9), silently corrupting the dump (same warning as
    // mysql.rs cell_to_json). DATETIME/DATE/TIME need their own arms too -
    // they match none of the numeric/bytes decoders and used to export as NULL.
    let type_name = row.column(idx).type_info().name();
    if type_name.eq_ignore_ascii_case("DECIMAL") || type_name.eq_ignore_ascii_case("NEWDECIMAL") {
        if let Ok(v) = row.try_get::<Option<rust_decimal::Decimal>, _>(idx) {
            return v.map_or_else(|| "NULL".into(), |d| d.to_string());
        }
    }
    if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
        return v.map_or_else(|| "NULL".into(), |n| n.to_string());
    }
    // BIGINT UNSIGNED - the signed arm rejects the UNSIGNED flag.
    if let Ok(v) = row.try_get::<Option<u64>, _>(idx) {
        return v.map_or_else(|| "NULL".into(), |n| n.to_string());
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(idx) {
        // MySQL rejects NaN/Infinity literals on insert, so store them as NULL.
        return v.map_or_else(|| "NULL".into(), |n| if n.is_finite() { n.to_string() } else { "NULL".into() });
    }
    if let Ok(v) = row.try_get::<Option<bool>, _>(idx) {
        return v.map_or_else(|| "NULL".into(), |b| if b { "1" } else { "0" }.into());
    }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveDateTime>, _>(idx) {
        return v.map_or_else(|| "NULL".into(), |d| format!("'{d}'"));
    }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveDate>, _>(idx) {
        return v.map_or_else(|| "NULL".into(), |d| format!("'{d}'"));
    }
    if let Ok(v) = row.try_get::<Option<chrono::NaiveTime>, _>(idx) {
        return v.map_or_else(|| "NULL".into(), |t| format!("'{t}'"));
    }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(idx) {
        return v.map_or_else(|| "NULL".into(), |b| {
            if let Ok(s) = std::str::from_utf8(&b) {
                format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
            } else {
                format!("0x{}", hex::encode(b))
            }
        });
    }
    "NULL".into()
}

// ── MySQL import ──────────────────────────────────────────────────────────────

async fn import_mysql(app: &AppHandle, pool: &sqlx::MySqlPool, sql: &str) -> Result<ImportResult, String> {
    let stmts = split_statements(sql, Escapes::Backslash);
    let total = stmts.len();
    emit_log(app, "restore-log", "info", format!("Starting restore: {} statements…", total));
    let mut ok = 0usize;
    let mut errors: Vec<String> = Vec::new();

    // One connection for the whole restore: the dump carries per-schema `USE …;`
    // directives, which are connection-scoped - on a pool, the USE and the
    // unqualified statements that depend on it could land on different
    // connections, restoring tables into the wrong database.
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| format!("Failed to acquire connection: {e}"))?;

    for (i, stmt) in stmts.iter().enumerate() {
        if is_cancelled() { break; }
        match sqlx::query(stmt).execute(&mut *conn).await {
            Ok(_) => {
                ok += 1;
                if (i + 1) % 50 == 0 {
                    emit_log(app, "restore-log", "info", format!("  {}/{} statements…", i + 1, total));
                }
            }
            Err(e) => errors.push(format!("{e} - near: {}…", truncate_chars(stmt, 80))),
        }
    }

    Ok(finish_import(app, total, ok, errors))
}

// ── D1 export ─────────────────────────────────────────────────────────────────

/// Cloudflare D1 rejects access to its internal tables (prefixed `_cf_`) with a
/// `SQLITE_AUTH` error, and SQLite's own `sqlite_*` tables aren't user data.
/// Skip both so a backup doesn't abort on e.g. `SELECT * FROM "_cf_KV"`.
fn is_d1_internal_table(name: &str) -> bool {
    name.starts_with("_cf_") || name.starts_with("sqlite_")
}

async fn export_d1(
    app: &AppHandle,
    cfg: &super::connection::D1Config,
    filter: Option<&[String]>,
    opts: &ExportOptions,
) -> Result<ExportResult, String> {
    emit_log(app, "backup-log", "info", "Starting Cloudflare D1 export…");
    let mut out = backup_header("Cloudflare D1 (SQLite)", None);
    out.push_str("PRAGMA foreign_keys=OFF;\nBEGIN TRANSACTION;\n\n");

    let ddl_result = super::d1::query(
        cfg,
        "SELECT type, name, tbl_name, sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY CASE type WHEN 'table' THEN 0 WHEN 'index' THEN 1 WHEN 'view' THEN 2 ELSE 3 END, name",
        vec![],
    ).await?;

    let type_idx = ddl_result.columns.iter().position(|c| c.name == "type").unwrap_or(0);
    let name_idx = ddl_result.columns.iter().position(|c| c.name == "name").unwrap_or(1);
    let tbl_idx  = ddl_result.columns.iter().position(|c| c.name == "tbl_name").unwrap_or(2);
    let sql_idx  = ddl_result.columns.iter().position(|c| c.name == "sql").unwrap_or(3);

    // A trigger/index belongs to `tbl_name`; only emit it when that table is in
    // the selection, or the restore would reference a table that was never created.
    let in_filter = |t: &str| filter.map_or(true, |f| f.iter().any(|x| x == t));

    let mut all_tables: Vec<String> = Vec::new();
    for row in &ddl_result.rows {
        let obj_type = row.get(type_idx).and_then(|v| v.as_str()).unwrap_or("");
        let name     = row.get(name_idx).and_then(|v| v.as_str()).unwrap_or("");
        let tbl_name = row.get(tbl_idx).and_then(|v| v.as_str()).unwrap_or("");
        let ddl_sql  = row.get(sql_idx).and_then(|v| v.as_str()).unwrap_or("");

        match obj_type {
            "table" => {
                // Skip Cloudflare D1 internal tables (e.g. _cf_KV): D1 rejects
                // SELECT against them with SQLITE_AUTH.
                if is_d1_internal_table(name) { continue; }
                all_tables.push(name.to_string());
                if opts.include_schema && in_filter(name) {
                    out.push_str(ddl_sql.trim()); out.push_str(";\n");
                }
            }
            "view" => {
                if opts.include_views { out.push_str(ddl_sql.trim()); out.push_str(";\n"); }
            }
            "trigger" => {
                if opts.include_triggers && in_filter(tbl_name) { out.push_str(ddl_sql.trim()); out.push_str(";\n"); }
            }
            _ => {
                if opts.include_schema && in_filter(tbl_name) { out.push_str(ddl_sql.trim()); out.push_str(";\n"); }
            }
        }
    }

    let tables_to_dump: Vec<&String> = all_tables.iter()
        .filter(|t| filter.map_or(true, |f| f.iter().any(|ft| ft == *t)))
        .collect();

    let mut total_rows = 0usize;
    if opts.include_data {
        emit_log(app, "backup-log", "info", format!("Exporting {} table(s)…", tables_to_dump.len()));
        for table in &tables_to_dump {
            if is_cancelled() { break; }
            emit_log(app, "backup-log", "info", format!("  → {table}"));
            let data_result = match super::d1::query(cfg,
                &format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")), vec![]).await {
                Ok(r) => r,
                Err(e) => {
                    // A single unreadable/prohibited table shouldn't abort the whole backup.
                    out.push_str(&format!("-- ERROR exporting {table}: {e}\n"));
                    emit_log(app, "backup-log", "error", format!("  ✗ {table}: {e}"));
                    continue;
                }
            };
            let n = data_result.rows.len();
            if !data_result.rows.is_empty() {
                let col_names: Vec<String> = data_result.columns.iter()
                    .map(|c| format!("\"{}\"", c.name.replace('"', "\"\"")))
                    .collect();
                let col_list = col_names.join(", ");
                out.push('\n');
                for row in &data_result.rows {
                    let vals: Vec<String> = data_result.columns.iter().enumerate()
                        .map(|(i, _)| row.get(i).map_or("NULL".into(), json_to_sql_val))
                        .collect();
                    out.push_str(&format!("INSERT OR REPLACE INTO \"{}\" ({}) VALUES ({});\n",
                        table.replace('"', "\"\""), col_list, vals.join(", ")));
                }
            }
            total_rows += n;
            emit_log(app, "backup-log", "ok", format!("  ✓ {table} - {n} rows"));
        }
    }

    out.push_str("\nCOMMIT;\nPRAGMA foreign_keys=ON;\n");
    emit_log(app, "backup-log", "ok", format!("Export complete: {} tables, {total_rows} rows", tables_to_dump.len()));
    Ok(ExportResult { sql: out, table_count: tables_to_dump.len(), row_count: total_rows, cancelled: is_cancelled() })
}

fn json_to_sql_val(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "NULL".into(),
        serde_json::Value::Bool(b) => if *b { "1" } else { "0" }.into(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        // D1's HTTP API returns BLOB columns as JSON arrays of byte integers.
        // Emit them as SQLite hex blob literals (X'…') so binary data round-trips
        // instead of being stored as the literal text "[1,2,3]".
        serde_json::Value::Array(arr) => {
            let bytes: Option<Vec<u8>> = arr.iter()
                .map(|b| b.as_u64().filter(|n| *n <= 255).map(|n| n as u8))
                .collect();
            match bytes {
                Some(b) => format!("X'{}'", hex::encode(b)),
                None => format!("'{}'", v.to_string().replace('\'', "''")),
            }
        }
        other => format!("'{}'", other.to_string().replace('\'', "''")),
    }
}

// ── D1 import ────────────────────────────────────────────────────────────────

async fn import_d1(app: &AppHandle, cfg: &super::connection::D1Config, sql: &str) -> Result<ImportResult, String> {
    let stmts = split_statements(sql, Escapes::Standard);
    let total = stmts.len();
    emit_log(app, "restore-log", "info", format!("Starting restore: {} statements…", total));
    let mut ok = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for (i, stmt) in stmts.iter().enumerate() {
        if is_cancelled() { break; }
        let low = stmt.trim_start().to_lowercase();
        if low.starts_with("pragma foreign_keys") { ok += 1; continue; }
        match super::d1::query(cfg, stmt, vec![]).await {
            Ok(_) => {
                ok += 1;
                if (i + 1) % 50 == 0 {
                    emit_log(app, "restore-log", "info", format!("  {}/{} statements…", i + 1, total));
                }
            }
            Err(e) => errors.push(format!("{e} - near: {}…", truncate_chars(stmt, 60))),
        }
    }

    Ok(finish_import(app, total, ok, errors))
}

// ── DuckDB export / import ────────────────────────────────────────────────────

/// DuckDB's `EXPORT`/`IMPORT DATABASE` operate on a *directory* of Parquet files
/// (schema.sql + load.sql + one file per table), not a single SQL script. The
/// engine is embedded, so that directory lives on this client's filesystem.
///
/// The whole database is exported - DuckDB has no per-table/schema `EXPORT`, so
/// the schema/table selection used by the SQL-scripting engines doesn't apply.
/// The export directory is derived next to the database file (a `:memory:`
/// database falls back to the system temp dir). The returned "script" records
/// the directory and, when re-imported, runs `IMPORT DATABASE` against it.
async fn export_duckdb(
    app: &AppHandle,
    handle: &super::connection::DuckdbHandle,
) -> Result<ExportResult, String> {
    emit_log(app, "backup-log", "info", "Starting DuckDB export…");

    let db_path = duckdb_database_path(handle).await;
    let export_dir = derive_duckdb_export_dir(db_path.as_deref());
    std::fs::create_dir_all(&export_dir)
        .map_err(|e| format!("Failed to create export directory {}: {e}", export_dir.display()))?;
    let dir_str = export_dir.to_string_lossy().to_string();
    let dir_esc = dir_str.replace('\'', "''");

    emit_log(app, "backup-log", "info", format!("Exporting database to {dir_str}…"));
    super::duckdb::execute_sql(handle, &format!("EXPORT DATABASE '{dir_esc}' (FORMAT PARQUET)")).await?;

    // Best-effort table count for the UI; the export has already succeeded.
    let table_count = duckdb_table_count(handle).await;

    let mut out = backup_header("DuckDB", None);
    out.push_str(
        "-- NOTE: DuckDB EXPORT/IMPORT DATABASE operate on a directory of Parquet\n\
         --       files on the local filesystem. The data was written to the path\n\
         --       below; keep that directory to restore. Re-running this script\n\
         --       executes IMPORT DATABASE against it.\n\n",
    );
    out.push_str(&format!("IMPORT DATABASE '{dir_esc}';\n"));

    emit_log(app, "backup-log", "ok", format!("Export complete: {table_count} tables → {dir_str}"));
    Ok(ExportResult { sql: out, table_count, row_count: 0, cancelled: is_cancelled() })
}

/// Path of the current DuckDB database file (`None` for an in-memory database).
async fn duckdb_database_path(handle: &super::connection::DuckdbHandle) -> Option<String> {
    let res = super::duckdb::execute_sql(
        handle,
        "SELECT path FROM duckdb_databases() WHERE path IS NOT NULL AND path <> '' LIMIT 1",
    )
    .await
    .ok()?;
    res.rows.first()?.first()?.as_str().map(|s| s.to_string())
}

/// Fresh, timestamped export directory beside the database file (or in the system
/// temp dir when the database has no on-disk path).
fn derive_duckdb_export_dir(db_path: Option<&str>) -> std::path::PathBuf {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match db_path {
        Some(p) => {
            let path = std::path::Path::new(p);
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("duckdb");
            let parent = match path.parent() {
                Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
                _ => std::path::PathBuf::from("."),
            };
            parent.join(format!("{stem}_stroke_backup_{ts}"))
        }
        None => std::env::temp_dir().join(format!("duckdb_stroke_backup_{ts}")),
    }
}

async fn duckdb_table_count(handle: &super::connection::DuckdbHandle) -> usize {
    super::duckdb::execute_sql(
        handle,
        "SELECT count(*) FROM information_schema.tables WHERE table_type = 'BASE TABLE'",
    )
    .await
    .ok()
    .and_then(|r| r.rows.into_iter().next())
    .and_then(|row| row.into_iter().next())
    .and_then(|v| v.as_i64())
    .unwrap_or(0) as usize
}

async fn import_duckdb(
    app: &AppHandle,
    handle: &super::connection::DuckdbHandle,
    sql: &str,
) -> Result<ImportResult, String> {
    let stmts = split_statements(sql, Escapes::Standard);
    let total = stmts.len();
    emit_log(app, "restore-log", "info", format!("Starting restore: {} statements…", total));
    let mut ok = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for stmt in &stmts {
        if is_cancelled() { break; }
        let low = stmt.trim_start().to_lowercase();
        if low.starts_with("pragma foreign_keys") { ok += 1; continue; }
        match super::duckdb::execute_sql(handle, stmt).await {
            Ok(_) => { ok += 1; }
            Err(e) => errors.push(format!("{e} - near: {}…", truncate_chars(stmt, 60))),
        }
    }

    Ok(finish_import(app, total, ok, errors))
}

// ── MS SQL Server export / import ─────────────────────────────────────────────

/// SQL Server backup/restore is a whole-database, server-side operation:
/// `BACKUP`/`RESTORE` write/read a file on the **SQL Server host** filesystem,
/// not on this client. A bare filename lands in the instance's configured default
/// backup directory. The returned "script" records the matching `RESTORE`.
async fn export_mssql(
    app: &AppHandle,
    handle: &super::connection::MssqlHandle,
) -> Result<ExportResult, String> {
    emit_log(app, "backup-log", "info", "Starting MS SQL Server export…");

    // The database name comes from the live connection - the active-connection
    // handle carries the client, not the MssqlConfig.
    let db = mssql_current_db(handle).await?;
    let bak_file = format!("{db}_stroke_backup.bak");
    let db_ident = db.replace(']', "]]"); // bracket-quoted identifier
    let path_esc = bak_file.replace('\'', "''"); // single-quoted literal

    emit_log(app, "backup-log", "info", format!("Backing up [{db}] to server-side file {bak_file}…"));
    super::mssql::execute_sql(
        handle,
        &format!("BACKUP DATABASE [{db_ident}] TO DISK = N'{path_esc}' WITH FORMAT, INIT"),
    )
    .await?;

    let table_count = mssql_table_count(handle).await;

    let mut out = backup_header("MS SQL Server", None);
    out.push_str(
        "-- NOTE: BACKUP/RESTORE operate on a file on the SQL Server host filesystem\n\
         --       (server-side), not on this client. The database was written to the\n\
         --       server-side path below. Re-running this script issues RESTORE.\n\n",
    );
    out.push_str(&format!("RESTORE DATABASE [{db_ident}] FROM DISK = N'{path_esc}' WITH REPLACE;\n"));

    emit_log(app, "backup-log", "ok", format!("Backup complete: [{db}] → {bak_file}"));
    Ok(ExportResult { sql: out, table_count, row_count: 0, cancelled: is_cancelled() })
}

/// Name of the database the active MS SQL connection is currently using.
async fn mssql_current_db(handle: &super::connection::MssqlHandle) -> Result<String, String> {
    let res = super::mssql::execute_sql(handle, "SELECT DB_NAME()").await?;
    res.rows
        .first()
        .and_then(|r| r.first())
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Could not determine current MS SQL Server database name".to_string())
}

async fn mssql_table_count(handle: &super::connection::MssqlHandle) -> usize {
    super::mssql::execute_sql(handle, "SELECT count(*) FROM sys.tables")
        .await
        .ok()
        .and_then(|r| r.rows.into_iter().next())
        .and_then(|row| row.into_iter().next())
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as usize
}

async fn import_mssql(
    app: &AppHandle,
    handle: &super::connection::MssqlHandle,
    sql: &str,
) -> Result<ImportResult, String> {
    let stmts = split_statements(sql, Escapes::Standard);
    let total = stmts.len();
    emit_log(app, "restore-log", "info", format!("Starting restore: {} statements…", total));
    let mut ok = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for stmt in &stmts {
        if is_cancelled() { break; }
        match super::mssql::execute_sql(handle, stmt).await {
            Ok(_) => { ok += 1; }
            Err(e) => errors.push(format!("{e} - near: {}…", truncate_chars(stmt, 80))),
        }
    }

    Ok(finish_import(app, total, ok, errors))
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{split_statements, truncate_chars, Escapes};

    /// Most dialects we export to use standard string rules.
    fn split(sql: &str) -> Vec<String> { split_statements(sql, Escapes::Standard) }

    #[test]
    fn splits_basic_statements() {
        let s = split("SELECT 1; SELECT 2;");
        assert_eq!(s, vec!["SELECT 1".to_string(), "SELECT 2".to_string()]);
    }

    #[test]
    fn ignores_semicolons_inside_strings() {
        let s = split("INSERT INTO t VALUES ('a;b', 'c''d;e');");
        assert_eq!(s.len(), 1);
        assert!(s[0].contains("'a;b'"));
    }

    #[test]
    fn keeps_dollar_quoted_body_intact() {
        // A Postgres enum DO-block: internal semicolons must not split it.
        let sql = "DO $$ BEGIN\n  CREATE TYPE \"s\" AS ENUM ('a','b');\nEXCEPTION WHEN duplicate_object THEN NULL;\nEND $$;\nSELECT 1;";
        let s = split(sql);
        assert_eq!(s.len(), 2, "got: {s:?}");
        assert!(s[0].starts_with("DO $$"));
        assert!(s[0].contains("EXCEPTION"));
        assert_eq!(s[1], "SELECT 1");
    }

    #[test]
    fn handles_tagged_dollar_quotes() {
        let sql = "CREATE FUNCTION f() RETURNS int AS $func$ BEGIN RETURN 1; END; $func$ LANGUAGE plpgsql;";
        let s = split(sql);
        assert_eq!(s.len(), 1, "got: {s:?}");
        assert!(s[0].contains("RETURN 1;"));
    }

    #[test]
    fn respects_mysql_delimiter() {
        let sql = "DELIMITER //\nCREATE TRIGGER t BEFORE INSERT ON x FOR EACH ROW BEGIN INSERT INTO log VALUES (1); UPDATE c SET n=n+1; END//\nDELIMITER ;\nSELECT 1;";
        let s = split(sql);
        assert_eq!(s.len(), 2, "got: {s:?}");
        assert!(s[0].starts_with("CREATE TRIGGER"));
        assert!(s[0].contains("UPDATE c SET n=n+1"));
        assert_eq!(s[1], "SELECT 1");
    }

    #[test]
    fn strips_line_comments() {
        let s = split("SELECT 1; -- a trailing note\nSELECT 2;");
        assert_eq!(s, vec!["SELECT 1".to_string(), "SELECT 2".to_string()]);
    }

    #[test]
    fn truncate_chars_is_utf8_safe() {
        // Would panic with byte slicing when the boundary lands mid-character.
        let s = "😀😀😀😀";
        assert_eq!(truncate_chars(s, 2), "😀😀");
        assert_eq!(truncate_chars(s, 10), s);
    }

    // ── Regressions ──────────────────────────────────────────────────────────

    #[test]
    fn mysql_backslash_escaped_quote_does_not_end_the_literal() {
        // Exactly what `mysql_val` emits for the value `it's; and \`. Read with
        // standard rules the literal ends at `it\'`, the `;` then splits the
        // row in half and every later statement inherits the damage.
        let sql = "INSERT INTO t VALUES ('it\\'s; and \\\\');\nSELECT 1;";
        let s = split_statements(sql, Escapes::Backslash);
        assert_eq!(s.len(), 2, "got: {s:?}");
        assert!(s[0].starts_with("INSERT INTO t"), "got: {s:?}");
        assert!(s[0].contains("it\\'s; and"), "got: {s:?}");
        assert_eq!(s[1], "SELECT 1");
    }

    #[test]
    fn standard_dialects_treat_a_trailing_backslash_as_data() {
        // The mirror image: in Postgres `'a\'` is a complete string. Applying
        // MySQL's rule here would swallow the terminator and join statements.
        let s = split("INSERT INTO t VALUES ('a\\');\nSELECT 1;");
        assert_eq!(s.len(), 2, "got: {s:?}");
        assert!(s[0].contains("'a\\'"));
        assert_eq!(s[1], "SELECT 1");
    }

    #[test]
    fn strips_block_comments_including_their_semicolons() {
        let s = split("SELECT 1; /* a note; with a semicolon */ SELECT 2;");
        assert_eq!(s.len(), 2, "got: {s:?}");
        assert_eq!(s[0], "SELECT 1");
        assert!(s[1].contains("SELECT 2"), "got: {s:?}");
        assert!(!s[1].contains("a note"), "comment leaked into the statement: {s:?}");
    }

    #[test]
    fn strips_nested_block_comments() {
        // Postgres nests these; stopping at the first `*/` would leave
        // `still a comment */` sitting in front of the next statement.
        let s = split("SELECT 1; /* outer /* inner; */ still a comment */ SELECT 2;");
        assert_eq!(s.len(), 2, "got: {s:?}");
        assert!(!s[1].contains("comment"), "got: {s:?}");
    }

    #[test]
    fn keeps_a_block_comment_inside_a_string_literal() {
        let s = split("INSERT INTO t VALUES ('/* not a comment; */');");
        assert_eq!(s.len(), 1, "got: {s:?}");
        assert!(s[0].contains("/* not a comment; */"));
    }

    #[test]
    fn drops_mysqldump_executable_comments() {
        let sql = "/*!40101 SET NAMES utf8 */;\nINSERT INTO t VALUES (1);";
        let s = split_statements(sql, Escapes::Backslash);
        assert_eq!(s, vec!["INSERT INTO t VALUES (1)".to_string()], "got: {s:?}");
    }

    #[test]
    fn division_is_not_a_block_comment() {
        let s = split("SELECT a/b FROM t; SELECT 2;");
        assert_eq!(s.len(), 2, "got: {s:?}");
        assert!(s[0].contains("a/b"));
    }
}
