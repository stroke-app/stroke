/*!
 * Nile adapter - Postgres. Sign in through the browser the way `nile connect`
 * does, list every database across the user's workspaces, and create database
 * credentials on connect (Nile hands out a credential id + password pair; the id
 * is the Postgres user).
 *
 * Reuses the official CLI's public PKCE client (`nilecli`), like Neon's
 * neonctl: no secret, token exchange straight to Nile, redirect to
 * `http://localhost:{port}/callback` on any free port.
 */

use super::{http, OAuthConfig, ProviderConnection, ProviderDatabase};
use serde_json::Value;

pub const OAUTH: OAuthConfig = OAuthConfig {
    client_id: "nilecli",
    auth_url: "https://console.thenile.dev/authorize",
    token_url: "https://global.thenile.dev/oauth2/token",
    // The CLI sends no scope; the token carries the developer's own access.
    scopes: "",
};

const API: &str = "https://global.thenile.dev";

async fn send(req: reqwest::RequestBuilder, what: &str) -> Result<Value, String> {
    let resp = req
        .send()
        .await
        .map_err(|e| format!("Nile request failed ({what}): {}", super::describe(&e)))?;
    let status = resp.status().as_u16();
    if status == 401 {
        return Err(super::UNAUTHORIZED.into());
    }
    let text = resp.text().await.map_err(|e| format!("Nile read failed: {e}"))?;
    let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        let msg = body["message"]
            .as_str()
            .or_else(|| body["errors"][0].as_str())
            .map(String::from)
            .unwrap_or_else(|| text.chars().take(160).collect());
        return Err(format!("Nile API error ({status}) on {what}: {msg}"));
    }
    Ok(body)
}

async fn get(token: &str, path: &str, what: &str) -> Result<Value, String> {
    send(http().get(format!("{API}{path}")).bearer_auth(token), what).await
}

/// `AWS_US_WEST_2` → `us-west-2.db.thenile.dev`; Azure regions live under
/// `db.{region}.azure.thenile.dev` (the same mapping `nile connect` uses).
fn host_for(region: &str) -> String {
    let lower = region.to_ascii_lowercase();
    let mut parts = lower.split('_');
    let cloud = parts.next().unwrap_or("aws");
    let id = parts.collect::<Vec<_>>().join("-");
    if cloud == "azure" {
        format!("db.{id}.azure.thenile.dev")
    } else {
        format!("{id}.db.thenile.dev")
    }
}

pub async fn list_databases(token: &str) -> Result<Vec<ProviderDatabase>, String> {
    let workspaces = get(token, "/workspaces", "listing workspaces").await?;
    let slugs: Vec<&str> = workspaces
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|w| w["slug"].as_str())
        .collect();
    // A workspace the token can't read is skipped rather than failing the list.
    let pages = futures::future::join_all(slugs.iter().map(|slug| {
        let path = format!("/workspaces/{}/databases", urlencoding::encode(slug));
        async move { get(token, &path, "listing databases").await.map(|b| (*slug, b)) }
    }))
    .await;
    let pages = super::merge_partial(pages, "")?;

    Ok(pages.iter().flat_map(|(slug, dbs)| parse_databases(slug, dbs)).collect())
}

/// One workspace's databases → picker rows. `db_ref` is "{workspace}/{database}".
fn parse_databases(slug: &str, dbs: &Value) -> Vec<ProviderDatabase> {
    dbs.as_array()
        .into_iter()
        .flatten()
        .filter_map(|db| {
            let name = db["name"].as_str()?;
            let region = db["region"].as_str().unwrap_or_default();
            Some(ProviderDatabase {
                db_ref: format!("{slug}/{name}"),
                name: name.to_string(),
                region: (!region.is_empty()).then(|| region.to_ascii_lowercase().replace('_', "-")),
                kind: Some("Database".into()),
                host: (!region.is_empty()).then(|| host_for(region)),
            })
        })
        .collect()
}

/// The credentials-create response → a connection. The credential id is the
/// Postgres user; the host comes from the database's region.
fn connection_from_credentials(creds: &Value, database: &str) -> Result<ProviderConnection, String> {
    let user = creds["id"].as_str().ok_or("Nile returned no credential id")?;
    let password = creds["password"].as_str().ok_or("Nile returned no password")?;
    let region = creds["database"]["region"]
        .as_str()
        .ok_or("Nile returned no region for this database")?;
    Ok(ProviderConnection {
        db_type: "postgres".into(),
        host: host_for(region),
        port: 5432,
        username: user.to_string(),
        password: password.to_string(),
        database: database.to_string(),
        ssl: true,
        needs_password: false,
        name: format!("Nile · {database}"),
    })
}

pub async fn build_connection(token: &str, db_ref: &str) -> Result<ProviderConnection, String> {
    let (workspace, database) = db_ref.split_once('/').ok_or("Invalid Nile database reference")?;
    let creds = send(
        http()
            .post(format!(
                "{API}/workspaces/{}/databases/{}/credentials",
                urlencoding::encode(workspace),
                urlencoding::encode(database)
            ))
            .bearer_auth(token),
        "creating database credentials",
    )
    .await?;
    connection_from_credentials(&creds, database)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn databases_and_credentials_map_to_the_region_host() {
        let rows = parse_databases("ws", &json!([{ "id": "d1", "name": "default", "region": "AWS_US_WEST_2", "status": "READY" }]));
        assert_eq!(rows[0].db_ref, "ws/default");
        assert_eq!(rows[0].host.as_deref(), Some("us-west-2.db.thenile.dev"));
        let creds = json!({ "id": "0190-cred", "password": "pw", "database": { "region": "AWS_US_WEST_2" } });
        let c = connection_from_credentials(&creds, "default").unwrap();
        assert_eq!((c.username.as_str(), c.host.as_str(), c.port), ("0190-cred", "us-west-2.db.thenile.dev", 5432));
        assert!(connection_from_credentials(&json!({ "id": "x" }), "d").is_err());
    }

    #[test]
    fn regions_map_to_the_cli_hosts() {
        assert_eq!(host_for("AWS_US_WEST_2"), "us-west-2.db.thenile.dev");
        assert_eq!(host_for("AZURE_EASTUS"), "db.eastus.azure.thenile.dev");
    }
}
