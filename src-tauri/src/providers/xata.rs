/*!
 * Xata adapter - Postgres. Signed in with an API key (Xata console → Account
 * or Organization settings → API keys), sent as `Authorization: Bearer`. Lists
 * every branch of every project in every organization the key can see, and
 * connects with the branch's own credentials (`…/branches/{id}/credentials`),
 * which Xata hands out per branch.
 *
 * Xata also runs OAuth (Keycloak, with dynamic client registration); that sign-in
 * can replace the key once its tokens are confirmed to carry `credentials:read`.
 */

use super::SendRetry;
use super::{http, OAuthConfig, ProviderConnection, ProviderDatabase};
use serde_json::Value;

/// Unused for a key-based provider; present so `Provider::oauth` stays total.
pub const OAUTH: OAuthConfig = OAuthConfig { client_id: "", auth_url: "", token_url: "", scopes: "" };

const API: &str = "https://api.xata.tech";

async fn get(token: &str, path: &str, what: &str) -> Result<Value, String> {
    let key = token.trim();
    if key.is_empty() {
        return Err("Xata needs an API key.".into());
    }
    let resp = http()
        .get(format!("{API}{path}"))
        .bearer_auth(key)
        .send_retrying()
        .await
        .map_err(|e| format!("Xata request failed ({what}): {}", super::describe(&e)))?;
    let status = resp.status().as_u16();
    if status == 401 {
        return Err(super::UNAUTHORIZED.into());
    }
    let text = resp.text().await.map_err(|e| format!("Xata read failed: {e}"))?;
    if status == 403 {
        return Err(format!("This Xata API key can't do that ({what}). Use a key with access to the organization's projects and branch credentials."));
    }
    if !(200..300).contains(&status) {
        let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let msg = body["message"].as_str().map(String::from).unwrap_or_else(|| text.chars().take(160).collect());
        return Err(format!("Xata API error ({status}) on {what}: {msg}"));
    }
    serde_json::from_str(&text).map_err(|_| format!("Xata returned a non-JSON response for {what}"))
}

fn enc(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

/// `{ org, project, branch }`, what `build_connection` needs.
fn db_ref(org: &str, project: &str, branch: &str) -> String {
    serde_json::json!({ "org": org, "project": project, "branch": branch }).to_string()
}

fn ids(body: &Value, list: &str) -> Vec<(String, String)> {
    body[list]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| {
            let id = o["id"].as_str()?.to_string();
            let name = o["name"].as_str().unwrap_or(&id).to_string();
            Some((id, name))
        })
        .collect()
}

pub async fn list_databases(token: &str) -> Result<Vec<ProviderDatabase>, String> {
    let orgs = get(token, "/organizations", "listing organizations").await?;
    let orgs = ids(&orgs, "organizations");
    let many_orgs = orgs.len() > 1;
    let mut out = Vec::new();
    for (org, org_name) in &orgs {
        let projects = get(token, &format!("/organizations/{}/projects", enc(org)), "listing projects").await?;
        // Every project's branches at once: an account with ten projects
        // shouldn't wait for ten round trips in a row.
        let fetches = ids(&projects, "projects").into_iter().map(|(project, project_name)| async move {
            let path = format!("/organizations/{}/projects/{}/branches", enc(org), enc(&project));
            let branches = get(token, &path, "listing branches").await;
            (project, project_name, branches)
        });
        for (project, project_name, branches) in futures::future::join_all(fetches).await {
            let branches = branches?;
            for b in branches["branches"].as_array().into_iter().flatten() {
                let Some(branch) = b["id"].as_str() else { continue };
                let branch_name = b["name"].as_str().unwrap_or(branch);
                let prefix = if many_orgs { format!("{org_name} / ") } else { String::new() };
                out.push(ProviderDatabase {
                    db_ref: db_ref(org, &project, branch),
                    name: format!("{prefix}{project_name} / {branch_name}"),
                    region: b["region"].as_str().filter(|r| !r.is_empty()).map(String::from),
                    kind: Some(if b["parentID"].as_str().is_some() { "Branch".into() } else { "Main branch".into() }),
                    host: None,
                });
            }
        }
    }
    Ok(out)
}

pub async fn build_connection(token: &str, reference: &str) -> Result<ProviderConnection, String> {
    let r: Value = serde_json::from_str(reference).map_err(|_| "That Xata branch reference is damaged. Refresh the list.".to_string())?;
    let (org, project, branch) = (
        r["org"].as_str().unwrap_or_default(),
        r["project"].as_str().unwrap_or_default(),
        r["branch"].as_str().unwrap_or_default(),
    );
    if org.is_empty() || project.is_empty() || branch.is_empty() {
        return Err("That Xata branch reference is incomplete. Refresh the list.".into());
    }
    let path = format!("/organizations/{}/projects/{}/branches/{}/credentials", enc(org), enc(project), enc(branch));
    let c = get(token, &path, "reading the branch credentials").await?;
    connection_from_credentials(&c, branch)
}

fn connection_from_credentials(c: &Value, branch: &str) -> Result<ProviderConnection, String> {
    let host = c["hostname"].as_str().filter(|h| !h.is_empty()).ok_or("Xata returned no host for this branch. It may still be starting.")?;
    Ok(ProviderConnection {
        db_type: "postgres".into(),
        host: host.to_string(),
        port: c["port"].as_u64().and_then(|p| u16::try_from(p).ok()).unwrap_or(5432),
        username: c["username"].as_str().unwrap_or("xata").to_string(),
        password: c["password"].as_str().unwrap_or_default().to_string(),
        database: c["dbname"].as_str().unwrap_or("xata").to_string(),
        // Xata only accepts TLS.
        ssl: true,
        needs_password: false,
        name: format!("Xata · {branch}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_become_a_tls_postgres_connection() {
        let c = serde_json::json!({ "username": "xata", "password": "p", "hostname": "us-east-1.xata.tech", "port": 5432, "dbname": "app", "connectionString": "postgresql://…" });
        let conn = connection_from_credentials(&c, "main").unwrap();
        assert_eq!((conn.db_type.as_str(), conn.host.as_str(), conn.port, conn.database.as_str()), ("postgres", "us-east-1.xata.tech", 5432, "app"));
        assert!(conn.ssl && !conn.needs_password);
        assert!(connection_from_credentials(&serde_json::json!({}), "main").is_err());
    }

    #[test]
    fn list_ids_read_name_and_id() {
        let body = serde_json::json!({ "projects": [{ "id": "p1", "name": "Shop" }, { "id": "p2" }, { "name": "no id" }] });
        assert_eq!(ids(&body, "projects"), vec![("p1".into(), "Shop".into()), ("p2".into(), "p2".into())]);
        let r: Value = serde_json::from_str(&db_ref("o", "p", "b")).unwrap();
        assert_eq!(r["branch"], "b");
    }
}
