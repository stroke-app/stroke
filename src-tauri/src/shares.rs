//! Share links: the app encrypts in the webview (src/lib/share-crypto.js) and
//! these commands carry the ciphertext to stroke.click with the account's
//! token, which never leaves Rust. The server stores bytes it cannot read.

use serde_json::Value;

use crate::account::{base_url, http, load_token};

async fn token(app: &tauri::AppHandle) -> Result<String, String> {
    load_token(app).await.ok_or_else(|| "Sign in to Stroke to share.".to_string())
}

/// The server's error message, or a plain one for the status.
async fn error_text(resp: reqwest::Response) -> String {
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    body["error"]["message"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| format!("stroke.click answered HTTP {status}"))
}

/// Upload an encrypted share. `meta` is `{kind, expiresIn, maxViews,
/// passwordSalt}`; returns `{id, url}`.
#[tauri::command]
pub async fn share_create(app: tauri::AppHandle, meta: Value, blob: Vec<u8>) -> Result<Value, String> {
    let token = token(&app).await?;
    let resp = http()
        .post(format!("{}/api/shares", base_url()))
        .bearer_auth(token)
        .header("x-share-meta", meta.to_string())
        .header("content-type", "application/octet-stream")
        .body(blob)
        .send()
        .await
        .map_err(|e| format!("Couldn't reach stroke.click: {e}"))?;
    if !resp.status().is_success() {
        return Err(error_text(resp).await);
    }
    resp.json().await.map_err(|e| e.to_string())
}

/// The account's shares, plan and limits.
#[tauri::command]
pub async fn share_list(app: tauri::AppHandle) -> Result<Value, String> {
    let token = token(&app).await?;
    let resp = http()
        .get(format!("{}/api/shares", base_url()))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Couldn't reach stroke.click: {e}"))?;
    if !resp.status().is_success() {
        return Err(error_text(resp).await);
    }
    resp.json().await.map_err(|e| e.to_string())
}

/// Revoke a share: the link stops working at once and its content is deleted.
#[tauri::command]
pub async fn share_revoke(app: tauri::AppHandle, id: String) -> Result<(), String> {
    if !id.chars().all(|c| c.is_ascii_alphanumeric()) || id.len() != 12 {
        return Err("Invalid share id".into());
    }
    let token = token(&app).await?;
    let resp = http()
        .delete(format!("{}/api/shares/{id}", base_url()))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Couldn't reach stroke.click: {e}"))?;
    if !resp.status().is_success() {
        return Err(error_text(resp).await);
    }
    Ok(())
}
