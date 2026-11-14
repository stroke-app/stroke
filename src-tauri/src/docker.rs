use crate::proc::quiet;
use std::process::Stdio;
use tauri::Emitter;
use tokio::io::AsyncBufReadExt;
use tokio::process::Command as Cmd;
use serde_json::Value;

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

#[derive(serde::Serialize, Clone)]
pub struct DockerLog {
    pub line: String,
    pub kind: String,
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

// ── The launch catalog ────────────────────────────────────────────────────────
// Every engine Stroke can open, as a container it can start with one click: the
// image, how to start it, the credentials it starts with, and how to tell it is
// ready. The palette lists these (docker_templates) and docker_run_db launches
// one, returning a connection the app opens as it is.

/// How to tell a fresh container is ready for a client.
#[derive(Clone, Copy)]
enum Ready {
    /// `SELECT 1` over the Postgres protocol (Postgres, CockroachDB).
    Postgres,
    /// `SELECT 1` over the MySQL protocol (MySQL, MariaDB).
    Mysql,
    /// This line in the container's log, at least this many times (MongoDB
    /// prints it once for its setup run and again for the real server).
    Log(&'static str, usize),
    /// Any HTTP answer at this path. Not a bare TCP connect: Docker's port
    /// proxy accepts those before anything listens inside the container.
    Http(&'static str),
}

struct Template {
    id: &'static str,
    label: &'static str,
    /// Stroke engine id: the connection type, and the palette's icon.
    engine: &'static str,
    /// Icon when it isn't the engine's own (pgvector shows Postgres anyway).
    icon: &'static str,
    group: &'static str,
    image: &'static str,
    note: &'static str,
    /// Port inside the container.
    port: u16,
    /// Preferred host port; the next free one is used when it is taken.
    host_port: u16,
    env: &'static [&'static str],
    /// Arguments after the image (the container's command).
    cmd: &'static [&'static str],
    user: &'static str,
    password: &'static str,
    database: &'static str,
    ready: Ready,
    /// Seconds to wait for `ready`; Oracle and SQL Server start slowly.
    wait_secs: u64,
}

const PG_ENV: &[&str] = &["POSTGRES_PASSWORD=postgres", "POSTGRES_USER=postgres", "POSTGRES_DB=postgres"];

const TEMPLATES: &[Template] = &[
    Template { id: "postgres", label: "PostgreSQL 17", engine: "postgres", icon: "postgres", group: "Relational", image: "postgres:17-alpine", note: "", port: 5432, host_port: 5433, env: PG_ENV, cmd: &[], user: "postgres", password: "postgres", database: "postgres", ready: Ready::Postgres, wait_secs: 90 },
    Template { id: "postgres16", label: "PostgreSQL 16", engine: "postgres", icon: "postgres", group: "Relational", image: "postgres:16-alpine", note: "", port: 5432, host_port: 5434, env: PG_ENV, cmd: &[], user: "postgres", password: "postgres", database: "postgres", ready: Ready::Postgres, wait_secs: 90 },
    Template { id: "pgvector", label: "PostgreSQL + pgvector", engine: "postgres", icon: "postgres", group: "Relational", image: "pgvector/pgvector:pg17", note: "Vector search", port: 5432, host_port: 5435, env: PG_ENV, cmd: &[], user: "postgres", password: "postgres", database: "postgres", ready: Ready::Postgres, wait_secs: 90 },
    Template { id: "postgis", label: "PostGIS", engine: "postgres", icon: "postgres", group: "Relational", image: "postgis/postgis:17-3.5", note: "Geospatial", port: 5432, host_port: 5436, env: PG_ENV, cmd: &[], user: "postgres", password: "postgres", database: "postgres", ready: Ready::Postgres, wait_secs: 120 },
    Template { id: "timescale", label: "TimescaleDB", engine: "postgres", icon: "timescale", group: "Relational", image: "timescale/timescaledb:latest-pg17", note: "Time series", port: 5432, host_port: 5437, env: PG_ENV, cmd: &[], user: "postgres", password: "postgres", database: "postgres", ready: Ready::Postgres, wait_secs: 120 },
    // MySQL 8.4 defaults to caching_sha2_password, which needs TLS or an RSA
    // exchange; native passwords let the driver connect over loopback.
    Template { id: "mysql", label: "MySQL 8.4", engine: "mysql", icon: "mysql", group: "Relational", image: "mysql:8.4", note: "", port: 3306, host_port: 3307, env: &["MYSQL_ROOT_PASSWORD=mysql", "MYSQL_DATABASE=app"], cmd: &["--mysql-native-password=ON"], user: "root", password: "mysql", database: "app", ready: Ready::Mysql, wait_secs: 120 },
    Template { id: "mariadb", label: "MariaDB 11", engine: "mariadb", icon: "mariadb", group: "Relational", image: "mariadb:11", note: "", port: 3306, host_port: 3308, env: &["MARIADB_ROOT_PASSWORD=mariadb", "MARIADB_DATABASE=app"], cmd: &[], user: "root", password: "mariadb", database: "app", ready: Ready::Mysql, wait_secs: 120 },
    Template { id: "mssql", label: "SQL Server 2022", engine: "mssql", icon: "mssql", group: "Relational", image: "mcr.microsoft.com/mssql/server:2022-latest", note: "x86-64 only", port: 1433, host_port: 1434, env: &["ACCEPT_EULA=Y", "MSSQL_SA_PASSWORD=Stroke_Passw0rd!"], cmd: &[], user: "sa", password: "Stroke_Passw0rd!", database: "master", ready: Ready::Log("SQL Server is now ready for client connections", 1), wait_secs: 180 },
    Template { id: "oracle", label: "Oracle Database Free", engine: "oracle", icon: "oracle", group: "Relational", image: "gvenzl/oracle-free:slim", note: "Needs Instant Client", port: 1521, host_port: 1521, env: &["ORACLE_PASSWORD=oracle", "APP_USER=app", "APP_USER_PASSWORD=app"], cmd: &[], user: "app", password: "app", database: "FREEPDB1", ready: Ready::Log("DATABASE IS READY TO USE!", 1), wait_secs: 300 },
    Template { id: "cockroachdb", label: "CockroachDB", engine: "cockroachdb", icon: "cockroachdb", group: "Relational", image: "cockroachdb/cockroach:latest", note: "Single node", port: 26257, host_port: 26257, env: &[], cmd: &["start-single-node", "--insecure"], user: "root", password: "", database: "defaultdb", ready: Ready::Postgres, wait_secs: 120 },
    Template { id: "mongodb", label: "MongoDB 8", engine: "mongodb", icon: "mongodb", group: "Document and key-value", image: "mongo:8", note: "", port: 27017, host_port: 27018, env: &["MONGO_INITDB_ROOT_USERNAME=mongo", "MONGO_INITDB_ROOT_PASSWORD=mongo"], cmd: &[], user: "mongo", password: "mongo", database: "app", ready: Ready::Log("Waiting for connections", 2), wait_secs: 90 },
    Template { id: "redis", label: "Redis 7", engine: "redis", icon: "redis", group: "Document and key-value", image: "redis:7-alpine", note: "", port: 6379, host_port: 6380, env: &[], cmd: &[], user: "", password: "", database: "", ready: Ready::Log("Ready to accept connections", 1), wait_secs: 60 },
    Template { id: "valkey", label: "Valkey 8", engine: "valkey", icon: "valkey", group: "Document and key-value", image: "valkey/valkey:8-alpine", note: "", port: 6379, host_port: 6381, env: &[], cmd: &[], user: "", password: "", database: "", ready: Ready::Log("Ready to accept connections", 1), wait_secs: 60 },
    Template { id: "couchdb", label: "CouchDB 3", engine: "couchdb", icon: "couchdb", group: "Document and key-value", image: "couchdb:3", note: "", port: 5984, host_port: 5985, env: &["COUCHDB_USER=admin", "COUCHDB_PASSWORD=couchdb"], cmd: &[], user: "admin", password: "couchdb", database: "", ready: Ready::Log("Apache CouchDB has started", 1), wait_secs: 90 },
    Template { id: "dynamodb", label: "DynamoDB Local", engine: "dynamodb", icon: "dynamodb", group: "Document and key-value", image: "amazon/dynamodb-local:latest", note: "", port: 8000, host_port: 8001, env: &[], cmd: &[], user: "", password: "", database: "", ready: Ready::Http("/"), wait_secs: 60 },
    Template { id: "cassandra", label: "Cassandra 5", engine: "cassandra", icon: "cassandra", group: "Document and key-value", image: "cassandra:5", note: "Starts in about a minute", port: 9042, host_port: 9043, env: &[], cmd: &[], user: "", password: "", database: "", ready: Ready::Log("Starting listening for CQL clients", 1), wait_secs: 240 },
    Template { id: "clickhouse", label: "ClickHouse", engine: "clickhouse", icon: "clickhouse", group: "Analytics", image: "clickhouse/clickhouse-server:latest", note: "", port: 8123, host_port: 8124, env: &["CLICKHOUSE_USER=default", "CLICKHOUSE_PASSWORD=clickhouse", "CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT=1"], cmd: &[], user: "default", password: "clickhouse", database: "default", ready: Ready::Http("/ping"), wait_secs: 90 },
    Template { id: "libsql", label: "libSQL server", engine: "libsql", icon: "turso", group: "Analytics", image: "ghcr.io/tursodatabase/libsql-server:latest", note: "Turso, self-hosted", port: 8080, host_port: 8081, env: &[], cmd: &[], user: "", password: "", database: "", ready: Ready::Http("/health"), wait_secs: 60 },
];

/// What the palette lists for one launchable image.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerTemplate {
    pub id: &'static str,
    pub label: &'static str,
    pub engine: &'static str,
    pub icon: &'static str,
    pub group: &'static str,
    pub image: &'static str,
    pub note: &'static str,
    pub port: u16,
}

#[tauri::command]
pub fn docker_templates() -> Vec<DockerTemplate> {
    TEMPLATES
        .iter()
        .map(|t| DockerTemplate { id: t.id, label: t.label, engine: t.engine, icon: t.icon, group: t.group, image: t.image, note: t.note, port: t.host_port })
        .collect()
}

/// The preferred port if nothing listens there, else the next free one.
fn free_port(preferred: u16) -> u16 {
    let free = |p: u16| std::net::TcpListener::bind(("127.0.0.1", p)).is_ok();
    (preferred..preferred.saturating_add(100)).find(|p| free(*p)).unwrap_or_else(|| {
        std::net::TcpListener::bind(("127.0.0.1", 0)).and_then(|l| l.local_addr()).map(|a| a.port()).unwrap_or(preferred)
    })
}

/// The saved-connection object the app opens for a launched template.
fn connection_for(t: &Template, port: u16) -> Value {
    let host = "127.0.0.1";
    let name = format!("Docker {} (:{port})", t.label);
    match t.engine {
        "mongodb" => serde_json::json!({ "type": "mongodb", "name": name, "uri": format!("mongodb://{}:{}@{host}:{port}/?authSource=admin", t.user, t.password), "database": t.database }),
        "redis" | "valkey" => serde_json::json!({ "type": t.engine, "name": name, "host": host, "port": port, "db": 0 }),
        "couchdb" => serde_json::json!({ "type": "couchdb", "name": name, "url": format!("http://{host}:{port}"), "user": t.user, "password": t.password }),
        "dynamodb" => serde_json::json!({ "type": "dynamodb", "name": name, "region": "us-east-1", "endpoint": format!("http://{host}:{port}"), "accessKeyId": "local", "secretAccessKey": "local" }),
        "cassandra" => serde_json::json!({ "type": "cassandra", "name": name, "hosts": format!("{host}:{port}") }),
        "libsql" => serde_json::json!({ "type": "libsql", "name": name, "url": format!("http://{host}:{port}") }),
        "oracle" => serde_json::json!({ "type": "oracle", "name": name, "host": host, "port": port, "serviceName": t.database, "user": t.user, "password": t.password }),
        "mssql" => serde_json::json!({ "type": "mssql", "name": name, "host": host, "port": port, "user": t.user, "password": t.password, "database": t.database, "trustCert": true }),
        _ => serde_json::json!({ "type": t.engine, "name": name, "host": host, "port": port, "user": t.user, "password": t.password, "database": t.database, "ssl": false }),
    }
}

async fn wait_ready(t: &Template, container: &str, port: u16) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(t.wait_secs);
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let ok = match t.ready {
            Ready::Postgres => {
                let url = format!("postgres://{}:{}@127.0.0.1:{port}/{}?sslmode=disable", t.user, t.password, t.database);
                match sqlx::postgres::PgPoolOptions::new().max_connections(1).acquire_timeout(std::time::Duration::from_secs(2)).connect(&url).await {
                    Ok(pool) => {
                        let ok = sqlx::query("SELECT 1").execute(&pool).await.is_ok();
                        pool.close().await;
                        ok
                    }
                    Err(_) => false,
                }
            }
            Ready::Mysql => {
                let url = format!("mysql://{}:{}@127.0.0.1:{port}/{}?ssl-mode=disabled", t.user, t.password, t.database);
                match sqlx::mysql::MySqlPoolOptions::new().max_connections(1).acquire_timeout(std::time::Duration::from_secs(2)).connect(&url).await {
                    Ok(pool) => {
                        let ok = sqlx::query("SELECT 1").execute(&pool).await.is_ok();
                        pool.close().await;
                        ok
                    }
                    Err(_) => false,
                }
            }
            Ready::Http(path) => {
                let url = format!("http://127.0.0.1:{port}{path}");
                let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(2)).build();
                match client {
                    Ok(c) => c.get(&url).send().await.is_ok(),
                    Err(_) => false,
                }
            }
            Ready::Log(line, times) => {
                let logs = docker().args(["logs", container]).output().await;
                logs.is_ok_and(|o| {
                    let all = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
                    all.matches(line).count() >= times
                })
            }
        };
        if ok {
            return true;
        }
    }
    false
}

/// Pull and run one catalog image, wait until it takes connections, and
/// return the connection to open. `db_type` is a template id (`postgres`,
/// `mongodb`, ...); logs stream as `docker-log:{event_id}`.
#[tauri::command]
pub async fn docker_run_db(app: tauri::AppHandle, db_type: String, event_id: String) -> Result<Value, String> {
    let evt = format!("docker-log:{event_id}");
    let t = TEMPLATES
        .iter()
        .find(|t| t.id == db_type)
        .ok_or_else(|| format!("No Docker image for {db_type}"))?;
    let host_port = free_port(t.host_port);
    if host_port != t.host_port {
        emit_log(&app, &evt, &format!("Port {} is taken, using {host_port}.", t.host_port), "info");
    }
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let container_name = format!("stroke-{}-{ts}", t.id);

    // ── Pull ──────────────────────────────────────────────────────────────────
    emit_log(&app, &evt, &format!("Pulling {}…", t.image), "info");
    let mut pull_child = docker()
        .args(["pull", t.image])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start docker pull: {e}"))?;
    if let Some(stderr) = pull_child.stderr.take() {
        let mut lines = tokio::io::BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim();
            if !line.is_empty() {
                emit_log(&app, &evt, line, "info");
            }
        }
    }
    let pull_status = pull_child.wait().await.map_err(|e| e.to_string())?;
    if !pull_status.success() {
        return Err(format!("docker pull {} failed. Check your internet connection, or whether the image runs on this machine ({}).", t.image, std::env::consts::ARCH));
    }
    emit_log(&app, &evt, "Image ready. Starting container…", "info");

    // ── Run ───────────────────────────────────────────────────────────────────
    let mut run_cmd = docker();
    run_cmd.args(["run", "-d", "--name", &container_name, "-p", &format!("{host_port}:{}", t.port)]);
    for e in t.env {
        run_cmd.args(["-e", e]);
    }
    run_cmd.arg(t.image).args(t.cmd);
    let run_out = run_cmd.output().await.map_err(|e| format!("Failed to launch container: {e}"))?;
    if !run_out.status.success() {
        let err = String::from_utf8_lossy(&run_out.stderr);
        return Err(if err.contains("port is already allocated") || err.contains("address already in use") {
            format!("Port {host_port} is already in use. Stop the existing service and try again.")
        } else {
            format!("Container failed to start: {err}")
        });
    }
    let cid = String::from_utf8_lossy(&run_out.stdout).trim().to_string();
    emit_log(&app, &evt, &format!("Container {} started on port {host_port}.", &cid[..cid.len().min(12)]), "info");
    emit_log(&app, &evt, "Waiting for the database to accept connections…", "info");

    if !wait_ready(t, &container_name, host_port).await {
        return Err(format!("{} did not become ready within {}s. Container: {container_name} (docker logs {container_name})", t.label, t.wait_secs));
    }
    emit_log(&app, &evt, "Database is ready.", "info");
    Ok(connection_for(t, host_port))
}

// ── Databases already running in Docker ───────────────────────────────────────
// Someone with a `docker compose up` database has already written the
// credentials down once, in the compose file. Asking them to type the same
// user/password/port into a connection form is asking twice, so this reads the
// running containers and hands back ready-to-open connections.

use serde::Serialize;

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
        "oracle" => 1521,
        "mongodb" => 27017,
        "couchdb" => 5984,
        "cassandra" => 9042,
        "dynamodb" => 8000,
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
    // gvenzl/oracle-free, gvenzl/oracle-xe, container-registry.oracle.com/database/free.
    if i.contains("oracle-free") || i.contains("oracle-xe") || i.contains("oracle/database") || i.contains("oracle.com/database") {
        return Some("oracle");
    }
    // Admin UIs share the name but are not databases.
    if i.contains("mongo") && !i.contains("mongo-express") {
        return Some("mongodb");
    }
    if i.contains("couchdb") {
        return Some("couchdb");
    }
    if i.contains("cassandra") || i.contains("scylla") {
        return Some("cassandra");
    }
    if i.contains("dynamodb-local") {
        return Some("dynamodb");
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
            1521 => "oracle",
            27017 => "mongodb",
            5984 => "couchdb",
            9042 => "cassandra",
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
        // gvenzl images: SYSTEM with ORACLE_PASSWORD; the official image uses ORACLE_PWD.
        // The database is the pluggable one the image opens (set in database_from_inspect).
        "oracle" => ("system".into(), get(&["ORACLE_PASSWORD", "ORACLE_PWD"]).unwrap_or_default(), String::new()),
        "mongodb" => (
            get(&["MONGO_INITDB_ROOT_USERNAME"]).unwrap_or_default(),
            get(&["MONGO_INITDB_ROOT_PASSWORD"]).unwrap_or_default(),
            get(&["MONGO_INITDB_DATABASE"]).unwrap_or_default(),
        ),
        "couchdb" => (
            get(&["COUCHDB_USER"]).unwrap_or_default(),
            get(&["COUCHDB_PASSWORD"]).unwrap_or_default(),
            String::new(),
        ),
        "cassandra" | "dynamodb" => (String::new(), String::new(), String::new()),
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
    let (user, password, mut database) = credentials(&engine, &env);
    if engine == "oracle" && database.is_empty() {
        // Free (23ai) opens FREEPDB1, XE opens XEPDB1; APP_USER-style images may add one.
        database = env
            .get("ORACLE_DATABASE")
            .cloned()
            .unwrap_or_else(|| if image.to_lowercase().contains("xe") { "XEPDB1".into() } else { "FREEPDB1".into() });
    }
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
        assert_eq!(engine_for_image("gvenzl/oracle-free:slim"), Some("oracle"));
        assert_eq!(engine_for_image("container-registry.oracle.com/database/free:latest"), Some("oracle"));
        assert_eq!(engine_for_image("mongo:7"), Some("mongodb"));
        assert_eq!(engine_for_image("mongo-express"), None);
        assert_eq!(engine_for_image("couchdb:3"), Some("couchdb"));
        assert_eq!(engine_for_image("scylladb/scylla"), Some("cassandra"));
        assert_eq!(engine_for_image("amazon/dynamodb-local"), Some("dynamodb"));
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

#[cfg(test)]
mod catalog_tests {
    use super::*;

    #[test]
    fn template_ids_are_unique_and_connections_name_their_engine() {
        let mut ids: Vec<&str> = TEMPLATES.iter().map(|t| t.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), TEMPLATES.len());
        for t in TEMPLATES {
            let c = connection_for(t, 4000);
            assert_eq!(c["type"], t.engine, "{}", t.id);
            assert!(c["name"].as_str().unwrap().contains(":4000"), "{}", t.id);
        }
    }

    #[test]
    fn free_port_skips_a_taken_one() {
        let taken = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = taken.local_addr().unwrap().port();
        assert_ne!(free_port(port), port);
    }
}
