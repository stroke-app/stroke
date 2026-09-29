/*!
 * Prisma Postgres adapter - Prisma's Management API OAuth (authorization-code,
 * confidential client). Sign in, list projects, and connect using the direct
 * Postgres credentials (`ppgDirectConnection`) the API exposes - no pasted
 * connection string, and never the Accelerate `prisma+postgres://` form (which
 * a normal Postgres driver can't speak).
 */

use super::{http, OAuthConfig, ProviderConnection, ProviderDatabase};
use serde_json::Value;

pub const OAUTH: OAuthConfig = OAuthConfig {
    // Public client_id (safe to embed). The client SECRET lives only on the
    // stroke.click proxy as PRISMA_CLIENT_SECRET - never in this binary.
    client_id: "cmr8zjp5z012evvf7cycv88xn",
    auth_url: "https://auth.prisma.io/authorize",
    // token_url is used by the proxy, not the app (the app posts to TOKEN_PROXY).
    token_url: "https://auth.prisma.io/token",
    // `offline_access` is REQUIRED to receive a refresh token - without it the
    // access token dies after 1h and every Management-API call 401s (the app
    // still shows "connected" because the dead access token is on disk).
    scopes: "workspace:admin offline_access",
};

const API: &str = "https://api.prisma.io/v1";

async fn get(token: &str, path: &str) -> Result<Value, String> {
    let resp = http()
        .get(format!("{API}{path}"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Prisma request failed: {e}"))?;
    let status = resp.status().as_u16();
    // Before reading the body: a 401 may not be JSON, and the caller refreshes on it.
    if status == 401 {
        return Err(super::UNAUTHORIZED.into());
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("Prisma: bad JSON: {e}"))?;
    if status != 200 {
        return Err(api_error(status, &body));
    }
    Ok(body)
}

/// Format a Management-API error. A 401 never gets here: `get`/`post` return
/// `UNAUTHORIZED` for it so the command layer can refresh and retry.
fn api_error(status: u16, body: &Value) -> String {
    let msg = body["message"]
        .as_str()
        .or_else(|| body["error"].as_str())
        .unwrap_or("request failed");
    format!("Prisma API error ({status}): {msg}")
}

async fn post(token: &str, path: &str, body: serde_json::Value) -> Result<Value, String> {
    let resp = http()
        .post(format!("{API}{path}"))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Prisma request failed: {e}"))?;
    let status = resp.status().as_u16();
    // Before reading the body: a 401 may not be JSON, and the caller refreshes on it.
    if status == 401 {
        return Err(super::UNAUTHORIZED.into());
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("Prisma: bad JSON: {e}"))?;
    if !(200..300).contains(&status) {
        return Err(api_error(status, &body));
    }
    Ok(body)
}

/// The projects array may live under `data`, `projects`, `items`, or be bare.
fn as_list(body: &Value) -> Vec<Value> {
    for key in ["data", "projects", "items"] {
        if let Some(a) = body[key].as_array() {
            return a.clone();
        }
    }
    body.as_array().cloned().unwrap_or_default()
}

pub async fn list_databases(token: &str) -> Result<Vec<ProviderDatabase>, String> {
    let body = get(token, "/projects").await?;
    Ok(as_list(&body)
        .iter()
        .filter_map(|p| {
            let id = p["id"].as_str()?;
            Some(ProviderDatabase {
                db_ref: id.to_string(),
                name: p["name"].as_str().unwrap_or(id).to_string(),
                region: p["region"].as_str().map(String::from),
                kind: Some("Project".into()),
                host: None,
            })
        })
        .collect())
}

/// Recursively search the whole response tree for a `ppgDirectConnection`
/// object, wherever Prisma nests it (project → environments → database → …).
fn find_direct(v: &Value) -> Option<(String, String, String, String)> {
    let creds = |obj: &Value, host: &str| {
        let user = obj["username"]
            .as_str()
            .or_else(|| obj["user"].as_str())
            .unwrap_or("")
            .to_string();
        let pass = obj["password"]
            .as_str()
            .or_else(|| obj["pass"].as_str())
            .or_else(|| obj["apiKey"].as_str())
            .or_else(|| obj["api_key"].as_str())
            .unwrap_or("")
            .to_string();
        // Only accept if we actually have a password to authenticate with.
        (!pass.is_empty()).then(|| (host.to_string(), user, pass, "postgres".to_string()))
    };

    // A directConnection / ppgDirectConnection object carrying host + creds.
    for key in ["directConnection", "ppgDirectConnection"] {
        if let Some(d) = v.get(key).filter(|d| d.is_object()) {
            if let Some(host) = d["host"].as_str() {
                if let Some(found) = creds(d, host) {
                    return Some(found);
                }
            }
        }
    }
    // Current shape: a connection object with endpoints.direct = { host, port }.
    // Credentials live on the connection object alongside `endpoints`.
    if let Some(direct) = v.get("endpoints").and_then(|e| e.get("direct")) {
        if let Some(host) = direct["host"].as_str() {
            if let Some(found) = creds(v, host) {
                return Some(found);
            }
        }
    }
    match v {
        Value::Object(map) => map.values().find_map(find_direct),
        Value::Array(arr) => arr.iter().find_map(find_direct),
        _ => None,
    }
}

/// Fallback: any usable direct `postgres://` connection string anywhere in the
/// response (not the `prisma+postgres://` Accelerate form, which drivers can't use).
fn find_pg_uri(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if s.starts_with("postgres://") || s.starts_with("postgresql://") => {
            Some(s.clone())
        }
        Value::Object(map) => map.values().find_map(find_pg_uri),
        Value::Array(arr) => arr.iter().find_map(find_pg_uri),
        _ => None,
    }
}

pub async fn build_connection(token: &str, project_id: &str) -> Result<ProviderConnection, String> {
    // The project (for its name) and its databases are independent reads: both
    // at once, so a connect is two round trips to Prisma's API instead of three.
    let (proj_path, dbs_path) = (format!("/projects/{project_id}"), format!("/projects/{project_id}/databases"));
    let (proj, dbs) = tokio::join!(get(token, &proj_path), get(token, &dbs_path));
    let (proj, dbs) = (proj?, dbs?);
    let name = proj["data"]["name"]
        .as_str()
        .or_else(|| proj["name"].as_str())
        .unwrap_or(project_id)
        .to_string();

    // A project's databases live at a separate endpoint; the read responses never
    // include the password (it's a secret shown once). Grab the database id, then
    // CREATE a fresh connection - that call returns usable direct credentials.
    let list = as_list(&dbs);
    let db = list.first().ok_or(
        "This Prisma project has no Postgres database yet. Create one in the Prisma \
         Console (or pick a project that already has Prisma Postgres), then try again.",
    )?;
    let db_id = db["id"].as_str().ok_or("Prisma: couldn't read the database id")?;

    let created = post(
        token,
        &format!("/databases/{db_id}/connections"),
        serde_json::json!({ "name": "Stroke" }),
    )
    .await?;

    connection_from_created(&created, &name)
}

/// The create-connection response → a connection. Prisma nests the direct
/// credentials at different depths depending on the API version, so look for a
/// credentials object first and a `postgres://` URI anywhere second.
fn connection_from_created(created: &Value, name: &str) -> Result<ProviderConnection, String> {
    let conn = |host, port, username, password, database: String| ProviderConnection {
        db_type: "postgres".into(),
        host,
        port,
        username,
        password,
        database: if database.is_empty() { "postgres".into() } else { database },
        ssl: true,
        needs_password: false,
        name: format!("Prisma · {name}"),
    };
    if let Some((host, user, pass, dbn)) = find_direct(created) {
        return Ok(conn(host, 5432, user, pass, dbn));
    }
    if let Some(uri) = find_pg_uri(created) {
        let (host, port, username, password, database) = super::neon::parse_pg_uri(&uri)?;
        return Ok(conn(host, port, username, password, database));
    }
    let shape: String = serde_json::to_string(created).unwrap_or_default().chars().take(1600).collect();
    Err(format!(
        "Prisma created a connection but returned no direct credentials we could parse. \
         Shape: {shape}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn credentials_object_is_read_wherever_prisma_nests_it() {
        let created = json!({ "data": { "endpoints": { "direct": {
            "ppgDirectConnection": { "host": "db.prisma.io", "user": "u1", "pass": "p1" }
        } } } });
        let c = connection_from_created(&created, "pulse").unwrap();
        assert_eq!((c.host.as_str(), c.username.as_str(), c.password.as_str()), ("db.prisma.io", "u1", "p1"));
        assert_eq!(c.name, "Prisma · pulse");
    }

    #[test]
    fn falls_back_to_a_postgres_uri_anywhere_in_the_response() {
        let created = json!({ "data": { "connectionString": "postgres://u2:p%212@db.prisma.io:5432/postgres?sslmode=require" } });
        let c = connection_from_created(&created, "x").unwrap();
        assert_eq!((c.username.as_str(), c.password.as_str()), ("u2", "p!2"));
        assert!(connection_from_created(&json!({ "data": {} }), "x").unwrap_err().contains("no direct credentials"));
    }
}
