//! DDL that every engine spells its own way: dropping or truncating a table or
//! view, running one statement outside a transaction, and copying a whole
//! database under a new name.
//!
//! The statement builders are pure so they can be tested without a server; the
//! executors only pick the driver call for the live connection.

use serde::Serialize;
use serde_json::Value;
use sqlx::mysql::MySqlConnection;
use sqlx::{Executor, MySqlPool};

use super::connection::{ActiveConnection, ClickhouseConfig, MssqlHandle};
use super::mysql::my_text;
use super::sql_util::{esc_backslash_quote, esc_single_quote, quote_backtick, quote_bracket, quote_double};

/// How a statement is spelled. D1 and libSQL speak SQLite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dialect {
    Postgres,
    Mysql,
    Sqlite,
    Duckdb,
    Mssql,
    Clickhouse,
}

impl Dialect {
    /// None for the connections that have no DDL at all (Redis, PostHog).
    pub(crate) fn of(conn: &ActiveConnection) -> Option<Self> {
        match conn {
            ActiveConnection::Postgres(_) => Some(Self::Postgres),
            ActiveConnection::Mysql(_) => Some(Self::Mysql),
            ActiveConnection::Sqlite(_) | ActiveConnection::D1(_) | ActiveConnection::LibSql(_) => Some(Self::Sqlite),
            ActiveConnection::Duckdb(_) => Some(Self::Duckdb),
            ActiveConnection::Mssql(_) => Some(Self::Mssql),
            ActiveConnection::Clickhouse(_) => Some(Self::Clickhouse),
            ActiveConnection::Redis(_) | ActiveConnection::Posthog(_) => None,
        }
    }

    /// CASCADE is real on Postgres and DuckDB. MySQL parses it and ignores it,
    /// the rest reject it, so it is only ever written where it does something.
    pub(crate) fn supports_cascade(self) -> bool {
        matches!(self, Self::Postgres | Self::Duckdb)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ObjectKind {
    Table,
    View,
    MaterializedView,
}

impl ObjectKind {
    pub(crate) fn parse(kind: Option<&str>) -> Result<Self, String> {
        match kind.unwrap_or("table") {
            "" | "table" => Ok(Self::Table),
            "view" => Ok(Self::View),
            "materialized_view" => Ok(Self::MaterializedView),
            other => Err(format!("Unknown object kind: {other}")),
        }
    }
}

/// The only rule a name has to meet here is surviving the quotes around it,
/// and the quote helpers double the quote character, so any name the catalog
/// lists can be dropped. A NUL byte is the one thing no engine accepts.
fn check_name(what: &str, name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err(format!("{what} name is empty"));
    }
    if name.contains('\0') {
        return Err(format!("{what} name contains a NUL byte"));
    }
    Ok(())
}

/// `schema.name` in the dialect's quotes. SQLite and DuckDB connections only
/// ever list `main`, so their names stay unqualified.
fn qualified(d: Dialect, schema: &str, name: &str) -> String {
    let schema = schema.trim();
    match d {
        Dialect::Sqlite | Dialect::Duckdb => quote_double(name),
        _ if schema.is_empty() => quote(d, name),
        _ => format!("{}.{}", quote(d, schema), quote(d, name)),
    }
}

fn quote(d: Dialect, ident: &str) -> String {
    match d {
        Dialect::Postgres | Dialect::Sqlite | Dialect::Duckdb => quote_double(ident),
        Dialect::Mysql | Dialect::Clickhouse => quote_backtick(ident),
        Dialect::Mssql => quote_bracket(ident),
    }
}

pub(crate) fn drop_object_sql(d: Dialect, schema: &str, name: &str, kind: ObjectKind, cascade: bool) -> Result<String, String> {
    check_name("Table", name)?;
    let noun = match (d, kind) {
        (_, ObjectKind::Table) => "TABLE",
        (_, ObjectKind::View) => "VIEW",
        (Dialect::Postgres, ObjectKind::MaterializedView) => "MATERIALIZED VIEW",
        // ClickHouse drops a materialized view with DROP VIEW.
        (Dialect::Clickhouse, ObjectKind::MaterializedView) => "VIEW",
        (_, ObjectKind::MaterializedView) => return Err("This database has no materialized views".into()),
    };
    let tail = if cascade && d.supports_cascade() { " CASCADE" } else { "" };
    Ok(format!("DROP {noun} {}{tail}", qualified(d, schema, name)))
}

/// SQLite has no TRUNCATE, and DuckDB's is an alias for the DELETE that works
/// on every version of it.
pub(crate) fn truncate_table_sql(d: Dialect, schema: &str, table: &str) -> Result<String, String> {
    check_name("Table", table)?;
    let target = qualified(d, schema, table);
    Ok(match d {
        Dialect::Sqlite | Dialect::Duckdb => format!("DELETE FROM {target}"),
        _ => format!("TRUNCATE TABLE {target}"),
    })
}

/// Run one statement for its effect on whatever engine is connected. MySQL
/// gets the bare string, which goes over the text protocol: `sqlx::query`
/// always prepares, and MySQL refuses to prepare `USE`, `CREATE TRIGGER`,
/// `CREATE PROCEDURE` and friends. (`sqlx::raw_sql` would too, but its generic
/// async `execute` trips rustc's Send check inside a Tauri command.)
pub(crate) async fn run_statement(conn: &ActiveConnection, sql: &str) -> Result<(), String> {
    match conn {
        ActiveConnection::Postgres(pool) => sqlx::query(sql).execute(pool).await.map(|_| ()).map_err(|e| e.to_string()),
        ActiveConnection::Mysql(pool) => pool.execute(sql).await.map(|_| ()).map_err(|e| e.to_string()),
        ActiveConnection::Sqlite(pool) => sqlx::query(sql).execute(pool).await.map(|_| ()).map_err(|e| e.to_string()),
        ActiveConnection::D1(cfg) => super::d1::query(cfg, sql, vec![]).await.map(|_| ()),
        ActiveConnection::LibSql(cfg) => super::libsql::query(cfg, sql, vec![]).await.map(|_| ()),
        ActiveConnection::Duckdb(h) => super::duckdb::execute_sql(h, sql).await.map(|_| ()),
        ActiveConnection::Mssql(h) => super::mssql::execute_sql(h, sql).await.map(|_| ()),
        ActiveConnection::Clickhouse(cfg) => super::clickhouse::query(cfg, sql).await.map(|_| ()),
        ActiveConnection::Redis(_) | ActiveConnection::Posthog(_) => Err("This connection does not run DDL statements".into()),
    }
}

pub(crate) async fn drop_object(conn: &ActiveConnection, schema: &str, name: &str, kind: ObjectKind, cascade: bool) -> Result<(), String> {
    let d = Dialect::of(conn).ok_or("Dropping tables is not supported on this connection")?;
    let sql = drop_object_sql(d, schema, name, kind, cascade)?;
    let what = if kind == ObjectKind::Table { "table" } else { "view" };
    run_statement(conn, &sql).await.map_err(|e| format!("Failed to drop {what}: {e}"))
}

pub(crate) async fn truncate_table(conn: &ActiveConnection, schema: &str, table: &str) -> Result<(), String> {
    let d = Dialect::of(conn).ok_or("Truncating tables is not supported on this connection")?;
    let sql = truncate_table_sql(d, schema, table)?;
    run_statement(conn, &sql).await.map_err(|e| format!("Failed to truncate table: {e}"))
}

// ── Copy a database ──────────────────────────────────────────────────────────

/// What a copy did, for the toast: one line, plus whatever it had to leave
/// behind (a view that would not compile, a routine the account cannot read).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneSummary {
    pub message: String,
    pub warnings: Vec<String>,
}

/// Copy `source` to a new database `target` on the same server, structure and
/// rows, without pulling a row into the app.
///
/// Postgres copies through a template. MySQL and ClickHouse have no such
/// statement, so the copy is rebuilt object by object with server-side
/// `INSERT ... SELECT`. SQL Server goes through BACKUP and RESTORE. When a table
/// fails to copy, the half-made database is dropped again: it was created a
/// moment ago by this call, and a copy missing tables looks whole in the list.
pub async fn clone_database(conn: &ActiveConnection, source: &str, target: &str) -> Result<CloneSummary, String> {
    check_name("Database", source)?;
    check_name("Database", target)?;
    if source == target {
        return Err("The copy needs a different name".into());
    }
    match conn {
        ActiveConnection::Postgres(pool) => {
            let sql = format!("CREATE DATABASE {} WITH TEMPLATE {}", quote_double(target), quote_double(source));
            sqlx::query(&sql).execute(pool).await.map_err(|e| e.to_string())?;
            Ok(CloneSummary { message: format!("Copied \"{source}\" to \"{target}\""), warnings: vec![] })
        }
        ActiveConnection::Mysql(pool) => clone_mysql(pool, source, target).await,
        ActiveConnection::Clickhouse(cfg) => clone_clickhouse(cfg, source, target).await,
        ActiveConnection::Mssql(h) => clone_mssql(h, source, target).await,
        _ => Err("This connection has no server-level databases to copy".into()),
    }
}

fn plural(n: u64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Remove a `DEFINER=user@host` clause. The copy is owned by whoever makes it,
/// and naming another account there needs SET_USER_ID (or SUPER), which most
/// accounts do not have.
pub(crate) fn strip_definer(sql: &str) -> String {
    let lower = sql.to_ascii_lowercase();
    let Some(start) = lower.find("definer=") else { return sql.to_string() };
    // `SQL SECURITY DEFINER` has no `=`, so this only ever matches the clause.
    let mut end = start + "definer=".len();
    let bytes = sql.as_bytes();
    let mut quote: Option<u8> = None;
    while end < bytes.len() {
        let b = bytes[end];
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None if b == b'`' || b == b'\'' || b == b'"' => quote = Some(b),
            None if b.is_ascii_whitespace() => break,
            None => {}
        }
        end += 1;
    }
    while end < bytes.len() && bytes[end].is_ascii_whitespace() {
        end += 1;
    }
    format!("{}{}", &sql[..start], &sql[end..])
}

/// Point names qualified with `source` at `target`, leaving string literals
/// alone. MySQL writes every name in backticks; ClickHouse quotes only names
/// that need it, so a bare `source.` counts there too.
pub(crate) fn retarget(sql: &str, source: &str, target: &str, bare: bool) -> String {
    let quoted_from = format!("{}.", quote_backtick(source));
    let quoted_to = format!("{}.", quote_backtick(target));
    let bare_from = format!("{source}.");
    let bare_ok = bare && !source.is_empty() && source.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    let bare_to = if target.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') { format!("{target}.") } else { quoted_to.clone() };

    let mut out = String::with_capacity(sql.len());
    let mut i = 0;
    let bytes = sql.as_bytes();
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\'' {
            // Copy the literal through untouched: '' and \' both stay inside it.
            let start = i;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'\'' {
                    if bytes.get(i + 1) == Some(&b'\'') {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push_str(&sql[start..i.min(bytes.len())]);
            continue;
        }
        if sql[i..].starts_with(&quoted_from) {
            out.push_str(&quoted_to);
            i += quoted_from.len();
            continue;
        }
        if bare_ok && sql[i..].starts_with(&bare_from) {
            let prev = if i == 0 { None } else { Some(bytes[i - 1]) };
            let boundary = !matches!(prev, Some(p) if p.is_ascii_alphanumeric() || p == b'_' || p == b'.' || p == b'`');
            if boundary {
                out.push_str(&bare_to);
                i += bare_from.len();
                continue;
            }
        }
        let ch = sql[i..].chars().next().unwrap_or('\0');
        out.push(ch);
        i += ch.len_utf8().max(1);
    }
    out
}

/// A name in an information_schema/SHOW reply, which MySQL flags as binary.
fn names(rows: &[sqlx::mysql::MySqlRow], idx: usize) -> Vec<String> {
    rows.iter().filter_map(|r| my_text(r, idx)).collect()
}

async fn clone_mysql(pool: &MySqlPool, source: &str, target: &str) -> Result<CloneSummary, String> {
    let mut conn = pool.acquire().await.map_err(|e| format!("Failed to acquire connection: {e}"))?;
    // The copy switches foreign key checks off and makes the target the current
    // database. Neither may leak to the next caller of this pooled connection,
    // so it is closed afterwards instead of handed back.
    conn.close_on_drop();

    let row = sqlx::query(
        "SELECT DEFAULT_CHARACTER_SET_NAME, DEFAULT_COLLATION_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = ?",
    )
    .bind(source)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| format!("Failed to read {source}: {e}"))?
    .ok_or_else(|| format!("Database {source} does not exist"))?;
    let word = |s: Option<String>| s.filter(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    let mut create = format!("CREATE DATABASE {}", quote_backtick(target));
    if let Some(cs) = word(my_text(&row, 0)) {
        create.push_str(&format!(" CHARACTER SET {cs}"));
    }
    if let Some(co) = word(my_text(&row, 1)) {
        create.push_str(&format!(" COLLATE {co}"));
    }
    (&mut *conn).execute(create.as_str()).await.map_err(|e| format!("Could not create {target}: {e}"))?;

    match copy_mysql_contents(&mut *conn, source, target).await {
        Ok(summary) => Ok(summary),
        Err(e) => {
            let _ = (&mut *conn).execute(format!("DROP DATABASE {}", quote_backtick(target)).as_str()).await;
            Err(format!("{e}. The partial copy was removed."))
        }
    }
}

async fn copy_mysql_contents(conn: &mut MySqlConnection, source: &str, target: &str) -> Result<CloneSummary, String> {
    let qs = quote_backtick(source);
    let qt = quote_backtick(target);
    // What mysqldump sets: no foreign key checks, so tables load in any order,
    // and a 0 in an AUTO_INCREMENT column stays 0 instead of taking the next id.
    // Leaving strict mode out also lets legacy zero dates through as they are.
    for stmt in [
        "SET SESSION FOREIGN_KEY_CHECKS = 0".to_string(),
        "SET SESSION sql_mode = 'NO_AUTO_VALUE_ON_ZERO'".to_string(),
        format!("USE {qt}"),
    ] {
        (&mut *conn).execute(stmt.as_str()).await.map_err(|e| format!("Preparing the copy: {e}"))?;
    }

    let tables = sqlx::query(
        "SELECT TABLE_NAME, TABLE_TYPE FROM information_schema.TABLES \
         WHERE TABLE_SCHEMA = ? AND TABLE_TYPE IN ('BASE TABLE', 'SYSTEM VERSIONED', 'SEQUENCE') ORDER BY TABLE_NAME",
    )
    .bind(source)
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| format!("Listing the tables of {source}: {e}"))?;

    // Every structure first: SHOW CREATE TABLE names its foreign key targets
    // unqualified, so run with the target current they point inside the copy.
    let mut with_rows = Vec::new();
    for r in &tables {
        let Some(name) = my_text(r, 0) else { continue };
        let kind = my_text(r, 1).unwrap_or_default();
        let ddl_row = sqlx::query(&format!("SHOW CREATE TABLE {qs}.{}", quote_backtick(&name)))
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| format!("Reading table {name}: {e}"))?;
        let ddl = my_text(&ddl_row, 1).ok_or_else(|| format!("No definition came back for table {name}"))?;
        (&mut *conn).execute(ddl.as_str()).await.map_err(|e| format!("Creating table {name}: {e}"))?;
        if kind != "SEQUENCE" {
            with_rows.push(name);
        }
    }

    // Then the rows, column by column: generated columns compute themselves and
    // refuse a value, and `SELECT *` would skip INVISIBLE columns.
    let mut row_total: u64 = 0;
    for name in &with_rows {
        let cols = sqlx::query(
            "SELECT COLUMN_NAME FROM information_schema.COLUMNS \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? AND COALESCE(EXTRA, '') NOT REGEXP 'VIRTUAL|STORED|PERSISTENT' \
             ORDER BY ORDINAL_POSITION",
        )
        .bind(source)
        .bind(name)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| format!("Reading the columns of {name}: {e}"))?;
        let cols = names(&cols, 0);
        if cols.is_empty() {
            continue;
        }
        let list = cols.iter().map(|c| quote_backtick(c)).collect::<Vec<_>>().join(", ");
        let qn = quote_backtick(name);
        let sql = format!("INSERT INTO {qt}.{qn} ({list}) SELECT {list} FROM {qs}.{qn}");
        let done = (&mut *conn).execute(sql.as_str()).await.map_err(|e| format!("Copying the rows of {name}: {e}"))?;
        row_total += done.rows_affected();
    }

    // Routines, views and triggers are best effort: each one that fails is
    // reported and the copy keeps its tables. Events are left out on purpose, so
    // a copy never starts running the original's scheduled jobs by itself.
    let mut warnings = Vec::new();
    let mut routines = Vec::new();
    let found = sqlx::query("SELECT ROUTINE_NAME, ROUTINE_TYPE FROM information_schema.ROUTINES WHERE ROUTINE_SCHEMA = ? ORDER BY ROUTINE_NAME")
        .bind(source)
        .fetch_all(&mut *conn)
        .await
        .unwrap_or_default();
    for r in &found {
        let (Some(name), Some(kind)) = (my_text(r, 0), my_text(r, 1)) else { continue };
        let noun = if kind.eq_ignore_ascii_case("FUNCTION") { "FUNCTION" } else { "PROCEDURE" };
        match sqlx::query(&format!("SHOW CREATE {noun} {qs}.{}", quote_backtick(&name))).fetch_one(&mut *conn).await {
            Ok(row) => match my_text(&row, 2) {
                Some(ddl) => routines.push((format!("{} {name}", noun.to_lowercase()), retarget(&strip_definer(&ddl), source, target, true))),
                None => warnings.push(format!("{} {name}: this account cannot read its body", noun.to_lowercase())),
            },
            Err(e) => warnings.push(format!("{} {name}: {e}", noun.to_lowercase())),
        }
    }
    let routines_made = create_mysql_in_passes(conn, routines, &mut warnings).await;

    let mut views = Vec::new();
    let found = sqlx::query("SELECT TABLE_NAME FROM information_schema.VIEWS WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME")
        .bind(source)
        .fetch_all(&mut *conn)
        .await
        .unwrap_or_default();
    for name in names(&found, 0) {
        match sqlx::query(&format!("SHOW CREATE VIEW {qs}.{}", quote_backtick(&name))).fetch_one(&mut *conn).await {
            // MySQL stores a view with every column qualified by its database,
            // so the definition is pointed at the copy before it runs there.
            Ok(row) => views.push((format!("view {name}"), retarget(&strip_definer(&my_text(&row, 1).unwrap_or_default()), source, target, false))),
            Err(e) => warnings.push(format!("view {name}: {e}")),
        }
    }
    let views_made = create_mysql_in_passes(conn, views, &mut warnings).await;

    let mut triggers = Vec::new();
    let found = sqlx::query(
        "SELECT TRIGGER_NAME FROM information_schema.TRIGGERS WHERE TRIGGER_SCHEMA = ? \
         ORDER BY EVENT_OBJECT_TABLE, EVENT_MANIPULATION, ACTION_TIMING, ACTION_ORDER",
    )
    .bind(source)
    .fetch_all(&mut *conn)
    .await
    .unwrap_or_default();
    for name in names(&found, 0) {
        match sqlx::query(&format!("SHOW CREATE TRIGGER {qs}.{}", quote_backtick(&name))).fetch_one(&mut *conn).await {
            // A trigger keeps the text it was written with, names bare or quoted.
            Ok(row) => triggers.push((format!("trigger {name}"), retarget(&strip_definer(&my_text(&row, 2).unwrap_or_default()), source, target, true))),
            Err(e) => warnings.push(format!("trigger {name}: {e}")),
        }
    }
    let triggers_made = create_mysql_in_passes(conn, triggers, &mut warnings).await;

    let mut parts = vec![plural(with_rows.len() as u64, "table", "tables"), plural(row_total, "row", "rows")];
    for (n, one, many) in [(views_made, "view", "views"), (routines_made, "routine", "routines"), (triggers_made, "trigger", "triggers")] {
        if n > 0 {
            parts.push(plural(n, one, many));
        }
    }
    Ok(CloneSummary { message: format!("Copied {source} to {target}: {}", parts.join(", ")), warnings })
}

/// Create objects that may depend on each other (a view over a view) by
/// retrying in passes until a pass makes no progress. Returns how many landed.
async fn create_mysql_in_passes(conn: &mut MySqlConnection, mut pending: Vec<(String, String)>, warnings: &mut Vec<String>) -> u64 {
    let mut made = 0;
    loop {
        let mut failed = Vec::new();
        let before = pending.len();
        for (label, sql) in pending {
            match (&mut *conn).execute(sql.as_str()).await {
                Ok(_) => made += 1,
                Err(e) => failed.push((label, sql, e.to_string())),
            }
        }
        if failed.is_empty() {
            return made;
        }
        if failed.len() == before {
            warnings.extend(failed.into_iter().map(|(label, _, e)| format!("{label}: {e}")));
            return made;
        }
        pending = failed.into_iter().map(|(label, sql, _)| (label, sql)).collect();
    }
}

fn ch_lit(s: &str) -> String {
    format!("'{}'", esc_backslash_quote(s))
}

fn cell_str(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn cell_i64(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    }
}

/// ClickHouse engines that hold rows themselves. Everything else (Distributed,
/// Buffer, Merge, URL, the database engines) is a pointer to data elsewhere and
/// is recreated from its definition without copying anything.
fn ch_stores_rows(engine: &str) -> bool {
    engine.ends_with("MergeTree") || matches!(engine, "Log" | "TinyLog" | "StripeLog" | "Memory")
}

/// Engines that consume a queue. A copy would read from the same topic and
/// take messages away from the original, so they are never recreated.
fn ch_is_stream(engine: &str) -> bool {
    matches!(engine, "Kafka" | "RabbitMQ" | "NATS" | "S3Queue" | "AzureQueue" | "FileLog")
}

async fn clone_clickhouse(cfg: &ClickhouseConfig, source: &str, target: &str) -> Result<CloneSummary, String> {
    use super::clickhouse::query;
    let found = query(cfg, &format!("SELECT name FROM system.databases WHERE name = {}", ch_lit(source))).await?;
    if found.rows.is_empty() {
        return Err(format!("Database {source} does not exist"));
    }
    query(cfg, &format!("CREATE DATABASE {}", quote_backtick(target)))
        .await
        .map_err(|e| format!("Could not create {target}: {e}"))?;
    match copy_clickhouse_contents(cfg, source, target).await {
        Ok(summary) => Ok(summary),
        Err(e) => {
            let _ = query(cfg, &format!("DROP DATABASE {}", quote_backtick(target))).await;
            Err(format!("{e}. The partial copy was removed."))
        }
    }
}

async fn copy_clickhouse_contents(cfg: &ClickhouseConfig, source: &str, target: &str) -> Result<CloneSummary, String> {
    use super::clickhouse::query;
    let qs = quote_backtick(source);
    let qt = quote_backtick(target);
    // `.inner` tables belong to materialized views and come back with them.
    let listed = query(
        cfg,
        &format!(
            "SELECT name, engine, create_table_query FROM system.tables \
             WHERE database = {} AND NOT is_temporary AND NOT startsWith(name, '.inner') ORDER BY name",
            ch_lit(source)
        ),
    )
    .await
    .map_err(|e| format!("Listing the tables of {source}: {e}"))?;
    let col = |name: &str| listed.columns.iter().position(|c| c.name == name).unwrap_or(0);
    let (name_i, engine_i, ddl_i) = (col("name"), col("engine"), col("create_table_query"));

    let mut warnings = Vec::new();
    let mut tables = 0u64;
    let mut later = Vec::new();
    for row in &listed.rows {
        let name = cell_str(row.get(name_i));
        let engine = cell_str(row.get(engine_i));
        let ddl = cell_str(row.get(ddl_i));
        let qn = quote_backtick(&name);
        if ch_is_stream(&engine) {
            warnings.push(format!("table {name} ({engine}) was not copied: the copy would read from the same queue"));
        } else if ch_stores_rows(&engine) {
            query(cfg, &format!("CREATE TABLE {qt}.{qn} AS {qs}.{qn}")).await.map_err(|e| format!("Creating table {name}: {e}"))?;
            query(cfg, &format!("INSERT INTO {qt}.{qn} SELECT * FROM {qs}.{qn}"))
                .await
                .map_err(|e| format!("Copying the rows of {name}: {e}"))?;
            tables += 1;
        } else {
            let fill = engine == "MaterializedView" && ddl.contains(" ENGINE = ");
            later.push((name, retarget(&ddl, source, target, true), fill));
        }
    }

    // Views, materialized views, dictionaries and pointer tables, in passes: a
    // view can read another view. A materialized view with its own storage is
    // refilled from the original, since recreating it starts it empty.
    let mut made = 0u64;
    let mut pending = later;
    loop {
        let before = pending.len();
        let mut failed = Vec::new();
        for (name, ddl, fill) in pending {
            match query(cfg, &ddl).await {
                Ok(_) => {
                    made += 1;
                    if fill {
                        let qn = quote_backtick(&name);
                        if let Err(e) = query(cfg, &format!("INSERT INTO {qt}.{qn} SELECT * FROM {qs}.{qn}")).await {
                            warnings.push(format!("materialized view {name} was created empty: {e}"));
                        }
                    }
                }
                Err(e) => failed.push((name, ddl, fill, e)),
            }
        }
        if failed.is_empty() {
            break;
        }
        if failed.len() == before {
            warnings.extend(failed.into_iter().map(|(name, _, _, e)| format!("{name}: {e}")));
            break;
        }
        pending = failed.into_iter().map(|(name, ddl, fill, _)| (name, ddl, fill)).collect();
    }

    let rows = query(cfg, &format!("SELECT sum(total_rows) AS n FROM system.tables WHERE database = {}", ch_lit(target)))
        .await
        .ok()
        .and_then(|r| cell_i64(r.rows.first().and_then(|row| row.first())))
        .unwrap_or(0)
        .max(0) as u64;
    let mut parts = vec![plural(tables, "table", "tables"), plural(rows, "row", "rows")];
    if made > 0 {
        parts.push(plural(made, "view or linked table", "views and linked tables"));
    }
    Ok(CloneSummary { message: format!("Copied {source} to {target}: {}", parts.join(", ")), warnings })
}

fn ms_lit(s: &str) -> String {
    format!("N'{}'", esc_single_quote(s))
}

/// `dir` joined to `file` with whichever separator the server's paths use:
/// SQL Server on Linux reports `/var/opt/mssql/data/`, on Windows `C:\...\`.
pub(crate) fn server_path(dir: &str, file: &str) -> String {
    let sep = if dir.contains('\\') { '\\' } else { '/' };
    if dir.ends_with(sep) { format!("{dir}{file}") } else { format!("{dir}{sep}{file}") }
}

/// The RESTORE ... WITH MOVE list that gives every file of the copy its own
/// name next to the server's default data and log folders.
pub(crate) fn mssql_moves(target: &str, files: &[(String, String)], data_dir: &str, log_dir: &str) -> Vec<String> {
    let stem: String = target.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    let (mut rows, mut logs, mut other) = (0, 0, 0);
    files
        .iter()
        .map(|(logical, kind)| {
            let path = match kind.as_str() {
                "LOG" => {
                    logs += 1;
                    server_path(log_dir, &if logs == 1 { format!("{stem}_log.ldf") } else { format!("{stem}_log{logs}.ldf") })
                }
                "ROWS" => {
                    rows += 1;
                    server_path(data_dir, &if rows == 1 { format!("{stem}.mdf") } else { format!("{stem}_{rows}.ndf") })
                }
                _ => {
                    other += 1;
                    server_path(data_dir, &format!("{stem}_fs{other}"))
                }
            };
            format!("MOVE {} TO {}", ms_lit(logical), ms_lit(&path))
        })
        .collect()
}

async fn clone_mssql(h: &MssqlHandle, source: &str, target: &str) -> Result<CloneSummary, String> {
    use super::mssql::execute_sql;
    let info = execute_sql(
        h,
        &format!(
            "SELECT CAST(SERVERPROPERTY('EngineEdition') AS int) AS edition, \
             CAST(SERVERPROPERTY('InstanceDefaultBackupPath') AS nvarchar(4000)) AS backup_dir, \
             CAST(SERVERPROPERTY('InstanceDefaultDataPath') AS nvarchar(4000)) AS data_dir, \
             CAST(SERVERPROPERTY('InstanceDefaultLogPath') AS nvarchar(4000)) AS log_dir, \
             DB_ID({}) AS db_id",
            ms_lit(source)
        ),
    )
    .await?;
    let row = info.rows.first().cloned().unwrap_or_default();
    if row.get(4).map_or(true, Value::is_null) {
        return Err(format!("Database {source} does not exist"));
    }
    let (qs, qt) = (quote_bracket(source), quote_bracket(target));
    // Azure SQL Database has no BACKUP TO DISK; it copies with one statement and
    // finishes in the background.
    if cell_i64(row.first()) == Some(5) {
        execute_sql(h, &format!("CREATE DATABASE {qt} AS COPY OF {qs}")).await?;
        return Ok(CloneSummary {
            message: format!("Azure is copying {source} to {target}; it shows up once the copy finishes"),
            warnings: vec![],
        });
    }
    let (backup_dir, data_dir, log_dir) = (cell_str(row.get(1)), cell_str(row.get(2)), cell_str(row.get(3)));
    if backup_dir.is_empty() || data_dir.is_empty() || log_dir.is_empty() {
        return Err("This server does not report its default backup and data folders (SQL Server 2019 and later do), so the copy cannot be made here".into());
    }
    let files = execute_sql(h, &format!("SELECT name, type_desc FROM sys.master_files WHERE database_id = DB_ID({}) ORDER BY file_id", ms_lit(source))).await?;
    let files: Vec<(String, String)> = files.rows.iter().map(|r| (cell_str(r.first()), cell_str(r.get(1)))).collect();
    if files.is_empty() {
        return Err(format!("Could not read the files of {source}"));
    }

    let stamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let stem: String = target.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let bak = server_path(&backup_dir, &format!("stroke-copy-{stem}-{stamp}.bak"));
    // COPY_ONLY keeps the original's backup chain as it was.
    execute_sql(h, &format!("BACKUP DATABASE {qs} TO DISK = {} WITH COPY_ONLY, INIT", ms_lit(&bak)))
        .await
        .map_err(|e| format!("Backing up {source} failed: {e}"))?;
    let restored = execute_sql(
        h,
        &format!("RESTORE DATABASE {qt} FROM DISK = {} WITH {}", ms_lit(&bak), mssql_moves(target, &files, &data_dir, &log_dir).join(", ")),
    )
    .await;
    // The backup file only carried the copy across; it goes either way.
    let removed = execute_sql(h, &format!("EXEC master.sys.xp_delete_file 0, {}", ms_lit(&bak))).await;
    restored.map_err(|e| format!("Restoring the copy failed: {e}"))?;
    let mut warnings = Vec::new();
    if removed.is_err() {
        warnings.push(format!("The temporary backup is still on the server at {bak}"));
    }
    Ok(CloneSummary { message: format!("Copied {source} to {target}"), warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_spells_each_dialect() {
        let t = ObjectKind::Table;
        assert_eq!(drop_object_sql(Dialect::Postgres, "public", "users", t, false).unwrap(), r#"DROP TABLE "public"."users""#);
        assert_eq!(drop_object_sql(Dialect::Postgres, "public", "users", t, true).unwrap(), r#"DROP TABLE "public"."users" CASCADE"#);
        assert_eq!(drop_object_sql(Dialect::Mysql, "shop", "order items", t, true).unwrap(), "DROP TABLE `shop`.`order items`");
        assert_eq!(drop_object_sql(Dialect::Mssql, "dbo", "a]b", t, true).unwrap(), "DROP TABLE [dbo].[a]]b]");
        assert_eq!(drop_object_sql(Dialect::Sqlite, "main", "x\"y", t, true).unwrap(), r#"DROP TABLE "x""y""#);
        assert_eq!(drop_object_sql(Dialect::Duckdb, "main", "events", t, true).unwrap(), r#"DROP TABLE "events" CASCADE"#);
        assert_eq!(drop_object_sql(Dialect::Clickhouse, "default", "hits", t, false).unwrap(), "DROP TABLE `default`.`hits`");
    }

    #[test]
    fn drop_names_views_by_their_kind() {
        assert_eq!(drop_object_sql(Dialect::Postgres, "s", "v", ObjectKind::View, true).unwrap(), r#"DROP VIEW "s"."v" CASCADE"#);
        assert_eq!(drop_object_sql(Dialect::Postgres, "s", "m", ObjectKind::MaterializedView, false).unwrap(), r#"DROP MATERIALIZED VIEW "s"."m""#);
        assert_eq!(drop_object_sql(Dialect::Mysql, "shop", "v", ObjectKind::View, false).unwrap(), "DROP VIEW `shop`.`v`");
        assert_eq!(drop_object_sql(Dialect::Clickhouse, "db", "mv", ObjectKind::MaterializedView, false).unwrap(), "DROP VIEW `db`.`mv`");
        assert!(drop_object_sql(Dialect::Mysql, "shop", "m", ObjectKind::MaterializedView, false).is_err());
    }

    #[test]
    fn truncate_falls_back_to_delete_where_there_is_none() {
        assert_eq!(truncate_table_sql(Dialect::Mysql, "shop", "t").unwrap(), "TRUNCATE TABLE `shop`.`t`");
        assert_eq!(truncate_table_sql(Dialect::Mssql, "dbo", "t").unwrap(), "TRUNCATE TABLE [dbo].[t]");
        assert_eq!(truncate_table_sql(Dialect::Sqlite, "main", "t").unwrap(), r#"DELETE FROM "t""#);
        assert_eq!(truncate_table_sql(Dialect::Duckdb, "main", "t").unwrap(), r#"DELETE FROM "t""#);
    }

    #[test]
    fn names_only_need_to_survive_quoting() {
        assert!(drop_object_sql(Dialect::Postgres, "public", "", ObjectKind::Table, false).is_err());
        assert!(drop_object_sql(Dialect::Postgres, "public", "a\0b", ObjectKind::Table, false).is_err());
        assert!(ObjectKind::parse(Some("index")).is_err());
        assert_eq!(ObjectKind::parse(None).unwrap(), ObjectKind::Table);
    }

    #[test]
    fn definer_goes_and_sql_security_stays() {
        assert_eq!(
            strip_definer("CREATE ALGORITHM=UNDEFINED DEFINER=`root`@`%` SQL SECURITY DEFINER VIEW `v` AS select 1"),
            "CREATE ALGORITHM=UNDEFINED SQL SECURITY DEFINER VIEW `v` AS select 1"
        );
        assert_eq!(strip_definer("CREATE DEFINER=`a b`@`localhost` TRIGGER t"), "CREATE TRIGGER t");
        assert_eq!(strip_definer("CREATE VIEW v AS SELECT 1"), "CREATE VIEW v AS SELECT 1");
    }

    #[test]
    fn retarget_moves_qualified_names_and_spares_literals() {
        assert_eq!(
            retarget("select `shop`.`orders`.`id` from `shop`.`orders` where note = '`shop`.x'", "shop", "shop_copy", false),
            "select `shop_copy`.`orders`.`id` from `shop_copy`.`orders` where note = '`shop`.x'"
        );
        // ClickHouse writes plain names bare; `myshop.` is a different database.
        assert_eq!(
            retarget("CREATE VIEW shop.v AS SELECT * FROM shop.t JOIN myshop.u USING id WHERE s = 'it''s shop.t'", "shop", "copy", true),
            "CREATE VIEW copy.v AS SELECT * FROM copy.t JOIN myshop.u USING id WHERE s = 'it''s shop.t'"
        );
        assert_eq!(retarget("FROM shop.t", "shop", "my-copy", true), "FROM `my-copy`.t");
    }

    #[test]
    fn restore_moves_every_file_into_place() {
        let files = vec![
            ("shop".to_string(), "ROWS".to_string()),
            ("shop_log".to_string(), "LOG".to_string()),
            ("shop_2".to_string(), "ROWS".to_string()),
        ];
        assert_eq!(
            mssql_moves("shop copy", &files, "/var/opt/mssql/data/", "/var/opt/mssql/data"),
            vec![
                "MOVE N'shop' TO N'/var/opt/mssql/data/shop_copy.mdf'",
                "MOVE N'shop_log' TO N'/var/opt/mssql/data/shop_copy_log.ldf'",
                "MOVE N'shop_2' TO N'/var/opt/mssql/data/shop_copy_2.ndf'",
            ]
        );
        assert_eq!(server_path(r"C:\Data", "x.bak"), r"C:\Data\x.bak");
    }
}

/// Against the dialect containers (`scripts/dialects.sh up`), on scratch
/// databases this test creates and removes itself:
/// `cargo test --lib admin_live -- --ignored --test-threads=1`
#[cfg(test)]
mod admin_live {
    use super::*;

    async fn mysql_case(url: &str) {
        let pool = MySqlPool::connect(url).await.expect("mysql fixture is running");
        let conn = ActiveConnection::Mysql(pool.clone());
        for db in ["zz_stroke_clone_src", "zz_stroke_clone_dst"] {
            let _ = run_statement(&conn, &format!("DROP DATABASE IF EXISTS `{db}`")).await;
        }
        let setup = [
            "CREATE DATABASE zz_stroke_clone_src",
            "CREATE TABLE zz_stroke_clone_src.customers (id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(40), upper_name VARCHAR(40) AS (UPPER(name)) VIRTUAL)",
            "CREATE TABLE zz_stroke_clone_src.orders (id INT PRIMARY KEY, customer_id INT, FOREIGN KEY (customer_id) REFERENCES zz_stroke_clone_src.customers(id))",
            // One call, one session: the id 0 only stays 0 with this mode set.
            "SET SESSION sql_mode = 'NO_AUTO_VALUE_ON_ZERO'; INSERT INTO zz_stroke_clone_src.customers (id, name) VALUES (0, 'zero'), (1, 'ada'), (2, 'alan')",
            "INSERT INTO zz_stroke_clone_src.orders VALUES (10, 1), (11, 2)",
            "CREATE VIEW zz_stroke_clone_src.big_orders AS SELECT o.id, c.name FROM zz_stroke_clone_src.orders o JOIN zz_stroke_clone_src.customers c ON c.id = o.customer_id",
            "CREATE VIEW zz_stroke_clone_src.a_view_of_a_view AS SELECT name FROM zz_stroke_clone_src.big_orders",
        ];
        for s in setup {
            run_statement(&conn, s).await.unwrap_or_else(|e| panic!("{s}: {e}"));
        }
        run_statement(&conn, "CREATE TRIGGER zz_stroke_clone_src.orders_bi BEFORE INSERT ON zz_stroke_clone_src.orders FOR EACH ROW SET NEW.id = NEW.id")
            .await
            .unwrap();

        let summary = clone_database(&conn, "zz_stroke_clone_src", "zz_stroke_clone_dst").await.expect("clone");
        assert!(summary.warnings.is_empty(), "{:?}", summary.warnings);
        assert!(summary.message.contains("2 tables, 5 rows, 2 views"), "{}", summary.message);

        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM zz_stroke_clone_dst.customers WHERE id = 0 AND upper_name = 'ZERO'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(n, 1, "id 0 and the generated column survive the copy");
        let v: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM zz_stroke_clone_dst.a_view_of_a_view").fetch_one(&pool).await.unwrap();
        assert_eq!(v, 2);
        let def: String = sqlx::query_scalar("SELECT CAST(VIEW_DEFINITION AS CHAR) FROM information_schema.VIEWS WHERE TABLE_SCHEMA = 'zz_stroke_clone_dst' AND TABLE_NAME = 'big_orders'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(!def.contains("zz_stroke_clone_src"), "view still reads the source: {def}");
        let fk: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.REFERENTIAL_CONSTRAINTS WHERE CONSTRAINT_SCHEMA = 'zz_stroke_clone_dst' AND UNIQUE_CONSTRAINT_SCHEMA = 'zz_stroke_clone_dst'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(fk, 1, "the foreign key points inside the copy");
        let trg: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.TRIGGERS WHERE TRIGGER_SCHEMA = 'zz_stroke_clone_dst'").fetch_one(&pool).await.unwrap();
        assert_eq!(trg, 1);

        assert!(clone_database(&conn, "zz_stroke_clone_src", "zz_stroke_clone_dst").await.is_err(), "an existing name is refused");

        drop_object(&conn, "zz_stroke_clone_dst", "a_view_of_a_view", ObjectKind::View, false).await.expect("drop view");
        truncate_table(&conn, "zz_stroke_clone_dst", "orders").await.expect("truncate");
        drop_object(&conn, "zz_stroke_clone_dst", "orders", ObjectKind::Table, true).await.expect("drop table");

        for db in ["zz_stroke_clone_src", "zz_stroke_clone_dst"] {
            run_statement(&conn, &format!("DROP DATABASE `{db}`")).await.expect("drop database");
        }
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_copies_and_drops() {
        mysql_case("mysql://root:stroke@127.0.0.1:53306/shop").await;
    }

    #[tokio::test]
    #[ignore]
    async fn mariadb_copies_and_drops() {
        mysql_case("mysql://root:stroke@127.0.0.1:53307/mysql").await;
    }

    /// `STROKE_CH_PORT` picks the HTTP port, 58123 (the dialects stack) by default.
    #[tokio::test]
    #[ignore]
    async fn clickhouse_copies_and_drops() {
        let port = std::env::var("STROKE_CH_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(58123);
        let cfg = ClickhouseConfig {
            name: "test".into(),
            host: "127.0.0.1".into(),
            port,
            database: "default".into(),
            user: "default".into(),
            password: "stroke".into(),
            secure: false,
        };
        let conn = ActiveConnection::Clickhouse(cfg.clone());
        for db in ["zz_stroke_clone_src", "zz_stroke_clone_dst"] {
            run_statement(&conn, &format!("DROP DATABASE IF EXISTS `{db}`")).await.unwrap();
        }
        for s in [
            "CREATE DATABASE zz_stroke_clone_src",
            "CREATE TABLE zz_stroke_clone_src.hits (id UInt64, url String) ENGINE = MergeTree ORDER BY id",
            "CREATE TABLE zz_stroke_clone_src.daily (url String, n UInt64) ENGINE = SummingMergeTree ORDER BY url",
            "INSERT INTO zz_stroke_clone_src.hits VALUES (1, 'a'), (2, 'b'), (3, 'a')",
            "INSERT INTO zz_stroke_clone_src.daily VALUES ('a', 2), ('b', 1)",
            "CREATE VIEW zz_stroke_clone_src.a_urls AS SELECT url FROM zz_stroke_clone_src.hits WHERE url = 'a'",
            "CREATE MATERIALIZED VIEW zz_stroke_clone_src.hits_to_daily TO zz_stroke_clone_src.daily AS SELECT url, count() AS n FROM zz_stroke_clone_src.hits GROUP BY url",
        ] {
            run_statement(&conn, s).await.unwrap_or_else(|e| panic!("{s}: {e}"));
        }

        let summary = clone_database(&conn, "zz_stroke_clone_src", "zz_stroke_clone_dst").await.expect("clone");
        assert!(summary.warnings.is_empty(), "{:?}", summary.warnings);
        assert!(summary.message.contains("2 tables, 5 rows"), "{}", summary.message);
        let views = super::super::clickhouse::query(
            &cfg,
            "SELECT create_table_query FROM system.tables WHERE database = 'zz_stroke_clone_dst' AND engine IN ('View', 'MaterializedView')",
        )
        .await
        .unwrap();
        assert_eq!(views.rows.len(), 2);
        for row in &views.rows {
            let ddl = cell_str(row.first());
            assert!(!ddl.contains("zz_stroke_clone_src"), "copy still reads the source: {ddl}");
        }

        drop_object(&conn, "zz_stroke_clone_dst", "a_urls", ObjectKind::View, false).await.expect("drop view");
        truncate_table(&conn, "zz_stroke_clone_dst", "hits").await.expect("truncate");
        drop_object(&conn, "zz_stroke_clone_dst", "hits", ObjectKind::Table, true).await.expect("drop table");
        for db in ["zz_stroke_clone_src", "zz_stroke_clone_dst"] {
            run_statement(&conn, &format!("DROP DATABASE `{db}`")).await.expect("drop database");
        }
    }

    #[tokio::test]
    #[ignore]
    async fn mssql_copies_and_drops() {
        let cfg = super::super::connection::MssqlConfig {
            name: "test".into(),
            host: "127.0.0.1".into(),
            port: 51433,
            database: "master".into(),
            user: "sa".into(),
            password: "Stroke!passw0rd".into(),
            encrypt: false,
            trust_cert: true,
        };
        let client = super::super::mssql::connect(&cfg).await.expect("stroke-test-mssql is running");
        let h: MssqlHandle = std::sync::Arc::new(tokio::sync::Mutex::new(client));
        let conn = ActiveConnection::Mssql(h.clone());
        for db in ["zz_stroke_clone_src", "zz_stroke_clone_dst"] {
            let _ = run_statement(&conn, &format!("IF DB_ID(N'{db}') IS NOT NULL DROP DATABASE [{db}]")).await;
        }
        for s in [
            "CREATE DATABASE zz_stroke_clone_src",
            "CREATE TABLE zz_stroke_clone_src.dbo.items (id INT IDENTITY PRIMARY KEY, name NVARCHAR(20))",
            "INSERT INTO zz_stroke_clone_src.dbo.items (name) VALUES (N'a'), (N'b')",
        ] {
            run_statement(&conn, s).await.unwrap_or_else(|e| panic!("{s}: {e}"));
        }

        let summary = clone_database(&conn, "zz_stroke_clone_src", "zz_stroke_clone_dst").await.expect("clone");
        assert!(summary.warnings.is_empty(), "{:?}", summary.warnings);
        let n = super::super::mssql::execute_sql(&h, "SELECT COUNT(*) AS n FROM zz_stroke_clone_dst.dbo.items").await.unwrap();
        assert_eq!(cell_i64(n.rows.first().and_then(|r| r.first())), Some(2));

        truncate_table(&conn, "dbo", "items").await.expect_err("no items table in master");
        // Dropping a table in the copy, then the copy itself with other sessions forced off.
        run_statement(&conn, "DROP TABLE zz_stroke_clone_dst.dbo.items").await.expect("drop table");
        run_statement(&conn, "ALTER DATABASE [zz_stroke_clone_dst] SET SINGLE_USER WITH ROLLBACK IMMEDIATE; DROP DATABASE [zz_stroke_clone_dst]")
            .await
            .expect("forced drop");
        run_statement(&conn, "DROP DATABASE [zz_stroke_clone_src]").await.expect("drop database");
    }
}
