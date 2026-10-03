use super::connection::{MssqlConfig, MssqlHandle};
use super::query::{ColumnInfo, ForeignKeyInfo, RowFilter, SqlResult, TableRows};
use super::schema::{ColumnStructureRow, IndexInfo, TableInfo};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Instant;
use tiberius::{AuthMethod, Client, Config, EncryptionLevel, Row};
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

pub type MssqlClient = Client<Compat<TcpStream>>;

// ── Connect ──────────────────────────────────────────────────────────────────

/// Open a tiberius client, following one Azure-style routing redirect if needed.
///
/// Has no timeout of its own: callers wrap this in `connect_racing_probe`, which
/// bounds the whole attempt and reports which phase stalled.
pub async fn connect(cfg: &MssqlConfig) -> Result<MssqlClient, String> {
    let config = build_config(cfg);
    let addr = config.get_addr();
    let tcp = TcpStream::connect(&addr)
        .await
        .map_err(|e| format!("Can't reach {addr}: {e}"))?;
    tcp.set_nodelay(true).ok();

    match Client::connect(config, tcp.compat_write()).await {
        Ok(client) => Ok(client),
        // Azure SQL hands back a redirect address on first connect.
        Err(tiberius::error::Error::Routing { host, port }) => {
            let mut redirected = build_config(cfg);
            redirected.host(&host);
            redirected.port(port);
            let tcp = TcpStream::connect(format!("{host}:{port}"))
                .await
                .map_err(|e| format!("Can't reach redirect {host}:{port}: {e}"))?;
            tcp.set_nodelay(true).ok();
            Client::connect(redirected, tcp.compat_write())
                .await
                .map_err(|e| format!("MS SQL connection failed: {e}"))
        }
        Err(e) => Err(explain_connect_error(cfg, &e.to_string())),
    }
}

/// Turn a driver error into something that says what to do about it.
///
/// One case earns its own sentence. Against a server with a self-signed
/// certificate - SQL Server 2022's default, and every dev container - a
/// connection with Encrypt on fails inside the TLS handshake even with
/// "Trust server certificate" on, and the rustls backend reports
/// `invalid peer certificate: Other(UnsupportedCertVersion)`. That names
/// neither the cause nor the one setting that avoids it.
///
/// Verified: reproducible with `encrypt: true` against the
/// `docker/dialects.yml` fixture, and gone with `encrypt: false`, which still
/// encrypts the login packet - the driver logs a TLS handshake either way. The
/// certificate is not reachable with `openssl s_client`, because TDS wraps TLS
/// inside its own pre-login exchange, so the error's own wording is as far as
/// this has been pinned down; treat the version as the driver's account of it
/// rather than something confirmed against the certificate.
fn explain_connect_error(cfg: &MssqlConfig, err: &str) -> String {
    if cfg.encrypt && err.contains("UnsupportedCertVersion") {
        return format!(
            "MS SQL connection failed: this client will not complete an \
             encrypted handshake with the server's certificate, even with \
             \"Trust server certificate\" on - it reports the certificate in a \
             format it does not accept. Turn Encrypt off for this connection \
             (the login is still encrypted), or install a CA-issued \
             certificate on the server. ({err})"
        );
    }
    format!("MS SQL connection failed: {err}")
}

fn build_config(cfg: &MssqlConfig) -> Config {
    let mut config = Config::new();
    config.host(&cfg.host);
    config.port(cfg.port);
    config.database(&cfg.database);
    config.authentication(AuthMethod::sql_server(&cfg.user, &cfg.password));
    config.encryption(if cfg.encrypt { EncryptionLevel::Required } else { EncryptionLevel::NotSupported });
    if cfg.trust_cert {
        config.trust_cert();
    }
    config
}

pub async fn ping(handle: &MssqlHandle) -> Result<(), String> {
    let mut client = handle.lock().await;
    client
        .simple_query("SELECT 1")
        .await
        .map_err(|e| format!("MS SQL ping failed: {e}"))?
        .into_results()
        .await
        .map_err(|e| format!("MS SQL ping failed: {e}"))?;
    Ok(())
}

// ── Cell conversion ──────────────────────────────────────────────────────────

/// Convert one cell to JSON by probing the column's concrete Rust type.
/// tiberius `try_get` fails (not panics) on a type mismatch, so the cascade
/// lands on the first type whose `FromSql` matches the column.
fn cell_to_json(row: &Row, i: usize) -> Value {
    macro_rules! try_scalar {
        ($t:ty, $conv:expr) => {
            if let Ok(v) = row.try_get::<$t, usize>(i) {
                return v.map($conv).unwrap_or(Value::Null);
            }
        };
    }

    // Cap oversized strings (NVARCHAR(MAX) etc.) - a multi-MB cell shipped
    // whole freezes the webview (see sql_util::CELL_VALUE_CAP).
    if let Ok(v) = row.try_get::<&str, usize>(i) {
        return v
            .map(|s| {
                if s.len() > super::sql_util::CELL_VALUE_CAP {
                    super::sql_util::oversize_cell("nvarchar", s.len(), s.as_bytes())
                } else {
                    Value::String(s.to_string())
                }
            })
            .unwrap_or(Value::Null);
    }
    try_scalar!(i32, Value::from);
    try_scalar!(i64, Value::from);
    try_scalar!(i16, Value::from);
    try_scalar!(u8, Value::from);
    try_scalar!(bool, Value::Bool);
    try_scalar!(f64, |f| serde_json::Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null));
    try_scalar!(f32, |f| serde_json::Number::from_f64(f as f64).map(Value::Number).unwrap_or(Value::Null));

    if let Ok(v) = row.try_get::<chrono::NaiveDateTime, usize>(i) {
        return v.map(|d| Value::String(d.format("%Y-%m-%d %H:%M:%S%.f").to_string())).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<chrono::NaiveDate, usize>(i) {
        return v.map(|d| Value::String(d.to_string())).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<chrono::NaiveTime, usize>(i) {
        return v.map(|t| Value::String(t.to_string())).unwrap_or(Value::Null);
    }
    if let Ok(v) = row.try_get::<&[u8], usize>(i) {
        return v.map(|b| Value::String(format!("<binary: {} bytes>", b.len()))).unwrap_or(Value::Null);
    }
    Value::Null
}

fn rows_to_result(rows: &[Row], elapsed: u64) -> SqlResult {
    let columns: Vec<ColumnInfo> = rows
        .first()
        .map(|r| {
            r.columns()
                .iter()
                .map(|c| ColumnInfo::new(c.name().to_string(), format!("{:?}", c.column_type()).to_lowercase()))
                .collect()
        })
        .unwrap_or_default();

    let data: Vec<Vec<Value>> = rows
        .iter()
        .map(|r| (0..r.columns().len()).map(|i| cell_to_json(r, i)).collect())
        .collect();

    let row_count = Some(data.len() as i64);
    // `sql` is stamped by the caller, which holds the statement string.
    SqlResult { columns, rows: data, row_count, message: None, query_ms: elapsed, sql: String::new() }
}

fn is_read_query(sql: &str) -> bool {
    let head = super::sql_util::statement_head(sql);
    matches!(head.as_str(), "select" | "with" | "show" | "exec" | "execute" | "values" | "declare" | "explain")
}

fn quote_ident(ident: &str) -> String {
    super::sql_util::quote_bracket(ident)
}

fn esc_literal(s: &str) -> String {
    super::sql_util::esc_single_quote(s)
}

/// Escape T-SQL `LIKE` metacharacters (`%`, `_`, `[`) with `\` so user input
/// matches literally (used with `ESCAPE '\'`), then quote-double for the literal.
fn esc_like(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
        .replace('[', "\\[");
    esc_literal(&escaped)
}

// ── Query ────────────────────────────────────────────────────────────────────

pub async fn execute_sql(handle: &MssqlHandle, sql: &str) -> Result<SqlResult, String> {
    let t0 = Instant::now();
    let mut client = handle.lock().await;

    if is_read_query(sql) {
        let results = client
            .simple_query(sql)
            .await
            .map_err(|e| format!("MS SQL error: {e}"))?
            .into_results()
            .await
            .map_err(|e| format!("MS SQL error: {e}"))?;
        let rows = results.into_iter().next().unwrap_or_default();
        let mut out = rows_to_result(&rows, t0.elapsed().as_millis() as u64);
        out.sql = sql.to_string();
        Ok(out)
    } else {
        let res = client
            .execute(sql, &[])
            .await
            .map_err(|e| format!("MS SQL error: {e}"))?;
        let affected: u64 = res.rows_affected().iter().sum();
        Ok(SqlResult {
            columns: vec![],
            rows: vec![],
            row_count: Some(affected as i64),
            message: Some(format!("{affected} row(s) affected")),
            query_ms: t0.elapsed().as_millis() as u64,
            sql: sql.to_string(),
        })
    }
}

/// Run a read query and return the first result set's rows (helper for introspection).
async fn fetch_rows(handle: &MssqlHandle, sql: &str) -> Result<Vec<Row>, String> {
    let mut client = handle.lock().await;
    let results = client
        .simple_query(sql)
        .await
        .map_err(|e| format!("MS SQL error: {e}"))?
        .into_results()
        .await
        .map_err(|e| format!("MS SQL error: {e}"))?;
    Ok(results.into_iter().next().unwrap_or_default())
}

// ── Schema introspection ───────────────────────────────────────────────────────

pub async fn list_schemas(handle: &MssqlHandle) -> Result<Vec<String>, String> {
    let rows = fetch_rows(
        handle,
        // `sys` and `INFORMATION_SCHEMA` are real schemas worth browsing, so they
        // come back and the UI decides whether to show them (it hides system
        // schemas behind a toggle). The fixed database roles stay out: those are
        // permission principals that happen to own a schema, and there is never
        // anything in them.
        "SELECT name FROM sys.schemas WHERE name NOT IN \
         ('guest','db_owner','db_accessadmin','db_securityadmin', \
          'db_ddladmin','db_backupoperator','db_datareader','db_datawriter','db_denydatareader','db_denydatawriter') \
         ORDER BY CASE WHEN name IN ('sys','INFORMATION_SCHEMA') THEN 1 ELSE 0 END, name",
    )
    .await?;
    Ok(rows.iter().filter_map(|r| r.try_get::<&str, _>(0).ok().flatten().map(String::from)).collect())
}

pub async fn list_tables(handle: &MssqlHandle, schema: &str) -> Result<Vec<TableInfo>, String> {
    let s = esc_literal(schema);
    let rows = fetch_rows(
        handle,
        &format!(
            "SELECT t.name, 'table' AS kind, ISNULL(SUM(p.rows), 0) AS row_count \
             FROM sys.tables t \
             JOIN sys.schemas sc ON sc.schema_id = t.schema_id \
             LEFT JOIN sys.partitions p ON p.object_id = t.object_id AND p.index_id IN (0,1) \
             WHERE sc.name = '{s}' GROUP BY t.name \
             UNION ALL \
             SELECT v.name, 'view' AS kind, 0 AS row_count \
             FROM sys.views v JOIN sys.schemas sc ON sc.schema_id = v.schema_id WHERE sc.name = '{s}' \
             ORDER BY 1"
        ),
    )
    .await?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            let name = r.try_get::<&str, _>(0).ok().flatten()?.to_string();
            let kind = r.try_get::<&str, _>(1).ok().flatten().unwrap_or("table").to_string();
            let row_count = r.try_get::<i64, _>(2).ok().flatten().unwrap_or(-1);
            Some(TableInfo { name, kind, row_count, rls_enabled: None })
        })
        .collect())
}

pub async fn list_indexes(handle: &MssqlHandle, schema: &str) -> Result<Vec<IndexInfo>, String> {
    let s = esc_literal(schema);
    let rows = fetch_rows(
        handle,
        &format!(
            "SELECT i.name AS index_name, t.name AS table_name, i.is_unique, i.is_primary_key, i.type_desc, \
                    STUFF((SELECT ',' + c.name FROM sys.index_columns ic2 \
                           JOIN sys.columns c ON c.object_id = ic2.object_id AND c.column_id = ic2.column_id \
                           WHERE ic2.object_id = i.object_id AND ic2.index_id = i.index_id \
                           ORDER BY ic2.key_ordinal FOR XML PATH('')), 1, 1, '') AS cols \
             FROM sys.indexes i \
             JOIN sys.tables t ON t.object_id = i.object_id \
             JOIN sys.schemas sc ON sc.schema_id = t.schema_id \
             WHERE sc.name = '{s}' AND i.name IS NOT NULL ORDER BY t.name, i.name"
        ),
    )
    .await?;
    Ok(rows
        .iter()
        .filter_map(|r| {
            let name = r.try_get::<&str, _>(0).ok().flatten()?.to_string();
            let table_name = r.try_get::<&str, _>(1).ok().flatten()?.to_string();
            let is_unique = r.try_get::<bool, _>(2).ok().flatten().unwrap_or(false);
            let is_primary = r.try_get::<bool, _>(3).ok().flatten().unwrap_or(false);
            let index_type = r.try_get::<&str, _>(4).ok().flatten().unwrap_or("").to_lowercase();
            let columns = r.try_get::<&str, _>(5).ok().flatten().unwrap_or("").to_string();
            Some(IndexInfo { name, table_name, columns, index_type, is_unique, is_primary, condition: None, comment: None })
        })
        .collect())
}

pub async fn get_column_structure(handle: &MssqlHandle, schema: &str, table: &str) -> Result<Vec<ColumnStructureRow>, String> {
    let s = esc_literal(schema);
    let t = esc_literal(table);
    let rows = fetch_rows(
        handle,
        &format!(
            "SELECT c.ORDINAL_POSITION, c.COLUMN_NAME, \
                    c.DATA_TYPE + ISNULL('(' + CASE WHEN c.CHARACTER_MAXIMUM_LENGTH = -1 THEN 'max' \
                        ELSE CONVERT(varchar, c.CHARACTER_MAXIMUM_LENGTH) END + ')', '') AS data_type, \
                    c.IS_NULLABLE, c.COLUMN_DEFAULT \
             FROM INFORMATION_SCHEMA.COLUMNS c \
             WHERE c.TABLE_SCHEMA = '{s}' AND c.TABLE_NAME = '{t}' ORDER BY c.ORDINAL_POSITION"
        ),
    )
    .await?;
    Ok(rows
        .iter()
        .enumerate()
        .filter_map(|(idx, r)| {
            let name = r.try_get::<&str, _>(1).ok().flatten()?.to_string();
            let data_type = r.try_get::<&str, _>(2).ok().flatten().unwrap_or("").to_string();
            let is_nullable = r.try_get::<&str, _>(3).ok().flatten().map(|s| s.eq_ignore_ascii_case("YES")).unwrap_or(true);
            let column_default = r.try_get::<&str, _>(4).ok().flatten().filter(|s| !s.is_empty()).map(String::from);
            let ordinal = r.try_get::<i32, _>(0).ok().flatten().unwrap_or((idx + 1) as i32);
            Some(ColumnStructureRow {
                ordinal_position: ordinal,
                name,
                data_type,
                is_nullable,
                column_default,
                foreign_key: None,
                fk_constraint_name: None,
                is_primary_key: false,
                comment: None,
            })
        })
        .collect())
}

pub async fn get_ddl(handle: &MssqlHandle, schema: &str, table: &str) -> Result<String, String> {
    // SQL Server has no SHOW CREATE TABLE; synthesize from column metadata.
    let cols = get_column_structure(handle, schema, table).await?;
    if cols.is_empty() {
        return Ok(String::new());
    }
    let pk = primary_key(handle, schema, table).await;
    let mut lines: Vec<String> = cols
        .iter()
        .map(|c| {
            let nullable = if c.is_nullable { "NULL" } else { "NOT NULL" };
            let def = c.column_default.as_deref().map(|d| format!(" DEFAULT {d}")).unwrap_or_default();
            format!("    {} {} {}{}", quote_ident(&c.name), c.data_type, nullable, def)
        })
        .collect();
    if !pk.is_empty() {
        let pk_cols = pk.iter().map(|c| quote_ident(c)).collect::<Vec<_>>().join(", ");
        lines.push(format!("    PRIMARY KEY ({pk_cols})"));
    }
    Ok(format!("CREATE TABLE {}.{} (\n{}\n);", quote_ident(schema), quote_ident(table), lines.join(",\n")))
}

async fn primary_key(handle: &MssqlHandle, schema: &str, table: &str) -> Vec<String> {
    let s = esc_literal(schema);
    let t = esc_literal(table);
    let sql = format!(
        "SELECT kcu.COLUMN_NAME FROM INFORMATION_SCHEMA.TABLE_CONSTRAINTS tc \
         JOIN INFORMATION_SCHEMA.KEY_COLUMN_USAGE kcu ON tc.CONSTRAINT_NAME = kcu.CONSTRAINT_NAME \
         WHERE tc.CONSTRAINT_TYPE = 'PRIMARY KEY' AND tc.TABLE_SCHEMA = '{s}' AND tc.TABLE_NAME = '{t}' \
         ORDER BY kcu.ORDINAL_POSITION"
    );
    match fetch_rows(handle, &sql).await {
        Ok(rows) => rows.iter().filter_map(|r| r.try_get::<&str, _>(0).ok().flatten().map(String::from)).collect(),
        Err(_) => Vec::new(),
    }
}

// ── Data browsing ──────────────────────────────────────────────────────────────

pub async fn get_table_rows(
    handle: &MssqlHandle,
    schema: &str,
    table: &str,
    limit: i64,
    offset: i64,
    search: Option<String>,
    sort_column: Option<String>,
    sort_direction: Option<String>,
    filters: Option<Vec<RowFilter>>,
    // MSSQL serializes every round-trip on a single locked connection, so each
    // skipped catalog query directly cuts latency. On plain pagination (no
    // search/filter/sort) the column structure isn't needed, and the PK is only
    // fetched when the frontend asks for fresh metadata.
    include_meta: bool,
) -> Result<TableRows, String> {
    let t0 = Instant::now();
    let has_search = search.as_deref().map(str::trim).is_some_and(|s| !s.is_empty());
    let has_filters = filters.as_ref().is_some_and(|f| !f.is_empty());
    let has_sort = sort_column.as_deref().map(str::trim).is_some_and(|s| !s.is_empty());
    let col_names: Vec<String> = if include_meta || has_search || has_filters || has_sort {
        get_column_structure(handle, schema, table)
            .await?
            .iter()
            .map(|c| c.name.clone())
            .collect()
    } else {
        Vec::new()
    };
    let tq = format!("{}.{}", quote_ident(schema), quote_ident(table));
    let where_clause = build_where(&col_names, search.as_deref(), filters.as_deref());

    let count_sql = format!("SELECT COUNT_BIG(*) FROM {tq}{where_clause}");
    let total = fetch_rows(handle, &count_sql)
        .await?
        .first()
        .and_then(|r| r.try_get::<i64, _>(0).ok().flatten())
        .unwrap_or(0);

    // OFFSET/FETCH requires an ORDER BY; fall back to the first column.
    let order_col = sort_column
        .as_deref()
        .filter(|c| !c.trim().is_empty() && col_names.iter().any(|k| k == c))
        .map(|c| c.to_string())
        .or_else(|| col_names.first().cloned());

    let order = match order_col {
        Some(c) => {
            let dir = if sort_direction.as_deref().map(|d| d.eq_ignore_ascii_case("desc")).unwrap_or(false) { "DESC" } else { "ASC" };
            format!(" ORDER BY {} {dir}", quote_ident(&c))
        }
        None => " ORDER BY (SELECT NULL)".to_string(),
    };

    let data_sql = format!("SELECT * FROM {tq}{where_clause}{order} OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY");
    let result = execute_sql(handle, &data_sql).await?;

    Ok(TableRows {
        // Preview fetching is a Postgres path (pg_stats + pg_column_size).
        preview_columns: Vec::new(),
        columns: result.columns,
        rows: result.rows,
        total,
        query_ms: t0.elapsed().as_millis() as u64,
        primary_key: if include_meta { primary_key(handle, schema, table).await } else { Vec::new() },
        foreign_keys: Vec::<ForeignKeyInfo>::new(),
        sql: format!("{data_sql}\n{count_sql}"),
    })
}

/// Build a WHERE clause with single-quoted literals (tiberius simple_query has
/// no positional params); identifiers validated against the column list.
fn build_where(cols: &[String], search: Option<&str>, filters: Option<&[RowFilter]>) -> String {
    let known: std::collections::HashSet<&str> = cols.iter().map(|s| s.as_str()).collect();
    let mut clauses: Vec<String> = Vec::new();

    if let Some(q) = search.map(str::trim).filter(|s| !s.is_empty()) {
        let needle = esc_like(q);
        let ors: Vec<String> = cols
            .iter()
            .map(|c| format!("CAST({} AS NVARCHAR(MAX)) LIKE '%{needle}%' ESCAPE '\\'", quote_ident(c)))
            .collect();
        if !ors.is_empty() {
            clauses.push(format!("({})", ors.join(" OR ")));
        }
    }

    for f in filters.unwrap_or(&[]) {
        if !known.contains(f.column.as_str()) {
            continue;
        }
        let col = quote_ident(&f.column);
        let raw = f.value.as_deref().unwrap_or("");
        let lit = esc_literal(raw);
        let like = esc_like(raw);
        let clause = match f.op.as_str() {
            "=" | "eq" => format!("CAST({col} AS NVARCHAR(MAX)) = '{lit}'"),
            "!=" | "ne" | "neq" => format!("CAST({col} AS NVARCHAR(MAX)) <> '{lit}'"),
            ">" | "gt" => format!("{col} > '{lit}'"),
            ">=" | "gte" => format!("{col} >= '{lit}'"),
            "<" | "lt" => format!("{col} < '{lit}'"),
            "<=" | "lte" => format!("{col} <= '{lit}'"),
            "contains" => format!("CAST({col} AS NVARCHAR(MAX)) LIKE '%{like}%' ESCAPE '\\'"),
            // A NULL cell contains nothing, so it satisfies "does not contain".
            "not_contains" => format!("({col} IS NULL OR NOT (CAST({col} AS NVARCHAR(MAX)) LIKE '%{like}%' ESCAPE '\\'))"),
            "starts_with" => format!("CAST({col} AS NVARCHAR(MAX)) LIKE '{like}%' ESCAPE '\\'"),
            "ends_with" => format!("CAST({col} AS NVARCHAR(MAX)) LIKE '%{like}' ESCAPE '\\'"),
            "between" => {
                let mut parts = raw.splitn(2, ',');
                let from = esc_literal(parts.next().unwrap_or("").trim());
                let to = esc_literal(parts.next().unwrap_or("").trim());
                format!("{col} BETWEEN '{from}' AND '{to}'")
            }
            "is_null" => format!("{col} IS NULL"),
            "is_not_null" => format!("{col} IS NOT NULL"),
            _ => continue,
        };
        clauses.push(clause);
    }

    if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    }
}

// ── Row editing ──────────────────────────────────────────────────────────────

/// Format a JSON value as a T-SQL literal.
fn sql_literal(v: &Value) -> String {
    match v {
        Value::Null => "NULL".to_string(),
        Value::Bool(b) => if *b { "1".to_string() } else { "0".to_string() },
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("'{}'", esc_literal(s)),
        other => format!("'{}'", esc_literal(&other.to_string())),
    }
}

fn pk_where(pk: &HashMap<String, Value>) -> String {
    let clauses: Vec<String> = pk.iter().map(|(c, v)| format!("{} = {}", quote_ident(c), sql_literal(v))).collect();
    format!(" WHERE {}", clauses.join(" AND "))
}

pub async fn update_table_cell(
    handle: &MssqlHandle,
    schema: &str,
    table: &str,
    primary_key: HashMap<String, Value>,
    column: &str,
    value: &Value,
) -> Result<(), String> {
    if primary_key.is_empty() {
        return Err("Cannot update row: table has no primary key".into());
    }
    let tq = format!("{}.{}", quote_ident(schema), quote_ident(table));
    let sql = format!("UPDATE {tq} SET {} = {}{}", quote_ident(column), sql_literal(value), pk_where(&primary_key));
    let res = execute_sql(handle, &sql).await?;
    if res.row_count == Some(0) {
        return Err("No row updated (row may have changed)".into());
    }
    Ok(())
}

pub async fn insert_table_row(
    handle: &MssqlHandle,
    schema: &str,
    table: &str,
    values: HashMap<String, Value>,
) -> Result<Vec<Value>, String> {
    let cols: Vec<String> = values.keys().cloned().collect();
    let col_list = cols.iter().map(|c| quote_ident(c)).collect::<Vec<_>>().join(", ");
    let val_list = cols.iter().map(|c| sql_literal(&values[c])).collect::<Vec<_>>().join(", ");
    let tq = format!("{}.{}", quote_ident(schema), quote_ident(table));
    // OUTPUT INSERTED.* returns the full new row (including server-generated columns).
    // This INSERT yields a result set, so it must go through simple_query - not
    // execute_sql, whose DML branch uses execute() and discards returned rows
    // (which left the grid showing an all-NULL row after insert).
    let sql = format!("INSERT INTO {tq} ({col_list}) OUTPUT INSERTED.* VALUES ({val_list})");
    let mut client = handle.lock().await;
    let results = client
        .simple_query(&sql)
        .await
        .map_err(|e| format!("MS SQL error: {e}"))?
        .into_results()
        .await
        .map_err(|e| format!("MS SQL error: {e}"))?;
    let rows = results.into_iter().next().unwrap_or_default();
    Ok(rows_to_result(&rows, 0).rows.into_iter().next().unwrap_or_default())
}

pub async fn delete_table_rows(
    handle: &MssqlHandle,
    schema: &str,
    table: &str,
    primary_keys: Vec<HashMap<String, Value>>,
) -> Result<u64, String> {
    if primary_keys.is_empty() {
        return Ok(0);
    }
    let tq = format!("{}.{}", quote_ident(schema), quote_ident(table));
    let mut deleted = 0u64;
    for pk in &primary_keys {
        if pk.is_empty() {
            return Err("Cannot delete rows: table has no primary key".into());
        }
        let sql = format!("DELETE FROM {tq}{}", pk_where(pk));
        let res = execute_sql(handle, &sql).await?;
        deleted += res.row_count.unwrap_or(0).max(0) as u64;
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::open_mssql;

    fn test_config() -> MssqlConfig {
        MssqlConfig {
            name: "test".into(),
            host: "127.0.0.1".into(),
            port: 1433,
            database: "master".into(),
            user: "sa".into(),
            password: "Strong!Passw0rd".into(),
            encrypt: false,
            trust_cert: true,
        }
    }

    // Requires a live SQL Server on 127.0.0.1:1433. Run with:
    //   cargo test --lib db::mssql::tests -- --ignored
    #[tokio::test]
    #[ignore]
    async fn introspection_roundtrip() {
        let h = open_mssql(&test_config()).await.expect("connect");

        execute_sql(&h, "IF OBJECT_ID('dbo.t_users','U') IS NOT NULL DROP TABLE dbo.t_users").await.unwrap();
        execute_sql(&h, "CREATE TABLE dbo.t_users (id INT PRIMARY KEY, name NVARCHAR(50), born DATE)").await.unwrap();
        execute_sql(&h, "INSERT INTO dbo.t_users (id, name, born) VALUES (1, N'ada', '1815-12-10'), (2, N'alan', '1912-06-23')").await.unwrap();

        let schemas = list_schemas(&h).await.unwrap();
        assert!(schemas.iter().any(|s| s == "dbo"), "schemas: {schemas:?}");

        let tables = list_tables(&h, "dbo").await.unwrap();
        assert!(tables.iter().any(|t| t.name == "t_users" && t.row_count == 2), "tables: {tables:?}");

        let cols = get_column_structure(&h, "dbo", "t_users").await.unwrap();
        assert_eq!(cols.len(), 3, "cols: {cols:?}");

        let rows = get_table_rows(&h, "dbo", "t_users", 10, 0, None, None, None, None, true).await.unwrap();
        assert_eq!(rows.total, 2);
        assert_eq!(rows.rows.len(), 2);
        assert_eq!(rows.primary_key, vec!["id".to_string()], "pk: {:?}", rows.primary_key);

        let _ = list_indexes(&h, "dbo").await.unwrap();
        let _ddl = get_ddl(&h, "dbo", "t_users").await.unwrap();

        update_table_cell(&h, "dbo", "t_users", std::collections::HashMap::from([("id".into(), Value::from(1))]), "name", &Value::from("ada2")).await.unwrap();
        let deleted = delete_table_rows(&h, "dbo", "t_users", vec![std::collections::HashMap::from([("id".into(), Value::from(2))])]).await.unwrap();
        assert_eq!(deleted, 1);

        execute_sql(&h, "DROP TABLE dbo.t_users").await.unwrap();
    }
}
