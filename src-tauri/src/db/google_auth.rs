//! Signing in to Google Cloud APIs, shared by Firestore, Spanner and BigQuery.
//!
//! - `emulator`: a local emulator; any token is accepted (`Bearer owner`).
//! - `serviceAccount`: a JSON key file. The app signs a JWT with the key
//!   (RS256, `ring`) and trades it for an access token at Google's token URL.
//! - `gcloud`: the gcloud CLI's own login (`gcloud auth print-access-token`).
//!
//! Tokens are cached per credential and scope until two minutes before they
//! expire.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64URL, engine::general_purpose::STANDARD as B64, Engine as _};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Firestore's scope.
pub const SCOPE_DATASTORE: &str = "https://www.googleapis.com/auth/datastore";
/// Spanner and BigQuery: data and schema.
pub const SCOPE_CLOUD_PLATFORM: &str = "https://www.googleapis.com/auth/cloud-platform";

fn client() -> &'static reqwest::Client {
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default()
    })
}

pub(crate) fn now() -> u64 {
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
pub fn sign_jwt(sa_email: &str, private_key_pem: &str, token_uri: &str, iat: u64, scope: &str) -> Result<String, String> {
    let header = B64URL.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
    let claims = B64URL.encode(
        json!({
            "iss": sa_email,
            "scope": scope,
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

async fn service_account_token(path: &str, scope: &str) -> Result<String, String> {
    let key = format!("sa:{scope}:{path}");
    if let Some(t) = cached_token(&key) {
        return Ok(t);
    }
    let raw = std::fs::read_to_string(path).map_err(|e| format!("Could not read the key file {path}: {e}"))?;
    let sa: ServiceAccount = serde_json::from_str(&raw).map_err(|e| format!("{path} is not a service account key: {e}"))?;
    let jwt = sign_jwt(&sa.client_email, &sa.private_key, &sa.token_uri, now(), scope)?;
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
    let mut cmd = tokio::process::Command::new("gcloud");
    cmd.args(["auth", "print-access-token"]).kill_on_drop(true);
    crate::proc::quiet(&mut cmd);
    let fut = cmd.output();
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

/// An access token for `auth` ("emulator", "serviceAccount" or "gcloud"). The
/// gcloud login carries its own scopes; a service account asks for `scope`.
pub async fn token(auth: &str, key_file: Option<&str>, scope: &str) -> Result<String, String> {
    match auth {
        "emulator" => Ok("owner".into()),
        "serviceAccount" => {
            let path = key_file.map(str::trim).filter(|p| !p.is_empty()).ok_or("Pick a service account key file")?;
            service_account_token(path, scope).await
        }
        "gcloud" => gcloud_token().await,
        other => Err(format!("Unknown sign-in \"{other}\"")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bad_key_is_refused() {
        assert!(sign_jwt("x", "-----BEGIN PRIVATE KEY-----\nnope\n-----END PRIVATE KEY-----", "u", 0, SCOPE_CLOUD_PLATFORM).is_err());
    }
}
