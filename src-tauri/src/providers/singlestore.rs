/*!
 * SingleStore adapter - MySQL wire protocol. Signed in with a Management API
 * key (Cloud Portal → Configuration → API keys), sent as `Authorization:
 * Bearer`. Lists every workspace in every workspace group, and the free
 * starter workspaces. The API never returns a database password, so the
 * connection asks for it once (`needs_password`), like Supabase: the workspace
 * group's admin password, or the starter workspace's user password.
 *
 * SingleStore also runs OAuth (authsvc.singlestore.com, PKCE), whose access
 * token logs in to the database directly; that needs a client ID registered
 * with SingleStore and replaces the key and the password prompt once we have one.
 */

use super::SendRetry;
use super::{http, OAuthConfig, ProviderConnection, ProviderDatabase};
use serde_json::Value;

/// Unused for a key-based provider; present so `Provider::oauth` stays total.
pub const OAUTH: OAuthConfig = OAuthConfig { client_id: "", auth_url: "", token_url: "", scopes: "" };

const API: &str = "https://api.singlestore.com/v1";

async fn get(token: &str, path: &str, what: &str) -> Result<Value, String> {
    let key = token.trim();
    if key.is_empty() {
        return Err("SingleStore needs a Management API key.".into());
    }
    let resp = http()
        .get(format!("{API}{path}"))
        .bearer_auth(key)
        .send_retrying()
        .await
        .map_err(|e| format!("SingleStore request failed ({what}): {}", super::describe(&e)))?;
    let status = resp.status().as_u16();
    if status == 401 {
        return Err(super::UNAUTHORIZED.into());
    }
    let text = resp.text().await.map_err(|e| format!("SingleStore read failed: {e}"))?;
    if !(200..300).contains(&status) {
        let snippet: String = text.chars().take(160).collect();
        return Err(format!("SingleStore API error ({status}) on {what}: {snippet}"));
    }
    serde_json::from_str(&text).map_err(|_| format!("SingleStore returned a non-JSON response for {what}"))
}

/// `{ kind: "workspace" | "starter", id }`, what `build_connection` needs.
fn db_ref(kind: &str, id: &str) -> String {
    serde_json::json!({ "kind": kind, "id": id }).to_string()
}

fn active(state: Option<&str>) -> bool {
    !matches!(state.map(str::to_ascii_uppercase).as_deref(), Some("TERMINATED") | Some("FAILED"))
}

pub async fn list_databases(token: &str) -> Result<Vec<ProviderDatabase>, String> {
    let groups = get(token, "/workspaceGroups", "listing workspace groups").await?;
    let mut out = Vec::new();
    for g in groups.as_array().into_iter().flatten() {
        let Some(gid) = g["workspaceGroupID"].as_str() else { continue };
        if !active(g["state"].as_str()) {
            continue;
        }
        let gname = g["name"].as_str().unwrap_or(gid);
        let path = format!("/workspaces?workspaceGroupID={}", urlencoding::encode(gid));
        let ws = get(token, &path, "listing workspaces").await?;
        for w in ws.as_array().into_iter().flatten() {
            let Some(wid) = w["workspaceID"].as_str() else { continue };
            if !active(w["state"].as_str()) {
                continue;
            }
            out.push(ProviderDatabase {
                db_ref: db_ref("workspace", wid),
                name: format!("{gname} / {}", w["name"].as_str().unwrap_or(wid)),
                region: g["regionName"].as_str().or_else(|| g["region"].as_str()).map(String::from),
                kind: Some("Workspace".into()),
                host: w["endpoint"].as_str().map(String::from),
            });
        }
    }
    // Starter (free) workspaces live under their own endpoint; an account
    // without any, or a key that can't read them, just has none.
    if let Ok(starters) = get(token, "/sharedtier/virtualWorkspaces", "listing starter workspaces").await {
        for w in starters.as_array().into_iter().flatten() {
            let Some(id) = w["virtualWorkspaceID"].as_str() else { continue };
            if !active(w["state"].as_str()) {
                continue;
            }
            out.push(ProviderDatabase {
                db_ref: db_ref("starter", id),
                name: w["name"].as_str().unwrap_or(id).to_string(),
                region: None,
                kind: Some("Starter workspace".into()),
                host: w["endpoint"].as_str().map(String::from),
            });
        }
    }
    Ok(out)
}

pub async fn build_connection(token: &str, reference: &str) -> Result<ProviderConnection, String> {
    let r: Value = serde_json::from_str(reference).map_err(|_| "That SingleStore reference is damaged. Refresh the list.".to_string())?;
    let id = r["id"].as_str().unwrap_or_default();
    if id.is_empty() {
        return Err("That SingleStore reference is incomplete. Refresh the list.".into());
    }
    let id_enc = urlencoding::encode(id);
    if r["kind"].as_str() == Some("starter") {
        let w = get(token, &format!("/sharedtier/virtualWorkspaces/{id_enc}"), "reading the starter workspace").await?;
        return connection_from(&w, "starter");
    }
    let w = get(token, &format!("/workspaces/{id_enc}"), "reading the workspace").await?;
    connection_from(&w, "workspace")
}

fn connection_from(w: &Value, kind: &str) -> Result<ProviderConnection, String> {
    let name = w["name"].as_str().unwrap_or("SingleStore").to_string();
    let state = w["state"].as_str().unwrap_or("ACTIVE").to_ascii_uppercase();
    if state == "SUSPENDED" {
        return Err(format!("{name} is suspended. Resume it in the SingleStore portal, then connect."));
    }
    let host = w["endpoint"].as_str().filter(|h| !h.is_empty()).ok_or_else(|| format!("{name} has no endpoint yet. It may still be starting."))?;
    let starter = kind == "starter";
    Ok(ProviderConnection {
        db_type: "mysql".into(),
        host: host.to_string(),
        port: w["mysqlDmlPort"].as_u64().and_then(|p| u16::try_from(p).ok()).unwrap_or(3306),
        username: if starter { w["username"].as_str().unwrap_or_default().to_string() } else { "admin".into() },
        password: String::new(),
        database: w["databaseName"].as_str().unwrap_or_default().to_string(),
        // SingleStore's cloud endpoints require TLS.
        ssl: true,
        needs_password: true,
        name: format!("SingleStore · {name}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspaces_connect_as_admin_over_tls_and_ask_for_the_password() {
        let w = serde_json::json!({ "name": "prod", "state": "ACTIVE", "endpoint": "svc-1.aws.svc.singlestore.com" });
        let c = connection_from(&w, "workspace").unwrap();
        assert_eq!((c.db_type.as_str(), c.port, c.username.as_str()), ("mysql", 3306, "admin"));
        assert!(c.ssl && c.needs_password);
        let s = serde_json::json!({ "name": "free", "endpoint": "svc-2.aws.svc.singlestore.com", "mysqlDmlPort": 3333, "databaseName": "db_free", "username": "u_1" });
        let c = connection_from(&s, "starter").unwrap();
        assert_eq!((c.port, c.database.as_str(), c.username.as_str()), (3333, "db_free", "u_1"));
        assert!(connection_from(&serde_json::json!({ "name": "x", "state": "SUSPENDED", "endpoint": "h" }), "workspace").unwrap_err().contains("suspended"));
        assert!(!active(Some("Terminated")) && active(Some("ACTIVE")) && active(None));
    }
}
