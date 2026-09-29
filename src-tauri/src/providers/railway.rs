/*!
 * Railway adapter - Postgres, MySQL and Redis services. Sign in with "Login with
 * Railway" (OAuth 2.0 + OIDC), list the database services across the projects
 * the user shared on the consent screen, and connect through the service's public
 * TCP proxy URL.
 *
 * Stroke registers as a Railway *native* app: a public client with PKCE and no
 * secret (the discovery document lists `none` among the token endpoint auth
 * methods), so the token exchange goes straight to Railway, like Neon. Access
 * tokens last an hour; `offline_access` (with `prompt=consent`) returns a refresh
 * token.
 */

use super::{http, OAuthConfig, ProviderConnection, ProviderDatabase};
use serde_json::{json, Value};

pub const OAUTH: OAuthConfig = OAuthConfig {
    // The native (public) OAuth app registered in Railway's workspace Developer
    // settings, redirect URI http://127.0.0.1:8989/oauth/callback. Public means
    // no secret: Railway rejects a token request that sends one (checked: the
    // same code with a secret gets `invalid_client`, without one `invalid_grant`).
    client_id: "rlwy_oaci_D7dCmSYWNMbBl3ZDVZc6nd7A",
    auth_url: "https://backboard.railway.com/oauth/auth",
    token_url: "https://backboard.railway.com/oauth/token",
    // workspace:viewer lists workspaces; project:member is what lets the token
    // read a service's variables, which is where the connection URL lives.
    scopes: "openid email profile offline_access workspace:viewer project:member",
};

const API: &str = "https://backboard.railway.com/graphql/v2";

async fn gql(token: &str, query: &str, variables: Value) -> Result<Value, String> {
    let resp = http()
        .post(API)
        .bearer_auth(token)
        .json(&json!({ "query": query, "variables": variables }))
        .send()
        .await
        .map_err(|e| format!("Railway request failed: {e}"))?;
    let status = resp.status().as_u16();
    if status == 401 {
        return Err(super::UNAUTHORIZED.into());
    }
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("Railway returned bad JSON: {e}"))?;
    if let Some(err) = body["errors"].as_array().and_then(|e| e.first()) {
        let msg = err["message"].as_str().unwrap_or("request failed");
        // GraphQL reports an expired or revoked token as an error, not a 401.
        if msg.to_ascii_lowercase().contains("not authorized") && body["data"].is_null() {
            return Err(super::UNAUTHORIZED.into());
        }
        return Err(format!("Railway API error: {msg}"));
    }
    if !(200..300).contains(&status) {
        return Err(format!("Railway API error ({status})"));
    }
    Ok(body["data"].clone())
}

/// Which engine a service runs, from its deploy image. Railway's database
/// templates are images (`ghcr.io/railwayapp-templates/postgres-ssl:17`,
/// `mysql:9`, `redis:8`); anything else (an app built from a repo) is skipped.
fn engine_of(image: &str) -> Option<&'static str> {
    let name = image.rsplit('/').next().unwrap_or(image).to_ascii_lowercase();
    let name = name.split(':').next().unwrap_or("");
    if name.starts_with("postgres") || name.starts_with("timescale") || name.starts_with("pgvector") {
        Some("postgres")
    } else if name.starts_with("mysql") || name.starts_with("mariadb") {
        Some("mysql")
    } else if name.starts_with("redis") || name.starts_with("valkey") {
        Some("redis")
    } else {
        None
    }
}

const PROJECTS_QUERY: &str = r#"
query StrokeProjects($workspaceId: String) {
  projects(first: 100, workspaceId: $workspaceId) {
    edges { node {
      id name
      environments { edges { node {
        id name
        serviceInstances { edges { node { serviceId serviceName source { image } } } }
      } } }
    } }
  }
}"#;

pub async fn list_databases(token: &str) -> Result<Vec<ProviderDatabase>, String> {
    let me = gql(token, "query { me { workspaces { id name } } }", json!({})).await?;
    let ids: Vec<String> = me["me"]["workspaces"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|w| w["id"].as_str().map(String::from))
        .collect();
    // Every workspace's projects at once. A workspace the token wasn't granted
    // errors on its own and is skipped rather than failing the whole list.
    let pages = futures::future::join_all(
        ids.iter().map(|id| gql(token, PROJECTS_QUERY, json!({ "workspaceId": id }))),
    )
    .await;
    let pages = super::merge_partial(pages, "")?;
    Ok(parse_projects(&pages))
}

/// `projects` pages → one row per database service instance. Services whose
/// image isn't a database template are skipped; a service deployed in several
/// environments gets the environment in its name.
fn parse_projects(pages: &[Value]) -> Vec<ProviderDatabase> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for page in pages {
        for p in page["projects"]["edges"].as_array().into_iter().flatten() {
            let p = &p["node"];
            let (Some(pid), Some(pname)) = (p["id"].as_str(), p["name"].as_str()) else { continue };
            let envs = p["environments"]["edges"].as_array().cloned().unwrap_or_default();
            let multi_env = envs.len() > 1;
            for e in &envs {
                let e = &e["node"];
                let (Some(eid), ename) = (e["id"].as_str(), e["name"].as_str().unwrap_or("")) else { continue };
                for si in e["serviceInstances"]["edges"].as_array().into_iter().flatten() {
                    let si = &si["node"];
                    let Some(sid) = si["serviceId"].as_str() else { continue };
                    let Some(engine) = si["source"]["image"].as_str().and_then(engine_of) else { continue };
                    if !seen.insert(format!("{eid}/{sid}")) {
                        continue;
                    }
                    let sname = si["serviceName"].as_str().unwrap_or(sid);
                    out.push(ProviderDatabase {
                        // Everything build_connection needs, so it can skip a lookup.
                        db_ref: json!({ "p": pid, "e": eid, "s": sid, "k": engine, "n": format!("{pname} / {sname}") })
                            .to_string(),
                        name: if multi_env { format!("{pname} / {sname} ({ename})") } else { format!("{pname} / {sname}") },
                        region: None,
                        kind: Some(match engine { "postgres" => "Postgres", "mysql" => "MySQL", _ => "Redis" }.into()),
                        host: None,
                    });
                }
            }
        }
    }
    out
}

pub async fn build_connection(token: &str, db_ref: &str) -> Result<ProviderConnection, String> {
    let r: Value = serde_json::from_str(db_ref).map_err(|_| "Invalid Railway service reference")?;
    let field = |k: &str| r[k].as_str().unwrap_or_default().to_string();
    let (engine, name) = (field("k"), field("n"));
    // Variables and TCP proxies in one request: the proxy is the fallback when
    // the template didn't write a *_PUBLIC_URL variable.
    let data = gql(
        token,
        "query StrokeVars($p: String!, $e: String!, $s: String!) { \
           variables(projectId: $p, environmentId: $e, serviceId: $s) \
           tcpProxies(environmentId: $e, serviceId: $s) { domain proxyPort applicationPort } }",
        json!({ "p": field("p"), "e": field("e"), "s": field("s") }),
    )
    .await?;
    // Diagnostics without secrets: which variables exist (names only) and what
    // proxies Railway reports, so a "no public access" can be traced.
    let var_names: Vec<&str> = data["variables"].as_object().map(|m| m.keys().map(String::as_str).collect()).unwrap_or_default();
    log::info!(
        "railway connect {name}: env={} service={} variables={var_names:?} tcpProxies={}",
        field("e"),
        field("s"),
        data["tcpProxies"]
    );
    connection_from_vars(&engine, &name, &data["variables"], &data["tcpProxies"]).map_err(|e| {
        // Point at the one screen that fixes it. Railway's public API has no
        // mutation to create a TCP proxy, so this can't be done from here.
        if e.contains("no public access") {
            format!(
                "{e} https://railway.com/project/{}/service/{}/settings?environmentId={}",
                field("p"),
                field("s"),
                field("e")
            )
        } else {
            e
        }
    })
}

/// A service's variables → a connection, through its PUBLIC address: the plain
/// URL points at `*.railway.internal`, which only resolves inside Railway's
/// network.
///
/// Preferably the template's `*_PUBLIC_URL`. When a service has a TCP proxy but
/// no such variable (custom images, older templates), the proxy's domain and
/// port are combined with the credentials the image reads from its variables.
fn connection_from_vars(engine: &str, name: &str, vars: &Value, proxies: &Value) -> Result<ProviderConnection, String> {
    let default_port = match engine { "postgres" => 5432, "mysql" => 3306, _ => 6379 };
    let named = |parts: (String, u16, String, String, String)| {
        let (host, port, username, password, database) = parts;
        ProviderConnection {
            db_type: engine.to_string(),
            host,
            port,
            username,
            password,
            database,
            // Railway's TCP proxy is plain TCP; the Postgres template serves TLS
            // itself (postgres-ssl) but does not require it, and MySQL/Redis there
            // are not TLS.
            ssl: false,
            needs_password: false,
            name: format!("Railway · {name}"),
        }
    };

    let url_keys: &[&str] = match engine {
        "postgres" => &["DATABASE_PUBLIC_URL"],
        "mysql" => &["MYSQL_PUBLIC_URL", "DATABASE_PUBLIC_URL"],
        _ => &["REDIS_PUBLIC_URL"],
    };
    // Right after public access is added, the template's URL can still be
    // half-rendered (`mysql://root:…@:/railway`): the proxy domain it references
    // only fills in on the next deploy. A URL without a host isn't an error to
    // report; fall through to the proxy itself.
    let public_url = url_keys
        .iter()
        .filter_map(|k| vars[*k].as_str().filter(|v| !v.is_empty()))
        .find_map(|raw| reqwest::Url::parse(raw).ok().filter(|u| u.host_str().is_some_and(|h| !h.is_empty())));
    if let Some(url) = public_url {
        let decode = |s: &str| urlencoding::decode(s).map(|c| c.into_owned()).unwrap_or_else(|_| s.to_string());
        return Ok(named((
            url.host_str().unwrap_or_default().to_string(),
            url.port().unwrap_or(default_port),
            decode(url.username()),
            decode(url.password().unwrap_or_default()),
            decode(url.path().trim_start_matches('/')),
        )));
    }

    let proxy = proxies.as_array().and_then(|list| {
        list.iter()
            .find(|p| p["applicationPort"].as_u64() == Some(u64::from(default_port)))
            .or_else(|| list.first())
    });
    if let Some(p) = proxy {
        let host = p["domain"].as_str().filter(|d| !d.is_empty());
        let port = p["proxyPort"].as_u64().and_then(|n| u16::try_from(n).ok());
        if let (Some(host), Some(port)) = (host, port) {
            let var = |keys: &[&str]| keys.iter().find_map(|k| vars[*k].as_str().filter(|v| !v.is_empty())).unwrap_or("").to_string();
            let (user, pass, db) = match engine {
                "postgres" => (
                    var(&["PGUSER", "POSTGRES_USER"]),
                    var(&["PGPASSWORD", "POSTGRES_PASSWORD"]),
                    var(&["PGDATABASE", "POSTGRES_DB"]),
                ),
                "mysql" => (
                    var(&["MYSQLUSER", "MYSQL_USER"]),
                    var(&["MYSQLPASSWORD", "MYSQL_ROOT_PASSWORD", "MYSQL_PASSWORD"]),
                    var(&["MYSQLDATABASE", "MYSQL_DATABASE"]),
                ),
                _ => (var(&["REDISUSER"]), var(&["REDISPASSWORD", "REDIS_PASSWORD"]), String::new()),
            };
            let user = if !user.is_empty() { user } else { match engine { "postgres" => "postgres", "mysql" => "root", _ => "default" }.into() };
            let db = if !db.is_empty() || engine == "redis" { db } else { match engine { "postgres" => "postgres", _ => "railway" }.into() };
            return Ok(named((host.to_string(), port, user, pass, db)));
        }
        // The proxy exists but Railway hasn't given it a public domain yet.
        return Err(format!(
            "{name}'s public access is still being set up: Railway hasn't assigned it an address yet. Give it a minute, then try again."
        ));
    }

    Err(format!(
        "{name} has no public access. Choose Add Public Access under Settings → Networking in Railway, or keep it private with `railway connect --tunnel-only`."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_database_services_only_and_names_environments() {
        let page = json!({ "projects": { "edges": [ { "node": {
            "id": "p1", "name": "shop",
            "environments": { "edges": [
                { "node": { "id": "e1", "name": "production", "serviceInstances": { "edges": [
                    { "node": { "serviceId": "s1", "serviceName": "Postgres", "source": { "image": "ghcr.io/railwayapp-templates/postgres-ssl:17" } } },
                    { "node": { "serviceId": "s2", "serviceName": "web", "source": { "repo": "me/web" } } }
                ] } } },
                { "node": { "id": "e2", "name": "staging", "serviceInstances": { "edges": [
                    { "node": { "serviceId": "s1", "serviceName": "Postgres", "source": { "image": "postgres-ssl:17" } } }
                ] } } }
            ] }
        } } ] } });
        let rows = parse_projects(&[page]);
        assert_eq!(rows.len(), 2, "the web app is not a database");
        assert_eq!(rows[0].name, "shop / Postgres (production)");
        let r: Value = serde_json::from_str(&rows[1].db_ref).unwrap();
        assert_eq!((r["e"].as_str(), r["k"].as_str()), (Some("e2"), Some("postgres")));
    }

    #[test]
    fn connects_through_the_public_url_and_decodes_credentials() {
        let vars = json!({
            "DATABASE_URL": "postgresql://postgres:x@postgres.railway.internal:5432/railway",
            "DATABASE_PUBLIC_URL": "postgresql://postgres:p%40ss@shortline.proxy.rlwy.net:41234/railway"
        });
        let c = connection_from_vars("postgres", "shop / Postgres", &vars, &json!([])).unwrap();
        assert_eq!((c.host.as_str(), c.port), ("shortline.proxy.rlwy.net", 41234));
        assert_eq!((c.username.as_str(), c.password.as_str(), c.database.as_str()), ("postgres", "p@ss", "railway"));
        let redis = json!({ "REDIS_PUBLIC_URL": "redis://default:pw@x.proxy.rlwy.net:6380" });
        assert_eq!(connection_from_vars("redis", "cache", &redis, &json!([])).unwrap().port, 6380);
        let private_only = json!({ "DATABASE_URL": "postgresql://u:p@postgres.railway.internal:5432/db" });
        assert!(connection_from_vars("postgres", "x", &private_only, &json!([])).unwrap_err().contains("no public access"));
    }

    #[test]
    fn a_half_rendered_url_or_a_proxy_without_a_domain_says_not_ready() {
        let vars = json!({ "MYSQL_PUBLIC_URL": "mysql://root:pw@:/railway", "MYSQL_ROOT_PASSWORD": "pw" });
        let pending = json!([{ "domain": "", "proxyPort": 0, "applicationPort": 3306 }]);
        assert!(connection_from_vars("mysql", "m", &vars, &pending).unwrap_err().contains("still being set up"));
        let ready = json!([{ "domain": "x.proxy.rlwy.net", "proxyPort": 4444, "applicationPort": 3306 }]);
        assert_eq!(connection_from_vars("mysql", "m", &vars, &ready).unwrap().port, 4444);
    }

    #[test]
    fn a_tcp_proxy_without_a_public_url_variable_still_connects() {
        let vars = json!({ "MYSQLUSER": "root", "MYSQL_ROOT_PASSWORD": "pw", "MYSQL_DATABASE": "railway" });
        let proxies = json!([{ "domain": "nozomi.proxy.rlwy.net", "proxyPort": 21337, "applicationPort": 3306 }]);
        let c = connection_from_vars("mysql", "luminous-flexibility / MySQL", &vars, &proxies).unwrap();
        assert_eq!((c.host.as_str(), c.port), ("nozomi.proxy.rlwy.net", 21337));
        assert_eq!((c.username.as_str(), c.password.as_str(), c.database.as_str()), ("root", "pw", "railway"));
    }

    #[test]
    fn engines_come_from_the_template_image() {
        assert_eq!(engine_of("ghcr.io/railwayapp-templates/postgres-ssl:17"), Some("postgres"));
        assert_eq!(engine_of("mysql:9"), Some("mysql"));
        assert_eq!(engine_of("bitnami/redis:7.2"), Some("redis"));
        assert_eq!(engine_of("ghcr.io/me/web-app:latest"), None);
    }
}
