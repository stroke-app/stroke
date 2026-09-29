use crate::db::connection::ActiveConnection;
use crate::db::mysql::{my_int, my_text, my_text_named};
use crate::mcp::ConnMeta;
use futures::TryStreamExt;
use serde_json::{json, Value};
use sqlx::{Column, Row, TypeInfo};

// ── Tool manifest ─────────────────────────────────────────────────────────────

pub fn tool_list() -> Value {
    json!([
        {
            "name": "list_databases",
            "description": "List all saved database connections and show which one is currently active. Call this at the start of a session when the user hasn't specified a database, or when you need to know what databases are available. If multiple databases exist, ask the user which one to use and instruct them to switch in Stroke.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "current_database",
            "description": "Show which database is currently connected and active. All data tools (execute_sql, list_tables, etc.) operate on this database.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "execute_sql",
            "description": "Execute a SQL query and return the results as JSON. Use this for any data retrieval or DML.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "sql": { "type": "string", "description": "SQL query to execute" },
                    "max_rows": { "type": "number", "description": "Maximum rows to return (default 200, max 1000)" }
                },
                "required": ["sql"]
            }
        },
        {
            "name": "list_tables",
            "description": "List all tables in the database with row counts and types.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "schema": { "type": "string", "description": "Schema name (default: public for Postgres)" }
                }
            }
        },
        {
            "name": "describe_table",
            "description": "Show full column definitions, primary key, foreign keys and indexes for a table.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "table": { "type": "string", "description": "Table name" },
                    "schema": { "type": "string", "description": "Schema name (default: public)" }
                },
                "required": ["table"]
            }
        },
        {
            "name": "check_migrations",
            "description": "Detect migration tables (Prisma, Drizzle, Rails, Flyway, Knex, Alembic, Goose) and report their current state. Essential for checking what migrations have been applied.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "schema": { "type": "string", "description": "Schema to search (default: public)" }
                }
            }
        },
        {
            "name": "explain_query",
            "description": "Analyze a SQL query's execution plan. Returns the query plan with estimated costs, scan types, and join strategies. Essential for diagnosing slow queries and understanding index usage.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "sql": { "type": "string", "description": "SQL query to explain (does NOT execute the query)" }
                },
                "required": ["sql"]
            }
        },
        {
            "name": "get_database_stats",
            "description": "Get database health metrics: table sizes, row counts, dead tuple counts (bloat), cache hit rates, and active connections. Essential for performance analysis and capacity planning.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "schema": { "type": "string", "description": "Schema name (default: public)" }
                }
            }
        },
        {
            "name": "export_query",
            "description": "Run a read-only SQL query and return its rows formatted as JSON, CSV, or a Markdown table. Compose any filtering/sorting with a SQL WHERE/ORDER BY clause. If an absolute 'path' is given, the formatted result is written to that file and the path is returned (for exporting/downloading data); otherwise the formatted content is returned inline.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "sql": { "type": "string", "description": "Read-only SQL query (SELECT/WITH). Add WHERE/ORDER BY/LIMIT to filter and sort." },
                    "format": { "type": "string", "enum": ["json", "csv", "markdown"], "description": "Output format (default: json)" },
                    "path": { "type": "string", "description": "Absolute file path to write the result to. If omitted, the formatted content is returned inline." },
                    "max_rows": { "type": "number", "description": "Maximum rows to include (default 5000, max 50000)" }
                },
                "required": ["sql"]
            }
        }
    ])
}

// ── Tool dispatchers ──────────────────────────────────────────────────────────

/// Handle tools that don't require an active DB connection.
pub fn call_meta_tool(
    name: &str,
    connections: &[ConnMeta],
    active_id: Option<&str>,
) -> Result<String, String> {
    match name {
        "list_databases" => {
            let list: Vec<Value> = connections
                .iter()
                .map(|c| {
                    let mut v = json!({
                        "id": c.id,
                        "name": c.name,
                        "type": c.conn_type,
                    });
                    if let Some(h) = &c.host { v["host"] = json!(h); }
                    if let Some(p) = c.port    { v["port"] = json!(p); }
                    if let Some(d) = &c.database { v["database"] = json!(d); }
                    if let Some(f) = &c.file_path { v["file_path"] = json!(f); }
                    if active_id.map(|id| id == c.id).unwrap_or(false) {
                        v["active"] = json!(true);
                    }
                    v
                })
                .collect();

            let active_name = active_id
                .and_then(|id| connections.iter().find(|c| c.id == id))
                .map(|c| c.name.as_str());

            Ok(json!({
                "databases": list,
                "total": list.len(),
                "active_id": active_id,
                "active_name": active_name,
                "note": if connections.len() > 1 {
                    "Multiple databases available. To switch, open Stroke and connect to the desired database. Then retry your request."
                } else if connections.is_empty() {
                    "No saved connections found. Open Stroke and add a connection first."
                } else {
                    "One database configured."
                }
            }).to_string())
        }

        "current_database" => {
            let active = active_id.and_then(|id| connections.iter().find(|c| c.id == id));
            match active {
                Some(c) => {
                    let mut v = json!({
                        "connected": true,
                        "name": c.name,
                        "type": c.conn_type,
                    });
                    if let Some(h) = &c.host { v["host"] = json!(h); }
                    if let Some(p) = c.port    { v["port"] = json!(p); }
                    if let Some(d) = &c.database { v["database"] = json!(d); }
                    if let Some(f) = &c.file_path { v["file_path"] = json!(f); }
                    Ok(v.to_string())
                }
                None => Ok(json!({
                    "connected": false,
                    "note": "No database is currently connected. Open Stroke and connect to a database, then retry."
                }).to_string()),
            }
        }

        _ => Err(format!("Unknown meta tool: {name}")),
    }
}

pub async fn call_tool(
    conn: &ActiveConnection,
    name: &str,
    args: &Value,
) -> Result<String, String> {
    match name {
        "execute_sql" => {
            let sql = args["sql"].as_str().ok_or("Missing sql argument")?;
            let max_rows = args["max_rows"].as_u64().unwrap_or(200).min(1000) as usize;
            execute_sql(conn, sql, max_rows).await
        }
        "list_tables" => {
            let schema = args["schema"].as_str().unwrap_or("public");
            list_tables(conn, schema).await
        }
        "describe_table" => {
            let table = args["table"].as_str().ok_or("Missing table argument")?;
            let schema = args["schema"].as_str().unwrap_or("public");
            describe_table(conn, schema, table).await
        }
        "check_migrations" => {
            let schema = args["schema"].as_str().unwrap_or("public");
            check_migrations(conn, schema).await
        }
        "explain_query" => {
            let sql = args["sql"].as_str().ok_or("Missing sql argument")?;
            explain_query(conn, sql).await
        }
        "get_database_stats" => {
            let schema = args["schema"].as_str().unwrap_or("public");
            get_database_stats(conn, schema).await
        }
        "export_query" => {
            let sql = args["sql"].as_str().ok_or("Missing sql argument")?;
            if !is_read_only_sql(sql) {
                return Err("export_query only supports read-only queries (SELECT, WITH, …).".to_string());
            }
            let format = args["format"].as_str().unwrap_or("json");
            let path = args["path"].as_str();
            let max_rows = args["max_rows"].as_u64().unwrap_or(5000).min(50000) as usize;
            export_query(conn, sql, format, path, max_rows).await
        }
        _ => Err(format!("Unknown tool: {name}")),
    }
}

// ── Read-only guard ───────────────────────────────────────────────────────────

/// Write keywords that make a statement mutating. Used to reject writes wrapped
/// in a leading CTE (`WITH ... DELETE/INSERT/UPDATE ...`), which the first-word
/// check alone can't catch.
const WRITE_KEYWORDS: &[&str] = &[
    "INSERT", "UPDATE", "DELETE", "DROP", "ALTER", "CREATE", "TRUNCATE",
    "REPLACE", "MERGE", "GRANT", "REVOKE", "ATTACH", "DETACH", "REINDEX", "VACUUM",
];

/// Returns true if the SQL is read-only (SELECT, WITH, EXPLAIN, SHOW, DESCRIBE,
/// PRAGMA) - used to enforce MCP read-only mode.
pub fn is_read_only_sql(sql: &str) -> bool {
    // Strip leading whitespace and block/line comments, then check the first keyword.
    let s = sql.trim();
    // Skip leading block comments (/* ... */) and line comments (--)
    let stripped = strip_sql_comments(s).trim();
    let first_word = stripped
        .split(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();

    // A `WITH` (CTE) prefix can hide a write in its final statement
    // (`WITH x AS (...) DELETE/INSERT/UPDATE ...`) or in a data-modifying CTE.
    // Scan the whole statement for write keywords and reject if any appear.
    if first_word == "WITH" {
        let upper = stripped.to_ascii_uppercase();
        for kw in WRITE_KEYWORDS {
            if upper
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .any(|tok| tok == *kw)
            {
                return false;
            }
        }
        return true;
    }

    matches!(
        first_word.as_str(),
        "SELECT" | "EXPLAIN" | "SHOW" | "DESCRIBE" | "DESC" | "PRAGMA"
    )
}

fn strip_sql_comments(s: &str) -> &str {
    let s = s.trim_start();
    if s.starts_with("--") {
        // skip to end of line
        s.find('\n').map_or("", |i| strip_sql_comments(&s[i + 1..]))
    } else if s.starts_with("/*") {
        s.find("*/").map_or("", |i| strip_sql_comments(&s[i + 2..]))
    } else {
        s
    }
}

// ── export_query ────────────────────────────────────────────────────────────

/// Run a read-only query and return its rows formatted as json/csv/markdown,
/// optionally writing the formatted result to a file. Reuses execute_sql so
/// every engine (Postgres, SQLite, MySQL, D1, LibSQL, ClickHouse, DuckDB,
/// MSSQL) is supported without duplicating per-engine logic.
async fn export_query(
    conn: &ActiveConnection,
    sql: &str,
    format: &str,
    path: Option<&str>,
    max_rows: usize,
) -> Result<String, String> {
    let raw = execute_sql(conn, sql, max_rows).await?;
    let parsed: Value =
        serde_json::from_str(&raw).map_err(|e| format!("Could not parse result: {e}"))?;
    let columns: Vec<String> = parsed["columns"]
        .as_array()
        .map(|a| a.iter().map(|c| c.as_str().unwrap_or("").to_string()).collect())
        .unwrap_or_default();
    let empty: Vec<Value> = Vec::new();
    let rows = parsed["rows"].as_array().unwrap_or(&empty);
    let row_count = parsed["row_count"].as_u64().unwrap_or(rows.len() as u64);
    let truncated = parsed["truncated"].as_bool().unwrap_or(false);

    let fmt = format.to_ascii_lowercase();
    let content = match fmt.as_str() {
        "csv" => rows_to_csv(&columns, rows),
        "markdown" | "md" => rows_to_markdown(&columns, rows),
        "json" => rows_to_json_objects(&columns, rows),
        other => {
            return Err(format!("Unknown format '{other}'. Use 'json', 'csv', or 'markdown'."))
        }
    };

    if let Some(p) = path {
        tokio::fs::write(p, &content)
            .await
            .map_err(|e| format!("Could not write file '{p}': {e}"))?;
        Ok(json!({
            "path": p,
            "bytes": content.len(),
            "row_count": row_count,
            "truncated": truncated,
            "format": fmt,
        })
        .to_string())
    } else {
        Ok(json!({
            "format": fmt,
            "row_count": row_count,
            "truncated": truncated,
            "content": content,
        })
        .to_string())
    }
}

/// Stringify a single JSON cell for CSV / Markdown output.
fn cell_to_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        // arrays / objects → compact JSON
        other => other.to_string(),
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(|c| c == ',' || c == '"' || c == '\n' || c == '\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn rows_to_csv(columns: &[String], rows: &[Value]) -> String {
    let mut out = columns.iter().map(|c| csv_escape(c)).collect::<Vec<_>>().join(",");
    for row in rows {
        let cells = row.as_array().cloned().unwrap_or_default();
        let line = (0..columns.len())
            .map(|i| csv_escape(&cell_to_text(cells.get(i).unwrap_or(&Value::Null))))
            .collect::<Vec<_>>()
            .join(",");
        out.push('\n');
        out.push_str(&line);
    }
    out
}

fn md_escape(s: &str) -> String {
    s.replace('|', "\\|").replace(['\n', '\r'], " ")
}

fn rows_to_markdown(columns: &[String], rows: &[Value]) -> String {
    let mut out = format!(
        "| {} |\n| {} |\n",
        columns.iter().map(|c| md_escape(c)).collect::<Vec<_>>().join(" | "),
        columns.iter().map(|_| "---").collect::<Vec<_>>().join(" | ")
    );
    for row in rows {
        let cells = row.as_array().cloned().unwrap_or_default();
        let line = (0..columns.len())
            .map(|i| md_escape(&cell_to_text(cells.get(i).unwrap_or(&Value::Null))))
            .collect::<Vec<_>>()
            .join(" | ");
        out.push_str(&format!("| {line} |\n"));
    }
    out
}

fn rows_to_json_objects(columns: &[String], rows: &[Value]) -> String {
    let records: Vec<Value> = rows
        .iter()
        .map(|row| {
            let cells = row.as_array().cloned().unwrap_or_default();
            let mut obj = serde_json::Map::new();
            for (i, col) in columns.iter().enumerate() {
                obj.insert(col.clone(), cells.get(i).cloned().unwrap_or(Value::Null));
            }
            Value::Object(obj)
        })
        .collect();
    serde_json::to_string_pretty(&records).unwrap_or_else(|_| "[]".to_string())
}

// ── execute_sql ───────────────────────────────────────────────────────────────

/// Serialise a fully-materialised SqlResult, truncated to `max_rows`.
/// `row_count` is the pre-truncation total, matching the streaming engines.
fn truncated_result_json(result: &crate::db::SqlResult, max_rows: usize) -> String {
    let truncated = result.rows.len() > max_rows;
    let rows: Vec<_> = result.rows.iter().take(max_rows).collect();
    json!({
        "columns": result.columns.iter().map(|c| &c.name).collect::<Vec<_>>(),
        "rows": rows,
        "row_count": result.rows.len(),
        "truncated": truncated
    })
    .to_string()
}

async fn execute_sql(
    conn: &ActiveConnection,
    sql: &str,
    max_rows: usize,
) -> Result<String, String> {
    match conn {
        ActiveConnection::Postgres(pool) => execute_sql_pg(pool, sql, max_rows).await,
        ActiveConnection::Sqlite(pool) => execute_sql_sqlite(pool, sql, max_rows).await,
        ActiveConnection::D1(cfg) => execute_sql_d1(cfg, sql, max_rows).await,
        ActiveConnection::LibSql(cfg) => {
            let result = crate::db::libsql::query(cfg, sql, vec![]).await?;
            Ok(truncated_result_json(&result, max_rows))
        }
        ActiveConnection::Mysql(pool) => execute_sql_mysql(pool, sql, max_rows).await,
        ActiveConnection::Clickhouse(cfg) => {
            let result = crate::db::clickhouse::query(cfg, sql).await?;
            Ok(truncated_result_json(&result, max_rows))
        }
        ActiveConnection::Posthog(cfg) => {
            let result = crate::db::posthog::query(cfg, sql).await?;
            Ok(truncated_result_json(&result, max_rows))
        }
        ActiveConnection::Redis(cfg) => {
            let result = crate::db::redis::query(cfg, sql).await?;
            Ok(truncated_result_json(&result, max_rows))
        }
        ActiveConnection::Duckdb(h) => {
            let result = crate::db::duckdb::execute_sql(h, sql).await?;
            Ok(truncated_result_json(&result, max_rows))
        }
        ActiveConnection::Mssql(h) => {
            let result = crate::db::mssql::execute_sql(h, sql).await?;
            Ok(truncated_result_json(&result, max_rows))
        }
    }
}

async fn execute_sql_mysql(
    pool: &sqlx::MySqlPool,
    sql: &str,
    max_rows: usize,
) -> Result<String, String> {
    // Stream rows instead of fetch_all - caps memory at max_rows regardless of result size.
    let mut stream = sqlx::query(sql).fetch(pool);
    let mut columns: Vec<String> = Vec::new();
    let mut data: Vec<Vec<serde_json::Value>> = Vec::with_capacity(max_rows.min(256));
    let mut total = 0usize;

    while let Some(row) = stream.try_next().await.map_err(|e| format!("SQL error: {e}"))? {
        if total == 0 {
            columns = row.columns().iter().map(|c| c.name().to_string()).collect();
        }
        total += 1;
        if data.len() < max_rows {
            data.push((0..columns.len()).map(|i| crate::db::mysql::cell_to_json(&row, i)).collect());
        }
    }

    Ok(json!({
        "columns": columns,
        "rows": data,
        "row_count": total,
        "truncated": total > max_rows
    }).to_string())
}

async fn execute_sql_pg(
    pool: &sqlx::PgPool,
    sql: &str,
    max_rows: usize,
) -> Result<String, String> {
    // Stream rows - never materialises more than max_rows rows in memory.
    let mut stream = sqlx::query(sql).fetch(pool);
    let mut columns: Vec<String> = Vec::new();
    let mut data: Vec<Vec<Value>> = Vec::with_capacity(max_rows.min(256));
    let mut total = 0usize;

    while let Some(row) = stream.try_next().await.map_err(|e| format!("SQL error: {e}"))? {
        if total == 0 {
            columns = row.columns().iter().map(|c| c.name().to_string()).collect();
        }
        total += 1;
        if data.len() < max_rows {
            data.push((0..columns.len()).map(|i| pg_cell(&row, i)).collect());
        }
    }

    Ok(json!({
        "columns": columns,
        "rows": data,
        "row_count": total,
        "truncated": total > max_rows
    })
    .to_string())
}

async fn execute_sql_sqlite(
    pool: &sqlx::SqlitePool,
    sql: &str,
    max_rows: usize,
) -> Result<String, String> {
    // Stream rows - never materialises more than max_rows rows in memory.
    let mut stream = sqlx::query(sql).fetch(pool);
    let mut columns: Vec<String> = Vec::new();
    let mut data: Vec<Vec<Value>> = Vec::with_capacity(max_rows.min(256));
    let mut total = 0usize;

    while let Some(row) = stream.try_next().await.map_err(|e| format!("SQL error: {e}"))? {
        if total == 0 {
            columns = row.columns().iter().map(|c| c.name().to_string()).collect();
        }
        total += 1;
        if data.len() < max_rows {
            data.push((0..columns.len()).map(|i| crate::db::sqlite::cell_to_json(&row, i)).collect());
        }
    }

    Ok(json!({
        "columns": columns,
        "rows": data,
        "row_count": total,
        "truncated": total > max_rows
    })
    .to_string())
}

async fn execute_sql_d1(
    cfg: &crate::db::D1Config,
    sql: &str,
    max_rows: usize,
) -> Result<String, String> {
    let result = crate::db::d1::query(cfg, sql, vec![]).await?;
    Ok(truncated_result_json(&result, max_rows))
}

// ── list_tables ───────────────────────────────────────────────────────────────

async fn list_tables(conn: &ActiveConnection, schema: &str) -> Result<String, String> {
    match conn {
        ActiveConnection::Postgres(pool) => list_tables_pg(pool, schema).await,
        ActiveConnection::Sqlite(pool) => list_tables_sqlite(pool).await,
        ActiveConnection::D1(cfg) => list_tables_d1(cfg).await,
        ActiveConnection::LibSql(cfg) => {
            let r = crate::db::libsql::query(cfg, "SELECT name,type FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name", vec![]).await?;
            Ok(json!({"tables":r.rows.iter().filter_map(|row|row.first()?.as_str().map(|n|n.to_string())).collect::<Vec<_>>()}).to_string())
        }
        ActiveConnection::Mysql(pool) => list_tables_mysql(pool, schema).await,
        ActiveConnection::Clickhouse(cfg) => {
            let tables = crate::db::clickhouse::list_tables(cfg, schema).await?;
            Ok(json!({"tables":tables.iter().map(|t|&t.name).collect::<Vec<_>>()}).to_string())
        }
        ActiveConnection::Posthog(cfg) => {
            let tables = crate::db::posthog::list_tables(cfg).await?;
            Ok(json!({"tables":tables.iter().map(|t|&t.name).collect::<Vec<_>>()}).to_string())
        }
        ActiveConnection::Redis(cfg) => {
            let tables = crate::db::redis::list_tables(cfg).await?;
            Ok(json!({"tables":tables.iter().map(|t|&t.name).collect::<Vec<_>>()}).to_string())
        }
        ActiveConnection::Duckdb(h) => {
            let tables = crate::db::duckdb::list_tables(h).await?;
            Ok(json!({"tables":tables.iter().map(|t|&t.name).collect::<Vec<_>>()}).to_string())
        }
        ActiveConnection::Mssql(h) => {
            let tables = crate::db::mssql::list_tables(h, schema).await?;
            Ok(json!({"tables":tables.iter().map(|t|&t.name).collect::<Vec<_>>()}).to_string())
        }
    }
}

async fn list_tables_mysql(pool: &sqlx::MySqlPool, schema: &str) -> Result<String, String> {
    let rows = sqlx::query(
        "SELECT TABLE_NAME, TABLE_TYPE, COALESCE(TABLE_ROWS, 0) \
         FROM information_schema.TABLES WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME",
    )
    .bind(schema)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list tables: {e}"))?;

    let tables: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            // information_schema columns are binary-flagged, so a String
            // decode is refused - see db::mysql::my_text. These read "" before.
            let name = my_text(r, 0).unwrap_or_default();
            let kind = my_text(r, 1).unwrap_or_default();
            let row_count = my_int(r, 2).unwrap_or(0);
            json!({ "name": name, "type": kind, "row_estimate": row_count })
        })
        .collect();
    Ok(json!({ "schema": schema, "tables": tables }).to_string())
}

async fn list_tables_pg(pool: &sqlx::PgPool, schema: &str) -> Result<String, String> {
    let rows = sqlx::query(
        r#"
        SELECT t.table_name, t.table_type,
               s.n_live_tup AS row_estimate
        FROM information_schema.tables t
        LEFT JOIN pg_stat_user_tables s
          ON s.schemaname = t.table_schema AND s.relname = t.table_name
        WHERE t.table_schema = $1
        ORDER BY t.table_name
        "#,
    )
    .bind(schema)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list tables: {e}"))?;

    let tables: Vec<Value> = rows
        .iter()
        .map(|r| {
            let name: String = r.try_get(0).unwrap_or_default();
            let kind: String = r.try_get(1).unwrap_or_default();
            let row_count: i64 = r.try_get(2).unwrap_or(-1);
            json!({ "name": name, "type": kind, "row_estimate": row_count })
        })
        .collect();

    Ok(json!({ "schema": schema, "tables": tables }).to_string())
}

async fn list_tables_sqlite(pool: &sqlx::SqlitePool) -> Result<String, String> {
    let rows = sqlx::query(
        "SELECT name, type FROM sqlite_master WHERE type IN ('table','view') ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to list tables: {e}"))?;

    let tables: Vec<Value> = rows
        .iter()
        .map(|r| {
            let name: String = r.try_get(0).unwrap_or_default();
            let kind: String = r.try_get(1).unwrap_or_default();
            json!({ "name": name, "type": kind })
        })
        .collect();

    Ok(json!({ "tables": tables }).to_string())
}

async fn list_tables_d1(cfg: &crate::db::D1Config) -> Result<String, String> {
    let result = crate::db::d1::query(
        cfg,
        "SELECT name, type FROM sqlite_master WHERE type IN ('table','view') ORDER BY name",
        vec![],
    )
    .await?;
    Ok(json!({ "tables": result.rows }).to_string())
}

// ── describe_table ────────────────────────────────────────────────────────────

async fn describe_table(
    conn: &ActiveConnection,
    schema: &str,
    table: &str,
) -> Result<String, String> {
    match conn {
        ActiveConnection::Postgres(pool) => describe_table_pg(pool, schema, table).await,
        ActiveConnection::Sqlite(pool) => describe_table_sqlite(pool, table).await,
        ActiveConnection::D1(cfg) => describe_table_sqlite_compat(cfg, table).await,
        ActiveConnection::LibSql(cfg) => {
            let tq = format!("\"{}\"", table.replace('"', "\"\""));
            let r = crate::db::libsql::query(cfg, &format!("PRAGMA table_info({tq})"), vec![]).await?;
            Ok(json!({"table":table,"pragma_info":r.rows}).to_string())
        }
        ActiveConnection::Mysql(pool) => describe_table_mysql(pool, schema, table).await,
        ActiveConnection::Posthog(cfg) => {
            let cols = crate::db::posthog::get_column_structure(cfg, table).await?;
            let columns: Vec<_> = cols.iter().map(|c| json!({ "name": c.name, "type": c.data_type, "nullable": c.is_nullable })).collect();
            Ok(json!({"table":table,"columns":columns,"note":"PostHog table, query it with HogQL"}).to_string())
        }
        ActiveConnection::Clickhouse(cfg) => {
            let cols = crate::db::clickhouse::get_column_structure(cfg, schema, table).await?;
            let columns: Vec<_> = cols.iter().map(|c| json!({
                "name": c.name, "type": c.data_type, "nullable": c.is_nullable,
                "default": c.column_default, "comment": c.comment,
            })).collect();
            Ok(json!({"table":table,"columns":columns}).to_string())
        }
        ActiveConnection::Redis(cfg) => {
            let cols = crate::db::redis::get_column_structure(cfg, table).await?;
            let columns: Vec<_> = cols.iter().map(|c| json!({
                "name": c.name, "type": c.data_type, "nullable": c.is_nullable,
                "default": c.column_default, "comment": c.comment,
            })).collect();
            Ok(json!({"table":table,"columns":columns}).to_string())
        }
        ActiveConnection::Duckdb(h) => {
            let cols = crate::db::duckdb::get_column_structure(h, table).await?;
            let columns: Vec<_> = cols.iter().map(|c| json!({
                "name": c.name, "type": c.data_type, "nullable": c.is_nullable,
                "default": c.column_default, "comment": c.comment,
            })).collect();
            Ok(json!({"table":table,"columns":columns}).to_string())
        }
        ActiveConnection::Mssql(h) => {
            let cols = crate::db::mssql::get_column_structure(h, schema, table).await?;
            let columns: Vec<_> = cols.iter().map(|c| json!({
                "name": c.name, "type": c.data_type, "nullable": c.is_nullable,
                "default": c.column_default, "comment": c.comment,
            })).collect();
            Ok(json!({"table":table,"columns":columns}).to_string())
        }
    }
}

async fn describe_table_mysql(pool: &sqlx::MySqlPool, schema: &str, table: &str) -> Result<String, String> {
    let col_rows = sqlx::query(
        "SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT, EXTRA, COLUMN_KEY \
         FROM information_schema.COLUMNS \
         WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Column query failed: {e}"))?;

    let columns: Vec<serde_json::Value> = col_rows
        .iter()
        .map(|r| {
            let name = my_text(r, 0).unwrap_or_default();
            let col_type = my_text(r, 1).unwrap_or_default();
            let nullable = my_text(r, 2).unwrap_or_else(|| "YES".to_string());
            let default = my_text(r, 3);
            let extra = my_text(r, 4).unwrap_or_default();
            json!({
                "name": name,
                "type": col_type,
                "nullable": nullable == "YES",
                "default": default,
                "identity": extra.to_lowercase().contains("auto_increment"),
            })
        })
        .collect();

    let pk = crate::db::mysql::fetch_primary_key(pool, schema, table).await.unwrap_or_default();
    Ok(json!({ "schema": schema, "table": table, "columns": columns, "primary_key": pk }).to_string())
}

async fn describe_table_pg(
    pool: &sqlx::PgPool,
    schema: &str,
    table: &str,
) -> Result<String, String> {
    // Columns
    let col_rows = sqlx::query(
        r#"
        SELECT c.column_name, c.data_type, c.is_nullable, c.column_default,
               c.character_maximum_length, c.is_identity,
               COALESCE(
                   (SELECT string_agg(e.enumlabel, ',' ORDER BY e.enumsortorder)
                    FROM pg_type t
                    JOIN pg_namespace n ON n.oid = t.typnamespace
                    JOIN pg_enum e ON e.enumtypid = t.oid
                    WHERE t.typname = c.udt_name AND n.nspname = c.udt_schema),
                   ''
               ) AS enum_values
        FROM information_schema.columns c
        WHERE c.table_schema = $1 AND c.table_name = $2
        ORDER BY c.ordinal_position
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Column query failed: {e}"))?;

    let columns: Vec<Value> = col_rows
        .iter()
        .map(|r| {
            let name: String = r.try_get(0).unwrap_or_default();
            let data_type: String = r.try_get(1).unwrap_or_default();
            let nullable: String = r.try_get(2).unwrap_or_else(|_| "YES".into());
            let default: Option<String> = r.try_get(3).ok().flatten();
            let max_len: Option<i32> = r.try_get(4).ok().flatten();
            let is_identity: String = r.try_get(5).unwrap_or_else(|_| "NO".into());
            let enum_vals: String = r.try_get(6).unwrap_or_default();
            let mut col = json!({
                "name": name,
                "type": if let Some(len) = max_len {
                    format!("{data_type}({len})")
                } else {
                    data_type
                },
                "nullable": nullable == "YES",
                "default": default,
                "identity": is_identity == "YES",
            });
            if !enum_vals.is_empty() {
                col["enum_values"] =
                    json!(enum_vals.split(',').collect::<Vec<_>>());
            }
            col
        })
        .collect();

    // Primary key
    let pk_rows = sqlx::query(
        r#"
        SELECT a.attname
        FROM pg_constraint c
        JOIN pg_class t ON t.oid = c.conrelid
        JOIN pg_namespace n ON n.oid = t.relnamespace
        JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(c.conkey)
        WHERE c.contype = 'p' AND n.nspname = $1 AND t.relname = $2
        ORDER BY array_position(c.conkey, a.attnum)
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("PK query failed: {e}"))?;
    let primary_key: Vec<String> = pk_rows
        .iter()
        .filter_map(|r| r.try_get(0).ok())
        .collect();

    // Foreign keys
    let fk_rows = sqlx::query(
        r#"
        SELECT c.conname, a.attname, fn.nspname, f.relname, fa.attname
        FROM pg_constraint c
        JOIN pg_class t ON t.oid = c.conrelid
        JOIN pg_namespace n ON n.oid = t.relnamespace
        JOIN pg_class f ON f.oid = c.confrelid
        JOIN pg_namespace fn ON fn.oid = f.relnamespace
        JOIN LATERAL unnest(c.conkey) WITH ORDINALITY AS pos(attnum, ord) ON true
        JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = pos.attnum
        JOIN LATERAL unnest(c.confkey) WITH ORDINALITY AS fpos(attnum, ord) ON pos.ord = fpos.ord
        JOIN pg_attribute fa ON fa.attrelid = f.oid AND fa.attnum = fpos.attnum
        WHERE c.contype = 'f' AND n.nspname = $1 AND t.relname = $2
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let foreign_keys: Vec<Value> = fk_rows
        .iter()
        .map(|r| {
            json!({
                "name": r.try_get::<String,_>(0).unwrap_or_default(),
                "column": r.try_get::<String,_>(1).unwrap_or_default(),
                "references": format!(
                    "{}.{}.{}",
                    r.try_get::<String,_>(2).unwrap_or_default(),
                    r.try_get::<String,_>(3).unwrap_or_default(),
                    r.try_get::<String,_>(4).unwrap_or_default(),
                )
            })
        })
        .collect();

    // Indexes
    let idx_rows = sqlx::query(
        r#"
        SELECT i.relname, ix.indisunique, ix.indisprimary,
               array_to_string(ARRAY(
                   SELECT a.attname
                   FROM pg_attribute a
                   WHERE a.attrelid = t.oid AND a.attnum = ANY(ix.indkey)
                   ORDER BY array_position(ix.indkey, a.attnum)
               ), ', ')
        FROM pg_index ix
        JOIN pg_class t ON t.oid = ix.indrelid
        JOIN pg_class i ON i.oid = ix.indexrelid
        JOIN pg_namespace n ON n.oid = t.relnamespace
        WHERE n.nspname = $1 AND t.relname = $2
        ORDER BY i.relname
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let indexes: Vec<Value> = idx_rows
        .iter()
        .map(|r| {
            json!({
                "name": r.try_get::<String,_>(0).unwrap_or_default(),
                "unique": r.try_get::<bool,_>(1).unwrap_or(false),
                "primary": r.try_get::<bool,_>(2).unwrap_or(false),
                "columns": r.try_get::<String,_>(3).unwrap_or_default(),
            })
        })
        .collect();

    Ok(json!({
        "schema": schema,
        "table": table,
        "columns": columns,
        "primary_key": primary_key,
        "foreign_keys": foreign_keys,
        "indexes": indexes,
    })
    .to_string())
}

async fn describe_table_sqlite(
    pool: &sqlx::SqlitePool,
    table: &str,
) -> Result<String, String> {
    let tq = format!("\"{}\"", table.replace('"', "\"\""));
    let rows = sqlx::query(&format!("PRAGMA table_info({tq})"))
        .fetch_all(pool)
        .await
        .map_err(|e| format!("PRAGMA failed: {e}"))?;

    let columns: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "name": r.try_get::<String,_>(1).unwrap_or_default(),
                "type": r.try_get::<String,_>(2).unwrap_or_else(|_| "text".into()),
                "nullable": r.try_get::<i64,_>(3).unwrap_or(0) == 0,
                "default": r.try_get::<Option<String>,_>(4).ok().flatten(),
                "primary_key": r.try_get::<i64,_>(5).unwrap_or(0) > 0,
            })
        })
        .collect();

    let pk: Vec<String> = rows
        .iter()
        .filter(|r| r.try_get::<i64, _>(5).unwrap_or(0) > 0)
        .filter_map(|r| r.try_get(1).ok())
        .collect();

    Ok(json!({ "table": table, "columns": columns, "primary_key": pk }).to_string())
}

async fn describe_table_sqlite_compat(
    cfg: &crate::db::D1Config,
    table: &str,
) -> Result<String, String> {
    let tq = format!("\"{}\"", table.replace('"', "\"\""));
    let result =
        crate::db::d1::query(cfg, &format!("PRAGMA table_info({tq})"), vec![]).await?;
    Ok(json!({ "table": table, "pragma_info": result.rows }).to_string())
}

// ── check_migrations ──────────────────────────────────────────────────────────

async fn check_migrations(conn: &ActiveConnection, schema: &str) -> Result<String, String> {
    match conn {
        ActiveConnection::Postgres(pool) => check_migrations_pg(pool, schema).await,
        ActiveConnection::Sqlite(pool) => check_migrations_sqlite(pool).await,
        ActiveConnection::D1(cfg) => check_migrations_d1(cfg).await,
        ActiveConnection::LibSql(_) => Ok(json!({"migrations":[],"note":"Migration detection not yet supported for LibSQL"}).to_string()),
        ActiveConnection::Mysql(pool) => check_migrations_mysql(pool, schema).await,
        ActiveConnection::Clickhouse(_) => Ok(json!({"migrations":[],"note":"Migration detection not supported for ClickHouse"}).to_string()),
        ActiveConnection::Posthog(_) => Ok(json!({"migrations":[],"note":"Migration detection not supported for PostHog"}).to_string()),
        ActiveConnection::Redis(_) => Ok(json!({"migrations":[],"note":"Migration detection not supported for Redis"}).to_string()),
        ActiveConnection::Duckdb(_) => Ok(json!({"migrations":[],"note":"Migration detection not supported for DuckDB"}).to_string()),
        ActiveConnection::Mssql(_) => Ok(json!({"migrations":[],"note":"Migration detection not supported for MS SQL Server"}).to_string()),
    }
}

async fn check_migrations_mysql(pool: &sqlx::MySqlPool, schema: &str) -> Result<String, String> {
    let existing: Vec<String> = sqlx::query_scalar(
        "SELECT TABLE_NAME FROM information_schema.TABLES WHERE TABLE_SCHEMA = ?",
    )
    .bind(schema)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut results = Vec::new();
    for (tbl, framework, name_col, ts_col) in MIGRATION_TABLES {
        if !existing.iter().any(|t| t.as_str() == *tbl) {
            results.push(json!({ "table": tbl, "framework": framework, "found": false }));
            continue;
        }
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM `{schema}`.`{tbl}`"))
            .fetch_one(pool)
            .await
            .unwrap_or(0);
        let order = if ts_col.is_empty() { format!("`{name_col}` DESC") } else { format!("`{ts_col}` DESC") };
        let recent = sqlx::query(&format!(
            "SELECT `{name_col}` as name FROM `{schema}`.`{tbl}` ORDER BY {order} LIMIT 5"
        ))
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        let entries: Vec<serde_json::Value> = recent
            .iter()
            .map(|r| json!({ "name": my_text_named(r, "name").unwrap_or_default() }))
            .collect();
        results.push(json!({ "table": tbl, "framework": framework, "found": true, "total_migrations": count, "recent": entries }));
    }
    Ok(json!({ "schema": schema, "migration_tables": results }).to_string())
}

/// Well-known migration tracking tables. Tuple: (table, framework, name_col, timestamp_col)
const MIGRATION_TABLES: &[(&str, &str, &str, &str)] = &[
    ("_prisma_migrations", "Prisma", "migration_name", "finished_at"),
    ("__drizzle_migrations", "Drizzle ORM", "hash", "created_at"),
    ("schema_migrations", "Rails / Goose", "version", ""),
    ("migrations", "Generic", "name", "executed_at"),
    ("flyway_schema_history", "Flyway", "script", "installed_on"),
    ("knex_migrations", "Knex.js", "name", "migration_time"),
    ("alembic_version", "Alembic", "version_num", ""),
    ("goose_db_version", "Goose", "version_id", "tstamp"),
];

async fn check_migrations_pg(pool: &sqlx::PgPool, schema: &str) -> Result<String, String> {
    let existing: Vec<String> = sqlx::query_scalar(
        "SELECT table_name::text FROM information_schema.tables WHERE table_schema = $1",
    )
    .bind(schema)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut found = Vec::new();
    let mut not_found = Vec::new();

    for (tbl, framework, name_col, ts_col) in MIGRATION_TABLES {
        if !existing.iter().any(|t| t.as_str() == *tbl) {
            not_found.push(json!({ "table": tbl, "framework": framework, "found": false }));
            continue;
        }

        let count: i64 =
            sqlx::query_scalar(&format!(r#"SELECT COUNT(*) FROM "{schema}"."{tbl}""#))
                .fetch_one(pool)
                .await
                .unwrap_or(0);

        let order = if ts_col.is_empty() {
            format!("\"{name_col}\" DESC")
        } else {
            format!("\"{ts_col}\" DESC")
        };
        let select_ts = if ts_col.is_empty() {
            String::new()
        } else {
            format!(", \"{ts_col}\" as ts")
        };
        let recent = sqlx::query(&format!(
            r#"SELECT "{name_col}" as name{select_ts} FROM "{schema}"."{tbl}" ORDER BY {order} LIMIT 5"#
        ))
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let entries: Vec<Value> = recent
            .iter()
            .map(|r| {
                let name: String = r.try_get("name").unwrap_or_default();
                if ts_col.is_empty() {
                    json!({ "name": name })
                } else {
                    let ts = pg_cell(r, 1);
                    json!({ "name": name, "applied_at": ts })
                }
            })
            .collect();

        found.push(json!({
            "table": tbl,
            "framework": framework,
            "found": true,
            "total_migrations": count,
            "recent": entries,
        }));
    }

    let all: Vec<Value> = found.into_iter().chain(not_found).collect();
    Ok(json!({ "schema": schema, "migration_tables": all }).to_string())
}

async fn check_migrations_sqlite(pool: &sqlx::SqlitePool) -> Result<String, String> {
    let existing: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table'")
            .fetch_all(pool)
            .await
            .unwrap_or_default();

    let mut results = Vec::new();
    for (tbl, framework, name_col, ts_col) in MIGRATION_TABLES {
        if !existing.iter().any(|t| t.as_str() == *tbl) {
            results.push(json!({ "table": tbl, "framework": framework, "found": false }));
            continue;
        }
        let count: i64 =
            sqlx::query_scalar(&format!("SELECT COUNT(*) FROM \"{tbl}\""))
                .fetch_one(pool)
                .await
                .unwrap_or(0);
        let order = if ts_col.is_empty() {
            format!("\"{name_col}\" DESC")
        } else {
            format!("\"{ts_col}\" DESC")
        };
        let recent = sqlx::query(&format!(
            "SELECT \"{name_col}\" as name FROM \"{tbl}\" ORDER BY {order} LIMIT 5"
        ))
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        let entries: Vec<Value> = recent
            .iter()
            .map(|r| {
                json!({ "name": r.try_get::<String,_>("name").unwrap_or_default() })
            })
            .collect();
        results.push(json!({
            "table": tbl,
            "framework": framework,
            "found": true,
            "total_migrations": count,
            "recent": entries
        }));
    }
    Ok(json!({ "migration_tables": results }).to_string())
}

async fn check_migrations_d1(cfg: &crate::db::D1Config) -> Result<String, String> {
    // Fetch all table names first
    let result =
        crate::db::d1::query(cfg, "SELECT name FROM sqlite_master WHERE type = 'table'", vec![])
            .await?;

    let name_idx = result
        .columns
        .iter()
        .position(|c| c.name == "name")
        .unwrap_or(0);

    let existing: Vec<String> = result
        .rows
        .iter()
        .filter_map(|r| r.get(name_idx)?.as_str().map(String::from))
        .collect();

    let mut results = Vec::new();
    for (tbl, framework, _, _) in MIGRATION_TABLES {
        let found = existing.iter().any(|n| n.as_str() == *tbl);
        results.push(json!({ "table": tbl, "framework": framework, "found": found }));
    }
    Ok(json!({ "migration_tables": results }).to_string())
}

// ── explain_query ─────────────────────────────────────────────────────────────

async fn explain_query(conn: &ActiveConnection, sql: &str) -> Result<String, String> {
    match conn {
        ActiveConnection::Postgres(pool) => explain_query_pg(pool, sql).await,
        ActiveConnection::Sqlite(pool) => explain_query_sqlite(pool, sql).await,
        ActiveConnection::D1(cfg) => explain_query_d1(cfg, sql).await,
        ActiveConnection::LibSql(cfg) => {
            let r = crate::db::libsql::query(cfg, &format!("EXPLAIN QUERY PLAN {sql}"), vec![]).await?;
            Ok(json!({"plan":r.rows,"database":"libsql"}).to_string())
        }
        ActiveConnection::Mysql(pool) => explain_query_mysql(pool, sql).await,
        ActiveConnection::Clickhouse(cfg) => {
            let r = crate::db::clickhouse::query(cfg, &format!("EXPLAIN {sql}")).await?;
            Ok(json!({"plan":r.rows,"database":"clickhouse"}).to_string())
        }
        ActiveConnection::Redis(_) => Ok(json!({"plan":[],"database":"redis","note":"EXPLAIN is not supported on Redis"}).to_string()),
        ActiveConnection::Posthog(_) => Ok(json!({"plan":[],"database":"posthog","note":"EXPLAIN is not available through PostHog's query API"}).to_string()),
        ActiveConnection::Duckdb(h) => {
            let r = crate::db::duckdb::execute_sql(h, &format!("EXPLAIN {sql}")).await?;
            Ok(json!({"plan":r.rows,"database":"duckdb"}).to_string())
        }
        ActiveConnection::Mssql(h) => {
            let r = crate::db::mssql::execute_sql(h, &format!("SET SHOWPLAN_ALL ON; {sql}")).await?;
            Ok(json!({"plan":r.rows,"database":"mssql"}).to_string())
        }
    }
}

async fn explain_query_mysql(pool: &sqlx::MySqlPool, sql: &str) -> Result<String, String> {
    let rows = sqlx::query(&format!("EXPLAIN FORMAT=JSON {sql}"))
        .fetch_all(pool)
        .await
        .map_err(|e| format!("EXPLAIN failed: {e}"))?;

    let plan_text: String = rows
        .first()
        .and_then(|r| my_text(&r, 0))
        .unwrap_or_default();
    let plan: serde_json::Value = serde_json::from_str(&plan_text).unwrap_or(json!(plan_text));
    Ok(json!({ "plan": plan, "database": "mysql" }).to_string())
}

async fn explain_query_pg(pool: &sqlx::PgPool, sql: &str) -> Result<String, String> {
    let rows = sqlx::query(&format!("EXPLAIN (FORMAT JSON) {sql}"))
        .fetch_all(pool)
        .await
        .map_err(|e| format!("EXPLAIN failed: {e}"))?;

    let plan_text: String = rows
        .first()
        .and_then(|r| r.try_get::<String, _>(0).ok())
        .unwrap_or_default();
    let plan: Value = serde_json::from_str(&plan_text).unwrap_or(json!(plan_text));

    Ok(json!({ "plan": plan, "database": "postgres" }).to_string())
}

async fn explain_query_sqlite(pool: &sqlx::SqlitePool, sql: &str) -> Result<String, String> {
    let rows = sqlx::query(&format!("EXPLAIN QUERY PLAN {sql}"))
        .fetch_all(pool)
        .await
        .map_err(|e| format!("EXPLAIN failed: {e}"))?;

    let steps: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.try_get::<i64,_>(0).unwrap_or(0),
                "parent": r.try_get::<i64,_>(1).unwrap_or(0),
                "detail": r.try_get::<String,_>(3).unwrap_or_default()
            })
        })
        .collect();

    Ok(json!({ "query_plan": steps, "database": "sqlite" }).to_string())
}

async fn explain_query_d1(cfg: &crate::db::D1Config, sql: &str) -> Result<String, String> {
    let result =
        crate::db::d1::query(cfg, &format!("EXPLAIN QUERY PLAN {sql}"), vec![]).await?;
    Ok(json!({ "query_plan": result.rows, "database": "d1" }).to_string())
}

// ── get_database_stats ────────────────────────────────────────────────────────

async fn get_database_stats(conn: &ActiveConnection, schema: &str) -> Result<String, String> {
    match conn {
        ActiveConnection::Postgres(pool) => get_database_stats_pg(pool, schema).await,
        ActiveConnection::Sqlite(pool) => get_database_stats_sqlite(pool).await,
        ActiveConnection::D1(cfg) => get_database_stats_d1(cfg).await,
        ActiveConnection::LibSql(cfg) => {
            let r = crate::db::libsql::query(cfg, "SELECT COUNT(*) as table_count FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'", vec![]).await?;
            let count = r.rows.first().and_then(|row| row.first()).and_then(|v| v.as_i64()).unwrap_or(0);
            Ok(json!({"database":"libsql","table_count":count}).to_string())
        }
        ActiveConnection::Mysql(pool) => get_database_stats_mysql(pool, schema).await,
        ActiveConnection::Clickhouse(cfg) => {
            let tables = crate::db::clickhouse::list_tables(cfg, schema).await?;
            Ok(json!({"database":"clickhouse","table_count":tables.len()}).to_string())
        }
        ActiveConnection::Posthog(cfg) => {
            let tables = crate::db::posthog::list_tables(cfg).await?;
            Ok(json!({"database":"posthog","dialect":"HogQL","table_count":tables.len()}).to_string())
        }
        ActiveConnection::Redis(cfg) => {
            let tables = crate::db::redis::list_tables(cfg).await?;
            Ok(json!({"database":"redis","table_count":tables.len()}).to_string())
        }
        ActiveConnection::Duckdb(h) => {
            let tables = crate::db::duckdb::list_tables(h).await?;
            Ok(json!({"database":"duckdb","table_count":tables.len()}).to_string())
        }
        ActiveConnection::Mssql(h) => {
            let tables = crate::db::mssql::list_tables(h, schema).await?;
            Ok(json!({"database":"mssql","table_count":tables.len()}).to_string())
        }
    }
}

async fn get_database_stats_mysql(pool: &sqlx::MySqlPool, schema: &str) -> Result<String, String> {
    let rows = sqlx::query(
        "SELECT TABLE_NAME, \
                COALESCE(DATA_LENGTH + INDEX_LENGTH, 0) AS total_size, \
                COALESCE(DATA_LENGTH, 0) AS data_size, \
                COALESCE(INDEX_LENGTH, 0) AS index_size, \
                COALESCE(TABLE_ROWS, 0) AS row_estimate \
         FROM information_schema.TABLES \
         WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'BASE TABLE' \
         ORDER BY total_size DESC LIMIT 30",
    )
    .bind(schema)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Stats query failed: {e}"))?;

    let tables: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let total: i64 = r.try_get::<Option<i64>, _>(1).ok().flatten().unwrap_or(0);
            let data: i64 = r.try_get::<Option<i64>, _>(2).ok().flatten().unwrap_or(0);
            let idx: i64 = r.try_get::<Option<i64>, _>(3).ok().flatten().unwrap_or(0);
            let live: i64 = r.try_get::<Option<i64>, _>(4).ok().flatten().unwrap_or(0);
            json!({
                "table": r.try_get::<String, _>(0).unwrap_or_default(),
                "total_size": fmt_bytes(total),
                "table_size": fmt_bytes(data),
                "index_size": fmt_bytes(idx),
                "live_rows": live,
            })
        })
        .collect();
    Ok(json!({ "schema": schema, "tables": tables }).to_string())
}

fn fmt_bytes(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1_048_576 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1_073_741_824 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
    }
}

async fn get_database_stats_pg(pool: &sqlx::PgPool, schema: &str) -> Result<String, String> {
    let table_rows = sqlx::query(
        r#"
        SELECT
            t.relname,
            pg_total_relation_size(t.oid),
            pg_relation_size(t.oid),
            pg_indexes_size(t.oid),
            COALESCE(s.n_live_tup, 0),
            COALESCE(s.n_dead_tup, 0),
            CASE WHEN (COALESCE(s.heap_blks_hit,0) + COALESCE(s.heap_blks_read,0)) > 0
                 THEN ROUND(100.0 * s.heap_blks_hit / (s.heap_blks_hit + s.heap_blks_read), 1)
                 ELSE NULL END
        FROM pg_class t
        JOIN pg_namespace n ON n.oid = t.relnamespace
        LEFT JOIN pg_stat_user_tables s ON s.relid = t.oid
        WHERE n.nspname = $1 AND t.relkind = 'r'
        ORDER BY pg_total_relation_size(t.oid) DESC
        LIMIT 30
        "#,
    )
    .bind(schema)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Stats query failed: {e}"))?;

    let tables: Vec<Value> = table_rows
        .iter()
        .map(|r| {
            let total: i64 = r.try_get(1).unwrap_or(0);
            let tbl: i64 = r.try_get(2).unwrap_or(0);
            let idx: i64 = r.try_get(3).unwrap_or(0);
            let live: i64 = r.try_get(4).unwrap_or(0);
            let dead: i64 = r.try_get(5).unwrap_or(0);
            let cache: Option<f64> = r.try_get(6).ok().flatten();
            json!({
                "table": r.try_get::<String,_>(0).unwrap_or_default(),
                "total_size": fmt_bytes(total),
                "table_size": fmt_bytes(tbl),
                "index_size": fmt_bytes(idx),
                "live_rows": live,
                "dead_rows": dead,
                "cache_hit_pct": cache,
                "bloat_pct": if live > 0 {
                    json!((dead as f64 / (live + dead) as f64 * 100.0).round())
                } else { json!(null) }
            })
        })
        .collect();

    let conn_rows = sqlx::query(
        r#"SELECT COALESCE(state,'idle'), count(*)::bigint FROM pg_stat_activity
           WHERE datname = current_database() GROUP BY state"#,
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let connections: Vec<Value> = conn_rows
        .iter()
        .map(|r| {
            json!({
                "state": r.try_get::<String,_>(0).unwrap_or_default(),
                "count": r.try_get::<i64,_>(1).unwrap_or(0)
            })
        })
        .collect();

    let db_stats = sqlx::query(
        r#"SELECT blks_hit, blks_read, xact_commit, xact_rollback
           FROM pg_stat_database WHERE datname = current_database()"#,
    )
    .fetch_one(pool)
    .await
    .ok();

    let database = db_stats.map(|r| {
        let hit: i64 = r.try_get(0).unwrap_or(0);
        let read: i64 = r.try_get(1).unwrap_or(0);
        let commits: i64 = r.try_get(2).unwrap_or(0);
        let rollbacks: i64 = r.try_get(3).unwrap_or(0);
        let total = hit + read;
        json!({
            "cache_hit_pct": if total > 0 { json!((hit as f64 / total as f64 * 100.0).round()) } else { json!(null) },
            "transactions_committed": commits,
            "transactions_rolled_back": rollbacks,
        })
    });

    Ok(json!({
        "schema": schema,
        "tables": tables,
        "connections": connections,
        "database": database,
    })
    .to_string())
}

async fn get_database_stats_sqlite(pool: &sqlx::SqlitePool) -> Result<String, String> {
    let page_count: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(pool)
        .await
        .unwrap_or(4096);
    let freelist: i64 = sqlx::query_scalar("PRAGMA freelist_count")
        .fetch_one(pool)
        .await
        .unwrap_or(0);

    let table_names: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .fetch_all(pool)
            .await
            .unwrap_or_default();

    let mut tables = Vec::new();
    for name in &table_names {
        let count: i64 =
            sqlx::query_scalar(&format!("SELECT COUNT(*) FROM \"{name}\""))
                .fetch_one(pool)
                .await
                .unwrap_or(0);
        tables.push(json!({ "table": name, "row_count": count }));
    }

    Ok(json!({
        "database_size": fmt_bytes(page_count * page_size),
        "free_space": fmt_bytes(freelist * page_size),
        "page_count": page_count,
        "page_size": page_size,
        "tables": tables,
    })
    .to_string())
}

async fn get_database_stats_d1(cfg: &crate::db::D1Config) -> Result<String, String> {
    let result = crate::db::d1::query(
        cfg,
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
        vec![],
    )
    .await?;
    Ok(json!({ "tables": result.rows }).to_string())
}

// ── Postgres cell → JSON ──────────────────────────────────────────────────────

fn pg_cell(row: &sqlx::postgres::PgRow, idx: usize) -> Value {
    use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
    use rust_decimal::Decimal;
    use sqlx::{Decode, Postgres, ValueRef};
    use uuid::Uuid;

    macro_rules! try_get {
        ($t:ty) => {
            if let Ok(v) = row.try_get::<Option<$t>, _>(idx) {
                return match v {
                    Some(x) => json!(x),
                    None => Value::Null,
                };
            }
        };
    }
    macro_rules! try_str {
        ($t:ty) => {
            if let Ok(v) = row.try_get::<Option<$t>, _>(idx) {
                return match v {
                    Some(x) => json!(x.to_string()),
                    None => Value::Null,
                };
            }
        };
    }

    try_get!(bool);
    try_get!(i16);
    try_get!(i32);
    try_get!(i64);
    try_get!(f32);
    try_get!(f64);
    try_str!(Decimal);
    try_str!(DateTime<Utc>);
    try_str!(NaiveDateTime);
    try_str!(NaiveDate);
    try_str!(NaiveTime);
    try_str!(Uuid);
    if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
        return match v {
            // Cap oversized text the same way the grid does - a multi-MB cell
            // in an MCP response floods the calling AI tool's context.
            Some(s) if s.len() > crate::db::sql_util::CELL_VALUE_CAP => {
                crate::db::sql_util::oversize_cell(
                    &row.column(idx).type_info().name().to_lowercase(),
                    s.len(),
                    s.as_bytes(),
                )
            }
            Some(s) => json!(s),
            None => Value::Null,
        };
    }

    let col = row.column(idx);
    let type_name = col.type_info().name();
    if type_name == "JSON" || type_name == "JSONB" {
        if let Ok(raw) = row.try_get_raw(idx) {
            if raw.is_null() {
                return Value::Null;
            }
            // Peek the wire bytes before decoding (see db::query cell handling):
            // binary-format jsonb = 1-byte version header + JSON text, and JSON
            // text can never start with 0x01.
            if let Ok(bytes) = raw.as_bytes() {
                let body = if bytes.first() == Some(&1) { &bytes[1..] } else { bytes };
                if body.len() > crate::db::sql_util::CELL_VALUE_CAP {
                    return crate::db::sql_util::oversize_cell(&type_name.to_lowercase(), body.len(), body);
                }
            }
        }
        if let Ok(v) = row.try_get::<Option<Value>, _>(idx) {
            return v.unwrap_or(Value::Null);
        }
    }
    if let Ok(raw) = row.try_get_raw(idx) {
        if raw.is_null() {
            return Value::Null;
        }
        if let Ok(bytes) = raw.as_bytes() {
            if bytes.len() > crate::db::sql_util::CELL_VALUE_CAP && std::str::from_utf8(bytes).is_ok() {
                return crate::db::sql_util::oversize_cell(&type_name.to_lowercase(), bytes.len(), bytes);
            }
        }
        if let Ok(text) = <String as Decode<Postgres>>::decode(raw) {
            return json!(text);
        }
    }
    Value::Null
}
