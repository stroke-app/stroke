/*!
 * Supabase adapter - sign in via the Management API (OAuth2 + PKCE) and list
 * every project. The database password is never exposed by the API for security,
 * so `build_connection` prefills host/port/user/database and sets
 * `needs_password = true`; the UI prompts for the password once.
 */

use super::{http, OAuthConfig, ProviderConnection, ProviderDatabase};
use serde_json::Value;

pub const OAUTH: OAuthConfig = OAuthConfig {
    // Public client_id (safe to embed). The client SECRET is held only by the
    // stroke.click proxy as SUPABASE_CLIENT_SECRET - never in this binary.
    client_id: "77c997a8-fd73-4eff-8b6b-1123182ef16a",
    auth_url: "https://api.supabase.com/v1/oauth/authorize",
    // token_url is used by the proxy, not the app (the app posts to TOKEN_PROXY).
    token_url: "https://api.supabase.com/v1/oauth/token",
    scopes: "all",
};

const API: &str = "https://api.supabase.com/v1";

async fn get(token: &str, path: &str) -> Result<Value, String> {
    let resp = http()
        .get(format!("{API}{path}"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Supabase request failed: {e}"))?;
    let status = resp.status().as_u16();
    // Before reading the body: a 401 may not be JSON, and the caller refreshes on it.
    if status == 401 {
        return Err(super::UNAUTHORIZED.into());
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("Supabase: bad JSON: {e}"))?;
    if status != 200 {
        let msg = body["message"].as_str().unwrap_or("request failed");
        return Err(format!("Supabase API error ({status}): {msg}"));
    }
    Ok(body)
}

pub async fn list_databases(token: &str) -> Result<Vec<ProviderDatabase>, String> {
    // /v1/projects returns a top-level array.
    let body = get(token, "/projects").await?;
    Ok(body
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let id = p["id"].as_str()?; // project ref
            Some(ProviderDatabase {
                db_ref: id.to_string(),
                name: p["name"].as_str().unwrap_or(id).to_string(),
                region: p["region"].as_str().map(String::from),
                kind: Some("Project".into()),
                host: p["database"]["host"].as_str().map(String::from),
            })
        })
        .collect())
}

pub async fn build_connection(token: &str, project_ref: &str) -> Result<ProviderConnection, String> {
    // The project and its pooler config are independent reads: fetch both at
    // once instead of one after the other (one round trip to Supabase's API
    // instead of two). A paused project answers the pooler call with `200 []`,
    // so the status check below still decides what to say.
    let (proj_path, pooler_path) = (
        format!("/projects/{project_ref}"),
        format!("/projects/{project_ref}/config/database/pooler"),
    );
    let (proj, pooler) = tokio::join!(get(token, &proj_path), get(token, &pooler_path));
    let proj = proj?;
    let name = proj["name"].as_str().unwrap_or(project_ref).to_string();
    status_error(&proj, &name)?;

    let pooler = pooler.map_err(|e| {
        if e.contains("scope") || e.contains("403") {
            "Your Supabase OAuth app is missing the \"Database Pooling Config: Read\" scope, \
             which Stroke needs to find the correct pooler host. Fix: in the Supabase \
             dashboard open your OAuth app → Scopes → enable Database Pooling Config (Read) \
             (enabling all Read scopes is fine), save, then Disconnect here and sign in again \
             to re-authorize."
                .to_string()
        } else {
            format!("Couldn't fetch Supabase pooler config (needed for a working host): {e}")
        }
    })?;
    let (host, user) = pooler_target(&proj, &pooler, project_ref, &name)?;

    Ok(ProviderConnection {
        db_type: "postgres".into(),
        host,
        port: 5432, // session mode - normal Postgres semantics, best for a GUI
        username: user,
        password: String::new(),
        database: "postgres".into(),
        ssl: true,
        needs_password: true, // Supabase never returns the DB password
        name: format!("Supabase · {name}"),
    })
}

/// Say what is actually wrong before asking for a pooler that cannot exist.
///
/// A project has to be running to have a Supavisor tenant, and a free-tier
/// project pauses itself after a week of inactivity. The pooler endpoint then
/// answers `200 []` rather than an error, so the failure surfaced as
/// "Supabase didn't return a pooler host for this project. Pooler config: []"
/// - a dump of an empty array, which tells the user nothing they can act on
/// when the real answer is "your project is asleep, go and wake it".
fn status_error(proj: &Value, name: &str) -> Result<(), String> {
    let Some(status) = proj["status"].as_str() else { return Ok(()) };
    match status {
        "ACTIVE_HEALTHY" | "ACTIVE_UNHEALTHY" | "UNKNOWN" => Ok(()),
        "INACTIVE" | "PAUSING" | "PAUSE_FAILED" => Err(format!(
            "{name} is paused, so it has no pooler to connect to. Restore it from your \
             Supabase dashboard, wait for it to come up, then try again."
        )),
        "COMING_UP" | "RESTORING" | "RESTARTING" | "RESIZING" | "UPGRADING" => Err(format!(
            "{name} is still starting up ({status}). Give it a moment and try again."
        )),
        "INIT_FAILED" | "RESTORE_FAILED" => Err(format!(
            "{name} failed to start ({status}). Check the project in your Supabase \
             dashboard - there is nothing to connect to until it comes up."
        )),
        "GOING_DOWN" | "REMOVED" => Err(format!("{name} is being removed ({status}).")),
        other => Err(format!(
            "{name} is not running ({other}). Check the project in your Supabase dashboard."
        )),
    }
}

/// Which host and user to connect with.
///
/// The Supavisor pooler, NOT the direct host: `db.<ref>.supabase.co:5432` is
/// IPv6-only, so it fails with "network unreachable" on the many networks
/// without IPv6, while the shared pooler is IPv4-compatible on every tier. Its
/// username embeds the project ref (`postgres.<ref>`), and session mode (5432)
/// behaves like a normal Postgres connection. The host comes from the pooler
/// config, never guessed: a wrong region prefix (aws-0 vs aws-1) reaches a
/// pooler node without this tenant ("tenant/user … not found").
///
/// Running but with no Supavisor config: fall back to the direct host rather
/// than refuse outright. It is IPv6-only unless the project has the IPv4
/// add-on, so it can still fail on an IPv4-only network, but a connection that
/// might work beats an error that certainly doesn't.
fn pooler_target(proj: &Value, pooler: &Value, project_ref: &str, name: &str) -> Result<(String, String), String> {
    let obj = pooler.as_array().and_then(|a| a.first()).unwrap_or(pooler);
    let mut host = String::new();
    let mut user = format!("postgres.{project_ref}");
    // The connection_string carries the exact pooler host + tenant user.
    if let Some(cs) = obj["connection_string"].as_str().or_else(|| obj["connectionString"].as_str()) {
        if let Ok((h, _p, u, _pw, _d)) = super::neon::parse_pg_uri(cs) {
            if !h.is_empty() {
                host = h;
            }
            if !u.is_empty() {
                user = u;
            }
        }
    }
    if host.is_empty() {
        if let Some(h) = obj["db_host"].as_str() {
            host = h.to_string();
        }
    }
    if let Some(u) = obj["db_user"].as_str().filter(|u| !u.is_empty()) {
        user = u.to_string();
    }
    if host.is_empty() {
        if let Some(direct) = proj["database"]["host"].as_str().filter(|h| !h.is_empty()) {
            host = direct.to_string();
            user = "postgres".into();
        }
    }
    if host.is_empty() {
        let shape: String = serde_json::to_string(obj).unwrap_or_default().chars().take(400).collect();
        return Err(format!(
            "Supabase returned no pooler and no database host for {name}. Pooler config: {shape}"
        ));
    }
    Ok((host, user))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn paused_and_starting_projects_say_so() {
        assert!(status_error(&json!({ "status": "ACTIVE_HEALTHY" }), "p").is_ok());
        assert!(status_error(&json!({}), "p").is_ok());
        assert!(status_error(&json!({ "status": "INACTIVE" }), "timeline").unwrap_err().starts_with("timeline is paused"));
        assert!(status_error(&json!({ "status": "RESTORING" }), "p").unwrap_err().contains("still starting"));
    }

    #[test]
    fn pooler_host_and_tenant_user_come_from_the_connection_string() {
        let pooler = json!([{ "connection_string": "postgresql://postgres.abcd:[YOUR-PASSWORD]@aws-1-ap-southeast-2.pooler.supabase.com:5432/postgres", "db_host": "ignored" }]);
        assert_eq!(
            pooler_target(&json!({}), &pooler, "abcd", "p").unwrap(),
            ("aws-1-ap-southeast-2.pooler.supabase.com".into(), "postgres.abcd".into())
        );
    }

    #[test]
    fn falls_back_to_db_host_then_the_direct_host() {
        let by_field = json!([{ "db_host": "aws-0-us-east-1.pooler.supabase.com", "db_user": "postgres.abcd" }]);
        assert_eq!(pooler_target(&json!({}), &by_field, "abcd", "p").unwrap().0, "aws-0-us-east-1.pooler.supabase.com");
        let proj = json!({ "database": { "host": "db.abcd.supabase.co" } });
        assert_eq!(pooler_target(&proj, &json!([]), "abcd", "p").unwrap(), ("db.abcd.supabase.co".into(), "postgres".into()));
        assert!(pooler_target(&json!({}), &json!([]), "abcd", "p").unwrap_err().contains("no pooler"));
    }
}
