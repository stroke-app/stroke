/*!
 * Convex adapter - the provider-picker side of the Convex engine
 * (`db/convex.rs`). Sign-in is Convex's OAuth app flow for team tokens: the
 * browser asks which team to authorize, and every cloud deployment on that
 * team is listed. Connecting mints a deploy key for the picked deployment
 * through the Management API, so no key is copied by hand.
 *
 * Convex OAuth apps are confidential clients, so the code exchange goes through
 * the stroke.click proxy, which adds CONVEX_CLIENT_SECRET. The app token has no
 * expiry or refresh token; a revoked one comes back as a 401 and the card asks
 * to sign in again. A project token also works here (`Scope::Project`).
 */

use super::SendRetry;
use super::{http, OAuthConfig, ProviderConnection, ProviderDatabase};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Stroke's OAuth app, registered in the Convex dashboard under Team Settings,
/// Applications, with the redirect URIs `http://localhost:{8989..8993}/oauth/callback`.
/// The scope comes from the authorize path (team), not a `scope` parameter.
pub const OAUTH: OAuthConfig = OAuthConfig {
    client_id: "573ebae448004ff7",
    auth_url: "https://dashboard.convex.dev/oauth/authorize/team",
    token_url: "https://api.convex.dev/oauth/token",
    scopes: "",
};

const API: &str = "https://api.convex.dev/v1";
/// Cap on listing pages (100 a page).
const MAX_PAGES: usize = 20;

async fn send(req: reqwest::RequestBuilder) -> Result<Value, String> {
    let resp = req.send_retrying().await.map_err(|e| format!("Convex request failed: {}", super::describe(&e)))?;
    let status = resp.status().as_u16();
    // A wrong or revoked token is a 401; twice in a row the command layer
    // ends the "session" and the token form comes back.
    if status == 401 {
        return Err(super::UNAUTHORIZED.into());
    }
    let text = resp.text().await.map_err(|e| format!("Convex read failed: {e}"))?;
    let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        let detail = body["message"].as_str().map(String::from).unwrap_or_else(|| text.chars().take(200).collect());
        return Err(if status == 403 {
            format!("Convex refused the token: {detail}")
        } else {
            format!("Convex API error ({status}): {detail}")
        });
    }
    Ok(body)
}

async fn get(token: &str, path: &str) -> Result<Value, String> {
    send(http().get(format!("{API}{path}")).bearer_auth(token.trim())).await
}

/// What the token can see: a whole team, or one project.
#[derive(Debug, PartialEq)]
enum Scope {
    Team(String),
    Project(String),
}

fn id_str(v: &Value) -> Option<String> {
    v.as_i64().map(|n| n.to_string()).or_else(|| v.as_str().map(String::from))
}

fn scope_of(details: &Value) -> Result<Scope, String> {
    match details["type"].as_str() {
        Some("teamToken") => id_str(&details["teamId"]).map(Scope::Team),
        Some("projectToken") => id_str(&details["projectId"]).map(Scope::Project),
        _ => None,
    }
    .ok_or_else(|| "This token is not a Convex team or project access token.".to_string())
}

/// Every page of a cursor-paginated `{ items, pagination }` listing.
async fn all_pages(token: &str, path: &str) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let sep = if path.contains('?') { '&' } else { '?' };
        let url = match &cursor {
            Some(c) => format!("{path}{sep}limit=100&cursor={}", urlencoding::encode(c)),
            None => format!("{path}{sep}limit=100"),
        };
        let body = get(token, &url).await?;
        out.extend(body["items"].as_array().cloned().unwrap_or_default());
        match (body["pagination"]["hasMore"].as_bool(), body["pagination"]["nextCursor"].as_str()) {
            (Some(true), Some(c)) if !c.is_empty() => cursor = Some(c.to_string()),
            _ => break,
        }
    }
    Ok(out)
}

/// Deployments → picker rows, `Project · prod` style, production first.
/// `db_ref` carries everything connecting needs, so it makes no second lookup.
fn rows(deployments: &[Value], projects: &HashMap<String, String>) -> Vec<ProviderDatabase> {
    let mut out: Vec<(u8, ProviderDatabase)> = deployments
        .iter()
        // Local deployments run on someone's laptop: nothing to reach.
        .filter(|d| d["kind"].as_str().unwrap_or("cloud") == "cloud")
        .filter_map(|d| {
            let name = d["name"].as_str()?.to_string();
            let url = d["deploymentUrl"].as_str().map(String::from).unwrap_or_else(|| format!("https://{name}.convex.cloud"));
            let kind = d["deploymentType"].as_str().unwrap_or("dev").to_string();
            let project = id_str(&d["projectId"]).and_then(|p| projects.get(&p).cloned());
            let label = match (&project, d["reference"].as_str()) {
                (Some(p), Some(r)) if kind == "preview" || kind == "custom" => format!("{p} · {r}"),
                (Some(p), _) => format!("{p} · {kind}"),
                (None, _) => name.clone(),
            };
            let rank = match kind.as_str() {
                "prod" => 0,
                "dev" => 1,
                _ => 2,
            };
            Some((
                rank,
                ProviderDatabase {
                    db_ref: json!({ "d": name, "u": url, "n": label }).to_string(),
                    name: label,
                    region: d["region"].as_str().map(String::from),
                    kind: Some(match kind.as_str() {
                        "prod" => "Production".into(),
                        "dev" => "Development".into(),
                        "preview" => "Preview".into(),
                        _ => "Custom".into(),
                    }),
                    host: Some(url.trim_start_matches("https://").to_string()),
                },
            ))
        })
        .collect();
    out.sort_by(|a, b| a.1.name.split(" · ").next().cmp(&b.1.name.split(" · ").next()).then(a.0.cmp(&b.0)).then_with(|| a.1.name.cmp(&b.1.name)));
    out.into_iter().map(|(_, d)| d).collect()
}

pub async fn list_databases(token: &str) -> Result<Vec<ProviderDatabase>, String> {
    let details = get(token, "/token_details").await?;
    match scope_of(&details)? {
        Scope::Team(team) => {
            let (dep_path, proj_path) = (format!("/teams/{team}/list_deployments"), format!("/teams/{team}/projects"));
            let (deployments, projects) = tokio::try_join!(all_pages(token, &dep_path), all_pages(token, &proj_path))?;
            let names = projects.iter().filter_map(|p| Some((id_str(&p["id"])?, p["name"].as_str()?.to_string()))).collect();
            Ok(rows(&deployments, &names))
        }
        Scope::Project(project) => {
            let (dep_path, proj_path) = (format!("/projects/{project}/list_deployments"), format!("/projects/{project}"));
            let (deployments, p) = tokio::try_join!(get(token, &dep_path), get(token, &proj_path))?;
            let names = HashMap::from([(project, p["name"].as_str().unwrap_or("Project").to_string())]);
            Ok(rows(deployments.as_array().map(Vec::as_slice).unwrap_or_default(), &names))
        }
    }
}

pub async fn build_connection(token: &str, db_ref: &str) -> Result<ProviderConnection, String> {
    let r: Value = serde_json::from_str(db_ref).map_err(|_| "Invalid Convex deployment reference")?;
    let deployment = r["d"].as_str().filter(|d| !d.is_empty()).ok_or("Invalid Convex deployment reference")?;
    let url = r["u"].as_str().unwrap_or_default();
    let label = r["n"].as_str().unwrap_or(deployment);
    // A deploy key, shown in the deployment's settings as "Stroke", revocable there.
    let key = send(
        http()
            .post(format!("{API}/deployments/{}/create_deploy_key", urlencoding::encode(deployment)))
            .bearer_auth(token.trim())
            .json(&json!({ "name": "Stroke" })),
    )
    .await?;
    let key = key["deployKey"].as_str().ok_or("Convex returned no deploy key")?;
    // The engine's adapter contract: the deployment URL in `host`, the key in
    // `password`. The frontend maps these to a `convex` connection ({ url, adminKey }).
    Ok(ProviderConnection {
        db_type: "convex".into(),
        host: if url.is_empty() { format!("https://{deployment}.convex.cloud") } else { url.to_string() },
        port: 443,
        username: String::new(),
        password: key.to_string(),
        database: deployment.to_string(),
        ssl: true,
        needs_password: false,
        name: format!("Convex · {label}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_scope() {
        assert_eq!(scope_of(&json!({ "type": "teamToken", "teamId": 41 })).unwrap(), Scope::Team("41".into()));
        assert_eq!(scope_of(&json!({ "type": "projectToken", "projectId": 7 })).unwrap(), Scope::Project("7".into()));
        assert!(scope_of(&json!({ "type": "other" })).is_err());
    }

    #[test]
    fn deployments_become_rows() {
        let deps = json!([
            { "name": "happy-otter-123", "deploymentType": "dev", "projectId": 7, "region": "aws-us-east-1", "deploymentUrl": "https://happy-otter-123.convex.cloud", "kind": "cloud" },
            { "name": "brave-cat-9", "deploymentType": "prod", "projectId": 7, "deploymentUrl": "https://brave-cat-9.convex.cloud", "kind": "cloud" },
            { "name": "pr-42", "deploymentType": "preview", "projectId": 7, "reference": "feat-x", "deploymentUrl": "https://pr-42.convex.cloud", "kind": "cloud" },
            { "name": "local-1", "deploymentType": "dev", "projectId": 7, "kind": "local" },
        ]);
        let names = HashMap::from([("7".to_string(), "Chat".to_string())]);
        let r = rows(deps.as_array().unwrap(), &names);
        assert_eq!(r.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["Chat · prod", "Chat · dev", "Chat · feat-x"]);
        assert_eq!(r[0].kind.as_deref(), Some("Production"));
        assert_eq!(r[1].host.as_deref(), Some("happy-otter-123.convex.cloud"));
        let rf: Value = serde_json::from_str(&r[1].db_ref).unwrap();
        assert_eq!((rf["d"].as_str(), rf["u"].as_str()), (Some("happy-otter-123"), Some("https://happy-otter-123.convex.cloud")));
        // No project name: the deployment's own name.
        assert_eq!(rows(&deps.as_array().unwrap()[..1], &HashMap::new())[0].name, "happy-otter-123");
    }
}
