/*!
 * Upstash adapter - Redis. Upstash has no OAuth for third-party apps, so this is
 * the first paste-a-token provider: the account email and a Developer API key
 * (Console → Account → Management API), stored together as `email:key` and sent
 * as HTTP basic auth. List every Redis database, and connect with the
 * database's own endpoint and password, which the API returns directly.
 */

use super::SendRetry;
use super::{http, CreateOptions, CreateRequest, CreatedDatabase, DatabaseStatus, OAuthConfig, ProviderConnection, ProviderDatabase};
use serde_json::Value;

/// Unused for a token provider; present so `Provider::oauth` stays total.
pub const OAUTH: OAuthConfig = OAuthConfig {
    client_id: "",
    auth_url: "",
    token_url: "",
    scopes: "",
};

const API: &str = "https://api.upstash.com/v2";

/// The stored credential is `email:api_key`. An email never contains a colon,
/// so the first one is the split.
fn credentials(token: &str) -> Result<(&str, &str), String> {
    token
        .split_once(':')
        .filter(|(e, k)| !e.is_empty() && !k.is_empty())
        .ok_or_else(|| "Upstash needs the account email and a Developer API key.".to_string())
}

async fn get(token: &str, path: &str, what: &str) -> Result<Value, String> {
    let (email, key) = credentials(token)?;
    let resp = http()
        .get(format!("{API}{path}"))
        .basic_auth(email, Some(key))
        .send_retrying()
        .await
        .map_err(|e| format!("Upstash request failed ({what}): {}", super::describe(&e)))?;
    let status = resp.status().as_u16();
    // A wrong email/key pair is a 401: the command layer turns a second one into
    // "Not signed in", which puts the token form back.
    if status == 401 || status == 403 {
        return Err(super::UNAUTHORIZED.into());
    }
    let text = resp.text().await.map_err(|e| format!("Upstash read failed: {e}"))?;
    if !(200..300).contains(&status) {
        let snippet: String = text.chars().take(160).collect();
        return Err(format!("Upstash API error ({status}) on {what}: {snippet}"));
    }
    serde_json::from_str(&text).map_err(|_| format!("Upstash returned a non-JSON response for {what}"))
}

pub async fn list_databases(token: &str) -> Result<Vec<ProviderDatabase>, String> {
    let body = get(token, "/redis/databases", "listing databases").await?;
    Ok(parse_list(&body))
}

fn parse_list(body: &Value) -> Vec<ProviderDatabase> {
    body.as_array()
        .into_iter()
        .flatten()
        .filter_map(|db| {
            let id = db["database_id"].as_str()?;
            Some(ProviderDatabase {
                db_ref: id.to_string(),
                name: db["database_name"].as_str().unwrap_or(id).to_string(),
                region: db["primary_region"]
                    .as_str()
                    .or_else(|| db["region"].as_str())
                    .filter(|r| !r.is_empty() && *r != "global")
                    .map(String::from),
                kind: Some("Redis".into()),
                host: db["endpoint"].as_str().map(String::from),
            })
        })
        .collect()
}

/// A database's detail → a connection with its own endpoint and password.
fn connection_from_detail(db: &Value, database_id: &str) -> Result<ProviderConnection, String> {
    let name = db["database_name"].as_str().unwrap_or(database_id).to_string();
    if let Some(state) = db["state"].as_str().filter(|s| *s != "active") {
        return Err(format!("{name} is not active ({state}). Check it in the Upstash console."));
    }
    let host = db["endpoint"].as_str().ok_or_else(|| format!("Upstash returned no endpoint for {name}"))?;
    let password = db["password"].as_str().ok_or_else(|| format!("Upstash returned no password for {name}"))?;
    let port = db["port"].as_u64().and_then(|p| u16::try_from(p).ok()).unwrap_or(6379);
    Ok(ProviderConnection {
        db_type: "redis".into(),
        host: host.to_string(),
        port,
        username: "default".into(),
        password: password.to_string(),
        database: "0".into(),
        // Upstash only accepts TLS on the Redis port.
        ssl: db["tls"].as_bool().unwrap_or(true),
        needs_password: false,
        name: format!("Upstash · {name}"),
    })
}

pub async fn build_connection(token: &str, database_id: &str) -> Result<ProviderConnection, String> {
    let db = get(
        token,
        &format!("/redis/database/{}", urlencoding::encode(database_id)),
        "reading the database",
    )
    .await?;
    connection_from_detail(&db, database_id)
}

// ── Create a Redis database ──────────────────────────────────────────────────
//
// `POST /v2/redis/database { database_name, platform, primary_region, tls }`
// with the same email + Developer API key, which has full account access. The
// plan is left out so Upstash picks free or pay-as-you-go from the account.
// There is no region-list endpoint; these are the AWS primary regions the
// create schema accepts. A new database is `active` right away, but the status
// is still checked so a slow one isn't connected to early.

const REGIONS: &[(&str, &str)] = &[
    ("us-east-1", "AWS US East (N. Virginia)"),
    ("us-east-2", "AWS US East (Ohio)"),
    ("us-west-1", "AWS US West (N. California)"),
    ("us-west-2", "AWS US West (Oregon)"),
    ("ca-central-1", "AWS Canada (Central)"),
    ("sa-east-1", "AWS South America (São Paulo)"),
    ("eu-west-1", "AWS Europe (Ireland)"),
    ("eu-west-2", "AWS Europe (London)"),
    ("eu-central-1", "AWS Europe (Frankfurt)"),
    ("ap-south-1", "AWS Asia Pacific (Mumbai)"),
    ("ap-southeast-1", "AWS Asia Pacific (Singapore)"),
    ("ap-southeast-2", "AWS Asia Pacific (Sydney)"),
    ("ap-northeast-1", "AWS Asia Pacific (Tokyo)"),
    ("af-south-1", "AWS Africa (Cape Town)"),
];

pub fn create_options() -> CreateOptions {
    let regions = super::static_choices(REGIONS);
    CreateOptions {
        scopes: Vec::new(),
        scope_label: String::new(),
        scope: None,
        default_region: super::pick_default(&regions, "us-east-1"),
        regions,
        region_label: "Primary region".into(),
    }
}

fn create_body(req: &CreateRequest) -> Result<Value, String> {
    let name = super::required_name(req)?;
    let region = super::required(&req.region, "Pick a region.")?;
    Ok(serde_json::json!({ "database_name": name, "platform": "aws", "primary_region": region, "tls": true }))
}

fn db_status(db: &Value) -> DatabaseStatus {
    match db["state"].as_str().unwrap_or("active") {
        s @ "active" => DatabaseStatus::ready(s),
        s @ "suspended" => DatabaseStatus::failed(s),
        s => DatabaseStatus::pending(s),
    }
}

fn parse_created(db: &Value) -> Result<CreatedDatabase, String> {
    let id = db["database_id"].as_str().ok_or("Upstash created the database but returned no id")?;
    Ok(CreatedDatabase {
        db_ref: id.to_string(),
        name: db["database_name"].as_str().unwrap_or(id).to_string(),
        ready: db_status(db).ready,
    })
}

pub async fn create_database(token: &str, req: &CreateRequest) -> Result<CreatedDatabase, String> {
    let body = create_body(req)?;
    let (email, key) = credentials(token)?;
    let resp = http()
        .post(format!("{API}/redis/database"))
        .basic_auth(email, Some(key))
        .json(&body)
        .send_retrying()
        .await
        .map_err(|e| format!("Upstash request failed (creating a database): {}", super::describe(&e)))?;
    let status = resp.status().as_u16();
    if status == 401 {
        return Err(super::UNAUTHORIZED.into());
    }
    let text = resp.text().await.map_err(|e| format!("Upstash read failed: {e}"))?;
    if !(200..300).contains(&status) {
        // Upstash answers errors as a bare string or `{ "error": … }`.
        let msg = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v["error"].as_str().or_else(|| v.as_str()).map(String::from))
            .unwrap_or_else(|| text.chars().take(160).collect());
        return Err(format!("Upstash API error ({status}) on creating a database: {msg}"));
    }
    let db: Value = serde_json::from_str(&text).map_err(|_| "Upstash returned a non-JSON response for creating a database".to_string())?;
    parse_created(&db)
}

pub async fn database_status(token: &str, database_id: &str) -> Result<DatabaseStatus, String> {
    let db = get(token, &format!("/redis/database/{}", urlencoding::encode(database_id)), "reading the database").await?;
    Ok(db_status(&db))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn create_body_is_a_tls_aws_database() {
        let req = CreateRequest { name: "cache".into(), region: Some("eu-west-1".into()), ..Default::default() };
        assert_eq!(
            create_body(&req).unwrap(),
            json!({ "database_name": "cache", "platform": "aws", "primary_region": "eu-west-1", "tls": true })
        );
    }

    #[test]
    fn created_database_reads_id_and_state() {
        let c = parse_created(&json!({ "database_id": "abc", "database_name": "cache", "state": "active" })).unwrap();
        assert_eq!(c, CreatedDatabase { db_ref: "abc".into(), name: "cache".into(), ready: true });
        assert!(db_status(&json!({ "state": "suspended" })).failed);
        assert_eq!(create_options().default_region.as_deref(), Some("us-east-1"));
    }

    #[test]
    fn list_and_detail_map_to_a_tls_redis_connection() {
        let rows = parse_list(&json!([{ "database_id": "abc", "database_name": "test", "region": "global", "primary_region": "ap-south-1", "endpoint": "fine-cat-1.upstash.io" }]));
        assert_eq!((rows[0].db_ref.as_str(), rows[0].region.as_deref()), ("abc", Some("ap-south-1")));
        let detail = json!({ "database_name": "test", "state": "active", "endpoint": "fine-cat-1.upstash.io", "port": 6379, "password": "pw", "tls": true });
        let c = connection_from_detail(&detail, "abc").unwrap();
        assert_eq!((c.db_type.as_str(), c.host.as_str(), c.port, c.ssl), ("redis", "fine-cat-1.upstash.io", 6379, true));
        assert!(connection_from_detail(&json!({ "database_name": "t", "state": "suspended" }), "abc").unwrap_err().contains("not active"));
    }

    #[test]
    fn stored_token_splits_into_email_and_key() {
        assert_eq!(credentials("me@x.dev:abc:def").unwrap(), ("me@x.dev", "abc:def"));
        assert!(credentials("no-colon").is_err());
        assert!(credentials(":key").is_err());
    }
}
