use crate::proc::quiet;
use std::process::Stdio;
use tauri::Emitter;
use tokio::io::AsyncBufReadExt;
use tokio::process::Command as Cmd;

/// `docker …`, with no console window on Windows. The container scan runs on the
/// connection screen, so a bare spawn flashed a cmd.exe window at app start.
fn docker() -> Cmd {
    let mut c = Cmd::new("docker");
    quiet(&mut c);
    c
}

// ── DB-level readiness checks ─────────────────────────────────────────────────
// TCP-open ≠ database-ready. MySQL/Postgres accept TCP connections seconds before
// they finish initialization. We retry actual SQL pings to confirm readiness.

async fn wait_mysql_ready(host_port: u16, password: &str) -> bool {
    let url = format!(
        "mysql://root:{password}@127.0.0.1:{host_port}/mysql?ssl-mode=disabled"
    );
    for _ in 0..60u32 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if let Ok(pool) = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(2))
            .connect(&url)
            .await
        {
            let ok = sqlx::query("SELECT 1").execute(&pool).await.is_ok();
            pool.close().await;
            if ok { return true; }
        }
    }
    false
}

async fn wait_postgres_ready(host_port: u16, password: &str) -> bool {
    let url = format!(
        "postgres://postgres:{password}@127.0.0.1:{host_port}/postgres?sslmode=disable"
    );
    for _ in 0..60u32 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if let Ok(pool) = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(2))
            .connect(&url)
            .await
        {
            let ok = sqlx::query("SELECT 1").execute(&pool).await.is_ok();
            pool.close().await;
            if ok { return true; }
        }
    }
    false
}

#[derive(serde::Serialize, Clone)]
pub struct DockerLog {
    pub line: String,
    pub kind: String,
}

#[derive(serde::Serialize, Clone)]
pub struct DockerConnInfo {
    pub db_type: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
    pub name: String,
}

fn emit_log(app: &tauri::AppHandle, event: &str, line: &str, kind: &str) {
    let _ = app.emit(
        event,
        DockerLog {
            line: line.to_owned(),
            kind: kind.to_owned(),
        },
    );
}

#[tauri::command]
pub async fn docker_check() -> Result<String, String> {
    let out = docker()
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .await
        .map_err(|_| {
            "Docker is not installed on this machine. Install Docker Desktop to use this feature."
                .to_string()
        })?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).to_lowercase();
        if err.contains("daemon")
            || err.contains("cannot connect")
            || err.contains("socket")
            || err.contains("pipe")
        {
            return Err(
                "Docker is installed but not running. Please start Docker Desktop.".to_string(),
            );
        }
        return Err(
            "Docker is not available. Please install Docker Desktop.".to_string(),
        );
    }

    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(if v.is_empty() { "ok".to_string() } else { v })
}

#[tauri::command]
pub async fn docker_run_db(
    app: tauri::AppHandle,
    db_type: String,
    event_id: String,
) -> Result<DockerConnInfo, String> {
    let evt = format!("docker-log:{event_id}");

    let (image, host_port, cont_port): (&str, u16, u16) = match db_type.as_str() {
        "postgres" => ("postgres:17-alpine", 5433, 5432),
        "mysql" => ("mysql:8.4", 3307, 3306),
        other => return Err(format!("Unsupported database type: {other}")),
    };

    let (user, password, database): (&str, &str, &str) = match db_type.as_str() {
        "postgres" => ("postgres", "postgres", "postgres"),
        "mysql" => ("root", "mysql", "mysql"),
        _ => unreachable!(),
    };

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let container_name = format!("stroke-{db_type}-{ts}");
    let port_map = format!("{host_port}:{cont_port}");

    // ── Pull ──────────────────────────────────────────────────────────────────
    emit_log(&app, &evt, &format!("Pulling {image}…"), "info");

    let mut pull_child = docker()
        .args(["pull", image])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start docker pull: {e}"))?;

    if let Some(stderr) = pull_child.stderr.take() {
        let mut lines = tokio::io::BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let t = line.trim();
            if !t.is_empty() {
                emit_log(&app, &evt, t, "info");
            }
        }
    }

    let pull_status = pull_child.wait().await.map_err(|e| e.to_string())?;
    if !pull_status.success() {
        return Err(format!(
            "docker pull {image} failed. Check your internet connection."
        ));
    }

    emit_log(&app, &evt, "Image ready. Starting container…", "info");

    // ── Run ───────────────────────────────────────────────────────────────────
    let mut run_cmd = docker();
    run_cmd
        .arg("run")
        .arg("-d")
        .arg("--name")
        .arg(&container_name)
        .arg("-p")
        .arg(&port_map);

    match db_type.as_str() {
        "postgres" => {
            run_cmd
                .arg("-e")
                .arg("POSTGRES_PASSWORD=postgres")
                .arg("-e")
                .arg("POSTGRES_USER=postgres")
                .arg("-e")
                .arg("POSTGRES_DB=postgres");
        }
        "mysql" => {
            run_cmd
                .arg("-e")
                .arg("MYSQL_ROOT_PASSWORD=mysql")
                .arg("-e")
                .arg("MYSQL_DATABASE=mysql")
                // MySQL 8.4 uses caching_sha2_password by default which requires SSL or
                // RSA key exchange. Enable mysql_native_password so sqlx can connect
                // without TLS on loopback connections.
                .arg("--mysql-native-password=ON");
        }
        _ => {}
    }
    run_cmd.arg(image);

    let run_out = run_cmd
        .output()
        .await
        .map_err(|e| format!("Failed to launch container: {e}"))?;

    if !run_out.status.success() {
        let err = String::from_utf8_lossy(&run_out.stderr);
        let msg = if err.contains("port is already allocated")
            || err.contains("address already in use")
        {
            format!("Port {host_port} is already in use. Stop the existing service and try again.")
        } else {
            format!("Container failed to start: {err}")
        };
        return Err(msg);
    }

    let cid = String::from_utf8_lossy(&run_out.stdout).trim().to_string();
    let short = &cid[..cid.len().min(12)];
    emit_log(&app, &evt, &format!("Container {short} started."), "info");
    emit_log(
        &app,
        &evt,
        "Waiting for database to accept connections…",
        "info",
    );

    let ready = match db_type.as_str() {
        "mysql"    => wait_mysql_ready(host_port, password).await,
        "postgres" => wait_postgres_ready(host_port, password).await,
        _          => false,
    };

    if !ready {
        return Err(format!(
            "Database did not become ready within 60s. Container: {container_name}"
        ));
    }

    emit_log(&app, &evt, "Database is ready.", "info");

    let label = if db_type == "postgres" {
        "PostgreSQL"
    } else {
        "MySQL"
    };

    Ok(DockerConnInfo {
        db_type,
        host: "127.0.0.1".to_string(),
        port: host_port,
        user: user.to_string(),
        password: password.to_string(),
        database: database.to_string(),
        name: format!("Docker {label} (:{host_port})"),
    })
}

// ── Databases already running in Docker ───────────────────────────────────────
// Someone with a `docker compose up` database has already written the
// credentials down once, in the compose file. Asking them to type the same
// user/password/port into a connection form is asking twice, so this reads the
// running containers and hands back ready-to-open connections.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DockerDatabase {
    /// Container name - stable across restarts, unlike the id.
    pub name: String,
    pub container_id: String,
    pub image: String,
    /// Stroke driver id (`postgres`, `mysql`, `redis`, …).
    pub engine: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
    /// Password-free label, e.g. `127.0.0.1:5439/sampledb`.
    pub target: String,
    /// Set when the container can't be opened from the host; says why.
    pub reason: Option<String>,
}

/// Container port each engine listens on, used both to recognise an image whose
/// name says nothing (`sha256:…`, a locally-built tag) and to pick the right
/// published port when a container publishes several.
fn engine_port(engine: &str) -> u16 {
    match engine {
        "mysql" | "mariadb" => 3306,
        "redis" => 6379,
        "clickhouse" => 8123,
        "mssql" => 1433,
        "cockroachdb" => 26257,
        _ => 5432,
    }
}

/// Stroke driver id for a Docker image reference, or None when it isn't a
/// database this app can open.
fn engine_for_image(image: &str) -> Option<&'static str> {
    let i = image.to_lowercase();
    // MariaDB before MySQL: its own tags mention mysql compatibility.
    if i.contains("mariadb") {
        return Some("mariadb");
    }
    if i.contains("mysql") || i.contains("percona") {
        return Some("mysql");
    }
    if i.contains("postgres") || i.contains("postgis") || i.contains("pgvector") || i.contains("timescale") {
        return Some("postgres");
    }
    if i.contains("cockroach") {
        return Some("cockroachdb");
    }
    if i.contains("clickhouse") {
        return Some("clickhouse");
    }
    if i.contains("valkey") || i.contains("redis") {
        return Some("redis");
    }
    if i.contains("mssql") || i.contains("sqlserver") || i.contains("azure-sql-edge") {
        return Some("mssql");
    }
    None
}

/// Falls back to what the container listens on when the image name is useless -
/// a locally-built tag, or an image id like `cc4c61127125`.
fn engine_for_ports(ports: &Value) -> Option<&'static str> {
    let map = ports.as_object()?;
    for key in map.keys() {
        let port: u16 = key.split('/').next()?.parse().ok()?;
        let engine = match port {
            5432 => "postgres",
            3306 => "mysql",
            6379 => "redis",
            8123 => "clickhouse",
            1433 => "mssql",
            26257 => "cockroachdb",
            _ => continue,
        };
        return Some(engine);
    }
    None
}

fn env_map(env: &Value) -> std::collections::HashMap<String, String> {
    env.as_array()
        .map(|list| {
            list.iter()
                .filter_map(|e| e.as_str())
                .filter_map(|e| e.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// The credentials the container was started with, straight out of its
/// environment - the same values the compose file already spells out.
fn credentials(engine: &str, env: &std::collections::HashMap<String, String>) -> (String, String, String) {
    let get = |keys: &[&str]| keys.iter().find_map(|k| env.get(*k)).cloned();
    match engine {
        "mysql" | "mariadb" => {
            let root = get(&["MYSQL_ROOT_PASSWORD", "MARIADB_ROOT_PASSWORD"]);
            let empty_ok = get(&["MYSQL_ALLOW_EMPTY_PASSWORD", "MARIADB_ALLOW_EMPTY_ROOT_PASSWORD"])
                .is_some_and(|v| matches!(v.to_lowercase().as_str(), "1" | "yes" | "true"));
            let db = get(&["MYSQL_DATABASE", "MARIADB_DATABASE"]).unwrap_or_else(|| "mysql".into());
            match (root, empty_ok) {
                (Some(p), _) => ("root".into(), p, db),
                (None, true) => ("root".into(), String::new(), db),
                // No root password: the image's own unprivileged user is the way in.
                (None, false) => (
                    get(&["MYSQL_USER", "MARIADB_USER"]).unwrap_or_else(|| "root".into()),
                    get(&["MYSQL_PASSWORD", "MARIADB_PASSWORD"]).unwrap_or_default(),
                    db,
                ),
            }
        }
        "clickhouse" => (
            get(&["CLICKHOUSE_USER"]).unwrap_or_else(|| "default".into()),
            get(&["CLICKHOUSE_PASSWORD"]).unwrap_or_default(),
            get(&["CLICKHOUSE_DB"]).unwrap_or_else(|| "default".into()),
        ),
        "mssql" => (
            "sa".into(),
            get(&["MSSQL_SA_PASSWORD", "SA_PASSWORD"]).unwrap_or_default(),
            "master".into(),
        ),
        "redis" => (
            String::new(),
            get(&["REDIS_PASSWORD", "REDIS_ARGS_PASSWORD"]).unwrap_or_default(),
            String::new(),
        ),
        "cockroachdb" => ("root".into(), String::new(), "defaultdb".into()),
        _ => {
            let user = get(&["POSTGRES_USER", "PGUSER"]).unwrap_or_else(|| "postgres".into());
            let db = get(&["POSTGRES_DB", "PGDATABASE"]).unwrap_or_else(|| user.clone());
            (user, get(&["POSTGRES_PASSWORD", "PGPASSWORD"]).unwrap_or_default(), db)
        }
    }
}

/// The host port this container's database is reachable on. Prefers the binding
/// for the engine's own port - a Postgres container that also publishes an
/// exporter on 9187 must not hand back the exporter.
fn published_port(ports: &Value, engine: &str) -> Option<u16> {
    let map = ports.as_object()?;
    let read = |v: &Value| -> Option<u16> {
        v.as_array()?
            .iter()
            .filter_map(|b| b.get("HostPort")?.as_str()?.parse::<u16>().ok())
            .find(|p| *p > 0)
    };
    let want = engine_port(engine);
    if let Some(p) = map.get(&format!("{want}/tcp")).and_then(read) {
        return Some(p);
    }
    map.iter()
        .filter(|(k, _)| k.ends_with("/tcp"))
        .filter_map(|(_, v)| read(v))
        .min()
}

/// One inspected container as a connectable database, or None when it isn't one.
fn database_from_inspect(c: &Value) -> Option<DockerDatabase> {
    let config = c.get("Config")?;
    let image = config.get("Image").and_then(Value::as_str).unwrap_or_default().to_string();
    let ports = c
        .get("NetworkSettings")
        .and_then(|n| n.get("Ports"))
        .cloned()
        .unwrap_or(Value::Null);

    let engine = engine_for_image(&image)
        .or_else(|| engine_for_ports(&ports))?
        .to_string();

    let env = env_map(config.get("Env").unwrap_or(&Value::Null));
    let (user, password, database) = credentials(&engine, &env);
    let name = c
        .get("Name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim_start_matches('/')
        .to_string();
    let container_id: String = c
        .get("Id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .take(12)
        .collect();

    let port = published_port(&ports, &engine);
    let reason = port.is_none().then(|| {
        format!(
            "Not reachable from the host - this container publishes no port (run it with -p {}:{}).",
            engine_port(&engine),
            engine_port(&engine)
        )
    });

    let host = "127.0.0.1".to_string();
    let port = port.unwrap_or_else(|| engine_port(&engine));
    let target = if database.is_empty() {
        format!("{host}:{port}")
    } else {
        format!("{host}:{port}/{database}")
    };

    Some(DockerDatabase {
        name: if name.is_empty() { container_id.clone() } else { name },
        container_id,
        image,
        engine,
        host,
        port,
        user,
        password,
        database,
        target,
        reason,
    })
}

/// Start, stop or restart one container, from the right-click menu on its card.
///
/// Only these three: they are reversible, and anything that deletes a container
/// or its volume belongs in Docker's own tools. The name or id is validated
/// against Docker's own character set so it can never be read as a flag
/// (`--rm`, `-f`) by the CLI.
#[tauri::command]
pub async fn docker_container_action(container: String, action: String) -> Result<(), String> {
    if !matches!(action.as_str(), "start" | "stop" | "restart") {
        return Err(format!("Unsupported Docker action: {action}"));
    }
    let valid = !container.is_empty()
        && !container.starts_with('-')
        && container.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    if !valid {
        return Err("That doesn't look like a container name or id.".into());
    }
    let out = docker()
        .arg(&action)
        .arg(&container)
        .output()
        .await
        .map_err(|e| format!("Could not run docker: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if msg.is_empty() { format!("docker {action} failed") } else { msg })
    }
}

/// Every database container running on this machine, with the credentials it was
/// started with. Docker missing or not running is not an error - it just means
/// there is nothing to offer, and the connection screen stays quiet about it.
#[tauri::command]
pub async fn scan_docker_databases() -> Result<Vec<DockerDatabase>, String> {
    let Ok(ps) = docker().args(["ps", "--format", "{{.ID}}"]).output().await else {
        return Ok(Vec::new());
    };
    if !ps.status.success() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = String::from_utf8_lossy(&ps.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    // One inspect for every container, line-delimited so a single malformed
    // entry can't take the whole scan down with it.
    let Ok(out) = docker()
        .arg("inspect")
        .arg("--format")
        .arg("{{json .}}")
        .args(&ids)
        .output()
        .await
    else {
        return Ok(Vec::new());
    };

    let mut found: Vec<DockerDatabase> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|c| database_from_inspect(&c))
        .collect();
    // Connectable first, then by name, so the list doesn't reshuffle every scan.
    found.sort_by(|a, b| a.reason.is_some().cmp(&b.reason.is_some()).then(a.name.cmp(&b.name)));
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn recognises_database_images_and_ignores_the_rest() {
        assert_eq!(engine_for_image("postgres:17-alpine"), Some("postgres"));
        assert_eq!(engine_for_image("pgvector/pgvector:pg16"), Some("postgres"));
        assert_eq!(engine_for_image("timescale/timescaledb:latest-pg16"), Some("postgres"));
        assert_eq!(engine_for_image("mariadb:11"), Some("mariadb"));
        assert_eq!(engine_for_image("mysql:8.4"), Some("mysql"));
        assert_eq!(engine_for_image("redis:7.2-alpine"), Some("redis"));
        assert_eq!(engine_for_image("clickhouse/clickhouse-server"), Some("clickhouse"));
        assert_eq!(engine_for_image("mcr.microsoft.com/mssql/server:2022-latest"), Some("mssql"));
        assert_eq!(engine_for_image("caddy:2-alpine"), None);
        assert_eq!(engine_for_image("adminer"), None);
    }

    #[test]
    fn falls_back_to_the_listening_port_for_an_unnamed_image() {
        // A locally-built tag or a bare image id says nothing about the engine.
        let ports = json!({ "5432/tcp": [{ "HostIp": "0.0.0.0", "HostPort": "5434" }] });
        assert_eq!(engine_for_image("cc4c61127125"), None);
        assert_eq!(engine_for_ports(&ports), Some("postgres"));
    }

    #[test]
    fn picks_the_engines_own_published_port() {
        let ports = json!({
            "9187/tcp": [{ "HostPort": "9187" }],
            "5432/tcp": [{ "HostIp": "0.0.0.0", "HostPort": "5440" }, { "HostIp": "::", "HostPort": "5440" }],
        });
        assert_eq!(published_port(&ports, "postgres"), Some(5440));
    }

    #[test]
    fn reads_credentials_out_of_the_container_environment() {
        let pg = env_map(&json!(["POSTGRES_USER=prisma", "POSTGRES_PASSWORD=secret", "POSTGRES_DB=sampledb"]));
        assert_eq!(credentials("postgres", &pg), ("prisma".into(), "secret".into(), "sampledb".into()));
        // Postgres defaults the database to the user when POSTGRES_DB is unset.
        let bare = env_map(&json!(["POSTGRES_PASSWORD=x"]));
        assert_eq!(credentials("postgres", &bare), ("postgres".into(), "x".into(), "postgres".into()));

        let my = env_map(&json!(["MYSQL_ROOT_PASSWORD=r00t", "MYSQL_DATABASE=shop"]));
        assert_eq!(credentials("mysql", &my), ("root".into(), "r00t".into(), "shop".into()));
        // No root password: the image's unprivileged user is the way in.
        let scoped = env_map(&json!(["MYSQL_USER=app", "MYSQL_PASSWORD=app-pw", "MYSQL_DATABASE=shop"]));
        assert_eq!(credentials("mysql", &scoped), ("app".into(), "app-pw".into(), "shop".into()));
    }

    #[test]
    fn a_container_with_no_published_port_says_why() {
        let c = json!({
            "Id": "abc123def4567890",
            "Name": "/vms_backend-postgres-1",
            "Config": { "Image": "postgres:latest", "Env": ["POSTGRES_PASSWORD=x"] },
            "NetworkSettings": { "Ports": {} },
        });
        let db = database_from_inspect(&c).unwrap();
        assert_eq!(db.name, "vms_backend-postgres-1");
        assert_eq!(db.engine, "postgres");
        assert!(db.reason.as_deref().unwrap().contains("publishes no port"));
    }

    #[test]
    fn builds_a_connectable_row_from_a_real_container() {
        let c = json!({
            "Id": "0123456789abcdef",
            "Name": "/prisma-studio-db",
            "Config": {
                "Image": "postgres:17-alpine",
                "Env": ["POSTGRES_USER=prisma", "POSTGRES_PASSWORD=prisma", "POSTGRES_DB=sampledb"],
            },
            "NetworkSettings": { "Ports": { "5432/tcp": [{ "HostIp": "0.0.0.0", "HostPort": "5439" }] } },
        });
        let db = database_from_inspect(&c).unwrap();
        assert_eq!((db.engine.as_str(), db.port, db.user.as_str(), db.database.as_str()), ("postgres", 5439, "prisma", "sampledb"));
        assert_eq!(db.target, "127.0.0.1:5439/sampledb");
        assert!(db.reason.is_none());
    }

    #[test]
    fn skips_containers_that_are_not_databases() {
        let caddy = json!({
            "Id": "f00",
            "Name": "/timeline-caddy-1",
            "Config": { "Image": "caddy:2-alpine", "Env": [] },
            "NetworkSettings": { "Ports": { "80/tcp": [{ "HostPort": "80" }], "443/tcp": [{ "HostPort": "443" }] } },
        });
        assert!(database_from_inspect(&caddy).is_none());
    }
}

#[cfg(test)]
mod action_tests {
    use super::docker_container_action;

    /// Nothing that could be read as a docker flag, and nothing destructive.
    #[tokio::test]
    async fn rejects_flags_and_unknown_actions_before_running_docker() {
        for bad in ["--rm", "-f", "a b", "x;rm -rf /", ""] {
            assert!(docker_container_action(bad.into(), "stop".into()).await.is_err(), "{bad:?}");
        }
        assert!(docker_container_action("stroke-test-mysql".into(), "rm".into()).await.unwrap_err().contains("Unsupported"));
    }

    /// Against the dialect-matrix container: `cargo test --lib docker_live -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn docker_live_restart_the_test_container() {
        docker_container_action("stroke-test-mysql".into(), "restart".into()).await.unwrap();
    }
}
