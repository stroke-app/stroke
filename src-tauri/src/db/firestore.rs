//! Cloud Firestore over its REST API (v1).
//!
//! Sign-in, in order of preference:
//! - `emulator`: a local emulator; any token is accepted (`Bearer owner`).
//! - `serviceAccount`: a JSON key file. The app signs a JWT with the key
//!   (RS256, `ring`) and trades it for an access token at Google's token URL.
//! - `gcloud`: the gcloud CLI's own login (`gcloud auth print-access-token`).
//!
//! Documents keep Firestore's typed values (`{"stringValue": ...}`) through
//! the IPC boundary; the frontend converts them to plain JSON for editing.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64URL, engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FirestoreConfig {
    pub name: String,
    pub project_id: String,
    /// Database id; `(default)` when blank.
    #[serde(default)]
    pub database: Option<String>,
    /// `emulator` | `serviceAccount` | `gcloud`.
    #[serde(default)]
    pub auth: String,
    /// Service account key file (JSON).
    #[serde(default)]
    pub key_file: Option<String>,
    /// Emulator `host:port`, e.g. `localhost:8080`.
    #[serde(default)]
    pub emulator_host: Option<String>,
}

fn database(cfg: &FirestoreConfig) -> String {
    cfg.database.as_deref().map(str::trim).filter(|d| !d.is_empty()).unwrap_or("(default)").to_string()
}

/// `projects/{p}/databases/{d}/documents`
pub fn documents_root(cfg: &FirestoreConfig) -> String {
    format!("projects/{}/databases/{}/documents", cfg.project_id.trim(), database(cfg))
}

fn base_url(cfg: &FirestoreConfig) -> String {
    if cfg.auth == "emulator" {
        let h = cfg.emulator_host.as_deref().map(str::trim).filter(|h| !h.is_empty()).unwrap_or("localhost:8080");
        let h = h.trim_end_matches('/');
        if h.contains("://") { format!("{h}/v1") } else { format!("http://{h}/v1") }
    } else {
        "https://firestore.googleapis.com/v1".into()
    }
}

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

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// Access tokens by credential source, with their expiry (unix seconds).
static TOKENS: OnceLock<Mutex<HashMap<String, (String, u64)>>> = OnceLock::new();

fn cached_token(key: &str) -> Option<String> {
    let map = TOKENS.get_or_init(Default::default).lock().ok()?;
    let (t, exp) = map.get(key)?;
    (now() + 120 < *exp).then(|| t.clone())
}

fn store_token(key: &str, token: &str, exp: u64) {
    if let Ok(mut map) = TOKENS.get_or_init(Default::default).lock() {
        map.insert(key.to_string(), (token.to_string(), exp));
    }
}

#[derive(Deserialize)]
struct ServiceAccount {
    client_email: String,
    private_key: String,
    #[serde(default = "default_token_uri")]
    token_uri: String,
}

fn default_token_uri() -> String {
    "https://oauth2.googleapis.com/token".into()
}

/// PKCS#8 DER from a `-----BEGIN PRIVATE KEY-----` block.
fn pem_to_der(pem: &str) -> Result<Vec<u8>, String> {
    let body: String = pem
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("-----") && !l.is_empty())
        .collect();
    B64.decode(body).map_err(|e| format!("The key file's private_key is not valid PEM: {e}"))
}

/// A signed JWT for the OAuth2 JWT-bearer grant.
pub fn sign_jwt(sa_email: &str, private_key_pem: &str, token_uri: &str, iat: u64) -> Result<String, String> {
    let header = B64URL.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
    let claims = B64URL.encode(
        json!({
            "iss": sa_email,
            "scope": "https://www.googleapis.com/auth/datastore",
            "aud": token_uri,
            "iat": iat,
            "exp": iat + 3600,
        })
        .to_string(),
    );
    let input = format!("{header}.{claims}");
    let der = pem_to_der(private_key_pem)?;
    let key = ring::signature::RsaKeyPair::from_pkcs8(&der).map_err(|e| format!("The key file's private key was rejected: {e}"))?;
    let mut sig = vec![0u8; key.public().modulus_len()];
    key.sign(&ring::signature::RSA_PKCS1_SHA256, &ring::rand::SystemRandom::new(), input.as_bytes(), &mut sig)
        .map_err(|_| "Could not sign with the service account key".to_string())?;
    Ok(format!("{input}.{}", B64URL.encode(sig)))
}

async fn service_account_token(path: &str) -> Result<String, String> {
    let key = format!("sa:{path}");
    if let Some(t) = cached_token(&key) {
        return Ok(t);
    }
    let raw = std::fs::read_to_string(path).map_err(|e| format!("Could not read the key file {path}: {e}"))?;
    let sa: ServiceAccount = serde_json::from_str(&raw).map_err(|e| format!("{path} is not a service account key: {e}"))?;
    let jwt = sign_jwt(&sa.client_email, &sa.private_key, &sa.token_uri, now())?;
    let resp = client()
        .post(&sa.token_uri)
        .form(&[("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"), ("assertion", jwt.as_str())])
        .send()
        .await
        .map_err(|e| format!("Could not reach Google's token service: {e}"))?;
    let status = resp.status();
    let v: Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!(
            "Google refused the service account: {}",
            v.get("error_description").or_else(|| v.get("error")).and_then(|e| e.as_str()).unwrap_or("unknown error")
        ));
    }
    let token = v["access_token"].as_str().ok_or("No access_token in Google's reply")?.to_string();
    store_token(&key, &token, now() + v["expires_in"].as_u64().unwrap_or(3600));
    Ok(token)
}

async fn gcloud_token() -> Result<String, String> {
    if let Some(t) = cached_token("gcloud") {
        return Ok(t);
    }
    let fut = tokio::process::Command::new("gcloud").args(["auth", "print-access-token"]).kill_on_drop(true).output();
    let out = tokio::time::timeout(Duration::from_secs(20), fut)
        .await
        .map_err(|_| "gcloud did not answer in time".to_string())?
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "The gcloud CLI is not installed; use a service account key file instead".to_string()
            } else {
                e.to_string()
            }
        })?;
    if !out.status.success() {
        return Err(format!(
            "gcloud has no login: {} Run `gcloud auth login` and try again.",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
    // print-access-token doesn't say when it expires; its tokens last an hour.
    store_token("gcloud", &token, now() + 3000);
    Ok(token)
}

async fn token(cfg: &FirestoreConfig) -> Result<String, String> {
    match cfg.auth.as_str() {
        "emulator" => Ok("owner".into()),
        "serviceAccount" => {
            let path = cfg.key_file.as_deref().map(str::trim).filter(|p| !p.is_empty()).ok_or("Pick a service account key file")?;
            service_account_token(path).await
        }
        "gcloud" => gcloud_token().await,
        other => Err(format!("Unknown sign-in \"{other}\"")),
    }
}

/// Only document paths under this database, and only document operations.
pub fn check_path(cfg: &FirestoreConfig, method: &str, path: &str) -> Result<(), String> {
    let root = documents_root(cfg);
    let p = path.trim_start_matches('/');
    if !(p == root || p.starts_with(&format!("{root}/")) || p.starts_with(&format!("{root}:"))) {
        return Err("Only this database's documents are available".into());
    }
    if p.contains("..") {
        return Err("Invalid path".into());
    }
    let rpc = p.rsplit_once(':').map(|(_, r)| r.split('?').next().unwrap_or(r));
    match (method, rpc) {
        ("GET", None) | ("PATCH", None) | ("DELETE", None) => Ok(()),
        ("POST", None) => Ok(()), // create with an auto id
        ("POST", Some("runQuery" | "listCollectionIds" | "runAggregationQuery")) => Ok(()),
        (m, Some(r)) => Err(format!("{m} :{r} is not available from Stroke")),
        (m, None) => Err(format!("HTTP method {m} is not allowed")),
    }
}

/// One Firestore call; `path` starts with `projects/...`, query string included.
pub async fn request(cfg: &FirestoreConfig, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
    if cfg.project_id.trim().is_empty() {
        return Err("Enter the Google Cloud project id".into());
    }
    check_path(cfg, method, path)?;
    let url = format!("{}/{}", base_url(cfg), path.trim_start_matches('/'));
    let m = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
    let mut req = client().request(m, &url).bearer_auth(token(cfg).await?);
    if let Some(b) = body {
        req = req.json(b);
    }
    let resp = req.send().await.map_err(|e| {
        if e.is_connect() {
            format!("Could not reach Firestore at {}: {e}", base_url(cfg))
        } else {
            e.to_string()
        }
    })?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::String(text));
    if !status.is_success() {
        // Errors arrive as {"error": {...}} or, from runQuery, as [{"error": {...}}].
        let e = v.get("error").or_else(|| v.get(0).and_then(|x| x.get("error")));
        let msg = e.and_then(|e| e.get("message")).and_then(|m| m.as_str()).unwrap_or("request failed");
        let code = e.and_then(|e| e.get("status")).and_then(|s| s.as_str()).unwrap_or("");
        return Err(if code.is_empty() { format!("Firestore: {msg}") } else { format!("{code}: {msg}") });
    }
    Ok(v)
}

/// Connection test: list the top-level collections.
pub async fn ping(cfg: &FirestoreConfig) -> Result<(), String> {
    list_collections(cfg).await.map(|_| ())
}

pub async fn list_collections(cfg: &FirestoreConfig) -> Result<Vec<String>, String> {
    let path = format!("{}:listCollectionIds", documents_root(cfg));
    let mut out = vec![];
    let mut token: Option<String> = None;
    loop {
        let mut body = json!({ "pageSize": 300 });
        if let Some(t) = &token {
            body["pageToken"] = t.clone().into();
        }
        let v = request(cfg, "POST", &path, Some(&body)).await?;
        out.extend(v["collectionIds"].as_array().into_iter().flatten().filter_map(|c| c.as_str().map(str::to_string)));
        token = v.get("nextPageToken").and_then(|t| t.as_str()).filter(|t| !t.is_empty()).map(str::to_string);
        if token.is_none() {
            return Ok(out);
        }
    }
}

// ── Console ──────────────────────────────────────────────────────────────────

/// A typed Firestore value as plain JSON.
pub fn plain(v: &Value) -> Value {
    let Some((t, x)) = v.as_object().and_then(|o| o.iter().next()) else { return v.clone() };
    match t.as_str() {
        "nullValue" => Value::Null,
        "integerValue" => x.as_str().and_then(|s| s.parse::<i64>().ok()).map(Value::from).unwrap_or_else(|| x.clone()),
        "mapValue" => Value::Object(x.get("fields").and_then(|f| f.as_object()).map(|f| f.iter().map(|(k, v)| (k.clone(), plain(v))).collect()).unwrap_or_default()),
        "arrayValue" => Value::Array(x.get("values").and_then(|a| a.as_array()).map(|a| a.iter().map(plain).collect()).unwrap_or_default()),
        _ => x.clone(),
    }
}

/// A document as plain JSON with its id under `_id`.
pub fn doc_plain(doc: &Value) -> Value {
    let id = doc["name"].as_str().and_then(|n| n.rsplit('/').next()).unwrap_or("");
    let mut o = serde_json::Map::new();
    o.insert("_id".into(), Value::String(id.into()));
    if let Some(f) = doc.get("fields").and_then(|f| f.as_object()) {
        for (k, v) in f {
            o.insert(k.clone(), plain(v));
        }
    }
    Value::Object(o)
}

/// A JSON literal as a typed Firestore value (for `where` filters).
pub fn typed(v: &Value) -> Value {
    match v {
        Value::Null => json!({ "nullValue": null }),
        Value::Bool(b) => json!({ "booleanValue": b }),
        Value::Number(n) if n.is_i64() => json!({ "integerValue": n.to_string() }),
        Value::Number(n) => json!({ "doubleValue": n.as_f64() }),
        Value::String(s) => json!({ "stringValue": s }),
        Value::Array(a) => json!({ "arrayValue": { "values": a.iter().map(typed).collect::<Vec<_>>() } }),
        Value::Object(o) => json!({ "mapValue": { "fields": o.iter().map(|(k, v)| (k.clone(), typed(v))).collect::<serde_json::Map<_, _>>() } }),
    }
}

const OPS: &[(&str, &str)] = &[
    ("==", "EQUAL"),
    ("!=", "NOT_EQUAL"),
    ("<", "LESS_THAN"),
    ("<=", "LESS_THAN_OR_EQUAL"),
    (">", "GREATER_THAN"),
    (">=", "GREATER_THAN_OR_EQUAL"),
    ("array-contains", "ARRAY_CONTAINS"),
    ("array-contains-any", "ARRAY_CONTAINS_ANY"),
    ("in", "IN"),
    ("not-in", "NOT_IN"),
];

/// A field filter from `field op value`.
pub fn field_filter(field: &str, op: &str, value: &Value) -> Result<Value, String> {
    let op_name = OPS.iter().find(|(o, _)| *o == op).map(|(_, n)| *n).ok_or_else(|| {
        format!("Unknown operator \"{op}\"; use one of {}", OPS.iter().map(|(o, _)| *o).collect::<Vec<_>>().join(" "))
    })?;
    Ok(json!({ "fieldFilter": { "field": { "fieldPath": field }, "op": op_name, "value": typed(value) } }))
}

#[derive(Debug, PartialEq)]
pub struct ConsoleQuery {
    /// Collection path, e.g. `users` or `users/u1/orders`.
    pub path: String,
    pub filters: Vec<Value>,
    pub order: Vec<(String, bool)>,
    pub limit: Option<u64>,
    pub get: Option<String>,
}

/// Parse `users.where("age", ">", 30).orderBy("age", "desc").limit(10)`,
/// `users/u1/orders.get("o1")`, or a bare collection path.
pub fn parse(stmt: &str) -> Result<ConsoleQuery, String> {
    let s = stmt.trim().trim_end_matches(';').trim();
    let usage = r#"Use users.where("age", ">", 30).orderBy("age", "desc").limit(10), users.get("u1"), or just users"#;
    let (path, rest) = match s.find('.') {
        Some(i) if s[i..].starts_with(".where(") || s[i..].starts_with(".orderBy(") || s[i..].starts_with(".limit(") || s[i..].starts_with(".get(") => (&s[..i], &s[i..]),
        Some(_) => return Err(usage.into()),
        None => (s, ""),
    };
    let path = path.trim().trim_matches('"').trim_matches('/').to_string();
    if path.is_empty() || path.contains(char::is_whitespace) || path.split('/').count() % 2 == 0 {
        return Err(format!("\"{path}\" is not a collection path. {usage}"));
    }
    let mut q = ConsoleQuery { path, filters: vec![], order: vec![], limit: None, get: None };
    let mut r = rest;
    while !r.is_empty() {
        let r2 = r.strip_prefix('.').ok_or(usage)?;
        let open = r2.find('(').ok_or(usage)?;
        let method = &r2[..open];
        // The call ends at the ')' that closes it; arguments are JSON.
        let mut depth = 0i32;
        let mut in_str = false;
        let mut esc = false;
        let mut close = None;
        for (i, c) in r2[open..].char_indices() {
            if in_str {
                if esc { esc = false } else if c == '\\' { esc = true } else if c == '"' { in_str = false }
                continue;
            }
            match c {
                '"' => in_str = true,
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close.ok_or("Unbalanced parentheses")?;
        let args: Vec<Value> = serde_json::from_str(&format!("[{}]", &r2[open + 1..close])).map_err(|e| format!("{method}() arguments aren't valid JSON: {e}"))?;
        match (method, args.as_slice()) {
            ("where", [Value::String(f), Value::String(op), v]) => q.filters.push(field_filter(f, op, v)?),
            ("orderBy", [Value::String(f)]) => q.order.push((f.clone(), false)),
            ("orderBy", [Value::String(f), Value::String(d)]) => q.order.push((f.clone(), d.eq_ignore_ascii_case("desc"))),
            ("limit", [Value::Number(n)]) => q.limit = n.as_u64(),
            ("get", [Value::String(id)]) => q.get = Some(id.clone()),
            _ => return Err(format!("Can't read .{method}(...). {usage}")),
        }
        r = &r2[close + 1..];
    }
    Ok(q)
}

/// The runQuery body for a parsed console query (or the browser's filter).
pub fn structured_query(collection_id: &str, filters: &[Value], order: &[(String, bool)], limit: Option<u64>, offset: Option<u64>) -> Value {
    let mut sq = json!({ "from": [{ "collectionId": collection_id }] });
    match filters.len() {
        0 => {}
        1 => sq["where"] = filters[0].clone(),
        _ => sq["where"] = json!({ "compositeFilter": { "op": "AND", "filters": filters } }),
    }
    if !order.is_empty() {
        sq["orderBy"] = Value::Array(order.iter().map(|(f, d)| json!({ "field": { "fieldPath": f }, "direction": if *d { "DESCENDING" } else { "ASCENDING" } })).collect());
    }
    if let Some(l) = limit {
        sq["limit"] = json!(l);
    }
    if let Some(o) = offset {
        sq["offset"] = json!(o);
    }
    json!({ "structuredQuery": sq })
}

/// The parent path and collection id of `a/b/c` → (`root/a/b`, `c`).
pub fn parent_and_id(cfg: &FirestoreConfig, path: &str) -> (String, String) {
    let root = documents_root(cfg);
    match path.rsplit_once('/') {
        Some((parent, id)) => (format!("{root}/{parent}"), id.to_string()),
        None => (root, path.to_string()),
    }
}

pub async fn query(cfg: &FirestoreConfig, stmt: &str) -> Result<super::query::SqlResult, String> {
    let t0 = std::time::Instant::now();
    let q = parse(stmt)?;
    let docs: Vec<Value> = if let Some(id) = &q.get {
        let d = request(cfg, "GET", &format!("{}/{}/{}", documents_root(cfg), q.path, urlencoding::encode(id)), None).await?;
        vec![doc_plain(&d)]
    } else {
        let (parent, coll) = parent_and_id(cfg, &q.path);
        let body = structured_query(&coll, &q.filters, &q.order, Some(q.limit.unwrap_or(1000).min(5000)), None);
        let out = request(cfg, "POST", &format!("{parent}:runQuery"), Some(&body)).await?;
        out.as_array().into_iter().flatten().filter_map(|r| r.get("document")).map(doc_plain).collect()
    };
    Ok(super::couchdb::docs_result(docs, None, stmt, t0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> FirestoreConfig {
        FirestoreConfig {
            name: "t".into(),
            project_id: "stroke-test".into(),
            database: None,
            auth: "emulator".into(),
            key_file: None,
            emulator_host: Some("localhost:8086".into()),
        }
    }

    #[test]
    fn console_parses() {
        let q = parse(r#"users.where("age", ">", 30).where("tags", "array-contains", "beta").orderBy("age", "desc").limit(5)"#).unwrap();
        assert_eq!(q.path, "users");
        assert_eq!(q.filters.len(), 2);
        assert_eq!(q.filters[0]["fieldFilter"]["op"], "GREATER_THAN");
        assert_eq!(q.filters[0]["fieldFilter"]["value"], json!({"integerValue": "30"}));
        assert_eq!(q.order, [("age".to_string(), true)]);
        assert_eq!(q.limit, Some(5));
        assert_eq!(parse(r#"users/u1/orders.get("o1")"#).unwrap().get.as_deref(), Some("o1"));
        assert_eq!(parse("products").unwrap().path, "products");
        assert!(parse("users/u1").is_err(), "a document path is not a collection");
        assert!(parse(r#"users.where("a", "~", 1)"#).unwrap_err().contains("Unknown operator"));
        assert!(parse("SELECT * FROM x").is_err());
        // A ')' inside a string argument doesn't end the call.
        assert_eq!(parse(r#"users.where("name", "==", "a)b")"#).unwrap().filters[0]["fieldFilter"]["value"], json!({"stringValue": "a)b"}));
    }

    #[test]
    fn typed_values_and_plain() {
        let v = json!({"mapValue": {"fields": {"n": {"integerValue": "7"}, "a": {"arrayValue": {"values": [{"stringValue": "x"}]}}, "z": {"nullValue": null}}}});
        assert_eq!(plain(&v), json!({"n": 7, "a": ["x"], "z": null}));
        assert_eq!(typed(&json!(1.5)), json!({"doubleValue": 1.5}));
        assert_eq!(doc_plain(&json!({"name": "projects/p/databases/(default)/documents/users/u9", "fields": {"a": {"booleanValue": true}}})), json!({"_id": "u9", "a": true}));
    }

    #[test]
    fn path_allowlist() {
        let c = cfg();
        let root = documents_root(&c);
        assert!(check_path(&c, "GET", &format!("{root}/users?pageSize=10")).is_ok());
        assert!(check_path(&c, "PATCH", &format!("{root}/users/u1")).is_ok());
        assert!(check_path(&c, "POST", &format!("{root}:runQuery")).is_ok());
        assert!(check_path(&c, "POST", &format!("{root}/users/u1:listCollectionIds")).is_ok());
        assert!(check_path(&c, "POST", &format!("{root}:batchWrite")).is_err());
        assert!(check_path(&c, "GET", "projects/other/databases/(default)/documents/x").is_err());
        assert!(check_path(&c, "DELETE", "projects/stroke-test/databases/(default)").is_err());
    }

    #[test]
    fn jwt_signs_with_a_pkcs8_key() {
        // Throwaway 2048-bit test key (not used anywhere).
        let pem = include_str!("../../tests/fixtures/test-rsa-key.pem");
        let jwt = sign_jwt("sa@x.iam.gserviceaccount.com", pem, "https://oauth2.googleapis.com/token", 1_700_000_000).unwrap();
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let claims: Value = serde_json::from_slice(&B64URL.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(claims["iss"], "sa@x.iam.gserviceaccount.com");
        assert_eq!(claims["exp"], 1_700_003_600u64);
        assert_eq!(B64URL.decode(parts[2]).unwrap().len(), 256);
        assert!(sign_jwt("x", "-----BEGIN PRIVATE KEY-----\nnope\n-----END PRIVATE KEY-----", "u", 0).is_err());
    }

    /// Against the dev/nosql Firestore emulator (port 8086):
    /// `cargo test --lib firestore_live -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn firestore_live() {
        let c = cfg();
        ping(&c).await.unwrap();
        let colls = list_collections(&c).await.unwrap();
        assert!(colls.contains(&"users".to_string()), "{colls:?}");
        let r = query(&c, r#"users.where("age", ">", 30)"#).await.unwrap();
        assert_eq!(r.rows.len(), 1, "only u1 is over 30");
        let r = query(&c, r#"products.orderBy("price", "desc").limit(3)"#).await.unwrap();
        assert_eq!(r.rows.len(), 3);
        let g = query(&c, r#"users/u1/orders.get("o1")"#).await.unwrap();
        assert_eq!(g.rows.len(), 1);
        let err = request(&c, "GET", &format!("{}/users/nope", documents_root(&c)), None).await.unwrap_err();
        assert!(err.contains("NOT_FOUND"), "{err}");
    }
}
