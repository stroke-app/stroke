//! DynamoDB over its JSON HTTP API, signed with SigV4 by hand.
//!
//! The AWS SDK would add a large dependency tree for what is one POST per call:
//! `X-Amz-Target: DynamoDB_20120810.<Op>` with a JSON body. Items stay in
//! DynamoDB's typed JSON (`{"S": "..."}`, `{"N": "1"}`, ...) end to end; the
//! frontend converts them for display and editing.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamoConfig {
    pub name: String,
    /// Blank: the profile's region, then `AWS_REGION`, then us-east-1.
    #[serde(default)]
    pub region: String,
    /// `~/.aws` profile to read credentials from when no keys are given.
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub access_key_id: Option<String>,
    #[serde(default)]
    pub secret_access_key: Option<String>,
    #[serde(default)]
    pub session_token: Option<String>,
    /// Custom endpoint such as DynamoDB Local (`http://localhost:8000`).
    #[serde(default)]
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Creds {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
}

fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

// ── ~/.aws files ─────────────────────────────────────────────────────────────

/// Sections of an AWS ini file, keyed by the profile name (`[profile x]` in
/// `config` and `[x]` in `credentials` both map to `x`).
fn parse_ini(text: &str) -> HashMap<String, HashMap<String, String>> {
    let mut out: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut cur: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let name = name.trim();
            let name = name.strip_prefix("profile ").map(str::trim).unwrap_or(name);
            cur = Some(name.to_string());
            out.entry(name.to_string()).or_default();
            continue;
        }
        if let (Some(sec), Some((k, v))) = (&cur, line.split_once('=')) {
            out.entry(sec.clone())
                .or_default()
                .insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    out
}

fn aws_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|h| std::path::PathBuf::from(h).join(".aws"))
}

fn read_aws_file(env: &str, name: &str) -> HashMap<String, HashMap<String, String>> {
    let path = std::env::var_os(env)
        .map(std::path::PathBuf::from)
        .or_else(|| aws_dir().map(|d| d.join(name)));
    path.and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| parse_ini(&t))
        .unwrap_or_default()
}

/// Profile names found in `~/.aws/config` and `~/.aws/credentials`, sorted.
pub fn list_profiles() -> Vec<String> {
    let mut names: Vec<String> = read_aws_file("AWS_CONFIG_FILE", "config")
        .into_keys()
        .chain(read_aws_file("AWS_SHARED_CREDENTIALS_FILE", "credentials").into_keys())
        .filter(|n| n != "sso-session" && !n.starts_with("sso-session "))
        .collect();
    names.sort();
    names.dedup();
    names
}

fn profile_name(cfg: &DynamoConfig) -> String {
    non_empty(&cfg.profile)
        .or_else(|| std::env::var("AWS_PROFILE").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "default".into())
}

pub fn resolve_region(cfg: &DynamoConfig) -> String {
    if !cfg.region.trim().is_empty() {
        return cfg.region.trim().to_string();
    }
    let profile = profile_name(cfg);
    read_aws_file("AWS_CONFIG_FILE", "config")
        .get(&profile)
        .and_then(|s| s.get("region").cloned())
        .or_else(|| std::env::var("AWS_REGION").ok())
        .or_else(|| std::env::var("AWS_DEFAULT_REGION").ok())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "us-east-1".into())
}

// Temporary credentials from `aws configure export-credentials`, by profile.
static CRED_CACHE: OnceLock<Mutex<HashMap<String, (Creds, Option<SystemTime>)>>> = OnceLock::new();

fn cached(profile: &str) -> Option<Creds> {
    let map = CRED_CACHE.get_or_init(Default::default).lock().ok()?;
    let (c, exp) = map.get(profile)?;
    // Refresh five minutes early.
    match exp {
        Some(t) if SystemTime::now() + Duration::from_secs(300) >= *t => None,
        _ => Some(c.clone()),
    }
}

/// SSO, assume-role and `credential_process` profiles: let the AWS CLI do the
/// work. `export-credentials` resolves any profile kind it supports.
async fn export_credentials(profile: &str) -> Result<Creds, String> {
    if let Some(c) = cached(profile) {
        return Ok(c);
    }
    let fut = tokio::process::Command::new("aws")
        .args(["configure", "export-credentials", "--format", "process", "--profile", profile])
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(Duration::from_secs(20), fut)
        .await
        .map_err(|_| format!("`aws configure export-credentials` timed out for profile \"{profile}\""))?
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!("Profile \"{profile}\" has no access keys, and the AWS CLI (needed for SSO and role profiles) is not installed")
            } else {
                e.to_string()
            }
        })?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let hint = if err.contains("sso") || err.contains("SSO") || err.contains("expired") {
            format!(" Run `aws sso login --profile {profile}` and try again.")
        } else {
            String::new()
        };
        return Err(format!("Could not load credentials for profile \"{profile}\": {err}{hint}"));
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
    let creds = Creds {
        access_key_id: s("AccessKeyId").ok_or("AWS CLI returned no AccessKeyId")?,
        secret_access_key: s("SecretAccessKey").ok_or("AWS CLI returned no SecretAccessKey")?,
        session_token: s("SessionToken"),
    };
    let exp = s("Expiration")
        .and_then(|e| chrono::DateTime::parse_from_rfc3339(&e).ok())
        .map(|d| SystemTime::UNIX_EPOCH + Duration::from_secs(d.timestamp().max(0) as u64));
    if let Ok(mut map) = CRED_CACHE.get_or_init(Default::default).lock() {
        map.insert(profile.to_string(), (creds.clone(), exp));
    }
    Ok(creds)
}

/// Keys typed into the form, then the profile's static keys, then the
/// environment (only when no profile was named), then the AWS CLI.
pub async fn resolve_creds(cfg: &DynamoConfig) -> Result<Creds, String> {
    if let (Some(id), Some(secret)) = (non_empty(&cfg.access_key_id), non_empty(&cfg.secret_access_key)) {
        return Ok(Creds { access_key_id: id, secret_access_key: secret, session_token: non_empty(&cfg.session_token) });
    }
    let named = non_empty(&cfg.profile).is_some();
    let profile = profile_name(cfg);
    let files = [
        read_aws_file("AWS_SHARED_CREDENTIALS_FILE", "credentials"),
        read_aws_file("AWS_CONFIG_FILE", "config"),
    ];
    for f in &files {
        if let Some(sec) = f.get(&profile) {
            if let (Some(id), Some(secret)) = (sec.get("aws_access_key_id"), sec.get("aws_secret_access_key")) {
                return Ok(Creds {
                    access_key_id: id.clone(),
                    secret_access_key: secret.clone(),
                    session_token: sec.get("aws_session_token").cloned(),
                });
            }
        }
    }
    if !named {
        if let (Ok(id), Ok(secret)) = (std::env::var("AWS_ACCESS_KEY_ID"), std::env::var("AWS_SECRET_ACCESS_KEY")) {
            if !id.is_empty() && !secret.is_empty() {
                return Ok(Creds { access_key_id: id, secret_access_key: secret, session_token: std::env::var("AWS_SESSION_TOKEN").ok() });
            }
        }
    }
    // DynamoDB Local takes any keys.
    if non_empty(&cfg.endpoint).is_some() && !files.iter().any(|f| f.contains_key(&profile)) {
        return Ok(Creds { access_key_id: "local".into(), secret_access_key: "local".into(), session_token: None });
    }
    if !files.iter().any(|f| f.contains_key(&profile)) {
        return Err(if named {
            format!("No AWS profile named \"{profile}\" in ~/.aws/config or ~/.aws/credentials")
        } else {
            "No AWS credentials: enter an access key, or pick a profile from ~/.aws".into()
        });
    }
    export_credentials(&profile).await
}

// ── SigV4 ────────────────────────────────────────────────────────────────────

type HmacSha256 = Hmac<Sha256>;

fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    let mut m = HmacSha256::new_from_slice(key).expect("HMAC takes any key length");
    m.update(data.as_bytes());
    m.finalize().into_bytes().to_vec()
}

fn sha_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// The `Authorization` header for one request. `headers` must already hold
/// every header to sign (host and x-amz-date included), names lowercase.
#[allow(clippy::too_many_arguments)]
pub fn sign(
    method: &str,
    path: &str,
    query: &str,
    headers: &[(String, String)],
    body: &[u8],
    creds: &Creds,
    region: &str,
    service: &str,
    amz_date: &str,
) -> String {
    let mut hs: Vec<(String, String)> = headers
        .iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), v.split_whitespace().collect::<Vec<_>>().join(" ")))
        .collect();
    hs.sort();
    let canonical_headers: String = hs.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let signed: String = hs.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(";");
    let canonical = format!("{method}\n{path}\n{query}\n{canonical_headers}\n{signed}\n{}", sha_hex(body));
    let date = &amz_date[..8];
    let scope = format!("{date}/{region}/{service}/aws4_request");
    let to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}", sha_hex(canonical.as_bytes()));
    let k = hmac(format!("AWS4{}", creds.secret_access_key).as_bytes(), date);
    let k = hmac(&k, region);
    let k = hmac(&k, service);
    let k = hmac(&k, "aws4_request");
    let sig = hex::encode(hmac(&k, &to_sign));
    format!("AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed}, Signature={sig}", creds.access_key_id)
}

// ── Requests ─────────────────────────────────────────────────────────────────

/// Operations the app may send. Anything else is refused, so the command
/// can't be used to reach other DynamoDB APIs (tables are not created or
/// dropped from here).
const ALLOWED_OPS: &[&str] = &[
    "ListTables",
    "DescribeTable",
    "DescribeTimeToLive",
    "Scan",
    "Query",
    "GetItem",
    "PutItem",
    "UpdateItem",
    "DeleteItem",
    "BatchWriteItem",
    "TransactWriteItems",
    "ExecuteStatement",
];

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default()
    })
}

fn endpoint_url(cfg: &DynamoConfig, region: &str) -> Result<reqwest::Url, String> {
    let raw = non_empty(&cfg.endpoint).unwrap_or_else(|| format!("https://dynamodb.{region}.amazonaws.com"));
    let raw = if raw.contains("://") { raw } else { format!("https://{raw}") };
    let url = reqwest::Url::parse(&raw).map_err(|e| format!("Invalid endpoint \"{raw}\": {e}"))?;
    if url.host_str().is_none() {
        return Err(format!("Invalid endpoint \"{raw}\": no host"));
    }
    Ok(url)
}

/// `ResourceNotFoundException: Requested resource not found`, from DynamoDB's
/// error body (`__type` is `com.amazonaws...#Name`).
fn error_message(status: reqwest::StatusCode, body: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    let kind = v.get("__type").and_then(|t| t.as_str()).map(|t| t.rsplit('#').next().unwrap_or(t));
    let msg = v
        .get("message")
        .or_else(|| v.get("Message"))
        .and_then(|m| m.as_str());
    match (kind, msg) {
        (Some(k), Some(m)) => format!("{k}: {m}"),
        (Some(k), None) => k.to_string(),
        (None, Some(m)) => m.to_string(),
        (None, None) if body.trim().is_empty() => format!("DynamoDB returned HTTP {status}"),
        (None, None) => format!("DynamoDB returned HTTP {status}: {}", body.chars().take(300).collect::<String>()),
    }
}

/// One DynamoDB API call.
pub async fn request(cfg: &DynamoConfig, op: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    if !ALLOWED_OPS.contains(&op) {
        return Err(format!("DynamoDB operation \"{op}\" is not allowed"));
    }
    let region = resolve_region(cfg);
    let creds = resolve_creds(cfg).await?;
    let url = endpoint_url(cfg, &region)?;
    let host = match url.port() {
        Some(p) => format!("{}:{p}", url.host_str().unwrap_or_default()),
        None => url.host_str().unwrap_or_default().to_string(),
    };
    let payload = serde_json::to_vec(body).map_err(|e| e.to_string())?;
    let amz_date = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let target = format!("DynamoDB_20120810.{op}");
    let mut headers = vec![
        ("content-type".to_string(), "application/x-amz-json-1.0".to_string()),
        ("host".to_string(), host),
        ("x-amz-date".to_string(), amz_date.clone()),
        ("x-amz-target".to_string(), target),
    ];
    if let Some(t) = &creds.session_token {
        headers.push(("x-amz-security-token".to_string(), t.clone()));
    }
    let path = if url.path().is_empty() { "/" } else { url.path() };
    let auth = sign("POST", path, "", &headers, &payload, &creds, &region, "dynamodb", &amz_date);
    let mut req = client().post(url.clone()).header("authorization", auth).body(payload);
    for (k, v) in &headers {
        if k != "host" {
            req = req.header(k.as_str(), v.as_str());
        }
    }
    let resp = req.send().await.map_err(|e| {
        if e.is_connect() {
            format!("Could not reach {url}: {e}")
        } else if e.is_timeout() {
            format!("DynamoDB did not answer in time ({url})")
        } else {
            e.to_string()
        }
    })?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(error_message(status, &text));
    }
    if text.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(&text).map_err(|e| format!("DynamoDB sent a reply that isn't JSON: {e}"))
}

/// Connection test: one cheap signed call.
pub async fn ping(cfg: &DynamoConfig) -> Result<(), String> {
    request(cfg, "ListTables", &serde_json::json!({ "Limit": 1 })).await.map(|_| ())
}

// ── PartiQL in the query console ─────────────────────────────────────────────

/// A typed attribute as plain JSON: numbers become JSON numbers when they fit
/// exactly, sets become arrays, binary stays base64 text.
pub fn unmarshall(v: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value as J;
    let Some((t, x)) = v.as_object().and_then(|o| o.iter().next()) else {
        return v.clone();
    };
    match (t.as_str(), x) {
        ("S" | "B", J::String(s)) => J::String(s.clone()),
        ("N", J::String(n)) => number(n),
        ("BOOL", J::Bool(b)) => J::Bool(*b),
        ("NULL", _) => J::Null,
        ("SS" | "BS", J::Array(a)) => J::Array(a.clone()),
        ("NS", J::Array(a)) => J::Array(a.iter().map(|n| n.as_str().map(number).unwrap_or(J::Null)).collect()),
        ("L", J::Array(a)) => J::Array(a.iter().map(unmarshall).collect()),
        ("M", J::Object(m)) => J::Object(m.iter().map(|(k, v)| (k.clone(), unmarshall(v))).collect()),
        _ => v.clone(),
    }
}

fn number(n: &str) -> serde_json::Value {
    if let Ok(i) = n.parse::<i64>() {
        return i.into();
    }
    match n.parse::<f64>() {
        // Keep the exact text when f64 would round it.
        Ok(f) if f.is_finite() && f.to_string() == n => serde_json::json!(f),
        _ => serde_json::Value::String(n.to_string()),
    }
}

/// Rows past this many stop the NextToken loop; the message says so.
const PARTIQL_ROW_CAP: usize = 5_000;

/// Run one PartiQL statement (`SELECT`, `INSERT`, `UPDATE`, `DELETE`).
pub async fn query(cfg: &DynamoConfig, statement: &str) -> Result<super::query::SqlResult, String> {
    use super::query::{ColumnInfo, SqlResult};
    let t0 = std::time::Instant::now();
    let stmt = statement.trim().trim_end_matches(';').trim();
    if stmt.is_empty() {
        return Err("Empty statement".into());
    }
    let mut items: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();
    let mut token: Option<String> = None;
    let mut capped = false;
    loop {
        let mut body = serde_json::json!({ "Statement": stmt });
        if let Some(t) = &token {
            body["NextToken"] = t.clone().into();
        }
        let out = request(cfg, "ExecuteStatement", &body).await?;
        if let Some(arr) = out.get("Items").and_then(|v| v.as_array()) {
            items.extend(arr.iter().filter_map(|i| i.as_object().cloned()));
        }
        token = out.get("NextToken").and_then(|v| v.as_str()).map(str::to_string);
        if token.is_none() {
            break;
        }
        if items.len() >= PARTIQL_ROW_CAP {
            capped = true;
            break;
        }
    }
    // Columns in first-seen order: items in one table needn't share attributes.
    let mut cols: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for it in &items {
        for k in it.keys() {
            if seen.insert(k.clone()) {
                cols.push(k.clone());
            }
        }
    }
    let rows: Vec<Vec<serde_json::Value>> = items
        .iter()
        .map(|it| cols.iter().map(|c| it.get(c).map(unmarshall).unwrap_or(serde_json::Value::Null)).collect())
        .collect();
    let is_select = stmt.split_whitespace().next().is_some_and(|w| w.eq_ignore_ascii_case("select"));
    let message = if capped {
        Some(format!("Showing the first {} items; add a WHERE on the key to narrow it", items.len()))
    } else if !is_select {
        Some("Statement applied".into())
    } else {
        None
    };
    Ok(SqlResult {
        columns: cols.iter().map(|c| ColumnInfo::new(c.clone(), "json")).collect(),
        row_count: Some(rows.len() as i64),
        rows,
        message,
        query_ms: t0.elapsed().as_millis() as u64,
        sql: statement.to_string(),
    })
}

/// Every table name (ListTables pages through 100 at a time).
pub async fn table_names(cfg: &DynamoConfig) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    let mut start: Option<String> = None;
    loop {
        let mut body = serde_json::json!({ "Limit": 100 });
        if let Some(s) = &start {
            body["ExclusiveStartTableName"] = s.clone().into();
        }
        let out = request(cfg, "ListTables", &body).await?;
        names.extend(out["TableNames"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)));
        start = out.get("LastEvaluatedTableName").and_then(|v| v.as_str()).map(str::to_string);
        if start.is_none() {
            return Ok(names);
        }
    }
}

/// Tables for the schema cache (sidebar, console autocomplete, MCP). Item
/// counts come from DescribeTable, one call per table, so they're left unknown.
pub async fn list_tables(cfg: &DynamoConfig) -> Result<Vec<super::schema::TableInfo>, String> {
    Ok(table_names(cfg)
        .await?
        .into_iter()
        .map(|name| super::schema::TableInfo {
            name,
            // Unknown: DescribeTable per table would be one call each.
            row_count: -1,
            kind: "table".into(),
            rls_enabled: None,
            created_at: None,
            create_order: None,
        })
        .collect())
}

/// A table's shape for the tree, from one DescribeTable: the attributes the
/// table declares (its key attributes and every index's, the only ones DynamoDB
/// types up front), its partition and sort key, and its secondary indexes.
pub async fn describe_shape(
    cfg: &DynamoConfig,
    table: &str,
) -> Result<(Vec<super::schema::ColumnStructureRow>, Vec<super::schema::IndexInfo>), String> {
    let d = request(cfg, "DescribeTable", &serde_json::json!({ "TableName": table })).await?;
    Ok(shape_from_describe(table, &d["Table"]))
}

/// `S` / `N` / `B` as the words the console uses.
fn attribute_type(t: &str) -> &str {
    match t {
        "S" => "string",
        "N" => "number",
        "B" => "binary",
        other => other,
    }
}

/// The attribute names of a KeySchema, HASH first.
fn key_attrs(keys: &serde_json::Value) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = keys
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|k| Some((k["AttributeName"].as_str()?.to_string(), k["KeyType"].as_str().unwrap_or("").to_string())))
        .collect();
    out.sort_by_key(|(_, kind)| kind != "HASH");
    out
}

pub fn shape_from_describe(
    table: &str,
    t: &serde_json::Value,
) -> (Vec<super::schema::ColumnStructureRow>, Vec<super::schema::IndexInfo>) {
    use super::schema::{ColumnStructureRow, IndexInfo};
    let keys = key_attrs(&t["KeySchema"]);
    let mut attrs: Vec<(String, String)> = t["AttributeDefinitions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| Some((a["AttributeName"].as_str()?.to_string(), attribute_type(a["AttributeType"].as_str().unwrap_or("")).to_string())))
        .collect();
    // Key attributes first, in key order; the rest as declared.
    let rank = |n: &str| keys.iter().position(|(k, _)| k == n).unwrap_or(usize::MAX);
    attrs.sort_by_key(|(n, _)| rank(n));
    let columns = attrs
        .into_iter()
        .enumerate()
        .map(|(i, (name, ty))| ColumnStructureRow {
            ordinal_position: i as i32 + 1,
            is_primary_key: rank(&name) != usize::MAX,
            is_nullable: rank(&name) == usize::MAX,
            name,
            data_type: ty,
            column_default: None,
            foreign_key: None,
            fk_constraint_name: None,
            comment: None,
        })
        .collect();
    let index = |name: String, columns: String, index_type: &str| IndexInfo {
        name,
        table_name: table.to_string(),
        columns,
        index_type: index_type.to_string(),
        is_unique: false,
        is_primary: false,
        condition: None,
        comment: None,
    };
    let mut indexes = Vec::new();
    // The table's keys, as key rows (see `list_indexes_for`).
    for (name, kind) in &keys {
        let label = if kind == "HASH" { "partition key" } else { "sort key" };
        indexes.push(index(label.to_string(), name.clone(), "key"));
    }
    for (field, kind) in [("GlobalSecondaryIndexes", "gsi"), ("LocalSecondaryIndexes", "lsi")] {
        for ix in t[field].as_array().into_iter().flatten() {
            let Some(name) = ix["IndexName"].as_str() else { continue };
            let cols = key_attrs(&ix["KeySchema"]).into_iter().map(|(n, _)| n).collect::<Vec<_>>().join(", ");
            indexes.push(index(name.to_string(), cols, kind));
        }
    }
    (columns, indexes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_table_becomes_key_attributes_keys_and_indexes() {
        let t = serde_json::json!({
            "AttributeDefinitions": [
                {"AttributeName": "gsi1pk", "AttributeType": "S"},
                {"AttributeName": "sk", "AttributeType": "S"},
                {"AttributeName": "pk", "AttributeType": "S"},
                {"AttributeName": "n", "AttributeType": "N"}
            ],
            "KeySchema": [{"AttributeName": "sk", "KeyType": "RANGE"}, {"AttributeName": "pk", "KeyType": "HASH"}],
            "GlobalSecondaryIndexes": [{"IndexName": "gsi1", "KeySchema": [{"AttributeName": "gsi1pk", "KeyType": "HASH"}]}],
            "LocalSecondaryIndexes": [{"IndexName": "by_n", "KeySchema": [{"AttributeName": "pk", "KeyType": "HASH"}, {"AttributeName": "n", "KeyType": "RANGE"}]}]
        });
        let (cols, idx) = shape_from_describe("items", &t);
        assert_eq!(cols.iter().map(|c| (c.name.as_str(), c.data_type.as_str(), c.is_primary_key)).collect::<Vec<_>>(),
            [("pk", "string", true), ("sk", "string", true), ("gsi1pk", "string", false), ("n", "number", false)]);
        let rows: Vec<_> = idx.iter().map(|i| (i.name.as_str(), i.columns.as_str(), i.index_type.as_str())).collect();
        assert_eq!(rows, [("partition key", "pk", "key"), ("sort key", "sk", "key"), ("gsi1", "gsi1pk", "gsi"), ("by_n", "pk, n", "lsi")]);
        // A table without indexes or a sort key.
        let (cols, idx) = shape_from_describe("t", &serde_json::json!({"AttributeDefinitions": [{"AttributeName": "id", "AttributeType": "N"}], "KeySchema": [{"AttributeName": "id", "KeyType": "HASH"}]}));
        assert_eq!(cols.len(), 1);
        assert_eq!(idx.len(), 1);
    }

    #[test]
    fn sigv4_matches_the_aws_test_suite() {
        // aws-sig-v4-test-suite: get-vanilla.
        let creds = Creds {
            access_key_id: "AKIDEXAMPLE".into(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(),
            session_token: None,
        };
        let headers = vec![
            ("host".to_string(), "example.amazonaws.com".to_string()),
            ("x-amz-date".to_string(), "20150830T123600Z".to_string()),
        ];
        let auth = sign("GET", "/", "", &headers, b"", &creds, "us-east-1", "service", "20150830T123600Z");
        assert_eq!(
            auth,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
             SignedHeaders=host;x-amz-date, \
             Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        );
    }

    #[test]
    fn unmarshall_plain_json() {
        let v = serde_json::json!({"M": {
            "s": {"S": "x"}, "n": {"N": "42"}, "f": {"N": "1.5"}, "big": {"N": "12345678901234567890123"},
            "b": {"BOOL": true}, "z": {"NULL": true}, "ss": {"SS": ["a", "b"]}, "ns": {"NS": ["1", "2"]},
            "l": {"L": [{"S": "y"}, {"N": "3"}]}
        }});
        assert_eq!(unmarshall(&v), serde_json::json!({
            "s": "x", "n": 42, "f": 1.5, "big": "12345678901234567890123", "b": true, "z": null,
            "ss": ["a", "b"], "ns": [1, 2], "l": ["y", 3]
        }));
    }

    #[test]
    fn ini_profiles_from_both_files() {
        let cfg = parse_ini("[default]\nregion = eu-west-1\n\n[profile work]\nsso_session = x\nregion=us-east-2\n# c\n");
        assert_eq!(cfg["default"]["region"], "eu-west-1");
        assert_eq!(cfg["work"]["region"], "us-east-2");
        let creds = parse_ini("[default]\naws_access_key_id = AKIA\naws_secret_access_key = s3cr3t\n");
        assert_eq!(creds["default"]["aws_access_key_id"], "AKIA");
    }

    #[test]
    fn error_bodies_read_well() {
        let s = reqwest::StatusCode::BAD_REQUEST;
        assert_eq!(
            error_message(s, r#"{"__type":"com.amazonaws.dynamodb.v20120810#ResourceNotFoundException","message":"Requested resource not found"}"#),
            "ResourceNotFoundException: Requested resource not found"
        );
        assert_eq!(error_message(s, r#"{"__type":"x#ValidationException","Message":"bad"}"#), "ValidationException: bad");
        assert_eq!(error_message(s, ""), "DynamoDB returned HTTP 400 Bad Request");
    }

    #[tokio::test]
    async fn unknown_ops_are_refused() {
        let cfg = DynamoConfig {
            name: "t".into(),
            region: "us-east-1".into(),
            profile: None,
            access_key_id: Some("a".into()),
            secret_access_key: Some("b".into()),
            session_token: None,
            endpoint: Some("http://127.0.0.1:1".into()),
        };
        assert!(request(&cfg, "DeleteTable", &serde_json::json!({})).await.unwrap_err().contains("not allowed"));
    }

    /// Against `dev/nosql` DynamoDB Local on port 8000:
    /// `cargo test --lib dynamodb_live -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn dynamodb_live() {
        let cfg = DynamoConfig {
            name: "local".into(),
            region: "us-east-1".into(),
            profile: None,
            access_key_id: None,
            secret_access_key: None,
            session_token: None,
            endpoint: Some("http://localhost:8000".into()),
        };
        ping(&cfg).await.unwrap();
        let t = request(&cfg, "ListTables", &serde_json::json!({})).await.unwrap();
        let names: Vec<&str> = t["TableNames"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        assert!(names.contains(&"stroke-test"), "{names:?}");
        let d = request(&cfg, "DescribeTable", &serde_json::json!({"TableName": "stroke-test"})).await.unwrap();
        assert_eq!(d["Table"]["KeySchema"][0]["AttributeName"], "pk");
        let item = serde_json::json!({"pk": {"S": "IT#1"}, "sk": {"S": "A"}, "n": {"N": "5"}});
        request(&cfg, "PutItem", &serde_json::json!({"TableName": "stroke-test", "Item": item})).await.unwrap();
        let q = request(&cfg, "Query", &serde_json::json!({
            "TableName": "stroke-test",
            "KeyConditionExpression": "#pk = :pk",
            "ExpressionAttributeNames": {"#pk": "pk"},
            "ExpressionAttributeValues": {":pk": {"S": "IT#1"}},
        })).await.unwrap();
        assert_eq!(q["Items"][0]["n"]["N"], "5");
        let p = request(&cfg, "ExecuteStatement", &serde_json::json!({
            "Statement": "SELECT * FROM \"stroke-test\" WHERE pk = ?",
            "Parameters": [{"S": "IT#1"}],
        })).await.unwrap();
        assert_eq!(p["Items"].as_array().unwrap().len(), 1);
        request(&cfg, "DeleteItem", &serde_json::json!({"TableName": "stroke-test", "Key": {"pk": {"S": "IT#1"}, "sk": {"S": "A"}}})).await.unwrap();
        let err = request(&cfg, "DescribeTable", &serde_json::json!({"TableName": "nope"})).await.unwrap_err();
        assert!(err.starts_with("ResourceNotFoundException"), "{err}");
    }
}
