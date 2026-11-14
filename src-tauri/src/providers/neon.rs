/*!
 * Neon adapter - the reference implementation. Neon is a public PKCE OAuth
 * client and its API hands back a full `postgres://` connection URI (password
 * included), so sign-in → list projects → one-click connect works end-to-end.
 */

use super::{http, Choice, CreateOptions, CreateRequest, CreatedDatabase, DatabaseStatus, OAuthConfig, ProviderConnection, ProviderDatabase};
use serde_json::Value;

pub const OAUTH: OAuthConfig = OAuthConfig {
    // Neon doesn't offer self-serve OAuth apps (partner-only), so - like we do
    // with Cloudflare/Wrangler - we reuse the official neonctl CLI's PUBLIC PKCE
    // client. No secret, no proxy: the token exchange goes straight to Neon
    // (see Provider::is_public_client). Redirect must be 127.0.0.1/callback,
    // which neonctl's client registered (see Provider::redirect_uri).
    client_id: "neonctl",
    auth_url: "https://oauth2.neon.tech/oauth2/auth",
    token_url: "https://oauth2.neon.tech/oauth2/token",
    // `projects:update` is not optional, and it is not about writing anything.
    // Neon's scope table maps `projects:read` to Viewer and `projects:update`
    // ("Modify Projects") to Editor, and reading `/connection_uri` hands back the
    // role's password - so Neon gates it behind Editor. Without this scope the
    // whole flow works right up to the last call and then fails with
    // "Editor access is required to read project credentials", which reads like a
    // permissions problem on the user's account rather than a missing scope on
    // ours. Neon's own documented example request asks for `update` too.
    //
    // Adding a scope does not upgrade a token that was already issued: anyone
    // signed in before this must disconnect and authorize again.
    scopes: "openid offline offline_access urn:neoncloud:projects:read urn:neoncloud:projects:create urn:neoncloud:projects:update urn:neoncloud:orgs:read",
};

const API: &str = "https://console.neon.tech/api/v2";

async fn get(token: &str, path: &str) -> Result<Value, String> {
    let resp = http()
        .get(format!("{API}{path}"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Neon request failed: {e}"))?;
    let status = resp.status().as_u16();
    // Before reading the body: a 401 may not be JSON, and the caller refreshes on it.
    if status == 401 {
        return Err(super::UNAUTHORIZED.into());
    }
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Neon read failed: {e}"))?;
    let body: Value = serde_json::from_str(&text).map_err(|_| {
        let snippet: String = text.chars().take(200).collect();
        format!("Neon non-JSON response from {path} (HTTP {status}): {snippet}")
    })?;
    if status != 200 {
        let msg = body["message"].as_str().unwrap_or("request failed");
        return Err(format!("Neon API error ({status}) on {path}: {msg}"));
    }
    Ok(body)
}

/// Each Neon project is presented as one connectable database. Neon scopes
/// projects under organizations, so `/projects` requires an `org_id` - list the
/// user's orgs first, then each org's projects, and flatten them.
pub async fn list_databases(token: &str) -> Result<Vec<ProviderDatabase>, String> {
    let orgs_body = get(token, "/users/me/organizations").await?;

    // One request per org, all in flight at once: with several orgs the serial
    // loop cost a full round-trip each before the picker could show anything.
    // try_join_all keeps the orgs' order, so the list reads the same as before.
    let org_ids: Vec<&str> = orgs_body["organizations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|org| org["id"].as_str())
        .collect();
    let pages = futures::future::join_all(org_ids.iter().map(|org_id| {
        let path = format!("/projects?org_id={}", urlencoding::encode(org_id));
        async move { get(token, &path).await }
    }))
    .await;
    // An org the token can't read is skipped rather than failing the list.
    let pages = super::merge_partial(pages, "")?;

    let mut out = Vec::new();
    for body in &pages {
        for p in body["projects"].as_array().into_iter().flatten() {
            if let Some(id) = p["id"].as_str() {
                out.push(ProviderDatabase {
                    db_ref: id.to_string(),
                    name: p["name"].as_str().unwrap_or(id).to_string(),
                    region: p["region_id"].as_str().map(String::from),
                    kind: Some("Project".into()),
                    host: None,
                });
            }
        }
    }
    Ok(out)
}

/// Resolve a project → default branch → its first database + owner role →
/// connection URI. Databases/roles live under a branch in Neon, not the project.
pub async fn build_connection(token: &str, project_id: &str) -> Result<ProviderConnection, String> {
    // Find the project's default branch (fall back to the first).
    let branches = get(token, &format!("/projects/{project_id}/branches")).await?;
    let branch_id = default_branch(&branches)?;
    let branch_id = branch_id.as_str();

    let dbs = get(token, &format!("/projects/{project_id}/branches/{branch_id}/databases")).await?;
    let (db_name, role) = first_database(&dbs)?;
    let (db_name, role) = (db_name.as_str(), role.as_str());

    let uri_body = get(
        token,
        &format!(
            "/projects/{project_id}/connection_uri?branch_id={}&database_name={}&role_name={}&pooled=true",
            urlencoding::encode(branch_id),
            urlencoding::encode(db_name),
            urlencoding::encode(role),
        ),
    )
    .await?;
    let uri = uri_body["uri"]
        .as_str()
        .ok_or("Neon did not return a connection URI")?;

    let (host, port, username, password, database) = parse_pg_uri(uri)?;
    Ok(ProviderConnection {
        db_type: "postgres".into(),
        host,
        port,
        username,
        password,
        database,
        ssl: true,
        needs_password: false,
        name: format!("Neon · {db_name}"),
    })
}

/// The project's default branch (Neon also calls it `primary`), else the first.
fn default_branch(branches: &Value) -> Result<String, String> {
    let list = branches["branches"].as_array().cloned().unwrap_or_default();
    let branch = list
        .iter()
        .find(|b| b["default"].as_bool() == Some(true) || b["primary"].as_bool() == Some(true))
        .or_else(|| list.first())
        .ok_or("This Neon project has no branches")?;
    branch["id"].as_str().map(String::from).ok_or_else(|| "Neon: missing branch id".into())
}

/// The branch's first database and the role that owns it.
fn first_database(dbs: &Value) -> Result<(String, String), String> {
    let first = dbs["databases"]
        .as_array()
        .and_then(|a| a.first())
        .ok_or("This Neon branch has no databases yet")?;
    let name = first["name"].as_str().ok_or("Neon: missing database name")?;
    let role = first["owner_name"].as_str().ok_or("Neon: missing owner role")?;
    Ok((name.to_string(), role.to_string()))
}

/// Parse `postgres://user:pass@host[:port]/db?query` into its parts (URL-decoding
/// the userinfo, which Neon percent-encodes).
pub(crate) fn parse_pg_uri(uri: &str) -> Result<(String, u16, String, String, String), String> {
    let rest = uri.split("://").nth(1).ok_or("Malformed connection URI")?;
    let (creds_host, db_and_query) = rest.split_once('/').ok_or("Connection URI has no database")?;
    let (creds, hostport) = creds_host
        .rsplit_once('@')
        .ok_or("Connection URI has no credentials")?;
    let (user_raw, pass_raw) = creds.split_once(':').unwrap_or((creds, ""));

    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => {
            (h.to_string(), p.parse().unwrap_or(5432))
        }
        _ => (hostport.to_string(), 5432),
    };
    let database = db_and_query
        .split(['?', '&'])
        .next()
        .unwrap_or("")
        .to_string();

    let dec = |s: &str| urlencoding::decode(s).map(|c| c.into_owned()).unwrap_or_else(|_| s.to_string());
    Ok((host, port, dec(user_raw), dec(pass_raw), dec(&database)))
}

// ── Create a project ─────────────────────────────────────────────────────────
//
// `POST /projects` with `urn:neoncloud:projects:create`, which the sign-in
// already requests. A project is Neon's unit of "a database" in the picker (one
// default branch, one `neondb`), so creating one is what "Create database"
// means here. Neon publishes no region-list endpoint; the ids below are the
// ones its Regions page lists for new projects (Azure regions are closed to new
// projects).

const REGIONS: &[(&str, &str)] = &[
    ("aws-us-east-1", "AWS US East (N. Virginia)"),
    ("aws-us-east-2", "AWS US East (Ohio)"),
    ("aws-us-west-2", "AWS US West (Oregon)"),
    ("aws-eu-central-1", "AWS Europe (Frankfurt)"),
    ("aws-eu-west-2", "AWS Europe (London)"),
    ("aws-ap-southeast-1", "AWS Asia Pacific (Singapore)"),
    ("aws-ap-southeast-2", "AWS Asia Pacific (Sydney)"),
    ("aws-sa-east-1", "AWS South America (São Paulo)"),
];

async fn post(token: &str, path: &str, body: &Value) -> Result<Value, String> {
    let resp = http()
        .post(format!("{API}{path}"))
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("Neon request failed: {e}"))?;
    let status = resp.status().as_u16();
    if status == 401 {
        return Err(super::UNAUTHORIZED.into());
    }
    let text = resp.text().await.map_err(|e| format!("Neon read failed: {e}"))?;
    let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        let msg = body["message"].as_str().map(String::from).unwrap_or_else(|| text.chars().take(160).collect());
        return Err(format!("Neon API error ({status}): {msg}"));
    }
    Ok(body)
}

/// `/users/me/organizations` → organization choices.
fn parse_orgs(body: &Value) -> Vec<Choice> {
    body["organizations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| {
            let id = o["id"].as_str()?;
            Some(Choice::new(id, o["name"].as_str().unwrap_or(id)))
        })
        .collect()
}

pub async fn create_options(token: &str, scope: Option<&str>) -> Result<CreateOptions, String> {
    let scopes = parse_orgs(&get(token, "/users/me/organizations").await?);
    let regions = super::static_choices(REGIONS);
    Ok(CreateOptions {
        scope: super::chosen_scope(&scopes, scope),
        scopes,
        scope_label: "Organization".into(),
        default_region: super::pick_default(&regions, "aws-us-east-2"),
        regions,
        region_label: "Region".into(),
    })
}

fn create_body(req: &CreateRequest) -> Result<Value, String> {
    let name = super::required_name(req)?;
    let region = super::required(&req.region, "Pick a region.")?;
    let mut project = serde_json::json!({ "name": name, "region_id": region });
    if let Some(org) = req.scope.as_deref().filter(|s| !s.is_empty()) {
        project["org_id"] = Value::String(org.to_string());
    }
    Ok(serde_json::json!({ "project": project }))
}

/// The create response → the new project. Ready once no operation is running.
fn parse_created(body: &Value) -> Result<CreatedDatabase, String> {
    let id = body["project"]["id"].as_str().ok_or("Neon created the project but returned no id")?;
    Ok(CreatedDatabase {
        db_ref: id.to_string(),
        name: body["project"]["name"].as_str().unwrap_or(id).to_string(),
        ready: operations_status(body).ready,
    })
}

/// A project's operations → whether it is ready. Neon starts the compute and
/// creates the branch as operations; connecting before they finish can fail.
fn operations_status(body: &Value) -> DatabaseStatus {
    let ops: Vec<&str> = body["operations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| o["status"].as_str())
        .collect();
    if ops.iter().any(|s| *s == "failed" || *s == "error") {
        return DatabaseStatus::failed("failed");
    }
    match ops.iter().find(|s| !matches!(**s, "finished" | "skipped" | "cancelled")) {
        Some(s) => DatabaseStatus::pending(s),
        None => DatabaseStatus::ready("ready"),
    }
}

pub async fn create_database(token: &str, req: &CreateRequest) -> Result<CreatedDatabase, String> {
    parse_created(&post(token, "/projects", &create_body(req)?).await?)
}

pub async fn database_status(token: &str, project_id: &str) -> Result<DatabaseStatus, String> {
    let body = get(token, &format!("/projects/{}/operations?limit=20", urlencoding::encode(project_id))).await?;
    Ok(operations_status(&body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn create_body_carries_the_org_and_region() {
        let req = CreateRequest { name: " shop ".into(), scope: Some("org-1".into()), region: Some("aws-us-east-2".into()), password: None };
        assert_eq!(
            create_body(&req).unwrap(),
            json!({ "project": { "name": "shop", "region_id": "aws-us-east-2", "org_id": "org-1" } })
        );
        let no_org = CreateRequest { name: "x".into(), region: Some("aws-us-east-1".into()), ..Default::default() };
        assert!(create_body(&no_org).unwrap()["project"].get("org_id").is_none());
        assert!(create_body(&CreateRequest { name: "x".into(), ..Default::default() }).is_err());
    }

    #[test]
    fn created_project_is_ready_when_its_operations_finish() {
        let body = json!({ "project": { "id": "gentle-river-1", "name": "shop" },
            "operations": [ { "status": "finished" }, { "status": "running" } ] });
        assert_eq!(parse_created(&body).unwrap(), CreatedDatabase { db_ref: "gentle-river-1".into(), name: "shop".into(), ready: false });
        assert!(operations_status(&json!({ "operations": [ { "status": "finished" } ] })).ready);
        assert!(operations_status(&json!({ "operations": [] })).ready);
        assert!(operations_status(&json!({ "operations": [ { "status": "failed" } ] })).failed);
        assert!(parse_created(&json!({})).is_err());
    }

    #[test]
    fn orgs_become_choices() {
        let orgs = parse_orgs(&json!({ "organizations": [ { "id": "org-1", "name": "Acme" }, { "name": "no id" } ] }));
        assert_eq!(orgs, vec![Choice::new("org-1", "Acme")]);
    }

    #[test]
    fn default_branch_is_preferred_over_the_first() {
        let b = json!({ "branches": [ { "id": "br-dev" }, { "id": "br-main", "default": true } ] });
        assert_eq!(default_branch(&b).unwrap(), "br-main");
        let legacy = json!({ "branches": [ { "id": "br-a" }, { "id": "br-b", "primary": true } ] });
        assert_eq!(default_branch(&legacy).unwrap(), "br-b");
        assert_eq!(default_branch(&json!({ "branches": [ { "id": "only" } ] })).unwrap(), "only");
        assert!(default_branch(&json!({ "branches": [] })).is_err());
    }

    #[test]
    fn first_database_carries_its_owner_role() {
        let dbs = json!({ "databases": [ { "name": "neondb", "owner_name": "neondb_owner" } ] });
        assert_eq!(first_database(&dbs).unwrap(), ("neondb".into(), "neondb_owner".into()));
        assert!(first_database(&json!({ "databases": [] })).unwrap_err().contains("no databases"));
    }

    #[test]
    fn connection_uri_decodes_credentials_and_drops_the_query() {
        let (h, p, u, pw, d) = parse_pg_uri(
            "postgresql://neondb_owner:p%40ss%2Fw@ep-x-pooler.c-4.us-east-1.aws.neon.tech/neondb?sslmode=require&channel_binding=require",
        )
        .unwrap();
        assert_eq!((h.as_str(), p), ("ep-x-pooler.c-4.us-east-1.aws.neon.tech", 5432));
        assert_eq!((u.as_str(), pw.as_str(), d.as_str()), ("neondb_owner", "p@ss/w", "neondb"));
        let (_, port, ..) = parse_pg_uri("postgres://u:p@h:6543/db").unwrap();
        assert_eq!(port, 6543);
    }
}
