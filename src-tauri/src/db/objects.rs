//! The sidebar's Objects tab: functions, procedures, triggers, sequences, types
//! and events, per engine, plus each object's definition as runnable DDL and
//! its DROP.
//!
//! Views and materialized views are listed by `list_tables` (the sidebar has
//! always drawn them from there), so the listing here only names them in
//! `kinds`; reading and dropping them goes through this module like the rest.
//!
//! Nothing the frontend sends is spliced into SQL as text. Catalog lookups bind
//! their values where the driver can, identifiers are quoted per dialect, and a
//! Postgres routine's argument list for DROP is read back from the catalog
//! rather than taken from the caller.

use serde::Serialize;
use serde_json::Value;
use sqlx::{MySqlPool, PgPool, Row, SqlitePool};
use tauri::State;

use super::connection::{require_conn, ActiveConnection, D1Config, DbState, LibSqlConfig};
use super::mysql::my_text_named;
use super::query::SqlResult;
use super::sql_util::{esc_backslash_quote, esc_single_quote, quote_backtick, quote_bracket, quote_double};

/// One object in a group.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DbObject {
    /// view | matview | function | procedure | trigger | sequence | type | event
    pub kind: String,
    pub name: String,
    /// A routine's identity arguments ("a integer, b text"): with the name it
    /// names one overload, for reading and dropping it.
    pub args: String,
    /// The argument types alone ("integer[], integer, boolean"), for the row.
    pub arg_types: String,
    /// The extension that owns the object (Postgres), empty for the schema's own.
    pub ext: String,
    /// The table a trigger fires on.
    pub table: String,
    /// One line for the row: what a function returns, when a trigger fires...
    pub detail: String,
    /// aggregate | window for Postgres routines, enum | domain | composite for
    /// types, macro | table_macro for DuckDB, scalar | table for SQL Server.
    pub subtype: String,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectListing {
    /// The kinds this engine has, in sidebar order: one group each.
    pub kinds: Vec<String>,
    /// Whether DROP ... CASCADE means anything here.
    pub cascade: bool,
    pub objects: Vec<DbObject>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectComment {
    pub name: String,
    pub comment: String,
}

/// The kinds each engine keeps, in the order the sidebar shows them.
pub fn kinds_for(conn: &ActiveConnection) -> &'static [&'static str] {
    match conn {
        ActiveConnection::Postgres(_) => &["view", "matview", "function", "procedure", "trigger", "sequence", "type"],
        ActiveConnection::Mysql(_) => &["view", "function", "procedure", "trigger", "event"],
        ActiveConnection::Mssql(_) => &["view", "function", "procedure", "trigger", "sequence"],
        ActiveConnection::Sqlite(_) | ActiveConnection::D1(_) | ActiveConnection::LibSql(_) => &["view", "trigger"],
        ActiveConnection::Duckdb(_) => &["view", "function", "sequence"],
        // Materialized views come back from list_tables as views here.
        ActiveConnection::Clickhouse(_) => &["view", "function"],
        ActiveConnection::Redis(_) | ActiveConnection::Posthog(_) => &[],
    }
}

fn unsupported(kind: &str) -> String {
    format!("This database has no {kind}s")
}

// ── Small helpers ─────────────────────────────────────────────────────────────

/// A JSON cell as text: strings as they are, numbers and booleans printed,
/// NULL as empty.
fn cell(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(other) => other.to_string(),
    }
}

/// Rows of a result as maps from column name to text.
fn rows_by_name(r: &SqlResult) -> Vec<std::collections::HashMap<String, String>> {
    r.rows
        .iter()
        .map(|row| r.columns.iter().enumerate().map(|(i, c)| (c.name.clone(), cell(row.get(i)))).collect())
        .collect()
}

fn get(m: &std::collections::HashMap<String, String>, k: &str) -> String {
    m.get(k).cloned().unwrap_or_default()
}

fn non_empty(s: String) -> Option<String> {
    if s.trim().is_empty() { None } else { Some(s) }
}

/// The statement without the semicolons and blank lines it ends on.
fn trim_statement(sql: &str) -> &str {
    sql.trim().trim_end_matches(';').trim_end()
}

/// `CREATE VIEW` → `CREATE OR REPLACE VIEW` (any keyword after CREATE), when
/// the statement does not say OR REPLACE already. Leading whitespace kept out.
fn or_replace(sql: &str) -> String {
    let t = sql.trim_start();
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("create ") && !lower["create ".len()..].trim_start().starts_with("or ") {
        format!("CREATE OR REPLACE {}", t["create ".len()..].trim_start())
    } else {
        t.to_string()
    }
}

/// Drop the `DEFINER=user@host` MySQL writes into every SHOW CREATE. Kept, it
/// makes the statement fail for anyone who is not that user or lacks SUPER /
/// SET_USER_ID, which is most people opening a definition to edit it.
pub(crate) fn strip_definer(sql: &str) -> String {
    let upper = sql.to_ascii_uppercase();
    let Some(at) = upper.find("DEFINER=") else { return sql.to_string() };
    // Only the clause between CREATE and the object keyword: a body may say
    // DEFINER= in a string or comment and that is not ours to touch.
    let before = &upper[..at];
    if !before.trim_start().starts_with("CREATE") || before.contains('(') {
        return sql.to_string();
    }
    let b = sql.as_bytes();
    let mut i = at + "DEFINER=".len();
    // user@host, each part optionally `quoted`, 'quoted' or "quoted"; or CURRENT_USER[()].
    while i < b.len() && !b[i].is_ascii_whitespace() {
        match b[i] {
            q @ (b'`' | b'\'' | b'"') => {
                i += 1;
                while i < b.len() {
                    if b[i] == q {
                        if i + 1 < b.len() && b[i + 1] == q {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    format!("{}{}", &sql[..at], &sql[i.min(sql.len())..])
}

/// Put the database in front of the object's own name, the first time
/// `<keyword> `name`` appears (case-insensitive keyword), so the statement
/// lands in the database the sidebar shows and not the connection's default.
pub(crate) fn qualify_after(sql: &str, keyword: &str, db: &str, name: &str) -> String {
    let needle = format!("{} {}", keyword.to_ascii_uppercase(), quote_backtick(name));
    let upper = sql.to_ascii_uppercase();
    let upper_needle = needle.to_ascii_uppercase();
    match upper.find(&upper_needle) {
        Some(at) => {
            let name_at = at + keyword.len() + 1;
            format!("{}{}.{}", &sql[..name_at], quote_backtick(db), &sql[name_at..])
        }
        None => sql.to_string(),
    }
}

/// SQL Server: `CREATE PROCEDURE` → `CREATE OR ALTER PROCEDURE` (2016 SP1 and
/// later), skipping the comments a module definition often starts with.
pub(crate) fn create_or_alter(def: &str) -> String {
    let b = def.as_bytes();
    let mut i = 0;
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if def[i..].starts_with("--") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if def[i..].starts_with("/*") {
            match def[i + 2..].find("*/") {
                Some(p) => i = i + 2 + p + 2,
                None => return def.to_string(),
            }
            continue;
        }
        break;
    }
    let rest = &def[i..];
    let lower = rest.to_ascii_lowercase();
    if lower.starts_with("create") && !lower["create".len()..].trim_start().starts_with("or ") {
        let after = rest["create".len()..].trim_start();
        format!("{}CREATE OR ALTER {}", &def[..i], after)
    } else {
        def.to_string()
    }
}

/// Timing and events out of an SQLite `CREATE TRIGGER`, which has no columns for them.
fn sqlite_trigger_detail(ddl: &str) -> String {
    let u = ddl.to_ascii_uppercase();
    let timing = if u.contains("INSTEAD OF") { "INSTEAD OF" } else if u.contains("BEFORE") { "BEFORE" } else { "AFTER" };
    // The events sit between the timing and ON; the body may name others.
    let head = u.split(" ON ").next().unwrap_or(&u);
    let events: Vec<&str> = ["INSERT", "UPDATE", "DELETE"].into_iter().filter(|e| head.contains(e)).collect();
    format!("{timing} {}", events.join(", ")).trim().to_string()
}

fn trigger_row(name: String, table: String, detail: String, comment: Option<String>) -> DbObject {
    DbObject { kind: "trigger".into(), name, table, detail, comment, ..Default::default() }
}

// ── Listing ───────────────────────────────────────────────────────────────────

pub async fn list_db_objects(state: State<'_, DbState>, schema: String) -> Result<ObjectListing, String> {
    let conn = require_conn(&state)?;
    let kinds: Vec<String> = kinds_for(&conn).iter().map(|k| k.to_string()).collect();
    let cascade = matches!(conn, ActiveConnection::Postgres(_));
    let objects = match &conn {
        ActiveConnection::Postgres(pool) => list_pg(pool, &schema).await?,
        ActiveConnection::Mysql(pool) => list_mysql(pool, &schema).await?,
        ActiveConnection::Mssql(h) => list_mssql(h, &schema).await?,
        ActiveConnection::Sqlite(pool) => list_sqlite(pool).await?,
        ActiveConnection::D1(cfg) => sqlite_triggers_from(d1_query(cfg, SQLITE_TRIGGERS, vec![]).await?),
        ActiveConnection::LibSql(cfg) => sqlite_triggers_from(libsql_query(cfg, SQLITE_TRIGGERS, vec![]).await?),
        ActiveConnection::Duckdb(h) => list_duckdb(h, &schema).await?,
        ActiveConnection::Clickhouse(cfg) => list_clickhouse(cfg).await?,
        ActiveConnection::Redis(_) | ActiveConnection::Posthog(_) => vec![],
    };
    Ok(ObjectListing { kinds, cascade, objects })
}

/// Routines in one schema, every overload its own row. Extension members are
/// in, named with their extension: other clients count them, and pgvector's
/// cosine_distance is a function you call. The sidebar files them under the
/// extension's own node.
const PG_ROUTINES: &str = r#"
    SELECT p.proname::text AS name,
           pg_get_function_identity_arguments(p.oid) AS args,
           oidvectortypes(p.proargtypes) AS arg_types,
           COALESCE(pg_get_function_result(p.oid), '') AS result,
           p.prokind::text AS prokind,
           obj_description(p.oid, 'pg_proc') AS comment,
           (SELECT e.extname::text FROM pg_catalog.pg_depend d JOIN pg_catalog.pg_extension e ON e.oid = d.refobjid
             WHERE d.classid = 'pg_catalog.pg_proc'::regclass AND d.objid = p.oid AND d.deptype = 'e' LIMIT 1) AS ext
    FROM pg_catalog.pg_proc p
    JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = $1
    ORDER BY 1, 2
"#;

/// Postgres before 11 has no prokind.
const PG_ROUTINES_OLD: &str = r#"
    SELECT p.proname::text AS name,
           pg_get_function_identity_arguments(p.oid) AS args,
           COALESCE(pg_get_function_result(p.oid), '') AS result,
           oidvectortypes(p.proargtypes) AS arg_types,
           CASE WHEN p.proisagg THEN 'a' WHEN p.proiswindow THEN 'w' ELSE 'f' END AS prokind,
           obj_description(p.oid, 'pg_proc') AS comment,
           (SELECT e.extname::text FROM pg_catalog.pg_depend d JOIN pg_catalog.pg_extension e ON e.oid = d.refobjid
             WHERE d.classid = 'pg_catalog.pg_proc'::regclass AND d.objid = p.oid AND d.deptype = 'e' LIMIT 1) AS ext
    FROM pg_catalog.pg_proc p
    JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
    WHERE n.nspname = $1
    ORDER BY 1, 2
"#;

const PG_TRIGGERS: &str = r#"
    SELECT t.tgname::text AS name,
           c.relname::text AS tbl,
           CASE WHEN (t.tgtype::integer & 64) = 64 THEN 'INSTEAD OF'
                WHEN (t.tgtype::integer & 2) = 2 THEN 'BEFORE'
                ELSE 'AFTER' END AS timing,
           array_to_string(array_remove(ARRAY[
               CASE WHEN (t.tgtype::integer & 4) = 4 THEN 'INSERT' END,
               CASE WHEN (t.tgtype::integer & 16) = 16 THEN 'UPDATE' END,
               CASE WHEN (t.tgtype::integer & 8) = 8 THEN 'DELETE' END,
               CASE WHEN (t.tgtype::integer & 32) = 32 THEN 'TRUNCATE' END
           ], NULL), ', ') AS events,
           (t.tgenabled::text <> 'D') AS enabled,
           obj_description(t.oid, 'pg_trigger') AS comment
    FROM pg_catalog.pg_trigger t
    JOIN pg_catalog.pg_class c ON c.oid = t.tgrelid
    JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
    WHERE n.nspname = $1 AND NOT t.tgisinternal
    ORDER BY 1, 2
"#;

/// Sequences an identity column owns are part of the column; a serial's
/// sequence (deptype 'a') is shown, with the column it feeds.
const PG_SEQUENCES: &str = r#"
    SELECT c.relname::text AS name,
           format_type(s.seqtypid, NULL) AS data_type,
           (SELECT dc.relname::text || '.' || a.attname::text
              FROM pg_catalog.pg_depend d
              JOIN pg_catalog.pg_class dc ON dc.oid = d.refobjid
              JOIN pg_catalog.pg_attribute a ON a.attrelid = d.refobjid AND a.attnum = d.refobjsubid
             WHERE d.classid = 'pg_catalog.pg_class'::regclass AND d.objid = c.oid AND d.deptype = 'a'
             LIMIT 1) AS owned_by,
           obj_description(c.oid, 'pg_class') AS comment
    FROM pg_catalog.pg_class c
    JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
    JOIN pg_catalog.pg_sequence s ON s.seqrelid = c.oid
    WHERE c.relkind = 'S' AND n.nspname = $1
      AND NOT EXISTS (SELECT 1 FROM pg_catalog.pg_depend d
                      WHERE d.classid = 'pg_catalog.pg_class'::regclass AND d.objid = c.oid AND d.deptype = 'i')
    ORDER BY 1
"#;

const PG_TYPES: &str = r#"
    SELECT t.typname::text AS name,
           t.typtype::text AS typtype,
           CASE t.typtype
               WHEN 'd' THEN format_type(t.typbasetype, t.typtypmod)
               WHEN 'e' THEN (SELECT count(*)::text FROM pg_catalog.pg_enum e WHERE e.enumtypid = t.oid) || ' values'
               ELSE (SELECT count(*)::text FROM pg_catalog.pg_attribute a
                      WHERE a.attrelid = t.typrelid AND a.attnum > 0 AND NOT a.attisdropped) || ' fields'
           END AS detail,
           obj_description(t.oid, 'pg_type') AS comment,
           (SELECT e.extname::text FROM pg_catalog.pg_depend d JOIN pg_catalog.pg_extension e ON e.oid = d.refobjid
             WHERE d.classid = 'pg_catalog.pg_type'::regclass AND d.objid = t.oid AND d.deptype = 'e' LIMIT 1) AS ext
    FROM pg_catalog.pg_type t
    JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
    LEFT JOIN pg_catalog.pg_class c ON c.oid = t.typrelid
    WHERE n.nspname = $1
      AND (t.typtype IN ('e', 'd') OR (t.typtype = 'c' AND c.relkind = 'c'))
    ORDER BY 1
"#;

async fn list_pg(pool: &PgPool, schema: &str) -> Result<Vec<DbObject>, String> {
    let routines = async {
        match sqlx::query(PG_ROUTINES).bind(schema).fetch_all(pool).await {
            Ok(rows) => Ok(rows),
            Err(e) if e.to_string().contains("prokind") => sqlx::query(PG_ROUTINES_OLD).bind(schema).fetch_all(pool).await,
            Err(e) => Err(e),
        }
    };
    let triggers = sqlx::query(PG_TRIGGERS).bind(schema).fetch_all(pool);
    // pg_sequence is Postgres 10+. Older servers list no sequences rather than fail the tab.
    let sequences = async { sqlx::query(PG_SEQUENCES).bind(schema).fetch_all(pool).await.unwrap_or_default() };
    let types = sqlx::query(PG_TYPES).bind(schema).fetch_all(pool);
    let (routines, triggers, sequences, types) = tokio::join!(routines, triggers, sequences, types);
    let routines = routines.map_err(|e| format!("Failed to list functions: {e}"))?;
    let triggers = triggers.map_err(|e| format!("Failed to list triggers: {e}"))?;
    let types = types.map_err(|e| format!("Failed to list types: {e}"))?;

    let text = |r: &sqlx::postgres::PgRow, c: &str| r.try_get::<Option<String>, _>(c).ok().flatten().unwrap_or_default();
    let mut out = Vec::new();
    for r in &routines {
        let prokind = text(r, "prokind");
        let result = text(r, "result");
        let (kind, subtype, detail) = match prokind.as_str() {
            "p" => ("procedure", "", String::new()),
            "a" => ("function", "aggregate", format!("aggregate → {result}")),
            "w" => ("function", "window", format!("window → {result}")),
            _ => ("function", "", format!("→ {result}")),
        };
        out.push(DbObject {
            kind: kind.into(),
            name: text(r, "name"),
            args: text(r, "args"),
            arg_types: text(r, "arg_types"),
            ext: text(r, "ext"),
            detail,
            subtype: subtype.into(),
            comment: non_empty(text(r, "comment")),
            ..Default::default()
        });
    }
    for r in &triggers {
        let enabled = r.try_get::<bool, _>("enabled").unwrap_or(true);
        let detail = format!("{} {}{}", text(r, "timing"), text(r, "events"), if enabled { "" } else { " · disabled" });
        out.push(trigger_row(text(r, "name"), text(r, "tbl"), detail, non_empty(text(r, "comment"))));
    }
    for r in &sequences {
        let owned = text(r, "owned_by");
        let detail = if owned.is_empty() { text(r, "data_type") } else { format!("{} · {owned}", text(r, "data_type")) };
        out.push(DbObject { kind: "sequence".into(), name: text(r, "name"), detail, comment: non_empty(text(r, "comment")), ..Default::default() });
    }
    for r in &types {
        let subtype = match text(r, "typtype").as_str() {
            "e" => "enum",
            "d" => "domain",
            _ => "composite",
        };
        out.push(DbObject {
            kind: "type".into(),
            name: text(r, "name"),
            detail: format!("{subtype} · {}", text(r, "detail")),
            subtype: subtype.into(),
            ext: text(r, "ext"),
            comment: non_empty(text(r, "comment")),
            ..Default::default()
        });
    }
    Ok(out)
}

async fn list_mysql(pool: &MySqlPool, schema: &str) -> Result<Vec<DbObject>, String> {
    let routines = sqlx::query(
        r#"SELECT r.ROUTINE_NAME AS name, r.ROUTINE_TYPE AS rtype, r.DTD_IDENTIFIER AS returns,
                  r.ROUTINE_COMMENT AS comment,
                  (SELECT GROUP_CONCAT(CONCAT_WS(' ', p.PARAMETER_MODE, p.PARAMETER_NAME, p.DTD_IDENTIFIER)
                                       ORDER BY p.ORDINAL_POSITION SEPARATOR ', ')
                     FROM information_schema.PARAMETERS p
                    WHERE p.SPECIFIC_SCHEMA = r.ROUTINE_SCHEMA AND p.SPECIFIC_NAME = r.SPECIFIC_NAME
                      AND p.ROUTINE_TYPE = r.ROUTINE_TYPE AND p.ORDINAL_POSITION > 0) AS args,
                  (SELECT GROUP_CONCAT(p.DTD_IDENTIFIER ORDER BY p.ORDINAL_POSITION SEPARATOR ', ')
                     FROM information_schema.PARAMETERS p
                    WHERE p.SPECIFIC_SCHEMA = r.ROUTINE_SCHEMA AND p.SPECIFIC_NAME = r.SPECIFIC_NAME
                      AND p.ROUTINE_TYPE = r.ROUTINE_TYPE AND p.ORDINAL_POSITION > 0) AS arg_types
           FROM information_schema.ROUTINES r
           WHERE r.ROUTINE_SCHEMA = ?
           ORDER BY r.ROUTINE_NAME"#,
    )
    .bind(schema)
    .fetch_all(pool);
    let triggers = sqlx::query(
        r#"SELECT TRIGGER_NAME AS name, EVENT_OBJECT_TABLE AS tbl, ACTION_TIMING AS timing, EVENT_MANIPULATION AS ev
           FROM information_schema.TRIGGERS WHERE TRIGGER_SCHEMA = ? ORDER BY TRIGGER_NAME"#,
    )
    .bind(schema)
    .fetch_all(pool);
    let events = sqlx::query(
        r#"SELECT EVENT_NAME AS name, EVENT_TYPE AS etype, INTERVAL_VALUE AS iv, INTERVAL_FIELD AS ifield,
                  CAST(EXECUTE_AT AS CHAR) AS at_time, STATUS AS status, EVENT_COMMENT AS comment
           FROM information_schema.EVENTS WHERE EVENT_SCHEMA = ? ORDER BY EVENT_NAME"#,
    )
    .bind(schema)
    .fetch_all(pool);
    let (routines, triggers, events) = tokio::join!(routines, triggers, events);
    let routines = routines.map_err(|e| format!("Failed to list routines: {e}"))?;
    let triggers = triggers.map_err(|e| format!("Failed to list triggers: {e}"))?;
    // The event scheduler is optional (and absent on some hosted MySQL); no
    // events is the honest answer when the table can't be read.
    let events = events.unwrap_or_default();

    let t = |r: &sqlx::mysql::MySqlRow, c: &str| my_text_named(r, c).unwrap_or_default();
    let mut out = Vec::new();
    for r in &routines {
        let is_proc = t(r, "rtype").eq_ignore_ascii_case("PROCEDURE");
        let returns = t(r, "returns");
        out.push(DbObject {
            kind: if is_proc { "procedure" } else { "function" }.into(),
            name: t(r, "name"),
            args: t(r, "args"),
            arg_types: t(r, "arg_types"),
            detail: if is_proc || returns.is_empty() { String::new() } else { format!("→ {returns}") },
            comment: non_empty(t(r, "comment")),
            ..Default::default()
        });
    }
    for r in &triggers {
        out.push(trigger_row(t(r, "name"), t(r, "tbl"), format!("{} {}", t(r, "timing"), t(r, "ev")), None));
    }
    for r in &events {
        let mut detail = if t(r, "etype").eq_ignore_ascii_case("RECURRING") {
            format!("every {} {}", t(r, "iv"), t(r, "ifield").to_lowercase())
        } else {
            format!("once at {}", t(r, "at_time"))
        };
        if t(r, "status").eq_ignore_ascii_case("DISABLED") {
            detail.push_str(" · disabled");
        }
        out.push(DbObject { kind: "event".into(), name: t(r, "name"), detail, comment: non_empty(t(r, "comment")), ..Default::default() });
    }
    Ok(out)
}

/// N'...' literal for SQL Server.
fn nlit(s: &str) -> String {
    format!("N'{}'", esc_single_quote(s))
}

async fn list_mssql(h: &super::connection::MssqlHandle, schema: &str) -> Result<Vec<DbObject>, String> {
    let s = nlit(schema);
    let routines = super::mssql::execute_sql(
        h,
        &format!(
            "SELECT o.name, RTRIM(o.type) AS otype, \
               STUFF((SELECT ', ' + p.name + ' ' + TYPE_NAME(p.user_type_id) + CASE WHEN p.is_output = 1 THEN ' OUTPUT' ELSE '' END \
                      FROM sys.parameters p WHERE p.object_id = o.object_id AND p.parameter_id > 0 ORDER BY p.parameter_id \
                      FOR XML PATH(''), TYPE).value('.', 'nvarchar(max)'), 1, 2, '') AS args, \
               STUFF((SELECT ', ' + TYPE_NAME(p.user_type_id) \
                      FROM sys.parameters p WHERE p.object_id = o.object_id AND p.parameter_id > 0 ORDER BY p.parameter_id \
                      FOR XML PATH(''), TYPE).value('.', 'nvarchar(max)'), 1, 2, '') AS arg_types, \
               (SELECT TOP 1 TYPE_NAME(p.user_type_id) FROM sys.parameters p WHERE p.object_id = o.object_id AND p.parameter_id = 0) AS returns, \
               CAST(ep.value AS nvarchar(4000)) AS comment \
             FROM sys.objects o JOIN sys.schemas sc ON sc.schema_id = o.schema_id \
             LEFT JOIN sys.extended_properties ep ON ep.class = 1 AND ep.major_id = o.object_id AND ep.minor_id = 0 AND ep.name = 'MS_Description' \
             WHERE sc.name = {s} AND o.is_ms_shipped = 0 AND o.type IN ('FN','IF','TF','FS','FT','P','PC') \
             ORDER BY o.name"
        ),
    )
    .await?;
    let triggers = super::mssql::execute_sql(
        h,
        &format!(
            "SELECT tr.name, po.name AS tbl, \
               CASE WHEN tr.is_instead_of_trigger = 1 THEN 'INSTEAD OF' ELSE 'AFTER' END AS timing, \
               STUFF((SELECT ', ' + te.type_desc FROM sys.trigger_events te WHERE te.object_id = tr.object_id \
                      FOR XML PATH(''), TYPE).value('.', 'nvarchar(max)'), 1, 2, '') AS ev, \
               tr.is_disabled \
             FROM sys.triggers tr JOIN sys.objects po ON po.object_id = tr.parent_id \
             JOIN sys.schemas sc ON sc.schema_id = po.schema_id \
             WHERE tr.parent_class = 1 AND sc.name = {s} ORDER BY tr.name"
        ),
    )
    .await?;
    let sequences = super::mssql::execute_sql(
        h,
        &format!(
            "SELECT seq.name, TYPE_NAME(seq.user_type_id) AS dtype FROM sys.sequences seq \
             JOIN sys.schemas sc ON sc.schema_id = seq.schema_id WHERE sc.name = {s} ORDER BY seq.name"
        ),
    )
    .await?;

    let mut out = Vec::new();
    for m in rows_by_name(&routines) {
        let otype = get(&m, "otype");
        let is_proc = otype == "P" || otype == "PC";
        let table_fn = matches!(otype.as_str(), "IF" | "TF" | "FT");
        let returns = get(&m, "returns");
        out.push(DbObject {
            kind: if is_proc { "procedure" } else { "function" }.into(),
            name: get(&m, "name"),
            args: get(&m, "args"),
            arg_types: get(&m, "arg_types"),
            detail: if is_proc { String::new() } else if table_fn { "→ table".into() } else { format!("→ {returns}") },
            subtype: if is_proc { String::new() } else if table_fn { "table".into() } else { "scalar".into() },
            comment: non_empty(get(&m, "comment")),
            ..Default::default()
        });
    }
    for m in rows_by_name(&triggers) {
        let disabled = matches!(get(&m, "is_disabled").as_str(), "true" | "1");
        let detail = format!("{} {}{}", get(&m, "timing"), get(&m, "ev"), if disabled { " · disabled" } else { "" });
        out.push(trigger_row(get(&m, "name"), get(&m, "tbl"), detail, None));
    }
    for m in rows_by_name(&sequences) {
        out.push(DbObject { kind: "sequence".into(), name: get(&m, "name"), detail: get(&m, "dtype"), ..Default::default() });
    }
    Ok(out)
}

const SQLITE_TRIGGERS: &str = "SELECT name, tbl_name, COALESCE(sql, '') AS sql FROM sqlite_master WHERE type = 'trigger' ORDER BY name";

async fn list_sqlite(pool: &SqlitePool) -> Result<Vec<DbObject>, String> {
    let rows = sqlx::query(SQLITE_TRIGGERS).fetch_all(pool).await.map_err(|e| format!("Failed to list triggers: {e}"))?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            let name: String = r.try_get("name").ok()?;
            let table: String = r.try_get("tbl_name").unwrap_or_default();
            let sql: String = r.try_get("sql").unwrap_or_default();
            Some(trigger_row(name, table, sqlite_trigger_detail(&sql), None))
        })
        .collect())
}

fn sqlite_triggers_from(r: SqlResult) -> Vec<DbObject> {
    rows_by_name(&r)
        .into_iter()
        .map(|m| trigger_row(get(&m, "name"), get(&m, "tbl_name"), sqlite_trigger_detail(&get(&m, "sql")), None))
        .collect()
}

async fn d1_query(cfg: &D1Config, sql: &str, params: Vec<Value>) -> Result<SqlResult, String> {
    super::d1::query(cfg, sql, params).await
}

async fn libsql_query(cfg: &LibSqlConfig, sql: &str, params: Vec<Value>) -> Result<SqlResult, String> {
    super::libsql::query(cfg, sql, params).await
}

/// `[a, b]` (DuckDB prints a list that way) → `a, b`.
fn duck_list(s: &str) -> String {
    s.trim().trim_start_matches('[').trim_end_matches(']').trim().to_string()
}

async fn list_duckdb(h: &super::connection::DuckdbHandle, schema: &str) -> Result<Vec<DbObject>, String> {
    let s = esc_single_quote(schema);
    let funcs = super::duckdb::execute_sql(
        h,
        &format!(
            "SELECT function_name AS name, function_type AS ftype, CAST(parameters AS VARCHAR) AS params, comment \
             FROM duckdb_functions() \
             WHERE NOT internal AND database_name = current_database() AND schema_name = '{s}' \
               AND function_type IN ('macro', 'table_macro') \
             ORDER BY function_name"
        ),
    )
    .await?;
    let seqs = super::duckdb::execute_sql(
        h,
        &format!(
            "SELECT sequence_name AS name, comment FROM duckdb_sequences() \
             WHERE database_name = current_database() AND schema_name = '{s}' AND NOT temporary ORDER BY sequence_name"
        ),
    )
    .await?;
    let mut out = Vec::new();
    for m in rows_by_name(&funcs) {
        let table = get(&m, "ftype") == "table_macro";
        out.push(DbObject {
            kind: "function".into(),
            name: get(&m, "name"),
            args: duck_list(&get(&m, "params")),
            arg_types: duck_list(&get(&m, "params")),
            detail: if table { "table macro".into() } else { "macro".into() },
            subtype: get(&m, "ftype"),
            comment: non_empty(get(&m, "comment")),
            ..Default::default()
        });
    }
    for m in rows_by_name(&seqs) {
        out.push(DbObject { kind: "sequence".into(), name: get(&m, "name"), comment: non_empty(get(&m, "comment")), ..Default::default() });
    }
    Ok(out)
}

async fn list_clickhouse(cfg: &super::connection::ClickhouseConfig) -> Result<Vec<DbObject>, String> {
    // SQL user-defined functions are server-wide, not per database.
    let r = super::clickhouse::query(cfg, "SELECT name FROM system.functions WHERE origin = 'SQLUserDefined' ORDER BY name").await?;
    Ok(rows_by_name(&r)
        .into_iter()
        .map(|m| DbObject { kind: "function".into(), name: get(&m, "name"), detail: "SQL function".into(), ..Default::default() })
        .collect())
}

// ── Definitions ───────────────────────────────────────────────────────────────

/// The object as a statement that recreates it: CREATE OR REPLACE where the
/// engine has it, a DROP ... IF EXISTS in front where it does not.
pub async fn get_object_definition(
    state: State<'_, DbState>,
    kind: String,
    schema: String,
    name: String,
    args: String,
    table: String,
) -> Result<String, String> {
    let conn = require_conn(&state)?;
    if !kinds_for(&conn).contains(&kind.as_str()) {
        return Err(unsupported(&kind));
    }
    let def = match &conn {
        ActiveConnection::Postgres(pool) => def_pg(pool, &kind, &schema, &name, &args, &table).await?,
        ActiveConnection::Mysql(pool) => def_mysql(pool, &kind, &schema, &name).await?,
        ActiveConnection::Mssql(h) => def_mssql(h, &kind, &schema, &name).await?,
        ActiveConnection::Sqlite(pool) => {
            let sql: Option<String> = sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type = ? AND name = ?")
                .bind(&kind)
                .bind(&name)
                .fetch_optional(pool)
                .await
                .map_err(|e| format!("Failed to read {name}: {e}"))?
                .flatten();
            sqlite_def(&kind, &name, sql)?
        }
        ActiveConnection::D1(cfg) => {
            let r = d1_query(cfg, "SELECT sql FROM sqlite_master WHERE type = ?1 AND name = ?2", vec![Value::from(kind.clone()), Value::from(name.clone())]).await?;
            sqlite_def(&kind, &name, non_empty(cell(r.rows.first().and_then(|row| row.first()))))?
        }
        ActiveConnection::LibSql(cfg) => {
            let r = libsql_query(cfg, "SELECT sql FROM sqlite_master WHERE type = ?1 AND name = ?2", vec![Value::from(kind.clone()), Value::from(name.clone())]).await?;
            sqlite_def(&kind, &name, non_empty(cell(r.rows.first().and_then(|row| row.first()))))?
        }
        ActiveConnection::Duckdb(h) => def_duckdb(h, &kind, &schema, &name).await?,
        ActiveConnection::Clickhouse(cfg) => def_clickhouse(cfg, &kind, &schema, &name).await?,
        ActiveConnection::Redis(_) | ActiveConnection::Posthog(_) => return Err(unsupported(&kind)),
    };
    Ok(format!("{}\n", def.trim_end()))
}

fn not_found(kind: &str, name: &str) -> String {
    format!("No {kind} named {name} here any more. Refresh the list.")
}

async fn pg_scalar(pool: &PgPool, sql: &str, binds: &[&str]) -> Result<Option<String>, String> {
    let mut q = sqlx::query_scalar::<_, Option<String>>(sql);
    for b in binds {
        q = q.bind(*b);
    }
    q.fetch_optional(pool).await.map(|v| v.flatten()).map_err(|e| e.to_string())
}

/// A routine's oid from schema, name and identity arguments, and whether it is
/// an aggregate. Bound values only.
async fn pg_routine(pool: &PgPool, schema: &str, name: &str, args: &str) -> Result<(i64, bool), String> {
    let row = sqlx::query(
        "SELECT p.oid::bigint AS oid, EXISTS (SELECT 1 FROM pg_catalog.pg_aggregate a WHERE a.aggfnoid = p.oid) AS is_agg \
         FROM pg_catalog.pg_proc p JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace \
         WHERE n.nspname = $1 AND p.proname = $2 AND pg_get_function_identity_arguments(p.oid) = $3",
    )
    .bind(schema)
    .bind(name)
    .bind(args)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| not_found("routine", &format!("{name}({args})")))?;
    Ok((row.try_get::<i64, _>("oid").map_err(|e| e.to_string())?, row.try_get::<bool, _>("is_agg").unwrap_or(false)))
}

/// The extension a Postgres routine or type belongs to, if any.
async fn pg_owning_extension(pool: &PgPool, kind: &str, schema: &str, name: &str, args: &str) -> Result<Option<String>, String> {
    let sql = match kind {
        "function" | "procedure" => {
            "SELECT e.extname::text FROM pg_catalog.pg_proc p \
             JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace \
             JOIN pg_catalog.pg_depend d ON d.classid = 'pg_catalog.pg_proc'::regclass AND d.objid = p.oid AND d.deptype = 'e' \
             JOIN pg_catalog.pg_extension e ON e.oid = d.refobjid \
             WHERE n.nspname = $1 AND p.proname = $2 AND pg_get_function_identity_arguments(p.oid) = $3 LIMIT 1"
        }
        "type" => {
            "SELECT e.extname::text FROM pg_catalog.pg_type t \
             JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace \
             JOIN pg_catalog.pg_depend d ON d.classid = 'pg_catalog.pg_type'::regclass AND d.objid = t.oid AND d.deptype = 'e' \
             JOIN pg_catalog.pg_extension e ON e.oid = d.refobjid \
             WHERE n.nspname = $1 AND t.typname = $2 AND $3 = $3 LIMIT 1"
        }
        _ => return Ok(None),
    };
    sqlx::query_scalar::<_, String>(sql)
        .bind(schema)
        .bind(name)
        .bind(args)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

async fn def_pg(pool: &PgPool, kind: &str, schema: &str, name: &str, args: &str, table: &str) -> Result<String, String> {
    let qn = format!("{}.{}", quote_double(schema), quote_double(name));
    match kind {
        "function" | "procedure" => {
            let (oid, is_agg) = pg_routine(pool, schema, name, args).await?;
            if is_agg {
                // pg_get_functiondef refuses aggregates; build the statement from pg_aggregate.
                let sql = "SELECT 'CREATE OR REPLACE AGGREGATE ' || quote_ident(n.nspname) || '.' || quote_ident(p.proname) \
                           || '(' || pg_get_function_identity_arguments(p.oid) || ') (' \
                           || E'\\n    SFUNC = ' || a.aggtransfn::regproc::text \
                           || E',\\n    STYPE = ' || format_type(a.aggtranstype, NULL) \
                           || CASE WHEN a.aggfinalfn::oid <> 0 THEN E',\\n    FINALFUNC = ' || a.aggfinalfn::regproc::text ELSE '' END \
                           || CASE WHEN a.agginitval IS NOT NULL THEN E',\\n    INITCOND = ' || quote_literal(a.agginitval) ELSE '' END \
                           || E'\\n);' \
                           FROM pg_catalog.pg_proc p JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace \
                           JOIN pg_catalog.pg_aggregate a ON a.aggfnoid = p.oid WHERE p.oid = $1::bigint::oid";
                let def: Option<String> = sqlx::query_scalar(sql).bind(oid).fetch_optional(pool).await.map_err(|e| e.to_string())?;
                return def.ok_or_else(|| not_found("aggregate", name));
            }
            let def: Option<String> = sqlx::query_scalar("SELECT pg_get_functiondef($1::bigint::oid)")
                .bind(oid)
                .fetch_optional(pool)
                .await
                .map_err(|e| format!("Failed to read {name}: {e}"))?;
            let def = def.ok_or_else(|| not_found(kind, name))?;
            Ok(format!("{};", trim_statement(&def)))
        }
        "trigger" => {
            let row = sqlx::query(
                "SELECT pg_get_triggerdef(t.oid, true) AS def, t.tgfoid::bigint AS fn_oid \
                 FROM pg_catalog.pg_trigger t JOIN pg_catalog.pg_class c ON c.oid = t.tgrelid \
                 JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND c.relname = $2 AND t.tgname = $3 AND NOT t.tgisinternal",
            )
            .bind(schema)
            .bind(table)
            .bind(name)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| not_found("trigger", name))?;
            let def: String = row.try_get("def").map_err(|e| e.to_string())?;
            let fn_oid: i64 = row.try_get("fn_oid").map_err(|e| e.to_string())?;
            // The function is where a trigger's logic lives, so it comes along.
            let func: Option<String> = sqlx::query_scalar("SELECT pg_get_functiondef($1::bigint::oid)")
                .bind(fn_oid)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten();
            let mut out = String::new();
            if let Some(f) = func {
                out.push_str("-- The function the trigger runs\n");
                out.push_str(trim_statement(&f));
                out.push_str(";\n\n");
            }
            out.push_str(&format!(
                "DROP TRIGGER IF EXISTS {} ON {}.{};\n{};",
                quote_double(name),
                quote_double(schema),
                quote_double(table),
                trim_statement(&def)
            ));
            Ok(out)
        }
        "view" | "matview" => {
            let relkind = if kind == "view" { "v" } else { "m" };
            let body = pg_scalar(
                pool,
                "SELECT pg_get_viewdef(c.oid, true) FROM pg_catalog.pg_class c \
                 JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind::text = $3",
                &[schema, name, relkind],
            )
            .await?
            .ok_or_else(|| not_found(kind, name))?;
            let body = trim_statement(&body);
            Ok(if kind == "view" {
                format!("CREATE OR REPLACE VIEW {qn} AS\n{body};")
            } else {
                format!(
                    "-- A materialized view can't be replaced in place: to change its query,\n\
                     -- drop it first with DROP MATERIALIZED VIEW {qn};\n\
                     CREATE MATERIALIZED VIEW {qn} AS\n{body}\nWITH DATA;"
                )
            })
        }
        "sequence" => {
            let row = sqlx::query(
                "SELECT format_type(s.seqtypid, NULL) AS dtype, s.seqstart, s.seqincrement, s.seqmin, s.seqmax, s.seqcache, s.seqcycle \
                 FROM pg_catalog.pg_sequence s JOIN pg_catalog.pg_class c ON c.oid = s.seqrelid \
                 JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = $1 AND c.relname = $2",
            )
            .bind(schema)
            .bind(name)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| not_found("sequence", name))?;
            let n = |c: &str| row.try_get::<i64, _>(c).unwrap_or(0);
            let dtype: String = row.try_get("dtype").unwrap_or_else(|_| "bigint".into());
            let cycle: bool = row.try_get("seqcycle").unwrap_or(false);
            Ok(format!(
                "CREATE SEQUENCE IF NOT EXISTS {qn}\n    AS {dtype}\n    INCREMENT BY {}\n    MINVALUE {}\n    MAXVALUE {}\n    START WITH {}\n    CACHE {}\n    {}CYCLE;",
                n("seqincrement"),
                n("seqmin"),
                n("seqmax"),
                n("seqstart"),
                n("seqcache"),
                if cycle { "" } else { "NO " }
            ))
        }
        "type" => {
            let row = sqlx::query(
                "SELECT t.typtype::text AS typtype, format_type(t.typbasetype, t.typtypmod) AS base, t.typnotnull, t.typdefault, \
                   (SELECT string_agg(quote_literal(e.enumlabel), ', ' ORDER BY e.enumsortorder) FROM pg_catalog.pg_enum e WHERE e.enumtypid = t.oid) AS labels, \
                   (SELECT string_agg(quote_ident(a.attname) || ' ' || format_type(a.atttypid, a.atttypmod), E',\\n    ' ORDER BY a.attnum) \
                      FROM pg_catalog.pg_attribute a WHERE a.attrelid = t.typrelid AND a.attnum > 0 AND NOT a.attisdropped) AS fields, \
                   (SELECT string_agg('CONSTRAINT ' || quote_ident(con.conname) || ' ' || pg_get_constraintdef(con.oid), E'\\n    ' ORDER BY con.conname) \
                      FROM pg_catalog.pg_constraint con WHERE con.contypid = t.oid) AS checks \
                 FROM pg_catalog.pg_type t JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace \
                 WHERE n.nspname = $1 AND t.typname = $2",
            )
            .bind(schema)
            .bind(name)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| not_found("type", name))?;
            let text = |c: &str| row.try_get::<Option<String>, _>(c).ok().flatten().unwrap_or_default();
            Ok(match text("typtype").as_str() {
                "e" => format!("CREATE TYPE {qn} AS ENUM ({});", text("labels")),
                "d" => {
                    let mut s = format!("CREATE DOMAIN {qn} AS {}", text("base"));
                    let default = text("typdefault");
                    if !default.is_empty() {
                        s.push_str(&format!("\n    DEFAULT {default}"));
                    }
                    if row.try_get::<bool, _>("typnotnull").unwrap_or(false) {
                        s.push_str("\n    NOT NULL");
                    }
                    let checks = text("checks");
                    if !checks.is_empty() {
                        s.push_str(&format!("\n    {checks}"));
                    }
                    s.push(';');
                    s
                }
                _ => format!("CREATE TYPE {qn} AS (\n    {}\n);", text("fields")),
            })
        }
        _ => Err(unsupported(kind)),
    }
}

/// SHOW CREATE goes over the text protocol: MySQL won't prepare SHOW CREATE
/// TRIGGER, nor any of the statements this module runs for routines.
async fn mysql_show(pool: &MySqlPool, sql: &str, column: &str) -> Result<Option<String>, String> {
    let rows = sqlx::raw_sql(sql).fetch_all(pool).await.map_err(|e| e.to_string())?;
    Ok(rows.first().and_then(|r| my_text_named(r, column)))
}

async fn def_mysql(pool: &MySqlPool, kind: &str, db: &str, name: &str) -> Result<String, String> {
    let qn = format!("{}.{}", quote_backtick(db), quote_backtick(name));
    let (keyword, column) = match kind {
        "function" => ("FUNCTION", "Create Function"),
        "procedure" => ("PROCEDURE", "Create Procedure"),
        "trigger" => ("TRIGGER", "SQL Original Statement"),
        "view" => ("VIEW", "Create View"),
        "event" => ("EVENT", "Create Event"),
        _ => return Err(unsupported(kind)),
    };
    let def = mysql_show(pool, &format!("SHOW CREATE {keyword} {qn}"), column)
        .await
        .map_err(|e| format!("Failed to read {name}: {e}"))?
        .ok_or_else(|| {
            format!("MySQL returned no definition for {name}: reading it takes the SHOW_ROUTINE privilege or owning the object")
        })?;
    let def = qualify_after(&strip_definer(&def), keyword, db, name);
    Ok(if kind == "view" {
        format!("{};", trim_statement(&or_replace(&def)))
    } else {
        format!("DROP {keyword} IF EXISTS {qn};\n{};", trim_statement(&def))
    })
}

async fn def_mssql(h: &super::connection::MssqlHandle, kind: &str, schema: &str, name: &str) -> Result<String, String> {
    let full = format!("{}.{}", quote_bracket(schema), quote_bracket(name));
    if kind == "sequence" {
        let r = super::mssql::execute_sql(
            h,
            &format!(
                "SELECT TYPE_NAME(user_type_id) AS dtype, CAST(start_value AS nvarchar(40)) AS start_value, \
                   CAST(increment AS nvarchar(40)) AS inc, CAST(minimum_value AS nvarchar(40)) AS min_value, \
                   CAST(maximum_value AS nvarchar(40)) AS max_value, is_cycling, is_cached, cache_size \
                 FROM sys.sequences WHERE object_id = OBJECT_ID({})",
                nlit(&full)
            ),
        )
        .await?;
        let m = rows_by_name(&r).into_iter().next().ok_or_else(|| not_found("sequence", name))?;
        let cycling = matches!(get(&m, "is_cycling").as_str(), "true" | "1");
        let cached = matches!(get(&m, "is_cached").as_str(), "true" | "1");
        let cache = get(&m, "cache_size");
        return Ok(format!(
            "-- SQL Server has no CREATE OR ALTER SEQUENCE: change it with ALTER SEQUENCE, or drop it first.\n\
             CREATE SEQUENCE {full}\n    AS {}\n    START WITH {}\n    INCREMENT BY {}\n    MINVALUE {}\n    MAXVALUE {}\n    {}\n    {};",
            get(&m, "dtype"),
            get(&m, "start_value"),
            get(&m, "inc"),
            get(&m, "min_value"),
            get(&m, "max_value"),
            if cycling { "CYCLE" } else { "NO CYCLE" },
            if !cached { "NO CACHE".to_string() } else if cache.is_empty() { "CACHE".to_string() } else { format!("CACHE {cache}") }
        ));
    }
    let r = super::mssql::execute_sql(h, &format!("SELECT OBJECT_DEFINITION(OBJECT_ID({})) AS def", nlit(&full))).await?;
    let def = non_empty(cell(r.rows.first().and_then(|row| row.first())))
        .ok_or_else(|| format!("SQL Server returned no definition for {name}: it is encrypted, or the login lacks VIEW DEFINITION"))?;
    Ok(create_or_alter(def.trim()))
}

fn sqlite_def(kind: &str, name: &str, sql: Option<String>) -> Result<String, String> {
    let sql = sql.ok_or_else(|| not_found(kind, name))?;
    let keyword = if kind == "view" { "VIEW" } else { "TRIGGER" };
    // SQLite has no CREATE OR REPLACE.
    Ok(format!("DROP {keyword} IF EXISTS {};\n{};", quote_double(name), trim_statement(&sql)))
}

async fn def_duckdb(h: &super::connection::DuckdbHandle, kind: &str, schema: &str, name: &str) -> Result<String, String> {
    let (s, n) = (esc_single_quote(schema), esc_single_quote(name));
    let qn = format!("{}.{}", quote_double(schema), quote_double(name));
    let scope = format!("database_name = current_database() AND schema_name = '{s}'");
    match kind {
        "function" => {
            let r = super::duckdb::execute_sql(
                h,
                &format!(
                    "SELECT function_type AS ftype, CAST(parameters AS VARCHAR) AS params, macro_definition AS body \
                     FROM duckdb_functions() WHERE NOT internal AND {scope} AND function_name = '{n}' \
                       AND function_type IN ('macro', 'table_macro') LIMIT 1"
                ),
            )
            .await?;
            let m = rows_by_name(&r).into_iter().next().ok_or_else(|| not_found("macro", name))?;
            let table = if get(&m, "ftype") == "table_macro" { "TABLE " } else { "" };
            Ok(format!("CREATE OR REPLACE MACRO {qn}({}) AS {table}{};", duck_list(&get(&m, "params")), trim_statement(&get(&m, "body"))))
        }
        "view" => {
            let r = super::duckdb::execute_sql(h, &format!("SELECT sql FROM duckdb_views() WHERE {scope} AND view_name = '{n}'")).await?;
            let sql = non_empty(cell(r.rows.first().and_then(|row| row.first()))).ok_or_else(|| not_found("view", name))?;
            Ok(format!("{};", trim_statement(&or_replace(&sql))))
        }
        "sequence" => {
            let r = super::duckdb::execute_sql(h, &format!("SELECT sql FROM duckdb_sequences() WHERE {scope} AND sequence_name = '{n}'")).await?;
            let sql = non_empty(cell(r.rows.first().and_then(|row| row.first()))).ok_or_else(|| not_found("sequence", name))?;
            Ok(format!("{};", trim_statement(&sql)))
        }
        _ => Err(unsupported(kind)),
    }
}

async fn def_clickhouse(cfg: &super::connection::ClickhouseConfig, kind: &str, db: &str, name: &str) -> Result<String, String> {
    match kind {
        "function" => {
            let r = super::clickhouse::query(
                cfg,
                &format!("SELECT create_query FROM system.functions WHERE origin = 'SQLUserDefined' AND name = '{}'", esc_backslash_quote(name)),
            )
            .await?;
            let sql = non_empty(cell(r.rows.first().and_then(|row| row.first()))).ok_or_else(|| not_found("function", name))?;
            Ok(format!("{};", trim_statement(&or_replace(&sql))))
        }
        "view" => {
            let r = super::clickhouse::query(cfg, &format!("SHOW CREATE TABLE {}.{}", quote_backtick(db), quote_backtick(name))).await?;
            let sql = non_empty(cell(r.rows.first().and_then(|row| row.first()))).ok_or_else(|| not_found("view", name))?;
            // A materialized view can't be replaced; a plain one can.
            let replaced = if sql.trim_start().to_ascii_uppercase().starts_with("CREATE VIEW") { or_replace(&sql) } else { sql };
            Ok(format!("{};", trim_statement(&replaced)))
        }
        _ => Err(unsupported(kind)),
    }
}

// ── Drop ──────────────────────────────────────────────────────────────────────

/// Drop one object. Returns the statement that ran, for the log.
#[allow(clippy::too_many_arguments)]
pub async fn drop_object(
    state: State<'_, DbState>,
    kind: String,
    schema: String,
    name: String,
    args: String,
    table: String,
    subtype: String,
    cascade: bool,
) -> Result<String, String> {
    let conn = require_conn(&state)?;
    if !kinds_for(&conn).contains(&kind.as_str()) {
        return Err(unsupported(&kind));
    }
    let fail = |e: String| format!("Failed to drop {name}: {e}");
    match &conn {
        ActiveConnection::Postgres(pool) => {
            let qn = format!("{}.{}", quote_double(&schema), quote_double(&name));
            let tail = if cascade { " CASCADE" } else { "" };
            if let Some(ext) = pg_owning_extension(pool, &kind, &schema, &name, &args).await? {
                return Err(format!("{name} is owned by the {ext} extension; DROP EXTENSION {ext} removes it"));
            }
            let sql = match kind.as_str() {
                "function" | "procedure" => {
                    let (oid, is_agg) = pg_routine(pool, &schema, &name, &args).await?;
                    // The argument list as the catalog prints it, never as the caller sent it.
                    let ident: String = sqlx::query_scalar("SELECT pg_get_function_identity_arguments($1::bigint::oid)")
                        .bind(oid)
                        .fetch_one(pool)
                        .await
                        .map_err(|e| e.to_string())?;
                    let keyword = if is_agg { "AGGREGATE" } else if kind == "procedure" { "PROCEDURE" } else { "FUNCTION" };
                    format!("DROP {keyword} {qn}({ident}){tail}")
                }
                "trigger" => format!("DROP TRIGGER {} ON {}.{}{tail}", quote_double(&name), quote_double(&schema), quote_double(&table)),
                "view" => format!("DROP VIEW {qn}{tail}"),
                "matview" => format!("DROP MATERIALIZED VIEW {qn}{tail}"),
                "sequence" => format!("DROP SEQUENCE {qn}{tail}"),
                "type" => format!("DROP {} {qn}{tail}", if subtype == "domain" { "DOMAIN" } else { "TYPE" }),
                _ => return Err(unsupported(&kind)),
            };
            sqlx::raw_sql(&sql).execute(pool).await.map_err(|e| fail(e.to_string()))?;
            Ok(sql)
        }
        ActiveConnection::Mysql(pool) => {
            let keyword = match kind.as_str() {
                "function" => "FUNCTION",
                "procedure" => "PROCEDURE",
                "trigger" => "TRIGGER",
                "view" => "VIEW",
                "event" => "EVENT",
                _ => return Err(unsupported(&kind)),
            };
            let sql = format!("DROP {keyword} {}.{}", quote_backtick(&schema), quote_backtick(&name));
            sqlx::raw_sql(&sql).execute(pool).await.map_err(|e| fail(e.to_string()))?;
            Ok(sql)
        }
        ActiveConnection::Mssql(h) => {
            let keyword = match kind.as_str() {
                "function" => "FUNCTION",
                "procedure" => "PROCEDURE",
                "trigger" => "TRIGGER",
                "view" => "VIEW",
                "sequence" => "SEQUENCE",
                _ => return Err(unsupported(&kind)),
            };
            let sql = format!("DROP {keyword} {}.{}", quote_bracket(&schema), quote_bracket(&name));
            super::mssql::execute_sql(h, &sql).await.map_err(fail)?;
            Ok(sql)
        }
        ActiveConnection::Sqlite(_) | ActiveConnection::D1(_) | ActiveConnection::LibSql(_) => {
            let keyword = if kind == "view" { "VIEW" } else { "TRIGGER" };
            let sql = format!("DROP {keyword} {}", quote_double(&name));
            match &conn {
                ActiveConnection::Sqlite(pool) => {
                    sqlx::query(&sql).execute(pool).await.map_err(|e| fail(e.to_string()))?;
                }
                ActiveConnection::D1(cfg) => {
                    d1_query(cfg, &sql, vec![]).await.map_err(fail)?;
                }
                ActiveConnection::LibSql(cfg) => {
                    libsql_query(cfg, &sql, vec![]).await.map_err(fail)?;
                }
                _ => {}
            }
            Ok(sql)
        }
        ActiveConnection::Duckdb(h) => {
            let qn = format!("{}.{}", quote_double(&schema), quote_double(&name));
            let sql = match kind.as_str() {
                "function" => format!("DROP MACRO {}{qn}", if subtype == "table_macro" { "TABLE " } else { "" }),
                "view" => format!("DROP VIEW {qn}"),
                "sequence" => format!("DROP SEQUENCE {qn}"),
                _ => return Err(unsupported(&kind)),
            };
            super::duckdb::execute_sql(h, &sql).await.map_err(fail)?;
            Ok(sql)
        }
        ActiveConnection::Clickhouse(cfg) => {
            let sql = match kind.as_str() {
                "function" => format!("DROP FUNCTION {}", quote_backtick(&name)),
                "view" => format!("DROP VIEW {}.{}", quote_backtick(&schema), quote_backtick(&name)),
                _ => return Err(unsupported(&kind)),
            };
            super::clickhouse::query(cfg, &sql).await.map_err(fail)?;
            Ok(sql)
        }
        ActiveConnection::Redis(_) | ActiveConnection::Posthog(_) => Err(unsupported(&kind)),
    }
}

// ── Comments ──────────────────────────────────────────────────────────────────

/// Table and view comments for one schema, in one query. Engines that keep
/// none (SQLite and its relatives) return an empty list.
pub async fn list_object_comments(state: State<'_, DbState>, schema: String) -> Result<Vec<ObjectComment>, String> {
    let pairs = |r: SqlResult| -> Vec<ObjectComment> {
        r.rows
            .iter()
            .filter_map(|row| {
                let name = cell(row.first());
                let comment = cell(row.get(1));
                if name.is_empty() || comment.trim().is_empty() { None } else { Some(ObjectComment { name, comment }) }
            })
            .collect()
    };
    match require_conn(&state)? {
        ActiveConnection::Postgres(pool) => {
            let rows = sqlx::query(
                "SELECT c.relname::text AS name, obj_description(c.oid, 'pg_class') AS comment \
                 FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND c.relkind IN ('r', 'p', 'v', 'm', 'f') \
                   AND obj_description(c.oid, 'pg_class') IS NOT NULL",
            )
            .bind(&schema)
            .fetch_all(&pool)
            .await
            .map_err(|e| format!("Failed to read comments: {e}"))?;
            Ok(rows
                .iter()
                .filter_map(|r| {
                    Some(ObjectComment { name: r.try_get("name").ok()?, comment: r.try_get::<Option<String>, _>("comment").ok()??.to_string() })
                })
                .collect())
        }
        ActiveConnection::Mysql(pool) => {
            let rows = sqlx::query(
                "SELECT TABLE_NAME AS name, TABLE_COMMENT AS comment FROM information_schema.TABLES \
                 WHERE TABLE_SCHEMA = ? AND TABLE_TYPE <> 'VIEW' AND TABLE_COMMENT <> ''",
            )
            .bind(&schema)
            .fetch_all(&pool)
            .await
            .map_err(|e| format!("Failed to read comments: {e}"))?;
            Ok(rows
                .iter()
                .filter_map(|r| Some(ObjectComment { name: my_text_named(r, "name")?, comment: my_text_named(r, "comment")? }))
                .collect())
        }
        ActiveConnection::Mssql(h) => Ok(pairs(
            super::mssql::execute_sql(
                &h,
                &format!(
                    "SELECT o.name, CAST(ep.value AS nvarchar(4000)) AS comment FROM sys.extended_properties ep \
                     JOIN sys.objects o ON o.object_id = ep.major_id JOIN sys.schemas sc ON sc.schema_id = o.schema_id \
                     WHERE ep.class = 1 AND ep.minor_id = 0 AND ep.name = 'MS_Description' AND o.type IN ('U', 'V') AND sc.name = {}",
                    nlit(&schema)
                ),
            )
            .await?,
        )),
        ActiveConnection::Clickhouse(cfg) => Ok(pairs(
            super::clickhouse::query(
                &cfg,
                &format!("SELECT name, comment FROM system.tables WHERE database = '{}' AND comment <> ''", esc_backslash_quote(&schema)),
            )
            .await?,
        )),
        ActiveConnection::Duckdb(h) => {
            let s = esc_single_quote(&schema);
            Ok(pairs(
                super::duckdb::execute_sql(
                    &h,
                    &format!(
                        "SELECT table_name, comment FROM duckdb_tables() WHERE database_name = current_database() AND schema_name = '{s}' AND comment IS NOT NULL \
                         UNION ALL \
                         SELECT view_name, comment FROM duckdb_views() WHERE NOT internal AND database_name = current_database() AND schema_name = '{s}' AND comment IS NOT NULL"
                    ),
                )
                .await?,
            ))
        }
        _ => Ok(vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_the_definer_clause_only() {
        assert_eq!(
            strip_definer("CREATE DEFINER=`root`@`%` PROCEDURE `p`() SELECT 1"),
            "CREATE PROCEDURE `p`() SELECT 1"
        );
        assert_eq!(
            strip_definer("CREATE ALGORITHM=UNDEFINED DEFINER=`a``b`@`localhost` SQL SECURITY DEFINER VIEW `v` AS select 1"),
            "CREATE ALGORITHM=UNDEFINED SQL SECURITY DEFINER VIEW `v` AS select 1"
        );
        assert_eq!(strip_definer("CREATE DEFINER=CURRENT_USER TRIGGER t"), "CREATE TRIGGER t");
        // A DEFINER= inside the body is not the clause.
        let body = "CREATE PROCEDURE p() SELECT 'DEFINER=x'";
        assert_eq!(strip_definer(body), body);
        assert_eq!(strip_definer("CREATE FUNCTION f() RETURNS INT RETURN 1"), "CREATE FUNCTION f() RETURNS INT RETURN 1");
    }

    #[test]
    fn qualifies_the_object_name_once() {
        assert_eq!(
            qualify_after("CREATE PROCEDURE `p`() BEGIN CALL `p`(); END", "PROCEDURE", "shop", "p"),
            "CREATE PROCEDURE `shop`.`p`() BEGIN CALL `p`(); END"
        );
        assert_eq!(qualify_after("CREATE TRIGGER `t` BEFORE INSERT ON `x`", "TRIGGER", "a`b", "t"), "CREATE TRIGGER `a``b`.`t` BEFORE INSERT ON `x`");
        // Already qualified, or a different name: left alone.
        assert_eq!(qualify_after("CREATE VIEW `db`.`v` AS select 1", "VIEW", "db", "v"), "CREATE VIEW `db`.`v` AS select 1");
    }

    #[test]
    fn create_becomes_create_or_alter() {
        assert_eq!(create_or_alter("CREATE PROCEDURE dbo.p AS SELECT 1"), "CREATE OR ALTER PROCEDURE dbo.p AS SELECT 1");
        assert_eq!(
            create_or_alter("-- header\n/* note */\ncreate   view dbo.v as select 1"),
            "-- header\n/* note */\nCREATE OR ALTER view dbo.v as select 1"
        );
        assert_eq!(create_or_alter("CREATE OR ALTER FUNCTION f() RETURNS int AS BEGIN RETURN 1 END"), "CREATE OR ALTER FUNCTION f() RETURNS int AS BEGIN RETURN 1 END");
    }

    #[test]
    fn or_replace_and_statement_trim() {
        assert_eq!(or_replace("  CREATE VIEW v AS SELECT 1"), "CREATE OR REPLACE VIEW v AS SELECT 1");
        assert_eq!(or_replace("CREATE OR REPLACE VIEW v AS SELECT 1"), "CREATE OR REPLACE VIEW v AS SELECT 1");
        assert_eq!(trim_statement(" SELECT 1;\n\n"), "SELECT 1");
    }

    #[test]
    fn sqlite_trigger_timing_and_events() {
        assert_eq!(sqlite_trigger_detail("CREATE TRIGGER t AFTER UPDATE OF a ON x BEGIN DELETE FROM y; END"), "AFTER UPDATE");
        assert_eq!(sqlite_trigger_detail("CREATE TRIGGER t BEFORE INSERT ON x BEGIN SELECT 1; END"), "BEFORE INSERT");
        assert_eq!(sqlite_trigger_detail("create trigger t instead of delete on v begin select 1; end"), "INSTEAD OF DELETE");
    }

    #[test]
    fn duckdb_lists_and_cells() {
        assert_eq!(duck_list("[a, b]"), "a, b");
        assert_eq!(duck_list("[]"), "");
        assert_eq!(cell(Some(&Value::from(3))), "3");
        assert_eq!(cell(Some(&Value::Null)), "");
        assert_eq!(cell(None), "");
    }
}
