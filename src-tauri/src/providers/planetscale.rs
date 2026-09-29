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
    // Two organization scopes, and they are different permissions:
    // `read_organizations` (user access) lists the orgs a user belongs to, which
    // is the first call the picker makes - without it `/organizations` answers
    // 403 "User does not have permission". `read_organization` (org access)
    // reads a single org. Connecting creates a branch password,
    // and PlanetScale's API reference requires `manage_passwords` for that, plus
    // `manage_production_branch_passwords` when the branch is production (the
    // default branch usually is). `write_databases` alone signs in and lists
    // fine, then fails on the first connect. Sent space-separated, WITHOUT PKCE
    // (see Provider::uses_pkce). Every scope here must also be ticked on the
    // OAuth app, or the authorize page rejects the request.
    scopes: "read_user read_organizations read_organization read_databases write_databases manage_passwords manage_production_branch_passwords",
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
    let text = resp
        .text()
        .await
        .map_err(|e| format!("PlanetScale read failed: {e}"))?;
    let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if status != 200 {
        // Say which call and what PlanetScale said. "PlanetScale API error (403)"
        // alone gave no way to tell a missing scope from an org that was never
        // granted to this app.
        let msg = body["message"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| text.chars().take(160).collect());
        return Err(format!("PlanetScale API error ({status}) on {path}: {msg}"));
    }
    if body.is_null() {
        return Err(format!("PlanetScale returned a non-JSON response for {path}"));
    }
    Ok(body)
}

/// Organizations the last listing saw, per token, so the next listing can ask
/// for every org's databases at the same moment it re-checks the org list:
/// one round trip in the common case instead of two in a row (3.1s measured).
static ORGS: std::sync::Mutex<Option<(String, Vec<String>)>> = std::sync::Mutex::new(None);

/// Default branch per "{org}/{database}", from the listing, so connecting
/// skips the database lookup and goes straight to minting the password.
static BRANCHES: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, String>>> =
    std::sync::OnceLock::new();

fn branches() -> &'static std::sync::Mutex<std::collections::HashMap<String, String>> {
    BRANCHES.get_or_init(Default::default)
}

async fn org_names(token: &str) -> Result<Vec<String>, String> {
    let orgs = get(token, "/organizations").await?;
    Ok(orgs["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|org| org["name"].as_str().map(String::from))
        .collect())
}

/// Every org's database list in flight at once rather than one after another.
async fn org_pages(token: &str, orgs: &[String]) -> Vec<Result<Value, String>> {
    futures::future::join_all(orgs.iter().map(|org_name| {
        let path = format!("/organizations/{org_name}/databases");
        async move { get(token, &path).await }
    }))
    .await
}

/// db_ref encodes "{org}/{database}" so build_connection can act without a
/// second lookup.
pub async fn list_databases(token: &str) -> Result<Vec<ProviderDatabase>, String> {
    let known = ORGS
        .lock()
        .ok()
        .and_then(|g| g.as_ref().filter(|(t, _)| t == token).map(|(_, o)| o.clone()));
    let (org_list, mut pages) = match known {
        // Re-check the org list and fetch the known orgs' databases together.
        Some(known) => {
            let (fresh, pages) = tokio::join!(org_names(token), org_pages(token, &known));
            let fresh = fresh?;
            let mut by_org: std::collections::HashMap<String, Result<Value, String>> =
                known.into_iter().zip(pages).collect();
            // An org that appeared since: fetch it now (rare).
            let added: Vec<String> = fresh.iter().filter(|o| !by_org.contains_key(*o)).cloned().collect();
            for (org, page) in added.iter().cloned().zip(org_pages(token, &added).await) {
                by_org.insert(org, page);
            }
            let pages = fresh.iter().map(|o| by_org.remove(o).unwrap_or_else(|| Err("missing".into()))).collect();
            (fresh, pages)
        }
        None => {
            let orgs = org_names(token).await?;
            let pages = org_pages(token, &orgs).await;
            (orgs, pages)
        }
    };
    if let Ok(mut g) = ORGS.lock() {
        *g = Some((token.to_string(), org_list.clone()));
    }
    // `/organizations` lists every org the user belongs to, but the token only
    // covers the ones picked on the consent screen: the rest answer 403.
    let pages = super::merge_partial(
        org_list.iter().map(String::as_str).zip(pages.drain(..)).map(|(org, r)| r.map(|b| (org, b))).collect(),
        "Stroke may not have been granted this organization: sign out, sign in again, and select it on PlanetScale's consent screen.",
    )?;
    if let Ok(mut map) = branches().lock() {
        for (org, dbs) in &pages {
            for db in dbs["data"].as_array().into_iter().flatten() {
                if let (Some(name), Some(branch)) = (db["name"].as_str(), db["default_branch"].as_str()) {
                    map.insert(format!("{org}/{name}"), branch.to_string());
                }
            }
        }
    }
    Ok(parse_databases(&pages))
}

/// `/organizations/{org}/databases` pages → picker rows. `db_ref` is
/// "{org}/{database}" so build_connection can act without a second lookup.
fn parse_databases(pages: &[(&str, Value)]) -> Vec<ProviderDatabase> {
    let mut out = Vec::new();
    for (org_name, dbs) in pages {
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
    out
}

pub async fn build_connection(token: &str, db_ref: &str) -> Result<ProviderConnection, String> {
    let (org, database) = db_ref
        .split_once('/')
        .ok_or("Invalid PlanetScale database reference")?;

    // Default branch (known from the listing, else looked up), then mint a
    // password on it.
    let cached = branches().lock().ok().and_then(|m| m.get(db_ref).cloned());
    let branch = match cached {
        Some(b) => b,
        None => {
            let db = get(token, &format!("/organizations/{org}/databases/{database}")).await?;
            db["default_branch"].as_str().unwrap_or("main").to_string()
        }
    };

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

    minted_connection(&body, database)
}

/// The password-create response → a connection. PlanetScale returns the
/// plaintext password exactly once, here, and the host either at the top level
/// or under `database_branch` depending on the API version.
fn minted_connection(body: &Value, database: &str) -> Result<ProviderConnection, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn databases_carry_their_org_in_the_ref() {
        let page = json!({ "data": [
            { "name": "app", "region": { "slug": "aws-ap-south-1" } },
            { "name": "logs" },
            { "region": { "slug": "no-name-is-skipped" } }
        ]});
        let rows = parse_databases(&[("acme", page)]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].db_ref, "acme/app");
        assert_eq!(rows[0].region.as_deref(), Some("aws-ap-south-1"));
        assert_eq!(rows[1].region, None);
    }

    #[test]
    fn minted_password_reads_either_host_location() {
        let top = json!({ "username": "u", "plain_text": "p", "access_host_url": "aws.connect.psdb.cloud" });
        let nested = json!({ "username": "u", "plain_text": "p", "database_branch": { "access_host_url": "h2" } });
        assert_eq!(minted_connection(&top, "app").unwrap().host, "aws.connect.psdb.cloud");
        assert_eq!(minted_connection(&nested, "app").unwrap().host, "h2");
        assert!(minted_connection(&json!({ "username": "u" }), "app").is_err());
    }
}
