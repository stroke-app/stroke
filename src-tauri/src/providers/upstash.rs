/*!
 * Upstash adapter - Redis. Upstash has no OAuth for third-party apps, so this is
 * the first paste-a-token provider: the account email and a Developer API key
 * (Console → Account → Management API), stored together as `email:key` and sent
 * as HTTP basic auth. List every Redis database, and connect with the
 * database's own endpoint and password, which the API returns directly.
 */

use super::{http, OAuthConfig, ProviderConnection, ProviderDatabase};
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
        .send()
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
