/*!
 * Neon adapter - the reference implementation. Neon is a public PKCE OAuth
 * client and its API hands back a full `postgres://` connection URI (password
 * included), so sign-in → list projects → one-click connect works end-to-end.
 */

use super::{http, OAuthConfig, ProviderConnection, ProviderDatabase};
use serde_json::Value;

pub const OAUTH: OAuthConfig = OAuthConfig {
    // Neon doesn't offer self-serve OAuth apps (partner-only), so - like we do
    // with Cloudflare/Wrangler - we reuse the official neonctl CLI's PUBLIC PKCE
    // client. No secret, no proxy: the token exchange goes straight to Neon
    // (see Provider::is_public_client). Redirect must be 127.0.0.1/callback,
    // which neonctl's client registered (see Provider::redirect_uri).
    client_id: "neonctl",
    auth_url: "https://oauth2.neon.tech/oauth2/auth",
    token_url: "https://oauth2.neon.tech/oauth2/token",
    // `projects:update` is not optional, and it is not about writing anything.
    // Neon's scope table maps `projects:read` to Viewer and `projects:update`
    // ("Modify Projects") to Editor, and reading `/connection_uri` hands back the
    // role's password - so Neon gates it behind Editor. Without this scope the
    // whole flow works right up to the last call and then fails with
    // "Editor access is required to read project credentials", which reads like a
    // permissions problem on the user's account rather than a missing scope on
    // ours. Neon's own documented example request asks for `update` too.
    //
    // Adding a scope does not upgrade a token that was already issued: anyone
    // signed in before this must disconnect and authorize again.
    scopes: "openid offline offline_access urn:neoncloud:projects:read urn:neoncloud:projects:create urn:neoncloud:projects:update urn:neoncloud:orgs:read",
};

const API: &str = "https://console.neon.tech/api/v2";

async fn get(token: &str, path: &str) -> Result<Value, String> {
    let resp = http()
        .get(format!("{API}{path}"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Neon request failed: {e}"))?;
    let status = resp.status().as_u16();
    // Before reading the body: a 401 may not be JSON, and the caller refreshes on it.
    if status == 401 {
        return Err(super::UNAUTHORIZED.into());
    }
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Neon read failed: {e}"))?;
    let body: Value = serde_json::from_str(&text).map_err(|_| {
        let snippet: String = text.chars().take(200).collect();
        format!("Neon non-JSON response from {path} (HTTP {status}): {snippet}")
    })?;
    if status != 200 {
        let msg = body["message"].as_str().unwrap_or("request failed");
        return Err(format!("Neon API error ({status}) on {path}: {msg}"));
    }
    Ok(body)
}

/// Each Neon project is presented as one connectable database. Neon scopes
/// projects under organizations, so `/projects` requires an `org_id` - list the
/// user's orgs first, then each org's projects, and flatten them.
pub async fn list_databases(token: &str) -> Result<Vec<ProviderDatabase>, String> {
    let orgs_body = get(token, "/users/me/organizations").await?;

    let mut out = Vec::new();
    for org in orgs_body["organizations"].as_array().into_iter().flatten() {
        let Some(org_id) = org["id"].as_str() else { continue };
        let body = get(token, &format!("/projects?org_id={}", urlencoding::encode(org_id))).await?;
        for p in body["projects"].as_array().into_iter().flatten() {
            if let Some(id) = p["id"].as_str() {
                out.push(ProviderDatabase {
                    db_ref: id.to_string(),
                    name: p["name"].as_str().unwrap_or(id).to_string(),
                    region: p["region_id"].as_str().map(String::from),
                    kind: Some("Project".into()),
                    host: None,
                });
            }
        }
    }
    Ok(out)
}

/// Resolve a project → default branch → its first database + owner role →
/// connection URI. Databases/roles live under a branch in Neon, not the project.
pub async fn build_connection(token: &str, project_id: &str) -> Result<ProviderConnection, String> {
    // Find the project's default branch (fall back to the first).
    let branches = get(token, &format!("/projects/{project_id}/branches")).await?;
    let branch_list = branches["branches"].as_array().cloned().unwrap_or_default();
    let branch = branch_list
        .iter()
        .find(|b| b["default"].as_bool() == Some(true) || b["primary"].as_bool() == Some(true))
        .or_else(|| branch_list.first())
        .ok_or("This Neon project has no branches")?;
    let branch_id = branch["id"].as_str().ok_or("Neon: missing branch id")?;

    let dbs = get(token, &format!("/projects/{project_id}/branches/{branch_id}/databases")).await?;
    let first = dbs["databases"]
        .as_array()
        .and_then(|a| a.first())
        .ok_or("This Neon branch has no databases yet")?;
    let db_name = first["name"].as_str().ok_or("Neon: missing database name")?;
    let role = first["owner_name"].as_str().ok_or("Neon: missing owner role")?;

    let uri_body = get(
        token,
        &format!(
            "/projects/{project_id}/connection_uri?branch_id={}&database_name={}&role_name={}&pooled=true",
            urlencoding::encode(branch_id),
            urlencoding::encode(db_name),
            urlencoding::encode(role),
        ),
    )
    .await?;
    let uri = uri_body["uri"]
        .as_str()
        .ok_or("Neon did not return a connection URI")?;

    let (host, port, username, password, database) = parse_pg_uri(uri)?;
    Ok(ProviderConnection {
        db_type: "postgres".into(),
        host,
        port,
        username,
        password,
        database,
        ssl: true,
        needs_password: false,
        name: format!("Neon · {db_name}"),
    })
}

/// Parse `postgres://user:pass@host[:port]/db?query` into its parts (URL-decoding
/// the userinfo, which Neon percent-encodes).
pub(crate) fn parse_pg_uri(uri: &str) -> Result<(String, u16, String, String, String), String> {
    let rest = uri.split("://").nth(1).ok_or("Malformed connection URI")?;
    let (creds_host, db_and_query) = rest.split_once('/').ok_or("Connection URI has no database")?;
    let (creds, hostport) = creds_host
        .rsplit_once('@')
        .ok_or("Connection URI has no credentials")?;
    let (user_raw, pass_raw) = creds.split_once(':').unwrap_or((creds, ""));

    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => {
            (h.to_string(), p.parse().unwrap_or(5432))
        }
        _ => (hostport.to_string(), 5432),
    };
    let database = db_and_query
        .split(['?', '&'])
        .next()
        .unwrap_or("")
        .to_string();

    let dec = |s: &str| urlencoding::decode(s).map(|c| c.into_owned()).unwrap_or_else(|_| s.to_string());
    Ok((host, port, dec(user_raw), dec(pass_raw), dec(&database)))
}
