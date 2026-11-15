//! Opening Stroke at a database from outside the app: a `stroke://open?url=…`
//! link (what `npx strokedb` and stroke.click/studio fire) or a connection on the
//! command line (`stroke postgres://…`, `stroke ./app.db`).
//!
//! Nothing here connects. A link can come from any web page, so all it does is
//! open the New Connection dialog with the fields filled in; the person reads
//! them and presses Connect. The request is parked in `LaunchState` and the page
//! takes it with `take_open_request`, either on startup (a link that launched
//! the app arrives before the page can listen) or when `open-request` pings it
//! (a link to the app that is already running - macOS hands it over that way).

use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, Url};

/// Schemes this build answers to. Stable and Nightly are separate apps, so each
/// has its own and a link opens the one it names.
pub const SCHEMES: &[&str] = &["stroke", "stroke-nightly"];

/// Longest connection string accepted from outside. Real ones are a few hundred
/// characters; this only stops a hostile link from parking megabytes.
const MAX_URI: usize = 4096;
const MAX_NAME: usize = 120;

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct OpenRequest {
    /// A connection URL, a `.env` line, or an absolute path to a database file.
    pub uri: String,
    /// What to call the connection, e.g. the project folder `npx strokedb` ran in.
    pub name: Option<String>,
}

#[derive(Default)]
pub struct LaunchState(Mutex<Option<OpenRequest>>);

/// `stroke://open?url=<connection>&name=<label>`. The host is the action, so a
/// later `stroke://something-else` is not misread as an open.
pub fn parse_link(link: &str) -> Option<OpenRequest> {
    let url = Url::parse(link.trim()).ok()?;
    if !SCHEMES.contains(&url.scheme()) {
        return None;
    }
    // `stroke://open?…` puts "open" in the host; `stroke:open?…` in the path.
    let action = url.host_str().unwrap_or_else(|| url.path().trim_matches('/'));
    if action != "open" {
        return None;
    }
    let mut uri = None;
    let mut name = None;
    for (k, v) in url.query_pairs() {
        match &*k {
            "url" | "uri" => uri = Some(v.trim().to_string()),
            "name" => name = Some(v.trim().to_string()),
            _ => {}
        }
    }
    let uri = uri.filter(|u| !u.is_empty() && u.len() <= MAX_URI)?;
    let name = name
        .filter(|n| !n.is_empty())
        .map(|n| n.chars().filter(|c| !c.is_control()).take(MAX_NAME).collect());
    Some(OpenRequest { uri, name })
}

/// One command-line argument: a `stroke://` link, a connection URL, or a path to
/// a database file. Flags (and macOS's `-psn_…`) are not connections.
pub fn parse_arg(arg: &str, cwd: &Path) -> Option<OpenRequest> {
    let arg = arg.trim();
    if arg.is_empty() || arg.starts_with('-') || arg.len() > MAX_URI {
        return None;
    }
    if let Some(req) = parse_link(arg) {
        return Some(req);
    }
    if is_connection_url(arg) {
        return Some(OpenRequest { uri: arg.to_string(), name: None });
    }
    let path = cwd.join(arg);
    if is_database_file(&path) {
        let abs = path.canonicalize().unwrap_or(path);
        return Some(OpenRequest { uri: abs.to_string_lossy().into_owned(), name: None });
    }
    None
}

/// The first argument that names a connection; the program path is skipped.
pub fn from_args<I: IntoIterator<Item = String>>(args: I, cwd: &Path) -> Option<OpenRequest> {
    args.into_iter().skip(1).find_map(|a| parse_arg(&a, cwd))
}

fn is_connection_url(s: &str) -> bool {
    let Some((scheme, rest)) = s.split_once("://") else { return false };
    !rest.is_empty()
        && !SCHEMES.contains(&scheme)
        && !matches!(scheme, "http" | "https" | "file")
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
}

fn is_database_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "db" | "sqlite" | "sqlite3" | "db3" | "duckdb") && path.is_file()
}

/// Park the request and tell the page; bring the window forward.
pub fn deliver(app: &AppHandle, req: OpenRequest) {
    if let Some(state) = app.try_state::<LaunchState>() {
        *state.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(req);
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
    let _ = app.emit("open-request", ());
}

/// The parked request, once: taking it clears it.
#[tauri::command]
pub fn take_open_request(state: tauri::State<'_, LaunchState>) -> Option<OpenRequest> {
    state.0.lock().unwrap_or_else(|e| e.into_inner()).take()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_with_url_and_name() {
        let r = parse_link("stroke://open?url=postgres%3A%2F%2Fme%3Ap%40ss%40db%3A5432%2Fapp&name=my-app").unwrap();
        assert_eq!(r.uri, "postgres://me:p@ss@db:5432/app");
        assert_eq!(r.name.as_deref(), Some("my-app"));
    }

    #[test]
    fn nightly_scheme_and_path_form() {
        assert!(parse_link("stroke-nightly://open?url=mysql%3A%2F%2Fa").is_some());
        assert!(parse_link("stroke:open?url=mysql%3A%2F%2Fa").is_some());
    }

    #[test]
    fn link_rejects_other_actions_schemes_and_empty() {
        assert_eq!(parse_link("stroke://delete?url=x"), None);
        assert_eq!(parse_link("https://open?url=x"), None);
        assert_eq!(parse_link("stroke://open"), None);
        assert_eq!(parse_link("stroke://open?url="), None);
        let long = format!("stroke://open?url={}", "a".repeat(MAX_URI + 1));
        assert_eq!(parse_link(&long), None);
    }

    #[test]
    fn name_is_trimmed_of_control_chars_and_capped() {
        let r = parse_link(&format!("stroke://open?url=x%3A%2F%2Fy&name=a%0Ab{}", "c".repeat(500))).unwrap();
        let name = r.name.unwrap();
        assert!(!name.contains('\n'));
        assert_eq!(name.chars().count(), MAX_NAME);
    }

    #[test]
    fn args_pick_the_first_connection() {
        let cwd = std::env::temp_dir();
        let args = ["stroke", "--flag", "-psn_0_123", "postgresql://u@h/db"].map(String::from);
        assert_eq!(from_args(args, &cwd).unwrap().uri, "postgresql://u@h/db");
        let none = ["stroke", "https://example.com", "notes.txt"].map(String::from);
        assert_eq!(from_args(none, &cwd), None);
    }

    #[test]
    fn args_take_a_database_file_as_an_absolute_path() {
        let dir = std::env::temp_dir().join(format!("stroke-launch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("app.db"), b"").unwrap();
        let r = parse_arg("app.db", &dir).unwrap();
        assert!(Path::new(&r.uri).is_absolute());
        assert!(r.uri.ends_with("app.db"));
        assert_eq!(parse_arg("missing.db", &dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
