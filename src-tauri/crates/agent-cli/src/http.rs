//! Just enough HTTP/1.1 to drive a local `opencode serve`: one request per
//! connection with a JSON body, and one long-lived server-sent-event stream.
//! It only ever talks to a loopback server we started, so there is no TLS, no
//! proxy and no redirect handling.

use std::io;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    /// A ready `Authorization` header value.
    pub auth: Option<String>,
}

impl Endpoint {
    /// From `http://127.0.0.1:4096` (anything after the port is ignored).
    pub fn parse(url: &str, auth: Option<String>) -> Option<Self> {
        let rest = url.trim().strip_prefix("http://")?;
        let hostport = rest.split('/').next()?;
        let (host, port) = hostport.rsplit_once(':')?;
        Some(Self {
            host: host.to_string(),
            port: port.parse().ok()?,
            auth,
        })
    }

    fn head(&self, method: &str, path: &str, accept: &str, body_len: Option<usize>) -> String {
        let mut h = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}:{}\r\nAccept: {accept}\r\nConnection: close\r\n",
            self.host, self.port
        );
        if let Some(a) = &self.auth {
            h.push_str(&format!("Authorization: {a}\r\n"));
        }
        if let Some(n) = body_len {
            h.push_str(&format!(
                "Content-Type: application/json\r\nContent-Length: {n}\r\n"
            ));
        }
        h.push_str("\r\n");
        h
    }
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Response {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn json(&self) -> Option<serde_json::Value> {
        serde_json::from_slice(&self.body).ok()
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).trim().to_string()
    }
}

/// One request. `body` makes it carry JSON.
pub async fn request(
    ep: &Endpoint,
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
    timeout: Duration,
) -> io::Result<Response> {
    let work = async {
        let mut stream = TcpStream::connect((ep.host.as_str(), ep.port)).await?;
        let body = match body {
            Some(b) => Some(serde_json::to_vec(b).map_err(io::Error::other)?),
            None if method == "GET" => None,
            None => Some(Vec::new()),
        };
        let head = ep.head(
            method,
            path,
            "application/json",
            body.as_ref().map(Vec::len),
        );
        stream.write_all(head.as_bytes()).await?;
        if let Some(b) = &body {
            stream.write_all(b).await?;
        }
        let mut reader = BufReader::new(stream);
        let head = read_head(&mut reader).await?;
        let mut raw = Vec::new();
        if head.chunked {
            let mut body = Body::new(true);
            while let Some(chunk) = body.next(&mut reader).await? {
                raw.extend_from_slice(&chunk);
            }
        } else if let Some(n) = head.length {
            raw.resize(n, 0);
            reader.read_exact(&mut raw).await?;
        } else {
            reader.read_to_end(&mut raw).await?;
        }
        Ok(Response {
            status: head.status,
            body: raw,
        })
    };
    tokio::time::timeout(timeout, work).await.map_err(|_| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            format!("{method} {path} timed out"),
        )
    })?
}

struct Head {
    status: u16,
    chunked: bool,
    length: Option<usize>,
}

async fn read_head<R: AsyncBufReadExt + Unpin>(r: &mut R) -> io::Result<Head> {
    let mut line = String::new();
    if r.read_line(&mut line).await? == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "connection closed before a response",
        ));
    }
    let status = line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("bad status line {line:?}"),
            )
        })?;
    let mut head = Head {
        status,
        chunked: false,
        length: None,
    };
    loop {
        line.clear();
        if r.read_line(&mut line).await? == 0 {
            break;
        }
        let l = line.trim_end();
        if l.is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
            if k == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked") {
                head.chunked = true;
            } else if k == "content-length" {
                head.length = v.parse().ok();
            }
        }
    }
    Ok(head)
}

/// A response body read a piece at a time, chunked or not.
struct Body {
    chunked: bool,
    done: bool,
}

impl Body {
    fn new(chunked: bool) -> Self {
        Self {
            chunked,
            done: false,
        }
    }

    async fn next<R: AsyncBufReadExt + Unpin>(&mut self, r: &mut R) -> io::Result<Option<Vec<u8>>> {
        if self.done {
            return Ok(None);
        }
        if !self.chunked {
            let mut buf = vec![0u8; 16 * 1024];
            let n = r.read(&mut buf).await?;
            if n == 0 {
                self.done = true;
                return Ok(None);
            }
            buf.truncate(n);
            return Ok(Some(buf));
        }
        let mut size = String::new();
        loop {
            size.clear();
            if r.read_line(&mut size).await? == 0 {
                self.done = true;
                return Ok(None);
            }
            if !size.trim().is_empty() {
                break;
            }
        }
        let hex = size.trim().split(';').next().unwrap_or("").trim();
        let n = usize::from_str_radix(hex, 16).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("bad chunk size {hex:?}"),
            )
        })?;
        if n == 0 {
            self.done = true;
            return Ok(None);
        }
        let mut buf = vec![0u8; n];
        r.read_exact(&mut buf).await?;
        // The CRLF after the chunk data.
        let mut crlf = [0u8; 2];
        r.read_exact(&mut crlf).await?;
        Ok(Some(buf))
    }
}

/// A `text/event-stream`, yielding each event's `data` payload.
pub struct EventStream {
    reader: BufReader<TcpStream>,
    body: Body,
    pending: Vec<u8>,
    data: String,
}

impl EventStream {
    pub async fn connect(ep: &Endpoint, path: &str, timeout: Duration) -> io::Result<Self> {
        let work = async {
            let mut stream = TcpStream::connect((ep.host.as_str(), ep.port)).await?;
            stream
                .write_all(ep.head("GET", path, "text/event-stream", None).as_bytes())
                .await?;
            let mut reader = BufReader::new(stream);
            let head = read_head(&mut reader).await?;
            if !(200..300).contains(&head.status) {
                return Err(io::Error::other(format!(
                    "event stream refused with HTTP {}",
                    head.status
                )));
            }
            Ok(Self {
                reader,
                body: Body::new(head.chunked),
                pending: Vec::new(),
                data: String::new(),
            })
        };
        tokio::time::timeout(timeout, work)
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "event stream did not open"))?
    }

    /// The next event's data; `None` once the server closes the stream.
    pub async fn next(&mut self) -> io::Result<Option<String>> {
        loop {
            while let Some(i) = self.pending.iter().position(|&b| b == b'\n') {
                let raw: Vec<u8> = self.pending.drain(..=i).collect();
                let line = String::from_utf8_lossy(&raw);
                let line = line.trim_end_matches(['\n', '\r']);
                if line.is_empty() {
                    if !self.data.is_empty() {
                        return Ok(Some(std::mem::take(&mut self.data)));
                    }
                } else if let Some(d) = line.strip_prefix("data:") {
                    if !self.data.is_empty() {
                        self.data.push('\n');
                    }
                    self.data.push_str(d.strip_prefix(' ').unwrap_or(d));
                }
                // `event:`, `id:`, `retry:` and `:` comments carry nothing we use.
            }
            match self.body.next(&mut self.reader).await? {
                Some(chunk) => self.pending.extend_from_slice(&chunk),
                None => return Ok(None),
            }
        }
    }
}

/// `Basic …` for a user and password.
pub fn basic_auth(user: &str, password: &str) -> String {
    format!("Basic {}", base64(format!("{user}:{password}").as_bytes()))
}

fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for c in input.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// A query-string value: everything but unreserved characters and `/`
/// percent-encoded, so a folder with spaces, `+` or non-ASCII survives.
pub fn query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(
            basic_auth("opencode", "pw123"),
            "Basic b3BlbmNvZGU6cHcxMjM="
        );
    }

    #[test]
    fn query_escape_keeps_paths_readable_and_safe() {
        assert_eq!(query_escape("/Users/me/my app+1"), "/Users/me/my%20app%2B1");
        assert_eq!(query_escape("/tmp/é"), "/tmp/%C3%A9");
    }

    #[test]
    fn endpoint_parses_the_url_opencode_prints() {
        let ep = Endpoint::parse("http://127.0.0.1:4096", None).unwrap();
        assert_eq!((ep.host.as_str(), ep.port), ("127.0.0.1", 4096));
        assert!(Endpoint::parse("https://x:1", None).is_none());
    }

    async fn serve_once(reply: &'static [u8]) -> Endpoint {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf).await;
            s.write_all(reply).await.unwrap();
        });
        Endpoint {
            host: "127.0.0.1".into(),
            port,
            auth: None,
        }
    }

    #[tokio::test]
    async fn reads_a_content_length_body() {
        let ep = serve_once(
            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 36\r\n\r\n{\"healthy\":true,\"version\":\"1.18.10\"}",
        )
        .await;
        let r = request(&ep, "GET", "/global/health", None, Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.json().unwrap()["version"], "1.18.10");
    }

    #[tokio::test]
    async fn reads_a_chunked_event_stream_split_mid_event() {
        // As `opencode serve` frames `/event`: chunked, one `data:` per event,
        // and here a chunk boundary in the middle of a line.
        let ep = serve_once(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n\
14\r\ndata: {\"type\":\"a\"}\n\n\r\n\
6\r\ndata: \r\n\
e\r\n{\"type\":\"b\"}\n\n\r\n\
0\r\n\r\n",
        )
        .await;
        let mut s = EventStream::connect(&ep, "/event", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(s.next().await.unwrap().as_deref(), Some(r#"{"type":"a"}"#));
        assert_eq!(s.next().await.unwrap().as_deref(), Some(r#"{"type":"b"}"#));
        assert_eq!(s.next().await.unwrap(), None);
    }
}
