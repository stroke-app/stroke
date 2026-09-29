//! Instance Insights - read-only database-monitoring snapshots.
//!
//! Powers the frontend "Instance Insights" dashboard (Activity / State / Config /
//! Replication tabs plus a cheap version header). Supported on PostgreSQL, MySQL,
//! SQLite, ClickHouse, and DuckDB; every other dialect returns [`UNSUPPORTED`].
//! The extra engines fill only the fields their own catalog can supply and leave
//! everything else at sensible defaults (0 / empty vec) rather than erroring.
//!
//! Design notes:
//! - All queries are parameterless catalog reads - no user input, no injection surface.
//! - Every section degrades gracefully: a catalog view that is missing or
//!   permission-denied yields empty rows / zeroed counters for that section rather
//!   than failing the whole command. The dashboard must never hard-fail because,
//!   e.g., `pg_stat_replication` requires a privileged role.

use super::connection::{require_conn, ActiveConnection, ClickhouseConfig, DbState};
use super::query::SqlResult;
use serde::Serialize;
use serde_json::{Map, Value};
use sqlx::{Column, MySqlPool, PgPool, Row, SqlitePool};
use std::collections::HashMap;
use tauri::State;

/// Returned when Instance Insights is requested on an unsupported engine.
const UNSUPPORTED: &str =
    "Instance Insights is only available for PostgreSQL, MySQL, SQLite, ClickHouse, and DuckDB";

// ── Row → JSON object helpers ──────────────────────────────────────────────────

/// Build a JSON object per row, keyed by the (aliased) SQL column names, using the
/// engine-specific cell decoder so all catalog types serialize cleanly.
fn pg_rows_to_objects(rows: &[sqlx::postgres::PgRow]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            let mut obj = Map::new();
            for (i, col) in row.columns().iter().enumerate() {
                obj.insert(col.name().to_string(), super::query::cell_to_json(row, i));
            }
            Value::Object(obj)
        })
        .collect()
}

fn mysql_rows_to_objects(rows: &[sqlx::mysql::MySqlRow]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            let mut obj = Map::new();
            for (i, col) in row.columns().iter().enumerate() {
                obj.insert(col.name().to_string(), super::mysql::cell_to_json(row, i));
            }
            Value::Object(obj)
        })
        .collect()
}

/// Run a catalog query and return one JSON object per row, or an empty vec if the
/// query fails (missing view / insufficient privilege). Never propagates the error.
async fn pg_query_objects(pool: &PgPool, sql: &str) -> Vec<Value> {
    match sqlx::query(sql).fetch_all(pool).await {
        Ok(rows) => pg_rows_to_objects(&rows),
        Err(_) => Vec::new(),
    }
}

async fn mysql_query_objects(pool: &MySqlPool, sql: &str) -> Vec<Value> {
    match sqlx::query(sql).fetch_all(pool).await {
        Ok(rows) => mysql_rows_to_objects(&rows),
        Err(_) => Vec::new(),
    }
}

// ── Extra-engine helpers (SQLite / ClickHouse / DuckDB) ─────────────────────────
// SQLite talks sqlx directly; ClickHouse and DuckDB return the shared `SqlResult`
// (columns + `Vec<Vec<Value>>` rows) from their own driver modules, so these
// helpers read cells positionally by column name off that shape.

/// A JSON cell rendered as a plain string - unwrap the `String` variant (no
/// surrounding quotes) and map null to empty.
fn json_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Best-effort integer read. ClickHouse's JSONCompact encodes 64-bit ints as
/// quoted strings, so accept number / string / float forms alike.
fn json_i64(v: &Value) -> i64 {
    v.as_i64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()))
        .or_else(|| v.as_f64().map(|f| f as i64))
        .unwrap_or(0)
}

/// Index of a result column by (case-insensitive) name.
fn col_pos(r: &SqlResult, name: &str) -> Option<usize> {
    r.columns.iter().position(|c| c.name.eq_ignore_ascii_case(name))
}

/// First cell of the first row as a string (scalar SELECTs like `version()`).
fn sqlresult_scalar(r: &SqlResult) -> String {
    r.rows.first().and_then(|row| row.first()).map(json_string).unwrap_or_default()
}

/// Turn a `SqlResult` (ClickHouse / DuckDB shape) into one JSON object per row,
/// keyed by column name - the generic row shape the State tab renders.
fn sqlresult_to_objects(r: &SqlResult) -> Vec<Value> {
    r.rows
        .iter()
        .map(|row| {
            let mut obj = Map::new();
            for (i, col) in r.columns.iter().enumerate() {
                obj.insert(col.name.clone(), row.get(i).cloned().unwrap_or(Value::Null));
            }
            Value::Object(obj)
        })
        .collect()
}

/// Build `ConfigSetting` rows from a settings-style result set, resolving each
/// field by column name (missing columns collapse to empty). Rows with no name
/// are dropped.
fn settings_to_config(
    r: &SqlResult,
    name_c: &str,
    value_c: &str,
    desc_c: &str,
    category_c: &str,
) -> Vec<ConfigSetting> {
    let ni = col_pos(r, name_c);
    let vi = col_pos(r, value_c);
    let di = col_pos(r, desc_c);
    let ci = col_pos(r, category_c);
    r.rows
        .iter()
        .filter_map(|row| {
            let name = ni.and_then(|i| row.get(i)).map(json_string).filter(|s| !s.is_empty())?;
            Some(ConfigSetting {
                name,
                category: ci.and_then(|i| row.get(i)).map(json_string).unwrap_or_default(),
                value: vi.and_then(|i| row.get(i)).map(json_string).unwrap_or_default(),
                description: di.and_then(|i| row.get(i)).map(json_string).unwrap_or_default(),
                ..Default::default()
            })
        })
        .collect()
}

/// One synthetic config row - surfaces size/storage snapshots that have no
/// dedicated struct field (SQLite DB size, ClickHouse parts, DuckDB DB size).
fn cfg_row(name: &str, value: impl Into<String>, unit: &str, category: &str) -> ConfigSetting {
    ConfigSetting {
        name: name.into(),
        category: category.into(),
        value: value.into(),
        unit: unit.into(),
        ..Default::default()
    }
}

/// A zeroed activity snapshot for embedded engines that expose no session /
/// counter catalogs (SQLite, DuckDB).
fn empty_activity(engine: &str) -> InstanceActivity {
    InstanceActivity {
        engine: engine.into(),
        sessions: SessionCounts { total: 0, active: 0, idle: 0, max: 0, usage_pct: 0.0 },
        counters: zero_counters(),
        buffer_hit_ratio: 0.0,
    }
}

// ── SQLite ─────────────────────────────────────────────────────────────────────

/// Read a numeric PRAGMA. `name` is a hard-coded identifier, never user input.
async fn sqlite_pragma_i64(pool: &SqlitePool, name: &str) -> i64 {
    sqlx::query(&format!("PRAGMA {name}"))
        .fetch_one(pool)
        .await
        .ok()
        .and_then(|r| r.try_get::<i64, _>(0).ok())
        .unwrap_or(0)
}

/// Read a textual PRAGMA (e.g. `journal_mode`).
async fn sqlite_pragma_str(pool: &SqlitePool, name: &str) -> String {
    sqlx::query(&format!("PRAGMA {name}"))
        .fetch_one(pool)
        .await
        .ok()
        .and_then(|r| r.try_get::<String, _>(0).ok())
        .unwrap_or_default()
}

/// SQLite exposes its knobs as PRAGMAs, mapped 1:1 onto `ConfigSetting` rows plus
/// a derived on-disk size (page_count * page_size).
async fn sqlite_config(pool: &SqlitePool) -> Vec<ConfigSetting> {
    let page_count = sqlite_pragma_i64(pool, "page_count").await;
    let page_size = sqlite_pragma_i64(pool, "page_size").await;
    let cache_size = sqlite_pragma_i64(pool, "cache_size").await;
    let freelist_count = sqlite_pragma_i64(pool, "freelist_count").await;
    let journal_mode = sqlite_pragma_str(pool, "journal_mode").await;
    let db_size = page_count.saturating_mul(page_size);
    vec![
        cfg_row("page_count", page_count.to_string(), "pages", "storage"),
        cfg_row("page_size", page_size.to_string(), "bytes", "storage"),
        cfg_row("cache_size", cache_size.to_string(), "pages", "cache"),
        cfg_row("freelist_count", freelist_count.to_string(), "pages", "storage"),
        cfg_row("journal_mode", journal_mode, "", "journal"),
        cfg_row("database_size", db_size.to_string(), "bytes", "storage"),
    ]
}

// ── ClickHouse ───────────────────────────────────────────────────────────────

/// Fetch a `metric → value` gauge map from a `system.metrics`-style query.
async fn ch_metric_map(cfg: &ClickhouseConfig, sql: &str) -> HashMap<String, i64> {
    let mut map = HashMap::new();
    if let Ok(r) = crate::db::clickhouse::query(cfg, sql).await {
        if let (Some(ni), Some(vi)) = (col_pos(&r, "metric"), col_pos(&r, "value")) {
            for row in &r.rows {
                if let Some(name) = row.get(ni).and_then(|v| v.as_str()) {
                    map.insert(name.to_string(), row.get(vi).map(json_i64).unwrap_or(0));
                }
            }
        }
    }
    map
}

/// Run a ClickHouse catalog read and shape it into row objects, empty on failure.
async fn ch_objects(cfg: &ClickhouseConfig, sql: &str) -> Vec<Value> {
    match crate::db::clickhouse::query(cfg, sql).await {
        Ok(r) => sqlresult_to_objects(&r),
        Err(_) => Vec::new(),
    }
}

/// Sessions map onto the live connection / running-query gauges in
/// `system.metrics`. ClickHouse has no per-database transaction/tuple/block
/// counters that fit the PostgreSQL counter model, so those stay zeroed.
async fn clickhouse_activity(cfg: &ClickhouseConfig) -> InstanceActivity {
    let m = ch_metric_map(cfg, "SELECT metric, value FROM system.metrics").await;
    let g = |k: &str| m.get(k).copied().unwrap_or(0);
    let active = g("Query");
    let total =
        g("TCPConnection") + g("HTTPConnection") + g("MySQLConnection") + g("PostgreSQLConnection");
    let idle = (total - active).max(0);
    InstanceActivity {
        engine: "clickhouse".into(),
        sessions: SessionCounts { total, active, idle, max: 0, usage_pct: 0.0 },
        counters: zero_counters(),
        buffer_hit_ratio: 0.0,
    }
}

// ══ 1. Version ═════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceVersion {
    /// "postgres" | "mysql" | "sqlite" | "clickhouse" | "duckdb"
    pub engine: String,
    pub version: String,
}

/// Cheap engine + version lookup so the dashboard header can render immediately.
pub async fn instance_version(state: State<'_, DbState>) -> Result<InstanceVersion, String> {
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => {
            let version = sqlx::query("SHOW server_version")
                .fetch_one(&pool)
                .await
                .ok()
                .and_then(|r| r.try_get::<String, _>(0).ok())
                .unwrap_or_default();
            Ok(InstanceVersion { engine: "postgres".into(), version })
        }
        ActiveConnection::Mysql(pool) => {
            let version = sqlx::query("SELECT VERSION()")
                .fetch_one(&pool)
                .await
                .ok()
                .and_then(|r| r.try_get::<String, _>(0).ok())
                .unwrap_or_default();
            Ok(InstanceVersion { engine: "mysql".into(), version })
        }
        ActiveConnection::Sqlite(pool) => {
            let version = sqlx::query("SELECT sqlite_version()")
                .fetch_one(&pool)
                .await
                .ok()
                .and_then(|r| r.try_get::<String, _>(0).ok())
                .unwrap_or_default();
            Ok(InstanceVersion { engine: "sqlite".into(), version })
        }
        ActiveConnection::Clickhouse(cfg) => {
            let version = crate::db::clickhouse::query(&cfg, "SELECT version()")
                .await
                .ok()
                .map(|r| sqlresult_scalar(&r))
                .unwrap_or_default();
            Ok(InstanceVersion { engine: "clickhouse".into(), version })
        }
        ActiveConnection::Duckdb(handle) => {
            let version = crate::db::duckdb::execute_sql(&handle, "SELECT version()")
                .await
                .ok()
                .map(|r| sqlresult_scalar(&r))
                .unwrap_or_default();
            Ok(InstanceVersion { engine: "duckdb".into(), version })
        }
        _ => Err(UNSUPPORTED.into()),
    }
}

// ══ 2. Activity ════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCounts {
    pub total: i64,
    pub active: i64,
    pub idle: i64,
    pub max: i64,
    /// total / max * 100, clamped to 0 when max is unknown.
    pub usage_pct: f64,
}

/// Cumulative counters. The frontend diffs successive snapshots to derive
/// per-second rates, so these are raw monotonic totals - do not reset them here.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityCounters {
    pub commits: i64,
    pub rollbacks: i64,
    pub tup_updated: i64,
    pub tup_deleted: i64,
    pub tup_fetched: i64,
    pub tup_returned: i64,
    pub blks_read: i64,
    pub blks_hit: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceActivity {
    pub engine: String,
    pub sessions: SessionCounts,
    pub counters: ActivityCounters,
    /// blks_hit / (blks_hit + blks_read); 0 when there has been no I/O yet.
    pub buffer_hit_ratio: f64,
}

fn hit_ratio(blks_hit: i64, blks_read: i64) -> f64 {
    let total = blks_hit + blks_read;
    if total > 0 {
        blks_hit as f64 / total as f64
    } else {
        0.0
    }
}

fn usage_pct(total: i64, max: i64) -> f64 {
    if max > 0 {
        total as f64 / max as f64 * 100.0
    } else {
        0.0
    }
}

pub async fn instance_activity(state: State<'_, DbState>) -> Result<InstanceActivity, String> {
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => Ok(pg_activity(&pool).await),
        ActiveConnection::Mysql(pool) => Ok(mysql_activity(&pool).await),
        // SQLite / DuckDB are embedded: no session table, no cumulative counters.
        ActiveConnection::Sqlite(_) => Ok(empty_activity("sqlite")),
        ActiveConnection::Clickhouse(cfg) => Ok(clickhouse_activity(&cfg).await),
        ActiveConnection::Duckdb(_) => Ok(empty_activity("duckdb")),
        _ => Err(UNSUPPORTED.into()),
    }
}

async fn pg_activity(pool: &PgPool) -> InstanceActivity {
    // Sessions - count total / active / idle from pg_stat_activity.
    let (total, active, idle) = match sqlx::query(
        "SELECT count(*) AS total,
                count(*) FILTER (WHERE state = 'active') AS active,
                count(*) FILTER (WHERE state = 'idle')   AS idle
         FROM pg_stat_activity",
    )
    .fetch_one(pool)
    .await
    {
        Ok(r) => (
            r.try_get::<i64, _>("total").unwrap_or(0),
            r.try_get::<i64, _>("active").unwrap_or(0),
            r.try_get::<i64, _>("idle").unwrap_or(0),
        ),
        Err(_) => (0, 0, 0),
    };

    let max = sqlx::query("SHOW max_connections")
        .fetch_one(pool)
        .await
        .ok()
        .and_then(|r| r.try_get::<String, _>(0).ok())
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(0);

    // Cumulative counters aggregated across all databases.
    let counters = match sqlx::query(
        "SELECT COALESCE(sum(xact_commit),0)::bigint   AS commits,
                COALESCE(sum(xact_rollback),0)::bigint AS rollbacks,
                COALESCE(sum(tup_updated),0)::bigint   AS tup_updated,
                COALESCE(sum(tup_deleted),0)::bigint   AS tup_deleted,
                COALESCE(sum(tup_fetched),0)::bigint   AS tup_fetched,
                COALESCE(sum(tup_returned),0)::bigint  AS tup_returned,
                COALESCE(sum(blks_read),0)::bigint     AS blks_read,
                COALESCE(sum(blks_hit),0)::bigint      AS blks_hit
         FROM pg_stat_database",
    )
    .fetch_one(pool)
    .await
    {
        Ok(r) => ActivityCounters {
            commits: r.try_get("commits").unwrap_or(0),
            rollbacks: r.try_get("rollbacks").unwrap_or(0),
            tup_updated: r.try_get("tup_updated").unwrap_or(0),
            tup_deleted: r.try_get("tup_deleted").unwrap_or(0),
            tup_fetched: r.try_get("tup_fetched").unwrap_or(0),
            tup_returned: r.try_get("tup_returned").unwrap_or(0),
            blks_read: r.try_get("blks_read").unwrap_or(0),
            blks_hit: r.try_get("blks_hit").unwrap_or(0),
        },
        Err(_) => zero_counters(),
    };

    let buffer_hit_ratio = hit_ratio(counters.blks_hit, counters.blks_read);
    InstanceActivity {
        engine: "postgres".into(),
        sessions: SessionCounts { total, active, idle, max, usage_pct: usage_pct(total, max) },
        counters,
        buffer_hit_ratio,
    }
}

async fn mysql_activity(pool: &MySqlPool) -> InstanceActivity {
    // Sessions - 'Sleep' command == idle connection.
    let (total, active, idle) = match sqlx::query(
        "SELECT COUNT(*) AS total,
                CAST(SUM(COMMAND <> 'Sleep') AS SIGNED) AS active,
                CAST(SUM(COMMAND =  'Sleep') AS SIGNED) AS idle
         FROM information_schema.PROCESSLIST",
    )
    .fetch_one(pool)
    .await
    {
        Ok(r) => (
            r.try_get::<i64, _>("total").unwrap_or(0),
            r.try_get::<Option<i64>, _>("active").ok().flatten().unwrap_or(0),
            r.try_get::<Option<i64>, _>("idle").ok().flatten().unwrap_or(0),
        ),
        Err(_) => (0, 0, 0),
    };

    let max = match sqlx::query("SHOW VARIABLES LIKE 'max_connections'")
        .fetch_one(pool)
        .await
    {
        // my_text: SHOW STATUS values can arrive VARBINARY on MySQL 8+.
        Ok(r) => super::mysql::my_text(&r, 1)
            .and_then(|s| s.trim().parse::<i64>().ok())
            .unwrap_or(0),
        Err(_) => 0,
    };

    // Global status counters → name/value map.
    let status = mysql_status_map(
        pool,
        &[
            "Com_commit",
            "Com_rollback",
            "Innodb_rows_updated",
            "Innodb_rows_deleted",
            "Innodb_rows_read",
            "Innodb_buffer_pool_read_requests",
            "Innodb_buffer_pool_reads",
        ],
    )
    .await;
    let g = |k: &str| -> i64 { status.get(k).copied().unwrap_or(0) };

    // InnoDB exposes logical read requests and physical (disk) reads. Map onto the
    // PostgreSQL buffer-cache model: hits = requests - physical reads (pages served
    // from the pool), reads = physical reads (pages fetched from disk). MySQL has no
    // separate "returned" tuple counter, so tup_returned mirrors rows_read.
    let read_requests = g("Innodb_buffer_pool_read_requests");
    let disk_reads = g("Innodb_buffer_pool_reads");
    let blks_hit = (read_requests - disk_reads).max(0);
    let counters = ActivityCounters {
        commits: g("Com_commit"),
        rollbacks: g("Com_rollback"),
        tup_updated: g("Innodb_rows_updated"),
        tup_deleted: g("Innodb_rows_deleted"),
        tup_fetched: g("Innodb_rows_read"),
        tup_returned: g("Innodb_rows_read"),
        blks_read: disk_reads,
        blks_hit,
    };

    let buffer_hit_ratio = hit_ratio(counters.blks_hit, counters.blks_read);
    InstanceActivity {
        engine: "mysql".into(),
        sessions: SessionCounts { total, active, idle, max, usage_pct: usage_pct(total, max) },
        counters,
        buffer_hit_ratio,
    }
}

fn zero_counters() -> ActivityCounters {
    ActivityCounters {
        commits: 0,
        rollbacks: 0,
        tup_updated: 0,
        tup_deleted: 0,
        tup_fetched: 0,
        tup_returned: 0,
        blks_read: 0,
        blks_hit: 0,
    }
}

/// Fetch the requested `SHOW GLOBAL STATUS` variables into a name→value map.
/// Values are string integers; unparseable / missing entries are simply absent.
async fn mysql_status_map(pool: &MySqlPool, names: &[&str]) -> HashMap<String, i64> {
    let list = names
        .iter()
        .map(|n| format!("'{n}'"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("SHOW GLOBAL STATUS WHERE Variable_name IN ({list})");
    let mut map = HashMap::new();
    if let Ok(rows) = sqlx::query(&sql).fetch_all(pool).await {
        for r in &rows {
            let name = super::mysql::my_text(r, 0).unwrap_or_default();
            let value = super::mysql::my_text(r, 1)
                .and_then(|s| s.trim().parse::<i64>().ok())
                .unwrap_or(0);
            if !name.is_empty() {
                map.insert(name, value);
            }
        }
    }
    map
}

// ══ 3. State ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceState {
    pub engine: String,
    pub sessions: Vec<Value>,
    pub locks: Vec<Value>,
    pub prepared_transactions: Vec<Value>,
}

pub async fn instance_state(state: State<'_, DbState>) -> Result<InstanceState, String> {
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => {
            let sessions = pg_query_objects(
                &pool,
                "SELECT pid,
                        usename          AS \"user\",
                        application_name AS application,
                        client_addr::text AS client,
                        state,
                        wait_event,
                        pg_blocking_pids(pid) AS blocking_pids
                 FROM pg_stat_activity
                 ORDER BY pid
                 LIMIT 500",
            )
            .await;
            let locks = pg_query_objects(
                &pool,
                "SELECT l.locktype       AS lock_type,
                        c.relname::text  AS target_relation
                 FROM pg_locks l
                 LEFT JOIN pg_class c ON c.oid = l.relation
                 LIMIT 200",
            )
            .await;
            let prepared_transactions =
                pg_query_objects(&pool, "SELECT * FROM pg_prepared_xacts LIMIT 200").await;
            Ok(InstanceState { engine: "postgres".into(), sessions, locks, prepared_transactions })
        }
        ActiveConnection::Mysql(pool) => {
            let sessions = mysql_query_objects(
                &pool,
                "SELECT ID AS id, USER AS user, HOST AS host, DB AS db,
                        COMMAND AS command, STATE AS state, INFO AS info
                 FROM information_schema.PROCESSLIST
                 ORDER BY ID
                 LIMIT 500",
            )
            .await;
            // performance_schema.data_locks may be disabled or unavailable → empty.
            let locks = mysql_query_objects(
                &pool,
                "SELECT * FROM performance_schema.data_locks LIMIT 200",
            )
            .await;
            // MySQL has no queryable prepared-transaction catalog (XA RECOVER only).
            Ok(InstanceState {
                engine: "mysql".into(),
                sessions,
                locks,
                prepared_transactions: Vec::new(),
            })
        }
        // SQLite: no session / lock / prepared-txn catalog is queryable.
        ActiveConnection::Sqlite(_) => Ok(InstanceState {
            engine: "sqlite".into(),
            sessions: Vec::new(),
            locks: Vec::new(),
            prepared_transactions: Vec::new(),
        }),
        // ClickHouse: system.processes is the running-query analog of sessions;
        // there is no user-lock or prepared-transaction catalog.
        ActiveConnection::Clickhouse(cfg) => {
            let sessions = ch_objects(
                &cfg,
                "SELECT query_id, user, toString(address) AS client, elapsed, \
                        read_rows, memory_usage, query \
                 FROM system.processes \
                 LIMIT 500",
            )
            .await;
            Ok(InstanceState {
                engine: "clickhouse".into(),
                sessions,
                locks: Vec::new(),
                prepared_transactions: Vec::new(),
            })
        }
        // DuckDB is embedded: no session / lock / prepared-txn catalog.
        ActiveConnection::Duckdb(_) => Ok(InstanceState {
            engine: "duckdb".into(),
            sessions: Vec::new(),
            locks: Vec::new(),
            prepared_transactions: Vec::new(),
        }),
        _ => Err(UNSUPPORTED.into()),
    }
}

// ══ 4. Config ══════════════════════════════════════════════════════════════════

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSetting {
    pub name: String,
    pub category: String,
    pub value: String,
    pub unit: String,
    pub requires_restart: bool,
    pub description: String,
    // ── Editing metadata ──────────────────────────────────────────────────────
    // Filled from `pg_settings` so the UI can render the right control (toggle /
    // select / number / text) and tell "you may change this" from "the server
    // will refuse". Engines without an equivalent catalog leave these empty and
    // the UI falls back to a plain text field.
    /// `internal` | `postmaster` | `sighup` | `superuser` | `user` | …
    pub context: String,
    /// `bool` | `enum` | `integer` | `real` | `string`
    pub vartype: String,
    /// Allowed values for `vartype = 'enum'`.
    pub enum_vals: Vec<String>,
    pub min_val: String,
    pub max_val: String,
    /// Compiled-in default - what a reset returns to.
    pub boot_val: String,
    /// Value a session reset would see (config file value, not the session override).
    pub reset_val: String,
    /// Where the current value came from: `default`, `configuration file`, …
    pub source: String,
    /// Already changed on disk, waiting for a server restart.
    pub pending_restart: bool,
    /// False for values the engine will never let us write (`context = internal`).
    pub editable: bool,
}

pub async fn instance_config(state: State<'_, DbState>) -> Result<Vec<ConfigSetting>, String> {
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => {
            let rows = sqlx::query(
                "SELECT name,
                        category,
                        setting AS value,
                        unit,
                        (context = 'postmaster') AS requires_restart,
                        short_desc AS description,
                        context,
                        vartype,
                        enumvals,
                        min_val,
                        max_val,
                        boot_val,
                        reset_val,
                        source,
                        pending_restart
                 FROM pg_settings
                 ORDER BY name",
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default();
            let opt = |r: &sqlx::postgres::PgRow, c: &str| -> String {
                r.try_get::<Option<String>, _>(c).ok().flatten().unwrap_or_default()
            };
            Ok(rows
                .iter()
                .map(|r| {
                    let context = opt(r, "context");
                    ConfigSetting {
                        name: r.try_get::<String, _>("name").unwrap_or_default(),
                        category: opt(r, "category"),
                        value: opt(r, "value"),
                        unit: opt(r, "unit"),
                        requires_restart: r
                            .try_get::<Option<bool>, _>("requires_restart")
                            .ok()
                            .flatten()
                            .unwrap_or(false),
                        description: opt(r, "description"),
                        // `internal` values are compiled in (block size, wal segment
                        // size) - the server rejects every attempt to set them.
                        editable: context != "internal",
                        context,
                        vartype: opt(r, "vartype"),
                        enum_vals: r
                            .try_get::<Option<Vec<String>>, _>("enumvals")
                            .ok()
                            .flatten()
                            .unwrap_or_default(),
                        min_val: opt(r, "min_val"),
                        max_val: opt(r, "max_val"),
                        boot_val: opt(r, "boot_val"),
                        reset_val: opt(r, "reset_val"),
                        source: opt(r, "source"),
                        pending_restart: r
                            .try_get::<Option<bool>, _>("pending_restart")
                            .ok()
                            .flatten()
                            .unwrap_or(false),
                    }
                })
                .collect())
        }
        ActiveConnection::Mysql(pool) => {
            let rows = sqlx::query("SHOW VARIABLES").fetch_all(&pool).await.unwrap_or_default();
            Ok(rows
                .iter()
                .map(|r| ConfigSetting {
                    name: r.try_get::<String, _>(0).unwrap_or_default(),
                    value: r.try_get::<String, _>(1).unwrap_or_default(),
                    editable: true,
                    ..Default::default()
                })
                .collect())
        }
        // SQLite: the tuning PRAGMAs plus a derived on-disk size.
        ActiveConnection::Sqlite(pool) => Ok(sqlite_config(&pool).await),
        // ClickHouse: system.settings is the pg_settings analog; append a storage
        // snapshot (part count + bytes on disk) from system.parts.
        ActiveConnection::Clickhouse(cfg) => {
            let mut out = match crate::db::clickhouse::query(
                &cfg,
                "SELECT * FROM system.settings ORDER BY name",
            )
            .await
            {
                Ok(r) => settings_to_config(&r, "name", "value", "description", ""),
                Err(_) => Vec::new(),
            };
            if let Ok(r) = crate::db::clickhouse::query(
                &cfg,
                "SELECT count() AS parts, sum(bytes_on_disk) AS bytes FROM system.parts",
            )
            .await
            {
                if let Some(row) = r.rows.first() {
                    let parts = col_pos(&r, "parts").and_then(|i| row.get(i)).map(json_i64).unwrap_or(0);
                    let bytes = col_pos(&r, "bytes").and_then(|i| row.get(i)).map(json_i64).unwrap_or(0);
                    out.push(cfg_row("system.parts.count", parts.to_string(), "parts", "storage"));
                    out.push(cfg_row("system.parts.bytes_on_disk", bytes.to_string(), "bytes", "storage"));
                }
            }
            Ok(out)
        }
        // DuckDB: duckdb_settings() (scope → category); append the database size.
        ActiveConnection::Duckdb(handle) => {
            let mut out = match crate::db::duckdb::execute_sql(&handle, "SELECT * FROM duckdb_settings()").await {
                Ok(r) => settings_to_config(&r, "name", "value", "description", "scope"),
                Err(_) => Vec::new(),
            };
            if let Ok(r) = crate::db::duckdb::execute_sql(&handle, "SELECT * FROM pragma_database_size()").await {
                if let Some(row) = r.rows.first() {
                    if let Some(v) = col_pos(&r, "database_size").and_then(|i| row.get(i)) {
                        out.push(cfg_row("database_size", json_string(v), "", "storage"));
                    }
                }
            }
            Ok(out)
        }
        _ => Err(UNSUPPORTED.into()),
    }
}

// ── Writing config ─────────────────────────────────────────────────────────────

/// Outcome of a single setting change, phrased for the UI to show verbatim.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetConfigResult {
    pub name: String,
    /// Value the server reports *after* the change (unchanged when a restart is pending).
    pub value: String,
    /// The write landed but only takes effect after a server restart.
    pub requires_restart: bool,
    /// `pg_reload_conf()` ran, so running backends already picked the value up.
    pub reloaded: bool,
    pub message: String,
}

/// GUC / system-variable names are `[a-z_][a-z0-9_]*`, optionally namespaced
/// (`auto_explain.log_min_duration`). Everything else is rejected before it can
/// reach a statement - `ALTER SYSTEM` / `SET GLOBAL` take no bind parameters, so
/// the name is the one part that must be proven safe rather than escaped.
fn valid_setting_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
}

/// Single-quoted SQL literal. Values are free-form (`archive_command` is a shell
/// line), so they are escaped rather than validated.
fn sql_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Change one server setting.
///
/// Postgres writes through `ALTER SYSTEM` (persisted in `postgresql.auto.conf`)
/// and then reloads, so `sighup`-context values apply immediately and
/// `postmaster` ones are reported as restart-pending. `value = None` resets the
/// setting to its default. Both require the connected role to be superuser (or
/// hold `pg_write_all_settings`) - the server's own error is passed straight
/// through when it is not.
pub async fn instance_set_config(
    state: State<'_, DbState>,
    name: String,
    value: Option<String>,
) -> Result<SetConfigResult, String> {
    let name = name.trim().to_string();
    if !valid_setting_name(&name) {
        return Err(format!("'{name}' is not a valid setting name"));
    }

    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => {
            // Read the catalog first: it decides whether the setting can be
            // written at all, and whether the change needs a restart.
            let meta = sqlx::query(
                "SELECT context, vartype FROM pg_settings WHERE name = $1",
            )
            .bind(&name)
            .fetch_optional(&pool)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("No such setting: {name}"))?;

            let context: String = meta.try_get("context").unwrap_or_default();
            if context == "internal" {
                return Err(format!(
                    "{name} is compiled into the server and cannot be changed at runtime"
                ));
            }

            let stmt = match &value {
                Some(v) => format!("ALTER SYSTEM SET \"{name}\" = {}", sql_literal(v)),
                None => format!("ALTER SYSTEM RESET \"{name}\""),
            };
            sqlx::query(&stmt).execute(&pool).await.map_err(|e| e.to_string())?;

            // Reload so sighup-context settings take effect without a restart.
            let reloaded = sqlx::query("SELECT pg_reload_conf()").execute(&pool).await.is_ok();

            let current: String = sqlx::query("SELECT setting FROM pg_settings WHERE name = $1")
                .bind(&name)
                .fetch_optional(&pool)
                .await
                .ok()
                .flatten()
                .and_then(|r| r.try_get::<String, _>("setting").ok())
                .unwrap_or_default();

            let requires_restart = context == "postmaster";
            let message = if value.is_none() {
                format!("{name} reset to its default")
            } else if requires_restart {
                format!("{name} written to postgresql.auto.conf - restart the server to apply it")
            } else if reloaded {
                format!("{name} is now {current}")
            } else {
                format!("{name} written, but the config reload failed - reload manually")
            };

            Ok(SetConfigResult { name, value: current, requires_restart, reloaded, message })
        }
        ActiveConnection::Mysql(pool) => {
            // Numbers go through unquoted (MySQL rejects '128M'-style strings for
            // some numeric variables); anything else is a quoted literal.
            let rendered = match &value {
                None => "DEFAULT".to_string(),
                Some(v) if v.parse::<f64>().is_ok() => v.clone(),
                Some(v) => sql_literal(v),
            };

            // MySQL 8 persists across restarts; older servers only have SET GLOBAL.
            let persisted = sqlx::query(&format!("SET PERSIST `{name}` = {rendered}"))
                .execute(&pool)
                .await
                .is_ok();
            if !persisted {
                sqlx::query(&format!("SET GLOBAL `{name}` = {rendered}"))
                    .execute(&pool)
                    .await
                    .map_err(|e| e.to_string())?;
            }

            let current: String = sqlx::query("SHOW VARIABLES LIKE ?")
                .bind(&name)
                .fetch_optional(&pool)
                .await
                .ok()
                .flatten()
                .and_then(|r| r.try_get::<String, _>(1).ok())
                .unwrap_or_default();

            let message = if value.is_none() {
                format!("{name} reset to its default")
            } else if persisted {
                format!("{name} is now {current} (persisted)")
            } else {
                format!("{name} is now {current} - not persisted, it resets on restart")
            };

            Ok(SetConfigResult {
                name,
                value: current,
                requires_restart: false,
                reloaded: true,
                message,
            })
        }
        _ => Err("Changing configuration is only supported on PostgreSQL and MySQL".into()),
    }
}

// ══ 5. Replication ═════════════════════════════════════════════════════════════

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceReplication {
    pub engine: String,
    pub stats: Vec<Value>,
    pub slots: Vec<Value>,
}

pub async fn instance_replication(state: State<'_, DbState>) -> Result<InstanceReplication, String> {
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => {
            let stats = pg_query_objects(
                &pool,
                "SELECT pid,
                        client_addr::text AS client_addr,
                        application_name,
                        state,
                        sync_state,
                        reply_time,
                        write_lag::text  AS write_lag,
                        flush_lag::text  AS flush_lag,
                        replay_lag::text AS replay_lag
                 FROM pg_stat_replication",
            )
            .await;
            let slots = pg_query_objects(&pool, "SELECT * FROM pg_replication_slots").await;
            Ok(InstanceReplication { engine: "postgres".into(), stats, slots })
        }
        ActiveConnection::Mysql(pool) => {
            // MySQL 8.0.22+ renamed SHOW SLAVE STATUS → SHOW REPLICA STATUS. Try the
            // new name, fall back to the legacy one, and if neither applies (not a
            // replica / not permitted) return empty rather than erroring.
            let stats = match sqlx::query("SHOW REPLICA STATUS").fetch_all(&pool).await {
                Ok(rows) => mysql_rows_to_objects(&rows),
                Err(_) => match sqlx::query("SHOW SLAVE STATUS").fetch_all(&pool).await {
                    Ok(rows) => mysql_rows_to_objects(&rows),
                    Err(_) => Vec::new(),
                },
            };
            // No slot concept in MySQL replication.
            Ok(InstanceReplication { engine: "mysql".into(), stats, slots: Vec::new() })
        }
        // No replication catalog these engines can surface here: SQLite/DuckDB are
        // embedded, and ClickHouse replication (system.replicas) only exists for
        // ReplicatedMergeTree tables and is out of scope - return empty, not error.
        ActiveConnection::Sqlite(_) => {
            Ok(InstanceReplication { engine: "sqlite".into(), stats: Vec::new(), slots: Vec::new() })
        }
        ActiveConnection::Clickhouse(_) => Ok(InstanceReplication {
            engine: "clickhouse".into(),
            stats: Vec::new(),
            slots: Vec::new(),
        }),
        ActiveConnection::Duckdb(_) => {
            Ok(InstanceReplication { engine: "duckdb".into(), stats: Vec::new(), slots: Vec::new() })
        }
        _ => Err(UNSUPPORTED.into()),
    }
}
