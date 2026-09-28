/*!
 * PlanetScale adapter - MySQL/Vitess. Sign in via OAuth, list databases across
 * the account's organizations, and mint fresh connection credentials on connect
 * (PlanetScale returns a password exactly once, at password-creation time).
 */

use super::{http, OAuthConfig, ProviderConnection, ProviderDatabase};
use serde_json::Value;

pub const OAUTH: OAuthConfig = OAuthConfig {
    // Public client_id (safe to embed). The client SECRET lives only on the
    // stroke.click proxy as PLANETSCALE_CLIENT_SECRET - never in this binary.
    client_id: "pscale_app_1c8af8139904805f4506cfb88a4f9967",
    auth_url: "https://auth.planetscale.com/oauth/authorize",
    // token_url is used by the proxy, not the app (the app posts to TOKEN_PROXY).
    token_url: "https://auth.planetscale.com/oauth/token",
    // Exact scope set PlanetScale's own CLI uses (proven valid). NOTE the real
    // scope strings differ from the dashboard checkbox labels: it's
    // `read_organization` (singular), and password creation is covered by
    // `write_databases` (there is no `manage_passwords` OAuth scope). Sent
    // space-separated, WITHOUT PKCE (see Provider::uses_pkce). Each must be
    // enabled on the OAuth app.
    scopes: "read_databases write_databases read_user read_organization",
};

const API: &str = "https://api.planetscale.com/v1";

async fn get(token: &str, path: &str) -> Result<Value, String> {
    let resp = http()
        .get(format!("{API}{path}"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("PlanetScale request failed: {e}"))?;
    let status = resp.status().as_u16();
    // Before reading the body: a 401 may not be JSON, and the caller refreshes on it.
    if status == 401 {
        return Err(super::UNAUTHORIZED.into());
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("PlanetScale: bad JSON: {e}"))?;
    if status != 200 {
        return Err(format!("PlanetScale API error ({status})"));
    }
    Ok(body)
}

/// db_ref encodes "{org}/{database}" so build_connection can act without a
/// second lookup.
pub async fn list_databases(token: &str) -> Result<Vec<ProviderDatabase>, String> {
    let orgs = get(token, "/organizations").await?;
    let mut out = Vec::new();
    for org in orgs["data"].as_array().into_iter().flatten() {
        let Some(org_name) = org["name"].as_str() else { continue };
        let dbs = get(token, &format!("/organizations/{org_name}/databases")).await?;
        for db in dbs["data"].as_array().into_iter().flatten() {
            let Some(name) = db["name"].as_str() else { continue };
            out.push(ProviderDatabase {
                db_ref: format!("{org_name}/{name}"),
                name: name.to_string(),
                region: db["region"]["slug"].as_str().map(String::from),
                kind: Some("Database".into()),
                host: None,
            });
        }
    }
    Ok(out)
}

pub async fn build_connection(token: &str, db_ref: &str) -> Result<ProviderConnection, String> {
    let (org, database) = db_ref
        .split_once('/')
        .ok_or("Invalid PlanetScale database reference")?;

    // Default branch, then mint a password on it.
    let db = get(token, &format!("/organizations/{org}/databases/{database}")).await?;
    let branch = db["default_branch"].as_str().unwrap_or("main");

    let resp = http()
        .post(format!(
            "{API}/organizations/{org}/databases/{database}/branches/{branch}/passwords"
        ))
        .bearer_auth(token)
        .json(&serde_json::json!({ "name": "stroke", "role": "admin" }))
        .send()
        .await
        .map_err(|e| format!("PlanetScale password create failed: {e}"))?;
    let status = resp.status().as_u16();
    // Before reading the body: a 401 may not be JSON, and the caller refreshes on it.
    if status == 401 {
        return Err(super::UNAUTHORIZED.into());
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("PlanetScale: bad JSON: {e}"))?;
    if !(200..300).contains(&status) {
        return Err(format!("PlanetScale could not create credentials ({status})"));
    }

    let username = body["username"].as_str().ok_or("PlanetScale: missing username")?;
    let password = body["plain_text"].as_str().ok_or("PlanetScale: missing password")?;
    let host = body["access_host_url"]
        .as_str()
        .or_else(|| body["database_branch"]["access_host_url"].as_str())
        .ok_or("PlanetScale: missing host")?;

    Ok(ProviderConnection {
        db_type: "mysql".into(),
        host: host.to_string(),
        port: 3306,
        username: username.to_string(),
        password: password.to_string(),
        database: database.to_string(),
        ssl: true,
        needs_password: false,
        name: format!("PlanetScale · {database}"),
    })
}
