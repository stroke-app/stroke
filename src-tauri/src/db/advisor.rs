//! Advisor: a read-only lint pass over the connected database.
//!
//! Every check is a catalog query. Nothing here reads a single row of user data,
//! nothing writes, and nothing depends on an extension being installed - so the
//! whole scan is safe to run against production. Findings carry the SQL that would
//! fix them, but applying it is always the user's action, never ours.
//!
//! Checks are values, not code paths: one entry in `run_all` per check, each
//! returning findings. A check that fails - a locked-down role that can't read a
//! catalog, a server too old for a column - reports its own status and the rest of
//! the report still lands. That matters more than it sounds: the databases most
//! worth advising are the ones with the tightest grants.

use serde::Serialize;
use sqlx::{PgPool, Row};

/// One thing worth telling the user about one entity.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AdvisorFinding {
    pub check_id: String,
    /// security | performance | schema
    pub category: String,
    /// error | warning | info - decided PER FINDING, so an unused 8GB index and an
    /// unused 2MB one don't shout equally loudly.
    pub severity: String,
    pub title: String,
    /// Qualified name of what the finding is about: `public.articles`, or an index.
    pub entity: String,
    /// table | index | column | constraint | function
    pub entity_kind: String,
    pub description: String,
    /// Numbers behind the verdict, already formatted for display.
    pub detail: Option<String>,
    /// SQL that would resolve it. Copyable; never executed by the app.
    pub remediation: Option<String>,
    /// Sort key within a check (bytes, ratio, row count). Bigger = worse.
    pub metric: f64,
}

/// Per-check outcome, so the UI can say "9 checks ran, 1 skipped" honestly.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AdvisorCheckStatus {
    pub id: String,
    pub title: String,
    pub category: String,
    /// ok | error
    pub status: String,
    pub findings: usize,
    pub ms: u64,
    pub error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvisorReport {
    pub engine: String,
    pub findings: Vec<AdvisorFinding>,
    pub checks: Vec<AdvisorCheckStatus>,
    /// True when the engine has no checks implemented yet, so the UI can say so
    /// rather than showing a clean bill of health it hasn't earned.
    pub unsupported: bool,
}

/// Schemas no advice applies to.
const SYSTEM_SCHEMAS: &str = "'pg_catalog','information_schema','pg_toast'";

fn bytes_pretty(bytes: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{} {}", v as i64, UNITS[u])
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

// ── Checks ────────────────────────────────────────────────────────────────────

/// Tables with row-level security switched off.
///
/// A warning rather than an error on purpose. RLS is the only thing standing
/// between a table and an untrusted client that can reach Postgres directly
/// (PostgREST, Supabase, an edge function with the anon key). Behind a trusted
/// application server it is simply not how access is enforced, and calling that an
/// error would mean every ordinary deployment opens this tab to a wall of red -
/// which teaches people to ignore the tab.
async fn check_rls_disabled(pool: &PgPool) -> Result<Vec<AdvisorFinding>, String> {
    // Is there a role that untrusted clients use (Supabase's anon/authenticated,
    // PostgREST's web_anon)? Without one, Postgres is reached through an app
    // server and RLS is simply not the access model: say so quietly (info)
    // instead of opening every ordinary database on a wall of warnings.
    let sql = format!(
        "WITH api AS (SELECT rolname FROM pg_roles WHERE rolname IN ('anon','authenticated','web_anon','authenticator'))
         SELECT n.nspname, c.relname,
                EXISTS (SELECT 1 FROM api) AS exposed,
                EXISTS (SELECT 1 FROM api WHERE api.rolname IN ('anon','web_anon')
                          AND has_table_privilege(api.rolname, c.oid, 'SELECT')) AS anon_reads
           FROM pg_class c
           JOIN pg_namespace n ON n.oid = c.relnamespace
          WHERE c.relkind = 'r'
            AND NOT c.relrowsecurity
            AND NOT EXISTS (SELECT 1 FROM pg_policy p WHERE p.polrelid = c.oid)
            AND n.nspname NOT IN ({SYSTEM_SCHEMAS})
            AND n.nspname NOT LIKE 'pg\\_temp%'
          ORDER BY 1, 2"
    );
    let rows = sqlx::query(&sql).fetch_all(pool).await.map_err(|e| e.to_string())?;
    Ok(rows
        .iter()
        .map(|r| {
            let schema: String = r.get(0);
            let table: String = r.get(1);
            let exposed: bool = r.get(2);
            let anon_reads: bool = r.get(3);
            let (severity, description, detail) = if anon_reads {
                ("error", "The anonymous API role can SELECT this table and nothing filters the rows: anyone with the public key reads all of it.", Some("anon can SELECT · no RLS".to_string()))
            } else if exposed {
                ("warning", "This database has API roles that clients use directly (PostgREST, Supabase). Without RLS, any of them granted access to the table reads every row.", None)
            } else {
                ("info", "No API roles here, so access goes through an application server and RLS is not how it is enforced. Turn it on before exposing the database to clients directly.", None)
            };
            AdvisorFinding {
                check_id: "rls_disabled".into(),
                category: "security".into(),
                severity: severity.into(),
                title: "Table without row-level security".into(),
                entity: format!("{schema}.{table}"),
                entity_kind: "table".into(),
                description: description.into(),
                detail,
                remediation: Some(format!(
                    "ALTER TABLE \"{schema}\".\"{table}\" ENABLE ROW LEVEL SECURITY;\n-- then add at least one policy, or the table denies everything:\n-- CREATE POLICY \"{table}_select\" ON \"{schema}\".\"{table}\" FOR SELECT USING (true);"
                )),
                metric: if anon_reads { 2.0 } else if exposed { 1.0 } else { 0.0 },
            }
        })
        .collect())
}

/// Policies written for a table whose RLS is off: they look like protection and
/// enforce nothing.
async fn check_policies_ignored(pool: &PgPool) -> Result<Vec<AdvisorFinding>, String> {
    let sql = format!(
        "SELECT n.nspname, c.relname, count(p.*)::int8
           FROM pg_class c
           JOIN pg_namespace n ON n.oid = c.relnamespace
           JOIN pg_policy p ON p.polrelid = c.oid
          WHERE c.relkind IN ('r','p') AND NOT c.relrowsecurity
            AND n.nspname NOT IN ({SYSTEM_SCHEMAS})
          GROUP BY 1, 2 ORDER BY 1, 2"
    );
    let rows = sqlx::query(&sql).fetch_all(pool).await.map_err(|e| e.to_string())?;
    Ok(rows.iter().map(|r| {
        let schema: String = r.get(0);
        let table: String = r.get(1);
        let n: i64 = r.get(2);
        AdvisorFinding {
            check_id: "policies_ignored".into(),
            category: "security".into(),
            severity: "error".into(),
            title: "Policies that are not enforced".into(),
            entity: format!("{schema}.{table}"),
            entity_kind: "table".into(),
            description: "The table has row-level policies, but RLS is off on it, so Postgres ignores every one of them and each role sees all rows.".into(),
            detail: Some(format!("{n} polic{} defined · RLS off", if n == 1 { "y" } else { "ies" })),
            remediation: Some(format!("ALTER TABLE \"{schema}\".\"{table}\" ENABLE ROW LEVEL SECURITY;")),
            metric: n as f64,
        }
    }).collect())
}

/// RLS switched on with no policy: only the owner (and superusers) see rows.
async fn check_rls_no_policy(pool: &PgPool) -> Result<Vec<AdvisorFinding>, String> {
    let sql = format!(
        "SELECT n.nspname, c.relname
           FROM pg_class c
           JOIN pg_namespace n ON n.oid = c.relnamespace
          WHERE c.relkind IN ('r','p') AND c.relrowsecurity
            AND NOT EXISTS (SELECT 1 FROM pg_policy p WHERE p.polrelid = c.oid)
            AND n.nspname NOT IN ({SYSTEM_SCHEMAS})
          ORDER BY 1, 2"
    );
    let rows = sqlx::query(&sql).fetch_all(pool).await.map_err(|e| e.to_string())?;
    Ok(rows.iter().map(|r| {
        let schema: String = r.get(0);
        let table: String = r.get(1);
        AdvisorFinding {
            check_id: "rls_no_policy".into(),
            category: "security".into(),
            severity: "warning".into(),
            title: "RLS on, but no policy".into(),
            entity: format!("{schema}.{table}"),
            entity_kind: "table".into(),
            description: "Row-level security is enabled and no policy grants anything, so every role except the owner and superusers sees an empty table. Usually a policy that was meant to follow.".into(),
            detail: None,
            remediation: Some(format!("CREATE POLICY \"{table}_select\" ON \"{schema}\".\"{table}\" FOR SELECT USING (true);")),
            metric: 0.0,
        }
    }).collect())
}

/// SECURITY DEFINER functions run as their owner; without a pinned search_path
/// a caller can put their own objects first on it and run code as that owner.
async fn check_definer_search_path(pool: &PgPool) -> Result<Vec<AdvisorFinding>, String> {
    let sql = format!(
        "SELECT n.nspname, p.proname, pg_get_function_identity_arguments(p.oid)
           FROM pg_proc p
           JOIN pg_namespace n ON n.oid = p.pronamespace
          WHERE p.prosecdef
            AND n.nspname NOT IN ({SYSTEM_SCHEMAS})
            AND NOT EXISTS (SELECT 1 FROM unnest(coalesce(p.proconfig, '{{}}'::text[])) c WHERE c LIKE 'search\\_path=%')
          ORDER BY 1, 2"
    );
    let rows = sqlx::query(&sql).fetch_all(pool).await.map_err(|e| e.to_string())?;
    Ok(rows.iter().map(|r| {
        let schema: String = r.get(0);
        let name: String = r.get(1);
        let args: String = r.get(2);
        AdvisorFinding {
            check_id: "definer_search_path".into(),
            category: "security".into(),
            severity: "warning".into(),
            title: "SECURITY DEFINER without a search_path".into(),
            entity: format!("{schema}.{name}({args})"),
            entity_kind: "function".into(),
            description: "The function runs with its owner's rights and resolves names through the caller's search_path, so a caller who can create objects can shadow a table or function it uses and run code as the owner.".into(),
            detail: None,
            remediation: Some(format!("ALTER FUNCTION \"{schema}\".\"{name}\"({args}) SET search_path = pg_catalog, \"{schema}\";")),
            metric: 0.0,
        }
    }).collect())
}

/// Two indexes with the same definition: one is pure cost.
async fn check_duplicate_index(pool: &PgPool) -> Result<Vec<AdvisorFinding>, String> {
    let sql = format!(
        "SELECT n.nspname, t.relname, i2.relname, i1.relname, pg_relation_size(x2.indexrelid)::float8,
                x2.indisprimary OR x2.indisunique OR EXISTS (SELECT 1 FROM pg_constraint c WHERE c.conindid = x2.indexrelid)
           FROM pg_index x1
           JOIN pg_index x2 ON x1.indrelid = x2.indrelid AND x1.indexrelid < x2.indexrelid
            AND x1.indkey = x2.indkey AND x1.indclass = x2.indclass
            AND coalesce(pg_get_expr(x1.indexprs, x1.indrelid), '') = coalesce(pg_get_expr(x2.indexprs, x2.indrelid), '')
            AND coalesce(pg_get_expr(x1.indpred, x1.indrelid), '') = coalesce(pg_get_expr(x2.indpred, x2.indrelid), '')
           JOIN pg_class i1 ON i1.oid = x1.indexrelid
           JOIN pg_class i2 ON i2.oid = x2.indexrelid
           JOIN pg_class t ON t.oid = x1.indrelid
           JOIN pg_namespace n ON n.oid = t.relnamespace
          WHERE n.nspname NOT IN ({SYSTEM_SCHEMAS})
          ORDER BY 5 DESC"
    );
    let rows = sqlx::query(&sql).fetch_all(pool).await.map_err(|e| e.to_string())?;
    Ok(rows.iter().map(|r| {
        let schema: String = r.get(0);
        let table: String = r.get(1);
        let dup: String = r.get(2);
        let keep: String = r.get(3);
        let bytes: f64 = r.get(4);
        let backs_constraint: bool = r.get(5);
        AdvisorFinding {
            check_id: "duplicate_index".into(),
            category: "performance".into(),
            severity: if bytes > 64.0 * 1024.0 * 1024.0 { "warning" } else { "info" }.into(),
            title: "Duplicate index".into(),
            entity: format!("{schema}.{dup}"),
            entity_kind: "index".into(),
            description: "Another index on the same table has the same columns, operator classes, expressions and predicate. Every write maintains both and the planner only needs one.".into(),
            detail: Some(format!("{} on {schema}.{table} · same as {keep}", bytes_pretty(bytes))),
            remediation: Some(if backs_constraint {
                format!("-- \"{dup}\" backs a constraint; drop \"{keep}\" instead:\nDROP INDEX CONCURRENTLY \"{schema}\".\"{keep}\";")
            } else {
                format!("DROP INDEX CONCURRENTLY \"{schema}\".\"{dup}\";")
            }),
            metric: bytes,
        }
    }).collect())
}

/// Sequences close to the end of their type: inserts start failing at the max.
async fn check_sequence_exhaustion(pool: &PgPool) -> Result<Vec<AdvisorFinding>, String> {
    let sql = format!(
        "SELECT schemaname, sequencename, data_type::text, last_value::float8, max_value::float8
           FROM pg_sequences
          WHERE last_value IS NOT NULL AND max_value > 0
            AND schemaname NOT IN ({SYSTEM_SCHEMAS})
            AND last_value::float8 / max_value::float8 > 0.5
          ORDER BY last_value::float8 / max_value::float8 DESC"
    );
    let rows = sqlx::query(&sql).fetch_all(pool).await.map_err(|e| e.to_string())?;
    Ok(rows.iter().map(|r| {
        let schema: String = r.get(0);
        let seq: String = r.get(1);
        let ty: String = r.get(2);
        let last: f64 = r.get(3);
        let max: f64 = r.get(4);
        let ratio = last / max;
        AdvisorFinding {
            check_id: "sequence_exhaustion".into(),
            category: "schema".into(),
            severity: if ratio > 0.85 { "error" } else { "warning" }.into(),
            title: "Sequence running out".into(),
            entity: format!("{schema}.{seq}"),
            entity_kind: "sequence".into(),
            description: "The sequence has used most of its range. When it reaches the maximum, every insert that needs a new id fails.".into(),
            detail: Some(format!("{:.0}% of {ty} used · {last:.0} of {max:.0}", ratio * 100.0)),
            remediation: Some(format!("ALTER SEQUENCE \"{schema}\".\"{seq}\" AS bigint;\n-- and widen the column it feeds to bigint (ALTER TABLE ... ALTER COLUMN ... TYPE bigint)")),
            metric: ratio,
        }
    }).collect())
}

/// Tables the planner is guessing about: never analyzed, or changed a lot since.
async fn check_stale_stats(pool: &PgPool) -> Result<Vec<AdvisorFinding>, String> {
    let sql = "SELECT s.schemaname, s.relname, s.n_live_tup::int8, s.n_mod_since_analyze::int8,
                      (s.last_analyze IS NULL AND s.last_autoanalyze IS NULL)
                 FROM pg_stat_user_tables s
                WHERE s.n_live_tup > 1000
                  AND ((s.last_analyze IS NULL AND s.last_autoanalyze IS NULL)
                       OR s.n_mod_since_analyze > GREATEST(10000, s.n_live_tup / 5))
                ORDER BY s.n_mod_since_analyze DESC";
    let rows = sqlx::query(sql).fetch_all(pool).await.map_err(|e| e.to_string())?;
    Ok(rows.iter().map(|r| {
        let schema: String = r.get(0);
        let table: String = r.get(1);
        let live: i64 = r.get(2);
        let changed: i64 = r.get(3);
        let never: bool = r.get(4);
        AdvisorFinding {
            check_id: "stale_stats".into(),
            category: "performance".into(),
            severity: if never { "warning" } else { "info" }.into(),
            title: "Planner statistics out of date".into(),
            entity: format!("{schema}.{table}"),
            entity_kind: "table".into(),
            description: "The planner chooses joins and indexes from these statistics. A table never analyzed, or changed a lot since, gets plans built on guesses.".into(),
            detail: Some(if never { format!("never analyzed · ~{live} rows") } else { format!("{changed} rows changed since the last ANALYZE · ~{live} rows") }),
            remediation: Some(format!("ANALYZE \"{schema}\".\"{table}\";")),
            metric: changed as f64,
        }
    }).collect())
}

async fn check_unindexed_fk(pool: &PgPool) -> Result<Vec<AdvisorFinding>, String> {
    let sql = format!(
        "SELECT n.nspname, c.relname, con.conname, pg_get_constraintdef(con.oid),
                (SELECT string_agg(quote_ident(a.attname), ', ' ORDER BY x.ord)
                   FROM unnest(con.conkey) WITH ORDINALITY AS x(attnum, ord)
                   JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = x.attnum) AS cols,
                c.reltuples::bigint
           FROM pg_constraint con
           JOIN pg_class c ON c.oid = con.conrelid
           JOIN pg_namespace n ON n.oid = c.relnamespace
          WHERE con.contype = 'f'
            AND n.nspname NOT IN ({SYSTEM_SCHEMAS})
            AND NOT EXISTS (
                  SELECT 1 FROM pg_index i
                   WHERE i.indrelid = con.conrelid
                     AND (i.indkey::int2[])[1:cardinality(con.conkey::int2[])] @> con.conkey::int2[]
                )
          ORDER BY c.reltuples DESC NULLS LAST, 1, 2"
    );
    let rows = sqlx::query(&sql).fetch_all(pool).await.map_err(|e| e.to_string())?;
    Ok(rows
        .iter()
        .map(|r| {
            let schema: String = r.get(0);
            let table: String = r.get(1);
            let name: String = r.get(2);
            let def: String = r.get(3);
            let cols: String = r.get(4);
            let tuples: i64 = r.get(5);
            // The bigger the child table, the more each unindexed join costs.
            let severity = if tuples > 100_000 { "error" } else { "warning" };
            AdvisorFinding {
                check_id: "unindexed_fk".into(),
                category: "performance".into(),
                severity: severity.into(),
                title: "Foreign key without an index".into(),
                entity: format!("{schema}.{table} ({name})"),
                entity_kind: "constraint".into(),
                description: "Postgres indexes the referenced side of a foreign key, never the referencing side. Joins across it, and the check behind every parent DELETE or UPDATE, scan this table end to end.".into(),
                detail: Some(format!("{def} · ~{tuples} rows")),
                remediation: Some(format!(
                    "CREATE INDEX CONCURRENTLY ON \"{schema}\".\"{table}\" ({cols});"
                )),
                metric: tuples as f64,
            }
        })
        .collect())
}

/// Indexes the planner has never used since statistics were last reset.
async fn check_unused_index(pool: &PgPool) -> Result<Vec<AdvisorFinding>, String> {
    // Unique and primary-key indexes are excluded even at zero scans: they enforce a
    // constraint, so "unused" says nothing about whether they can go.
    let sql = "SELECT s.schemaname, s.relname, s.indexrelname,
                      pg_relation_size(s.indexrelid)::float8,
                      (SELECT COALESCE(EXTRACT(EPOCH FROM (now() - stats_reset)), 0)
                         FROM pg_stat_database WHERE datname = current_database())
                 FROM pg_stat_user_indexes s
                 JOIN pg_index i ON i.indexrelid = s.indexrelid
                WHERE s.idx_scan = 0
                  AND NOT i.indisunique
                  AND NOT i.indisprimary
                  -- Tiny indexes are noise: dropping them saves nothing.
                  AND pg_relation_size(s.indexrelid) >= 1048576
                  -- The index a foreign key's checks use: dropping it would turn
                  -- every parent DELETE into a scan of this table.
                  AND NOT EXISTS (
                        SELECT 1 FROM pg_constraint con
                         WHERE con.contype = 'f' AND con.conrelid = i.indrelid
                           AND (i.indkey::int2[])[1:cardinality(con.conkey::int2[])] @> con.conkey::int2[]
                      )
                ORDER BY 4 DESC";
    let rows = sqlx::query(sql).fetch_all(pool).await.map_err(|e| e.to_string())?;
    Ok(rows
        .iter()
        .map(|r| {
            let schema: String = r.get(0);
            let table: String = r.get(1);
            let index: String = r.get(2);
            let bytes: f64 = r.get(3);
            let since_reset: f64 = r.try_get(4).unwrap_or(0.0);
            // Weighted by what it costs to keep: every write maintains it, and it sits
            // in the buffer cache competing with data.
            let severity = if bytes > 512.0 * 1024.0 * 1024.0 {
                "error"
            } else if bytes > 16.0 * 1024.0 * 1024.0 {
                "warning"
            } else {
                "info"
            };
            let window = if since_reset > 3600.0 {
                format!("{:.0}h of statistics", since_reset / 3600.0)
            } else {
                "a short statistics window - reset recently, so treat with care".into()
            };
            AdvisorFinding {
                check_id: "unused_index".into(),
                category: "performance".into(),
                severity: severity.into(),
                title: "Index never used".into(),
                entity: format!("{schema}.{index}"),
                entity_kind: "index".into(),
                description: "No scan has touched this index, yet every insert and update to the table maintains it and it competes for cache. Confirm the window covers your real workload - a nightly job that has not run yet also looks like this.".into(),
                detail: Some(format!("{} on {schema}.{table} · {window}", bytes_pretty(bytes))),
                remediation: Some(format!("DROP INDEX CONCURRENTLY \"{schema}\".\"{index}\";")),
                metric: bytes,
            }
        })
        .collect())
}

/// Ordinary tables with no primary key.
async fn check_no_primary_key(pool: &PgPool) -> Result<Vec<AdvisorFinding>, String> {
    let sql = format!(
        "SELECT n.nspname, c.relname, c.reltuples::bigint
           FROM pg_class c
           JOIN pg_namespace n ON n.oid = c.relnamespace
          WHERE c.relkind = 'r'
            AND n.nspname NOT IN ({SYSTEM_SCHEMAS})
            AND n.nspname NOT LIKE 'pg\\_temp%'
            AND NOT EXISTS (SELECT 1 FROM pg_index i WHERE i.indrelid = c.oid AND i.indisprimary)
          ORDER BY c.reltuples DESC NULLS LAST"
    );
    let rows = sqlx::query(&sql).fetch_all(pool).await.map_err(|e| e.to_string())?;
    Ok(rows
        .iter()
        .map(|r| {
            let schema: String = r.get(0);
            let table: String = r.get(1);
            let tuples: i64 = r.get(2);
            AdvisorFinding {
                check_id: "no_primary_key".into(),
                category: "schema".into(),
                severity: (if tuples > 10_000 { "warning" } else { "info" }).into(),
                title: "Table without a primary key".into(),
                entity: format!("{schema}.{table}"),
                entity_kind: "table".into(),
                description: "Nothing identifies a row here: duplicates can accumulate unnoticed, logical replication cannot carry updates, and tools that page by key - including this one - fall back to less stable ordering.".into(),
                detail: Some(format!("~{tuples} rows")),
                remediation: Some(format!(
                    "-- pick the column(s) that already identify a row:\nALTER TABLE \"{schema}\".\"{table}\" ADD PRIMARY KEY (id);\n-- or add a surrogate key:\n-- ALTER TABLE \"{schema}\".\"{table}\" ADD COLUMN id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY;"
                )),
                metric: tuples as f64,
            }
        })
        .collect())
}

/// Tables carrying a lot of dead tuples relative to live ones.
async fn check_dead_tuples(pool: &PgPool) -> Result<Vec<AdvisorFinding>, String> {
    let sql = "SELECT schemaname, relname, n_live_tup, n_dead_tup,
                      (n_dead_tup::float8 / GREATEST(n_live_tup, 1)::float8) AS ratio,
                      COALESCE(to_char(GREATEST(last_vacuum, last_autovacuum), 'YYYY-MM-DD HH24:MI'), 'never') AS vac
                 FROM pg_stat_user_tables
                WHERE n_dead_tup > 1000
                  AND (n_dead_tup::float8 / GREATEST(n_live_tup, 1)::float8) > 0.2
                ORDER BY 5 DESC";
    let rows = sqlx::query(sql).fetch_all(pool).await.map_err(|e| e.to_string())?;
    Ok(rows
        .iter()
        .map(|r| {
            let schema: String = r.get(0);
            let table: String = r.get(1);
            let live: i64 = r.get(2);
            let dead: i64 = r.get(3);
            let ratio: f64 = r.get(4);
            let vac: String = r.get(5);
            AdvisorFinding {
                check_id: "dead_tuples".into(),
                category: "performance".into(),
                severity: (if ratio > 0.5 { "error" } else { "warning" }).into(),
                title: "Table bloated with dead rows".into(),
                entity: format!("{schema}.{table}"),
                entity_kind: "table".into(),
                description: "Updated and deleted rows still occupy pages until vacuum reclaims them. Every sequential scan reads them, so both scan cost and disk grow with the bloat.".into(),
                detail: Some(format!(
                    "{dead} dead vs {live} live ({:.0}%) · last vacuum: {vac}",
                    ratio * 100.0
                )),
                remediation: Some(format!(
                    "VACUUM (ANALYZE) \"{schema}\".\"{table}\";\n-- if this recurs, autovacuum is not keeping up with the write rate:\n-- ALTER TABLE \"{schema}\".\"{table}\" SET (autovacuum_vacuum_scale_factor = 0.05);"
                )),
                metric: ratio,
            }
        })
        .collect())
}

/// Tables whose average row is very large once TOAST is counted.
///
/// The metric is table size (main + TOAST, indexes excluded) over estimated rows,
/// NOT `pg_stats.avg_width`. A TOASTed value - the embedding column that makes this
/// check worth having - shows up in `pg_stats` as the ~18-byte pointer stored in the
/// main tuple, so a stats-based check misses precisely the tables that hurt most.
async fn check_wide_rows(pool: &PgPool) -> Result<Vec<AdvisorFinding>, String> {
    let sql = format!(
        "SELECT n.nspname, c.relname, c.reltuples::bigint,
                pg_table_size(c.oid)::float8,
                (pg_table_size(c.oid) / GREATEST(c.reltuples, 1))::float8 AS bpr,
                (SELECT string_agg(t.attname || ' (' || t.typ || ')', ', ')
                   FROM (SELECT a.attname, format_type(a.atttypid, a.atttypmod) AS typ
                           FROM pg_attribute a
                          WHERE a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped
                            AND a.attstorage IN ('x','e')
                          ORDER BY a.attnum LIMIT 4) t) AS toastable
           FROM pg_class c
           JOIN pg_namespace n ON n.oid = c.relnamespace
          WHERE c.relkind = 'r'
            AND n.nspname NOT IN ({SYSTEM_SCHEMAS})
            AND c.reltuples > 100
            AND (pg_table_size(c.oid) / GREATEST(c.reltuples, 1)) > 1024
          ORDER BY 5 DESC"
    );
    let rows = sqlx::query(&sql).fetch_all(pool).await.map_err(|e| e.to_string())?;
    Ok(rows
        .iter()
        .map(|r| {
            let schema: String = r.get(0);
            let table: String = r.get(1);
            let tuples: i64 = r.get(2);
            let size: f64 = r.get(3);
            let bpr: f64 = r.get(4);
            let toastable: Option<String> = r.try_get(5).ok().flatten();
            let severity = if bpr > 8192.0 {
                "warning"
            } else {
                "info"
            };
            AdvisorFinding {
                check_id: "wide_rows".into(),
                category: "performance".into(),
                severity: severity.into(),
                title: "Very wide rows".into(),
                entity: format!("{schema}.{table}"),
                entity_kind: "table".into(),
                description: "Every tool that fetches rows pays this per row - this client included. Selecting a page of a table this wide moves megabytes, so prefer an explicit column list over SELECT * and keep large values (embeddings, documents, blobs) out of browse queries.".into(),
                detail: Some(format!(
                    "{} per row · ~{tuples} rows · {} total{}",
                    bytes_pretty(bpr),
                    bytes_pretty(size),
                    toastable.map(|t| format!(" · large columns: {t}")).unwrap_or_default()
                )),
                remediation: None,
                metric: bpr,
            }
        })
        .collect())
}

// ── Driver ────────────────────────────────────────────────────────────────────

/// Run every check for this engine and collect one report.
///
/// Sequential on purpose: these are catalog reads measured in single-digit
/// milliseconds, and firing them concurrently would take several connections out of
/// a pool the user's own queries are sharing, to save a few ms on a scan they
/// asked for and are waiting on anyway.
pub async fn scan(conn: &super::connection::ActiveConnection) -> Result<AdvisorReport, String> {
    let pool = match conn {
        super::connection::ActiveConnection::Postgres(pool) => pool,
        other => {
            // Every other engine: an honest empty report rather than a clean bill of
            // health. Checks are Postgres-specific catalog queries; MySQL and SQLite
            // need their own, and pretending otherwise would be worse than silence.
            return Ok(AdvisorReport {
                engine: other.driver().to_string(),
                findings: vec![],
                checks: vec![],
                unsupported: true,
            });
        }
    };

    let mut findings: Vec<AdvisorFinding> = Vec::new();
    let mut checks: Vec<AdvisorCheckStatus> = Vec::new();

    macro_rules! run_check {
        ($id:literal, $title:literal, $cat:literal, $f:expr) => {{
            let started = std::time::Instant::now();
            let result = $f(pool).await;
            let ms = started.elapsed().as_millis() as u64;
            match result {
                Ok(found) => {
                    checks.push(AdvisorCheckStatus {
                        id: $id.into(),
                        title: $title.into(),
                        category: $cat.into(),
                        status: "ok".into(),
                        findings: found.len(),
                        ms,
                        error: None,
                    });
                    findings.extend(found);
                }
                Err(e) => checks.push(AdvisorCheckStatus {
                    id: $id.into(),
                    title: $title.into(),
                    category: $cat.into(),
                    status: "error".into(),
                    findings: 0,
                    ms,
                    error: Some(e),
                }),
            }
        }};
    }

    run_check!("policies_ignored", "Policies that are not enforced", "security", check_policies_ignored);
    run_check!("rls_disabled", "Tables without row-level security", "security", check_rls_disabled);
    run_check!("rls_no_policy", "RLS on, but no policy", "security", check_rls_no_policy);
    run_check!("definer_search_path", "SECURITY DEFINER without a search_path", "security", check_definer_search_path);
    run_check!("duplicate_index", "Duplicate indexes", "performance", check_duplicate_index);
    run_check!("stale_stats", "Planner statistics out of date", "performance", check_stale_stats);
    run_check!("sequence_exhaustion", "Sequences running out", "schema", check_sequence_exhaustion);
    run_check!("unindexed_fk", "Foreign keys without an index", "performance", check_unindexed_fk);
    run_check!("unused_index", "Indexes never used", "performance", check_unused_index);
    run_check!("dead_tuples", "Tables bloated with dead rows", "performance", check_dead_tuples);
    run_check!("wide_rows", "Very wide rows", "performance", check_wide_rows);
    run_check!("no_primary_key", "Tables without a primary key", "schema", check_no_primary_key);

    // Worst first, so the report opens on what matters. Severity, then the check's
    // own metric (bytes, ratio, rows) - which is why findings carry one.
    let rank = |s: &str| match s {
        "error" => 0,
        "warning" => 1,
        _ => 2,
    };
    findings.sort_by(|a, b| {
        rank(&a.severity)
            .cmp(&rank(&b.severity))
            .then(b.metric.partial_cmp(&a.metric).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.entity.cmp(&b.entity))
    });

    Ok(AdvisorReport {
        engine: "postgres".into(),
        findings,
        checks,
        unsupported: false,
    })
}

#[cfg(test)]
mod tests {
    use super::bytes_pretty;

    #[test]
    fn formats_sizes_the_way_a_dba_reads_them() {
        assert_eq!(bytes_pretty(512.0), "512 B");
        assert_eq!(bytes_pretty(2048.0), "2.0 KB");
        assert_eq!(bytes_pretty(17_064.0), "16.7 KB");
        assert_eq!(bytes_pretty(203_603_968.0), "194.2 MB");
    }
}
