use super::connection::D1Config;
use super::query::{ColumnInfo, SqlResult};
use serde::Deserialize;
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Instant;

// ── Shared HTTP client ─────────────────────────────────────────────────────────
// One client for the whole process lifetime. reqwest maintains an internal
// connection pool, so subsequent requests to the same host reuse the TLS
// session and avoid the ~300-700 ms TLS handshake on every call.

static D1_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn client() -> &'static reqwest::Client {
    D1_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            // Without these, a REST call that never answers (captive portal, dropped
            // route, Cloudflare edge stall) leaves the UI spinning on "Connecting…"
            // forever - the command simply never returns.
            .connect_timeout(std::time::Duration::from_secs(25))
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("failed to build D1 HTTP client")
    })
}

// ── Response types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct D1Response {
    result: Vec<D1QueryResult>,
    success: bool,
    errors: Vec<Value>,
}

#[derive(Deserialize)]
struct D1QueryResult {
    #[serde(default)]
    results: Vec<serde_json::Map<String, Value>>,
    success: bool,
    meta: D1Meta,
}

#[derive(Deserialize)]
struct D1Meta {
    #[serde(default)]
    #[allow(dead_code)]
    duration: f64,
    #[serde(default)]
    changes: i64,
    #[serde(default)]
    rows_read: i64,
    #[serde(default)]
    rows_written: i64,
}

fn result_to_sql(result: D1QueryResult, elapsed: u64) -> Result<SqlResult, String> {
    if !result.success {
        return Err("D1 query returned success=false".to_string());
    }
    let columns: Vec<ColumnInfo> = result
        .results
        .first()
        .map(|row| {
            row.keys()
                .map(|k| ColumnInfo::new(k.clone(), "text"))
                .collect()
        })
        .unwrap_or_default();

    let rows: Vec<Vec<Value>> = result
        .results
        .iter()
        .map(|row| {
            columns
                .iter()
                // Cap oversized cells - a multi-MB TEXT/JSON value shipped whole
                // freezes the webview (see sql_util::CELL_VALUE_CAP); small
                // scalars pass through untouched.
                .map(|c| {
                    super::sql_util::cap_json_value(
                        "text",
                        row.get(&c.name).cloned().unwrap_or(Value::Null),
                    )
                })
                .collect()
        })
        .collect();

    let row_count = if result.meta.rows_written > 0 || result.meta.changes > 0 {
        Some(result.meta.changes)
    } else if !rows.is_empty() {
        Some(result.meta.rows_read)
    } else {
        Some(0)
    };

    let message = if result.meta.changes > 0 {
        Some(format!("{} row(s) affected", result.meta.changes))
    } else {
        None
    };

    // `sql` is stamped by `query` (the caller has the statement string).
    Ok(SqlResult { columns, rows, row_count, message, query_ms: elapsed, sql: String::new() })
}

// ── Public API ─────────────────────────────────────────────────────────────────

/// Execute a single SQL statement. Reuses the shared HTTP client so subsequent
/// calls skip the TLS handshake and use a pooled connection.
pub async fn query(config: &D1Config, sql: &str, params: Vec<Value>) -> Result<SqlResult, String> {
    let url = format!(
        "https://api.cloudflare.com/client/v4/accounts/{}/d1/database/{}/query",
        config.account_id, config.database_id
    );

    let t0 = Instant::now();
    let body = serde_json::json!({ "sql": sql, "params": params });
    let send = |token: String| {
        let url = url.clone();
        let body = body.clone();
        async move {
            client()
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| crate::cloudflare::describe_request_error("D1 request", &e))
        }
    };

    let mut res = send(config.api_token.clone()).await?;

    // Cloudflare OAuth access tokens expire while the app is still connected, so
    // a session that worked ten minutes ago starts answering 401 to everything
    // until you reconnect. Exchange the refresh token once and replay the
    // request; if that still fails, the error below is the real one and the user
    // genuinely needs to re-authorize. Exactly one retry - a revoked token would
    // otherwise loop.
    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        if let Some(fresh) = crate::cloudflare::refreshed_token().await {
            // Update the live connection too, or every subsequent query pays for
            // its own 401 + refresh round trip.
            super::connection::update_d1_token(&fresh);
            res = send(fresh).await?;
        }
    }

    let status = res.status();
    let elapsed = t0.elapsed().as_millis() as u64;

    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(format!("D1 API error {status}: {}", body.chars().take(400).collect::<String>()));
    }

    let d1: D1Response = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse D1 response: {e}"))?;

    if !d1.success {
        let errs: Vec<String> = d1.errors.iter().map(|e| e.to_string()).collect();
        return Err(format!("D1 error: {}", errs.join("; ")));
    }

    let result = d1.result.into_iter().next().ok_or("Empty D1 result")?;
    let mut out = result_to_sql(result, elapsed)?;
    out.sql = sql.to_string();
    Ok(out)
}

