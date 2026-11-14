/*!
Backup and restore for all supported database engines.

Export produces a plain SQL script that can be re-executed to restore.
Options control which object types are included (schema DDL, data, sequences,
enums, functions, triggers, views).

Speed notes (why the code is shaped the way it is):

- The dump streams straight to a buffered file when the caller names one, so
  a large backup never sits in memory and never crosses IPC as one string.
- Postgres data is `COPY … TO STDOUT` passed through byte for byte and written
  as pg_dump-style `COPY … FROM stdin` blocks; restore feeds them back with
  `COPY … FROM STDIN`. Tables are copied on a small pool of connections that
  share one exported snapshot, so the dump is still a single consistent view.
- MySQL / SQLite / D1 rows go out as multi-row INSERTs, not one per row.
- Restore runs batches of INSERTs per round trip inside one transaction and
  only falls back to statement-by-statement when a batch fails, so a single
  bad row is still reported on its own.
- Progress is a per-table event, throttled while a table is being copied.
*/

use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use futures::{StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use sqlx::{Column, Executor, Row, TypeInfo};
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

/// How often a long-running table or restore reports progress.
const PROGRESS_EVERY: Duration = Duration::from_millis(200);
/// Tables copied at once on Postgres. Each takes a pool connection (plus one
/// that holds the shared snapshot), so this leaves room for the rest of the app.
const PG_PARALLEL: usize = 3;
/// MySQL pools are 4 wide; two workers keep two free for the UI.
const MYSQL_PARALLEL: usize = 2;
/// Target size of one multi-row INSERT. Well under MySQL's default
/// max_allowed_packet (4-64 MB depending on version).
const INSERT_BATCH_BYTES: usize = 1 << 20;
/// D1 rejects statements over 100 KB, and SQLite dumps are often replayed into
/// D1, so the SQLite family stays under that.
const SQLITE_BATCH_BYTES: usize = 90 * 1024;
/// Restore: INSERTs sent per round trip.
const RESTORE_BATCH_BYTES: usize = 4 << 20;
const RESTORE_BATCH_STMTS: usize = 1000;

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    /// The script, when no `path` was given. Empty when it went to a file.
    pub sql: String,
    pub table_count: usize,
    pub row_count: usize,
    /// True when the run stopped at a cancel request, so `sql` covers only the
    /// objects reached before it. The script is a partial dump and must not be
    /// offered as a usable backup. A file export deletes its partial file.
    pub cancelled: bool,
    /// File the dump was written to, when the caller asked for one.
    pub path: Option<String>,
    /// Size of the dump in bytes.
    pub bytes: u64,
    /// Tables that could not be exported (their error is in the dump and the log).
    pub failed_tables: usize,
    pub elapsed_ms: u64,
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
    pub elapsed_ms: u64,
}

/// What `backup_inspect_file` learns from the first few KB of a dump.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFileInfo {
    pub size: u64,
    /// "PostgreSQL", "MySQL", "SQLite", … when the header says so.
    pub engine: Option<String>,
    /// Written by Stroke (vs pg_dump / mysqldump / sqlite3 .dump / unknown).
    pub stroke: bool,
    /// A cancelled Stroke export: restoring it would load a fraction of the data.
    pub incomplete: bool,
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

// ── Progress reporting ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct BackupLog {
    level: &'static str,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanEntry {
    schema: Option<String>,
    name: String,
    est_rows: Option<i64>,
}

/// `backup-progress` payload, `kind: "plan"`: every table the export will
/// visit, sent once up front so the UI can list them before any finish.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanEvent {
    kind: &'static str,
    tables: Vec<PlanEntry>,
}

/// `backup-progress` payload, `kind: "table"`: one table's state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TableEvent<'a> {
    kind: &'static str,
    schema: Option<&'a str>,
    name: &'a str,
    /// "running" | "done" | "error"
    status: &'static str,
    rows: u64,
    error: Option<String>,
}

/// `restore-progress` payload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RestoreProgress {
    bytes_done: u64,
    bytes_total: u64,
    ok: usize,
    failed: usize,
}

/// Where progress goes. Holding the app handle optionally lets the exporters
/// and importers run without Tauri, which is how the tests drive them.
pub(crate) struct Reporter {
    app: Option<AppHandle>,
    log_event: &'static str,
    progress_event: &'static str,
}

impl Reporter {
    fn backup(app: &AppHandle) -> Self {
        Self { app: Some(app.clone()), log_event: "backup-log", progress_event: "backup-progress" }
    }
    fn restore(app: &AppHandle) -> Self {
        Self { app: Some(app.clone()), log_event: "restore-log", progress_event: "restore-progress" }
    }
    #[cfg(test)]
    pub(crate) fn silent() -> Self {
        Self { app: None, log_event: "", progress_event: "" }
    }
    fn log(&self, level: &'static str, message: impl Into<String>) {
        if let Some(app) = &self.app {
            app.emit(self.log_event, BackupLog { level, message: message.into() }).ok();
        }
    }
    fn progress<T: Serialize + Clone>(&self, payload: T) {
        if let Some(app) = &self.app {
            app.emit(self.progress_event, payload).ok();
        }
    }
    fn plan(&self, tables: Vec<PlanEntry>) {
        self.progress(PlanEvent { kind: "plan", tables });
    }
    fn table(&self, schema: Option<&str>, name: &str, status: &'static str, rows: u64, error: Option<String>) {
        self.progress(TableEvent { kind: "table", schema, name, status, rows, error });
    }
}

// ── Output ────────────────────────────────────────────────────────────────────

enum Sink {
    Mem(Vec<u8>),
    File(BufWriter<File>),
}

/// The dump being written: a buffered file, or memory when the caller wants
/// the script back as a string. Write errors are latched and surfaced by
/// `check`/`finish`, so the hot path stays a plain `push_str`.
pub(crate) struct Out {
    sink: Sink,
    bytes: u64,
    err: Option<io::Error>,
}

impl Out {
    pub(crate) fn memory() -> Self {
        Self { sink: Sink::Mem(Vec::with_capacity(1 << 16)), bytes: 0, err: None }
    }

    pub(crate) fn file(path: &Path) -> Result<Self, String> {
        let f = File::create(path).map_err(|e| format!("Could not create {}: {e}", path.display()))?;
        Ok(Self { sink: Sink::File(BufWriter::with_capacity(1 << 20, f)), bytes: 0, err: None })
    }

    fn write(&mut self, b: &[u8]) {
        if self.err.is_some() { return; }
        match &mut self.sink {
            Sink::Mem(v) => v.extend_from_slice(b),
            Sink::File(w) => {
                if let Err(e) = w.write_all(b) { self.err = Some(e); return; }
            }
        }
        self.bytes += b.len() as u64;
    }

    fn push_str(&mut self, s: &str) { self.write(s.as_bytes()); }
    fn push(&mut self, c: char) {
        let mut buf = [0u8; 4];
        self.push_str(c.encode_utf8(&mut buf));
    }

    /// Append a table's buffered output (memory or spill file) in one go.
    fn append(&mut self, mut chunk: Chunk) -> Result<(), String> {
        if let Some(e) = chunk.err.take() {
            return Err(format!("Could not buffer the dump: {e}"));
        }
        match chunk.spill.take() {
            None => self.write(&chunk.mem),
            Some((path, w)) => {
                let res = (|| -> io::Result<u64> {
                    drop(w.into_inner().map_err(|e| e.into_error())?);
                    let mut r = File::open(&path)?;
                    match &mut self.sink {
                        Sink::Mem(v) => r.read_to_end(v).map(|n| n as u64),
                        Sink::File(w) => io::copy(&mut r, w),
                    }
                })();
                let _ = std::fs::remove_file(&path);
                match res {
                    Ok(n) => self.bytes += n,
                    Err(e) => return Err(format!("Could not write the dump: {e}")),
                }
            }
        }
        self.check()
    }

    fn check(&self) -> Result<(), String> {
        match &self.err {
            Some(e) => Err(format!("Could not write the dump: {e}")),
            None => Ok(()),
        }
    }

    /// Flush. Returns the script for a memory sink (empty for a file).
    fn finish(self) -> Result<(Vec<u8>, u64), String> {
        self.check()?;
        match self.sink {
            Sink::Mem(v) => Ok((v, self.bytes)),
            Sink::File(mut w) => {
                w.flush().map_err(|e| format!("Could not write the dump: {e}"))?;
                w.get_ref().sync_all().ok();
                Ok((Vec::new(), self.bytes))
            }
        }
    }
}

/// Above this a table's buffered output moves to a temp file, so tables
/// copied in parallel never hold more than a few MB of RAM each.
const SPILL_AT: usize = 32 << 20;
static SPILL_SEQ: AtomicU64 = AtomicU64::new(0);

/// One table's worth of dump, produced off the main writer so several tables
/// can be read at once and still land in the file in a fixed order.
struct Chunk {
    mem: Vec<u8>,
    spill: Option<(PathBuf, BufWriter<File>)>,
    err: Option<io::Error>,
}

impl Chunk {
    fn new() -> Self { Self { mem: Vec::with_capacity(1 << 16), spill: None, err: None } }

    fn put(&mut self, b: &[u8]) {
        if self.err.is_some() { return; }
        if let Some((_, w)) = &mut self.spill {
            if let Err(e) = w.write_all(b) { self.err = Some(e); }
            return;
        }
        self.mem.extend_from_slice(b);
        if self.mem.len() >= SPILL_AT { self.spill_now(); }
    }

    fn put_str(&mut self, s: &str) { self.put(s.as_bytes()); }

    fn spill_now(&mut self) {
        let n = SPILL_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("stroke-backup-{}-{n}.part", std::process::id()));
        match File::create(&path) {
            Ok(f) => {
                let mut w = BufWriter::with_capacity(1 << 20, f);
                if let Err(e) = w.write_all(&self.mem) { self.err = Some(e); }
                self.mem = Vec::new();
                self.spill = Some((path, w));
            }
            Err(e) => self.err = Some(e),
        }
    }
}

impl Drop for Chunk {
    fn drop(&mut self) {
        if let Some((path, w)) = self.spill.take() {
            drop(w);
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Collects rows into multi-row INSERT statements of about `max_bytes` each.
struct InsertBatcher {
    prefix: String,
    suffix: &'static str,
    buf: String,
    max_bytes: usize,
}

impl InsertBatcher {
    /// `prefix` is everything up to and including `VALUES`.
    fn new(prefix: String, suffix: &'static str, max_bytes: usize) -> Self {
        Self { prefix, suffix, buf: String::with_capacity(max_bytes + 4096), max_bytes }
    }

    /// Start a row: returns the buffer to write the comma-separated values into.
    fn row(&mut self) -> &mut String {
        if self.buf.is_empty() {
            self.buf.push_str(&self.prefix);
            self.buf.push_str("\n(");
        } else {
            self.buf.push_str(",\n(");
        }
        &mut self.buf
    }

    /// Finish a row; writes the statement out once it is big enough.
    fn end_row(&mut self, chunk: &mut Chunk) {
        self.buf.push(')');
        if self.buf.len() >= self.max_bytes { self.flush(chunk); }
    }

    fn flush(&mut self, chunk: &mut Chunk) {
        if self.buf.is_empty() { return; }
        self.buf.push_str(self.suffix);
        self.buf.push_str(";\n");
        chunk.put_str(&self.buf);
        self.buf.clear();
    }
}

/// Double-quoted identifier (Postgres / SQLite).
fn qi(s: &str) -> String { format!("\"{}\"", s.replace('"', "\"\"")) }
/// Schema-qualified double-quoted name.
fn qn(schema: &str, name: &str) -> String { format!("{}.{}", qi(schema), qi(name)) }
/// Backtick identifier (MySQL).
fn qb(s: &str) -> String { format!("`{}`", s.replace('`', "``")) }
/// Single-quoted standard SQL literal.
fn ql(s: &str) -> String { format!("'{}'", s.replace('\'', "''")) }

fn count_rows_in(buf: &[u8]) -> u64 { buf.iter().filter(|&&b| b == b'\n').count() as u64 }

fn rows_word(n: u64) -> &'static str { if n == 1 { "row" } else { "rows" } }
fn tables_word(n: usize) -> &'static str { if n == 1 { "table" } else { "tables" } }

/// Where an export streams while it runs: beside the target, renamed over it
/// only once the dump is complete, so a cancelled or failed run never leaves a
/// truncated file under the name the user picked.
fn partial_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".partial");
    path.with_file_name(name)
}

// ── Tauri commands ────────────────────────────────────────────────────────────

/// Export the connected database. With `path`, the dump streams to that file
/// (the result's `sql` is then empty); without it the script is returned.
#[tauri::command]
pub async fn backup_export(
    app: AppHandle,
    state: State<'_, DbState>,
    schema: Option<String>,
    tables: Option<Vec<String>>,
    options: Option<ExportOptions>,
    path: Option<String>,
) -> Result<ExportResult, String> {
    let opts = options.unwrap_or_default();
    reset_cancel();
    let rep = Reporter::backup(&app);
    let conn = require_conn(&state)?;
    run_export(&rep, conn, schema, tables, &opts, path).await
}

async fn run_export(
    rep: &Reporter,
    conn: ActiveConnection,
    schema: Option<String>,
    tables: Option<Vec<String>>,
    opts: &ExportOptions,
    path: Option<String>,
) -> Result<ExportResult, String> {
    let started = Instant::now();
    let target = path.as_deref().map(PathBuf::from);
    let partial = target.as_deref().map(partial_path);
    let mut out = match &partial {
        Some(p) => Out::file(p)?,
        None => Out::memory(),
    };
    let discard = |p: &Option<PathBuf>| {
        if let Some(p) = p { let _ = std::fs::remove_file(p); }
    };

    let stats = match export_into(rep, conn, schema, tables, opts, &mut out).await {
        Ok(s) => s,
        Err(e) => {
            drop(out);
            discard(&partial);
            rep.log("error", format!("Export failed: {e}"));
            return Err(e);
        }
    };
    let cancelled = is_cancelled();
    let elapsed_ms = started.elapsed().as_millis() as u64;

    if cancelled {
        rep.log("warn", "Cancelled - this dump is incomplete");
        if partial.is_some() {
            // Nothing to offer: the partial file goes, the target is untouched.
            drop(out);
            discard(&partial);
            return Ok(ExportResult {
                sql: String::new(), table_count: stats.tables, row_count: stats.rows, cancelled: true,
                path: None, bytes: 0, failed_tables: stats.failed, elapsed_ms,
            });
        }
        // A cancelled export stops at whatever object it had reached, so the
        // script is a prefix of the real dump. Say so in the script itself: it
        // may well outlive this window, and a truncated backup that looks
        // complete is worse than no backup at all.
        let (bytes, _) = out.finish()?;
        let body = String::from_utf8_lossy(&bytes);
        let mut sql = String::with_capacity(body.len() + 192);
        sql.push_str("-- !! INCOMPLETE BACKUP - cancelled before it finished. Do not restore from this file. !!\n");
        sql.push_str(&body);
        sql.push_str("\n-- !! END OF INCOMPLETE BACKUP - objects after this point were never exported. !!\n");
        let n = sql.len() as u64;
        return Ok(ExportResult {
            sql, table_count: stats.tables, row_count: stats.rows, cancelled: true,
            path: None, bytes: n, failed_tables: stats.failed, elapsed_ms,
        });
    }

    let (bytes, size) = match out.finish() {
        Ok(v) => v,
        Err(e) => { discard(&partial); return Err(e); }
    };
    if let (Some(tmp), Some(dst)) = (&partial, &target) {
        if let Err(e) = std::fs::rename(tmp, dst) {
            discard(&partial);
            return Err(format!("Could not save {}: {e}", dst.display()));
        }
    }
    let sql = if target.is_some() {
        String::new()
    } else {
        String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
    };
    let secs = elapsed_ms as f64 / 1000.0;
    rep.log(
        if stats.failed > 0 { "warn" } else { "ok" },
        format!(
            "Export complete: {} {}, {} {} in {secs:.1}s",
            stats.tables, tables_word(stats.tables), stats.rows, rows_word(stats.rows as u64),
        ),
    );
    Ok(ExportResult {
        sql, table_count: stats.tables, row_count: stats.rows, cancelled: false,
        path: path.clone(), bytes: size, failed_tables: stats.failed, elapsed_ms,
    })
}

#[derive(Debug, Default, Clone, Copy)]
struct Stats {
    tables: usize,
    rows: usize,
    failed: usize,
}

async fn export_into(
    rep: &Reporter,
    conn: ActiveConnection,
    schema: Option<String>,
    tables: Option<Vec<String>>,
    opts: &ExportOptions,
    out: &mut Out,
) -> Result<Stats, String> {
    match conn {
        ActiveConnection::Sqlite(pool) => export_sqlite(rep, &pool, tables.as_deref(), opts, out).await,
        ActiveConnection::Postgres(pool) => export_postgres(rep, &pool, schema.as_deref(), tables.as_deref(), opts, out).await,
        ActiveConnection::Mysql(pool) => export_mysql(rep, &pool, schema.as_deref(), tables.as_deref(), opts, out).await,
        ActiveConnection::D1(cfg) => export_d1(rep, &cfg, tables.as_deref(), opts, out).await,
        ActiveConnection::LibSql(_) => Err("Backup export is not supported for LibSQL/Turso connections".to_string()),
        ActiveConnection::Clickhouse(_) => Err("Backup export is not supported for ClickHouse connections".to_string()),
        ActiveConnection::Posthog(_) => Err("Backup is not supported for PostHog".to_string()),
        ActiveConnection::Redis(_) => Err("Backup is not supported on Redis".to_string()),
        ActiveConnection::Dynamodb(_) | ActiveConnection::Doc(_) => Err("Backup is not supported on DynamoDB".to_string()),
        ActiveConnection::Duckdb(h) => export_duckdb(rep, &h, out).await,
        ActiveConnection::Mssql(h) => export_mssql(rep, &h, out).await,
        ActiveConnection::Oracle(_) => Err("Backup export is not supported for Oracle yet".to_string()),
    }
}

/// Restore a script into the connected database. Pass the script as `sql`, or
/// a file as `path` (preferred: the file is read here, not shipped over IPC).
#[tauri::command]
pub async fn backup_import(
    app: AppHandle,
    state: State<'_, DbState>,
    sql: Option<String>,
    path: Option<String>,
) -> Result<ImportResult, String> {
    reset_cancel();
    let rep = Reporter::restore(&app);
    let script = match (sql, path) {
        (Some(s), _) => s,
        (None, Some(p)) => read_script(&rep, &p).await?,
        (None, None) => return Err("Nothing to restore: no file or SQL given".into()),
    };
    match require_conn(&state)? {
        ActiveConnection::Sqlite(pool) => import_sqlite(&rep, &pool, &script).await,
        ActiveConnection::Postgres(pool) => import_postgres(&rep, &pool, &script).await,
        ActiveConnection::Mysql(pool) => import_mysql(&rep, &pool, &script).await,
        ActiveConnection::D1(cfg) => import_d1(&rep, &cfg, &script).await,
        ActiveConnection::LibSql(_) => Err("Backup import is not supported for LibSQL/Turso connections".to_string()),
        ActiveConnection::Clickhouse(_) => Err("Backup import is not supported for ClickHouse connections".to_string()),
        ActiveConnection::Posthog(_) => Err("Backup is not supported for PostHog".to_string()),
        ActiveConnection::Redis(_) => Err("Backup is not supported on Redis".to_string()),
        ActiveConnection::Dynamodb(_) | ActiveConnection::Doc(_) => Err("Backup is not supported on DynamoDB".to_string()),
        ActiveConnection::Duckdb(h) => import_duckdb(&rep, &h, &script).await,
        ActiveConnection::Mssql(h) => import_mssql(&rep, &h, &script).await,
        ActiveConnection::Oracle(_) => Err("Backup import is not supported for Oracle yet".to_string()),
    }
}

async fn read_script(rep: &Reporter, path: &str) -> Result<String, String> {
    let bytes = tokio::fs::read(path).await.map_err(|e| format!("Could not read {path}: {e}"))?;
    match String::from_utf8(bytes) {
        Ok(s) => Ok(s),
        Err(e) => {
            rep.log("warn", "The file is not valid UTF-8; invalid bytes were replaced");
            Ok(String::from_utf8_lossy(e.as_bytes()).into_owned())
        }
    }
}

/// Size and origin of a dump file, read from its first few KB, so the restore
/// screen can name the engine and warn about a cancelled (incomplete) export.
#[tauri::command]
pub async fn backup_inspect_file(path: String) -> Result<BackupFileInfo, String> {
    let meta = tokio::fs::metadata(&path).await.map_err(|e| format!("Could not open {path}: {e}"))?;
    let mut head = vec![0u8; 4096];
    let n = {
        let mut f = File::open(&path).map_err(|e| format!("Could not open {path}: {e}"))?;
        f.read(&mut head).map_err(|e| format!("Could not read {path}: {e}"))?
    };
    head.truncate(n);
    Ok(inspect_head(meta.len(), &String::from_utf8_lossy(&head)))
}

fn inspect_head(size: u64, head: &str) -> BackupFileInfo {
    let incomplete = head.starts_with("-- !! INCOMPLETE BACKUP");
    let stroke = head.contains("-- Stroke Backup");
    let engine = head
        .lines()
        .find_map(|l| l.strip_prefix("-- Engine").map(|r| r.trim_start_matches([' ', ':']).trim().to_string()))
        .filter(|s| !s.is_empty())
        .or_else(|| {
            if head.contains("PostgreSQL database dump") { Some("PostgreSQL".into()) }
            else if head.contains("-- MySQL dump") || head.contains("-- MariaDB dump") { Some("MySQL".into()) }
            else if head.starts_with("PRAGMA foreign_keys=OFF;\nBEGIN TRANSACTION;") { Some("SQLite".into()) }
            else { None }
        });
    BackupFileInfo { size, engine, stroke, incomplete }
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

/// The first `n` characters of a statement, trimmed and lower-cased: enough to
/// classify it without copying a multi-megabyte INSERT.
fn head_lower(stmt: &str, n: usize) -> String {
    truncate_chars(stmt.trim_start(), n).to_ascii_lowercase()
}

fn is_insert(stmt: &str) -> bool {
    stmt.trim_start().get(..6).is_some_and(|h| h.eq_ignore_ascii_case("insert"))
}

/// Transaction control in a dump. The importers run the whole restore in their
/// own transaction, so a script's BEGIN/COMMIT would otherwise end it early.
fn is_tx_control(stmt: &str) -> bool {
    if stmt.len() > 40 { return false; }
    let words: Vec<String> = stmt.split_whitespace().map(|w| w.to_ascii_lowercase()).collect();
    let w: Vec<&str> = words.iter().map(String::as_str).collect();
    matches!(
        w.as_slice(),
        ["begin"] | ["begin", "transaction"] | ["begin", "work"] | ["begin", "deferred" | "immediate" | "exclusive", ..]
            | ["start", "transaction"] | ["commit"] | ["commit", "transaction" | "work"]
            | ["end"] | ["end", "transaction"]
    )
}

/// Build and log the result of a finished restore.
///
/// `total` is how many statements the script held; anything neither applied nor
/// failed was never attempted, which only happens when the user cancelled.
fn finish_import(rep: &Reporter, total: usize, ok: usize, errors: Vec<String>, started: Instant) -> ImportResult {
    let cancelled = is_cancelled();
    let failed = errors.len();
    let skipped = total.saturating_sub(ok + failed);
    let secs = started.elapsed().as_secs_f64();
    let msg = if cancelled {
        format!("Restore cancelled: {ok} applied, {failed} failed, {skipped} not run")
    } else {
        format!("Restore complete: {ok} ok, {failed} failed in {secs:.1}s")
    };
    let level = if cancelled || failed > 0 { "warn" } else { "ok" };
    rep.log(level, &msg);
    ImportResult {
        statements_ok: ok, statements_err: failed, statements_skipped: skipped, cancelled, errors,
        elapsed_ms: started.elapsed().as_millis() as u64,
    }
}

/// Throttled `restore-progress` sender.
struct Meter<'a> {
    rep: &'a Reporter,
    total: u64,
    last: Instant,
}

impl<'a> Meter<'a> {
    fn new(rep: &'a Reporter, total: usize) -> Self {
        Self { rep, total: total as u64, last: Instant::now() - PROGRESS_EVERY }
    }
    fn tick(&mut self, done: usize, ok: usize, failed: usize, force: bool) {
        if !force && self.last.elapsed() < PROGRESS_EVERY { return; }
        self.last = Instant::now();
        self.rep.progress(RestoreProgress { bytes_done: done as u64, bytes_total: self.total, ok, failed });
    }
}

fn stmt_error(e: impl std::fmt::Display, stmt: &str) -> String {
    format!("{e} - near: {}…", truncate_chars(stmt.trim(), 80))
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
    /// escape, alongside `''`. This is what our MySQL exporter emits.
    Backslash,
}

/// One unit of a restore script.
#[derive(Debug)]
enum Stmt<'a> {
    Sql(String),
    /// `COPY … FROM stdin` and the inline data block that follows it, up to
    /// the `\.` line (pg_dump's plain format, and ours).
    Copy { sql: String, data: &'a str },
}

/// Lazily splits a SQL script into statements.
///
/// Beyond simple `;` splitting this understands the constructs our exporters
/// (and pg_dump / mysqldump / sqlite3) emit, so bodies containing embedded
/// semicolons survive a round-trip:
///   - single-quoted strings (`'…''…'`, plus `\'` when `esc` is `Backslash`)
///   - double-quoted (`"…"`) and backtick (`` `…` ``) identifiers
///   - PostgreSQL dollar-quoted strings (`$$ … $$`, `$tag$ … $tag$`)
///   - line comments (`-- …`) and block comments (`/* … */`, nesting like
///     Postgres) are stripped; mysqldump's executable comments
///     (`/*!40000 … */`) go with them, which only skips tuning hints
///   - MySQL `DELIMITER` directives (change the active terminator, e.g. `//`)
///   - Postgres `COPY … FROM stdin;` data blocks, and psql `\` meta-command
///     lines (`\connect`, `\restrict`), which are skipped
///
/// It scans bytes (every delimiter is ASCII, so slicing stays on UTF-8
/// boundaries) and copies whole runs at once instead of character by character,
/// and it yields one statement at a time so a large restore never holds a
/// second copy of the script.
struct Splitter<'a> {
    src: &'a str,
    pos: usize,
    delim: Vec<u8>,
    esc: Escapes,
    /// Only whitespace since the last newline (for DELIMITER and `\` lines).
    line_start: bool,
}

impl<'a> Splitter<'a> {
    fn new(src: &'a str, esc: Escapes) -> Self {
        Self { src, pos: 0, delim: vec![b';'], esc, line_start: true }
    }

    /// Bytes consumed so far, for progress.
    fn position(&self) -> usize { self.pos }

    /// The data block after a `COPY … FROM stdin;` statement: from the next line
    /// up to the `\.` terminator line. Moves `pos` past the terminator.
    fn take_copy_data(&mut self) -> &'a str {
        let src = self.src;
        let b = src.as_bytes();
        let n = b.len();
        // The data starts on the line after the statement's terminator.
        let mut start = self.pos;
        while start < n && b[start] != b'\n' { start += 1; }
        if start < n { start += 1; }
        let is_end = |at: usize| {
            src[at..].starts_with("\\.") && matches!(b.get(at + 2), None | Some(b'\n') | Some(b'\r'))
        };
        let end = if is_end(start) {
            start
        } else {
            let mut from = start;
            loop {
                match src[from..].find("\n\\.") {
                    Some(off) => {
                        let at = from + off + 1;
                        if is_end(at) { break at; }
                        from = at;
                    }
                    None => break n,
                }
            }
        };
        let mut after = end;
        while after < n && b[after] != b'\n' { after += 1; }
        if after < n { after += 1; }
        self.pos = after;
        self.line_start = true;
        &src[start..end]
    }
}

fn is_copy_from_stdin(stmt: &str) -> bool {
    stmt.len() < 64 * 1024
        && stmt.get(..4).is_some_and(|h| h.eq_ignore_ascii_case("copy"))
        && stmt.to_ascii_lowercase().contains("from stdin")
}

/// A SQLite `CREATE TRIGGER … BEGIN … END` holds `;`-terminated statements and
/// only ends at the `;` after its END. True while that END is still to come
/// (BEGIN/CASE open a level, END closes one; quoted text is skipped).
fn trigger_body_open(stmt: &str) -> bool {
    let head = head_lower(stmt, 40);
    let head = head.split_whitespace().collect::<Vec<_>>();
    let is_trigger = matches!(head.as_slice(), ["create", "trigger", ..] | ["create", "temp" | "temporary", "trigger", ..]);
    if !is_trigger { return false; }
    let b = stmt.as_bytes();
    let (mut i, mut depth) = (0usize, 0i32);
    while i < b.len() {
        match b[i] {
            q @ (b'\'' | b'"' | b'`' | b'[') => {
                let close = if q == b'[' { b']' } else { q };
                i += 1;
                while i < b.len() && b[i] != close { i += 1; }
                i += 1;
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let s = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') { i += 1; }
                let w = &stmt[s..i];
                if w.eq_ignore_ascii_case("begin") || w.eq_ignore_ascii_case("case") { depth += 1; }
                else if w.eq_ignore_ascii_case("end") { depth -= 1; }
            }
            _ => i += 1,
        }
    }
    depth > 0
}

impl<'a> Iterator for Splitter<'a> {
    type Item = Stmt<'a>;

    fn next(&mut self) -> Option<Stmt<'a>> {
        let src = self.src;
        let b = src.as_bytes();
        let n = b.len();
        let mut cur = String::new();
        let mut seg = self.pos;
        let mut i = self.pos;

        loop {
            if i >= n {
                cur.push_str(&src[seg..n]);
                self.pos = n;
                let s = cur.trim();
                return if s.is_empty() { None } else { Some(Stmt::Sql(s.to_string())) };
            }
            let c = b[i];

            if self.line_start {
                // MySQL DELIMITER directive (line-level, not inside a body).
                if (c == b'D' || c == b'd')
                    && src.get(i..i + 9).is_some_and(|w| w.eq_ignore_ascii_case("DELIMITER"))
                    && b.get(i + 9).is_some_and(|c| c.is_ascii_whitespace())
                {
                    cur.push_str(&src[seg..i]);
                    i += 9;
                    while i < n && (b[i] == b' ' || b[i] == b'\t') { i += 1; }
                    let ds = i;
                    while i < n && !b[i].is_ascii_whitespace() { i += 1; }
                    if i > ds { self.delim = b[ds..i].to_vec(); }
                    while i < n && b[i] != b'\n' { i += 1; }
                    if i < n { i += 1; }
                    seg = i;
                    continue;
                }
                // psql meta-command line (`\connect db`, `\restrict key`).
                if c == b'\\' && self.esc == Escapes::Standard {
                    cur.push_str(&src[seg..i]);
                    while i < n && b[i] != b'\n' { i += 1; }
                    seg = i;
                    continue;
                }
            }

            // Statement terminator (the active delimiter).
            if b[i..].starts_with(&self.delim) {
                cur.push_str(&src[seg..i]);
                i += self.delim.len();
                self.pos = i;
                self.line_start = false;
                let s = cur.trim();
                if s.is_empty() {
                    cur.clear();
                    seg = i;
                    continue;
                }
                if trigger_body_open(s) {
                    // `;` inside a SQLite trigger body: keep reading.
                    cur.push_str(&src[i - self.delim.len()..i]);
                    seg = i;
                    continue;
                }
                let s = s.to_string();
                if self.esc == Escapes::Standard && is_copy_from_stdin(&s) {
                    let data = self.take_copy_data();
                    return Some(Stmt::Copy { sql: s, data });
                }
                return Some(Stmt::Sql(s));
            }

            match c {
                b'\'' => {
                    i += 1;
                    while i < n {
                        let c = b[i];
                        // A backslash consumes whatever follows it, so an escaped
                        // quote can't be mistaken for the end of the literal.
                        if self.esc == Escapes::Backslash && c == b'\\' { i += 2; continue; }
                        if c == b'\'' {
                            if b.get(i + 1) == Some(&b'\'') { i += 2; continue; }
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                    self.line_start = false;
                }
                b'"' => {
                    i += 1;
                    while i < n {
                        if b[i] == b'"' {
                            if b.get(i + 1) == Some(&b'"') { i += 2; continue; }
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                    self.line_start = false;
                }
                b'`' => {
                    i += 1;
                    while i < n {
                        if b[i] == b'`' { i += 1; break; }
                        i += 1;
                    }
                    self.line_start = false;
                }
                // Dollar-quoted string ($tag$ … $tag$). Only a real tag opens one,
                // and not in the middle of an identifier (`a$b$c`); a bare `$`
                // (or `$1` param) is an ordinary character.
                b'$' => {
                    let ident_before = i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_' || b[i - 1] >= 0x80);
                    let mut j = i + 1;
                    while j < n && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] >= 0x80) { j += 1; }
                    if !ident_before && j < n && b[j] == b'$' {
                        let tag = &src[i..=j];
                        i = j + 1;
                        match src[i..].find(tag) {
                            Some(off) => i += off + tag.len(),
                            None => i = n,
                        }
                    } else {
                        i += 1;
                    }
                    self.line_start = false;
                }
                // Line comment - stripped (replaced by a newline separator).
                b'-' if b.get(i + 1) == Some(&b'-') => {
                    cur.push_str(&src[seg..i]);
                    while i < n && b[i] != b'\n' { i += 1; }
                    cur.push('\n');
                    seg = i;
                }
                // Block comment - stripped. Its contents are not SQL, so a `;` in
                // there must not end the statement. Postgres nests these; the depth
                // counter is harmless for the dialects that don't.
                b'/' if b.get(i + 1) == Some(&b'*') => {
                    cur.push_str(&src[seg..i]);
                    let mut depth = 1usize;
                    i += 2;
                    while i < n && depth > 0 {
                        if b[i] == b'/' && b.get(i + 1) == Some(&b'*') { depth += 1; i += 2; }
                        else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') { depth -= 1; i += 2; }
                        else { i += 1; }
                    }
                    cur.push(' ');
                    seg = i.min(n);
                    self.line_start = false;
                }
                b'\n' => { self.line_start = true; i += 1; }
                b' ' | b'\t' | b'\r' => { i += 1; }
                _ => { self.line_start = false; i += 1; }
            }
        }
    }
}

/// Split a whole script into statements (COPY blocks give their statement).
#[cfg(test)]
fn split_statements(sql: &str, esc: Escapes) -> Vec<String> {
    Splitter::new(sql, esc)
        .map(|s| match s { Stmt::Sql(s) => s, Stmt::Copy { sql, .. } => sql })
        .collect()
}

// ── SQLite export ─────────────────────────────────────────────────────────────

async fn export_sqlite(
    rep: &Reporter,
    pool: &sqlx::SqlitePool,
    filter: Option<&[String]>,
    opts: &ExportOptions,
    out: &mut Out,
) -> Result<Stats, String> {
    rep.log("info", "Starting SQLite export…");
    out.push_str(&backup_header("SQLite", None));
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

    let mut tables: Vec<String> = Vec::new();
    // Indexes, views and triggers go after the data: building an index once is
    // cheaper than maintaining it row by row, and a trigger created before the
    // rows would fire for every one of them on restore.
    let mut after: Vec<String> = Vec::new();
    for row in &ddl_rows {
        let obj_type: String = row.try_get(0).unwrap_or_default();
        let name: String = row.try_get(1).unwrap_or_default();
        let tbl_name: String = row.try_get(2).unwrap_or_default();
        let sql: String = row.try_get(3).unwrap_or_default();
        let stmt = format!("{};\n", sql.trim());
        match obj_type.as_str() {
            "table" => {
                if in_filter(&name) {
                    tables.push(name);
                    if opts.include_schema { out.push_str(&stmt); }
                }
            }
            "view" => if opts.include_views { after.push(stmt) },
            "trigger" => if opts.include_triggers && in_filter(&tbl_name) { after.push(stmt) },
            _ => if opts.include_schema && in_filter(&tbl_name) { after.push(stmt) },
        }
    }

    rep.plan(tables.iter().map(|t| PlanEntry { schema: None, name: t.clone(), est_rows: None }).collect());

    let mut stats = Stats { tables: tables.len(), ..Stats::default() };
    if opts.include_data {
        for table in &tables {
            if is_cancelled() { break; }
            rep.table(None, table, "running", 0, None);
            match sqlite_dump_rows(rep, pool, table).await {
                Ok((chunk, n)) => {
                    out.append(chunk)?;
                    stats.rows += n as usize;
                    rep.table(None, table, "done", n, None);
                }
                Err(e) if is_cancelled() => { let _ = e; break; }
                Err(e) => {
                    out.push_str(&format!("-- ERROR exporting {table}: {e}\n"));
                    rep.table(None, table, "error", 0, Some(e.clone()));
                    rep.log("error", format!("{table}: {e}"));
                    stats.failed += 1;
                }
            }
        }
    } else {
        for t in &tables { rep.table(None, t, "done", 0, None); }
    }

    out.push('\n');
    for s in &after { out.push_str(s); }
    out.push_str("\nCOMMIT;\nPRAGMA foreign_keys=ON;\n");
    out.check()?;
    Ok(stats)
}

/// One SQLite table's rows as multi-row `INSERT OR REPLACE` statements.
async fn sqlite_dump_rows(rep: &Reporter, pool: &sqlx::SqlitePool, table: &str) -> Result<(Chunk, u64), String> {
    let mut chunk = Chunk::new();
    let col_names: Vec<String> = sqlx::query_scalar::<_, String>(
        &format!("SELECT name FROM pragma_table_info({})", ql(table))
    ).fetch_all(pool).await.map_err(|e| e.to_string())?;
    if col_names.is_empty() { return Ok((chunk, 0)); }

    // `quote()` is SQLite's own value-to-literal function: it returns X'…' for
    // a blob, an escaped 'string' for text, a bare number, or the four
    // characters NULL. Letting the engine do this removes the guesswork a
    // Rust-side decoder would need - and with it the chance of a value it
    // doesn't recognise quietly becoming NULL.
    let select_list = col_names.iter().map(|c| format!("quote({})", qi(c))).collect::<Vec<_>>().join(", ");
    let col_list = col_names.iter().map(|c| qi(c)).collect::<Vec<_>>().join(", ");
    let q = format!("SELECT {select_list} FROM {}", qi(table));

    let mut batch = InsertBatcher::new(
        format!("\nINSERT OR REPLACE INTO {} ({col_list}) VALUES", qi(table)),
        "",
        SQLITE_BATCH_BYTES,
    );
    let mut stream = sqlx::query(&q).fetch(pool);
    let mut n = 0u64;
    let mut last = Instant::now();
    while let Some(row) = stream.try_next().await.map_err(|e| e.to_string())? {
        n += 1;
        let buf = batch.row();
        for i in 0..col_names.len() {
            let lit: Option<&str> = row.try_get(i).map_err(|e| {
                format!("Could not read {table}.\"{}\" at row {n}: {e}", col_names[i])
            })?;
            if i > 0 { buf.push_str(", "); }
            buf.push_str(lit.unwrap_or("NULL"));
        }
        batch.end_row(&mut chunk);
        if n % 1024 == 0 {
            if is_cancelled() { return Err("Cancelled".into()); }
            if last.elapsed() >= PROGRESS_EVERY {
                rep.table(None, table, "running", n, None);
                last = Instant::now();
            }
        }
    }
    batch.flush(&mut chunk);
    Ok((chunk, n))
}

// ── SQLite import ─────────────────────────────────────────────────────────────

async fn import_sqlite(rep: &Reporter, pool: &sqlx::SqlitePool, sql: &str) -> Result<ImportResult, String> {
    let started = Instant::now();
    rep.log("info", "Starting restore…");
    let mut conn = pool.acquire().await.map_err(|e| e.to_string())?;
    // FK enforcement off for the load: SQLite checks each row as it lands, so
    // a child table that sorts before its parent would fail row by row. It has
    // to be set outside a transaction, so before the BEGIN below.
    (&mut *conn).execute("PRAGMA foreign_keys = OFF").await.ok();
    (&mut *conn).execute("BEGIN").await.map_err(|e| e.to_string())?;

    let mut run = Runner::new(rep, sql.len());
    let mut sp = Splitter::new(sql, Escapes::Standard);
    while let Some(stmt) = sp.next() {
        let Stmt::Sql(s) = stmt else { run.total += 1; run.errors.push("COPY blocks are Postgres-only".into()); continue };
        run.total += 1;
        let low = head_lower(&s, 24);
        if low.starts_with("pragma foreign_keys") || is_tx_control(&s) { run.ok += 1; continue; }
        if is_insert(&s) {
            run.queue(s);
            if run.batch_full() { run.flush_lite(&mut *conn).await; }
        } else {
            run.flush_lite(&mut *conn).await;
            run.single_lite(&mut *conn, &s).await;
        }
        run.meter.tick(sp.position(), run.ok, run.errors.len(), false);
        if is_cancelled() { break; }
    }
    if !is_cancelled() { run.flush_lite(&mut *conn).await; }

    let res = if is_cancelled() {
        run.total += sp.count();
        (&mut *conn).execute("ROLLBACK").await.ok();
        rep.log("warn", "Cancelled - rolled back, the database is unchanged");
        run.ok = 0;
        Ok(())
    } else {
        (&mut *conn).execute("COMMIT").await.map(|_| ()).map_err(|e| e.to_string())
    };
    (&mut *conn).execute("PRAGMA foreign_keys = ON").await.ok();
    res?;
    run.meter.tick(sql.len(), run.ok, run.errors.len(), true);
    Ok(finish_import(rep, run.total, run.ok, run.errors, started))
}

/// Shared restore loop state: counts, errors, and the pending INSERT batch.
struct Runner<'a> {
    meter: Meter<'a>,
    total: usize,
    ok: usize,
    errors: Vec<String>,
    batch: Vec<String>,
    batch_bytes: usize,
}

impl<'a> Runner<'a> {
    fn new(rep: &'a Reporter, len: usize) -> Self {
        Self { meter: Meter::new(rep, len), total: 0, ok: 0, errors: Vec::new(), batch: Vec::new(), batch_bytes: 0 }
    }

    fn queue(&mut self, s: String) {
        self.batch_bytes += s.len();
        self.batch.push(s);
    }

    fn batch_full(&self) -> bool {
        self.batch_bytes >= RESTORE_BATCH_BYTES || self.batch.len() >= RESTORE_BATCH_STMTS
    }
}

/// `single_*` runs one statement inside a savepoint, so a failure undoes only
/// itself and the surrounding transaction carries on (Postgres would otherwise
/// abort it; SQLite and MySQL just need the partial work undone).
///
/// `flush_*` sends the queued INSERTs in one round trip. If any of them fails
/// the whole batch is rolled back and replayed one by one, so the error lands
/// on the statement that caused it and every other row still goes in.
///
/// One copy per connection type: a generic `Executor` bound here makes the
/// command futures fail tauri's `Send` check.
macro_rules! runner_exec {
    ($single:ident, $flush:ident, $conn:ty) => {
        impl<'a> Runner<'a> {
            async fn $single(&mut self, conn: &mut $conn, stmt: &str) {
                let sql = format!("SAVEPOINT stroke_sp;\n{stmt};\nRELEASE SAVEPOINT stroke_sp");
                match (&mut *conn).execute(sql.as_str()).await {
                    Ok(_) => self.ok += 1,
                    Err(e) => {
                        (&mut *conn).execute("ROLLBACK TO SAVEPOINT stroke_sp; RELEASE SAVEPOINT stroke_sp").await.ok();
                        self.errors.push(stmt_error(e, stmt));
                    }
                }
            }

            async fn $flush(&mut self, conn: &mut $conn) {
                if self.batch.is_empty() { return; }
                let batch = std::mem::take(&mut self.batch);
                self.batch_bytes = 0;
                // A failing batch is split in halves until the bad statement is
                // alone, instead of replaying all of it one by one: one bad row in
                // 1,000 costs ~20 round trips, not 1,000 (painful on a remote
                // server). Order is kept: the first half always runs first.
                let mut stack: Vec<Vec<String>> = vec![batch];
                while let Some(chunk) = stack.pop() {
                    if is_cancelled() { break; }
                    if chunk.len() == 1 {
                        self.$single(conn, &chunk[0]).await;
                        continue;
                    }
                    let mut sql = String::with_capacity(chunk.iter().map(|s| s.len() + 2).sum::<usize>() + 64);
                    sql.push_str("SAVEPOINT stroke_sp;\n");
                    for s in &chunk { sql.push_str(s); sql.push_str(";\n"); }
                    sql.push_str("RELEASE SAVEPOINT stroke_sp");
                    match (&mut *conn).execute(sql.as_str()).await {
                        Ok(_) => self.ok += chunk.len(),
                        Err(_) => {
                            (&mut *conn).execute("ROLLBACK TO SAVEPOINT stroke_sp; RELEASE SAVEPOINT stroke_sp").await.ok();
                            let mut first = chunk;
                            let second = first.split_off(first.len() / 2);
                            stack.push(second);
                            stack.push(first);
                        }
                    }
                }
            }
        }
    };
}

runner_exec!(single_pg, flush_pg, sqlx::PgConnection);
runner_exec!(single_my, flush_my, sqlx::MySqlConnection);
runner_exec!(single_lite, flush_lite, sqlx::SqliteConnection);

// ── PostgreSQL export ─────────────────────────────────────────────────────────

/// Unwrap a secondary-object catalog query for the Postgres export. On failure
/// (permission error, catalog incompatibility) the export continues without
/// those objects, but says so - both in the log and as a WARNING line in the
/// dump - instead of silently producing an incomplete backup.
fn catalog_or_warn<T>(res: Result<Vec<T>, sqlx::Error>, rep: &Reporter, out: &mut Out, kind: &str) -> Vec<T> {
    match res {
        Ok(v) => v,
        Err(e) => {
            rep.log("warn", format!("Could not export {kind}: {e}"));
            out.push_str(&format!("-- WARNING: could not export {kind}: {e}\n\n"));
            Vec::new()
        }
    }
}

#[derive(Debug, Clone)]
struct PgCol {
    name: String,
    typ: String,
    nullable: bool,
    default: Option<String>,
    identity: bool,
    generated: bool,
    base: String,
}

#[derive(Debug, Clone)]
struct PgTable {
    schema: String,
    name: String,
    est_rows: Option<i64>,
    cols: Vec<PgCol>,
    pk: Vec<String>,
}

/// Every foreign key in the given schemas as a ready-to-run ALTER TABLE.
///
/// This used to be written with `\` line continuations, which in Rust also eat
/// the next line's leading whitespace: `… c\` + `JOIN …` became `cJOIN`, every
/// backup failed this query with `syntax error at or near "pg_catalog"`, and
/// restores came back with no foreign keys at all. A raw string keeps the
/// newlines.
const PG_FK_SQL: &str = r#"
    SELECT n.nspname::text, t.relname::text,
           format('ALTER TABLE %I.%I ADD CONSTRAINT %I %s',
                  n.nspname, t.relname, c.conname, pg_get_constraintdef(c.oid))
    FROM pg_catalog.pg_constraint c
    JOIN pg_catalog.pg_class t ON t.oid = c.conrelid
    JOIN pg_catalog.pg_namespace n ON n.oid = t.relnamespace
    WHERE c.contype = 'f' AND n.nspname = ANY($1)
    ORDER BY n.nspname, t.relname, c.conname
"#;

async fn export_postgres(
    rep: &Reporter,
    pool: &sqlx::PgPool,
    schema_filter: Option<&str>,
    table_filter: Option<&[String]>,
    opts: &ExportOptions,
    out: &mut Out,
) -> Result<Stats, String> {
    let schemas: Vec<String> = if let Some(s) = schema_filter {
        vec![s.to_string()]
    } else {
        sqlx::query_scalar(
            "SELECT nspname::text FROM pg_catalog.pg_namespace \
             WHERE nspname NOT IN ('pg_catalog','information_schema','pg_toast') \
               AND nspname NOT LIKE 'pg_temp_%' AND nspname NOT LIKE 'pg_toast_temp_%' \
             ORDER BY nspname"
        )
        .fetch_all(pool).await.map_err(|e| e.to_string())?
    };

    rep.log("info", format!("Starting PostgreSQL export ({} schema{})…", schemas.len(), if schemas.len() == 1 { "" } else { "s" }));

    out.push_str(&backup_header("PostgreSQL", schema_filter));
    out.push_str("SET client_encoding = 'UTF8';\n");
    out.push_str("SET standard_conforming_strings = on;\n");
    // Function bodies are validated against tables that may not exist yet at
    // that point of the script; pg_dump turns this off for the same reason.
    out.push_str("SET check_function_bodies = false;\n\n");

    for s in &schemas {
        // `public` exists everywhere, and creating it needs CREATE on the
        // database, which most roles lack since PG 15.
        if s != "public" {
            out.push_str(&format!("CREATE SCHEMA IF NOT EXISTS {};\n", qi(s)));
        }
    }
    out.push('\n');

    // ── Catalog: one query per object kind for every schema at once. Per-table
    // lookups cost three round trips a table, which on a far host is seconds.
    let tables = pg_load_tables(pool, &schemas, table_filter).await?;
    let exported: std::collections::HashSet<(String, String)> =
        tables.iter().map(|t| (t.schema.clone(), t.name.clone())).collect();

    // ── Enums ──
    if opts.include_enums {
        let res = sqlx::query_as::<_, (String, String, Vec<String>)>(
            "SELECT n.nspname::text, t.typname::text, \
                    array_agg(e.enumlabel::text ORDER BY e.enumsortorder) \
             FROM pg_catalog.pg_type t \
             JOIN pg_catalog.pg_enum e ON e.enumtypid = t.oid \
             JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace \
             WHERE n.nspname = ANY($1) \
             GROUP BY n.nspname, t.typname ORDER BY 1, 2"
        )
        .bind(&schemas).fetch_all(pool).await;
        let enums = catalog_or_warn(res, rep, out, "enum types");
        if !enums.is_empty() {
            out.push_str("-- Custom enum types\n");
            for (schema, name, labels) in &enums {
                let quoted: Vec<String> = labels.iter().map(|l| ql(l)).collect();
                out.push_str(&format!(
                    "DO $$ BEGIN\n  CREATE TYPE {} AS ENUM ({});\nEXCEPTION WHEN duplicate_object THEN NULL;\nEND $$;\n",
                    qn(schema, name), quoted.join(", ")
                ));
            }
            out.push('\n');
        }
    }

    // ── Sequences ── (created before the tables whose defaults call them)
    let mut setvals: Vec<String> = Vec::new();
    if opts.include_sequences {
        let res = sqlx::query(
            r#"
            SELECT s.schemaname::text, s.sequencename::text, s.data_type::text,
                   s.start_value, s.min_value, s.max_value, s.increment_by, s.cycle, s.last_value,
                   dn.nspname::text AS ident_schema, dt.relname::text AS ident_table, da.attname::text AS ident_col
            FROM pg_catalog.pg_sequences s
            JOIN pg_catalog.pg_namespace sn ON sn.nspname = s.schemaname
            JOIN pg_catalog.pg_class sc ON sc.relnamespace = sn.oid AND sc.relname = s.sequencename
            LEFT JOIN pg_catalog.pg_depend d
                   ON d.classid = 'pg_catalog.pg_class'::regclass AND d.objid = sc.oid
                  AND d.refclassid = 'pg_catalog.pg_class'::regclass AND d.deptype = 'i'
            LEFT JOIN pg_catalog.pg_class dt ON dt.oid = d.refobjid
            LEFT JOIN pg_catalog.pg_namespace dn ON dn.oid = dt.relnamespace
            LEFT JOIN pg_catalog.pg_attribute da ON da.attrelid = d.refobjid AND da.attnum = d.refobjsubid
            WHERE s.schemaname = ANY($1)
            ORDER BY 1, 2
            "#,
        )
        .bind(&schemas).fetch_all(pool).await;
        let seqs = catalog_or_warn(res, rep, out, "sequences");
        let mut created = false;
        for r in &seqs {
            let schema: String = r.try_get(0).unwrap_or_default();
            let name: String = r.try_get(1).unwrap_or_default();
            let last: Option<i64> = r.try_get(8).ok().flatten();
            let ident_table: Option<String> = r.try_get(10).ok().flatten();
            if let Some(it) = ident_table {
                // An identity column's sequence comes with its table (under a
                // name the target picks), so it is only fast-forwarded.
                let is_schema: String = r.try_get(9).unwrap_or_default();
                let col: String = r.try_get(11).unwrap_or_default();
                if let Some(v) = last {
                    if exported.contains(&(is_schema.clone(), it.clone())) {
                        setvals.push(format!(
                            "SELECT pg_catalog.setval(pg_catalog.pg_get_serial_sequence({}, {}), {v}, true);\n",
                            ql(&qn(&is_schema, &it)), ql(&col)
                        ));
                    }
                }
                continue;
            }
            if !created { out.push_str("-- Sequences\n"); created = true; }
            let dtype: String = r.try_get(2).unwrap_or_else(|_| "bigint".into());
            let start: i64 = r.try_get(3).unwrap_or(1);
            let min: i64 = r.try_get(4).unwrap_or(1);
            let max: i64 = r.try_get(5).unwrap_or(i64::MAX);
            let inc: i64 = r.try_get(6).unwrap_or(1);
            let cycle: bool = r.try_get(7).unwrap_or(false);
            let cycle_str = if cycle { "CYCLE" } else { "NO CYCLE" };
            out.push_str(&format!(
                "CREATE SEQUENCE IF NOT EXISTS {}\n    AS {dtype}\n    START WITH {start}\n    INCREMENT BY {inc}\n    MINVALUE {min}\n    MAXVALUE {max}\n    {cycle_str};\n",
                qn(&schema, &name)
            ));
            if let Some(v) = last {
                setvals.push(format!("SELECT pg_catalog.setval({}, {v}, true);\n", ql(&qn(&schema, &name))));
            }
        }
        if created { out.push('\n'); }
    }

    // ── Tables ──
    if opts.include_schema {
        for t in &tables { out.push_str(&pg_create_table(t)); }
        out.push('\n');
    }

    rep.plan(tables.iter().map(|t| PlanEntry {
        schema: Some(t.schema.clone()), name: t.name.clone(), est_rows: t.est_rows,
    }).collect());

    // ── Data ──
    let mut stats = Stats { tables: tables.len(), ..Stats::default() };
    if opts.include_data && !tables.is_empty() {
        pg_export_data(rep, pool, &tables, out, &mut stats).await?;
        if !setvals.is_empty() && !is_cancelled() {
            out.push_str("-- Sequence values\n");
            for s in &setvals { out.push_str(s); }
            out.push('\n');
        }
    } else {
        for t in &tables { rep.table(Some(&t.schema), &t.name, "done", 0, None); }
    }
    if is_cancelled() { return Ok(stats); }

    if opts.include_schema && !tables.is_empty() {
        // ── Indexes ── (after the data: one build beats row-by-row maintenance)
        let res = sqlx::query_as::<_, (String, String, String)>(
            "SELECT n.nspname::text, t.relname::text, pg_get_indexdef(i.oid) \
             FROM pg_catalog.pg_index ix \
             JOIN pg_catalog.pg_class i ON i.oid = ix.indexrelid \
             JOIN pg_catalog.pg_class t ON t.oid = ix.indrelid \
             JOIN pg_catalog.pg_namespace n ON n.oid = t.relnamespace \
             WHERE n.nspname = ANY($1) AND NOT ix.indisprimary \
             ORDER BY 1, 2, i.relname"
        )
        .bind(&schemas).fetch_all(pool).await;
        let indexes = catalog_or_warn(res, rep, out, "indexes");
        let mut first = true;
        for (schema, table, def) in &indexes {
            if !exported.contains(&(schema.clone(), table.clone())) { continue; }
            if first { out.push_str("-- Indexes\n"); first = false; }
            let safe = def
                .replacen("CREATE INDEX ", "CREATE INDEX IF NOT EXISTS ", 1)
                .replacen("CREATE UNIQUE INDEX ", "CREATE UNIQUE INDEX IF NOT EXISTS ", 1);
            out.push_str(&safe);
            out.push_str(";\n");
        }
        if !first { out.push('\n'); }

        // ── Foreign keys ── (last, once every table and row is in place)
        let res = sqlx::query_as::<_, (String, String, String)>(PG_FK_SQL)
            .bind(&schemas).fetch_all(pool).await;
        let fks = catalog_or_warn(res, rep, out, "foreign keys");
        let mut first = true;
        for (schema, table, def) in &fks {
            if !exported.contains(&(schema.clone(), table.clone())) { continue; }
            if first { out.push_str("-- Foreign keys\n"); first = false; }
            out.push_str(def);
            out.push_str(";\n");
        }
        if !first { out.push('\n'); }
    }

    // ── Views ──
    if opts.include_views {
        let res = sqlx::query_as::<_, (String, String, String)>(
            "SELECT n.nspname::text, c.relname::text, pg_get_viewdef(c.oid, true) \
             FROM pg_catalog.pg_class c \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = ANY($1) AND c.relkind = 'v' \
             ORDER BY 1, 2"
        )
        .bind(&schemas).fetch_all(pool).await;
        let views = catalog_or_warn(res, rep, out, "views");
        if !views.is_empty() {
            out.push_str("-- Views\n");
            for (schema, name, def) in &views {
                out.push_str(&format!("CREATE OR REPLACE VIEW {} AS\n{};\n", qn(schema, name), def.trim().trim_end_matches(';')));
            }
            out.push('\n');
        }
    }

    // ── Functions & Procedures ──
    if opts.include_functions {
        let res = sqlx::query_as::<_, (String, String)>(
            "SELECT p.proname::text, pg_get_functiondef(p.oid) \
             FROM pg_catalog.pg_proc p \
             JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace \
             WHERE n.nspname = ANY($1) AND p.prokind IN ('f', 'p') \
               AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_depend d \
                               WHERE d.classid = 'pg_catalog.pg_proc'::regclass \
                                 AND d.objid = p.oid AND d.deptype = 'e') \
             ORDER BY n.nspname, p.proname"
        )
        .bind(&schemas).fetch_all(pool).await;
        let funcs = catalog_or_warn(res, rep, out, "functions/procedures");
        if !funcs.is_empty() {
            out.push_str("-- Functions & Procedures\n");
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
        let res = sqlx::query_as::<_, (String, String, String)>(
            "SELECT n.nspname::text, c.relname::text, pg_get_triggerdef(t.oid) \
             FROM pg_catalog.pg_trigger t \
             JOIN pg_catalog.pg_class c ON c.oid = t.tgrelid \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = ANY($1) AND NOT t.tgisinternal \
             ORDER BY 1, 2, t.tgname"
        )
        .bind(&schemas).fetch_all(pool).await;
        let triggers = catalog_or_warn(res, rep, out, "triggers");
        let mut first = true;
        for (schema, table, def) in &triggers {
            if table_filter.is_some() && !exported.contains(&(schema.clone(), table.clone())) { continue; }
            if first { out.push_str("-- Triggers\n"); first = false; }
            out.push_str(def.trim());
            out.push_str(";\n");
        }
        if !first { out.push('\n'); }
    }

    out.check()?;
    Ok(stats)
}

/// Tables (with columns and primary keys) in the given schemas, filtered by name.
async fn pg_load_tables(pool: &sqlx::PgPool, schemas: &[String], filter: Option<&[String]>) -> Result<Vec<PgTable>, String> {
    let rows = sqlx::query(
        "SELECT n.nspname::text, c.relname::text, \
                CASE WHEN c.reltuples < 0 THEN NULL ELSE c.reltuples::bigint END \
         FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = ANY($1) AND c.relkind = 'r' AND NOT c.relispartition \
         ORDER BY 1, 2"
    )
    .bind(schemas).fetch_all(pool).await.map_err(|e| e.to_string())?;

    let mut tables: Vec<PgTable> = rows.iter().filter_map(|r| {
        let schema: String = r.try_get(0).ok()?;
        let name: String = r.try_get(1).ok()?;
        if !filter.map_or(true, |f| f.iter().any(|x| *x == name)) { return None; }
        Some(PgTable { schema, name, est_rows: r.try_get(2).ok().flatten(), cols: Vec::new(), pk: Vec::new() })
    }).collect();
    if tables.is_empty() { return Ok(tables); }

    let index: std::collections::HashMap<(String, String), usize> = tables.iter().enumerate()
        .map(|(i, t)| ((t.schema.clone(), t.name.clone()), i)).collect();

    let col_rows = sqlx::query(r#"
        SELECT
            n.nspname::text,
            c.relname::text,
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
        WHERE n.nspname = ANY($1) AND c.relkind = 'r'
          AND a.attnum > 0 AND NOT a.attisdropped
        ORDER BY n.nspname, c.relname, a.attnum
    "#)
    .bind(schemas).fetch_all(pool).await.map_err(|e| e.to_string())?;
    for r in &col_rows {
        let key = (r.try_get::<String, _>(0).unwrap_or_default(), r.try_get::<String, _>(1).unwrap_or_default());
        let Some(&i) = index.get(&key) else { continue };
        tables[i].cols.push(PgCol {
            name: r.try_get(2).unwrap_or_default(),
            typ: r.try_get(3).unwrap_or_default(),
            nullable: r.try_get(4).unwrap_or(true),
            default: r.try_get(5).ok().flatten(),
            identity: r.try_get(6).unwrap_or(false),
            generated: r.try_get(7).unwrap_or(false),
            base: r.try_get(8).unwrap_or_default(),
        });
    }

    let pk_rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT n.nspname::text, t.relname::text, a.attname::text \
         FROM pg_catalog.pg_constraint c \
         JOIN pg_catalog.pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = ANY(c.conkey) \
         JOIN pg_catalog.pg_class t ON t.oid = c.conrelid \
         JOIN pg_catalog.pg_namespace n ON n.oid = t.relnamespace \
         WHERE c.contype = 'p' AND n.nspname = ANY($1) \
         ORDER BY 1, 2, array_position(c.conkey, a.attnum)"
    )
    .bind(schemas).fetch_all(pool).await.unwrap_or_default();
    for (schema, table, col) in pk_rows {
        if let Some(&i) = index.get(&(schema, table)) { tables[i].pk.push(col); }
    }
    Ok(tables)
}

fn pg_create_table(t: &PgTable) -> String {
    let mut col_defs: Vec<String> = Vec::with_capacity(t.cols.len() + 1);
    for c in &t.cols {
        let mut def = format!("    {} {}", qi(&c.name), c.typ);
        if c.generated {
            // A generated column computes itself. Emitting its expression as a
            // DEFAULT instead would make the column writable and leave the
            // restored table holding whatever the dump inserted.
            if let Some(d) = &c.default { def.push_str(&format!(" GENERATED ALWAYS AS ({d}) STORED")); }
            if !c.nullable { def.push_str(" NOT NULL"); }
        } else {
            if !c.nullable { def.push_str(" NOT NULL"); }
            if c.identity {
                def.push_str(" GENERATED BY DEFAULT AS IDENTITY");
            } else if let Some(d) = &c.default {
                def.push_str(&format!(" DEFAULT {d}"));
            }
        }
        col_defs.push(def);
    }
    if !t.pk.is_empty() {
        let pk = t.pk.iter().map(|c| qi(c)).collect::<Vec<_>>().join(", ");
        col_defs.push(format!("    PRIMARY KEY ({pk})"));
    }
    format!("CREATE TABLE IF NOT EXISTS {} (\n{}\n);\n", qn(&t.schema, &t.name), col_defs.join(",\n"))
}

/// Copy every table's rows into the dump, `PG_PARALLEL` at a time.
///
/// All workers read from one exported snapshot (as `pg_dump -j` does), so the
/// tables are mutually consistent even though they're read on different
/// connections. Results are written in table order whatever order they finish.
async fn pg_export_data(
    rep: &Reporter,
    pool: &sqlx::PgPool,
    tables: &[PgTable],
    out: &mut Out,
    stats: &mut Stats,
) -> Result<(), String> {
    // The leader holds the snapshot open until every worker has started on it.
    let mut leader = None;
    let mut snapshot: Option<String> = None;
    if tables.len() > 1 {
        if let Ok(mut conn) = pool.acquire().await {
            if (&mut *conn).execute("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY").await.is_ok() {
                match sqlx::query_scalar::<_, String>("SELECT pg_catalog.pg_export_snapshot()").fetch_one(&mut *conn).await {
                    Ok(s) if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') => snapshot = Some(s),
                    _ => { (&mut *conn).execute("ROLLBACK").await.ok(); }
                }
            }
            leader = Some(conn);
        }
    }

    let snap = snapshot.as_deref();
    let jobs: Vec<_> = tables.iter().map(|t| Box::pin(pg_copy_table(rep, pool, snap, t))).collect();
    let mut results = futures::stream::iter(jobs).buffered(PG_PARALLEL);
    let mut outcome = Ok(());
    let mut idx = 0usize;
    while let Some(res) = results.next().await {
        let t = &tables[idx];
        idx += 1;
        match res {
            Ok((chunk, rows)) => {
                if let Err(e) = out.append(chunk) { outcome = Err(e); break; }
                stats.rows += rows as usize;
                rep.table(Some(&t.schema), &t.name, "done", rows, None);
            }
            Err(_) if is_cancelled() => break,
            Err(e) => {
                out.push_str(&format!("-- ERROR exporting {}.{}: {e}\n\n", t.schema, t.name));
                rep.table(Some(&t.schema), &t.name, "error", 0, Some(e.clone()));
                rep.log("error", format!("{}.{}: {e}", t.schema, t.name));
                stats.failed += 1;
            }
        }
    }
    drop(results);

    if let Some(mut conn) = leader {
        if snapshot.is_some() { (&mut *conn).execute("COMMIT").await.ok(); }
    }
    outcome
}

/// One table through `COPY … TO STDOUT`, passed through untouched as a
/// pg_dump-style `COPY … FROM stdin` block.
async fn pg_copy_table(
    rep: &Reporter,
    pool: &sqlx::PgPool,
    snapshot: Option<&str>,
    t: &PgTable,
) -> Result<(Chunk, u64), String> {
    let mut chunk = Chunk::new();
    // A generated column rejects an explicit value, so it's left out of the copy.
    let cols: Vec<&PgCol> = t.cols.iter().filter(|c| !c.generated).collect();
    if cols.is_empty() || is_cancelled() {
        return if is_cancelled() { Err("Cancelled".into()) } else { Ok((chunk, 0)) };
    }
    let target = qn(&t.schema, &t.name);
    let col_list = cols.iter().map(|c| qi(&c.name)).collect::<Vec<_>>().join(", ");
    let copy_out = if cols.iter().any(|c| c.base.eq_ignore_ascii_case("money")) {
        // money's text form is locale-formatted ("$1,234.56") and only reads
        // back under the same lc_monetary. numeric is portable.
        let sel = cols.iter().map(|c| {
            if c.base.eq_ignore_ascii_case("money") { format!("{}::numeric", qi(&c.name)) } else { qi(&c.name) }
        }).collect::<Vec<_>>().join(", ");
        format!("COPY (SELECT {sel} FROM {target}) TO STDOUT")
    } else {
        format!("COPY {target} ({col_list}) TO STDOUT")
    };

    rep.table(Some(&t.schema), &t.name, "running", 0, None);
    let mut conn = pool.acquire().await.map_err(|e| e.to_string())?;

    let res: Result<u64, String> = async {
        // SET LOCAL keeps these off the pooled connection afterwards.
        //  - extra_float_digits 3: floats read back bit-identical (pre-PG12)
        //  - ISO dates / postgres intervals: the forms every server parses
        let settings = "SET LOCAL extra_float_digits = 3; SET LOCAL datestyle = 'ISO, YMD'; SET LOCAL intervalstyle = 'postgres'";
        let snapped = match snapshot {
            Some(s) => (&mut *conn).execute(format!(
                "BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY; SET TRANSACTION SNAPSHOT '{s}'; {settings}"
            ).as_str()).await.is_ok(),
            None => false,
        };
        if !snapped {
            if snapshot.is_some() { (&mut *conn).execute("ROLLBACK").await.ok(); }
            (&mut *conn).execute(format!("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY; {settings}").as_str()).await.map_err(|e| e.to_string())?;
        }

        chunk.put_str(&format!("COPY {target} ({col_list}) FROM stdin;\n"));
        let mut rows = 0u64;
        let mut last = Instant::now();
        {
            let mut stream = conn.copy_out_raw(&copy_out).await.map_err(|e| e.to_string())?;
            while let Some(buf) = stream.try_next().await.map_err(|e| e.to_string())? {
                rows += count_rows_in(&buf);
                chunk.put(&buf);
                if is_cancelled() { return Err("Cancelled".to_string()); }
                if last.elapsed() >= PROGRESS_EVERY {
                    rep.table(Some(&t.schema), &t.name, "running", rows, None);
                    last = Instant::now();
                }
            }
        }
        (&mut *conn).execute("COMMIT").await.map_err(|e| e.to_string())?;
        chunk.put_str("\\.\n\n");
        Ok(rows)
    }.await;

    match res {
        Ok(rows) => Ok((chunk, rows)),
        Err(e) => {
            // Mid-COPY or mid-transaction: don't hand this back to the pool.
            conn.close_on_drop();
            Err(e)
        }
    }
}

// ── PostgreSQL import ─────────────────────────────────────────────────────────

fn pg_error_code(e: &sqlx::Error) -> Option<String> {
    match e {
        sqlx::Error::Database(d) => d.code().map(|c| c.into_owned()),
        _ => None,
    }
}

/// `COPY target (cols) FROM stdin` → (`target`, `Some("cols")`).
fn parse_copy_target(stmt: &str) -> Option<(String, Option<String>)> {
    let rest = stmt.trim_start().get(4..)?.trim_start();
    let lower = rest.to_ascii_lowercase();
    let from = lower.rfind(" from stdin")?;
    let head = rest[..from].trim();
    match (head.find('('), head.rfind(')')) {
        (Some(o), Some(c)) if c > o => Some((head[..o].trim().to_string(), Some(head[o + 1..c].trim().to_string()))),
        _ => Some((head.to_string(), None)),
    }
}

/// Statements that can go in a multi-statement batch inside the restore's
/// transaction. `low` is the lowercased head of the statement.
fn pg_batchable(low: &str) -> bool {
    let no = ["create database", "drop database", "vacuum", "create index concurrently",
        "drop index concurrently", "reindex", "alter system", "cluster", "alter database"];
    !no.iter().any(|p| low.starts_with(p))
        && !(low.starts_with("create unique index concurrently"))
}

async fn import_postgres(rep: &Reporter, pool: &sqlx::PgPool, sql: &str) -> Result<ImportResult, String> {
    let started = Instant::now();
    rep.log("info", "Starting restore…");
    let mut conn = pool.acquire().await.map_err(|e| e.to_string())?;
    // The dump's SETs (and our own) are session-level; this connection is
    // closed afterwards rather than returned to the pool carrying them.
    conn.close_on_drop();

    (&mut *conn).execute("BEGIN").await.map_err(|e| e.to_string())?;
    // Skip triggers and FK checks during the load when the role may (superuser
    // or granted). Inside a savepoint: a refused SET would otherwise abort the
    // whole transaction and fail every statement after it. Our own dumps add
    // FKs and triggers after the data, so they don't need it.
    if (&mut *conn).execute("SAVEPOINT stroke_rr; SET LOCAL session_replication_role = replica; RELEASE SAVEPOINT stroke_rr").await.is_err()
    {
        (&mut *conn).execute("ROLLBACK TO SAVEPOINT stroke_rr; RELEASE SAVEPOINT stroke_rr").await.ok();
    }

    let mut run = Runner::new(rep, sql.len());
    let mut sp = Splitter::new(sql, Escapes::Standard);
    while let Some(stmt) = sp.next() {
        run.total += 1;
        match stmt {
            Stmt::Sql(s) => {
                let low = head_lower(&s, 40);
                if low.starts_with("set session_replication_role")
                    || low.starts_with("set client_encoding")
                    || low.starts_with("set standard_conforming")
                    || is_tx_control(&s)
                {
                    run.ok += 1;
                    continue;
                }
                // Postgres DDL is transactional, so schema statements batch like
                // INSERTs: one round trip per ~1,000 instead of one each. Only
                // what cannot run inside a transaction goes alone.
                if is_insert(&s) || pg_batchable(&low) {
                    run.queue(s);
                    if run.batch_full() { run.flush_pg(&mut *conn).await; }
                } else {
                    run.flush_pg(&mut *conn).await;
                    run.single_pg(&mut *conn, &s).await;
                }
            }
            Stmt::Copy { sql: copy_sql, data } => {
                run.flush_pg(&mut *conn).await;
                match pg_copy_in(&mut conn, &copy_sql, data).await {
                    Ok(()) => run.ok += 1,
                    Err(e) => run.errors.push(stmt_error(e, &copy_sql)),
                }
            }
        }
        run.meter.tick(sp.position(), run.ok, run.errors.len(), false);
        if is_cancelled() { break; }
    }
    if !is_cancelled() { run.flush_pg(&mut *conn).await; }

    if is_cancelled() {
        run.total += sp.count();
        // Everything so far is inside this transaction, so cancelling can still
        // undo it in full. Committing here would leave the database holding an
        // arbitrary prefix of the dump - the one outcome a cancel must not have.
        (&mut *conn).execute("ROLLBACK").await.map_err(|e| e.to_string())?;
        rep.log("warn", "Cancelled - rolled back, the database is unchanged");
        return Ok(finish_import(rep, run.total, 0, run.errors, started));
    }
    (&mut *conn).execute("COMMIT").await.map_err(|e| e.to_string())?;
    run.meter.tick(sql.len(), run.ok, run.errors.len(), true);
    Ok(finish_import(rep, run.total, run.ok, run.errors, started))
}

/// Feed one COPY block. A table that already holds some of the rows rejects
/// the whole COPY on the first duplicate key; in that case the rows go through
/// a temp table and `ON CONFLICT DO NOTHING`, which is what the old
/// INSERT-per-row dumps did, so restoring over existing data still works.
async fn pg_copy_in(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    stmt: &str,
    data: &str,
) -> Result<(), String> {
    async fn send(conn: &mut sqlx::PgConnection, stmt: &str, data: &str) -> Result<u64, sqlx::Error> {
        let mut copy = conn.copy_in_raw(stmt).await?;
        for piece in data.as_bytes().chunks(1 << 20) {
            if is_cancelled() {
                copy.abort("cancelled").await.ok();
                return Err(sqlx::Error::Protocol("Cancelled".into()));
            }
            copy.send(piece).await?;
        }
        copy.finish().await
    }

    (&mut **conn).execute("SAVEPOINT stroke_sp").await.map_err(|e| e.to_string())?;
    match send(&mut *conn, stmt, data).await {
        Ok(_) => {
            (&mut **conn).execute("RELEASE SAVEPOINT stroke_sp").await.map_err(|e| e.to_string())?;
            Ok(())
        }
        Err(e) => {
            (&mut **conn).execute("ROLLBACK TO SAVEPOINT stroke_sp; RELEASE SAVEPOINT stroke_sp").await.ok();
            if pg_error_code(&e).as_deref() != Some("23505") {
                return Err(e.to_string());
            }
            let Some((target, cols)) = parse_copy_target(stmt) else { return Err(e.to_string()) };
            let (into, select) = match &cols {
                Some(c) => (format!("{target} ({c})"), c.clone()),
                None => (target.clone(), "*".to_string()),
            };
            let tmp_copy = match &cols {
                Some(c) => format!("COPY stroke_restore_tmp ({c}) FROM STDIN"),
                None => "COPY stroke_restore_tmp FROM STDIN".to_string(),
            };
            (&mut **conn).execute(format!(
                "SAVEPOINT stroke_sp; CREATE TEMP TABLE stroke_restore_tmp (LIKE {target})"
            ).as_str()).await.map_err(|e2| e2.to_string())?;
            let merged = async {
                send(&mut *conn, &tmp_copy, data).await?;
                (&mut **conn).execute(format!(
                    "INSERT INTO {into} SELECT {select} FROM stroke_restore_tmp ON CONFLICT DO NOTHING; \
                     DROP TABLE stroke_restore_tmp; RELEASE SAVEPOINT stroke_sp"
                ).as_str()).await
            }.await;
            match merged {
                Ok(_) => Ok(()),
                Err(e2) => {
                    (&mut **conn).execute("ROLLBACK TO SAVEPOINT stroke_sp; RELEASE SAVEPOINT stroke_sp").await.ok();
                    Err(e2.to_string())
                }
            }
        }
    }
}

// ── MySQL export ──────────────────────────────────────────────────────────────

async fn export_mysql(
    rep: &Reporter,
    pool: &sqlx::MySqlPool,
    schema_filter: Option<&str>,
    table_filter: Option<&[String]>,
    opts: &ExportOptions,
    out: &mut Out,
) -> Result<Stats, String> {
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

    rep.log("info", format!("Starting MySQL export ({} schema{})…", schemas.len(), if schemas.len() == 1 { "" } else { "s" }));

    out.push_str(&backup_header("MySQL", schema_filter));
    out.push_str("SET NAMES utf8mb4;\nSET FOREIGN_KEY_CHECKS=0;\nSET UNIQUE_CHECKS=0;\nSET AUTOCOMMIT=0;\n\n");

    // Every table up front, for the progress plan.
    let mut by_schema: Vec<(String, Vec<(String, Option<i64>)>)> = Vec::new();
    for schema in &schemas {
        let rows = sqlx::query(
            "SELECT TABLE_NAME, CAST(TABLE_ROWS AS SIGNED) FROM information_schema.TABLES \
             WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'BASE TABLE' \
             ORDER BY TABLE_NAME"
        )
        .bind(schema).fetch_all(pool).await.map_err(|e| e.to_string())?;
        let list = rows.iter().filter_map(|r| {
            let name = super::mysql::my_text(r, 0)?;
            if !table_filter.map_or(true, |f| f.iter().any(|x| *x == name)) { return None; }
            Some((name, r.try_get::<Option<i64>, _>(1).ok().flatten()))
        }).collect();
        by_schema.push((schema.clone(), list));
    }
    rep.plan(by_schema.iter().flat_map(|(s, ts)| ts.iter().map(move |(t, est)| PlanEntry {
        schema: Some(s.clone()), name: t.clone(), est_rows: *est,
    })).collect());

    let mut stats = Stats::default();
    for (schema, tables) in &by_schema {
        if is_cancelled() { break; }
        out.push_str(&format!("USE {};\n\n", qb(schema)));
        stats.tables += tables.len();

        // ── Tables (DDL + rows, a couple at a time, written in order) ──
        let jobs: Vec<_> = tables.iter().map(|(t, _)| Box::pin(my_dump_table(rep, pool, schema, t, opts))).collect();
        let mut results = futures::stream::iter(jobs).buffered(MYSQL_PARALLEL);
        let mut idx = 0usize;
        while let Some(res) = results.next().await {
            let table = &tables[idx].0;
            idx += 1;
            match res {
                Ok((chunk, rows)) => {
                    out.append(chunk)?;
                    stats.rows += rows as usize;
                    rep.table(Some(schema), table, "done", rows, None);
                }
                Err(_) if is_cancelled() => break,
                Err(e) => {
                    out.push_str(&format!("-- ERROR exporting {schema}.{table}: {e}\n\n"));
                    rep.table(Some(schema), table, "error", 0, Some(e.clone()));
                    rep.log("error", format!("{schema}.{table}: {e}"));
                    stats.failed += 1;
                }
            }
        }
        drop(results);
        if is_cancelled() { break; }

        // ── Views ──
        if opts.include_views {
            let view_names: Vec<String> = sqlx::query_scalar::<_, String>(
                "SELECT TABLE_NAME FROM information_schema.VIEWS WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME"
            )
            .bind(schema).fetch_all(pool).await.unwrap_or_default();
            if !view_names.is_empty() {
                out.push_str("-- Views\n");
                for view in &view_names {
                    if let Ok(row) = sqlx::query(&format!("SHOW CREATE VIEW {}.{}", qb(schema), qb(view))).fetch_one(pool).await {
                        let create = super::mysql::my_text(&row, 1).unwrap_or_default();
                        out.push_str(&create.replacen("CREATE ", "CREATE OR REPLACE ", 1));
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
                out.push_str("-- Functions & Procedures\nDELIMITER //\n");
                for (name, rtype) in &routines {
                    let keyword = if rtype == "FUNCTION" { "FUNCTION" } else { "PROCEDURE" };
                    if let Ok(row) = sqlx::query(&format!("SHOW CREATE {keyword} {}.{}", qb(schema), qb(name))).fetch_one(pool).await {
                        let create = super::mysql::my_text(&row, 2).unwrap_or_default();
                        out.push_str(&create);
                        out.push_str("//\n\n");
                    }
                }
                out.push_str("DELIMITER ;\n\n");
            }
        }

        // ── Triggers ──
        if opts.include_triggers {
            let triggers: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
                "SELECT TRIGGER_NAME, EVENT_OBJECT_TABLE FROM information_schema.TRIGGERS \
                 WHERE TRIGGER_SCHEMA = ? ORDER BY TRIGGER_NAME"
            )
            .bind(schema).fetch_all(pool).await.unwrap_or_default();
            let triggers: Vec<_> = triggers.into_iter()
                .filter(|(_, tbl)| table_filter.is_none() || tables.iter().any(|(t, _)| t == tbl))
                .collect();
            if !triggers.is_empty() {
                out.push_str("-- Triggers\nDELIMITER //\n");
                for (trig, _) in &triggers {
                    if let Ok(row) = sqlx::query(&format!("SHOW CREATE TRIGGER {}.{}", qb(schema), qb(trig))).fetch_one(pool).await {
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
    out.check()?;
    Ok(stats)
}

/// How a MySQL column's text-protocol value becomes a literal.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MyKind {
    /// Numbers: the server's own text is already an exact literal.
    Num,
    /// Bytes: hex, which never needs escaping and survives any charset.
    Bin,
    /// Everything else: a backslash-escaped string.
    Text,
}

fn my_kind(type_name: &str) -> MyKind {
    let t = type_name.to_ascii_uppercase();
    let base = t.split_whitespace().next().unwrap_or("");
    match base {
        "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "INTEGER" | "BIGINT" | "DECIMAL" | "FLOAT" | "DOUBLE"
        | "YEAR" | "BOOLEAN" => MyKind::Num,
        "BINARY" | "VARBINARY" | "TINYBLOB" | "BLOB" | "MEDIUMBLOB" | "LONGBLOB" | "BIT" | "GEOMETRY" => MyKind::Bin,
        _ => MyKind::Text,
    }
}

const HEX: &[u8; 16] = b"0123456789abcdef";

fn push_hex(buf: &mut String, bytes: &[u8]) {
    if bytes.is_empty() {
        buf.push_str("''");
        return;
    }
    buf.reserve(bytes.len() * 2 + 2);
    buf.push_str("0x");
    for &b in bytes {
        buf.push(HEX[(b >> 4) as usize] as char);
        buf.push(HEX[(b & 15) as usize] as char);
    }
}

/// MySQL string literal with mysqldump's escapes. Newlines are escaped too, so
/// every row stays on one line of the dump.
fn push_my_str(buf: &mut String, s: &str) {
    buf.reserve(s.len() + 2);
    buf.push('\'');
    let mut start = 0;
    for (i, b) in s.bytes().enumerate() {
        let esc = match b {
            b'\\' => "\\\\",
            b'\'' => "\\'",
            b'\0' => "\\0",
            b'\n' => "\\n",
            b'\r' => "\\r",
            0x1a => "\\Z",
            _ => continue,
        };
        buf.push_str(&s[start..i]);
        buf.push_str(esc);
        start = i + 1;
    }
    buf.push_str(&s[start..]);
    buf.push('\'');
}

/// One MySQL table: `SHOW CREATE TABLE` plus multi-row `INSERT IGNORE`s.
///
/// Rows are read over the text protocol (an unprepared query), where every value
/// arrives in the server's own canonical text form - exact DECIMALs, full
/// DATETIME precision, TIME past 24h - so there is no per-type decoding to get
/// wrong (the old binary-protocol decoder had to guess, and DECIMAL once came
/// out truncated to its integer part).
async fn my_dump_table(
    rep: &Reporter,
    pool: &sqlx::MySqlPool,
    schema: &str,
    table: &str,
    opts: &ExportOptions,
) -> Result<(Chunk, u64), String> {
    let mut chunk = Chunk::new();
    if is_cancelled() { return Err("Cancelled".into()); }
    rep.table(Some(schema), table, "running", 0, None);
    let mut conn = pool.acquire().await.map_err(|e| e.to_string())?;

    if opts.include_schema {
        let row = sqlx::query(&format!("SHOW CREATE TABLE {}.{}", qb(schema), qb(table)))
            .fetch_one(&mut *conn).await
            .map_err(|e| format!("SHOW CREATE TABLE failed: {e}"))?;
        let create_sql = super::mysql::my_text(&row, 1).unwrap_or_default();
        chunk.put_str(&create_sql.replacen("CREATE TABLE ", "CREATE TABLE IF NOT EXISTS ", 1));
        chunk.put_str(";\n\n");
    }
    if !opts.include_data { return Ok((chunk, 0)); }

    let q = format!("SELECT * FROM {}.{}", qb(schema), qb(table));
    let mut n = 0u64;
    let mut last = Instant::now();
    let res: Result<(), String> = async {
        let mut stream = (&mut *conn).fetch(q.as_str());
        let mut kinds: Vec<MyKind> = Vec::new();
        let mut batch: Option<InsertBatcher> = None;
        while let Some(row) = stream.try_next().await.map_err(|e| e.to_string())? {
            if batch.is_none() {
                kinds = row.columns().iter().map(|c| my_kind(c.type_info().name())).collect();
                let cols = row.columns().iter().map(|c| qb(c.name())).collect::<Vec<_>>().join(", ");
                // Unqualified: the dump's `USE` picks the database, so the
                // script can be pointed at another one by editing one line.
                batch = Some(InsertBatcher::new(format!("INSERT IGNORE INTO {} ({cols}) VALUES", qb(table)), "", INSERT_BATCH_BYTES));
            }
            let b = batch.as_mut().expect("batch set above");
            n += 1;
            let buf = b.row();
            for (i, kind) in kinds.iter().enumerate() {
                if i > 0 { buf.push_str(", "); }
                let v: Option<&[u8]> = row.try_get_unchecked(i).map_err(|e| {
                    format!("Could not read {table}.{} at row {n}: {e}", row.column(i).name())
                })?;
                match (v, kind) {
                    (None, _) => buf.push_str("NULL"),
                    (Some(bytes), MyKind::Num) => match std::str::from_utf8(bytes) {
                        Ok(s) if !s.is_empty() => buf.push_str(s),
                        _ => push_hex(buf, bytes),
                    },
                    (Some(bytes), MyKind::Bin) => push_hex(buf, bytes),
                    (Some(bytes), MyKind::Text) => match std::str::from_utf8(bytes) {
                        Ok(s) => push_my_str(buf, s),
                        Err(_) => push_hex(buf, bytes),
                    },
                }
            }
            b.end_row(&mut chunk);
            if n % 1024 == 0 {
                if is_cancelled() { return Err("Cancelled".into()); }
                if last.elapsed() >= PROGRESS_EVERY {
                    rep.table(Some(schema), table, "running", n, None);
                    last = Instant::now();
                }
            }
        }
        if let Some(mut b) = batch { b.flush(&mut chunk); }
        Ok(())
    }.await;
    if let Err(e) = res {
        conn.close_on_drop();
        return Err(e);
    }
    chunk.put_str("\n");
    Ok((chunk, n))
}

// ── MySQL import ──────────────────────────────────────────────────────────────

async fn import_mysql(rep: &Reporter, pool: &sqlx::MySqlPool, sql: &str) -> Result<ImportResult, String> {
    let started = Instant::now();
    rep.log("info", "Starting restore…");
    // One connection for the whole restore: the dump carries per-schema `USE …;`
    // directives, which are connection-scoped - on a pool, the USE and the
    // unqualified statements that depend on it could land on different
    // connections, restoring tables into the wrong database. It is closed
    // afterwards so neither the USE nor the session flags below leak.
    let mut conn = pool.acquire().await.map_err(|e| format!("Failed to acquire connection: {e}"))?;
    conn.close_on_drop();
    // autocommit off keeps a transaction open, which is what lets a failed
    // INSERT batch roll back to its savepoint. FK and unique checks are off
    // for the load, as mysqldump does: the rows came from a consistent database.
    (&mut *conn).execute("SET autocommit = 0, foreign_key_checks = 0, unique_checks = 0").await.map_err(|e| e.to_string())?;

    let mut run = Runner::new(rep, sql.len());
    let mut sp = Splitter::new(sql, Escapes::Backslash);
    while let Some(stmt) = sp.next() {
        let Stmt::Sql(s) = stmt else { continue };
        run.total += 1;
        if is_insert(&s) {
            run.queue(s);
            if run.batch_full() { run.flush_my(&mut *conn).await; }
        } else {
            run.flush_my(&mut *conn).await;
            // DDL commits implicitly in MySQL, so a savepoint would not survive
            // it; a failing statement is atomic on its own.
            match (&mut *conn).execute(s.as_str()).await {
                Ok(_) => run.ok += 1,
                Err(e) => run.errors.push(stmt_error(e, &s)),
            }
        }
        run.meter.tick(sp.position(), run.ok, run.errors.len(), false);
        if is_cancelled() { break; }
    }
    if is_cancelled() {
        run.total += sp.count();
        (&mut *conn).execute("ROLLBACK").await.ok();
    } else {
        run.flush_my(&mut *conn).await;
        (&mut *conn).execute("COMMIT").await.map_err(|e| e.to_string())?;
    }
    run.meter.tick(sql.len(), run.ok, run.errors.len(), true);
    Ok(finish_import(rep, run.total, run.ok, run.errors, started))
}

// ── D1 export ─────────────────────────────────────────────────────────────────

/// Cloudflare D1 rejects access to its internal tables (prefixed `_cf_`) with a
/// `SQLITE_AUTH` error, and SQLite's own `sqlite_*` tables aren't user data.
/// Skip both so a backup doesn't abort on e.g. `SELECT * FROM "_cf_KV"`.
fn is_d1_internal_table(name: &str) -> bool {
    name.starts_with("_cf_") || name.starts_with("sqlite_")
}

async fn export_d1(
    rep: &Reporter,
    cfg: &super::connection::D1Config,
    filter: Option<&[String]>,
    opts: &ExportOptions,
    out: &mut Out,
) -> Result<Stats, String> {
    rep.log("info", "Starting Cloudflare D1 export…");
    out.push_str(&backup_header("Cloudflare D1 (SQLite)", None));
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

    let in_filter = |t: &str| filter.map_or(true, |f| f.iter().any(|x| x == t));

    let mut tables: Vec<String> = Vec::new();
    let mut after: Vec<String> = Vec::new();
    for row in &ddl_result.rows {
        let obj_type = row.get(type_idx).and_then(|v| v.as_str()).unwrap_or("");
        let name     = row.get(name_idx).and_then(|v| v.as_str()).unwrap_or("");
        let tbl_name = row.get(tbl_idx).and_then(|v| v.as_str()).unwrap_or("");
        let ddl_sql  = row.get(sql_idx).and_then(|v| v.as_str()).unwrap_or("");
        let stmt = format!("{};\n", ddl_sql.trim());
        match obj_type {
            "table" => {
                // Skip Cloudflare D1 internal tables (e.g. _cf_KV): D1 rejects
                // SELECT against them with SQLITE_AUTH.
                if is_d1_internal_table(name) || !in_filter(name) { continue; }
                tables.push(name.to_string());
                if opts.include_schema { out.push_str(&stmt); }
            }
            "view" => if opts.include_views { after.push(stmt) },
            "trigger" => if opts.include_triggers && in_filter(tbl_name) { after.push(stmt) },
            _ => if opts.include_schema && in_filter(tbl_name) { after.push(stmt) },
        }
    }

    rep.plan(tables.iter().map(|t| PlanEntry { schema: None, name: t.clone(), est_rows: None }).collect());

    let mut stats = Stats { tables: tables.len(), ..Stats::default() };
    for table in &tables {
        if is_cancelled() { break; }
        if !opts.include_data { rep.table(None, table, "done", 0, None); continue; }
        rep.table(None, table, "running", 0, None);
        let data_result = match super::d1::query(cfg, &format!("SELECT * FROM {}", qi(table)), vec![]).await {
            Ok(r) => r,
            Err(e) => {
                // A single unreadable/prohibited table shouldn't abort the whole backup.
                out.push_str(&format!("-- ERROR exporting {table}: {e}\n"));
                rep.table(None, table, "error", 0, Some(e.clone()));
                rep.log("error", format!("{table}: {e}"));
                stats.failed += 1;
                continue;
            }
        };
        let n = data_result.rows.len();
        let mut chunk = Chunk::new();
        if n > 0 {
            let col_list = data_result.columns.iter().map(|c| qi(&c.name)).collect::<Vec<_>>().join(", ");
            let mut batch = InsertBatcher::new(
                format!("\nINSERT OR REPLACE INTO {} ({col_list}) VALUES", qi(table)), "", SQLITE_BATCH_BYTES,
            );
            for row in &data_result.rows {
                let buf = batch.row();
                for i in 0..data_result.columns.len() {
                    if i > 0 { buf.push_str(", "); }
                    buf.push_str(&row.get(i).map_or("NULL".into(), json_to_sql_val));
                }
                batch.end_row(&mut chunk);
            }
            batch.flush(&mut chunk);
        }
        out.append(chunk)?;
        stats.rows += n;
        rep.table(None, table, "done", n as u64, None);
    }

    out.push('\n');
    for s in &after { out.push_str(s); }
    out.push_str("\nCOMMIT;\nPRAGMA foreign_keys=ON;\n");
    out.check()?;
    Ok(stats)
}

fn json_to_sql_val(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "NULL".into(),
        serde_json::Value::Bool(b) => if *b { "1" } else { "0" }.into(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => ql(s),
        // D1's HTTP API returns BLOB columns as JSON arrays of byte integers.
        // Emit them as SQLite hex blob literals (X'…') so binary data round-trips
        // instead of being stored as the literal text "[1,2,3]".
        serde_json::Value::Array(arr) => {
            let bytes: Option<Vec<u8>> = arr.iter()
                .map(|b| b.as_u64().filter(|n| *n <= 255).map(|n| n as u8))
                .collect();
            match bytes {
                Some(b) => format!("X'{}'", hex::encode(b)),
                None => ql(&v.to_string()),
            }
        }
        other => ql(&other.to_string()),
    }
}

// ── D1 import ────────────────────────────────────────────────────────────────

async fn import_d1(rep: &Reporter, cfg: &super::connection::D1Config, sql: &str) -> Result<ImportResult, String> {
    let started = Instant::now();
    rep.log("info", "Starting restore…");
    let mut run = Runner::new(rep, sql.len());
    let mut sp = Splitter::new(sql, Escapes::Standard);
    while let Some(stmt) = sp.next() {
        let Stmt::Sql(s) = stmt else { continue };
        run.total += 1;
        // D1 has no client transactions and manages FK enforcement itself.
        if head_lower(&s, 24).starts_with("pragma foreign_keys") || is_tx_control(&s) { run.ok += 1; continue; }
        match super::d1::query(cfg, &s, vec![]).await {
            Ok(_) => run.ok += 1,
            Err(e) => run.errors.push(stmt_error(e, &s)),
        }
        run.meter.tick(sp.position(), run.ok, run.errors.len(), false);
        if is_cancelled() { run.total += sp.count(); break; }
    }
    run.meter.tick(sql.len(), run.ok, run.errors.len(), true);
    Ok(finish_import(rep, run.total, run.ok, run.errors, started))
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
    rep: &Reporter,
    handle: &super::connection::DuckdbHandle,
    out: &mut Out,
) -> Result<Stats, String> {
    rep.log("info", "Starting DuckDB export…");

    let db_path = duckdb_database_path(handle).await;
    let export_dir = derive_duckdb_export_dir(db_path.as_deref());
    std::fs::create_dir_all(&export_dir)
        .map_err(|e| format!("Failed to create export directory {}: {e}", export_dir.display()))?;
    let dir_str = export_dir.to_string_lossy().to_string();
    let dir_esc = dir_str.replace('\'', "''");

    rep.log("info", format!("Exporting database to {dir_str}…"));
    super::duckdb::execute_sql(handle, &format!("EXPORT DATABASE '{dir_esc}' (FORMAT PARQUET)")).await?;

    // Best-effort table count for the UI; the export has already succeeded.
    let table_count = duckdb_table_count(handle).await;

    out.push_str(&backup_header("DuckDB", None));
    out.push_str(
        "-- NOTE: DuckDB EXPORT/IMPORT DATABASE operate on a directory of Parquet\n\
         --       files on the local filesystem. The data was written to the path\n\
         --       below; keep that directory to restore. Re-running this script\n\
         --       executes IMPORT DATABASE against it.\n\n",
    );
    out.push_str(&format!("IMPORT DATABASE '{dir_esc}';\n"));

    rep.log("ok", format!("Exported {table_count} tables to {dir_str}"));
    Ok(Stats { tables: table_count, rows: 0, failed: 0 })
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
    rep: &Reporter,
    handle: &super::connection::DuckdbHandle,
    sql: &str,
) -> Result<ImportResult, String> {
    let started = Instant::now();
    rep.log("info", "Starting restore…");
    let mut run = Runner::new(rep, sql.len());
    let mut sp = Splitter::new(sql, Escapes::Standard);
    while let Some(stmt) = sp.next() {
        let Stmt::Sql(s) = stmt else { continue };
        run.total += 1;
        if head_lower(&s, 24).starts_with("pragma foreign_keys") { run.ok += 1; continue; }
        match super::duckdb::execute_sql(handle, &s).await {
            Ok(_) => run.ok += 1,
            Err(e) => run.errors.push(stmt_error(e, &s)),
        }
        run.meter.tick(sp.position(), run.ok, run.errors.len(), false);
        if is_cancelled() { run.total += sp.count(); break; }
    }
    Ok(finish_import(rep, run.total, run.ok, run.errors, started))
}

// ── MS SQL Server export / import ─────────────────────────────────────────────

/// SQL Server backup/restore is a whole-database, server-side operation:
/// `BACKUP`/`RESTORE` write/read a file on the **SQL Server host** filesystem,
/// not on this client. A bare filename lands in the instance's configured default
/// backup directory. The returned "script" records the matching `RESTORE`.
async fn export_mssql(
    rep: &Reporter,
    handle: &super::connection::MssqlHandle,
    out: &mut Out,
) -> Result<Stats, String> {
    rep.log("info", "Starting MS SQL Server export…");

    // The database name comes from the live connection - the active-connection
    // handle carries the client, not the MssqlConfig.
    let db = mssql_current_db(handle).await?;
    let bak_file = format!("{db}_stroke_backup.bak");
    let db_ident = db.replace(']', "]]"); // bracket-quoted identifier
    let path_esc = bak_file.replace('\'', "''"); // single-quoted literal

    rep.log("info", format!("Backing up [{db}] to server-side file {bak_file}…"));
    super::mssql::execute_sql(
        handle,
        &format!("BACKUP DATABASE [{db_ident}] TO DISK = N'{path_esc}' WITH FORMAT, INIT"),
    )
    .await?;

    let table_count = mssql_table_count(handle).await;

    out.push_str(&backup_header("MS SQL Server", None));
    out.push_str(
        "-- NOTE: BACKUP/RESTORE operate on a file on the SQL Server host filesystem\n\
         --       (server-side), not on this client. The database was written to the\n\
         --       server-side path below. Re-running this script issues RESTORE.\n\n",
    );
    out.push_str(&format!("RESTORE DATABASE [{db_ident}] FROM DISK = N'{path_esc}' WITH REPLACE;\n"));

    rep.log("ok", format!("Backed up [{db}] to {bak_file}"));
    Ok(Stats { tables: table_count, rows: 0, failed: 0 })
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
    rep: &Reporter,
    handle: &super::connection::MssqlHandle,
    sql: &str,
) -> Result<ImportResult, String> {
    let started = Instant::now();
    rep.log("info", "Starting restore…");
    let mut run = Runner::new(rep, sql.len());
    let mut sp = Splitter::new(sql, Escapes::Standard);
    while let Some(stmt) = sp.next() {
        let Stmt::Sql(s) = stmt else { continue };
        run.total += 1;
        match super::mssql::execute_sql(handle, &s).await {
            Ok(_) => run.ok += 1,
            Err(e) => run.errors.push(stmt_error(e, &s)),
        }
        run.meter.tick(sp.position(), run.ok, run.errors.len(), false);
        if is_cancelled() { run.total += sp.count(); break; }
    }
    Ok(finish_import(rep, run.total, run.ok, run.errors, started))
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_dollar_inside_an_identifier_is_not_a_quote() {
        let s = split("SELECT a$b$c FROM t; SELECT 2;");
        assert_eq!(s, vec!["SELECT a$b$c FROM t".to_string(), "SELECT 2".to_string()]);
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
    fn keeps_a_sqlite_trigger_body_together() {
        // SQLite has no DELIMITER: the trigger ends at the `;` after its END.
        // Splitting at the first `;` broke every exported SQLite trigger.
        let sql = "CREATE TRIGGER t AFTER UPDATE ON x BEGIN UPDATE x SET n = CASE WHEN 1 THEN 'end;' END WHERE id=NEW.id; INSERT INTO l VALUES (1); END;\nSELECT 1;";
        let s = split(sql);
        assert_eq!(s.len(), 2, "got: {s:?}");
        assert!(s[0].ends_with("END"), "got: {s:?}");
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

    #[test]
    fn mysql_backslash_escaped_quote_does_not_end_the_literal() {
        // Exactly what the MySQL exporter emits for the value `it's; and \`.
        // Read with standard rules the literal ends at `it\'`, the `;` then
        // splits the row in half and every later statement inherits the damage.
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

    #[test]
    fn reads_copy_blocks_with_their_data() {
        let sql = "SET x = 1;\nCOPY public.t (a, b) FROM stdin;\n1\tit's; fine\n2\t\\N\n\\.\nCOPY public.e (a) FROM stdin;\n\\.\n\nSELECT 1;";
        let stmts: Vec<Stmt> = Splitter::new(sql, Escapes::Standard).collect();
        assert_eq!(stmts.len(), 4, "got: {stmts:?}");
        match &stmts[1] {
            Stmt::Copy { sql, data } => {
                assert_eq!(sql, "COPY public.t (a, b) FROM stdin");
                assert_eq!(*data, "1\tit's; fine\n2\t\\N\n");
            }
            other => panic!("expected COPY, got {other:?}"),
        }
        match &stmts[2] {
            Stmt::Copy { data, .. } => assert_eq!(*data, ""),
            other => panic!("expected empty COPY, got {other:?}"),
        }
        assert!(matches!(&stmts[3], Stmt::Sql(s) if s == "SELECT 1"));
    }

    #[test]
    fn skips_psql_meta_commands() {
        // pg_dump 17.6+ writes `\restrict <key>` / `\unrestrict <key>` lines.
        let s = split("\\restrict abc123\nSELECT 1;\n\\connect other\nSELECT 2;\n\\unrestrict abc123\n");
        assert_eq!(s, vec!["SELECT 1".to_string(), "SELECT 2".to_string()]);
    }

    #[test]
    fn recognises_transaction_control() {
        for s in ["BEGIN", "begin transaction", "COMMIT", "END", "START TRANSACTION", "BEGIN IMMEDIATE"] {
            assert!(is_tx_control(s), "{s}");
        }
        for s in ["BEGIN; SELECT 1", "COMMIT AND CHAIN NOW PLEASE", "END_DATE", "INSERT INTO t VALUES (1)"] {
            assert!(!is_tx_control(s), "{s}");
        }
    }

    #[test]
    fn parses_copy_targets() {
        assert_eq!(
            parse_copy_target("COPY \"public\".\"t\" (\"a\", \"b\") FROM stdin"),
            Some(("\"public\".\"t\"".into(), Some("\"a\", \"b\"".into())))
        );
        assert_eq!(parse_copy_target("copy t from stdin"), Some(("t".into(), None)));
    }

    #[test]
    fn inspects_dump_headers() {
        let i = inspect_head(10, &backup_header("PostgreSQL", Some("public")));
        assert!(i.stroke && !i.incomplete);
        assert_eq!(i.engine.as_deref(), Some("PostgreSQL"));
        let i = inspect_head(10, "-- !! INCOMPLETE BACKUP - cancelled\n-- Stroke Backup\n-- Engine   : MySQL\n");
        assert!(i.incomplete);
        assert_eq!(i.engine.as_deref(), Some("MySQL"));
        assert_eq!(inspect_head(1, "--\n-- PostgreSQL database dump\n").engine.as_deref(), Some("PostgreSQL"));
    }

    #[test]
    fn escapes_mysql_strings_like_mysqldump() {
        let mut b = String::new();
        push_my_str(&mut b, "it's\\ a\nline\0é");
        assert_eq!(b, "'it\\'s\\\\ a\\nline\\0é'");
        let s = split_statements(&format!("INSERT INTO t VALUES ({b});SELECT 1;"), Escapes::Backslash);
        assert_eq!(s.len(), 2, "got: {s:?}");
        let mut h = String::new();
        push_hex(&mut h, &[0, 255, 16]);
        assert_eq!(h, "0x00ff10");
    }

    #[test]
    fn batches_rows_into_multi_row_inserts() {
        let mut chunk = Chunk::new();
        let mut b = InsertBatcher::new("INSERT INTO t (a) VALUES".into(), "", 40);
        for i in 0..5 {
            b.row().push_str(&i.to_string());
            b.end_row(&mut chunk);
        }
        b.flush(&mut chunk);
        let text = String::from_utf8(chunk.mem.clone()).unwrap();
        let stmts = split(&text);
        assert!(stmts.len() >= 2 && stmts.len() < 5, "got: {stmts:?}");
        assert!(stmts.iter().all(|s| s.starts_with("INSERT INTO t (a) VALUES")));
        assert_eq!(text.matches('(').count() - stmts.len(), 5);
    }

    // ── SQLite round trip (runs everywhere: no server needed) ────────────────

    async fn sqlite_pool(path: &Path) -> sqlx::SqlitePool {
        let opts: sqlx::sqlite::SqliteConnectOptions =
            format!("sqlite://{}", path.display()).parse().unwrap();
        sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts.create_if_missing(true))
            .await
            .unwrap()
    }

    fn scratch(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("stroke-backup-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[tokio::test]
    async fn sqlite_round_trip_keeps_rows_foreign_keys_and_triggers() {
        let src_path = scratch("src.db");
        let dst_path = scratch("dst.db");
        let src = sqlite_pool(&src_path).await;
        // `a_child` sorts before `z_parent`: the restore loads it first, which
        // only works with FK enforcement off for the load.
        sqlx::raw_sql(
            "CREATE TABLE z_parent (id INTEGER PRIMARY KEY, name TEXT);
             CREATE TABLE a_child (id INTEGER PRIMARY KEY, parent_id INTEGER NOT NULL REFERENCES z_parent(id), note TEXT, blob BLOB);
             CREATE INDEX a_child_parent ON a_child(parent_id);
             CREATE TABLE log (n INTEGER);
             CREATE TRIGGER a_child_log AFTER INSERT ON a_child BEGIN INSERT INTO log VALUES (NEW.id); END;
             CREATE VIEW v AS SELECT id FROM a_child;",
        ).execute(&src).await.unwrap();
        sqlx::raw_sql(
            "WITH RECURSIVE s(g) AS (SELECT 1 UNION ALL SELECT g+1 FROM s WHERE g < 300)
             INSERT INTO z_parent SELECT g, 'O''Brien; ' || g || char(10) || 'é' FROM s;
             WITH RECURSIVE s(g) AS (SELECT 1 UNION ALL SELECT g+1 FROM s WHERE g < 3000)
             INSERT INTO a_child SELECT g, 1 + g % 300, printf('%.200c', 'x'), randomblob(32) FROM s;
             DELETE FROM log;",
        ).execute(&src).await.unwrap();

        let rep = Reporter::silent();
        let mut out = Out::memory();
        let stats = export_sqlite(&rep, &src, None, &ExportOptions::default(), &mut out).await.unwrap();
        let (bytes, _) = out.finish().unwrap();
        let dump = String::from_utf8(bytes).unwrap();
        assert_eq!(stats.rows, 3300);
        assert!(dump.matches("INSERT OR REPLACE").count() < 100, "rows should be batched");

        let dst = sqlite_pool(&dst_path).await;
        let res = import_sqlite(&rep, &dst, &dump).await.unwrap();
        assert!(res.errors.is_empty(), "restore errors: {:?}", res.errors);

        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM a_child").fetch_one(&dst).await.unwrap();
        assert_eq!(n, 3000);
        let name: String = sqlx::query_scalar("SELECT name FROM z_parent WHERE id = 7").fetch_one(&dst).await.unwrap();
        assert_eq!(name, "O'Brien; 7\né");
        let same_blob: i64 = sqlx::query_scalar("SELECT count(*) FROM a_child WHERE length(blob) = 32").fetch_one(&dst).await.unwrap();
        assert_eq!(same_blob, 3000);
        // The trigger was created after the rows, so it never fired on restore.
        let logged: i64 = sqlx::query_scalar("SELECT count(*) FROM log").fetch_one(&dst).await.unwrap();
        assert_eq!(logged, 0);
        let fks: i64 = sqlx::query_scalar("SELECT count(*) FROM pragma_foreign_key_list('a_child')").fetch_one(&dst).await.unwrap();
        assert_eq!(fks, 1);
        let trig: i64 = sqlx::query_scalar("SELECT count(*) FROM sqlite_master WHERE type='trigger'").fetch_one(&dst).await.unwrap();
        assert_eq!(trig, 1);
        // FK enforcement is back on for the connection after the restore.
        assert!(sqlx::query("INSERT INTO a_child (id, parent_id) VALUES (99999, 424242)").execute(&dst).await.is_err());

        src.close().await;
        dst.close().await;
        let _ = std::fs::remove_file(src_path);
        let _ = std::fs::remove_file(dst_path);
    }

    #[tokio::test]
    async fn file_export_streams_to_disk_and_leaves_no_partial() {
        let src_path = scratch("file-src.db");
        let src = sqlite_pool(&src_path).await;
        sqlx::raw_sql("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT); INSERT INTO t VALUES (1, 'a'), (2, 'b');")
            .execute(&src).await.unwrap();
        let target = scratch("dump.sql");
        let res = run_export(
            &Reporter::silent(), ActiveConnection::Sqlite(src.clone()), None, None,
            &ExportOptions::default(), Some(target.to_string_lossy().into_owned()),
        ).await.unwrap();
        assert!(res.sql.is_empty());
        assert_eq!(res.row_count, 2);
        let on_disk = std::fs::read_to_string(&target).unwrap();
        assert_eq!(on_disk.len() as u64, res.bytes);
        assert!(on_disk.contains("INSERT OR REPLACE INTO \"t\""));
        assert!(!partial_path(&target).exists());
        src.close().await;
        let _ = std::fs::remove_file(src_path);
        let _ = std::fs::remove_file(target);
    }

    // ── Live servers (ignored; point them at throwaway databases) ────────────
    //
    //   STROKE_TEST_PG_URL=postgres://postgres@127.0.0.1:55871 \
    //   STROKE_TEST_MYSQL_URL=mysql://root@127.0.0.1:33871 \
    //   cargo test --lib backup::tests::live_ -- --ignored
    //
    // Each creates and drops its own databases (stroke_bk_*) on that server.

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_postgres_dump_restores_with_its_foreign_keys() {
        let Ok(base) = std::env::var("STROKE_TEST_PG_URL") else { return };
        let admin = sqlx::PgPool::connect(&format!("{base}/postgres")).await.unwrap();
        // Separate statements: DROP/CREATE DATABASE refuse multi-statement strings.
        for db in ["stroke_bk_src", "stroke_bk_dst"] {
            sqlx::query(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)")).execute(&admin).await.unwrap();
            sqlx::query(&format!("CREATE DATABASE {db}")).execute(&admin).await.unwrap();
        }
        let src = sqlx::postgres::PgPoolOptions::new().max_connections(6).connect(&format!("{base}/stroke_bk_src")).await.unwrap();
        sqlx::raw_sql(
            "CREATE TYPE mood AS ENUM ('ok', 'a,b');
             CREATE TABLE parent (id serial PRIMARY KEY, name text NOT NULL, m mood, price money);
             CREATE TABLE child (id bigint GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
                                 parent_id int NOT NULL REFERENCES parent(id) ON DELETE CASCADE,
                                 doc jsonb, raw bytea, at timestamptz, span interval, f float8,
                                 total numeric GENERATED ALWAYS AS (id * 2) STORED);
             CREATE INDEX child_parent ON child (parent_id);
             INSERT INTO parent (name, m, price) SELECT 'p' || g || E' it''s;\\n\\\\', 'a,b', 12.5 FROM generate_series(1, 50) g;
             INSERT INTO child (parent_id, doc, raw, at, span, f)
               SELECT 1 + g % 50, jsonb_build_object('g', g), '\\x00ff'::bytea, now(), '1 day 2 hours', 0.1 + g
               FROM generate_series(1, 2000) g;",
        ).execute(&src).await.unwrap();

        let rep = Reporter::silent();
        let mut out = Out::memory();
        let stats = export_postgres(&rep, &src, Some("public"), None, &ExportOptions::default(), &mut out).await.unwrap();
        let dump = String::from_utf8(out.finish().unwrap().0).unwrap();
        assert_eq!(stats.rows, 2050);
        assert!(!dump.contains("WARNING: could not export"), "catalog query failed:\n{dump}");
        assert!(dump.contains("ADD CONSTRAINT child_parent_id_fkey FOREIGN KEY (parent_id) REFERENCES"), "no FK in dump");

        let dst = sqlx::postgres::PgPoolOptions::new().max_connections(6).connect(&format!("{base}/stroke_bk_dst")).await.unwrap();
        let res = import_postgres(&rep, &dst, &dump).await.unwrap();
        assert!(res.errors.is_empty(), "restore errors: {:?}", res.errors);

        let fks: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_constraint WHERE contype = 'f'").fetch_one(&dst).await.unwrap();
        assert_eq!(fks, 1, "foreign key missing after restore");
        let orphan = sqlx::query("INSERT INTO child (parent_id) VALUES (999999)").execute(&dst).await;
        assert!(orphan.is_err(), "FK is not enforced after restore");
        let name: String = sqlx::query_scalar("SELECT name FROM parent WHERE id = 3").fetch_one(&dst).await.unwrap();
        assert_eq!(name, "p3 it's;\n\\");
        let total: f64 = sqlx::query_scalar("SELECT sum(f) FROM child").fetch_one(&dst).await.unwrap();
        let src_total: f64 = sqlx::query_scalar("SELECT sum(f) FROM child").fetch_one(&src).await.unwrap();
        assert_eq!(total, src_total);
        // Sequences were fast-forwarded: new rows don't collide with restored ids.
        sqlx::query("INSERT INTO parent (name) VALUES ('new')").execute(&dst).await.unwrap();
        sqlx::query("INSERT INTO child (parent_id) VALUES (1)").execute(&dst).await.unwrap();

        // Restoring the same dump again over the data: COPY hits duplicates and
        // falls back to ON CONFLICT DO NOTHING. Only "already exists" errors.
        let again = import_postgres(&rep, &dst, &dump).await.unwrap();
        assert!(again.errors.iter().all(|e| e.contains("already exists")), "{:?}", again.errors);
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM child").fetch_one(&dst).await.unwrap();
        assert_eq!(n, 2001);

        src.close().await;
        dst.close().await;
        for db in ["stroke_bk_src", "stroke_bk_dst"] {
            sqlx::query(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)")).execute(&admin).await.ok();
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn live_mysql_dump_restores_with_its_foreign_keys() {
        let Ok(base) = std::env::var("STROKE_TEST_MYSQL_URL") else { return };
        let admin = sqlx::mysql::MySqlPoolOptions::new().max_connections(2).connect(&base).await.unwrap();
        for db in ["stroke_bk_src", "stroke_bk_dst"] {
            sqlx::query(&format!("DROP DATABASE IF EXISTS {db}")).execute(&admin).await.unwrap();
            sqlx::query(&format!("CREATE DATABASE {db}")).execute(&admin).await.unwrap();
        }
        let src = sqlx::mysql::MySqlPoolOptions::new().max_connections(4).connect(&format!("{base}/stroke_bk_src")).await.unwrap();
        sqlx::raw_sql(
            "CREATE TABLE z_parent (id INT PRIMARY KEY, name TEXT, price DECIMAL(10,2), at DATETIME(6), t TIME);
             CREATE TABLE a_child (id INT AUTO_INCREMENT PRIMARY KEY, parent_id INT NOT NULL, raw BLOB, doc JSON,
                                   CONSTRAINT fk_child FOREIGN KEY (parent_id) REFERENCES z_parent(id));
             INSERT INTO z_parent VALUES (1, 'it''s; \\\\ \\n é', 9.99, '2024-01-02 03:04:05.123456', '-838:59:59'),
                                         (2, NULL, 0.01, NULL, '25:00:00');
             INSERT INTO a_child (parent_id, raw, doc) VALUES (1, UNHEX('00FF10'), JSON_OBJECT('a', 'q\"s')), (2, '', NULL);",
        ).execute(&src).await.unwrap();

        let rep = Reporter::silent();
        let mut out = Out::memory();
        export_mysql(&rep, &src, Some("stroke_bk_src"), None, &ExportOptions::default(), &mut out).await.unwrap();
        let dump = String::from_utf8(out.finish().unwrap().0).unwrap()
            .replace("USE `stroke_bk_src`;", "USE `stroke_bk_dst`;");

        let dst = sqlx::mysql::MySqlPoolOptions::new().max_connections(4).connect(&format!("{base}/stroke_bk_dst")).await.unwrap();
        let res = import_mysql(&rep, &dst, &dump).await.unwrap();
        assert!(res.errors.is_empty(), "restore errors: {:?}", res.errors);

        let fks: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM information_schema.REFERENTIAL_CONSTRAINTS WHERE CONSTRAINT_SCHEMA = 'stroke_bk_dst'",
        ).fetch_one(&dst).await.unwrap();
        assert_eq!(fks, 1, "foreign key missing after restore");
        let row = sqlx::raw_sql(
            "SELECT CONCAT_WS('|', name, price, at, t, HEX((SELECT raw FROM a_child WHERE id = 1)), (SELECT doc FROM a_child WHERE id = 1)) FROM z_parent WHERE id = 1",
        ).fetch_one(&dst).await.unwrap();
        let got = super::super::mysql::my_text(&row, 0).unwrap();
        let row = sqlx::raw_sql(
            "SELECT CONCAT_WS('|', name, price, at, t, HEX((SELECT raw FROM a_child WHERE id = 1)), (SELECT doc FROM a_child WHERE id = 1)) FROM z_parent WHERE id = 1",
        ).fetch_one(&src).await.unwrap();
        let want = super::super::mysql::my_text(&row, 0).unwrap();
        assert_eq!(got, want);

        src.close().await;
        dst.close().await;
        for db in ["stroke_bk_src", "stroke_bk_dst"] {
            sqlx::query(&format!("DROP DATABASE IF EXISTS {db}")).execute(&admin).await.ok();
        }
    }

    // ── Benchmark (ignored) ───────────────────────────────────────────────────
    //
    //   STROKE_BENCH_DIR=<dir with bench.db, dumps land here>
    //   STROKE_TEST_PG_URL=… (a `bench` database to dump; `bench_restore` is recreated)
    //   STROKE_TEST_MYSQL_URL=… (same)
    //   cargo test --release --lib backup::tests::bench_ -- --ignored --nocapture --test-threads=1

    fn bench_dir() -> Option<PathBuf> { std::env::var("STROKE_BENCH_DIR").ok().map(PathBuf::from) }

    async fn export_to_file<F, Fut>(path: &Path, f: F) -> (Stats, u64, Duration)
    where
        F: FnOnce(Out) -> Fut,
        Fut: std::future::Future<Output = (Out, Result<Stats, String>)>,
    {
        let t = Instant::now();
        let (out, stats) = f(Out::file(path).unwrap()).await;
        let (_, bytes) = out.finish().unwrap();
        (stats.unwrap(), bytes, t.elapsed())
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn bench_postgres() {
        let (Some(dir), Ok(base)) = (bench_dir(), std::env::var("STROKE_TEST_PG_URL")) else { return };
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(10).connect(&format!("{base}/bench")).await.unwrap();
        let rep = Reporter::silent();
        let file = dir.join("bench_pg.sql");
        let (stats, bytes, ex) = export_to_file(&file, |mut out| async {
            let r = export_postgres(&rep, &pool, Some("public"), None, &ExportOptions::default(), &mut out).await;
            (out, r)
        }).await;
        let admin = sqlx::PgPool::connect(&format!("{base}/postgres")).await.unwrap();
        sqlx::query("DROP DATABASE IF EXISTS bench_restore WITH (FORCE)").execute(&admin).await.unwrap();
        sqlx::query("CREATE DATABASE bench_restore").execute(&admin).await.unwrap();
        let rp = sqlx::postgres::PgPoolOptions::new().max_connections(10).connect(&format!("{base}/bench_restore")).await.unwrap();
        let t = Instant::now();
        let script = tokio::fs::read_to_string(&file).await.unwrap();
        let ir = import_postgres(&rep, &rp, &script).await.unwrap();
        let im = t.elapsed();
        let fks: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_constraint WHERE contype = 'f'").fetch_one(&rp).await.unwrap();
        eprintln!("NEW PG export {ex:?} ({} rows, {bytes} bytes) restore {im:?} ok={} err={} fks={fks} errs={:?}",
            stats.rows, ir.statements_ok, ir.statements_err, ir.errors.iter().take(3).collect::<Vec<_>>());
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn bench_mysql() {
        let (Some(dir), Ok(base)) = (bench_dir(), std::env::var("STROKE_TEST_MYSQL_URL")) else { return };
        let pool = sqlx::mysql::MySqlPoolOptions::new().max_connections(4).connect(&format!("{base}/bench")).await.unwrap();
        let rep = Reporter::silent();
        let file = dir.join("bench_mysql.sql");
        let (stats, bytes, ex) = export_to_file(&file, |mut out| async {
            let r = export_mysql(&rep, &pool, Some("bench"), None, &ExportOptions::default(), &mut out).await;
            (out, r)
        }).await;
        sqlx::query("DROP DATABASE IF EXISTS bench_restore").execute(&pool).await.unwrap();
        sqlx::query("CREATE DATABASE bench_restore").execute(&pool).await.unwrap();
        let rp = sqlx::mysql::MySqlPoolOptions::new().max_connections(4).connect(&format!("{base}/bench_restore")).await.unwrap();
        let t = Instant::now();
        let script = tokio::fs::read_to_string(&file).await.unwrap().replace("USE `bench`;", "USE `bench_restore`;");
        let ir = import_mysql(&rep, &rp, &script).await.unwrap();
        let im = t.elapsed();
        let fks: i64 = sqlx::query_scalar("SELECT count(*) FROM information_schema.REFERENTIAL_CONSTRAINTS WHERE CONSTRAINT_SCHEMA = 'bench_restore'").fetch_one(&rp).await.unwrap();
        eprintln!("NEW MYSQL export {ex:?} ({} rows, {bytes} bytes) restore {im:?} ok={} err={} fks={fks} errs={:?}",
            stats.rows, ir.statements_ok, ir.statements_err, ir.errors.iter().take(3).collect::<Vec<_>>());
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn bench_sqlite() {
        let Some(dir) = bench_dir() else { return };
        let pool = sqlite_pool(&dir.join("bench.db")).await;
        let rep = Reporter::silent();
        let file = dir.join("bench_sqlite.sql");
        let (stats, bytes, ex) = export_to_file(&file, |mut out| async {
            let r = export_sqlite(&rep, &pool, None, &ExportOptions::default(), &mut out).await;
            (out, r)
        }).await;
        let dst = dir.join("restore_new.db");
        let _ = std::fs::remove_file(&dst);
        let rp = sqlite_pool(&dst).await;
        let t = Instant::now();
        let script = tokio::fs::read_to_string(&file).await.unwrap();
        let ir = import_sqlite(&rep, &rp, &script).await.unwrap();
        let im = t.elapsed();
        let fks: i64 = sqlx::query_scalar("SELECT count(*) FROM sqlite_master m, pragma_foreign_key_list(m.name) WHERE m.type = 'table'").fetch_one(&rp).await.unwrap();
        eprintln!("NEW SQLITE export {ex:?} ({} rows, {bytes} bytes) restore {im:?} ok={} err={} fks={fks} errs={:?}",
            stats.rows, ir.statements_ok, ir.statements_err, ir.errors.iter().take(3).collect::<Vec<_>>());
    }
}
