/*!
 * Turso adapter - libSQL. Sign in through the browser the way `turso auth login`
 * does, list every database across the user's organizations, and create a
 * database token on connect.
 *
 * Turso has no OAuth app registration. Its CLI opens
 * `https://api.turso.tech/?port={port}&redirect=true&type=cli&state={state}`,
 * and after sign-in the browser is sent to `http://localhost:{port}/?jwt=…&state=…`
 * with a platform API token (valid about a week, no refresh token). This reuses
 * that page, the same way Neon reuses neonctl's client.
 */

use super::{http, OAuthConfig, ProviderConnection, ProviderDatabase};
use serde_json::Value;

pub const API: &str = "https://api.turso.tech";

pub const OAUTH: OAuthConfig = OAuthConfig {
    // No client: the CLI login page identifies callers with `type=cli`.
    client_id: "",
    auth_url: API,
    token_url: "",
    scopes: "",
};

/// Cap on database list pages per organization.
const MAX_PAGES: usize = 20;

async fn send(req: reqwest::RequestBuilder, what: &str) -> Result<Value, String> {
    let resp = req
        .send()
        .await
        .map_err(|e| format!("Turso request failed ({what}): {}", super::describe(&e)))?;
    let status = resp.status().as_u16();
    // Before reading the body: a 401 may not be JSON, and the caller refreshes on it.
    if status == 401 {
        return Err(super::UNAUTHORIZED.into());
    }
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Turso read failed: {e}"))?;
    let body: Value = serde_json::from_str(&text).map_err(|_| {
        let snippet: String = text.chars().take(200).collect();
        format!("Turso non-JSON response for {what} (HTTP {status}): {snippet}")
    })?;
    if !(200..300).contains(&status) {
        let msg = body["error"].as_str().unwrap_or("request failed");
        return Err(format!("Turso API error ({status}) on {what}: {msg}"));
    }
    Ok(body)
}

async fn get(token: &str, path: &str, what: &str) -> Result<Value, String> {
    send(http().get(format!("{API}{path}")).bearer_auth(token), what).await
}

/// Turso's JSON mixes `Name`/`Hostname` with camelCase keys; read either spelling.
fn field<'a>(v: &'a Value, upper: &str, lower: &str) -> Option<&'a str> {
    v[upper].as_str().or_else(|| v[lower].as_str())
}

/// One `databases` page of an org → picker rows. `db_ref` is "{org}/{name}".
fn parse_page(org: &str, body: &Value) -> Vec<ProviderDatabase> {
    body["databases"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|db| {
            let name = field(db, "Name", "name")?;
            Some(ProviderDatabase {
                db_ref: format!("{org}/{name}"),
                name: name.to_string(),
                region: field(db, "primaryRegion", "primary_region").map(String::from),
                kind: Some("Database".into()),
                host: field(db, "Hostname", "hostname").map(String::from),
            })
        })
        .collect()
}

async fn list_org(token: &str, org: &str) -> Result<Vec<ProviderDatabase>, String> {
    let base = format!("/v1/organizations/{}/databases", urlencoding::encode(org));
    let mut out = Vec::new();
    let mut cursor = String::new();
    for _ in 0..MAX_PAGES {
        let path = if cursor.is_empty() {
            base.clone()
        } else {
            format!("{base}?cursor={}", urlencoding::encode(&cursor))
        };
        let body = get(token, &path, "listing databases").await?;
        out.extend(parse_page(org, &body));
        match body["pagination"]["next"].as_str() {
            Some(next) if !next.is_empty() => cursor = next.to_string(),
            _ => break,
        }
    }
    Ok(out)
}

pub async fn list_databases(token: &str) -> Result<Vec<ProviderDatabase>, String> {
    let orgs = get(token, "/v2/organizations", "listing organizations").await?;
    let slugs: Vec<&str> = orgs["organizations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| o["slug"].as_str())
        .collect();
    // Every organization's list in flight at once, in the orgs' own order. An
    // org the token can't read is skipped rather than failing the whole list.
    let pages = futures::future::join_all(slugs.iter().map(|slug| list_org(token, slug))).await;
    Ok(super::merge_partial(pages, "")?.into_iter().flatten().collect())
}

pub async fn build_connection(token: &str, db_ref: &str) -> Result<ProviderConnection, String> {
    let (org, database) = db_ref
        .split_once('/')
        .ok_or("Invalid Turso database reference")?;
    let (org_enc, db_enc) = (urlencoding::encode(org), urlencoding::encode(database));

    // The hostname lookup and the token mint don't depend on each other, so
    // both go out at once: one round trip instead of two.
    let db_path = format!("/v1/organizations/{org_enc}/databases/{db_enc}");
    let (db, minted) = tokio::join!(
        get(token, &db_path, "reading the database"),
        // A full-access token with no expiry: the connection is saved and
        // reused, and a token that lapsed in a week would break it with no way
        // to renew in place.
        send(
            http()
                .post(format!(
                    "{API}/v1/organizations/{org_enc}/databases/{db_enc}/auth/tokens?expiration=never&authorization=full-access"
                ))
                .bearer_auth(token)
                .json(&serde_json::json!({})),
            "creating a database token",
        ),
    );
    let (db, minted) = (db?, minted?);
    let host = field(&db["database"], "Hostname", "hostname")
        .ok_or_else(|| format!("Turso returned no hostname for {database}"))?
        .to_string();
    let jwt = minted["jwt"]
        .as_str()
        .ok_or("Turso did not return a database token")?
        .to_string();

    // libSQL has no host/port/user split. The adapter contract carries the URL in
    // `host` and the token in `password`; the frontend maps them to `url` and
    // `authToken` for a libsql connection.
    Ok(ProviderConnection {
        db_type: "libsql".into(),
        host: format!("libsql://{host}"),
        port: 443,
        username: String::new(),
        password: jwt,
        database: database.to_string(),
        ssl: true,
        needs_password: false,
        name: format!("Turso · {database}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_either_casing_of_turso_fields() {
        let body = json!({ "databases": [
            { "Name": "cool", "Hostname": "cool-me.aws-ap-south-1.turso.io", "primaryRegion": "aws-ap-south-1" },
            { "name": "lower", "hostname": "lower-me.turso.io" },
            { "Hostname": "no-name-skipped" }
        ]});
        let rows = parse_page("me", &body);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].db_ref, "me/cool");
        assert_eq!(rows[0].region.as_deref(), Some("aws-ap-south-1"));
        assert_eq!(rows[1].host.as_deref(), Some("lower-me.turso.io"));
    }
}
