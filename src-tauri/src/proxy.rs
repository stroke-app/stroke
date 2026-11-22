//! Proxy settings by kind of traffic (Settings > Network).
//!
//! Four kinds, each System (the default: whatever the environment says,
//! `HTTPS_PROXY` and friends), Custom (a proxy URL) or Off (always direct):
//!
//! - **App**: update checks, sign-in, sync, sharing, web search, provider APIs.
//! - **AI**: requests to AI providers.
//! - **Database**: connections to databases. HTTP engines (ClickHouse,
//!   Elasticsearch, D1, ...) get the proxy on their client; TCP engines
//!   (Postgres, MySQL, SQL Server, ...) dial a local forwarder that opens the
//!   connection through it, the same way an SSH tunnel's local end is used.
//! - **SSH**: the connection to the SSH bastion, through the same forwarder;
//!   kubectl and the AWS CLI get it as `HTTPS_PROXY`.
//!
//! A custom URL is `http://host:port` (HTTP CONNECT) or `socks5://host:port`,
//! either with `user:password@`. Database and SSH traffic is not HTTP, so
//! "System" for them means direct unless a proxy is set here.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    App,
    Ai,
    Database,
    Ssh,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    /// "system" (default), "custom" or "off".
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProxySettings {
    #[serde(default)]
    pub app: Rule,
    #[serde(default)]
    pub ai: Rule,
    #[serde(default)]
    pub database: Rule,
    #[serde(default)]
    pub ssh: Rule,
}

static SETTINGS: RwLock<Option<ProxySettings>> = RwLock::new(None);
/// Bumped on every change, so cached clients know to rebuild.
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

fn rule(kind: Kind) -> Rule {
    let guard = SETTINGS.read().unwrap_or_else(|e| e.into_inner());
    let Some(s) = guard.as_ref() else { return Rule::default() };
    match kind {
        Kind::App => s.app.clone(),
        Kind::Ai => s.ai.clone(),
        Kind::Database => s.database.clone(),
        Kind::Ssh => s.ssh.clone(),
    }
}

/// The proxy URL a kind should go through, when it is set to Custom.
pub fn custom_url(kind: Kind) -> Option<String> {
    let r = rule(kind);
    (r.mode == "custom" && !r.url.trim().is_empty()).then(|| r.url.trim().to_string())
}

/// True when this kind is set to Off (always direct).
pub fn is_off(kind: Kind) -> bool {
    rule(kind).mode == "off"
}

/// What a URL is, for the settings to refuse a bad one before it is used.
pub fn validate(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url.trim()).map_err(|_| format!("{url} is not a proxy URL. Use http://host:port or socks5://host:port."))?;
    match parsed.scheme() {
        "http" | "socks5" | "socks5h" => {}
        "https" => return Err("HTTPS proxies are not supported: use http://host:port (the connection through it is still encrypted).".into()),
        s => return Err(format!("{s}:// is not a proxy kind Stroke speaks. Use http:// or socks5://.")),
    }
    if parsed.host_str().is_none() || parsed.port_or_known_default().is_none() {
        return Err("The proxy URL needs a host and a port.".into());
    }
    Ok(())
}

/// Replace the settings (from the window, on launch and on every change).
#[tauri::command]
pub fn proxy_set(settings: ProxySettings) -> Result<(), String> {
    for (name, r) in [("App", &settings.app), ("AI", &settings.ai), ("Database", &settings.database), ("SSH", &settings.ssh)] {
        if r.mode == "custom" {
            validate(&r.url).map_err(|e| format!("{name} proxy: {e}"))?;
        }
    }
    let mut guard = SETTINGS.write().unwrap_or_else(|e| e.into_inner());
    if guard.as_ref() != Some(&settings) {
        *guard = Some(settings);
        GENERATION.fetch_add(1, Ordering::Relaxed);
    }
    Ok(())
}

/// Reach `target` through `proxy`, then report whether it answered. For the
/// settings' Test button.
#[tauri::command]
pub async fn proxy_test(url: String, target: Option<String>) -> Result<u64, String> {
    validate(&url)?;
    let target = target.unwrap_or_else(|| "example.com:443".into());
    let (host, port) = target.rsplit_once(':').ok_or("target is host:port")?;
    let port: u16 = port.parse().map_err(|_| "target port")?;
    let started = std::time::Instant::now();
    let stream = tokio::time::timeout(std::time::Duration::from_secs(10), dial(&url, host, port))
        .await
        .map_err(|_| "The proxy did not answer within 10 seconds.".to_string())??;
    drop(stream);
    Ok(started.elapsed().as_millis() as u64)
}

/// A `reqwest` builder with this kind's proxy applied.
pub fn http_builder(kind: Kind) -> reqwest::ClientBuilder {
    apply(reqwest::Client::builder(), kind)
}

/// Apply this kind's proxy to an existing builder.
pub fn apply(builder: reqwest::ClientBuilder, kind: Kind) -> reqwest::ClientBuilder {
    let r = rule(kind);
    match r.mode.as_str() {
        "off" => builder.no_proxy(),
        "custom" if !r.url.trim().is_empty() => match reqwest::Proxy::all(r.url.trim()) {
            Ok(p) => builder.proxy(p),
            Err(e) => {
                log::warn!("proxy: ignoring {kind:?} proxy: {e}");
                builder
            }
        },
        _ => builder,
    }
}

/// The environment a child process (kubectl, the AWS CLI) should get: the
/// SSH kind's proxy as `HTTPS_PROXY`, or nothing to change.
pub fn child_env() -> Vec<(&'static str, String)> {
    match custom_url(Kind::Ssh) {
        Some(url) => vec![("HTTPS_PROXY", url.clone()), ("HTTP_PROXY", url), ("NO_PROXY", "127.0.0.1,localhost".into())],
        None => Vec::new(),
    }
}

// ── Dialing through a proxy ───────────────────────────────────────────────────

/// A TCP connection to `host:port` through `proxy`.
pub async fn dial(proxy: &str, host: &str, port: u16) -> Result<TcpStream, String> {
    let u = reqwest::Url::parse(proxy.trim()).map_err(|e| format!("Bad proxy URL: {e}"))?;
    let phost = u.host_str().ok_or("The proxy URL has no host")?.to_string();
    let pport = u.port_or_known_default().ok_or("The proxy URL has no port")?;
    let user = urldecode(u.username());
    let pass = u.password().map(urldecode).unwrap_or_default();
    let mut s = TcpStream::connect((phost.as_str(), pport))
        .await
        .map_err(|e| format!("Could not reach the proxy {phost}:{pport}: {e}"))?;
    let _ = s.set_nodelay(true);
    match u.scheme() {
        "socks5" | "socks5h" => socks5_connect(&mut s, host, port, &user, &pass).await?,
        _ => http_connect(&mut s, host, port, &user, &pass).await?,
    }
    Ok(s)
}

fn urldecode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

async fn http_connect(s: &mut TcpStream, host: &str, port: u16, user: &str, pass: &str) -> Result<(), String> {
    use base64::Engine;
    let target = if host.contains(':') { format!("[{host}]:{port}") } else { format!("{host}:{port}") };
    let mut req = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
    if !user.is_empty() {
        let token = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
        req.push_str(&format!("Proxy-Authorization: Basic {token}\r\n"));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes()).await.map_err(|e| format!("Proxy write failed: {e}"))?;
    // Read the reply headers byte by byte: anything past them belongs to the
    // connection being made.
    let mut head = Vec::with_capacity(256);
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        let n = s.read(&mut byte).await.map_err(|e| format!("Proxy read failed: {e}"))?;
        if n == 0 {
            return Err("The proxy closed the connection.".into());
        }
        head.push(byte[0]);
        if head.len() > 8192 {
            return Err("The proxy sent an oversized reply.".into());
        }
    }
    let line = String::from_utf8_lossy(&head);
    let status = line.split_whitespace().nth(1).unwrap_or("");
    match status {
        "200" => Ok(()),
        "407" => Err("The proxy asked for a login: add user:password@ to its URL.".into()),
        "403" => Err(format!("The proxy refused to connect to {target}.")),
        other => Err(format!("The proxy answered {other} to CONNECT {target}.")),
    }
}

async fn socks5_connect(s: &mut TcpStream, host: &str, port: u16, user: &str, pass: &str) -> Result<(), String> {
    let io = |e: std::io::Error| format!("SOCKS proxy: {e}");
    let methods: &[u8] = if user.is_empty() { &[0x00] } else { &[0x00, 0x02] };
    let mut hello = vec![0x05, methods.len() as u8];
    hello.extend_from_slice(methods);
    s.write_all(&hello).await.map_err(io)?;
    let mut reply = [0u8; 2];
    s.read_exact(&mut reply).await.map_err(io)?;
    match reply[1] {
        0x00 => {}
        0x02 => {
            let mut auth = vec![0x01, user.len() as u8];
            auth.extend_from_slice(user.as_bytes());
            auth.push(pass.len() as u8);
            auth.extend_from_slice(pass.as_bytes());
            s.write_all(&auth).await.map_err(io)?;
            let mut r = [0u8; 2];
            s.read_exact(&mut r).await.map_err(io)?;
            if r[1] != 0 {
                return Err("The SOCKS proxy rejected the login.".into());
            }
        }
        0xff => return Err("The SOCKS proxy needs a login: add user:password@ to its URL.".into()),
        m => return Err(format!("The SOCKS proxy chose an unsupported method ({m}).")),
    }
    // The host goes by name, so the proxy resolves it (a private hostname may
    // only resolve on the proxy's side).
    let mut req = vec![0x05, 0x01, 0x00];
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        match ip {
            std::net::IpAddr::V4(v4) => {
                req.push(0x01);
                req.extend_from_slice(&v4.octets());
            }
            std::net::IpAddr::V6(v6) => {
                req.push(0x04);
                req.extend_from_slice(&v6.octets());
            }
        }
    } else {
        if host.len() > 255 {
            return Err("Host name too long for SOCKS.".into());
        }
        req.push(0x03);
        req.push(host.len() as u8);
        req.extend_from_slice(host.as_bytes());
    }
    req.extend_from_slice(&port.to_be_bytes());
    s.write_all(&req).await.map_err(io)?;
    let mut head = [0u8; 4];
    s.read_exact(&mut head).await.map_err(io)?;
    if head[1] != 0x00 {
        let why = match head[1] {
            0x02 => "not allowed by its rules",
            0x03 => "network unreachable",
            0x04 => "host unreachable",
            0x05 => "connection refused",
            0x06 => "timed out",
            _ => "failed",
        };
        return Err(format!("The SOCKS proxy could not reach {host}:{port}: {why}."));
    }
    // Skip the bound address it reports.
    let skip = match head[3] {
        0x01 => 4 + 2,
        0x04 => 16 + 2,
        0x03 => {
            let mut l = [0u8; 1];
            s.read_exact(&mut l).await.map_err(io)?;
            l[0] as usize + 2
        }
        _ => return Err("The SOCKS proxy sent a malformed reply.".into()),
    };
    let mut rest = vec![0u8; skip];
    s.read_exact(&mut rest).await.map_err(io)?;
    Ok(())
}

/// A local listener whose every connection is opened to `host:port` through
/// `proxy`. Dropping it stops listening; connections already open run on.
pub struct ProxyForward {
    pub local_port: u16,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for ProxyForward {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl ProxyForward {
    /// Start forwarding; fails when the proxy cannot reach the target, so the
    /// connect says why instead of a driver timing out on a dead port.
    pub async fn start(proxy: &str, host: &str, port: u16) -> Result<Self, String> {
        // Proved once up front: the error a driver would give is "connection
        // reset", which says nothing about the proxy.
        drop(dial(proxy, host, port).await?);
        let listener = TcpListener::bind("127.0.0.1:0").await.map_err(|e| format!("Could not open a local port: {e}"))?;
        let local_port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let (proxy, host) = (proxy.to_string(), host.to_string());
        let task = tokio::spawn(async move {
            while let Ok((mut inbound, _)) = listener.accept().await {
                let (proxy, host) = (proxy.clone(), host.clone());
                tokio::spawn(async move {
                    let _ = inbound.set_nodelay(true);
                    match dial(&proxy, &host, port).await {
                        Ok(mut outbound) => {
                            let _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await;
                        }
                        Err(e) => log::warn!("proxy forward to {host}:{port}: {e}"),
                    }
                });
            }
        });
        Ok(ProxyForward { local_port, task })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_urls() {
        assert!(validate("http://proxy:3128").is_ok());
        assert!(validate("socks5://u:p@10.0.0.1:1080").is_ok());
        assert!(validate("https://proxy:443").is_err());
        assert!(validate("ftp://x:1").is_err());
        assert!(validate("nonsense").is_err());
    }

    #[test]
    fn decodes_credentials() {
        assert_eq!(urldecode("p%40ss"), "p@ss");
        assert_eq!(urldecode("plain"), "plain");
    }

    /// A tiny SOCKS5 server (no auth) and an HTTP CONNECT server in-process,
    /// each forwarding to an echo server: both dial paths carry bytes.
    #[tokio::test]
    async fn dials_through_socks5_and_http_connect() {
        let echo = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let echo_port = echo.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut c, _)) = echo.accept().await {
                tokio::spawn(async move {
                    let (mut r, mut w) = c.split();
                    let _ = tokio::io::copy(&mut r, &mut w).await;
                });
            }
        });

        let socks = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let socks_port = socks.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut c, _)) = socks.accept().await {
                tokio::spawn(async move {
                    let mut b = [0u8; 3];
                    c.read_exact(&mut b).await.unwrap();
                    c.write_all(&[5, 0]).await.unwrap();
                    let mut h = [0u8; 4];
                    c.read_exact(&mut h).await.unwrap();
                    let mut l = [0u8; 1];
                    c.read_exact(&mut l).await.unwrap();
                    let mut name = vec![0u8; l[0] as usize];
                    c.read_exact(&mut name).await.unwrap();
                    let mut p = [0u8; 2];
                    c.read_exact(&mut p).await.unwrap();
                    let mut up = TcpStream::connect(("127.0.0.1", u16::from_be_bytes(p))).await.unwrap();
                    c.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]).await.unwrap();
                    let _ = tokio::io::copy_bidirectional(&mut c, &mut up).await;
                });
            }
        });

        let http = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let http_port = http.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut c, _)) = http.accept().await {
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut byte = [0u8; 1];
                    while !head.ends_with(b"\r\n\r\n") {
                        c.read_exact(&mut byte).await.unwrap();
                        head.push(byte[0]);
                    }
                    let line = String::from_utf8_lossy(&head).to_string();
                    let target = line.split_whitespace().nth(1).unwrap().to_string();
                    let port: u16 = target.rsplit_once(':').unwrap().1.parse().unwrap();
                    let mut up = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
                    c.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await.unwrap();
                    let _ = tokio::io::copy_bidirectional(&mut c, &mut up).await;
                });
            }
        });

        for proxy in [format!("socks5://127.0.0.1:{socks_port}"), format!("http://127.0.0.1:{http_port}")] {
            let fwd = ProxyForward::start(&proxy, "localhost", echo_port).await.expect("forward");
            let mut s = TcpStream::connect(("127.0.0.1", fwd.local_port)).await.unwrap();
            s.write_all(b"ping").await.unwrap();
            let mut buf = [0u8; 4];
            s.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ping", "{proxy}");
        }
    }
}
