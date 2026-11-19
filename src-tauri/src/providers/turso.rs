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

use super::SendRetry;
use super::{http, Choice, CreateOptions, CreateRequest, CreatedDatabase, OAuthConfig, ProviderConnection, ProviderDatabase};
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
        .send_retrying()
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

// ── Create a database ────────────────────────────────────────────────────────
//
// `POST /v1/organizations/{org}/databases { name, group }`. The group must
// exist: an organization that already has groups picks one; one with none (a
// brand-new account) picks a location instead, and Stroke first creates a
// `default` group there with `POST /v1/organizations/{org}/groups`, the same
// thing `turso db create` does on a fresh account. Turso databases are ready
// the moment the call returns.

const GROUP_PREFIX: &str = "group:";
const LOCATION_PREFIX: &str = "loc:";

async fn post(token: &str, path: &str, body: &Value, what: &str) -> Result<Value, String> {
    send(http().post(format!("{API}{path}")).bearer_auth(token).json(body), what).await
}

fn parse_orgs(body: &Value) -> Vec<Choice> {
    body["organizations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| {
            let slug = o["slug"].as_str()?;
            Some(Choice::new(slug, o["name"].as_str().filter(|n| !n.is_empty()).unwrap_or(slug)))
        })
        .collect()
}

/// `/groups` → group choices, labelled with their primary location.
fn parse_groups(body: &Value) -> Vec<Choice> {
    body["groups"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| {
            let name = g["name"].as_str()?;
            let label = match g["primary"].as_str() {
                Some(p) if !p.is_empty() => format!("{name} · {p}"),
                _ => name.to_string(),
            };
            Some(Choice::new(format!("{GROUP_PREFIX}{name}"), label))
        })
        .collect()
}

/// `/v1/locations` (`{ locations: { code: description } }`) → location choices.
fn parse_locations(body: &Value) -> Vec<Choice> {
    let mut out: Vec<Choice> = body["locations"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(code, desc)| Choice::new(format!("{LOCATION_PREFIX}{code}"), desc.as_str().unwrap_or(code)))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub async fn create_options(token: &str, scope: Option<&str>) -> Result<CreateOptions, String> {
    let scopes = parse_orgs(&get(token, "/v2/organizations", "listing organizations").await?);
    let scope = super::chosen_scope(&scopes, scope);
    let Some(org) = scope.clone() else {
        return Err("This Turso account has no organization to create a database in.".into());
    };
    let groups = parse_groups(&get(token, &format!("/v1/organizations/{}/groups", urlencoding::encode(&org)), "listing groups").await?);
    let (regions, region_label, preferred) = if groups.is_empty() {
        let locations = parse_locations(&get(token, "/v1/locations", "listing locations").await?);
        (locations, "Location", format!("{LOCATION_PREFIX}aws-us-east-1"))
    } else {
        (groups, "Group", format!("{GROUP_PREFIX}default"))
    };
    Ok(CreateOptions {
        scopes,
        scope_label: "Organization".into(),
        scope,
        default_region: super::pick_default(&regions, &preferred),
        regions,
        region_label: region_label.into(),
    })
}

/// What the form's region answer asks for: an existing group, or a location
/// to create the `default` group in first.
#[derive(Debug, PartialEq, Eq)]
enum Placement<'a> {
    Group(&'a str),
    NewGroupAt(&'a str),
}

fn placement(region: &str) -> Result<Placement<'_>, String> {
    if let Some(g) = region.strip_prefix(GROUP_PREFIX) {
        Ok(Placement::Group(g))
    } else if let Some(l) = region.strip_prefix(LOCATION_PREFIX) {
        Ok(Placement::NewGroupAt(l))
    } else {
        Err("Pick a group or a location.".into())
    }
}

/// Turso names: lowercase letters, numbers and dashes, at most 64 characters.
fn valid_name(name: &str) -> Result<(), String> {
    if name.len() > 64 || !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err("Turso database names use lowercase letters, numbers and dashes (up to 64).".into());
    }
    Ok(())
}

pub async fn create_database(token: &str, req: &CreateRequest) -> Result<CreatedDatabase, String> {
    let name = super::required_name(req)?;
    valid_name(name)?;
    let org = super::required(&req.scope, "Pick an organization.")?;
    let org_enc = urlencoding::encode(org);
    let group = match placement(super::required(&req.region, "Pick a group or a location.")?)? {
        Placement::Group(g) => g.to_string(),
        Placement::NewGroupAt(location) => {
            let made = post(
                token,
                &format!("/v1/organizations/{org_enc}/groups"),
                &serde_json::json!({ "name": "default", "location": location }),
                "creating a group",
            )
            .await;
            // A `default` group made since the form loaded is fine to use.
            if let Err(e) = made {
                if !e.contains("(409)") {
                    return Err(e);
                }
            }
            "default".to_string()
        }
    };
    let body = post(
        token,
        &format!("/v1/organizations/{org_enc}/databases"),
        &serde_json::json!({ "name": name, "group": group }),
        "creating a database",
    )
    .await?;
    let created = field(&body["database"], "Name", "name").unwrap_or(name);
    Ok(CreatedDatabase { db_ref: format!("{org}/{created}"), name: created.to_string(), ready: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn groups_or_locations_become_the_placement() {
        let g = parse_groups(&json!({ "groups": [ { "name": "default", "primary": "aws-us-east-1" } ] }));
        assert_eq!(g, vec![Choice::new("group:default", "default · aws-us-east-1")]);
        let l = parse_locations(&json!({ "locations": { "aws-us-east-1": "AWS US East (Virginia)", "aws-eu-west-1": "AWS EU West (Ireland)" } }));
        assert_eq!(l[0], Choice::new("loc:aws-eu-west-1", "AWS EU West (Ireland)"));
        assert_eq!(placement("group:default").unwrap(), Placement::Group("default"));
        assert_eq!(placement("loc:aws-us-east-1").unwrap(), Placement::NewGroupAt("aws-us-east-1"));
        assert!(placement("aws-us-east-1").is_err());
    }

    #[test]
    fn names_follow_turso_rules() {
        assert!(valid_name("shop-db-2").is_ok());
        assert!(valid_name("Shop").is_err());
        assert!(valid_name("shop_db").is_err());
        assert!(valid_name(&"a".repeat(65)).is_err());
    }

    #[test]
    fn orgs_fall_back_to_the_slug_for_a_name() {
        let o = parse_orgs(&json!({ "organizations": [ { "slug": "me", "name": "" }, { "slug": "acme", "name": "Acme" } ] }));
        assert_eq!(o, vec![Choice::new("me", "me"), Choice::new("acme", "Acme")]);
    }

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
