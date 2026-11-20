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

// ── Stop ─────────────────────────────────────────────────────────────────────
//
// tiberius has no way to send SQL Server's attention signal, and dropping a
// query's future mid-stream leaves the TDS reply half read on the socket: the
// next query on that client would read the rest of the old one. So Stop does
// two things: `KILL`s the session from a second connection, which ends the
// statement on the server, and then `reset` swaps in a fresh client.
//
// The handle does not carry its config, so it is kept here, keyed by the
// handle's address, with the session id (`@@SPID`) to kill.

static SESSIONS: std::sync::LazyLock<std::sync::Mutex<HashMap<usize, (MssqlConfig, i16)>>> =
    std::sync::LazyLock::new(Default::default);

fn key(handle: &MssqlHandle) -> usize {
    std::sync::Arc::as_ptr(handle) as *const () as usize
}

async fn spid(client: &mut MssqlClient) -> Option<i16> {
    let row = client.simple_query("SELECT CAST(@@SPID AS smallint)").await.ok()?.into_row().await.ok()??;
    row.get::<i16, _>(0)
}

/// Remember how to reach a handle's session, so Stop can kill it.
pub async fn remember(handle: &MssqlHandle, cfg: &MssqlConfig) {
    let id = spid(&mut *handle.lock().await).await;
    if let Some(id) = id {
        SESSIONS.lock().unwrap_or_else(|e| e.into_inner()).insert(key(handle), (cfg.clone(), id));
    }
}

/// Drop a handle's registry entry once the handle is done with (a
/// transaction's own client). The entry holds the login, and the address it
/// is keyed by can be reused by the next handle.
pub fn forget(handle: &MssqlHandle) {
    SESSIONS.lock().unwrap_or_else(|e| e.into_inner()).remove(&key(handle));
}

/// A new client on the same server and login, for a transaction: on the
/// shared client every other query would have run inside it.
pub async fn open_tx_client(handle: &MssqlHandle) -> Result<MssqlHandle, String> {
    let cfg = SESSIONS.lock().unwrap_or_else(|e| e.into_inner()).get(&key(handle)).map(|(c, _)| c.clone());
    let cfg = cfg.ok_or("This SQL Server connection can't open a second session. Reconnect and try again.")?;
    let client = connect(&cfg).await?;
    let h = std::sync::Arc::new(tokio::sync::Mutex::new(client));
    remember(&h, &cfg).await;
    Ok(h)
}

/// One batch on the handle, for BEGIN / COMMIT / ROLLBACK.
pub async fn batch(handle: &MssqlHandle, sql: &str) -> Result<(), String> {
    let mut client = handle.lock().await;
    client.simple_query(sql).await.map_err(|e| format!("MS SQL error: {e}"))?.into_results().await.map_err(|e| format!("MS SQL error: {e}"))?;
    Ok(())
}

/// End the handle's running statement on the server. Best effort: KILL needs
/// ALTER ANY CONNECTION, and without it `reset` closing the socket is what
/// stops the work.
pub async fn cancel(handle: &MssqlHandle) {
    let Some((cfg, id)) = SESSIONS.lock().unwrap_or_else(|e| e.into_inner()).get(&key(handle)).cloned() else { return };
    let kill = async {
        let mut other = connect(&cfg).await?;
        other.simple_query(format!("KILL {id}")).await.map_err(|e| e.to_string())?.into_results().await.map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    };
    match tokio::time::timeout(std::time::Duration::from_secs(10), kill).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => log::info!("mssql stop: KILL {id} refused ({e}); closing the connection instead"),
        Err(_) => log::info!("mssql stop: KILL {id} timed out; closing the connection instead"),
    }
}

/// Replace the handle's client after a Stop: the old socket is closed (which
/// also makes the server abandon anything still running) and the next query
/// gets a clean protocol stream.
pub async fn reset(handle: &MssqlHandle) -> Result<(), String> {
    let Some((cfg, _)) = SESSIONS.lock().unwrap_or_else(|e| e.into_inner()).get(&key(handle)).cloned() else {
        return Err("The connection was stopped and can't be reopened here. Reconnect to keep working.".into());
    };
    let mut fresh = connect(&cfg).await?;
    let id = spid(&mut fresh).await;
    *handle.lock().await = fresh;
    if let Some(id) = id {
        SESSIONS.lock().unwrap_or_else(|e| e.into_inner()).insert(key(handle), (cfg, id));
    }
    Ok(())
}

// ── Connect ──────────────────────────────────────────────────────────────────

/// Open a tiberius client, following one Azure-style routing redirect if needed.
///
/// Has no timeout of its own: callers wrap this in `connect_racing_probe`, which
/// bounds the whole attempt and reports which phase stalled.
pub async fn connect(cfg: &MssqlConfig) -> Result<MssqlClient, String> {
    let config = build_config(cfg);
    // Through an SSH tunnel the socket goes to the tunnel's local end while
    // the config keeps the server's name, which TLS checks the certificate
    // against.
    let addr = cfg.via.map(|a| a.to_string()).unwrap_or_else(|| config.get_addr());
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

/// `execute_sql` inside a transaction that is always rolled back, so nothing
/// the statement does is kept (SQL Server's DDL is transactional too). For
/// read-only callers (the MCP server at "Read only"). BEGIN, the statement and
/// ROLLBACK are separate batches on one held connection, and ROLLBACK runs
/// even when the statement fails.
pub async fn execute_sql_readonly(handle: &MssqlHandle, sql: &str) -> Result<SqlResult, String> {
    let t0 = Instant::now();
    let mut client = handle.lock().await;
    client
        .simple_query("BEGIN TRANSACTION")
        .await
        .map_err(|e| format!("MS SQL error: {e}"))?
        .into_results()
        .await
        .map_err(|e| format!("MS SQL error: {e}"))?;
    let run = async {
        client
            .simple_query(sql)
            .await
            .map_err(|e| format!("MS SQL error: {e}"))?
            .into_results()
            .await
            .map_err(|e| format!("MS SQL error: {e}"))
    }
    .await;
    if let Ok(stream) = client.simple_query("IF @@TRANCOUNT > 0 ROLLBACK TRANSACTION").await {
        let _ = stream.into_results().await;
    }
    let rows = run?.into_iter().next().unwrap_or_default();
    let mut out = rows_to_result(&rows, t0.elapsed().as_millis() as u64);
    out.sql = sql.to_string();
    Ok(out)
}

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

/// A read query's first result set, streamed into the console's sink as the
/// server sends it rather than collected. Later result sets are read and
/// dropped, so the protocol stream ends clean. Stop is the caller's (it races
/// this and resets the client, see query::run_one_cancellable).
pub async fn stream_into(handle: &MssqlHandle, sql: &str, sink: &mut super::query::RowSink) -> Result<SqlResult, String> {
    use futures::TryStreamExt;
    let t0 = Instant::now();
    let mut client = handle.lock().await;
    let mut stream = client.simple_query(sql).await.map_err(|e| format!("MS SQL error: {e}"))?;
    let mut columns: Vec<ColumnInfo> = Vec::new();
    let mut total = 0usize;
    let mut sink_err: Option<String> = None;
    while let Some(item) = stream.try_next().await.map_err(|e| format!("MS SQL error: {e}"))? {
        match item {
            tiberius::QueryItem::Metadata(m) if m.result_index() == 0 => {
                columns = m
                    .columns()
                    .iter()
                    .map(|c| ColumnInfo::new(c.name().to_string(), format!("{:?}", c.column_type()).to_lowercase()))
                    .collect();
            }
            tiberius::QueryItem::Row(r) if r.result_index() == 0 && sink_err.is_none() => {
                if total == 0 {
                    if let Err(e) = sink.start(columns.clone()) {
                        sink_err = Some(e);
                        continue;
                    }
                }
                match sink.push((0..r.columns().len()).map(|i| cell_to_json(&r, i)).collect()) {
                    Ok(()) => total += 1,
                    Err(e) => sink_err = Some(e),
                }
            }
            _ => {}
        }
    }
    drop(stream);
    if let Some(e) = sink_err {
        return Err(e);
    }
    if total > 0 {
        sink.finish()?;
    }
    Ok(SqlResult { columns, rows: vec![], row_count: Some(total as i64), message: None, query_ms: t0.elapsed().as_millis() as u64, sql: sql.to_string() })
}

/// Whether `sql` reads (and so has rows to stream).
pub(crate) fn reads(sql: &str) -> bool {
    is_read_query(sql)
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

// ── Explain ────────────────────────────────────────────────────────────────────

/// The three batches `explain_showplan_all` runs, in order: plan mode on, the
/// target query, plan mode off. `SET SHOWPLAN_ALL` must be alone in its batch,
/// so each is sent as its own `simple_query`.
fn showplan_all_batches(sql: &str) -> [String; 3] {
    let query = sql.trim().trim_end_matches(';').trim();
    ["SET SHOWPLAN_ALL ON".to_string(), query.to_string(), "SET SHOWPLAN_ALL OFF".to_string()]
}

/// Return the estimated plan for `sql` as SHOWPLAN_ALL rows without running it.
///
/// SHOWPLAN_ALL is session state and the handle is one shared connection, so
/// the guard is held for the whole sequence and plan mode is always turned off
/// again, even when the query fails. Otherwise every later query on this
/// connection would return a plan instead of rows.
pub async fn explain_showplan_all(handle: &MssqlHandle, sql: &str) -> Result<SqlResult, String> {
    let t0 = Instant::now();
    let [on, query, off] = showplan_all_batches(sql);
    let mut client = handle.lock().await;

    // If plan mode can't be enabled, stop: running the query now would execute it.
    client
        .simple_query(on.as_str())
        .await
        .map_err(|e| format!("EXPLAIN failed: {e}"))?
        .into_results()
        .await
        .map_err(|e| format!("EXPLAIN failed: {e}"))?;

    let captured = match client.simple_query(query.as_str()).await {
        Ok(stream) => stream.into_results().await,
        Err(e) => Err(e),
    };

    let restored = match client.simple_query(off.as_str()).await {
        Ok(stream) => stream.into_results().await.map(|_| ()),
        Err(e) => Err(e),
    };
    drop(client);

    let result_sets = captured.map_err(|e| format!("EXPLAIN failed: {e}"))?;
    restored.map_err(|e| format!("EXPLAIN ran, but SHOWPLAN_ALL could not be turned off again: {e}"))?;

    // One result set per statement in the batch; same columns in each.
    let rows: Vec<Row> = result_sets.into_iter().flatten().collect();
    let mut out = rows_to_result(&rows, t0.elapsed().as_millis() as u64);
    out.sql = query;
    Ok(out)
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
            "SELECT t.name, 'table' AS kind, ISNULL(SUM(p.rows), 0) AS row_count, \
                    CONVERT(varchar(19), t.create_date, 120) AS created \
             FROM sys.tables t \
             JOIN sys.schemas sc ON sc.schema_id = t.schema_id \
             LEFT JOIN sys.partitions p ON p.object_id = t.object_id AND p.index_id IN (0,1) \
             WHERE sc.name = '{s}' GROUP BY t.name, t.create_date \
             UNION ALL \
             SELECT v.name, 'view' AS kind, 0 AS row_count, CONVERT(varchar(19), v.create_date, 120) AS created \
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
            let created_at = r.try_get::<&str, _>(3).ok().flatten().map(str::to_string);
            Some(TableInfo { name, kind, row_count, rls_enabled: None, created_at, create_order: None, system: None })
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

/// Every column from the sys catalog: the declared type with its length,
/// precision or scale (`nvarchar(50)`, `decimal(10,2)`, `varchar(max)`,
/// `datetime2(3)`), primary key membership, the foreign key it belongs to and
/// its MS_Description. `{filter}` narrows it to a schema, and a table.
const MSSQL_COLUMNS_SQL: &str = "\
    SELECT o.name AS table_name, c.column_id, c.name, \
        CASE \
            WHEN ty.name IN ('varchar', 'char', 'varbinary', 'binary') \
                THEN ty.name + '(' + CASE WHEN c.max_length = -1 THEN 'max' ELSE CAST(c.max_length AS varchar(10)) END + ')' \
            WHEN ty.name IN ('nvarchar', 'nchar') \
                THEN ty.name + '(' + CASE WHEN c.max_length = -1 THEN 'max' ELSE CAST(c.max_length / 2 AS varchar(10)) END + ')' \
            WHEN ty.name IN ('decimal', 'numeric') \
                THEN ty.name + '(' + CAST(c.precision AS varchar(10)) + ',' + CAST(c.scale AS varchar(10)) + ')' \
            WHEN ty.name IN ('datetime2', 'time', 'datetimeoffset') \
                THEN ty.name + '(' + CAST(c.scale AS varchar(10)) + ')' \
            ELSE ty.name \
        END AS data_type, \
        c.is_nullable, \
        OBJECT_DEFINITION(c.default_object_id) AS column_default, \
        CAST(CASE WHEN pk.column_id IS NULL THEN 0 ELSE 1 END AS bit) AS is_pk, \
        fk.ref, fk.fk_name, \
        CAST(ep.value AS nvarchar(4000)) AS comment \
    FROM sys.columns c \
    JOIN sys.objects o ON o.object_id = c.object_id AND o.type IN ('U', 'V') \
    JOIN sys.schemas sc ON sc.schema_id = o.schema_id \
    JOIN sys.types ty ON ty.user_type_id = c.user_type_id \
    LEFT JOIN ( \
        SELECT ic.object_id, ic.column_id FROM sys.indexes i \
        JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
        WHERE i.is_primary_key = 1 \
    ) pk ON pk.object_id = c.object_id AND pk.column_id = c.column_id \
    OUTER APPLY ( \
        SELECT TOP 1 rs.name + '.' + rt.name + '.' + rc.name AS ref, f.name AS fk_name \
        FROM sys.foreign_key_columns fkc \
        JOIN sys.foreign_keys f ON f.object_id = fkc.constraint_object_id \
        JOIN sys.objects rt ON rt.object_id = fkc.referenced_object_id \
        JOIN sys.schemas rs ON rs.schema_id = rt.schema_id \
        JOIN sys.columns rc ON rc.object_id = fkc.referenced_object_id AND rc.column_id = fkc.referenced_column_id \
        WHERE fkc.parent_object_id = c.object_id AND fkc.parent_column_id = c.column_id \
        ORDER BY f.name \
    ) fk \
    LEFT JOIN sys.extended_properties ep \
        ON ep.class = 1 AND ep.major_id = c.object_id AND ep.minor_id = c.column_id AND ep.name = 'MS_Description' \
    WHERE {filter} \
    ORDER BY o.name, c.column_id";

fn columns_sql(schema: &str, table: Option<&str>) -> String {
    let mut filter = format!("sc.name = '{}'", esc_literal(schema));
    if let Some(t) = table {
        filter.push_str(&format!(" AND o.name = '{}'", esc_literal(t)));
    }
    MSSQL_COLUMNS_SQL.replace("{filter}", &filter)
}

fn columns_from_rows(rows: &[Row]) -> Vec<(String, ColumnStructureRow)> {
    rows.iter()
        .enumerate()
        .filter_map(|(idx, r)| {
            let text = |i: usize| r.try_get::<&str, _>(i).ok().flatten().filter(|s| !s.is_empty()).map(String::from);
            let table = text(0)?;
            let name = text(2)?;
            Some((
                table,
                ColumnStructureRow {
                    ordinal_position: r.try_get::<i32, _>(1).ok().flatten().unwrap_or((idx + 1) as i32),
                    name,
                    data_type: text(3).unwrap_or_default(),
                    is_nullable: r.try_get::<bool, _>(4).ok().flatten().unwrap_or(true),
                    column_default: text(5),
                    is_primary_key: r.try_get::<bool, _>(6).ok().flatten().unwrap_or(false),
                    foreign_key: text(7),
                    fk_constraint_name: text(8),
                    comment: text(9),
                },
            ))
        })
        .collect()
}

pub async fn get_column_structure(handle: &MssqlHandle, schema: &str, table: &str) -> Result<Vec<ColumnStructureRow>, String> {
    let rows = fetch_rows(handle, &columns_sql(schema, Some(table))).await?;
    Ok(columns_from_rows(&rows).into_iter().map(|(_, c)| c).collect())
}

/// Every table's and view's columns in a schema, in one statement.
pub async fn schema_column_structure(handle: &MssqlHandle, schema: &str) -> Result<Vec<super::schema::TableColumnStructure>, String> {
    let rows = fetch_rows(handle, &columns_sql(schema, None)).await?;
    let mut out: Vec<super::schema::TableColumnStructure> = Vec::new();
    for (table, col) in columns_from_rows(&rows) {
        match out.last_mut() {
            Some(last) if last.table == table => last.columns.push(col),
            _ => out.push(super::schema::TableColumnStructure { table, columns: vec![col] }),
        }
    }
    Ok(out)
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

    let sort_col = sort_column
        .as_deref()
        .filter(|c| !c.trim().is_empty() && col_names.iter().any(|k| k == c));
    // A table open (include_meta) re-reads the key; it's also the PK the grid gets.
    let order_info = cached_order_info(handle, schema, table, &col_names, include_meta).await;

    // OFFSET/FETCH requires an ORDER BY, and it has to be total or pages overlap.
    // Unsorted pages use the same default on every page - the primary key, else
    // the first column - instead of reshuffling under `(SELECT NULL)`; sorted
    // pages get the key as a tie-breaker for rows equal on the sort column.
    let mut terms: Vec<String> = Vec::new();
    if let Some(c) = sort_col {
        let dir = if sort_direction.as_deref().map(|d| d.eq_ignore_ascii_case("desc")).unwrap_or(false) { "DESC" } else { "ASC" };
        terms.push(format!("{} {dir}", quote_ident(c)));
        terms.extend(order_info.pk.iter().filter(|k| k.as_str() != c).map(|k| quote_ident(k)));
    } else {
        terms.extend(order_info.order_columns().iter().map(|k| quote_ident(k)));
    }
    let order = if terms.is_empty() {
        " ORDER BY (SELECT NULL)".to_string()
    } else {
        format!(" ORDER BY {}", terms.join(", "))
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
        primary_key: if include_meta { order_info.pk } else { Vec::new() },
        foreign_keys: Vec::<ForeignKeyInfo>::new(),
        sql: format!("{data_sql}\n{count_sql}"),
    })
}

/// What an unsorted page orders by: the primary key, else the first column.
#[derive(Clone)]
struct OrderInfo {
    pk: Vec<String>,
    first_col: Option<String>,
}

impl OrderInfo {
    fn order_columns(&self) -> &[String] {
        if !self.pk.is_empty() {
            &self.pk
        } else {
            self.first_col.as_slice()
        }
    }
}

type OrderCacheEntry = (Instant, std::sync::Weak<tokio::sync::Mutex<MssqlClient>>, OrderInfo);

/// Memoised per connection + table. MSSQL serializes every round-trip on one
/// locked connection, so uncached this puts one or two catalog queries in front
/// of every infinite-scroll page. Keyed by the handle's address and checked
/// against a `Weak` to it, so a reconnect that reuses the address never reads
/// the old connection's entry.
static ORDER_CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<(usize, String), OrderCacheEntry>>> =
    std::sync::OnceLock::new();
const ORDER_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(30);

async fn cached_order_info(
    handle: &MssqlHandle,
    schema: &str,
    table: &str,
    col_names: &[String],
    refresh: bool,
) -> OrderInfo {
    let key = (std::sync::Arc::as_ptr(handle) as usize, format!("{schema}\u{0}{table}"));
    let cache = ORDER_CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    if !refresh {
        if let Ok(map) = cache.lock() {
            if let Some((at, weak, info)) = map.get(&key) {
                let same_conn = weak.upgrade().is_some_and(|h| std::sync::Arc::ptr_eq(&h, handle));
                if same_conn && at.elapsed() < ORDER_CACHE_TTL {
                    return info.clone();
                }
            }
        }
    }
    let pk = primary_key(handle, schema, table).await;
    let first_col = if !pk.is_empty() {
        None
    } else if let Some(c) = col_names.first() {
        Some(c.clone())
    } else {
        get_column_structure(handle, schema, table)
            .await
            .ok()
            .and_then(|cols| cols.into_iter().next().map(|c| c.name))
    };
    let info = OrderInfo { pk, first_col };
    if let Ok(mut map) = cache.lock() {
        if map.len() > 256 {
            map.retain(|_, (at, weak, _)| at.elapsed() < ORDER_CACHE_TTL && weak.strong_count() > 0);
        }
        map.insert(key, (Instant::now(), std::sync::Arc::downgrade(handle), info.clone()));
    }
    info
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
        // Typed SQL filter. Already checked in `query::validate_raw_filters`
        // before this builder runs; an error here can't happen, and nothing is
        // dropped silently.
        if f.op == super::sql_util::RAW_SQL_FILTER_OP {
            let raw = f.value.as_deref().unwrap_or("");
            if let Ok(Some(expr)) = super::sql_util::raw_where_expr(raw, super::sql_util::RawFilterDialect::Literal) {
                clauses.push(expr);
            }
            continue;
        }
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
        MssqlConfig { ssh: None, via: None,
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

    #[test]
    fn showplan_all_batches_turn_plan_mode_off_last() {
        let [on, query, off] = showplan_all_batches("  SELECT * FROM t WHERE id = 1; ");
        assert_eq!(on, "SET SHOWPLAN_ALL ON");
        assert_eq!(query, "SELECT * FROM t WHERE id = 1");
        assert_eq!(off, "SET SHOWPLAN_ALL OFF");
    }

    // Requires a live SQL Server on 127.0.0.1:1433 (run with --ignored).
    #[tokio::test]
    #[ignore]
    async fn explain_leaves_showplan_off() {
        let h = open_mssql(&test_config()).await.expect("connect");

        let plan = explain_showplan_all(&h, "SELECT name FROM sys.databases").await.unwrap();
        assert!(plan.columns.iter().any(|c| c.name.eq_ignore_ascii_case("StmtText")), "cols: {:?}", plan.columns);

        // A failing query must still turn plan mode off.
        assert!(explain_showplan_all(&h, "SELECT * FROM no_such_table_xyz").await.is_err());

        let rows = execute_sql(&h, "SELECT 42 AS answer").await.unwrap();
        assert_eq!(rows.columns.first().map(|c| c.name.as_str()), Some("answer"));
        assert_eq!(rows.rows, vec![vec![Value::from(42)]]);
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

#[cfg(test)]
mod via_tests {
    // The socket goes to `via` while the config keeps the server's name. With
    // a name that does not resolve and something listening at `via` that is
    // not SQL Server, the connect must get as far as the handshake: failing on
    // a lookup or "Can't reach" would mean the name was dialed.
    //   STROKE_TEST_FAKE_RESP=127.0.0.1:16390 cargo test --lib mssql::via_tests
    #[tokio::test]
    async fn dials_the_tunnel_and_keeps_the_name() {
        let Ok(addr) = std::env::var("STROKE_TEST_FAKE_RESP") else { return };
        let cfg = super::MssqlConfig {
            name: "t".into(),
            host: "mssql.stroke.test".into(),
            port: 1433,
            database: "master".into(),
            user: "sa".into(),
            password: "x".into(),
            encrypt: false,
            trust_cert: false,
            ssh: None,
            via: Some(addr.parse().unwrap()),
        };
        let err = tokio::time::timeout(std::time::Duration::from_secs(5), super::connect(&cfg))
            .await
            .map(|r| r.err().unwrap_or_default())
            .unwrap_or_else(|_| "handshake stalled (reached the listener)".into());
        assert!(!err.contains("Can't reach") && !err.to_lowercase().contains("lookup"), "{err}");
    }
}
