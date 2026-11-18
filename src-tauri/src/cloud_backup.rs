//! Scheduled backups (Pro). The schedule and history live on stroke.click
//! (`/api/backups/*`); this side runs a job: open the saved connection, dump
//! it with the backup engine (`db::backup`), gzip it, and send it either to
//! Stroke Cloud (chunked upload through the API) or to the user's own
//! S3-compatible bucket (one signed PUT). Database credentials never leave the
//! device, and neither do the bucket's keys: they sit in the keychain vault.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::Manager;

use crate::account::{base_url, http, load_token, read_error, ApiError};
use crate::db::connection::{open_duckdb, open_mssql, open_mysql, open_pg, open_sqlite, ActiveConnection, AnyConnectionConfig};

/// Stroke Cloud upload part size. R2 wants at least 5 MB for every part but
/// the last, and the Worker reads each part into memory.
const PART_BYTES: usize = 16 * 1024 * 1024;
const VAULT_PREFIX: &str = "backup-s3:";

/// What the scheduler needs from a job, as the server lists it.
#[derive(Deserialize, Debug, Clone)]
pub struct JobSpec {
    pub id: String,
    pub label: String,
    pub destination: String,
    #[serde(default)]
    pub s3_bucket: Option<String>,
    #[serde(default)]
    pub s3_endpoint: Option<String>,
    #[serde(default)]
    pub s3_prefix: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct RunOutcome {
    /// "success", "failed", or "skipped" (another device has this tick).
    pub status: &'static str,
    pub message: String,
    pub bytes: u64,
}

impl RunOutcome {
    fn skipped(message: impl Into<String>) -> Self {
        Self { status: "skipped", message: message.into(), bytes: 0 }
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct S3Secret {
    access_key_id: String,
    secret_access_key: String,
    #[serde(default)]
    region: String,
}

fn api_message(e: ApiError) -> String {
    match e {
        ApiError::Unauthorized => "Your Stroke session ended. Sign in again.".into(),
        ApiError::Network(_) => "Couldn't reach stroke.click. Check your connection.".into(),
        ApiError::Server(_, m) => m,
    }
}

async fn token(app: &tauri::AppHandle) -> Result<String, String> {
    load_token(app).await.ok_or_else(|| "Sign in to use scheduled backups.".to_string())
}

async fn send_json(req: reqwest::RequestBuilder) -> Result<Value, ApiError> {
    let resp = req.send().await.map_err(|e| ApiError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(read_error(resp).await);
    }
    resp.json().await.map_err(|e| ApiError::Network(e.to_string()))
}

// ── Job management (thin proxies; the server validates) ───────────────────────

#[tauri::command]
pub async fn backup_jobs_list(app: tauri::AppHandle) -> Result<Value, String> {
    let t = token(&app).await?;
    send_json(http().get(format!("{}/api/backups/jobs", base_url())).bearer_auth(t))
        .await
        .map_err(api_message)
}

#[tauri::command]
pub async fn backup_job_save(app: tauri::AppHandle, job: Value) -> Result<Value, String> {
    let t = token(&app).await?;
    send_json(http().post(format!("{}/api/backups/jobs", base_url())).bearer_auth(t).json(&job))
        .await
        .map_err(api_message)
}

#[tauri::command]
pub async fn backup_job_patch(app: tauri::AppHandle, id: String, patch: Value) -> Result<Value, String> {
    let t = token(&app).await?;
    send_json(http().patch(format!("{}/api/backups/jobs/{}", base_url(), enc(&id))).bearer_auth(t).json(&patch))
        .await
        .map_err(api_message)
}

#[tauri::command]
pub async fn backup_job_delete(app: tauri::AppHandle, id: String) -> Result<Value, String> {
    let t = token(&app).await?;
    let out = send_json(http().delete(format!("{}/api/backups/jobs/{}", base_url(), enc(&id))).bearer_auth(t))
        .await
        .map_err(api_message)?;
    let key = format!("{VAULT_PREFIX}{id}");
    let _ = crate::secrets::update_async(&app, move |m| {
        m.remove(&key);
    })
    .await;
    Ok(out)
}

/// Keep a job's S3 keys in the keychain vault. Empty keys remove them.
#[tauri::command]
pub async fn backup_s3_save(
    app: tauri::AppHandle,
    job_id: String,
    access_key_id: String,
    secret_access_key: String,
    region: String,
) -> Result<(), String> {
    let key = format!("{VAULT_PREFIX}{job_id}");
    let secret = S3Secret {
        access_key_id: access_key_id.trim().to_string(),
        secret_access_key: secret_access_key.trim().to_string(),
        region: region.trim().to_string(),
    };
    let json = serde_json::to_string(&secret).map_err(|e| e.to_string())?;
    crate::secrets::update_async(&app, move |m| {
        if secret.access_key_id.is_empty() || secret.secret_access_key.is_empty() {
            m.remove(&key);
        } else {
            m.insert(key, json);
        }
    })
    .await
}

/// Whether this device has S3 keys for the job (the keys themselves stay put).
#[tauri::command]
pub async fn backup_s3_status(app: tauri::AppHandle, job_id: String) -> Result<Option<String>, String> {
    Ok(load_s3(&app, &job_id).await.map(|s| s.region))
}

async fn load_s3(app: &tauri::AppHandle, job_id: &str) -> Option<S3Secret> {
    let raw = crate::secrets::read_all_async(app).await.get(&format!("{VAULT_PREFIX}{job_id}")).cloned()?;
    serde_json::from_str(&raw).ok()
}

// ── Running a job ─────────────────────────────────────────────────────────────

/// Run one job now: claim `slot` on the server, dump, upload, report.
#[tauri::command]
pub async fn backup_job_run(
    app: tauri::AppHandle,
    job: JobSpec,
    config: AnyConnectionConfig,
    slot: String,
) -> Result<RunOutcome, String> {
    let t = token(&app).await?;
    let base = base_url();

    // S3 keys first: without them there is nothing to claim the tick for.
    let s3 = if job.destination == "s3" {
        match load_s3(&app, &job.id).await {
            Some(s) => Some(s),
            None => return Err("This device has no S3 keys for this backup. Add them in the schedule's settings.".into()),
        }
    } else {
        None
    };

    let claim = http()
        .post(format!("{base}/api/backups/runs"))
        .bearer_auth(&t)
        .json(&serde_json::json!({ "job_id": job.id, "slot": slot }));
    let run = match send_json(claim).await {
        Ok(v) => v,
        Err(ApiError::Server(code, msg)) if code == "claimed" => return Ok(RunOutcome::skipped(msg)),
        Err(e) => return Err(api_message(e)),
    };
    let run_id = run.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
    if run_id.is_empty() {
        return Err("stroke.click didn't start the run.".into());
    }

    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("scheduled-backups");
    let result = dump_and_upload(&dir, &job, config, s3, &base, &t, &run_id).await;

    let body = match &result {
        Ok(done) => serde_json::json!({
            "status": "success", "tables": done.tables, "rows": done.rows,
            "bytes": done.bytes, "object_key": done.object_key,
        }),
        Err(e) => serde_json::json!({ "status": "failed", "error": e }),
    };
    let finish = http().patch(format!("{base}/api/backups/runs/{}", enc(&run_id))).bearer_auth(&t).json(&body);
    let reported = send_json(finish).await;

    match (result, reported) {
        (Ok(done), Ok(_)) => Ok(RunOutcome { status: "success", message: format!("Backed up {} tables.", done.tables), bytes: done.bytes }),
        (Ok(_), Err(e)) => Err(format!("The backup finished but couldn't be recorded: {}", api_message(e))),
        (Err(e), _) => Ok(RunOutcome { status: "failed", message: e, bytes: 0 }),
    }
}

struct Done {
    tables: usize,
    rows: usize,
    bytes: u64,
    object_key: Option<String>,
}

async fn dump_and_upload(
    dir: &Path,
    job: &JobSpec,
    config: AnyConnectionConfig,
    s3: Option<S3Secret>,
    base: &str,
    token: &str,
    run_id: &str,
) -> Result<Done, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let sql_path = dir.join(format!("{run_id}.sql"));
    let gz_path = dir.join(format!("{run_id}.sql.gz"));
    let cleanup = |paths: &[&PathBuf]| {
        for p in paths {
            let _ = std::fs::remove_file(p);
        }
    };

    let result = async {
        let conn = open(config).await?;
        crate::db::backup::reset_cancel();
        let rep = crate::db::backup::Reporter::silent();
        let opts = crate::db::backup::ExportOptions::default();
        let export = crate::db::backup::run_export(&rep, conn.clone(), None, None, &opts, Some(sql_path.to_string_lossy().into_owned())).await;
        crate::db::connection::close_in_background(conn);
        let export = export?;
        if export.cancelled {
            return Err("The backup was cancelled.".to_string());
        }
        let src = sql_path.clone();
        let dst = gz_path.clone();
        let bytes = tokio::task::spawn_blocking(move || gzip(&src, &dst)).await.map_err(|e| e.to_string())??;
        let _ = std::fs::remove_file(&sql_path);

        let object_key = match s3 {
            Some(s) => Some(upload_s3(&gz_path, job, &s, bytes).await?),
            None => {
                upload_stroke(&gz_path, base, token, run_id).await?;
                None
            }
        };
        Ok(Done { tables: export.table_count, rows: export.row_count, bytes, object_key })
    }
    .await;
    cleanup(&[&sql_path, &gz_path]);
    result
}

/// A standalone connection for the dump, apart from whatever the page has open.
async fn open(config: AnyConnectionConfig) -> Result<ActiveConnection, String> {
    Ok(match config {
        AnyConnectionConfig::Postgres(c) => ActiveConnection::Postgres(open_pg(&c).await?),
        AnyConnectionConfig::Sqlite(c) => ActiveConnection::Sqlite(open_sqlite(&c).await?),
        AnyConnectionConfig::Mysql(c) => ActiveConnection::Mysql(open_mysql(&c).await?),
        AnyConnectionConfig::D1(c) => ActiveConnection::D1(c),
        AnyConnectionConfig::Libsql(c) => ActiveConnection::LibSql(c),
        AnyConnectionConfig::Duckdb(c) => ActiveConnection::Duckdb(open_duckdb(&c).await?),
        AnyConnectionConfig::Mssql(c) => ActiveConnection::Mssql(open_mssql(&c).await?),
        _ => return Err("Scheduled backups support PostgreSQL, MySQL, SQLite, D1, Turso/libSQL, DuckDB and SQL Server.".into()),
    })
}

fn gzip(src: &Path, dst: &Path) -> Result<u64, String> {
    let mut input = std::fs::File::open(src).map_err(|e| e.to_string())?;
    let out = std::fs::File::create(dst).map_err(|e| e.to_string())?;
    let mut enc = flate2::write::GzEncoder::new(std::io::BufWriter::new(out), flate2::Compression::default());
    std::io::copy(&mut input, &mut enc).map_err(|e| e.to_string())?;
    enc.finish().map_err(|e| e.to_string())?.flush().map_err(|e| e.to_string())?;
    Ok(std::fs::metadata(dst).map_err(|e| e.to_string())?.len())
}

/// Stroke Cloud: the file in PART_BYTES parts, each retried a few times.
async fn upload_stroke(path: &Path, base: &str, token: &str, run_id: &str) -> Result<(), String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut n = 1u32;
    loop {
        let mut buf = vec![0u8; PART_BYTES];
        let mut filled = 0;
        while filled < PART_BYTES {
            let read = file.read(&mut buf[filled..]).map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            filled += read;
        }
        if filled == 0 {
            break;
        }
        buf.truncate(filled);
        let url = format!("{base}/api/backups/runs/{}/parts/{n}", enc(run_id));
        let mut attempt = 0;
        loop {
            attempt += 1;
            let sent = http()
                .put(&url)
                .bearer_auth(token)
                .header("content-type", "application/octet-stream")
                .timeout(Duration::from_secs(300))
                .body(buf.clone())
                .send()
                .await;
            match sent {
                Ok(r) if r.status().is_success() => break,
                Ok(r) if !crate::account::is_outage(r.status()) || attempt >= 3 => {
                    return Err(format!("Uploading to Stroke Cloud failed: {}", api_message(read_error(r).await)));
                }
                Err(e) if attempt >= 3 => return Err(format!("Uploading to Stroke Cloud failed: {e}")),
                _ => tokio::time::sleep(Duration::from_secs(2 * attempt)).await,
            }
        }
        if filled < PART_BYTES {
            break;
        }
        n += 1;
    }
    Ok(())
}

/// Object keys use only these characters, so the path needs no escaping.
fn key_safe(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '/') { c } else { '-' })
        .collect();
    out.trim_matches('/').to_string()
}

/// The user's bucket: one PUT, the payload streamed from disk and left
/// unsigned (`UNSIGNED-PAYLOAD`, accepted by S3, R2, MinIO and the rest).
async fn upload_s3(path: &Path, job: &JobSpec, s: &S3Secret, bytes: u64) -> Result<String, String> {
    let bucket = job.s3_bucket.as_deref().filter(|b| !b.is_empty()).ok_or("The schedule has no S3 bucket.")?;
    let region = if s.region.is_empty() { "us-east-1" } else { s.region.as_str() };
    let stamp = chrono::Utc::now().format("%Y-%m-%dT%H-%M-%SZ");
    let prefix = job.s3_prefix.as_deref().map(key_safe).filter(|p| !p.is_empty());
    let name = format!("{}-{stamp}.sql.gz", key_safe(&job.label).replace('/', "-"));
    let key = match prefix {
        Some(p) => format!("{p}/{name}"),
        None => name,
    };

    // A custom endpoint (R2, MinIO, B2, Wasabi) is addressed path-style; AWS
    // itself virtual-host style.
    let (scheme_host, host, path_part) = match job.s3_endpoint.as_deref().map(str::trim).filter(|e| !e.is_empty()) {
        Some(ep) => {
            let ep = ep.trim_end_matches('/');
            let with_scheme = if ep.contains("://") { ep.to_string() } else { format!("https://{ep}") };
            let host = with_scheme.split("://").nth(1).unwrap_or_default().split('/').next().unwrap_or_default().to_string();
            (with_scheme, host, format!("/{bucket}/{key}"))
        }
        None => {
            let host = format!("{bucket}.s3.{region}.amazonaws.com");
            (format!("https://{host}"), host, format!("/{key}"))
        }
    };

    let amz_date = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let headers = vec![
        ("host".to_string(), host),
        ("x-amz-content-sha256".to_string(), "UNSIGNED-PAYLOAD".to_string()),
        ("x-amz-date".to_string(), amz_date.clone()),
    ];
    let creds = crate::db::dynamodb::Creds {
        access_key_id: s.access_key_id.clone(),
        secret_access_key: s.secret_access_key.clone(),
        session_token: None,
    };
    let auth = crate::db::dynamodb::sign("PUT", &path_part, "", &headers, b"", &creds, region, "s3", &amz_date);

    let file = tokio::fs::File::open(path).await.map_err(|e| e.to_string())?;
    let body = reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(file));
    let resp = http()
        .put(format!("{scheme_host}{path_part}"))
        .header("authorization", auth)
        .header("x-amz-content-sha256", "UNSIGNED-PAYLOAD")
        .header("x-amz-date", amz_date)
        .header("content-type", "application/gzip")
        .header("content-length", bytes.to_string())
        .timeout(Duration::from_secs(3600))
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the S3 endpoint: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let code = text.split("<Code>").nth(1).and_then(|s| s.split("</Code>").next()).unwrap_or("");
        let msg = text.split("<Message>").nth(1).and_then(|s| s.split("</Message>").next()).unwrap_or("");
        return Err(format!("S3 refused the upload ({status}{}{}).", if code.is_empty() { "" } else { ", " }, [code, msg].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(": ")));
    }
    Ok(key)
}

/// Ids are UUIDs from the app, but escape anything else defensively.
fn enc(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_keys_need_no_escaping() {
        assert_eq!(key_safe("prod db (main)"), "prod-db--main-");
        assert_eq!(key_safe("/backups/stroke/"), "backups/stroke");
        assert_eq!(enc("a1-b2_c3/../x"), "a1-b2_c3x");
    }

    #[test]
    fn gzip_round_trip() {
        let dir = std::env::temp_dir().join(format!("stroke-gz-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("a.sql");
        let dst = dir.join("a.sql.gz");
        std::fs::write(&src, "CREATE TABLE t (x int);\n".repeat(1000)).unwrap();
        let n = gzip(&src, &dst).unwrap();
        assert!(n > 0 && n < 24_000);
        let mut out = String::new();
        flate2::read::GzDecoder::new(std::fs::File::open(&dst).unwrap()).read_to_string(&mut out).unwrap();
        assert_eq!(out.len(), 24_000);
        let _ = std::fs::remove_dir_all(dir);
    }
}
