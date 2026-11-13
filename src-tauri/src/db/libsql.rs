use super::connection::LibSqlConfig;
use super::query::{ColumnInfo, SqlResult};
use serde::Deserialize;
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Instant;

static LIBSQL_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn client() -> &'static reqwest::Client {
    LIBSQL_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            // Bounded, but generously: this carries queries, not just metadata.
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("failed to build LibSQL HTTP client")
    })
}

/// Normalise a user-supplied URL to an https:// base URL.
fn base_url(config: &LibSqlConfig) -> String {
    let url = config.url.trim().trim_end_matches('/');
    if url.starts_with("libsql://") {
        format!("https://{}", &url["libsql://".len()..])
    } else {
        url.to_string()
    }
}

// ── Wire format ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct PipelineResponse {
    results: Vec<PipelineItem>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PipelineItem {
    Ok { response: OkResponse },
    Error { error: ApiError },
}

#[derive(Deserialize)]
struct OkResponse {
    #[serde(rename = "type")]
    kind: String,
    result: Option<ExecResult>,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

#[derive(Deserialize)]
struct ExecResult {
    cols: Vec<ColDef>,
    rows: Vec<Vec<TypedValue>>,
    affected_row_count: i64,
}

#[derive(Deserialize)]
struct ColDef {
    name: String,
    #[allow(dead_code)]
    decltype: Option<String>,
}

/// Turso v2 typed-value cell: `{"type":"text","value":"hello"}` etc.
#[derive(Deserialize)]
struct TypedValue {
    #[serde(rename = "type")]
    kind: String,
    value: Option<Value>,
    base64: Option<String>,
}

impl TypedValue {
    fn to_json(&self) -> Value {
        match self.kind.as_str() {
            "null" => Value::Null,
            "integer" => self.value.as_ref()
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok())
                .map(Value::from)
                .unwrap_or(Value::Null),
            "real" => self.value.as_ref()
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .map(|f| Value::from(f))
                .unwrap_or(Value::Null),
            // Capped - a multi-MB text cell shipped whole freezes the webview
            // (see sql_util::CELL_VALUE_CAP).
            "text" => super::sql_util::cap_json_value(
                "text",
                self.value.clone().unwrap_or(Value::Null),
            ),
            "blob" => self.base64.as_ref()
                .map(|b| Value::String(format!("<blob:{}>", b.len())))
                .unwrap_or(Value::Null),
            _ => self.value.clone().unwrap_or(Value::Null),
        }
    }
}

// ── Param serialisation ────────────────────────────────────────────────────────

/// Convert a `serde_json::Value` into the LibSQL v2 typed-value object.
fn to_libsql_arg(v: &Value) -> Value {
    match v {
        Value::Null => serde_json::json!({ "type": "null" }),
        Value::Bool(b) => serde_json::json!({ "type": "integer", "value": if *b { "1" } else { "0" } }),
        Value::Number(n) => {
            if n.is_f64() {
                serde_json::json!({ "type": "real", "value": n.to_string() })
            } else {
                serde_json::json!({ "type": "integer", "value": n.to_string() })
            }
        }
        Value::String(s) => serde_json::json!({ "type": "text", "value": s }),
        Value::Array(_) | Value::Object(_) => {
            serde_json::json!({ "type": "text", "value": v.to_string() })
        }
    }
}

// ── Public API ─────────────────────────────────────────────────────────────────

pub async fn query(config: &LibSqlConfig, sql: &str, params: Vec<Value>) -> Result<SqlResult, String> {
    query_with(config, sql, params, false).await
}

/// `query` inside a transaction that is always rolled back: whatever the
/// statement is, nothing it does is kept. For read-only callers (the MCP server
/// at "Read only"), where the classifier alone used to be the only guard.
pub async fn query_readonly(config: &LibSqlConfig, sql: &str) -> Result<SqlResult, String> {
    query_with(config, sql, vec![], true).await
}

async fn query_with(config: &LibSqlConfig, sql: &str, params: Vec<Value>, rollback: bool) -> Result<SqlResult, String> {
    let url = format!("{}/v2/pipeline", base_url(config));
    let t0 = Instant::now();

    let args: Vec<Value> = params.iter().map(to_libsql_arg).collect();
    let stmt = serde_json::json!({ "type": "execute", "stmt": { "sql": sql, "args": args } });
    let requests = if rollback {
        serde_json::json!([
            { "type": "execute", "stmt": { "sql": "BEGIN" } },
            stmt,
            { "type": "execute", "stmt": { "sql": "ROLLBACK" } },
            { "type": "close" }
        ])
    } else {
        serde_json::json!([stmt, { "type": "close" }])
    };
    // Which `execute` response is the statement's own: after BEGIN when wrapped.
    let want = if rollback { 1 } else { 0 };

    let mut builder = client()
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "requests": requests }));

    if let Some(token) = &config.auth_token {
        if !token.is_empty() {
            builder = builder.header("Authorization", format!("Bearer {}", token));
        }
    }

    let res = builder.send().await.map_err(|e| format!("LibSQL request failed: {e}"))?;
    let elapsed = t0.elapsed().as_millis() as u64;

    if !res.status().is_success() {
        let status = res.status().as_u16();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("LibSQL error {status}: {}", body.chars().take(400).collect::<String>()));
    }

    let pipeline: PipelineResponse = res.json().await.map_err(|e| format!("Failed to parse LibSQL response: {e}"))?;

    let mut exec_result = None;
    let mut seen = 0;
    for item in pipeline.results {
        match item {
            PipelineItem::Error { error } => return Err(format!("LibSQL error: {}", error.message)),
            PipelineItem::Ok { response } if response.kind == "execute" => {
                if seen == want {
                    exec_result = response.result;
                    break;
                }
                seen += 1;
            }
            _ => {}
        }
    }

    let exec = exec_result.ok_or("No execute result in LibSQL response")?;

    let columns: Vec<ColumnInfo> = exec.cols.iter()
        .map(|c| ColumnInfo::new(c.name.clone(), "text"))
        .collect();

    let rows: Vec<Vec<Value>> = exec.rows.iter()
        .map(|row| row.iter().map(|v| v.to_json()).collect())
        .collect();

    let affected = exec.affected_row_count;
    let row_count = Some(if affected > 0 { affected } else { rows.len() as i64 });
    let message = if affected > 0 { Some(format!("{} row(s) affected", affected)) } else { None };

    Ok(SqlResult { columns, rows, row_count, message, query_ms: elapsed, sql: sql.to_string() })
}
