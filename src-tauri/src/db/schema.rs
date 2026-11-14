use super::connection::{require_conn, ActiveConnection, AnyConnectionConfig, DbState};
use serde::Serialize;
use sqlx::{MySqlPool, PgPool, Row};
use tauri::State;

use super::mysql::{my_int, my_text, my_text_named};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub name: String,
    pub row_count: i64,
    /// "table" | "view" | "materialized_view" | "foreign_table"
    pub kind: String,
    /// Row-level security enabled (PostgreSQL only; None for other backends)
    pub rls_enabled: Option<bool>,
    /// When the table was created, where the engine records it (MySQL/MariaDB
    /// CREATE_TIME, SQL Server create_date), as "YYYY-MM-DD HH:MM:SS".
    pub created_at: Option<String>,
    /// Creation order where no timestamp is kept: the Postgres OID, the rowid
    /// of the SQLite catalog row, the DuckDB object id. Larger is newer. For
    /// sorting only; the numbers mean nothing on their own.
    pub create_order: Option<i64>,
    /// The engine's own object rather than the user's (Oracle's
    /// ORACLE_MAINTAINED). None where the engine doesn't say; the sidebar then
    /// goes by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    pub name: String,
    pub table_name: String,
    /// Comma-separated column names (empty for SQLite/D1)
    pub columns: String,
    /// "btree", "hash", "gist", "gin", "brin", etc.
    pub index_type: String,
    pub is_unique: bool,
    pub is_primary: bool,
    /// Partial index WHERE clause expression, if any
    pub condition: Option<String>,
    /// Index comment
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnStructureRow {
    pub ordinal_position: i32,
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
    pub column_default: Option<String>,
    pub foreign_key: Option<String>,
    pub fk_constraint_name: Option<String>,
    pub comment: Option<String>,
    /// Part of the table's primary key. From the catalog, never guessed from a
    /// name: `id TEXT PRIMARY KEY` on SQLite reports as nullable, and a `uuid`
    /// key has no `nextval` default, so neither heuristic found them.
    pub is_primary_key: bool,
}

/// One table's column list, as returned by the schema-wide structure fetch.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableColumnStructure {
    pub table: String,
    pub columns: Vec<ColumnStructureRow>,
}

pub(crate) fn validate_ident(name: &str) -> Result<(), String> {
    // Disallow `.` - every caller passes a single schema/table/column component
    // (never a `schema.table` string), so allowing `.` would let "a.b" collapse
    // into one literal quoted identifier and target the wrong object.
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!("Invalid identifier: {name}"));
    }
    Ok(())
}

// ── PostgreSQL ────────────────────────────────────────────────────────────────

const LIST_TABLES_SQL: &str = r#"
    SELECT
        c.relname::text AS name,
        CASE c.relkind
            WHEN 'r' THEN 'table'
            WHEN 'p' THEN 'table'
            WHEN 'v' THEN 'view'
            WHEN 'm' THEN 'materialized_view'
            WHEN 'f' THEN 'foreign_table'
            ELSE 'table'
        END AS kind,
        CASE
            WHEN c.relkind IN ('v', 'f') THEN -1
            -- Trust the stats/planner estimate ONLY for large tables, where an
            -- exact COUNT(*) would be a slow sequential scan. For everything
            -- else we return -1 (unknown) and resolve it with an exact COUNT(*)
            -- below, because estimates are unreliable in both directions for
            -- small tables: a stale reltuples reports rows for a table that was
            -- emptied (n_live_tup catches up slowly after a bulk DELETE), and a
            -- freshly created / bulk-loaded / restored table reports 0 before
            -- autovacuum or the stats collector has run. The 100k threshold
            -- mirrors ESTIMATE_THRESHOLD in query.rs::get_table_rows, where an
            -- exact count below it is sub-millisecond.
            WHEN GREATEST(COALESCE(s.n_live_tup, 0), c.reltuples::bigint) >= 100000
                THEN GREATEST(COALESCE(s.n_live_tup, 0), c.reltuples::bigint)
            ELSE -1
        END AS row_count,
        CASE WHEN c.relkind IN ('r', 'p') THEN c.relrowsecurity ELSE false END AS rls_enabled,
        -- Postgres keeps no creation time; OIDs are handed out in increasing
        -- order, so they sort tables by when they were created.
        c.oid::bigint AS create_order
    FROM pg_catalog.pg_class c
    JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
    LEFT JOIN pg_stat_user_tables s
        ON s.schemaname = n.nspname
        AND s.relname = c.relname
    WHERE n.nspname = $1
      AND c.relkind IN ('r', 'p', 'v', 'm', 'f')
      -- `IS NOT TRUE` (not `NOT x`) so partition-less tables survive on engines
      -- like CockroachDB where pg_class.relispartition is NULL rather than false.
      AND c.relispartition IS NOT TRUE
    ORDER BY c.relkind, c.relname
    "#;

const LIST_INDEXES_SQL: &str = r#"
    SELECT
        i.relname::text AS name,
        t.relname::text AS table_name,
        COALESCE(string_agg(a.attname, ', ' ORDER BY x.ordinality) FILTER (WHERE x.attnum > 0), '') AS columns,
        am.amname::text AS index_type,
        ix.indisunique AS is_unique,
        ix.indisprimary AS is_primary,
        MAX(pg_get_expr(ix.indpred, ix.indrelid)) AS condition,
        MAX(obj_description(i.oid, 'pg_class')) AS comment
    FROM pg_catalog.pg_index ix
    JOIN pg_catalog.pg_class i ON i.oid = ix.indexrelid
    JOIN pg_catalog.pg_class t ON t.oid = ix.indrelid
    JOIN pg_catalog.pg_namespace n ON n.oid = t.relnamespace
    JOIN pg_catalog.pg_am am ON am.oid = i.relam
    LEFT JOIN LATERAL unnest(ix.indkey) WITH ORDINALITY AS x(attnum, ordinality) ON TRUE
    LEFT JOIN pg_catalog.pg_attribute a
        ON a.attrelid = t.oid AND a.attnum = x.attnum AND x.attnum > 0
    WHERE n.nspname = $1
      AND t.relkind IN ('r', 'p', 'm')
    GROUP BY i.relname, i.oid, t.relname, am.amname, ix.indisunique, ix.indisprimary
    ORDER BY t.relname, ix.indisprimary DESC, i.relname
    "#;

/// How long one sidebar COUNT(*) may hold a pooled connection.
///
/// The session default is `statement_timeout = 10min` (see open_pg), which is
/// right for a query the user asked for and catastrophic for a background one:
/// `COUNT(*)` on a large production table is a full scan, so a handful of them
/// pin most of the pool for minutes. Interactive queries then can't get a
/// connection and fail with "pool timed out while waiting for an open
/// connection" - the app starving itself, which reads as the connection being
/// broken. A count that can't finish in this budget is not worth a sidebar
/// number; it degrades to "unknown" instead.
const COUNT_STATEMENT_TIMEOUT: &str = "4s";
/// Same budget in milliseconds, for engines whose knob takes ms (MySQL).
const COUNT_STATEMENT_TIMEOUT_MS: u64 = 4_000;

async fn exact_row_count(pool: &PgPool, schema: &str, table: &str) -> Result<i64, String> {
    // Names arrive from the frontend in the lazy-count pass - escape embedded
    // quotes so an unusual (or hostile) identifier can't break out of the quoting.
    let q = |id: &str| format!("\"{}\"", id.replace('"', "\"\""));
    let sql = format!("SELECT COUNT(*)::bigint FROM {}.{}", q(schema), q(table));

    // SET LOCAL needs a transaction; it reverts on rollback, so the pooled
    // connection goes back with the session's 10min default intact.
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| format!("Failed to count rows for {table}: {e}"))?;
    let timeout_sql = format!("SET LOCAL statement_timeout = '{COUNT_STATEMENT_TIMEOUT}'");
    sqlx::query(&timeout_sql)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("Failed to count rows for {table}: {e}"))?;
    let count: Result<i64, _> = sqlx::query_scalar(&sql).fetch_one(&mut *tx).await;
    // Read-only, so COMMIT is as cheap as ROLLBACK and resets SET LOCAL the same
    // way. Not ROLLBACK: Nile's proxy rejects it here, the connection comes back
    // marked in-transaction, sqlx closes it, and every row count on a Nile table
    // cost a fresh ~4s handshake.
    let _ = tx.commit().await;
    count.map_err(|e| format!("Failed to count rows for {table}: {e}"))
}

/// Bound on concurrent COUNT(*) queries in the lazy-count pass. Firing all of
/// them at once (join_all) on a large schema swamps the small desktop pool -
/// 70+ acquires queue for seconds each ("time to acquire exceeded slow
/// threshold") and starve interactive queries.
///
/// Deliberately BELOW the pool's max_connections (10, see open_pg) so the
/// background pass can never consume the whole pool: a table open bursts ~6
/// concurrent queries and must still find connections while counts are running.
const COUNT_CONCURRENCY: usize = 3;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableRowCount {
    pub name: String,
    pub row_count: i64,
}

/// Exact COUNT(*) for the given tables of the active connection.
///
/// `list_tables` returns immediately with estimates (-1 = unknown) so the
/// sidebar renders without waiting on counts; the UI then calls this in a
/// background pass and patches the counts in as they resolve. Engines whose
/// `list_tables` already returns exact counts inline (SQLite/D1/libSQL/…)
/// never report -1, so they return an empty list here.
pub async fn table_row_counts(
    state: State<'_, DbState>,
    schema: String,
    tables: Vec<String>,
) -> Result<Vec<TableRowCount>, String> {
    use futures::stream::StreamExt;
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => {
            let counts: Vec<TableRowCount> =
                futures::stream::iter(tables.into_iter().map(|name| {
                    let pool = pool.clone();
                    let schema = schema.clone();
                    async move {
                        // -1 (not 0) on failure: 0 is a real "empty" count and must
                        // not be faked when the COUNT actually errored, or a view
                        // whose count can't be resolved reads as genuinely empty.
                        let row_count = match exact_row_count(&pool, &schema, &name).await {
                            Ok(n) => n,
                            Err(e) => {
                                eprintln!("[row count] {schema}.{name}: {e}");
                                -1
                            }
                        };
                        TableRowCount { name, row_count }
                    }
                }))
                .buffer_unordered(COUNT_CONCURRENCY)
                .collect()
                .await;
            Ok(counts)
        }
        ActiveConnection::Mysql(pool) => {
            let counts: Vec<TableRowCount> =
                futures::stream::iter(tables.into_iter().map(|name| {
                    let pool = pool.clone();
                    let schema = schema.clone();
                    async move {
                        let row_count = match mysql_exact_row_count(&pool, &schema, &name).await {
                            Ok(n) => n,
                            Err(e) => {
                                eprintln!("[row count] {schema}.{name}: {e}");
                                -1
                            }
                        };
                        TableRowCount { name, row_count }
                    }
                }))
                .buffer_unordered(COUNT_CONCURRENCY)
                .collect()
                .await;
            Ok(counts)
        }
        ActiveConnection::Oracle(h) => {
            // One session: the counts run one after another.
            let mut counts = Vec::with_capacity(tables.len());
            for name in tables {
                let row_count = match super::oracle::exact_row_count(&h, &schema, &name).await {
                    Ok(n) => n,
                    Err(e) => {
                        eprintln!("[row count] {schema}.{name}: {e}");
                        -1
                    }
                };
                counts.push(TableRowCount { name, row_count });
            }
            Ok(counts)
        }
        _ => Ok(Vec::new()),
    }
}

async fn list_schemas_pg(pool: &PgPool) -> Result<Vec<String>, String> {
    let rows = sqlx::query(
        // `pg_catalog` and `information_schema` are returned: they are real,
        // browsable schemas, and a client that hides the catalog cannot answer
        // "what does pg think this table looks like" without dropping to SQL. The
        // UI filters them out of the picker by default and offers a toggle -
        // that is a display choice, not something to decide in the query.
        //
        // Toast and temp namespaces stay excluded. They are per-session,
        // per-relation storage with generated names; there is nothing there to
        // browse, and on a busy server they would outnumber the real schemas.
        r#"SELECT n.nspname::text FROM pg_catalog.pg_namespace n
           WHERE n.nspname <> 'pg_toast'
             AND n.nspname NOT LIKE 'pg_temp_%'
             AND n.nspname NOT LIKE 'pg_toast_%'
           ORDER BY (n.nspname IN ('pg_catalog','information_schema')), n.nspname"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list schemas: {e}"))?;

    Ok(rows.iter().filter_map(|r| r.try_get::<String, _>(0).ok()).collect())
}

async fn list_tables_pg(pool: &PgPool, schema: &str) -> Result<Vec<TableInfo>, String> {
    let rows = sqlx::query(LIST_TABLES_SQL)
        .bind(schema)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("Failed to list tables: {e}"))?;

    let mut tables: Vec<TableInfo> = rows
        .iter()
        .filter_map(|r| {
            Some(TableInfo {
                name: r.try_get(0).ok()?,
                kind: r.try_get::<String, _>(1).unwrap_or_else(|_| "table".to_string()),
                row_count: r.try_get::<i64, _>(2).unwrap_or(-1),
                rls_enabled: r.try_get::<bool, _>(3).ok(),
                created_at: None,
                create_order: r.try_get::<i64, _>(4).ok(),
                system: None,
            })
        })
        .collect();

    // Views and foreign tables never get a COUNT(*) - report 0 immediately.
    // Real tables keep -1 (unknown): returning right away lets the sidebar
    // render instantly, and the UI resolves exact counts in a background
    // `table_row_counts` pass instead of blocking the list on N COUNT(*)s.
    for t in tables.iter_mut() {
        if t.row_count < 0 && (t.kind == "view" || t.kind == "foreign_table") {
            t.row_count = 0;
        }
    }
    Ok(tables)
}

async fn list_indexes_pg(pool: &PgPool, schema: &str) -> Result<Vec<IndexInfo>, String> {
    let rows = sqlx::query(LIST_INDEXES_SQL)
        .bind(schema)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("Failed to list indexes: {e}"))?;

    Ok(rows
        .iter()
        .filter_map(|r| {
            Some(IndexInfo {
                name: r.try_get(0).ok()?,
                table_name: r.try_get(1).ok()?,
                columns: r.try_get::<String, _>(2).unwrap_or_default(),
                index_type: r.try_get::<String, _>(3).unwrap_or_else(|_| "btree".to_string()),
                is_unique: r.try_get::<bool, _>(4).unwrap_or(false),
                is_primary: r.try_get::<bool, _>(5).unwrap_or(false),
                condition: r.try_get::<Option<String>, _>(6).ok().flatten(),
                comment: r.try_get::<Option<String>, _>(7).ok().flatten(),
            })
        })
        .collect())
}

// ── SQLite / D1 ───────────────────────────────────────────────────────────────

async fn list_tables_sqlite(pool: &sqlx::SqlitePool) -> Result<Vec<TableInfo>, String> {
    // The catalog's rowid grows as objects are created, so it doubles as the
    // creation order SQLite otherwise does not keep.
    let rows = sqlx::query(
        "SELECT name, type, rowid FROM sqlite_master WHERE type IN ('table','view') ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list tables: {e}"))?;

    let mut tables: Vec<TableInfo> = rows
        .iter()
        .filter_map(|r| {
            let name = r.try_get::<Option<String>, _>(0).ok().flatten()?;
            let ty = r.try_get::<Option<String>, _>(1).ok().flatten().unwrap_or_default();
            let kind = if ty == "view" { "view".to_string() } else { "table".to_string() };
            let create_order = r.try_get::<i64, _>(2).ok();
            Some(TableInfo { name, kind, row_count: -1, rls_enabled: None, created_at: None, create_order, system: None })
        })
        .collect();

    // Views are always reported as 0; only base tables need a COUNT(*).
    let count_idx: Vec<usize> = tables
        .iter()
        .enumerate()
        .filter(|(_, t)| t.kind != "view")
        .map(|(i, _)| i)
        .collect();
    // Batch the COUNT(*)s into a single UNION ALL round-trip per chunk instead of
    // one query per table. SQLite pools use a single connection, so this is the
    // only way to avoid N serialized statements on sidebar open.
    for chunk in count_idx.chunks(SQLITE_COUNT_BATCH) {
        let sql = chunk
            .iter()
            .map(|&i| format!("SELECT COUNT(*) FROM \"{}\"", tables[i].name.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" UNION ALL ");
        match sqlx::query(&sql).fetch_all(pool).await {
            Ok(rows) => {
                for (&i, row) in chunk.iter().zip(rows.iter()) {
                    tables[i].row_count =
                        row.try_get::<Option<i64>, _>(0).ok().flatten().unwrap_or(0);
                }
            }
            // A single bad table (e.g. a virtual table) fails the whole batch;
            // fall back to per-table counts for this chunk so the rest still work.
            Err(_) => {
                for &i in chunk {
                    let one = format!("SELECT COUNT(*) FROM \"{}\"", tables[i].name.replace('"', "\"\""));
                    if let Ok(row) = sqlx::query(&one).fetch_one(pool).await {
                        tables[i].row_count =
                            row.try_get::<Option<i64>, _>(0).ok().flatten().unwrap_or(0);
                    }
                }
            }
        }
    }
    for t in tables.iter_mut() {
        if t.row_count < 0 {
            t.row_count = 0;
        }
    }
    Ok(tables)
}

/// Max number of COUNT(*) subqueries per batched UNION ALL round-trip. Kept well
/// under SQLite's default SQLITE_MAX_COMPOUND_SELECT (500) with headroom.
const SQLITE_COUNT_BATCH: usize = 200;
/// Counts per D1 statement: each is one result column, and D1 allows 100.
const D1_COUNT_BATCH: usize = 50;
/// Simultaneous D1 requests when a batch has to be retried table by table.
const D1_FALLBACK_CONCURRENCY: usize = 8;

/// The column list of a SQLite `CREATE [UNIQUE] INDEX name ON tbl (a, b DESC)`,
/// as "a, b". `sqlite_master.sql` is the only place D1 and libSQL expose it
/// without a PRAGMA round trip per index; automatic indexes have no SQL.
fn sqlite_index_columns(sql: &str) -> String {
    let upper = sql.to_uppercase();
    let Some(on) = upper.find(" ON ") else { return String::new() };
    let rest = &sql[on + 4..];
    let Some(open) = rest.find('(') else { return String::new() };
    let mut depth = 0i32;
    let mut end = None;
    for (i, ch) in rest[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(end) = end else { return String::new() };
    rest[open + 1..end]
        .split(',')
        .map(|c| c.trim().trim_matches(|ch| ch == '"' || ch == '`' || ch == '[' || ch == ']').to_string())
        .filter(|c| !c.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod sqlite_index_columns_tests {
    #[test]
    fn reads_the_column_list() {
        assert_eq!(super::sqlite_index_columns("CREATE INDEX `a_idx` ON `account` (`userId`)"), "userId");
        assert_eq!(super::sqlite_index_columns("CREATE UNIQUE INDEX x ON t(a, \"b\" DESC)"), "a, b\" DESC");
        assert_eq!(super::sqlite_index_columns(""), "");
    }
}

async fn list_indexes_sqlite_master(pool: &sqlx::SqlitePool) -> Result<Vec<IndexInfo>, String> {
    let rows = sqlx::query(
        "SELECT name, tbl_name, COALESCE(sql, '') FROM sqlite_master WHERE type = 'index' ORDER BY tbl_name, name",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list indexes: {e}"))?;

    Ok(rows
        .iter()
        .filter_map(|r| {
            let name = r.try_get::<Option<String>, _>(0).ok().flatten()?;
            let table_name = r.try_get::<Option<String>, _>(1).ok().flatten()?;
            let raw_sql = r.try_get::<String, _>(2).unwrap_or_default();
            let sql = raw_sql.to_uppercase();
            let is_unique = sql.contains("UNIQUE") || name.starts_with("sqlite_autoindex_");
            let is_primary = name.starts_with("sqlite_autoindex_");
            Some(IndexInfo {
                name,
                table_name,
                columns: sqlite_index_columns(&raw_sql),
                index_type: "btree".to_string(),
                is_unique,
                is_primary,
                condition: None,
                comment: None,
            })
        })
        .collect())
}

async fn list_tables_d1(cfg: &super::connection::D1Config) -> Result<Vec<TableInfo>, String> {
    let result = super::d1::query(
        cfg,
        "SELECT name, type, rowid AS seq FROM sqlite_master WHERE type IN ('table','view') ORDER BY name",
        vec![],
    )
    .await?;

    let name_idx = result.columns.iter().position(|c| c.name == "name").unwrap_or(0);
    let type_idx = result.columns.iter().position(|c| c.name == "type").unwrap_or(1);
    let seq_idx = result.columns.iter().position(|c| c.name == "seq");

    let mut tables: Vec<TableInfo> = result
        .rows
        .iter()
        .filter_map(|r| {
            let name = r.get(name_idx)?.as_str()?.to_string();
            // Cloudflare D1 blocks access to its internal `_cf_*` tables with a
            // SQLITE_AUTH error, so hide them (can't be browsed or backed up).
            if name.starts_with("_cf_") {
                return None;
            }
            let ty = r.get(type_idx).and_then(|v| v.as_str()).unwrap_or("table");
            let kind = if ty == "view" { "view".to_string() } else { "table".to_string() };
            let create_order = seq_idx.and_then(|i| r.get(i)).and_then(|v| v.as_i64());
            Some(TableInfo { name, kind, row_count: -1, rls_enabled: None, created_at: None, create_order, system: None })
        })
        .collect();

    // Each D1 count is a full HTTPS round-trip to Cloudflare, so N tables used to
    // mean N sequential requests on sidebar open. Batch them into one UNION ALL
    // request per chunk; fall back to per-table only if a batch fails.
    let count_idx: Vec<usize> = tables
        .iter()
        .enumerate()
        .filter(|(_, t)| t.kind != "view")
        .map(|(i, _)| i)
        .collect();
    // Scalar subqueries in ONE select, not a UNION ALL: D1 caps a compound
    // SELECT at a handful of terms, so the union failed on any real schema and
    // every table fell back to its own request, one after another (thirteen
    // tables measured at 3.9s). Chunks, and the per-table fallback when a chunk
    // still fails (one unreadable table fails its whole statement), run
    // concurrently.
    let quoted: Vec<String> = count_idx
        .iter()
        .map(|&i| format!("\"{}\"", tables[i].name.replace('"', "\"\"")))
        .collect();
    // Owned per-chunk futures: borrowing `cfg` and the names across a closure
    // makes the command's future fail its Send bound.
    let mut jobs = Vec::new();
    for chunk in quoted.chunks(D1_COUNT_BATCH) {
        let (cfg, chunk) = (cfg.clone(), chunk.to_vec());
        jobs.push(async move {
            let sql = format!(
                "SELECT {}",
                chunk
                    .iter()
                    .enumerate()
                    .map(|(n, t)| format!("(SELECT COUNT(*) FROM {t}) AS c{n}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            if let Ok(res) = super::d1::query(&cfg, &sql, vec![]).await {
                if res.rows.first().is_some_and(|r| r.len() == chunk.len()) {
                    return res.rows[0].iter().map(|v| v.as_i64().unwrap_or(0)).collect::<Vec<i64>>();
                }
            }
            let mut out = Vec::with_capacity(chunk.len());
            for group in chunk.chunks(D1_FALLBACK_CONCURRENCY) {
                let mut singles = Vec::new();
                for t in group {
                    let (cfg, sql) = (cfg.clone(), format!("SELECT COUNT(*) FROM {t}"));
                    singles.push(async move {
                        super::d1::query(&cfg, &sql, vec![])
                            .await
                            .ok()
                            .and_then(|res| res.rows.first().and_then(|r| r.first().and_then(|v| v.as_i64())))
                            .unwrap_or(0)
                    });
                }
                out.extend(futures::future::join_all(singles).await);
            }
            out
        });
    }
    let counts: Vec<Vec<i64>> = futures::future::join_all(jobs).await;
    for (&i, n) in count_idx.iter().zip(counts.into_iter().flatten()) {
        tables[i].row_count = n;
    }
    for t in tables.iter_mut() {
        if t.row_count < 0 {
            t.row_count = 0;
        }
    }
    Ok(tables)
}

async fn list_indexes_d1_master(cfg: &super::connection::D1Config) -> Result<Vec<IndexInfo>, String> {
    let result = super::d1::query(
        cfg,
        "SELECT name, tbl_name, COALESCE(sql, '') FROM sqlite_master WHERE type = 'index' ORDER BY tbl_name, name",
        vec![],
    )
    .await?;

    let name_idx = result.columns.iter().position(|c| c.name == "name").unwrap_or(0);
    let tbl_idx = result.columns.iter().position(|c| c.name == "tbl_name").unwrap_or(1);
    let sql_idx = result.columns.iter().position(|c| c.name == "sql").unwrap_or(2);

    Ok(result
        .rows
        .iter()
        .filter_map(|r| {
            let name = r.get(name_idx)?.as_str()?.to_string();
            let table_name = r.get(tbl_idx)?.as_str()?.to_string();
            let raw_sql = r.get(sql_idx).and_then(|v| v.as_str()).unwrap_or("");
            let sql = raw_sql.to_uppercase();
            let is_unique = sql.contains("UNIQUE") || name.starts_with("sqlite_autoindex_");
            let is_primary = name.starts_with("sqlite_autoindex_");
            Some(IndexInfo {
                name,
                table_name,
                columns: sqlite_index_columns(raw_sql),
                index_type: "btree".to_string(),
                is_unique,
                is_primary,
                condition: None,
                comment: None,
            })
        })
        .collect())
}

// ── LibSQL / Turso ────────────────────────────────────────────────────────────

async fn list_tables_libsql(cfg: &super::connection::LibSqlConfig) -> Result<Vec<TableInfo>, String> {
    let result = super::libsql::query(
        cfg,
        "SELECT name, type, rowid AS seq FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name",
        vec![],
    ).await?;
    let name_idx = result.columns.iter().position(|c| c.name == "name").unwrap_or(0);
    let type_idx = result.columns.iter().position(|c| c.name == "type").unwrap_or(1);
    let seq_idx = result.columns.iter().position(|c| c.name == "seq");
    let mut tables: Vec<TableInfo> = result.rows.iter().filter_map(|r| {
        let name = r.get(name_idx)?.as_str()?.to_string();
        let ty = r.get(type_idx).and_then(|v| v.as_str()).unwrap_or("table");
        let kind = if ty == "view" { "view".to_string() } else { "table".to_string() };
        let create_order = seq_idx.and_then(|i| r.get(i)).and_then(|v| v.as_i64());
        Some(TableInfo { name, kind, row_count: -1, rls_enabled: None, created_at: None, create_order, system: None })
    }).collect();

    // Batch counts into one round-trip per chunk (each libsql query is a remote
    // HTTP request), with a per-table fallback if a batch fails.
    let count_idx: Vec<usize> = tables
        .iter()
        .enumerate()
        .filter(|(_, t)| t.kind != "view")
        .map(|(i, _)| i)
        .collect();
    for chunk in count_idx.chunks(SQLITE_COUNT_BATCH) {
        let sql = chunk
            .iter()
            .map(|&i| format!("SELECT COUNT(*) FROM \"{}\"", tables[i].name.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" UNION ALL ");
        match super::libsql::query(cfg, &sql, vec![]).await {
            Ok(res) if res.rows.len() == chunk.len() => {
                for (&i, row) in chunk.iter().zip(res.rows.iter()) {
                    tables[i].row_count = row.first().and_then(|v| v.as_i64()).unwrap_or(0);
                }
            }
            _ => {
                for &i in chunk {
                    let one = format!("SELECT COUNT(*) FROM \"{}\"", tables[i].name.replace('"', "\"\""));
                    if let Ok(res) = super::libsql::query(cfg, &one, vec![]).await {
                        if let Some(row) = res.rows.first() {
                            tables[i].row_count = row.first().and_then(|v| v.as_i64()).unwrap_or(0);
                        }
                    }
                }
            }
        }
    }
    for t in tables.iter_mut() {
        if t.row_count < 0 {
            t.row_count = 0;
        }
    }
    Ok(tables)
}

async fn list_indexes_libsql_master(cfg: &super::connection::LibSqlConfig) -> Result<Vec<IndexInfo>, String> {
    let result = super::libsql::query(
        cfg,
        "SELECT name, tbl_name, COALESCE(sql,'') FROM sqlite_master WHERE type='index' ORDER BY tbl_name, name",
        vec![],
    ).await?;
    let name_idx = result.columns.iter().position(|c| c.name == "name").unwrap_or(0);
    let tbl_idx  = result.columns.iter().position(|c| c.name == "tbl_name").unwrap_or(1);
    let sql_idx  = result.columns.iter().position(|c| c.name == "sql").unwrap_or(2);
    Ok(result.rows.iter().filter_map(|r| {
        let name = r.get(name_idx)?.as_str()?.to_string();
        let table_name = r.get(tbl_idx)?.as_str()?.to_string();
        let raw_sql = r.get(sql_idx).and_then(|v| v.as_str()).unwrap_or("");
        let sql = raw_sql.to_uppercase();
        let is_unique = sql.contains("UNIQUE") || name.starts_with("sqlite_autoindex_");
        let is_primary = name.starts_with("sqlite_autoindex_");
        Some(IndexInfo { name, table_name, columns: sqlite_index_columns(raw_sql), index_type: "btree".to_string(), is_unique, is_primary, condition: None, comment: None })
    }).collect())
}

// ── MySQL ─────────────────────────────────────────────────────────────────────

async fn list_schemas_mysql(pool: &MySqlPool) -> Result<Vec<String>, String> {
    let row = sqlx::query("SELECT DATABASE()")
        .fetch_one(pool)
        .await
        .map_err(|e| format!("Failed to get database: {e}"))?;
    let db: Option<String> = my_text(&row, 0);
    Ok(vec![db.unwrap_or_else(|| "default".to_string())])
}

async fn mysql_exact_row_count(pool: &MySqlPool, schema: &str, table: &str) -> Result<i64, String> {
    let q = |id: &str| format!("`{}`", id.replace('`', "``"));
    // Same starvation guard as the Postgres path (see COUNT_STATEMENT_TIMEOUT):
    // an unbounded background COUNT(*) on a large remote table pins a pooled
    // connection for minutes, and this pool only has 4, so interactive queries
    // then fail with "pool timed out". MAX_EXECUTION_TIME is an optimizer hint
    // (MySQL 5.7.8+), so it needs no session state and nothing to reset;
    // MariaDB ignores it as a comment, hence the client-side backstop below.
    let ms = COUNT_STATEMENT_TIMEOUT_MS;
    let sql = format!(
        "SELECT /*+ MAX_EXECUTION_TIME({ms}) */ COUNT(*) FROM {}.{}",
        q(schema),
        q(table)
    );
    let query = sqlx::query_scalar::<_, i64>(&sql).fetch_one(pool);
    // Backstop for servers that ignore the hint. Dropping the future returns the
    // connection to the pool, which is the point - a missing sidebar count is
    // cheaper than a starved pool.
    match tokio::time::timeout(std::time::Duration::from_millis(ms + 1_000), query).await {
        Ok(r) => r.map_err(|e| format!("Failed to count rows for {table}: {e}")),
        Err(_) => Err(format!("Failed to count rows for {table}: timed out")),
    }
}

async fn list_tables_mysql(pool: &MySqlPool, schema: &str) -> Result<Vec<TableInfo>, String> {
    let rows = sqlx::query(
        "SELECT TABLE_NAME, TABLE_TYPE, COALESCE(TABLE_ROWS, 0), \
                DATE_FORMAT(CREATE_TIME, '%Y-%m-%d %H:%i:%s') \
         FROM information_schema.TABLES \
         WHERE TABLE_SCHEMA = ? \
         ORDER BY TABLE_NAME",
    )
    .bind(schema)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list tables: {e}"))?;

    let mut tables: Vec<TableInfo> = rows
        .iter()
        .filter_map(|r| {
            let name: String = my_text(r, 0)?;
            let ty: String = my_text(r, 1).unwrap_or_else(|| "BASE TABLE".to_string());
            // TABLE_ROWS is declared BIGINT UNSIGNED, but `COALESCE(TABLE_ROWS, 0)`
            // comes back as decimal(21,0) - so a u64 decode failed and every
            // table read 0. For InnoDB it is an estimate anyway, and frequently
            // 0 (or far off) until ANALYZE TABLE runs, so a 0 is treated as
            // unknown (-1) and resolved with an exact COUNT(*) below.
            //
            // A small estimate is not trusted either. MySQL 8 caches these
            // statistics for `information_schema_stats_expiry` (86400s by
            // default), so after an INSERT the sidebar kept the old number
            // through every refresh: 3 rows listed for a table holding 24, on
            // the stroke-test-mysql fixture. Below the Postgres threshold
            // (ESTIMATE_THRESHOLD, 100k) an exact count is cheap, so it is
            // always taken; above it the estimate stands.
            let est: i64 = my_int(r, 2).unwrap_or(0);
            let kind = if ty == "VIEW" { "view" } else { "table" }.to_string();
            let row_count = if est >= 100_000 { est } else { -1 };
            // NULL for views; a real timestamp for tables.
            let created_at = my_text(r, 3);
            Some(TableInfo { name, kind, row_count, rls_enabled: None, created_at, create_order: None, system: None })
        })
        .collect();

    // Views report 0 immediately; base tables with no usable estimate keep -1
    // (unknown) and are resolved lazily via `table_row_counts` - see the
    // PostgreSQL path above.
    for t in tables.iter_mut() {
        if t.row_count < 0 && t.kind == "view" {
            t.row_count = 0;
        }
    }
    Ok(tables)
}

async fn list_indexes_mysql(pool: &MySqlPool, schema: &str) -> Result<Vec<IndexInfo>, String> {
    let rows = sqlx::query(
        "SELECT INDEX_NAME, TABLE_NAME, \
                GROUP_CONCAT(COLUMN_NAME ORDER BY SEQ_IN_INDEX SEPARATOR ', ') AS columns, \
                MIN(INDEX_TYPE) AS index_type, \
                MIN(NON_UNIQUE) AS non_unique, \
                MAX(IF(INDEX_NAME = 'PRIMARY', 1, 0)) AS is_primary \
         FROM information_schema.STATISTICS \
         WHERE TABLE_SCHEMA = ? \
         GROUP BY INDEX_NAME, TABLE_NAME \
         ORDER BY TABLE_NAME, MAX(IF(INDEX_NAME = 'PRIMARY', 1, 0)) DESC, INDEX_NAME",
    )
    .bind(schema)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list indexes: {e}"))?;

    Ok(rows
        .iter()
        .filter_map(|r| {
            let name: String = my_text(r, 0)?;
            let table_name: String = my_text(r, 1)?;
            let columns: String = my_text(r, 2).unwrap_or_default();
            let index_type: String = my_text(r, 3).unwrap_or_else(|| "BTREE".to_string());
            let non_unique: i64 = r.try_get::<Option<i64>, _>(4).ok().flatten().unwrap_or(1);
            let is_primary_i: i64 = r.try_get::<Option<i64>, _>(5).ok().flatten().unwrap_or(0);
            Some(IndexInfo {
                name,
                table_name,
                columns,
                index_type: index_type.to_lowercase(),
                is_unique: non_unique == 0,
                is_primary: is_primary_i != 0,
                condition: None,
                comment: None,
            })
        })
        .collect())
}

// ── Enums (PostgreSQL only) ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnumInfo {
    pub name: String,
    pub values: Vec<String>,
    /// Tables in the same schema that have a column using this enum type.
    pub used_in_tables: Vec<String>,
}

// ── Triggers (PostgreSQL only) ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerInfo {
    pub name: String,
    pub table_name: String,
    /// "BEFORE" | "AFTER" | "INSTEAD OF"
    pub timing: String,
    /// e.g. "INSERT, UPDATE"
    pub events: String,
    pub function_name: String,
    pub enabled: bool,
}

// ── Functions / Procedures (PostgreSQL only) ─────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionInfo {
    pub name: String,
    /// Full call signature e.g. "fn(arg1 text, arg2 int)"
    pub signature: String,
    pub return_type: String,
    /// "function" | "procedure" | "aggregate"
    pub kind: String,
}

// ── Sequences (PostgreSQL only) ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceInfo {
    pub name: String,
    pub data_type: String,
    pub start_value: i64,
    pub min_value: i64,
    pub max_value: i64,
    pub increment: i64,
    pub cycle: bool,
    /// "table.column" when owned by a serial/identity column, None otherwise
    pub owned_by: Option<String>,
}

async fn list_enums_pg(pool: &PgPool, schema: &str) -> Result<Vec<EnumInfo>, String> {
    let rows = sqlx::query(
        r#"
        -- Values and used-in-tables are computed as independent subqueries so the
        -- column join can't fan out (multiply) the enum labels. A previous version
        -- joined information_schema.columns and array_agg'd labels without DISTINCT,
        -- so an enum used in N columns produced each label N times.
        SELECT t.typname::text AS name,
               (
                 SELECT array_agg(e.enumlabel::text ORDER BY e.enumsortorder)
                 FROM pg_catalog.pg_enum e
                 WHERE e.enumtypid = t.oid
               ) AS values,
               COALESCE(
                 (
                   SELECT array_agg(DISTINCT c.table_name::text)
                   FROM information_schema.columns c
                   WHERE c.udt_schema = n.nspname
                     AND c.udt_name   = t.typname
                     AND c.table_schema = n.nspname
                     AND c.table_name IS NOT NULL
                 ),
                 ARRAY[]::text[]
               ) AS used_in_tables
        FROM pg_catalog.pg_type t
        JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
        WHERE n.nspname = $1
          AND t.typtype = 'e'
        ORDER BY t.typname
        "#,
    )
    .bind(schema)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list enums: {e}"))?;

    rows.iter()
        .map(|row| {
            let name: String = row.try_get("name").map_err(|e| e.to_string())?;
            let values: Vec<String> = row.try_get("values").map_err(|e| e.to_string())?;
            let used_in_tables: Vec<String> = row.try_get("used_in_tables").map_err(|e| e.to_string())?;
            Ok(EnumInfo { name, values, used_in_tables })
        })
        .collect()
}

async fn list_triggers_pg(pool: &PgPool, schema: &str) -> Result<Vec<TriggerInfo>, String> {
    let rows = sqlx::query(r#"
        SELECT
            t.tgname::text,
            c.relname::text,
            CASE
                WHEN (t.tgtype::integer & 64) = 64 THEN 'INSTEAD OF'
                WHEN (t.tgtype::integer & 2)  = 2  THEN 'BEFORE'
                ELSE 'AFTER'
            END,
            array_to_string(
                array_remove(ARRAY[
                    CASE WHEN (t.tgtype::integer & 4)  = 4  THEN 'INSERT'   ELSE NULL END,
                    CASE WHEN (t.tgtype::integer & 8)  = 8  THEN 'DELETE'   ELSE NULL END,
                    CASE WHEN (t.tgtype::integer & 16) = 16 THEN 'UPDATE'   ELSE NULL END,
                    CASE WHEN (t.tgtype::integer & 32) = 32 THEN 'TRUNCATE' ELSE NULL END
                ], NULL),
                ', '
            ),
            p.proname::text,
            (t.tgenabled::text != 'D')
        FROM pg_catalog.pg_trigger t
        JOIN pg_catalog.pg_class c     ON c.oid = t.tgrelid
        JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_catalog.pg_proc p      ON p.oid = t.tgfoid
        WHERE n.nspname = $1
          AND NOT t.tgisinternal
        ORDER BY c.relname, t.tgname
    "#)
    .bind(schema)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list triggers: {e}"))?;

    Ok(rows
        .iter()
        .filter_map(|r| {
            Some(TriggerInfo {
                name:          r.try_get::<String, _>(0).ok()?,
                table_name:    r.try_get::<String, _>(1).ok()?,
                timing:        r.try_get::<String, _>(2).unwrap_or_else(|_| "AFTER".to_string()),
                events:        r.try_get::<String, _>(3).unwrap_or_default(),
                function_name: r.try_get::<String, _>(4).unwrap_or_default(),
                enabled:       r.try_get::<bool, _>(5).unwrap_or(true),
            })
        })
        .collect())
}

async fn list_triggers_mysql(pool: &sqlx::MySqlPool, schema: &str) -> Result<Vec<TriggerInfo>, String> {
    // MySQL triggers are single-event and single-timing; the action body is
    // inline (no named function) and there is no per-trigger enable flag.
    let rows = sqlx::query(
        r#"SELECT TRIGGER_NAME       AS name,
                  EVENT_OBJECT_TABLE AS table_name,
                  ACTION_TIMING      AS timing,
                  EVENT_MANIPULATION AS events
           FROM information_schema.TRIGGERS
           WHERE TRIGGER_SCHEMA = ?
           ORDER BY EVENT_OBJECT_TABLE, TRIGGER_NAME"#,
    )
    .bind(schema)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list triggers: {e}"))?;

    Ok(rows
        .iter()
        .filter_map(|r| {
            Some(TriggerInfo {
                name:          my_text_named(r, "name")?,
                table_name:    my_text_named(r, "table_name").unwrap_or_default(),
                timing:        my_text_named(r, "timing").unwrap_or_else(|| "AFTER".to_string()),
                events:        my_text_named(r, "events").unwrap_or_default(),
                function_name: String::new(),
                enabled:       true,
            })
        })
        .collect())
}

async fn list_triggers_sqlite(pool: &sqlx::SqlitePool) -> Result<Vec<TriggerInfo>, String> {
    // SQLite keeps triggers in sqlite_master; timing/event aren't broken out into
    // columns, so parse them out of the stored `CREATE TRIGGER` DDL.
    let rows = sqlx::query(
        r#"SELECT name, tbl_name, COALESCE(sql, '') AS sql
           FROM sqlite_master
           WHERE type = 'trigger'
           ORDER BY tbl_name, name"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list triggers: {e}"))?;

    Ok(rows
        .iter()
        .filter_map(|r| {
            let name: String = r.try_get("name").ok()?;
            let table_name: String = r.try_get("tbl_name").unwrap_or_default();
            let ddl: String = r.try_get::<String, _>("sql").unwrap_or_default().to_uppercase();
            let timing = if ddl.contains("INSTEAD OF") {
                "INSTEAD OF"
            } else if ddl.contains("BEFORE") {
                "BEFORE"
            } else {
                "AFTER"
            };
            let events = ["INSERT", "UPDATE", "DELETE"]
                .iter()
                .filter(|ev| ddl.contains(**ev))
                .copied()
                .collect::<Vec<_>>()
                .join(", ");
            Some(TriggerInfo {
                name,
                table_name,
                timing: timing.to_string(),
                events,
                function_name: String::new(),
                enabled: true,
            })
        })
        .collect())
}

async fn list_sequences_pg(pool: &PgPool, schema: &str) -> Result<Vec<SequenceInfo>, String> {
    let rows = sqlx::query(r#"
        SELECT
            s.sequence_name::text,
            s.data_type::text,
            s.start_value::bigint,
            s.minimum_value::bigint,
            s.maximum_value::bigint,
            s.increment::bigint,
            (s.cycle_option = 'YES')::boolean,
            (
                SELECT dep_rel.relname::text || '.' || dep_att.attname::text
                FROM pg_catalog.pg_class seq_cls
                JOIN pg_catalog.pg_depend d
                    ON d.objid = seq_cls.oid AND d.deptype = 'a'
                JOIN pg_catalog.pg_class dep_rel
                    ON dep_rel.oid = d.refobjid
                JOIN pg_catalog.pg_attribute dep_att
                    ON dep_att.attrelid = d.refobjid AND dep_att.attnum = d.refobjsubid
                WHERE seq_cls.relname = s.sequence_name
                  AND seq_cls.relkind = 'S'
                  AND seq_cls.relnamespace = (
                      SELECT oid FROM pg_catalog.pg_namespace WHERE nspname = $1
                  )
                LIMIT 1
            )
        FROM information_schema.sequences s
        WHERE s.sequence_schema = $1
        ORDER BY s.sequence_name
    "#)
    .bind(schema)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list sequences: {e}"))?;

    Ok(rows
        .iter()
        .filter_map(|r| {
            Some(SequenceInfo {
                name:        r.try_get::<String, _>(0).ok()?,
                data_type:   r.try_get::<String, _>(1).unwrap_or_else(|_| "bigint".to_string()),
                start_value: r.try_get::<i64, _>(2).unwrap_or(1),
                min_value:   r.try_get::<i64, _>(3).unwrap_or(1),
                max_value:   r.try_get::<i64, _>(4).unwrap_or(i64::MAX),
                increment:   r.try_get::<i64, _>(5).unwrap_or(1),
                cycle:       r.try_get::<bool, _>(6).unwrap_or(false),
                owned_by:    r.try_get::<Option<String>, _>(7).ok().flatten(),
            })
        })
        .collect())
}

// ── Public dispatch ───────────────────────────────────────────────────────────

pub async fn list_schemas(state: State<'_, DbState>) -> Result<Vec<String>, String> {
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => list_schemas_pg(&pool).await,
        ActiveConnection::Mysql(pool) => list_schemas_mysql(&pool).await,
        ActiveConnection::Clickhouse(cfg) => super::clickhouse::list_schemas(&cfg).await,
        ActiveConnection::Posthog(cfg) => super::posthog::list_schemas(&cfg).await,
        ActiveConnection::Redis(_) => Ok(vec![]),
        ActiveConnection::Dynamodb(_) => Ok(vec![super::docdb::FLAT_SCHEMA.to_string()]),
        ActiveConnection::Doc(d) => super::docdb::list_schemas(&d).await,
        ActiveConnection::Mssql(h) => super::mssql::list_schemas(&h).await,
        ActiveConnection::Oracle(h) => super::oracle::list_schemas(&h).await,
        ActiveConnection::Sqlite(_) | ActiveConnection::D1(_) | ActiveConnection::LibSql(_) | ActiveConnection::Duckdb(_) => Ok(vec!["main".to_string()]),
    }
}

pub async fn list_tables(state: State<'_, DbState>, schema: String) -> Result<Vec<TableInfo>, String> {
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => {
            validate_ident(&schema)?;
            let tables = list_tables_pg(&pool, &schema).await?;
            // Answer the wide-column question for the whole schema now, off the
            // critical path, so a table's first open is one round trip, not two.
            let names: Vec<String> = tables.iter().map(|t| t.name.clone()).collect();
            let (pool, schema) = (pool.clone(), schema.clone());
            tokio::spawn(async move {
                tokio::join!(
                    super::wide_columns::prefetch_schema(&pool, &schema, &names),
                    super::query::prefetch_primary_keys(&pool, &schema, &names),
                );
            });
            Ok(tables)
        }
        ActiveConnection::Mysql(pool) => list_tables_mysql(&pool, &schema).await,
        ActiveConnection::Sqlite(pool) => list_tables_sqlite(&pool).await,
        ActiveConnection::D1(cfg) => list_tables_d1(&cfg).await,
        ActiveConnection::LibSql(cfg) => list_tables_libsql(&cfg).await,
        ActiveConnection::Clickhouse(cfg) => super::clickhouse::list_tables(&cfg, &schema).await,
        ActiveConnection::Posthog(cfg) => super::posthog::list_tables(&cfg).await,
        ActiveConnection::Redis(cfg) => super::redis::list_tables(&cfg).await,
        ActiveConnection::Dynamodb(cfg) => super::dynamodb::list_tables(&cfg).await,
        ActiveConnection::Doc(d) => super::docdb::list_tables_in(&d, &schema).await,
        ActiveConnection::Duckdb(h) => super::duckdb::list_tables(&h).await,
        ActiveConnection::Mssql(h) => super::mssql::list_tables(&h, &schema).await,
        ActiveConnection::Oracle(h) => super::oracle::list_tables(&h, &schema).await,
    }
}

/// Indexes for a schema, or for one table of it.
///
/// The SQL engines answer for the whole schema in one catalog query and ignore
/// `table` (the caller filters). MongoDB, CouchDB and DynamoDB keep indexes per
/// collection, with no schema-wide listing, so they answer only when `table` is
/// given - a schema-wide call would be one request per collection.
///
/// Rows with `index_type == "key"` are not indexes but the table's key columns
/// on engines whose keys are not a single primary key (Cassandra partition and
/// clustering keys, DynamoDB partition and sort keys): the tree shows them under
/// Keys, not Indexes.
pub async fn list_indexes_for(
    state: State<'_, DbState>,
    schema: String,
    table: Option<String>,
) -> Result<Vec<IndexInfo>, String> {
    let table = table.filter(|t| !t.is_empty());
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => {
            validate_ident(&schema)?;
            list_indexes_pg(&pool, &schema).await
        }
        ActiveConnection::Mysql(pool) => list_indexes_mysql(&pool, &schema).await,
        ActiveConnection::Sqlite(pool) => list_indexes_sqlite(&pool).await,
        ActiveConnection::D1(cfg) => list_indexes_d1(&cfg).await,
        ActiveConnection::LibSql(cfg) => list_indexes_libsql(&cfg).await,
        ActiveConnection::Clickhouse(cfg) => super::clickhouse::list_indexes(&cfg, &schema).await,
        ActiveConnection::Posthog(cfg) => super::posthog::list_indexes(&cfg).await,
        ActiveConnection::Redis(cfg) => super::redis::list_indexes(&cfg).await,
        ActiveConnection::Dynamodb(cfg) => match table {
            Some(t) => Ok(super::dynamodb::describe_shape(&cfg, &t).await?.1),
            None => Ok(vec![]),
        },
        ActiveConnection::Doc(d) => super::docdb::indexes_in(&d, &schema, table.as_deref()).await,
        ActiveConnection::Duckdb(h) => super::duckdb::list_indexes(&h).await,
        ActiveConnection::Mssql(h) => super::mssql::list_indexes(&h, &schema).await,
        ActiveConnection::Oracle(h) => super::oracle::list_indexes(&h, &schema).await,
    }
}

async fn list_functions_pg(pool: &PgPool, schema: &str) -> Result<Vec<FunctionInfo>, String> {
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT ON (p.proname)
               p.proname::text AS name,
               pg_get_function_arguments(p.oid) AS arg_types,
               COALESCE(pg_get_function_result(p.oid), 'void') AS return_type,
               CASE p.prokind
                   WHEN 'f' THEN 'function'
                   WHEN 'p' THEN 'procedure'
                   WHEN 'a' THEN 'aggregate'
                   ELSE 'function'
               END AS kind
        FROM pg_catalog.pg_proc p
        JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
        WHERE n.nspname = $1
          AND p.prokind IN ('f', 'p', 'a')
        ORDER BY p.proname
        "#,
    )
    .bind(schema)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list functions: {e}"))?;

    rows.iter()
        .map(|row| {
            let name: String = row.try_get("name").map_err(|e| e.to_string())?;
            let arg_types: String = row.try_get("arg_types").map_err(|e| e.to_string())?;
            let return_type: String = row.try_get("return_type").map_err(|e| e.to_string())?;
            let kind: String = row.try_get("kind").map_err(|e| e.to_string())?;
            Ok(FunctionInfo {
                signature: format!("{}({})", name, arg_types),
                name,
                return_type,
                kind,
            })
        })
        .collect()
}

async fn list_functions_mysql(pool: &sqlx::MySqlPool, schema: &str) -> Result<Vec<FunctionInfo>, String> {
    let rows = sqlx::query(
        r#"SELECT ROUTINE_NAME AS name, DTD_IDENTIFIER AS return_type, ROUTINE_TYPE AS routine_type
           FROM information_schema.ROUTINES
           WHERE ROUTINE_SCHEMA = ?
           ORDER BY ROUTINE_NAME"#,
    )
    .bind(schema)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list functions: {e}"))?;

    rows.iter()
        .map(|row| {
            // `map_err(...)?` here, so this one did not fail quietly: every
            // MySQL routine list came back as "mismatched types ... VARBINARY".
            let name = my_text_named(row, "name").ok_or("Routine has no name")?;
            let return_type = my_text_named(row, "return_type");
            let routine_type = my_text_named(row, "routine_type").unwrap_or_else(|| "FUNCTION".to_string());
            Ok(FunctionInfo {
                signature: format!("{name}()"),
                name,
                return_type: return_type.unwrap_or_default(),
                kind: routine_type.to_lowercase(),
            })
        })
        .collect()
}

pub async fn list_functions(state: State<'_, DbState>, schema: String) -> Result<Vec<FunctionInfo>, String> {
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => {
            validate_ident(&schema)?;
            list_functions_pg(&pool, &schema).await
        }
        ActiveConnection::Mysql(pool) => {
            validate_ident(&schema)?;
            list_functions_mysql(&pool, &schema).await
        }
        _ => Ok(vec![]),
    }
}

pub async fn list_enums(state: State<'_, DbState>, schema: String) -> Result<Vec<EnumInfo>, String> {
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => {
            validate_ident(&schema)?;
            list_enums_pg(&pool, &schema).await
        }
        _ => Ok(vec![]),
    }
}

pub async fn list_triggers(state: State<'_, DbState>, schema: String) -> Result<Vec<TriggerInfo>, String> {
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => {
            validate_ident(&schema)?;
            list_triggers_pg(&pool, &schema).await
        }
        ActiveConnection::Mysql(pool) => {
            validate_ident(&schema)?;
            list_triggers_mysql(&pool, &schema).await
        }
        ActiveConnection::Sqlite(pool) => list_triggers_sqlite(&pool).await,
        _ => Ok(vec![]),
    }
}

pub async fn list_sequences(state: State<'_, DbState>, schema: String) -> Result<Vec<SequenceInfo>, String> {
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => {
            validate_ident(&schema)?;
            list_sequences_pg(&pool, &schema).await
        }
        // Sequences are a PostgreSQL concept. MySQL uses AUTO_INCREMENT and SQLite
        // uses sqlite_sequence rowids - neither exposes first-class sequence objects,
        // so there is nothing to list for those engines (documented N/A per todo P2.12).
        _ => Ok(vec![]),
    }
}

// Both spell the statement per engine in `admin`; this used to know Postgres
// and SQLite only and told every other engine it was unsupported.
pub async fn truncate_table(state: State<'_, DbState>, schema: String, table: String) -> Result<(), String> {
    let conn = require_conn(&state)?;
    super::admin::truncate_table(&conn, &schema, &table).await
}

pub async fn drop_table(state: State<'_, DbState>, schema: String, table: String, cascade: bool, kind: Option<String>) -> Result<(), String> {
    let kind = super::admin::ObjectKind::parse(kind.as_deref())?;
    let conn = require_conn(&state)?;
    super::admin::drop_object(&conn, &schema, &table, kind, cascade).await
}

pub async fn get_table_column_structure(
    state: State<'_, DbState>,
    schema: String,
    table: String,
) -> Result<Vec<ColumnStructureRow>, String> {
    // The NoSQL engines take names as values (a request field, a bound CQL
    // parameter), never spliced into SQL, and their names are not identifiers:
    // `fs.files` is an ordinary MongoDB collection.
    match require_conn(&state)? {
        ActiveConnection::Dynamodb(cfg) => return Ok(super::dynamodb::describe_shape(&cfg, &table).await?.0),
        ActiveConnection::Doc(d) => return super::docdb::columns_in(&d, &schema, &table).await,
        // Oracle binds the names as values, and they may hold `$` or `#`.
        ActiveConnection::Oracle(h) => return super::oracle::get_column_structure(&h, &schema, &table).await,
        _ => {}
    }
    validate_ident(&schema)?;
    validate_ident(&table)?;
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => {
            get_column_structure_pg(&pool, &schema, &table).await
        }
        ActiveConnection::Mysql(pool) => {
            get_column_structure_mysql(&pool, &schema, &table).await
        }
        ActiveConnection::Sqlite(pool) => {
            get_column_structure_sqlite(&pool, &table).await
        }
        ActiveConnection::D1(cfg) => {
            get_column_structure_d1(&cfg, &table).await
        }
        ActiveConnection::LibSql(cfg) => {
            get_column_structure_libsql(&cfg, &table).await
        }
        ActiveConnection::Clickhouse(cfg) => {
            super::clickhouse::get_column_structure(&cfg, &schema, &table).await
        }
        ActiveConnection::Posthog(cfg) => super::posthog::get_column_structure(&cfg, &table).await,
        ActiveConnection::Redis(cfg) => {
            super::redis::get_column_structure(&cfg, &table).await
        }
        // Answered above.
        ActiveConnection::Dynamodb(_) | ActiveConnection::Doc(_) | ActiveConnection::Oracle(_) => Ok(vec![]),
        ActiveConnection::Duckdb(h) => {
            super::duckdb::get_column_structure(&h, &table).await
        }
        ActiveConnection::Mssql(h) => {
            super::mssql::get_column_structure(&h, &schema, &table).await
        }
    }
}

async fn get_column_structure_sqlite(
    pool: &sqlx::SqlitePool,
    table: &str,
) -> Result<Vec<ColumnStructureRow>, String> {
    if let Ok(mut v) = sqlite_family_columns(SqliteFamily::Local(pool), Some(table)).await {
        return Ok(v.pop().map(|t| t.columns).unwrap_or_default());
    }
    get_column_structure_sqlite_pragma(pool, table).await
}

async fn get_column_structure_sqlite_pragma(
    pool: &sqlx::SqlitePool,
    table: &str,
) -> Result<Vec<ColumnStructureRow>, String> {
    use std::collections::HashMap;
    let tq = format!("\"{}\"", table.replace('"', "\"\""));

    // cid, name, type, notnull, dflt_value, pk
    let info_rows = sqlx::query(&format!("PRAGMA table_info({tq})"))
        .fetch_all(pool)
        .await
        .map_err(|e| format!("PRAGMA table_info failed: {e}"))?;

    // id, seq, table, from, to, on_update, on_delete, match
    let fk_rows = sqlx::query(&format!("PRAGMA foreign_key_list({tq})"))
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    let mut fk_map: HashMap<String, String> = Default::default();
    for r in &fk_rows {
        let from_col: String = r.try_get::<Option<String>, _>(3).ok().flatten().unwrap_or_default();
        let ref_table: String = r.try_get::<Option<String>, _>(2).ok().flatten().unwrap_or_default();
        let to_col: String = r.try_get::<Option<String>, _>(4).ok().flatten().unwrap_or_default();
        if !from_col.is_empty() {
            fk_map.insert(from_col, format!("main.{ref_table}.{to_col}"));
        }
    }

    Ok(info_rows.iter().filter_map(|r| {
        let cid: i64 = r.try_get::<Option<i64>, _>(0).ok().flatten().unwrap_or(0);
        let name: String = r.try_get::<Option<String>, _>(1).ok().flatten()?;
        let data_type = r.try_get::<Option<String>, _>(2).ok().flatten().unwrap_or_default().to_lowercase();
        let notnull: i64 = r.try_get::<Option<i64>, _>(3).ok().flatten().unwrap_or(0);
        let pk: i64 = r.try_get::<Option<i64>, _>(5).ok().flatten().unwrap_or(0);
        let column_default: Option<String> = r.try_get::<Option<String>, _>(4).ok().flatten();
        let foreign_key = fk_map.get(&name).cloned();
        Some(ColumnStructureRow {
            ordinal_position: (cid + 1) as i32,
            name: name.clone(),
            data_type,
            is_nullable: notnull == 0,
            is_primary_key: pk > 0,
            column_default,
            foreign_key,
            fk_constraint_name: None,
            comment: None,
        })
    }).collect())
}

async fn get_column_structure_mysql(
    pool: &MySqlPool,
    schema: &str,
    table: &str,
) -> Result<Vec<ColumnStructureRow>, String> {
    use std::collections::HashMap;

    let col_rows = sqlx::query(
        "SELECT ORDINAL_POSITION, COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT, COLUMN_KEY \
         FROM information_schema.COLUMNS \
         WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? \
         ORDER BY ORDINAL_POSITION",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to get column info: {e}"))?;

    // REFERENCED_TABLE_NAME alone marks a foreign key column. Joining
    // REFERENTIAL_CONSTRAINTS to say the same thing scanned every constraint in
    // the schema: 250ms per table on a 5k-table MariaDB, against 0.1ms without.
    let fk_rows = sqlx::query(
        "SELECT kcu.COLUMN_NAME, kcu.REFERENCED_TABLE_SCHEMA, \
                kcu.REFERENCED_TABLE_NAME, kcu.REFERENCED_COLUMN_NAME, kcu.CONSTRAINT_NAME \
         FROM information_schema.KEY_COLUMN_USAGE kcu \
         WHERE kcu.TABLE_SCHEMA = ? AND kcu.TABLE_NAME = ? \
           AND kcu.REFERENCED_TABLE_NAME IS NOT NULL \
         ORDER BY kcu.CONSTRAINT_NAME, kcu.ORDINAL_POSITION",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut fk_map: HashMap<String, (String, String)> = Default::default();
    for row in &fk_rows {
        let col: String = my_text(row, 0).unwrap_or_default();
        let ref_schema: String = my_text(row, 1).unwrap_or_default();
        let ref_table: String = my_text(row, 2).unwrap_or_default();
        let ref_col: String = my_text(row, 3).unwrap_or_default();
        let constraint: String = my_text(row, 4).unwrap_or_default();
        fk_map.insert(col, (format!("{ref_schema}.{ref_table}.{ref_col}"), constraint));
    }

    Ok(col_rows.iter().filter_map(|r| {
        let ordinal: i32 = r.try_get::<u32, _>(0).unwrap_or(0) as i32;
        let name: String = my_text(r, 1)?;
        let data_type: String = my_text(r, 2).unwrap_or_default().to_lowercase();
        let is_nullable_str: String = my_text(r, 3).unwrap_or_else(|| "YES".to_string());
        let column_default: Option<String> = my_text(r, 4);
        let (foreign_key, fk_constraint_name) = fk_map
            .get(&name)
            .map(|(fk, cn)| (Some(fk.clone()), Some(cn.clone())))
            .unwrap_or((None, None));
        Some(ColumnStructureRow {
            ordinal_position: ordinal,
            name: name.clone(),
            data_type,
            is_nullable: is_nullable_str.eq_ignore_ascii_case("YES"),
            is_primary_key: my_text(r, 5).map(|k| k.eq_ignore_ascii_case("PRI")).unwrap_or(false),
            column_default,
            foreign_key,
            fk_constraint_name,
            comment: None,
        })
    }).collect())
}

/// Shared SQLite-pragma-based column structure for D1 and LibSQL (both SQLite-compatible).
/// `fk_prefix` is the schema name to use in the "schema.table.col" FK string (e.g. "main").
fn parse_column_structure_from_pragma_results(
    info_rows: Vec<Vec<serde_json::Value>>,
    fk_rows: Vec<Vec<serde_json::Value>>,
    fk_prefix: &str,
) -> Vec<ColumnStructureRow> {
    use std::collections::HashMap;
    let mut fk_map: HashMap<String, String> = Default::default();
    for r in &fk_rows {
        // PRAGMA foreign_key_list: id(0), seq(1), table(2), from(3), to(4)
        let from_col = r.get(3).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let ref_table = r.get(2).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let to_col = r.get(4).and_then(|v| v.as_str()).unwrap_or("").to_string();
        if !from_col.is_empty() {
            fk_map.insert(from_col, format!("{fk_prefix}.{ref_table}.{to_col}"));
        }
    }

    info_rows.iter().filter_map(|r| {
        // PRAGMA table_info: cid(0), name(1), type(2), notnull(3), dflt_value(4), pk(5)
        let cid = r.get(0).and_then(|v| v.as_i64()).unwrap_or(0);
        let name = r.get(1)?.as_str()?.to_string();
        let data_type = r.get(2).and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
        let notnull = r.get(3).and_then(|v| v.as_i64()).unwrap_or(0);
        let pk = r.get(5).and_then(|v| v.as_i64()).unwrap_or(0);
        let column_default = r.get(4).and_then(|v| v.as_str()).map(|s| s.to_string());
        let foreign_key = fk_map.get(&name).cloned();
        Some(ColumnStructureRow {
            ordinal_position: (cid + 1) as i32,
            name: name.clone(),
            data_type,
            is_nullable: notnull == 0,
            is_primary_key: pk > 0,
            column_default,
            foreign_key,
            fk_constraint_name: None,
            comment: None,
        })
    }).collect()
}

async fn get_column_structure_d1(
    cfg: &super::connection::D1Config,
    table: &str,
) -> Result<Vec<ColumnStructureRow>, String> {
    if let Ok(mut v) = sqlite_family_columns(SqliteFamily::D1(cfg), Some(table)).await {
        return Ok(v.pop().map(|t| t.columns).unwrap_or_default());
    }
    let tq = format!("\"{}\"", table.replace('"', "\"\""));

    let info_rows = super::d1::query(cfg, &format!("PRAGMA table_info({tq})"), vec![])
        .await
        .map(|r| r.rows)
        .map_err(|e| format!("PRAGMA table_info failed: {e}"))?;
    let fk_rows = super::d1::query(cfg, &format!("PRAGMA foreign_key_list({tq})"), vec![])
        .await
        .map(|r| r.rows)
        .unwrap_or_default();

    Ok(parse_column_structure_from_pragma_results(info_rows, fk_rows, "main"))
}

async fn get_column_structure_libsql(
    cfg: &super::connection::LibSqlConfig,
    table: &str,
) -> Result<Vec<ColumnStructureRow>, String> {
    if let Ok(mut v) = sqlite_family_columns(SqliteFamily::LibSql(cfg), Some(table)).await {
        return Ok(v.pop().map(|t| t.columns).unwrap_or_default());
    }
    let tq = format!("\"{}\"", table.replace('"', "\"\""));

    let info_rows = super::libsql::query(cfg, &format!("PRAGMA table_info({tq})"), vec![])
        .await
        .map(|r| r.rows)
        .map_err(|e| format!("PRAGMA table_info failed: {e}"))?;
    let fk_rows = super::libsql::query(cfg, &format!("PRAGMA foreign_key_list({tq})"), vec![])
        .await
        .map(|r| r.rows)
        .unwrap_or_default();

    Ok(parse_column_structure_from_pragma_results(info_rows, fk_rows, "main"))
}

async fn get_column_structure_pg(
    pool: &PgPool,
    schema: &str,
    table: &str,
) -> Result<Vec<ColumnStructureRow>, String> {
    // pg_catalog.pg_attribute is 10-100x faster than information_schema.columns on hosted
    // PostgreSQL (Supabase, RDS, etc.). FK lookups use OID-based matching; comments use
    // the built-in col_description() index function.
    let rows = sqlx::query(r#"
        SELECT
            a.attnum::int,
            a.attname::text,
            CASE
                WHEN t.typtype = 'b' AND t.typelem <> 0 AND t.typname LIKE '\_%'
                    THEN (SELECT bt.typname FROM pg_catalog.pg_type bt WHERE bt.oid = t.typelem) || '[]'
                WHEN a.atttypmod > 0 AND t.typname IN ('varchar','bpchar')
                    THEN t.typname || '(' || (a.atttypmod - 4)::text || ')'
                WHEN a.atttypmod > 0 AND t.typname = 'numeric' AND a.atttypmod <> -1
                    THEN 'numeric(' || (((a.atttypmod - 4) >> 16) & 65535)::text
                        || ',' || ((a.atttypmod - 4) & 65535)::text || ')'
                WHEN a.atttypmod > 0 AND t.typname IN ('bit','varbit')
                    THEN t.typname || '(' || a.atttypmod::text || ')'
                ELSE t.typname
            END,
            NOT a.attnotnull,
            pg_get_expr(ad.adbin, ad.adrelid),
            fk.ref,
            fk.conname,
            col_description(a.attrelid, a.attnum),
            EXISTS (
                SELECT 1 FROM pg_catalog.pg_constraint pk
                WHERE pk.contype = 'p' AND pk.conrelid = a.attrelid AND a.attnum = ANY(pk.conkey)
            )
        FROM pg_catalog.pg_attribute a
        JOIN pg_catalog.pg_class c ON c.oid = a.attrelid
        JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_catalog.pg_type t ON t.oid = a.atttypid
        LEFT JOIN pg_catalog.pg_attrdef ad ON ad.adrelid = a.attrelid AND ad.adnum = a.attnum
        -- The foreign key this column is part of, matched by position, so every
        -- column of a composite key is marked (conkey[1] only marked the first).
        LEFT JOIN LATERAL (
            SELECT rns.nspname || '.' || rt.relname || '.' || ra.attname AS ref, pc.conname::text AS conname
            FROM pg_catalog.pg_constraint pc
            JOIN pg_catalog.pg_class rt ON rt.oid = pc.confrelid
            JOIN pg_catalog.pg_namespace rns ON rns.oid = rt.relnamespace
            JOIN pg_catalog.pg_attribute ra ON ra.attrelid = rt.oid
                AND ra.attnum = pc.confkey[array_position(pc.conkey, a.attnum)]
            WHERE pc.contype = 'f' AND pc.conrelid = a.attrelid AND a.attnum = ANY(pc.conkey)
            ORDER BY pc.conname
            LIMIT 1
        ) fk ON true
        WHERE n.nspname = $1 AND c.relname = $2
          AND a.attnum > 0 AND NOT a.attisdropped
        ORDER BY a.attnum
    "#)
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Column structure query failed: {e}"))?;

    let result = rows
        .iter()
        .map(|r| {
            let ordinal: i32 = r.try_get(0).unwrap_or(0);
            let name: String = r.try_get(1).unwrap_or_default();
            let data_type: String = r.try_get(2).unwrap_or_default();
            let is_nullable: bool = r.try_get(3).unwrap_or(true);
            let column_default: Option<String> = r.try_get(4).ok().flatten();
            let foreign_key: Option<String> = r.try_get(5).ok().flatten();
            let fk_constraint_name: Option<String> = r.try_get(6).ok().flatten();
            let comment: Option<String> = r.try_get(7).ok().flatten();
            let is_primary_key: bool = r.try_get(8).unwrap_or(false);
            ColumnStructureRow { ordinal_position: ordinal, name, data_type, is_nullable, column_default, foreign_key, fk_constraint_name, comment, is_primary_key }
        })
        .collect();

    Ok(result)
}

// ── Schema-wide column structure (ERD) ───────────────────────────────────────

/// Every table's columns for one schema, in a single round trip.
///
/// The ER diagram needs the full column list for the whole schema before it can
/// draw anything. Asking table by table cost two catalog queries and one IPC hop
/// *per table*, so a 120-table schema spent most of its load time on round-trip
/// latency. Every SQL engine answers from one catalog statement (SQLite and its
/// remote flavours through the pragma table functions); an engine that cannot
/// falls back to a server-side loop, so the frontend waits on one call either way.
///
/// MongoDB, CouchDB, Firestore, DynamoDB and Redis have no schema-wide column
/// catalog: they answer empty at once, and the caller reads per table.
pub async fn get_schema_column_structure(
    state: State<'_, DbState>,
    schema: String,
) -> Result<Vec<TableColumnStructure>, String> {
    match require_conn(&state)? {
        ActiveConnection::Doc(d) => return super::docdb::schema_columns_in(&d, &schema).await,
        ActiveConnection::Dynamodb(_) | ActiveConnection::Redis(_) => return Ok(vec![]),
        ActiveConnection::Oracle(h) => return super::oracle::schema_column_structure(&h, &schema).await,
        _ => {}
    }
    validate_ident(&schema)?;
    let bulk = match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => return schema_column_structure_pg(&pool, &schema).await,
        ActiveConnection::Mysql(pool) => return schema_column_structure_mysql(&pool, &schema).await,
        ActiveConnection::Sqlite(pool) => sqlite_family_columns(SqliteFamily::Local(&pool), None).await,
        ActiveConnection::D1(cfg) => sqlite_family_columns(SqliteFamily::D1(&cfg), None).await,
        ActiveConnection::LibSql(cfg) => sqlite_family_columns(SqliteFamily::LibSql(&cfg), None).await,
        ActiveConnection::Clickhouse(cfg) => super::clickhouse::schema_column_structure(&cfg, &schema).await,
        ActiveConnection::Duckdb(h) => super::duckdb::schema_column_structure(&h).await,
        ActiveConnection::Mssql(h) => super::mssql::schema_column_structure(&h, &schema).await,
        _ => Err("no bulk catalog query".into()),
    };
    match bulk {
        Ok(v) => Ok(v),
        Err(_) => {
            // No bulk catalog query worked (an old SQLite without the pragma
            // table functions, a view over a dropped table failing the whole
            // statement): fan out over the per-table path here rather than
            // making the frontend do it one invoke at a time.
            let tables = list_tables(state.clone(), schema.clone()).await?;
            let mut out = Vec::with_capacity(tables.len());
            for t in tables {
                let columns = get_table_column_structure(state.clone(), schema.clone(), t.name.clone()).await?;
                out.push(TableColumnStructure { table: t.name, columns });
            }
            Ok(out)
        }
    }
}

/// Groups flat `(table, column…)` rows into per-table lists, preserving the
/// order the query returned them in (table name, then ordinal position).
fn group_by_table(rows: Vec<(String, ColumnStructureRow)>) -> Vec<TableColumnStructure> {
    let mut out: Vec<TableColumnStructure> = Vec::new();
    for (table, col) in rows {
        match out.last_mut() {
            Some(last) if last.table == table => last.columns.push(col),
            _ => out.push(TableColumnStructure { table, columns: vec![col] }),
        }
    }
    out
}

async fn schema_column_structure_pg(
    pool: &PgPool,
    schema: &str,
) -> Result<Vec<TableColumnStructure>, String> {
    // Same projection as get_column_structure_pg, minus the relname filter and
    // with the table name carried through so the rows can be regrouped.
    let rows = sqlx::query(r#"
        SELECT
            c.relname::text,
            a.attnum::int,
            a.attname::text,
            CASE
                WHEN t.typtype = 'b' AND t.typelem <> 0 AND t.typname LIKE '\_%'
                    THEN (SELECT bt.typname FROM pg_catalog.pg_type bt WHERE bt.oid = t.typelem) || '[]'
                WHEN a.atttypmod > 0 AND t.typname IN ('varchar','bpchar')
                    THEN t.typname || '(' || (a.atttypmod - 4)::text || ')'
                WHEN a.atttypmod > 0 AND t.typname = 'numeric' AND a.atttypmod <> -1
                    THEN 'numeric(' || (((a.atttypmod - 4) >> 16) & 65535)::text
                        || ',' || ((a.atttypmod - 4) & 65535)::text || ')'
                WHEN a.atttypmod > 0 AND t.typname IN ('bit','varbit')
                    THEN t.typname || '(' || a.atttypmod::text || ')'
                ELSE t.typname
            END,
            NOT a.attnotnull,
            pg_get_expr(ad.adbin, ad.adrelid),
            fk.ref,
            fk.conname,
            col_description(a.attrelid, a.attnum),
            EXISTS (
                SELECT 1 FROM pg_catalog.pg_constraint pk
                WHERE pk.contype = 'p' AND pk.conrelid = a.attrelid AND a.attnum = ANY(pk.conkey)
            )
        FROM pg_catalog.pg_attribute a
        JOIN pg_catalog.pg_class c ON c.oid = a.attrelid
        JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_catalog.pg_type t ON t.oid = a.atttypid
        LEFT JOIN pg_catalog.pg_attrdef ad ON ad.adrelid = a.attrelid AND ad.adnum = a.attnum
        -- The foreign key this column is part of, matched by position, so every
        -- column of a composite key is marked (conkey[1] only marked the first).
        LEFT JOIN LATERAL (
            SELECT rns.nspname || '.' || rt.relname || '.' || ra.attname AS ref, pc.conname::text AS conname
            FROM pg_catalog.pg_constraint pc
            JOIN pg_catalog.pg_class rt ON rt.oid = pc.confrelid
            JOIN pg_catalog.pg_namespace rns ON rns.oid = rt.relnamespace
            JOIN pg_catalog.pg_attribute ra ON ra.attrelid = rt.oid
                AND ra.attnum = pc.confkey[array_position(pc.conkey, a.attnum)]
            WHERE pc.contype = 'f' AND pc.conrelid = a.attrelid AND a.attnum = ANY(pc.conkey)
            ORDER BY pc.conname
            LIMIT 1
        ) fk ON true
        WHERE n.nspname = $1
          AND c.relkind IN ('r','p','v','m','f')
          AND a.attnum > 0 AND NOT a.attisdropped
        ORDER BY c.relname, a.attnum
    "#)
    .bind(schema)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Schema column structure query failed: {e}"))?;

    Ok(group_by_table(rows.iter().map(|r| {
        let table: String = r.try_get(0).unwrap_or_default();
        (table, ColumnStructureRow {
            ordinal_position: r.try_get(1).unwrap_or(0),
            name: r.try_get(2).unwrap_or_default(),
            data_type: r.try_get(3).unwrap_or_default(),
            is_nullable: r.try_get(4).unwrap_or(true),
            column_default: r.try_get(5).ok().flatten(),
            foreign_key: r.try_get(6).ok().flatten(),
            fk_constraint_name: r.try_get(7).ok().flatten(),
            comment: r.try_get(8).ok().flatten(),
            is_primary_key: r.try_get(9).unwrap_or(false),
        })
    }).collect()))
}

async fn schema_column_structure_mysql(
    pool: &MySqlPool,
    schema: &str,
) -> Result<Vec<TableColumnStructure>, String> {
    use std::collections::HashMap;

    // The column and foreign key reads go out together (the pool has more than
    // one connection), so the schema costs one round trip of latency, not two.
    let cols_q = sqlx::query(
        "SELECT TABLE_NAME, ORDINAL_POSITION, COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT, COLUMN_KEY \
         FROM information_schema.COLUMNS \
         WHERE TABLE_SCHEMA = ? \
         ORDER BY TABLE_NAME, ORDINAL_POSITION",
    )
    .bind(schema)
    .fetch_all(pool);

    // One FK pass for the whole schema, keyed by (table, column).
    let fk_q = sqlx::query(
        "SELECT kcu.TABLE_NAME, kcu.COLUMN_NAME, kcu.REFERENCED_TABLE_SCHEMA, \
                kcu.REFERENCED_TABLE_NAME, kcu.REFERENCED_COLUMN_NAME, kcu.CONSTRAINT_NAME \
         FROM information_schema.KEY_COLUMN_USAGE kcu \
         WHERE kcu.TABLE_SCHEMA = ? AND kcu.REFERENCED_TABLE_NAME IS NOT NULL \
         ORDER BY kcu.CONSTRAINT_NAME, kcu.ORDINAL_POSITION",
    )
    .bind(schema)
    .fetch_all(pool);
    let (col_rows, fk_rows) = tokio::join!(cols_q, fk_q);
    let col_rows = col_rows.map_err(|e| format!("Failed to get column info: {e}"))?;
    let fk_rows = fk_rows.unwrap_or_default();

    let mut fk_map: HashMap<(String, String), (String, String)> = Default::default();
    for row in &fk_rows {
        let table: String = my_text(row, 0).unwrap_or_default();
        let col: String = my_text(row, 1).unwrap_or_default();
        let ref_schema: String = my_text(row, 2).unwrap_or_default();
        let ref_table: String = my_text(row, 3).unwrap_or_default();
        let ref_col: String = my_text(row, 4).unwrap_or_default();
        let constraint: String = my_text(row, 5).unwrap_or_default();
        fk_map.insert((table, col), (format!("{ref_schema}.{ref_table}.{ref_col}"), constraint));
    }

    Ok(group_by_table(col_rows.iter().map(|r| {
        let table: String = my_text(r, 0).unwrap_or_default();
        let ordinal: u32 = r.try_get(1).unwrap_or(0);
        let name: String = my_text(r, 2).unwrap_or_default();
        let data_type: String = my_text(r, 3).unwrap_or_default();
        let nullable: String = my_text(r, 4).unwrap_or_default();
        let column_default: Option<String> = my_text(r, 5);
        let fk = fk_map.get(&(table.clone(), name.clone()));
        (table, ColumnStructureRow {
            ordinal_position: ordinal as i32,
            name,
            data_type,
            is_nullable: nullable.eq_ignore_ascii_case("YES"),
            is_primary_key: my_text(r, 6).map(|k| k.eq_ignore_ascii_case("PRI")).unwrap_or(false),
            column_default,
            foreign_key: fk.map(|(t, _)| t.clone()),
            fk_constraint_name: fk.map(|(_, c)| c.clone()),
            comment: None,
        })
    }).collect()))
}

// ── SQLite family: one catalog statement per schema ──────────────────────────
//
// SQLite, D1 and libSQL answer the tree's column and index questions through
// the pragma table functions joined to `sqlite_master`: one statement for every
// table, where the pragma statements needed two per table. The remote flavours
// pay a network round trip per statement, so this is the difference between one
// request and ten thousand on a 5k-table database.

/// Where a SQLite-family statement runs.
#[derive(Clone, Copy)]
enum SqliteFamily<'a> {
    Local(&'a sqlx::SqlitePool),
    D1(&'a super::connection::D1Config),
    LibSql(&'a super::connection::LibSqlConfig),
}

/// One local row as JSON, so the three flavours share one decoder. Reads each
/// cell by its storage class: guessing a type and catching the mismatch built
/// an error per text cell, which was most of the time on a large catalog.
fn sqlite_row_values(r: &sqlx::sqlite::SqliteRow) -> Vec<serde_json::Value> {
    use sqlx::{TypeInfo, ValueRef};
    (0..r.len())
        .map(|i| {
            let class = match r.try_get_raw(i) {
                Ok(v) if !v.is_null() => v.type_info().name().to_string(),
                _ => return serde_json::Value::Null,
            };
            match class.as_str() {
                "INTEGER" | "BOOLEAN" => r.try_get::<i64, _>(i).map(Into::into).unwrap_or_default(),
                "REAL" => r.try_get::<f64, _>(i).map(Into::into).unwrap_or_default(),
                _ => r.try_get::<String, _>(i).map(Into::into).unwrap_or_default(),
            }
        })
        .collect()
}

/// Run one statement with optional text parameters, rows as JSON.
async fn sqlite_family_rows(
    at: SqliteFamily<'_>,
    sql: &str,
    params: &[&str],
) -> Result<Vec<Vec<serde_json::Value>>, String> {
    match at {
        SqliteFamily::Local(pool) => {
            let mut q = sqlx::query(sql);
            for p in params {
                q = q.bind(p.to_string());
            }
            let rows = q.fetch_all(pool).await.map_err(|e| e.to_string())?;
            Ok(rows.iter().map(sqlite_row_values).collect())
        }
        SqliteFamily::D1(cfg) => {
            let ps = params.iter().map(|p| serde_json::Value::from(*p)).collect();
            Ok(super::d1::query(cfg, sql, ps).await?.rows)
        }
        SqliteFamily::LibSql(cfg) => {
            let ps = params.iter().map(|p| serde_json::Value::from(*p)).collect();
            Ok(super::libsql::query(cfg, sql, ps).await?.rows)
        }
    }
}

/// Every result column has its own alias: D1 and libSQL return rows as JSON
/// objects keyed by column name, so two columns both called `name` collapsed
/// into one and shifted every value after them (the diagram drew each column
/// as a table of its own).
///
/// Every column of every table and view (`c` rows: table, cid, name, type,
/// notnull, default, pk), then every foreign key column (`f` rows: table, id,
/// from, referenced table, to, seq). Two lists in one statement: joining the
/// foreign keys onto the columns re-ran the foreign key pragma for every
/// column, eight times slower. `_cf_*` are D1's own tables: reading them is
/// refused, and one refusal would fail the whole statement. `{one}` narrows
/// both halves to one table.
const SQLITE_COLUMNS_SQL: &str = r#"
    SELECT 'c' AS k, m.name AS t, p.cid AS n, p.name AS c, p.type AS ty, p."notnull" AS nn, p.dflt_value AS dv, p.pk AS pk
    FROM sqlite_master m JOIN pragma_table_info(m.name) p
    WHERE m.type IN ('table', 'view') AND m.name NOT LIKE '\_cf\_%' ESCAPE '\'{one}
    UNION ALL
    SELECT 'f', m.name, f.id, f."from", f."table", f."to", f.seq, NULL
    FROM sqlite_master m JOIN pragma_foreign_key_list(m.name) f
    WHERE m.type = 'table' AND m.name NOT LIKE '\_cf\_%' ESCAPE '\'{one}
    ORDER BY 1, 2, 3, 7"#;

/// Every index of every table, with its columns in key order, where it came
/// from (`pk` primary key, `u` UNIQUE constraint, `c` CREATE INDEX) and, for a
/// partial index, its statement (the WHERE clause is read from it; looked up
/// only for partial ones, as `sqlite_master` has no index on name).
const SQLITE_INDEXES_SQL: &str = r#"
    SELECT m.name AS t, il.name AS i, il."unique" AS u, il.origin AS o, il.partial AS p,
           (SELECT group_concat(coalesce(ii.name, '(expression)'), ', ')
              FROM (SELECT name FROM pragma_index_info(il.name) ORDER BY seqno) ii) AS cols,
           CASE WHEN il.partial THEN (SELECT s.sql FROM sqlite_master s WHERE s.type = 'index' AND s.name = il.name) END AS wh
    FROM sqlite_master m
    JOIN pragma_index_list(m.name) il
    WHERE m.type = 'table' AND m.name NOT LIKE '\_cf\_%' ESCAPE '\'
    ORDER BY m.name, il.origin = 'pk' DESC, il.name"#;

fn json_text(v: Option<&serde_json::Value>) -> Option<String> {
    match v? {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn json_int(v: Option<&serde_json::Value>) -> Option<i64> {
    match v? {
        serde_json::Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        serde_json::Value::String(s) => s.parse().ok(),
        serde_json::Value::Bool(b) => Some(*b as i64),
        _ => None,
    }
}

/// SQLite keeps no foreign key names. Name one the way Postgres would, after
/// its table and columns, so a two-column key reads as one key, not two.
fn sqlite_fk_name(table: &str, cols: &[String]) -> String {
    format!("{table}_{}_fkey", cols.join("_"))
}

/// Rows of `SQLITE_COLUMNS_SQL` as per-table lists.
fn sqlite_columns_from_rows(rows: &[Vec<serde_json::Value>]) -> Vec<TableColumnStructure> {
    use std::collections::HashMap;
    let is = |r: &Vec<serde_json::Value>, k: &str| r.first().and_then(|v| v.as_str()) == Some(k);
    // Foreign keys first: each column's first key, and each key's columns, so
    // every column of a composite key carries the same name.
    let mut fk_cols: HashMap<(String, i64), Vec<String>> = HashMap::new();
    let mut fk_of: HashMap<(String, String), (i64, String)> = HashMap::new();
    for r in rows.iter().filter(|r| is(r, "f")) {
        let (Some(t), Some(id), Some(from)) = (json_text(r.get(1)), json_int(r.get(2)), json_text(r.get(3))) else { continue };
        let target = format!("main.{}.{}", json_text(r.get(4)).unwrap_or_default(), json_text(r.get(5)).unwrap_or_default());
        fk_cols.entry((t.clone(), id)).or_default().push(from.clone());
        fk_of.entry((t, from)).or_insert((id, target));
    }
    let flat = rows
        .iter()
        .filter(|r| is(r, "c"))
        .filter_map(|r| {
            let (table, cid, name) = (json_text(r.get(1))?, json_int(r.get(2))?, json_text(r.get(3))?);
            let fk = fk_of.get(&(table.clone(), name.clone()));
            let fk_constraint_name = fk.map(|(id, _)| sqlite_fk_name(&table, fk_cols.get(&(table.clone(), *id)).map(Vec::as_slice).unwrap_or_default()));
            let column = ColumnStructureRow {
                ordinal_position: (cid + 1) as i32,
                data_type: json_text(r.get(4)).unwrap_or_default().to_lowercase(),
                is_nullable: json_int(r.get(5)).unwrap_or(0) == 0,
                column_default: json_text(r.get(6)),
                is_primary_key: json_int(r.get(7)).unwrap_or(0) > 0,
                foreign_key: fk.map(|(_, target)| target.clone()),
                fk_constraint_name,
                comment: None,
                name,
            };
            Some((table, column))
        })
        .collect();
    group_by_table(flat)
}

/// Rows of `SQLITE_INDEXES_SQL` as index entries.
fn sqlite_indexes_from_rows(rows: &[Vec<serde_json::Value>]) -> Vec<IndexInfo> {
    rows.iter()
        .filter_map(|r| {
            let table_name = json_text(r.first())?;
            let name = json_text(r.get(1))?;
            let origin = json_text(r.get(3)).unwrap_or_default();
            let partial = json_int(r.get(4)).unwrap_or(0) != 0;
            let condition = if partial {
                json_text(r.get(6)).and_then(|sql| {
                    let at = sql.to_ascii_uppercase().rfind(" WHERE ")?;
                    Some(sql[at + 7..].trim().trim_end_matches(';').trim().to_string())
                })
            } else {
                None
            };
            Some(IndexInfo {
                name,
                table_name,
                columns: json_text(r.get(5)).unwrap_or_default(),
                index_type: "btree".to_string(),
                is_unique: json_int(r.get(2)).unwrap_or(0) != 0,
                is_primary: origin == "pk",
                condition,
                comment: None,
            })
        })
        .collect()
}

/// Columns for every table (or the one named), in one statement.
async fn sqlite_family_columns(at: SqliteFamily<'_>, table: Option<&str>) -> Result<Vec<TableColumnStructure>, String> {
    let sql = SQLITE_COLUMNS_SQL.replace("{one}", if table.is_some() { " AND m.name = ?" } else { "" });
    let params: Vec<&str> = table.map(|t| vec![t, t]).unwrap_or_default();
    Ok(sqlite_columns_from_rows(&sqlite_family_rows(at, &sql, &params).await?))
}

async fn sqlite_family_indexes(at: SqliteFamily<'_>) -> Result<Vec<IndexInfo>, String> {
    Ok(sqlite_indexes_from_rows(&sqlite_family_rows(at, SQLITE_INDEXES_SQL, &[]).await?))
}

async fn list_indexes_sqlite(pool: &sqlx::SqlitePool) -> Result<Vec<IndexInfo>, String> {
    match sqlite_family_indexes(SqliteFamily::Local(pool)).await {
        Ok(v) => Ok(v),
        Err(_) => list_indexes_sqlite_master(pool).await,
    }
}

async fn list_indexes_d1(cfg: &super::connection::D1Config) -> Result<Vec<IndexInfo>, String> {
    match sqlite_family_indexes(SqliteFamily::D1(cfg)).await {
        Ok(v) => Ok(v),
        Err(_) => list_indexes_d1_master(cfg).await,
    }
}

async fn list_indexes_libsql(cfg: &super::connection::LibSqlConfig) -> Result<Vec<IndexInfo>, String> {
    match sqlite_family_indexes(SqliteFamily::LibSql(cfg)).await {
        Ok(v) => Ok(v),
        Err(_) => list_indexes_libsql_master(cfg).await,
    }
}

// ── Incoming foreign keys (reverse / one-to-many relationships) ───────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingForeignKey {
    pub from_schema: String,
    pub from_table: String,
    pub from_columns: Vec<String>,
    pub to_columns: Vec<String>,
    pub constraint_name: String,
}

async fn get_incoming_fks_pg(pool: &PgPool, schema: &str, table: &str) -> Result<Vec<IncomingForeignKey>, String> {
    let rows = sqlx::query(r#"
        SELECT
            fn.nspname::text,
            fc.relname::text,
            ARRAY(SELECT a.attname::text FROM pg_catalog.pg_attribute a
                  WHERE a.attrelid = pc.conrelid AND a.attnum = ANY(pc.conkey)
                  ORDER BY array_position(pc.conkey, a.attnum))::text[],
            ARRAY(SELECT a.attname::text FROM pg_catalog.pg_attribute a
                  WHERE a.attrelid = pc.confrelid AND a.attnum = ANY(pc.confkey)
                  ORDER BY array_position(pc.confkey, a.attnum))::text[],
            pc.conname::text
        FROM pg_catalog.pg_constraint pc
        JOIN pg_catalog.pg_class       fc  ON fc.oid = pc.conrelid
        JOIN pg_catalog.pg_namespace   fn  ON fn.oid = fc.relnamespace
        JOIN pg_catalog.pg_class       tc  ON tc.oid = pc.confrelid
        JOIN pg_catalog.pg_namespace   tn  ON tn.oid = tc.relnamespace
        WHERE pc.contype = 'f'
          AND tn.nspname = $1
          AND tc.relname = $2
        ORDER BY fn.nspname, fc.relname, pc.conname
    "#)
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Incoming FK query failed: {e}"))?;

    Ok(rows.iter().filter_map(|r| Some(IncomingForeignKey {
        from_schema:     r.try_get(0).ok()?,
        from_table:      r.try_get(1).ok()?,
        from_columns:    r.try_get(2).unwrap_or_default(),
        to_columns:      r.try_get(3).unwrap_or_default(),
        constraint_name: r.try_get(4).unwrap_or_default(),
    })).collect())
}

async fn get_incoming_fks_mysql(pool: &MySqlPool, schema: &str, table: &str) -> Result<Vec<IncomingForeignKey>, String> {
    let rows = sqlx::query(r#"
        SELECT kcu.table_schema, kcu.table_name, kcu.column_name,
               kcu.referenced_column_name, kcu.constraint_name
        FROM information_schema.key_column_usage kcu
        JOIN information_schema.table_constraints tc
            ON tc.constraint_name = kcu.constraint_name
           AND tc.table_schema    = kcu.table_schema
           AND tc.table_name      = kcu.table_name
        WHERE tc.constraint_type          = 'FOREIGN KEY'
          AND kcu.referenced_table_schema = ?
          AND kcu.referenced_table_name   = ?
        ORDER BY kcu.table_name, kcu.constraint_name, kcu.ordinal_position
    "#)
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Incoming FK query failed: {e}"))?;

    let mut map: std::collections::BTreeMap<(String, String, String), (Vec<String>, Vec<String>)> = Default::default();
    for r in &rows {
        let fs: String = my_text(r, 0).unwrap_or_default();
        let ft: String = my_text(r, 1).unwrap_or_default();
        let fc: String = my_text(r, 2).unwrap_or_default();
        let tc: String = my_text(r, 3).unwrap_or_default();
        let cn: String = my_text(r, 4).unwrap_or_default();
        let e = map.entry((fs, ft, cn)).or_default();
        e.0.push(fc); e.1.push(tc);
    }
    Ok(map.into_iter().map(|((fs, ft, cn), (fc, tc))| IncomingForeignKey {
        from_schema: fs, from_table: ft, from_columns: fc, to_columns: tc, constraint_name: cn,
    }).collect())
}

async fn get_incoming_fks_sqlite(pool: &sqlx::SqlitePool, table: &str) -> Result<Vec<IncomingForeignKey>, String> {
    let tables: Vec<String> = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"
    ).fetch_all(pool).await.unwrap_or_default()
     .iter().filter_map(|r| r.try_get::<String, _>(0).ok()).collect();

    let mut result = Vec::new();
    for from_table in tables {
        let tq = format!("\"{}\"", from_table.replace('"', "\"\""));
        let fk_rows = sqlx::query(&format!("PRAGMA foreign_key_list({tq})"))
            .fetch_all(pool).await.unwrap_or_default();
        let mut fk_map: std::collections::BTreeMap<i64, (Vec<String>, Vec<String>)> = Default::default();
        for r in &fk_rows {
            let id: i64 = r.try_get(0).unwrap_or(0);
            let ref_table: String = r.try_get::<Option<String>, _>(2).ok().flatten().unwrap_or_default();
            if !ref_table.eq_ignore_ascii_case(table) { continue; }
            let fc: String = r.try_get::<Option<String>, _>(3).ok().flatten().unwrap_or_default();
            let tc: String = r.try_get::<Option<String>, _>(4).ok().flatten().unwrap_or_default();
            let e = fk_map.entry(id).or_default(); e.0.push(fc); e.1.push(tc);
        }
        for (_, (from_columns, to_columns)) in fk_map {
            result.push(IncomingForeignKey {
                from_schema: "main".to_string(), from_table: from_table.clone(),
                from_columns, to_columns, constraint_name: String::new(),
            });
        }
    }
    Ok(result)
}

/// Group `PRAGMA foreign_key_list` JSON rows (id, seq, table, from, to, ...) whose
/// referenced `table` equals `target` into per-constraint IncomingForeignKey entries.
/// Shared by the SQLite-shaped-over-HTTP engines (D1, LibSQL).
fn incoming_fks_from_pragma_json(
    from_table: &str,
    fk_rows: &[Vec<serde_json::Value>],
    target: &str,
) -> Vec<IncomingForeignKey> {
    let mut fk_map: std::collections::BTreeMap<i64, (Vec<String>, Vec<String>)> = Default::default();
    for r in fk_rows {
        let ref_table = r.get(2).and_then(|v| v.as_str()).unwrap_or("");
        if !ref_table.eq_ignore_ascii_case(target) { continue; }
        let id = r.get(0).and_then(|v| v.as_i64()).unwrap_or(0);
        let fc = r.get(3).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let tc = r.get(4).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let e = fk_map.entry(id).or_default(); e.0.push(fc); e.1.push(tc);
    }
    fk_map.into_iter().map(|(_, (from_columns, to_columns))| IncomingForeignKey {
        from_schema: "main".to_string(), from_table: from_table.to_string(),
        from_columns, to_columns, constraint_name: String::new(),
    }).collect()
}

/// One batched query for a chunk of tables, replacing one `PRAGMA
/// foreign_key_list` HTTP round-trip per table: SQLite >= 3.16 (both D1 and
/// libSQL) exposes the pragma as a table-valued function, so a chunk of tables
/// UNION ALLs into a single scan. `ORDER BY src, id, seq` keeps each
/// constraint's columns in key order regardless of how the engine merges the
/// branches.
fn batched_incoming_fk_sql(tables: &[String]) -> String {
    let mut sql = tables
        .iter()
        .map(|t| {
            let lit = t.replace('\'', "''");
            format!(
                "SELECT fk.id AS id, fk.seq AS seq, fk.\"table\" AS \"table\", \
                 fk.\"from\" AS \"from\", fk.\"to\" AS \"to\", '{lit}' AS src \
                 FROM pragma_foreign_key_list('{lit}') fk"
            )
        })
        .collect::<Vec<_>>()
        .join(" UNION ALL ");
    sql.push_str(" ORDER BY src, id, seq");
    sql
}

/// Group the batched scan's rows into per-constraint entries, keeping `chunk`
/// (table-list) order like the per-table loop did. Returns `None` when the
/// expected columns are missing, so the caller can fall back to per-table
/// PRAGMAs.
fn incoming_fks_from_batched(
    res: &super::query::SqlResult,
    chunk: &[String],
    target: &str,
) -> Option<Vec<IncomingForeignKey>> {
    // A successful empty result means "no FKs in this chunk" - it must not fall
    // back: D1 derives column names from row keys, so zero rows = zero columns.
    if res.rows.is_empty() {
        return Some(Vec::new());
    }
    let idx = |n: &str| res.columns.iter().position(|c| c.name == n);
    let (src_i, id_i, tbl_i, from_i, to_i) =
        (idx("src")?, idx("id")?, idx("table")?, idx("from")?, idx("to")?);

    // Rows per source table, in arrival (id, seq) order.
    let mut by_src: std::collections::HashMap<&str, Vec<&Vec<serde_json::Value>>> = Default::default();
    for r in &res.rows {
        if let Some(src) = r.get(src_i).and_then(|v| v.as_str()) {
            by_src.entry(src).or_default().push(r);
        }
    }

    let mut result = Vec::new();
    for from_table in chunk {
        let mut fk_map: std::collections::BTreeMap<i64, (Vec<String>, Vec<String>)> = Default::default();
        for r in by_src.get(from_table.as_str()).map(|v| v.as_slice()).unwrap_or(&[]) {
            let ref_table = r.get(tbl_i).and_then(|v| v.as_str()).unwrap_or("");
            if !ref_table.eq_ignore_ascii_case(target) { continue; }
            let id = r.get(id_i).and_then(|v| v.as_i64()).unwrap_or(0);
            let fc = r.get(from_i).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let tc = r.get(to_i).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let e = fk_map.entry(id).or_default(); e.0.push(fc); e.1.push(tc);
        }
        for (_, (from_columns, to_columns)) in fk_map {
            result.push(IncomingForeignKey {
                from_schema: "main".to_string(), from_table: from_table.clone(),
                from_columns, to_columns, constraint_name: String::new(),
            });
        }
    }
    Some(result)
}

/// Every foreign key that points at one table, in a single statement: the
/// pragma joined onto the table list and filtered on the server. It replaces a
/// table listing plus a batched scan, and on D1 that scan never worked - it
/// named the `_cf_*` tables, whose refusal fails the whole statement - so each
/// table open fell back to one request per table (4s measured). Columns: src,
/// id, from, to, in key order.
const SQLITE_INCOMING_FKS_SQL: &str = r#"
    SELECT m.name, f.id, f."from", f."to"
    FROM sqlite_master m JOIN pragma_foreign_key_list(m.name) f
    WHERE m.type = 'table' AND m.name NOT LIKE '\_cf\_%' ESCAPE '\'
      AND lower(f."table") = lower(?1)
    ORDER BY m.name, f.id, f.seq"#;

/// Rows of `SQLITE_INCOMING_FKS_SQL` as one entry per constraint.
fn incoming_fks_from_joined(rows: &[Vec<serde_json::Value>]) -> Vec<IncomingForeignKey> {
    let mut result: Vec<IncomingForeignKey> = Vec::new();
    let mut last: Option<(String, i64)> = None;
    for r in rows {
        let Some(src) = json_text(r.first()) else { continue };
        let key = (src, json_int(r.get(1)).unwrap_or(0));
        if last.as_ref() != Some(&key) {
            result.push(IncomingForeignKey {
                from_schema: "main".to_string(), from_table: key.0.clone(),
                from_columns: Vec::new(), to_columns: Vec::new(), constraint_name: String::new(),
            });
            last = Some(key);
        }
        if let Some(fk) = result.last_mut() {
            fk.from_columns.push(json_text(r.get(2)).unwrap_or_default());
            fk.to_columns.push(json_text(r.get(3)).unwrap_or_default());
        }
    }
    result
}

async fn get_incoming_fks_d1(cfg: &super::connection::D1Config, table: &str) -> Result<Vec<IncomingForeignKey>, String> {
    if let Ok(rows) = sqlite_family_rows(SqliteFamily::D1(cfg), SQLITE_INCOMING_FKS_SQL, &[table]).await {
        return Ok(incoming_fks_from_joined(&rows));
    }
    let tables: Vec<String> = super::d1::query(
        cfg,
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        vec![],
    )
    .await
    .map_err(|e| format!("Failed to list tables: {e}"))?
    .rows.iter().filter_map(|r| r.first().and_then(|v| v.as_str()).map(String::from)).collect();

    let mut result = Vec::new();
    for chunk in tables.chunks(SQLITE_COUNT_BATCH) {
        let batched = super::d1::query(cfg, &batched_incoming_fk_sql(chunk), vec![])
            .await.ok()
            .and_then(|res| incoming_fks_from_batched(&res, chunk, table));
        match batched {
            Some(mut fks) => result.append(&mut fks),
            // Table-valued pragma unavailable - per-table PRAGMA fallback.
            None => {
                for from_table in chunk {
                    let tq = format!("\"{}\"", from_table.replace('"', "\"\""));
                    let fk_rows = super::d1::query(cfg, &format!("PRAGMA foreign_key_list({tq})"), vec![])
                        .await.map(|r| r.rows).unwrap_or_default();
                    result.extend(incoming_fks_from_pragma_json(from_table, &fk_rows, table));
                }
            }
        }
    }
    Ok(result)
}

async fn get_incoming_fks_libsql(cfg: &super::connection::LibSqlConfig, table: &str) -> Result<Vec<IncomingForeignKey>, String> {
    if let Ok(rows) = sqlite_family_rows(SqliteFamily::LibSql(cfg), SQLITE_INCOMING_FKS_SQL, &[table]).await {
        return Ok(incoming_fks_from_joined(&rows));
    }
    let tables: Vec<String> = super::libsql::query(
        cfg,
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        vec![],
    )
    .await
    .map_err(|e| format!("Failed to list tables: {e}"))?
    .rows.iter().filter_map(|r| r.first().and_then(|v| v.as_str()).map(String::from)).collect();

    let mut result = Vec::new();
    for chunk in tables.chunks(SQLITE_COUNT_BATCH) {
        let batched = super::libsql::query(cfg, &batched_incoming_fk_sql(chunk), vec![])
            .await.ok()
            .and_then(|res| incoming_fks_from_batched(&res, chunk, table));
        match batched {
            Some(mut fks) => result.append(&mut fks),
            // Table-valued pragma unavailable - per-table PRAGMA fallback.
            None => {
                for from_table in chunk {
                    let tq = format!("\"{}\"", from_table.replace('"', "\"\""));
                    let fk_rows = super::libsql::query(cfg, &format!("PRAGMA foreign_key_list({tq})"), vec![])
                        .await.map(|r| r.rows).unwrap_or_default();
                    result.extend(incoming_fks_from_pragma_json(from_table, &fk_rows, table));
                }
            }
        }
    }
    Ok(result)
}

async fn get_incoming_fks_duckdb(handle: &super::connection::DuckdbHandle, table: &str) -> Result<Vec<IncomingForeignKey>, String> {
    let t = table.replace('\'', "''");
    let sql = format!(
        "SELECT schema_name, table_name, array_to_string(constraint_column_names, ','), \
                array_to_string(referenced_column_names, ','), constraint_name \
         FROM duckdb_constraints() \
         WHERE constraint_type = 'FOREIGN KEY' AND referenced_table = '{t}' \
         ORDER BY schema_name, table_name, constraint_name"
    );
    let rows = super::duckdb::execute_sql(handle, &sql)
        .await
        .map_err(|e| format!("Failed to get incoming foreign keys: {e}"))?
        .rows;
    Ok(rows.iter().filter_map(|r| Some(IncomingForeignKey {
        from_schema:  r.get(0).and_then(|v| v.as_str()).unwrap_or("main").to_string(),
        from_table:   r.get(1).and_then(|v| v.as_str())?.to_string(),
        from_columns: r.get(2).and_then(|v| v.as_str()).unwrap_or("").split(',').filter(|s| !s.is_empty()).map(String::from).collect(),
        to_columns:   r.get(3).and_then(|v| v.as_str()).unwrap_or("").split(',').filter(|s| !s.is_empty()).map(String::from).collect(),
        constraint_name: r.get(4).and_then(|v| v.as_str()).unwrap_or("").to_string(),
    })).collect())
}

async fn get_incoming_fks_mssql(handle: &super::connection::MssqlHandle, schema: &str, table: &str) -> Result<Vec<IncomingForeignKey>, String> {
    let s = schema.replace('\'', "''");
    let t = table.replace('\'', "''");
    let sql = format!(
        "SELECT sch.name, tp.name, cp.name, cr.name, fk.name \
         FROM sys.foreign_keys fk \
         JOIN sys.foreign_key_columns fkc ON fkc.constraint_object_id = fk.object_id \
         JOIN sys.tables tp   ON tp.object_id  = fk.parent_object_id \
         JOIN sys.schemas sch ON sch.schema_id = tp.schema_id \
         JOIN sys.tables tr   ON tr.object_id  = fk.referenced_object_id \
         JOIN sys.schemas rsch ON rsch.schema_id = tr.schema_id \
         JOIN sys.columns cp ON cp.object_id = fkc.parent_object_id     AND cp.column_id = fkc.parent_column_id \
         JOIN sys.columns cr ON cr.object_id = fkc.referenced_object_id AND cr.column_id = fkc.referenced_column_id \
         WHERE rsch.name = '{s}' AND tr.name = '{t}' \
         ORDER BY sch.name, tp.name, fk.name, fkc.constraint_column_id"
    );
    let rows = super::mssql::execute_sql(handle, &sql)
        .await
        .map_err(|e| format!("Failed to get incoming foreign keys: {e}"))?
        .rows;

    let mut map: std::collections::BTreeMap<(String, String, String), (Vec<String>, Vec<String>)> = Default::default();
    for r in &rows {
        let fs: String = r.get(0).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let ft: String = r.get(1).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let fc: String = r.get(2).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let tc: String = r.get(3).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let cn: String = r.get(4).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let e = map.entry((fs, ft, cn)).or_default();
        e.0.push(fc); e.1.push(tc);
    }
    Ok(map.into_iter().map(|((fs, ft, cn), (fc, tc))| IncomingForeignKey {
        from_schema: fs, from_table: ft, from_columns: fc, to_columns: tc, constraint_name: cn,
    }).collect())
}

pub async fn get_incoming_foreign_keys(
    state: State<'_, DbState>,
    schema: String,
    table: String,
) -> Result<Vec<IncomingForeignKey>, String> {
    if let ActiveConnection::Oracle(h) = require_conn(&state)? {
        return super::oracle::incoming_foreign_keys(&h, &schema, &table).await;
    }
    validate_ident(&schema)?;
    validate_ident(&table)?;
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => get_incoming_fks_pg(&pool, &schema, &table).await,
        ActiveConnection::Mysql(pool)    => get_incoming_fks_mysql(&pool, &schema, &table).await,
        ActiveConnection::Sqlite(pool)   => get_incoming_fks_sqlite(&pool, &table).await,
        ActiveConnection::D1(cfg)        => get_incoming_fks_d1(&cfg, &table).await,
        ActiveConnection::LibSql(cfg)    => get_incoming_fks_libsql(&cfg, &table).await,
        ActiveConnection::Duckdb(h)      => get_incoming_fks_duckdb(&h, &table).await,
        ActiveConnection::Mssql(h)       => get_incoming_fks_mssql(&h, &schema, &table).await,
        // ClickHouse and Redis have no foreign keys.
        _                                => Ok(vec![]),
    }
}

// ── DDL reconstruction ────────────────────────────────────────────────────────

async fn get_ddl_pg(pool: &PgPool, schema: &str, table: &str) -> Result<String, String> {
    let col_rows = sqlx::query(
        r#"SELECT
            a.attname,
            pg_catalog.format_type(a.atttypid, a.atttypmod),
            NOT a.attnotnull,
            pg_catalog.pg_get_expr(d.adbin, d.adrelid)
        FROM pg_catalog.pg_attribute a
        LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
        WHERE a.attrelid = ('"' || $1 || '"."' || $2 || '"')::regclass
          AND a.attnum > 0 AND NOT a.attisdropped
        ORDER BY a.attnum"#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("DDL columns query failed: {e}"))?;

    let pk_rows = sqlx::query(
        r#"SELECT a.attname
           FROM pg_catalog.pg_constraint c
           JOIN pg_catalog.pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = ANY(c.conkey)
           WHERE c.conrelid = ('"' || $1 || '"."' || $2 || '"')::regclass
             AND c.contype = 'p'
           ORDER BY array_position(c.conkey, a.attnum)"#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let pk_cols: Vec<String> = pk_rows
        .iter()
        .filter_map(|r| r.try_get::<Option<String>, _>(0).ok().flatten())
        .collect();

    let mut lines: Vec<String> = col_rows
        .iter()
        .filter_map(|r| {
            let name: String = r.try_get::<Option<String>, _>(0).ok().flatten()?;
            let typ: String = r
                .try_get::<Option<String>, _>(1)
                .ok()
                .flatten()
                .unwrap_or_else(|| "text".to_string());
            let nullable: bool = r
                .try_get::<Option<bool>, _>(2)
                .ok()
                .flatten()
                .unwrap_or(true);
            let default: Option<String> = r.try_get::<Option<String>, _>(3).ok().flatten();
            let mut col = format!("  \"{}\" {}", name, typ);
            if let Some(d) = default {
                col.push_str(&format!(" DEFAULT {}", d));
            }
            if !nullable {
                col.push_str(" NOT NULL");
            }
            Some(col)
        })
        .collect();

    if !pk_cols.is_empty() {
        let pk_list = pk_cols
            .iter()
            .map(|c| format!("\"{}\"", c))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("  PRIMARY KEY ({})", pk_list));
    }

    Ok(format!(
        "CREATE TABLE \"{}\".\"{}\" (\n{}\n);",
        schema,
        table,
        lines.join(",\n")
    ))
}

async fn get_ddl_sqlite(pool: &sqlx::SqlitePool, table: &str) -> Result<String, String> {
    let row = sqlx::query("SELECT sql FROM sqlite_master WHERE type='table' AND name=?")
        .bind(table)
        .fetch_one(pool)
        .await
        .map_err(|e| format!("DDL query failed: {e}"))?;
    Ok(row
        .try_get::<Option<String>, _>(0)
        .ok()
        .flatten()
        .unwrap_or_default())
}

async fn get_ddl_mysql(pool: &MySqlPool, schema: &str, table: &str) -> Result<String, String> {
    let sql = format!(
        "SHOW CREATE TABLE `{}`.`{}`",
        schema.replace('`', ""),
        table.replace('`', "")
    );
    let row = sqlx::query(&sql)
        .fetch_one(pool)
        .await
        .map_err(|e| format!("SHOW CREATE TABLE failed: {e}"))?;
    // SHOW CREATE TABLE's second column is binary-flagged too, so this
    // returned the empty string and the DDL view showed nothing at all.
    Ok(my_text(&row, 1).unwrap_or_default())
}

async fn get_ddl_d1(cfg: &super::connection::D1Config, table: &str) -> Result<String, String> {
    let result = super::d1::query(
        cfg,
        "SELECT sql FROM sqlite_master WHERE type='table' AND name=?",
        vec![serde_json::Value::String(table.to_string())],
    )
    .await
    .map_err(|e| format!("D1 DDL query failed: {e}"))?;

    let sql_idx = result.columns.iter().position(|c| c.name == "sql").unwrap_or(0);
    Ok(result
        .rows
        .first()
        .and_then(|r| r.get(sql_idx))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string())
}

async fn get_ddl_libsql(
    cfg: &super::connection::LibSqlConfig,
    table: &str,
) -> Result<String, String> {
    let result = super::libsql::query(
        cfg,
        "SELECT sql FROM sqlite_master WHERE type='table' AND name=?",
        vec![serde_json::Value::String(table.to_string())],
    )
    .await
    .map_err(|e| format!("LibSQL DDL query failed: {e}"))?;

    let sql_idx = result.columns.iter().position(|c| c.name == "sql").unwrap_or(0);
    Ok(result
        .rows
        .first()
        .and_then(|r| r.get(sql_idx))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string())
}

pub async fn get_table_ddl(
    state: State<'_, DbState>,
    schema: String,
    table: String,
) -> Result<String, String> {
    if let ActiveConnection::Oracle(h) = require_conn(&state)? {
        return super::oracle::get_ddl(&h, &schema, &table).await;
    }
    validate_ident(&schema)?;
    validate_ident(&table)?;
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => get_ddl_pg(&pool, &schema, &table).await,
        ActiveConnection::Mysql(pool) => get_ddl_mysql(&pool, &schema, &table).await,
        ActiveConnection::Sqlite(pool) => get_ddl_sqlite(&pool, &table).await,
        ActiveConnection::D1(cfg) => get_ddl_d1(&cfg, &table).await,
        ActiveConnection::LibSql(cfg) => get_ddl_libsql(&cfg, &table).await,
        ActiveConnection::Clickhouse(cfg) => super::clickhouse::get_ddl(&cfg, &schema, &table).await,
        ActiveConnection::Posthog(cfg) => super::posthog::get_ddl(&cfg, &table).await,
        ActiveConnection::Redis(cfg) => super::redis::get_ddl(&cfg, &table).await,
        ActiveConnection::Dynamodb(_) | ActiveConnection::Doc(_) => Ok(String::new()),
        ActiveConnection::Duckdb(h) => super::duckdb::get_ddl(&h, &table).await,
        ActiveConnection::Mssql(h) => super::mssql::get_ddl(&h, &schema, &table).await,
        ActiveConnection::Oracle(_) => unreachable!("answered above"),
    }
}

// ── Cross-connection introspection (for Data Diff) ────────────────────────────

/// List schemas for an arbitrary saved connection without changing global state.
pub async fn list_schemas_on_conn(config: AnyConnectionConfig) -> Result<Vec<String>, String> {
    use super::connection::{open_mysql, open_pg};
    match config {
        AnyConnectionConfig::Postgres(c) => {
            let pool = open_pg(&c).await?;
            let result = list_schemas_pg(&pool).await;
            pool.close().await;
            result
        }
        AnyConnectionConfig::Mysql(c) => {
            let pool = open_mysql(&c).await?;
            let result = list_schemas_mysql(&pool).await;
            pool.close().await;
            result
        }
        AnyConnectionConfig::Clickhouse(c) => super::clickhouse::list_schemas(&c).await,
        AnyConnectionConfig::Oracle(c) => {
            let h = super::connection::open_oracle(&c).await?;
            super::oracle::list_schemas(&h).await
        }
        _ => Ok(vec!["main".to_string()]),
    }
}

/// List table names for an arbitrary saved connection without changing global state.
pub async fn list_tables_on_conn(
    config: AnyConnectionConfig,
    schema: String,
) -> Result<Vec<String>, String> {
    use super::connection::{open_mysql, open_pg, open_sqlite};
    let to_names = |ts: Vec<TableInfo>| ts.into_iter().map(|t| t.name).collect::<Vec<_>>();
    match config {
        AnyConnectionConfig::Postgres(c) => {
            validate_ident(&schema)?;
            let pool = open_pg(&c).await?;
            let result = list_tables_pg(&pool, &schema).await.map(to_names);
            pool.close().await;
            result
        }
        AnyConnectionConfig::Mysql(c) => {
            let pool = open_mysql(&c).await?;
            let result = list_tables_mysql(&pool, &schema).await.map(to_names);
            pool.close().await;
            result
        }
        AnyConnectionConfig::Sqlite(c) => {
            let pool = open_sqlite(&c).await?;
            let result = list_tables_sqlite(&pool).await.map(to_names);
            pool.close().await;
            result
        }
        AnyConnectionConfig::D1(c) => list_tables_d1(&c).await.map(to_names),
        AnyConnectionConfig::Libsql(c) => list_tables_libsql(&c).await.map(to_names),
        AnyConnectionConfig::Clickhouse(c) => super::clickhouse::list_tables(&c, &schema).await.map(to_names),
        AnyConnectionConfig::Posthog(c) => super::posthog::list_tables(&c).await.map(to_names),
        AnyConnectionConfig::Redis(c) => super::redis::list_tables(&c).await.map(to_names),
        AnyConnectionConfig::Dynamodb(c) => super::dynamodb::list_tables(&c).await.map(to_names),
        AnyConnectionConfig::Couchdb(c) => super::couchdb::list_databases(&c).await,
        AnyConnectionConfig::Mongodb(_) => Ok(Vec::new()),
        AnyConnectionConfig::Duckdb(c) => {
            let h = super::connection::open_duckdb(&c).await?;
            super::duckdb::list_tables(&h).await.map(to_names)
        }
        AnyConnectionConfig::Mssql(c) => {
            let h = super::connection::open_mssql(&c).await?;
            super::mssql::list_tables(&h, &schema).await.map(to_names)
        }
        AnyConnectionConfig::Oracle(c) => {
            let h = super::connection::open_oracle(&c).await?;
            super::oracle::list_tables(&h, &schema).await.map(to_names)
        }
    }
}

/// Get CREATE TABLE DDL for a table on an arbitrary saved connection.
pub async fn get_table_ddl_on_conn(
    config: AnyConnectionConfig,
    schema: String,
    table: String,
) -> Result<String, String> {
    use super::connection::{open_mysql, open_pg, open_sqlite};
    if let AnyConnectionConfig::Oracle(c) = &config {
        let h = super::connection::open_oracle(c).await?;
        return super::oracle::get_ddl(&h, &schema, &table).await;
    }
    validate_ident(&schema)?;
    validate_ident(&table)?;
    match config {
        AnyConnectionConfig::Postgres(c) => {
            let pool = open_pg(&c).await?;
            let result = get_ddl_pg(&pool, &schema, &table).await;
            pool.close().await;
            result
        }
        AnyConnectionConfig::Mysql(c) => {
            let pool = open_mysql(&c).await?;
            let result = get_ddl_mysql(&pool, &schema, &table).await;
            pool.close().await;
            result
        }
        AnyConnectionConfig::Sqlite(c) => {
            let pool = open_sqlite(&c).await?;
            let result = get_ddl_sqlite(&pool, &table).await;
            pool.close().await;
            result
        }
        AnyConnectionConfig::D1(c) => get_ddl_d1(&c, &table).await,
        AnyConnectionConfig::Libsql(c) => get_ddl_libsql(&c, &table).await,
        AnyConnectionConfig::Clickhouse(c) => super::clickhouse::get_ddl(&c, &schema, &table).await,
        AnyConnectionConfig::Posthog(c) => super::posthog::get_ddl(&c, &table).await,
        AnyConnectionConfig::Redis(c) => super::redis::get_ddl(&c, &table).await,
        AnyConnectionConfig::Dynamodb(_) | AnyConnectionConfig::Couchdb(_) | AnyConnectionConfig::Mongodb(_) => Ok(String::new()),
        AnyConnectionConfig::Duckdb(c) => {
            let h = super::connection::open_duckdb(&c).await?;
            super::duckdb::get_ddl(&h, &table).await
        }
        AnyConnectionConfig::Mssql(c) => {
            let h = super::connection::open_mssql(&c).await?;
            super::mssql::get_ddl(&h, &schema, &table).await
        }
        AnyConnectionConfig::Oracle(_) => unreachable!("answered above"),
    }
}


#[cfg(test)]
mod create_order_tests {
    /// SQLite keeps no creation time; the catalog's rowid stands in for it.
    #[tokio::test]
    async fn sqlite_tables_carry_their_creation_order() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        for sql in ["CREATE TABLE zeta (id INTEGER)", "CREATE TABLE alpha (id INTEGER)", "CREATE VIEW mid AS SELECT 1 AS x"] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        let tables = super::list_tables_sqlite(&pool).await.unwrap();
        let order = |n: &str| tables.iter().find(|t| t.name == n).and_then(|t| t.create_order).unwrap();
        assert!(order("zeta") < order("alpha") && order("alpha") < order("mid"));
        assert_eq!(tables.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["alpha", "mid", "zeta"]);
        assert!(tables.iter().all(|t| t.created_at.is_none()));
    }
}

#[cfg(test)]
mod mysql_metadata_tests {
    /// information_schema hands back `TABLE_NAME` as VARBINARY, which is why
    /// every metadata decode here goes through `my_text`. Needs the local
    /// fixture, so it is `#[ignore]`d:
    /// `docker compose -f docker/test-stack.yml up -d mysql`
    /// `cargo test --lib mysql_metadata -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn lists_tables_from_a_binary_flagged_catalog() {
        let pool = sqlx::MySqlPool::connect("mysql://root:stroke@127.0.0.1:53306/shop")
            .await
            .expect("local mysql fixture");
        let tables = super::list_tables_mysql(&pool, "shop").await.expect("list");
        assert!(
            tables.iter().any(|t| t.name == "products"),
            "expected the fixture's `products` table, got {:?}",
            tables.iter().map(|t| &t.name).collect::<Vec<_>>()
        );
        // Base tables carry MySQL's CREATE_TIME, formatted for the sidebar's sort.
        let products = tables.iter().find(|t| t.name == "products").unwrap();
        let created = products.created_at.as_deref().expect("CREATE_TIME for a base table");
        assert_eq!(created.len(), 19, "YYYY-MM-DD HH:MM:SS, got {created:?}");
        let ddl = super::get_ddl_mysql(&pool, "shop", "products").await.expect("ddl");
        assert!(ddl.contains("CREATE TABLE"), "SHOW CREATE TABLE returned {ddl:?}");
    }
}

/// The tree sidebar's per-schema catalog reads, timed against local throwaway
/// databases. Ignored; point them somewhere with:
///   TREE_PG=postgres://postgres@127.0.0.1:55432/tree
///   TREE_MY=mysql://root@127.0.0.1:53306/tree
///   TREE_SQLITE=/path/to/file.sqlite
///   cargo test --lib tree_catalog -- --ignored --nocapture --test-threads=1
#[cfg(test)]
mod tree_catalog_live {
    use std::time::Instant;

    fn env(k: &str) -> Option<String> {
        std::env::var(k).ok().filter(|v| !v.is_empty())
    }

    #[tokio::test]
    #[ignore]
    async fn tree_catalog_pg() {
        let Some(url) = env("TREE_PG") else { return };
        let pool = sqlx::PgPool::connect(&url).await.expect("pg");
        for schema in ["public", "big"] {
            let t = Instant::now();
            let cols = super::schema_column_structure_pg(&pool, schema).await.expect("cols");
            let c_ms = t.elapsed().as_millis();
            let t = Instant::now();
            let idx = super::list_indexes_pg(&pool, schema).await.expect("idx");
            let i_ms = t.elapsed().as_millis();
            eprintln!("[tree pg {schema}] columns {} tables in {c_ms}ms, {} indexes in {i_ms}ms", cols.len(), idx.len());
            if schema != "public" {
                let t = Instant::now();
                let one = super::get_column_structure_pg(&pool, schema, "t4321").await.expect("one");
                eprintln!("[tree pg {schema}] one table: {} columns in {}ms", one.len(), t.elapsed().as_millis());
                continue;
            }
            // Declared types, and both columns of a composite foreign key.
            let of = |t: &str| cols.iter().find(|x| x.table == t).map(|x| x.columns.clone()).unwrap_or_default();
            let users = of("users");
            let ty = |n: &str| users.iter().find(|c| c.name == n).map(|c| c.data_type.clone()).unwrap_or_default();
            assert_eq!(
                ["email", "balance", "tags", "meta", "doc", "uid", "m", "c", "bits"].map(ty),
                ["varchar(255)", "numeric(10,2)", "text[]", "jsonb", "json", "uuid", "mood", "bpchar(3)", "bit(8)"]
            );
            let members = of("members");
            let fk = |n: &str| members.iter().find(|c| c.name == n).and_then(|c| c.foreign_key.clone());
            assert_eq!(fk("org_a").as_deref(), Some("public.orgs.a"));
            assert_eq!(fk("org_b").as_deref(), Some("public.orgs.b"));
            assert_eq!(fk("user_id").as_deref(), Some("public.users.id"));
        }
    }

    #[tokio::test]
    #[ignore]
    async fn tree_catalog_mysql() {
        let Some(url) = env("TREE_MY") else { return };
        let pool = sqlx::MySqlPool::connect(&url).await.expect("mysql");
        for schema in ["tree", "big"] {
            let t = Instant::now();
            let cols = super::schema_column_structure_mysql(&pool, schema).await.expect("cols");
            let c_ms = t.elapsed().as_millis();
            let t = Instant::now();
            let idx = super::list_indexes_mysql(&pool, schema).await.expect("idx");
            let i_ms = t.elapsed().as_millis();
            eprintln!("[tree mysql {schema}] columns {} tables in {c_ms}ms, {} indexes in {i_ms}ms", cols.len(), idx.len());
            if schema != "tree" {
                let t = Instant::now();
                let one = super::get_column_structure_mysql(&pool, schema, "t4321").await.expect("one");
                eprintln!("[tree mysql {schema}] one table: {} columns in {}ms", one.len(), t.elapsed().as_millis());
                continue;
            }
            let of = |t: &str| cols.iter().find(|x| x.table == t).map(|x| x.columns.clone()).unwrap_or_default();
            let users = of("users");
            assert_eq!(
                users.iter().map(|c| c.data_type.as_str()).collect::<Vec<_>>(),
                ["int(11)", "varchar(255)", "decimal(10,2)", "longtext", "enum('a','b')", "int(10) unsigned"]
            );
            let members = of("members");
            assert_eq!(members[3].foreign_key.as_deref(), Some("tree.orgs.b"));
            assert_eq!(members[3].fk_constraint_name.as_deref(), Some("m_org_fk"));
            assert!(idx.iter().any(|i| i.table_name == "orgs" && i.is_primary && i.columns == "a, b"));
        }
    }

    #[tokio::test]
    #[ignore]
    async fn tree_catalog_sqlite() {
        let Some(path) = env("TREE_SQLITE") else { return };
        let pool = sqlx::SqlitePool::connect(&format!("sqlite://{path}?mode=ro")).await.expect("sqlite");
        let tables = super::list_tables_sqlite(&pool).await.expect("tables");
        // What `get_schema_column_structure` did for SQLite: one table at a time.
        let t = Instant::now();
        let mut n = 0;
        for tb in &tables {
            n += super::get_column_structure_sqlite_pragma(&pool, &tb.name).await.expect("cols").len();
        }
        eprintln!("[tree sqlite] per-table loop: {} tables, {n} columns in {}ms", tables.len(), t.elapsed().as_millis());
        // What it does now: one statement.
        let t = Instant::now();
        let raw = sqlx::query(&super::SQLITE_COLUMNS_SQL.replace("{one}", "")).fetch_all(&pool).await.expect("raw");
        eprintln!("[tree sqlite] statement alone: {} rows in {}ms", raw.len(), t.elapsed().as_millis());
        let t = Instant::now();
        let all = super::sqlite_family_columns(super::SqliteFamily::Local(&pool), None).await.expect("bulk");
        let n: usize = all.iter().map(|t| t.columns.len()).sum();
        eprintln!("[tree sqlite] one statement: {} tables, {n} columns in {}ms", all.len(), t.elapsed().as_millis());
        let t = Instant::now();
        let one = super::get_column_structure_sqlite(&pool, "members").await.expect("one");
        eprintln!("[tree sqlite] one table: {} columns in {}us", one.len(), t.elapsed().as_micros());
        let t = Instant::now();
        let idx = super::list_indexes_sqlite(&pool).await.expect("idx");
        eprintln!("[tree sqlite] {} indexes in {}ms", idx.len(), t.elapsed().as_millis());
    }
}

/// The SQLite family's one-statement catalog reads, against an in-memory
/// database (D1 and libSQL run the same statements and the same decoder).
#[cfg(test)]
mod sqlite_tree_catalog {
    use super::*;

    async fn db() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        for sql in [
            "CREATE TABLE users (id INTEGER PRIMARY KEY, email VARCHAR(255) UNIQUE NOT NULL, balance NUMERIC(10,2) DEFAULT 0, meta JSON)",
            "CREATE TABLE orgs (a INT, b INT, name TEXT, PRIMARY KEY (a, b)) WITHOUT ROWID",
            "CREATE TABLE members (id INTEGER PRIMARY KEY, user_id INT REFERENCES users(id), org_a INT, org_b INT, \
               FOREIGN KEY (org_a, org_b) REFERENCES orgs(a, b))",
            "CREATE INDEX members_user_idx ON members(user_id)",
            "CREATE INDEX users_lower ON users(lower(email)) WHERE email IS NOT NULL",
            "CREATE TABLE \"odd name\" (code TEXT PRIMARY KEY)",
            "CREATE VIEW v_users AS SELECT id, email FROM users",
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        pool
    }

    #[tokio::test]
    async fn columns_keys_and_types_in_one_statement() {
        let pool = db().await;
        let all = sqlite_family_columns(SqliteFamily::Local(&pool), None).await.unwrap();
        assert_eq!(all.iter().map(|t| t.table.as_str()).collect::<Vec<_>>(), ["members", "odd name", "orgs", "users", "v_users"]);
        let users = &all[3].columns;
        assert_eq!(users.iter().map(|c| c.data_type.as_str()).collect::<Vec<_>>(), ["integer", "varchar(255)", "numeric(10,2)", "json"]);
        assert!(users[0].is_primary_key && !users[1].is_nullable && users[2].is_nullable);
        assert_eq!(users[2].column_default.as_deref(), Some("0"));
        // A composite foreign key: both columns, one name, each its own target.
        let m = &all[0].columns;
        assert_eq!(m[1].foreign_key.as_deref(), Some("main.users.id"));
        assert_eq!(m[1].fk_constraint_name.as_deref(), Some("members_user_id_fkey"));
        assert_eq!(m[2].foreign_key.as_deref(), Some("main.orgs.a"));
        assert_eq!(m[3].foreign_key.as_deref(), Some("main.orgs.b"));
        assert_eq!(m[2].fk_constraint_name.as_deref(), Some("members_org_a_org_b_fkey"));
        assert_eq!(m[3].fk_constraint_name, m[2].fk_constraint_name);
        assert_eq!(all[2].columns.iter().filter(|c| c.is_primary_key).count(), 2);
        // One table, and a name the identifier check would have refused.
        let one = get_column_structure_sqlite(&pool, "members").await.unwrap();
        assert_eq!(one.len(), 4);
        let odd = sqlite_family_columns(SqliteFamily::Local(&pool), Some("odd name")).await.unwrap();
        assert!(odd[0].columns[0].is_primary_key);
        // The pragma path agrees on names.
        let legacy = get_column_structure_sqlite_pragma(&pool, "members").await.unwrap();
        assert_eq!(legacy.iter().map(|c| &c.name).collect::<Vec<_>>(), one.iter().map(|c| &c.name).collect::<Vec<_>>());
    }

    #[tokio::test]
    async fn indexes_with_columns_origin_and_condition() {
        let pool = db().await;
        let idx = list_indexes_sqlite(&pool).await.unwrap();
        let find = |n: &str| idx.iter().find(|i| i.name == n).unwrap_or_else(|| panic!("{n} in {idx:?}"));
        let pk = find("sqlite_autoindex_orgs_1");
        assert!(pk.is_primary && pk.is_unique && pk.columns == "a, b" && pk.table_name == "orgs");
        let uq = find("sqlite_autoindex_users_1");
        assert!(!uq.is_primary && uq.is_unique && uq.columns == "email");
        let ix = find("members_user_idx");
        assert!(!ix.is_unique && ix.columns == "user_id");
        let partial = find("users_lower");
        assert_eq!(partial.columns, "(expression)");
        assert_eq!(partial.condition.as_deref(), Some("email IS NOT NULL"));
        // A rowid table's INTEGER PRIMARY KEY has no index of its own.
        assert!(!idx.iter().any(|i| i.table_name == "users" && i.is_primary));
    }

    #[test]
    fn remote_rows_decode_the_same() {
        // D1 / libSQL hand rows back as JSON, numbers sometimes as floats.
        let rows = vec![
            serde_json::json!(["c", "t", 0, "id", "INTEGER", 1, null, 1]),
            serde_json::json!(["c", "t", 1.0, "p", "INT", 0, "3", 0]),
            serde_json::json!(["f", "t", 0, "p", "parent", "id", 0, null]),
        ]
        .into_iter()
        .map(|v| v.as_array().unwrap().clone())
        .collect::<Vec<_>>();
        let t = sqlite_columns_from_rows(&rows);
        assert_eq!(t[0].columns[1].foreign_key.as_deref(), Some("main.parent.id"));
        assert_eq!(t[0].columns[1].fk_constraint_name.as_deref(), Some("t_p_fkey"));
        assert_eq!(t[0].columns[1].ordinal_position, 2);
        assert!(t[0].columns[0].is_primary_key && !t[0].columns[0].is_nullable);
    }
}
