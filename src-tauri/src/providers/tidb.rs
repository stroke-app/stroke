/*!
 * TiDB Cloud adapter - MySQL wire protocol. Sign in with the OAuth 2.0 device
 * code grant, list the account's Starter/Essential clusters, and create a SQL
 * user on connect (TiDB Cloud never hands back an existing password, same as
 * PlanetScale).
 *
 * Like Neon, this reuses the official CLI's public OAuth client: TiDB Cloud does
 * not offer self-serve OAuth apps, and `ticloud auth login` authorizes with the
 * device code grant and this client id alone. The CLI's source ships a client
 * secret too, but device authorization and the token poll never send it, so
 * neither do we.
 */

use super::{http, OAuthConfig, ProviderConnection, ProviderDatabase};
use serde_json::{json, Value};

pub const OAUTH: OAuthConfig = OAuthConfig {
    // ticloud's client id (github.com/tidbcloud/tidbcloud-cli, internal/config).
    client_id: "wiSBy7f27zWBaBCxS16tDm7DDj2T3POgwFFbefTrgx8FAXKhzaPzv1Uta9NTck2r",
    auth_url: "https://oauth.tidbcloud.com/v1/device_authorization",
    token_url: "https://oauth.tidbcloud.com/v1/token",
    // The device grant takes no scope; the token carries the user's own access.
    scopes: "",
};

const SERVERLESS_API: &str = "https://serverless.tidbapi.com/v1beta1";
const IAM_API: &str = "https://iam.tidbapi.com/v1beta1";

/// Cap on cluster list pages, so a broken `nextPageToken` can't loop forever.
const MAX_PAGES: usize = 20;

async fn send(req: reqwest::RequestBuilder, what: &str) -> Result<Value, String> {
    // One retry when the request never got a response (a socket the far end had
    // already closed). Safe for the POST too: a request that failed to send
    // created nothing.
    let retry = req.try_clone();
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) if e.is_connect() || e.is_request() => match retry {
            Some(r) => r
                .send()
                .await
                .map_err(|e| format!("TiDB Cloud request failed ({what}): {}", super::describe(&e)))?,
            None => return Err(format!("TiDB Cloud request failed ({what}): {}", super::describe(&e))),
        },
        Err(e) => return Err(format!("TiDB Cloud request failed ({what}): {}", super::describe(&e))),
    };
    let status = resp.status().as_u16();
    // Before reading the body: a 401 may not be JSON, and the caller refreshes on it.
    if status == 401 {
        return Err(super::UNAUTHORIZED.into());
    }
    let text = resp
        .text()
        .await
        .map_err(|e| format!("TiDB Cloud read failed: {e}"))?;
    let body: Value = serde_json::from_str(&text).map_err(|_| {
        let snippet: String = text.chars().take(200).collect();
        format!("TiDB Cloud non-JSON response for {what} (HTTP {status}): {snippet}")
    })?;
    if !(200..300).contains(&status) {
        let msg = body["message"]
            .as_str()
            .or_else(|| body["error"]["message"].as_str())
            .unwrap_or("request failed");
        return Err(format!("TiDB Cloud API error ({status}) on {what}: {msg}"));
    }
    Ok(body)
}

async fn get(token: &str, url: String, what: &str) -> Result<Value, String> {
    send(http().get(url).bearer_auth(token), what).await
}

/// Each Starter/Essential cluster is one connectable database.
pub async fn list_databases(token: &str) -> Result<Vec<ProviderDatabase>, String> {
    let mut out = Vec::new();
    let mut page_token = String::new();
    for _ in 0..MAX_PAGES {
        let mut url = format!("{SERVERLESS_API}/clusters?pageSize=100");
        if !page_token.is_empty() {
            url.push_str(&format!("&pageToken={}", urlencoding::encode(&page_token)));
        }
        let body = get(token, url, "listing clusters").await?;
        out.extend(parse_clusters(&body));
        match body["nextPageToken"].as_str() {
            Some(next) if !next.is_empty() => page_token = next.to_string(),
            _ => break,
        }
    }
    Ok(out)
}

/// One `clusters` page → picker rows.
fn parse_clusters(body: &Value) -> Vec<ProviderDatabase> {
    body["clusters"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let id = c["clusterId"].as_str()?;
            Some(ProviderDatabase {
                db_ref: id.to_string(),
                name: c["displayName"].as_str().unwrap_or(id).to_string(),
                region: c["region"]["displayName"]
                    .as_str()
                    .or_else(|| c["region"]["regionId"].as_str())
                    .map(String::from),
                kind: Some("Cluster".into()),
                host: c["endpoints"]["public"]["host"].as_str().map(String::from),
            })
        })
        .collect()
}

/// Whether a cluster can be connected to right now, and its public endpoint.
/// Errors are worded for the picker's toast ("is paused", "still starting").
fn endpoint_of(c: &Value, name: &str) -> Result<(String, u16), String> {
    match c["state"].as_str().unwrap_or("ACTIVE") {
        "ACTIVE" => {}
        "PAUSED" | "PAUSING" | "INACTIVE" => {
            return Err(format!("{name} is paused. Resume it from the TiDB Cloud console, then try again."))
        }
        s @ ("CREATING" | "RESUMING" | "RESTORING" | "UPGRADING" | "MODIFYING" | "MAINTENANCE" | "IMPORTING") => {
            return Err(format!("{name} is still starting up ({s}). Give it a moment and try again."))
        }
        s => return Err(format!("{name} is not available to connect to ({s}).")),
    }
    let public = &c["endpoints"]["public"];
    if public["disabled"].as_bool() == Some(true) {
        return Err(format!(
            "{name} has its public endpoint turned off. Enable it in the TiDB Cloud console to connect from Stroke."
        ));
    }
    let host = public["host"]
        .as_str()
        .ok_or_else(|| format!("TiDB Cloud returned no public host for {name}"))?
        .to_string();
    let port = public["port"].as_u64().and_then(|p| u16::try_from(p).ok()).unwrap_or(4000);
    Ok((host, port))
}

/// The SQL user name the server created: with `autoPrefix` it is
/// `{userPrefix}.{userName}`, echoed back in the response when present.
fn created_user(created: &Value, prefix: &str, user: &str) -> String {
    created["userName"]
        .as_str()
        .filter(|u| u.contains('.'))
        .map(String::from)
        .unwrap_or_else(|| if prefix.is_empty() { user.to_string() } else { format!("{prefix}.{user}") })
}

/// Alphanumeric only: TiDB accepts symbols in passwords, but a URL-shaped
/// character in one has to be escaped everywhere the credential travels.
fn alnum(n: usize) -> String {
    super::random_base64url(n * 2)
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(n)
        .collect()
}

pub async fn build_connection(token: &str, cluster_id: &str) -> Result<ProviderConnection, String> {
    let c = get(
        token,
        format!("{SERVERLESS_API}/clusters/{}", urlencoding::encode(cluster_id)),
        "reading the cluster",
    )
    .await?;
    let name = c["displayName"].as_str().unwrap_or(cluster_id).to_string();

    let (host, port) = endpoint_of(&c, &name)?;
    let prefix = c["userPrefix"].as_str().unwrap_or_default();

    // A fresh admin user per connect, like PlanetScale's minted password. The
    // frontend reuses a saved connection's credentials when it has them, so
    // this only runs for a cluster Stroke hasn't connected to before.
    let user = format!("stroke_{}", alnum(6).to_lowercase());
    let password = alnum(24);
    let created = send(
        http()
            .post(format!("{IAM_API}/clusters/{}/sqlUsers", urlencoding::encode(cluster_id)))
            .bearer_auth(token)
            .json(&json!({
                "userName": user,
                "password": password,
                "builtinRole": "role_admin",
                "authMethod": "mysql_native_password",
                "autoPrefix": true,
            })),
        "creating a SQL user",
    )
    .await?;
    let username = created_user(&created, prefix, &user);

    Ok(ProviderConnection {
        db_type: "mysql".into(),
        host,
        port,
        username,
        password,
        // No default schema: the sidebar lists every database on the cluster.
        database: String::new(),
        ssl: true,
        needs_password: false,
        name: format!("TiDB · {name}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn clusters_list_with_region_and_host() {
        let body = json!({ "clusters": [
            { "clusterId": "1094", "displayName": "test", "region": { "regionId": "aws-ap-northeast-1", "displayName": "Tokyo (ap-northeast-1)" },
              "endpoints": { "public": { "host": "gateway01.ap-northeast-1.prod.aws.tidbcloud.com", "port": 4000 } } },
            { "displayName": "no id, skipped" }
        ]});
        let rows = parse_clusters(&body);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].region.as_deref(), Some("Tokyo (ap-northeast-1)"));
    }

    #[test]
    fn cluster_state_and_endpoint_gate_the_connect() {
        let ok = json!({ "state": "ACTIVE", "endpoints": { "public": { "host": "h", "port": 4000 } } });
        assert_eq!(endpoint_of(&ok, "t").unwrap(), ("h".into(), 4000));
        assert!(endpoint_of(&json!({ "state": "PAUSED" }), "t").unwrap_err().contains("is paused"));
        assert!(endpoint_of(&json!({ "state": "RESUMING" }), "t").unwrap_err().contains("still starting"));
        let off = json!({ "state": "ACTIVE", "endpoints": { "public": { "disabled": true } } });
        assert!(endpoint_of(&off, "t").unwrap_err().contains("public endpoint"));
    }

    #[test]
    fn sql_user_gets_the_cluster_prefix() {
        assert_eq!(created_user(&json!({ "userName": "3Xq.stroke_ab" }), "3Xq", "stroke_ab"), "3Xq.stroke_ab");
        assert_eq!(created_user(&json!({}), "3Xq", "stroke_ab"), "3Xq.stroke_ab");
        assert_eq!(created_user(&json!({}), "", "stroke_ab"), "stroke_ab");
    }
}
