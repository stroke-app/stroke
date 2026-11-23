//! Web access for the AI agent: a search, and a page reader.
//!
//! Both live in Rust rather than the webview because the webview cannot make
//! these requests at all - no search endpoint or arbitrary site sends CORS
//! headers that would let a `fetch()` from the app origin read the response.
//!
//! Search goes through DuckDuckGo's HTML endpoint, which needs no API key. That
//! keeps the feature working on a fresh install with nothing to configure, at
//! the cost of parsing markup we do not control: the extractor below is written
//! to return *fewer* results rather than wrong ones when the markup shifts, and
//! every caller treats an empty result list as "no answer", never as an error.

use serde::Serialize;
use std::time::Duration;

/// Browsers get served different markup than an unrecognised client, and some
/// sites refuse an empty user agent outright.
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                  (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// A slow page must not hold the agent's tool call open indefinitely.
const TIMEOUT: Duration = Duration::from_secs(15);

/// Cap on extracted page text. Whatever comes back is going into a model prompt,
/// so the limit is about context budget, not memory.
const MAX_PAGE_CHARS: usize = 12_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

fn client() -> Result<reqwest::Client, String> {
    crate::proxy::http_builder(crate::proxy::Kind::Ai)
        .timeout(TIMEOUT)
        .user_agent(UA)
        .build()
        .map_err(|e| format!("Could not start the web client: {e}"))
}

/// Search the web and return the top `limit` hits.
pub async fn search(query: String, limit: usize) -> Result<Vec<SearchHit>, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("Empty search query".into());
    }
    let limit = limit.clamp(1, 10);

    let body = client()?
        .post("https://html.duckduckgo.com/html/")
        .form(&[("q", q)])
        .send()
        .await
        .map_err(|e| format!("Search request failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Search request failed: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Could not read search results: {e}"))?;

    Ok(parse_results(&body, limit))
}

/// Pull `(title, url, snippet)` triples out of a DuckDuckGo HTML result page.
///
/// Kept to plain string scanning: a full HTML parser would be a heavy dependency
/// for two known anchor classes, and would not make the markup any less liable
/// to change. Anything that does not match the expected shape is skipped.
fn parse_results(html: &str, limit: usize) -> Vec<SearchHit> {
    let mut out = Vec::new();
    // Each result opens with an anchor carrying class `result__a`.
    for chunk in html.split("result__a").skip(1) {
        if out.len() >= limit {
            break;
        }
        let Some(href) = attr_after(chunk, "href=\"") else { continue };
        let Some(url) = real_url(&href) else { continue };
        let Some(title) = text_after_tag(chunk) else { continue };
        if title.is_empty() {
            continue;
        }
        let snippet = chunk
            .split_once("result__snippet")
            .and_then(|(_, rest)| text_after_tag(rest))
            .unwrap_or_default();
        out.push(SearchHit { title, url, snippet });
    }
    out
}

/// The first `attr="…"` value appearing after `marker`, unescaped.
fn attr_after(chunk: &str, marker: &str) -> Option<String> {
    let start = chunk.find(marker)? + marker.len();
    let rest = &chunk[start..];
    let end = rest.find('"')?;
    Some(unescape_entities(&rest[..end]))
}

/// DuckDuckGo wraps every result in a redirect (`/l/?uddg=<encoded target>`).
/// Unwrap it so the model gets a URL it can actually cite or fetch.
fn real_url(href: &str) -> Option<String> {
    let direct = |u: &str| u.starts_with("http://") || u.starts_with("https://");
    if let Some(idx) = href.find("uddg=") {
        let tail = &href[idx + 5..];
        let encoded = tail.split('&').next().unwrap_or(tail);
        let decoded = urlencoding::decode(encoded).ok()?.into_owned();
        return direct(&decoded).then_some(decoded);
    }
    // Protocol-relative links come back as `//example.com/…`.
    if let Some(stripped) = href.strip_prefix("//") {
        return Some(format!("https://{stripped}"));
    }
    direct(href).then(|| href.to_string())
}

/// The visible text of the tag the chunk opens with: skip to `>`, read to `<`.
fn text_after_tag(chunk: &str) -> Option<String> {
    let start = chunk.find('>')? + 1;
    let rest = &chunk[start..];
    let end = rest.find('<').unwrap_or(rest.len());
    Some(collapse_ws(&unescape_entities(&strip_tags(&rest[..end]))))
}

/// Fetch a URL and return its readable text.
pub async fn fetch_page(url: String) -> Result<String, String> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("Only http and https URLs can be fetched".into());
    }

    let res = client()?
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Request failed: {e}"))?;

    // Binaries would arrive as replacement characters and waste the whole budget.
    let content_type = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let is_texty = content_type.is_empty()
        || content_type.contains("text/")
        || content_type.contains("json")
        || content_type.contains("xml");
    if !is_texty {
        return Err(format!("Not a readable text page (content type: {content_type})"));
    }

    let body = res.text().await.map_err(|e| format!("Could not read the page: {e}"))?;
    let text = if content_type.contains("html") || body.trim_start().starts_with('<') {
        html_to_text(&body)
    } else {
        collapse_ws(&body)
    };

    if text.is_empty() {
        return Err("The page had no readable text".into());
    }
    Ok(truncate_chars(&text, MAX_PAGE_CHARS))
}

/// Reduce an HTML document to its readable text.
fn html_to_text(html: &str) -> String {
    // script/style hold code, not prose, and would dominate the character budget.
    let mut cleaned = String::with_capacity(html.len());
    let mut rest = html;
    loop {
        let lower = rest.to_ascii_lowercase();
        let next = ["<script", "<style", "<noscript"]
            .iter()
            .filter_map(|tag| lower.find(tag).map(|i| (i, *tag)))
            .min_by_key(|(i, _)| *i);
        let Some((start, tag)) = next else {
            cleaned.push_str(rest);
            break;
        };
        cleaned.push_str(&rest[..start]);
        let close = format!("</{}>", tag.trim_start_matches('<'));
        match lower[start..].find(&close) {
            Some(rel) => rest = &rest[start + rel + close.len()..],
            // Unclosed tag: everything after it is suspect, so stop here.
            None => break,
        }
    }
    collapse_ws(&unescape_entities(&strip_tags(&cleaned)))
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for ch in s.chars() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            // A tag boundary is also a word boundary: without this, "a</b>b"
            // would run together into "ab".
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
        if ch == '>' && depth == 0 {
            out.push(' ');
        }
    }
    out
}

/// Only the entities that actually show up in prose; anything else is left as-is.
fn unescape_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// Collapse runs of whitespace, keeping paragraph breaks readable.
fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank_run = 0usize;
    for line in s.lines() {
        let t = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if t.is_empty() {
            blank_run += 1;
            if blank_run == 1 && !out.is_empty() {
                out.push('\n');
            }
            continue;
        }
        blank_run = 0;
        out.push_str(&t);
        out.push('\n');
    }
    out.trim().to_string()
}

/// Truncate on a character boundary, not a byte one.
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push_str("\n\n[truncated]");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_duckduckgo_redirects() {
        let href = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fa&amp;rut=xyz";
        assert_eq!(real_url(href).as_deref(), Some("https://example.com/a"));
    }

    #[test]
    fn keeps_a_direct_url() {
        assert_eq!(real_url("https://example.com").as_deref(), Some("https://example.com"));
    }

    #[test]
    fn rejects_a_non_http_target() {
        assert_eq!(real_url("javascript:alert(1)"), None);
    }

    #[test]
    fn parses_a_result_block() {
        let html = r#"
          <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Frust-lang.org">The Rust Language</a>
          <a class="result__snippet">A language empowering everyone.</a>
        "#;
        let hits = parse_results(html, 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://rust-lang.org");
        assert_eq!(hits[0].title, "The Rust Language");
        assert_eq!(hits[0].snippet, "A language empowering everyone.");
    }

    #[test]
    fn skips_malformed_blocks_instead_of_failing() {
        // No href at all, then a well-formed one.
        let html = r#"
          <a class="result__a">Broken</a>
          <a class="result__a" href="https://ok.example">Fine</a>
        "#;
        let hits = parse_results(html, 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Fine");
    }

    #[test]
    fn honours_the_limit() {
        let one = r#"<a class="result__a" href="https://a.example">A</a>"#.repeat(6);
        assert_eq!(parse_results(&one, 3).len(), 3);
    }

    #[test]
    fn drops_script_and_style_from_page_text() {
        let html = "<html><head><style>.a{color:red}</style></head>\
                    <body><p>Hello</p><script>evil()</script><p>World</p></body></html>";
        let text = html_to_text(html);
        assert!(text.contains("Hello"), "got: {text}");
        assert!(text.contains("World"), "got: {text}");
        assert!(!text.contains("evil"), "got: {text}");
        assert!(!text.contains("color:red"), "got: {text}");
    }

    #[test]
    fn keeps_words_apart_across_tags() {
        assert!(html_to_text("<p>one</p><p>two</p>").contains("one"));
        assert!(!html_to_text("<b>one</b><b>two</b>").contains("onetwo"));
    }

    #[test]
    fn truncates_on_a_char_boundary() {
        let s = "é".repeat(50);
        let out = truncate_chars(&s, 10);
        assert!(out.starts_with(&"é".repeat(10)));
        assert!(out.ends_with("[truncated]"));
    }
}
