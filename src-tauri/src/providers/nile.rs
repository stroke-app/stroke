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

use super::SendRetry;
use super::{http, Choice, CreateOptions, CreateRequest, CreatedDatabase, DatabaseStatus, OAuthConfig, ProviderConnection, ProviderDatabase};
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
        .send_retrying()
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

// ── Create a database ────────────────────────────────────────────────────────
//
// `POST /workspaces/{slug}/databases { databaseName, region }`. Nile has no
// region-list endpoint; the two below are the request schema's whole enum.
// A database moves through PENDING/REQUESTED/BUILT/POOLED and is polled until
// READY.

const REGIONS: &[(&str, &str)] = &[
    ("AWS_US_WEST_2", "AWS US West (Oregon)"),
    ("AWS_EU_CENTRAL_1", "AWS Europe (Frankfurt)"),
];

fn parse_workspaces(body: &Value) -> Vec<Choice> {
    body.as_array()
        .into_iter()
        .flatten()
        .filter_map(|w| {
            let slug = w["slug"].as_str()?;
            Some(Choice::new(slug, w["name"].as_str().unwrap_or(slug)))
        })
        .collect()
}

pub async fn create_options(token: &str, scope: Option<&str>) -> Result<CreateOptions, String> {
    let scopes = parse_workspaces(&get(token, "/workspaces", "listing workspaces").await?);
    let regions = super::static_choices(REGIONS);
    Ok(CreateOptions {
        scope: super::chosen_scope(&scopes, scope),
        scopes,
        scope_label: "Workspace".into(),
        default_region: super::pick_default(&regions, "AWS_US_WEST_2"),
        regions,
        region_label: "Region".into(),
    })
}

/// Nile names: letters, digits and underscores, starting with a letter or
/// underscore, under 64 characters.
fn valid_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if !first_ok || name.len() >= 64 || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("Nile database names use letters, numbers and underscores, and start with a letter.".into());
    }
    Ok(())
}

fn create_body(req: &CreateRequest) -> Result<Value, String> {
    let name = super::required_name(req)?;
    valid_name(name)?;
    let region = super::required(&req.region, "Pick a region.")?;
    Ok(serde_json::json!({ "databaseName": name, "region": region }))
}

fn database_state(db: &Value) -> DatabaseStatus {
    match db["status"].as_str().unwrap_or("PENDING") {
        s @ "READY" => DatabaseStatus::ready(s),
        s @ ("FAILED" | "DELETED") => DatabaseStatus::failed(s),
        s => DatabaseStatus::pending(s),
    }
}

pub async fn create_database(token: &str, req: &CreateRequest) -> Result<CreatedDatabase, String> {
    let workspace = super::required(&req.scope, "Pick a workspace.")?;
    let body = create_body(req)?;
    let db = send(
        http()
            .post(format!("{API}/workspaces/{}/databases", urlencoding::encode(workspace)))
            .bearer_auth(token)
            .json(&body),
        "creating a database",
    )
    .await?;
    let name = db["name"].as_str().unwrap_or(body["databaseName"].as_str().unwrap_or_default());
    Ok(CreatedDatabase { db_ref: format!("{workspace}/{name}"), name: name.to_string(), ready: database_state(&db).ready })
}

pub async fn database_status(token: &str, db_ref: &str) -> Result<DatabaseStatus, String> {
    let (workspace, database) = db_ref.split_once('/').ok_or("Invalid Nile database reference")?;
    let db = get(
        token,
        &format!("/workspaces/{}/databases/{}", urlencoding::encode(workspace), urlencoding::encode(database)),
        "reading the database",
    )
    .await?;
    Ok(database_state(&db))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn create_body_and_name_rules() {
        let req = CreateRequest { name: "my_db".into(), scope: Some("ws".into()), region: Some("AWS_EU_CENTRAL_1".into()), password: None };
        assert_eq!(create_body(&req).unwrap(), json!({ "databaseName": "my_db", "region": "AWS_EU_CENTRAL_1" }));
        assert!(valid_name("_x1").is_ok());
        assert!(valid_name("1x").is_err());
        assert!(valid_name("my-db").is_err());
        assert!(valid_name(&"a".repeat(64)).is_err());
    }

    #[test]
    fn a_new_database_polls_until_ready() {
        assert!(database_state(&json!({ "status": "READY" })).ready);
        assert_eq!(database_state(&json!({ "status": "POOLED" })), DatabaseStatus::pending("POOLED"));
        assert_eq!(parse_workspaces(&json!([{ "slug": "ws", "name": "Mine" }])), vec![Choice::new("ws", "Mine")]);
    }

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
