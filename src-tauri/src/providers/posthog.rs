/*!
 * PostHog adapter - the provider-picker side of the PostHog engine
 * (`db/posthog.rs`). Paste a personal API key and the instance URL, pick a
 * project, and connect: no connection string.
 *
 * Token-based, like Upstash: the stored credential is `{base_url}|{api_key}`.
 * A URL never contains `|` and PostHog keys (`phx_…`) don't either, so the first
 * one is the split. The key needs the Project Read scope to list projects and
 * Query Read to run HogQL.
 */

use super::{http, OAuthConfig, ProviderConnection, ProviderDatabase};
use serde_json::{json, Value};

/// Unused for a token provider; present so `Provider::oauth` stays total.
pub const OAUTH: OAuthConfig = OAuthConfig { client_id: "", auth_url: "", token_url: "", scopes: "" };

/// Cap on project list pages.
const MAX_PAGES: usize = 20;

fn credentials(token: &str) -> Result<(String, &str), String> {
    let (host, key) = token
        .split_once('|')
        .filter(|(h, k)| !h.trim().is_empty() && !k.trim().is_empty())
        .ok_or("PostHog needs its URL and a personal API key.")?;
    let host = host.trim().trim_end_matches('/');
    let base = if host.starts_with("http://") || host.starts_with("https://") {
        host.to_string()
    } else {
        format!("https://{host}")
    };
    Ok((base, key.trim()))
}

async fn get(token: &str, url_or_path: &str) -> Result<Value, String> {
    let (base, key) = credentials(token)?;
    let url = if url_or_path.starts_with("http") { url_or_path.to_string() } else { format!("{base}{url_or_path}") };
    let resp = http()
        .get(&url)
        .bearer_auth(key)
        .send()
        .await
        .map_err(|e| format!("PostHog request failed: {}", super::describe(&e)))?;
    let status = resp.status().as_u16();
    // A wrong key is a 401; twice in a row the command layer ends the "session"
    // and the key form comes back.
    if status == 401 {
        return Err(super::UNAUTHORIZED.into());
    }
    let text = resp.text().await.map_err(|e| format!("PostHog read failed: {e}"))?;
    let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        let detail = body["detail"].as_str().map(String::from).unwrap_or_else(|| text.chars().take(200).collect());
        return Err(if status == 403 {
            format!("PostHog refused the key: {detail}. It needs the Project Read and Query Read scopes.")
        } else {
            format!("PostHog API error ({status}): {detail}")
        });
    }
    Ok(body)
}

/// One `/api/projects/` page → picker rows. `db_ref` carries the id and name so
/// connecting needs no second lookup.
fn parse_projects(body: &Value) -> Vec<ProviderDatabase> {
    body["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let id = p["id"].as_i64().map(|n| n.to_string()).or_else(|| p["id"].as_str().map(String::from))?;
            let name = p["name"].as_str().unwrap_or(&id).to_string();
            Some(ProviderDatabase {
                db_ref: json!({ "id": id, "n": name }).to_string(),
                name,
                region: None,
                kind: Some("Project".into()),
                host: None,
            })
        })
        .collect()
}

pub async fn list_databases(token: &str) -> Result<Vec<ProviderDatabase>, String> {
    let mut out = Vec::new();
    let mut next = "/api/projects/".to_string();
    for _ in 0..MAX_PAGES {
        let body = get(token, &next).await?;
        out.extend(parse_projects(&body));
        match body["next"].as_str() {
            Some(n) if !n.is_empty() => next = n.to_string(),
            _ => break,
        }
    }
    Ok(out)
}

pub async fn build_connection(token: &str, db_ref: &str) -> Result<ProviderConnection, String> {
    let r: Value = serde_json::from_str(db_ref).map_err(|_| "Invalid PostHog project reference")?;
    let id = r["id"].as_str().ok_or("Invalid PostHog project reference")?;
    let name = r["n"].as_str().unwrap_or(id);
    let (base, key) = credentials(token)?;
    // The engine's own adapter contract: the base URL in `host`, the project id
    // in `database`, the key in `password`. The frontend maps these to a
    // `posthog` connection ({ host, projectId, apiKey }).
    Ok(ProviderConnection {
        db_type: "posthog".into(),
        host: base,
        port: 443,
        username: String::new(),
        password: key.to_string(),
        database: id.to_string(),
        ssl: true,
        needs_password: false,
        name: format!("PostHog · {name}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_splits_url_and_key() {
        assert_eq!(credentials("https://eu.posthog.com/|phx_abc").unwrap(), ("https://eu.posthog.com".into(), "phx_abc"));
        assert_eq!(credentials("ph.example.com|phx_abc").unwrap().0, "https://ph.example.com");
        assert!(credentials("|phx_abc").is_err());
        assert!(credentials("https://us.posthog.com").is_err());
    }

    #[test]
    fn projects_carry_id_and_name() {
        let body = json!({ "results": [{ "id": 42, "name": "Stroke" }, { "name": "no id" }], "next": null });
        let rows = parse_projects(&body);
        assert_eq!(rows.len(), 1);
        let r: Value = serde_json::from_str(&rows[0].db_ref).unwrap();
        assert_eq!((r["id"].as_str(), r["n"].as_str()), (Some("42"), Some("Stroke")));
    }
}
