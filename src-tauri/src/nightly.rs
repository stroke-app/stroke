//! The update track: Stable and Nightly are separate apps (their own name,
//! identifier and data folder; see tauri.nightly.conf.json), so switching
//! installs the other one beside this one, if it isn't there already, and
//! opens it. Nothing here is replaced: each app keeps its own settings.

use std::path::PathBuf;

const NIGHTLY: &str = "https://github.com/stroke-app/stroke/releases/download/nightly";
const STABLE: &str = "https://github.com/stroke-app/stroke/releases/latest/download";

#[derive(Clone, Copy, PartialEq)]
enum Track {
    Stable,
    Nightly,
}

impl Track {
    fn parse(s: &str) -> Result<Self, String> {
        match s {
            "stable" => Ok(Track::Stable),
            "nightly" => Ok(Track::Nightly),
            _ => Err(format!("Unknown update track: {s}")),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Track::Stable => "Stroke",
            Track::Nightly => "Stroke Nightly",
        }
    }
    fn base(self) -> &'static str {
        match self {
            Track::Stable => STABLE,
            Track::Nightly => NIGHTLY,
        }
    }
}

fn client() -> reqwest::Client {
    crate::proxy::http_builder(crate::proxy::Kind::App)
        .user_agent("stroke-nightly-installer")
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .unwrap_or_default()
}

async fn download(track: Track, asset: &str, to: &std::path::Path) -> Result<(), String> {
    let name = track.name();
    let resp = client()
        .get(format!("{}/{asset}", track.base()))
        .send()
        .await
        .map_err(|e| format!("Couldn't download {name}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Couldn't download {name} (HTTP {}).", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    tokio::fs::write(to, &bytes).await.map_err(|e| format!("Couldn't save the download: {e}"))
}

/// The stable version the release feed offers, for its versioned asset names.
#[cfg(any(target_os = "windows", target_os = "linux"))]
async fn stable_version() -> Result<String, String> {
    let v: serde_json::Value = client()
        .get(format!("{STABLE}/latest.json"))
        .send()
        .await
        .map_err(|e| format!("Couldn't reach the release feed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Couldn't read the release feed: {e}"))?;
    v.get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.trim_start_matches('v').to_string())
        .ok_or_else(|| "The release feed has no version.".to_string())
}

/// Download and install Stroke Nightly; returns where it was installed.
#[tauri::command]
pub async fn nightly_install() -> Result<String, String> {
    track_install("nightly".into()).await
}

/// Start the installed Nightly.
#[tauri::command]
pub async fn nightly_open(path: String) -> Result<(), String> {
    track_open("nightly".into(), path).await
}

/// Where the other track's app is installed already, or null. Switching opens
/// that one instead of downloading it again.
#[tauri::command]
pub async fn track_installed(track: String) -> Result<Option<String>, String> {
    let track = Track::parse(&track)?;
    Ok(installed_path(track).map(|p| p.display().to_string()))
}

/// Download and install the app for `track`; returns where it went.
#[tauri::command]
pub async fn track_install(track: String) -> Result<String, String> {
    let track = Track::parse(&track)?;
    let tmp = std::env::temp_dir().join(format!("stroke-track-{}", std::process::id()));
    tokio::fs::create_dir_all(&tmp).await.map_err(|e| e.to_string())?;
    let result = install_into(track, &tmp).await;
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    result
}

/// Start the app installed for `track` at `path`.
#[tauri::command]
pub async fn track_open(track: String, path: String) -> Result<(), String> {
    let track = Track::parse(&track)?;
    open_app(track, &path)
}

#[cfg(target_os = "macos")]
fn installed_path(track: Track) -> Option<PathBuf> {
    let bundle = format!("{}.app", track.name());
    [PathBuf::from("/Applications"), dirs_home().join("Applications")]
        .into_iter()
        .map(|d| d.join(&bundle))
        .find(|p| p.is_dir())
}

#[cfg(target_os = "windows")]
fn installed_path(track: Track) -> Option<PathBuf> {
    let local = PathBuf::from(std::env::var("LOCALAPPDATA").ok()?);
    let dir = local.join(track.name());
    dir.join(windows_exe(track)).is_file().then_some(dir)
}

#[cfg(target_os = "windows")]
fn windows_exe(track: Track) -> &'static str {
    match track {
        Track::Stable => "stroke.exe",
        Track::Nightly => "stroke-nightly.exe",
    }
}

#[cfg(target_os = "linux")]
fn installed_path(track: Track) -> Option<PathBuf> {
    let bin = match track {
        Track::Stable => "stroke",
        Track::Nightly => "stroke-nightly",
    };
    [dirs_home().join(".local/bin"), PathBuf::from("/usr/bin"), PathBuf::from("/usr/local/bin"), PathBuf::from("/opt/stroke")]
        .into_iter()
        .map(|d| d.join(bin))
        .find(|p| p.is_file())
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn installed_path(_track: Track) -> Option<PathBuf> {
    None
}

#[cfg(target_os = "macos")]
async fn install_into(track: Track, tmp: &std::path::Path) -> Result<String, String> {
    // The updater's own .app.tar.gz: no disk image to mount, just unpack.
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x64" };
    let archive = tmp.join("app.tar.gz");
    let asset = match track {
        Track::Stable => format!("stroke_{arch}.app.tar.gz"),
        Track::Nightly => format!("stroke-nightly_{arch}.app.tar.gz"),
    };
    download(track, &asset, &archive).await?;
    let system = PathBuf::from("/Applications");
    let dest_dir = if tokio::fs::metadata(&system).await.map(|m| !m.permissions().readonly()).unwrap_or(false) {
        system
    } else {
        dirs_home().join("Applications")
    };
    tokio::fs::create_dir_all(&dest_dir).await.map_err(|e| e.to_string())?;
    let out = tokio::process::Command::new("tar")
        .arg("-xzf").arg(&archive).arg("-C").arg(&dest_dir)
        .output().await.map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("Couldn't unpack {}: {}", track.name(), String::from_utf8_lossy(&out.stderr).trim()));
    }
    let app = dest_dir.join(format!("{}.app", track.name()));
    // Downloaded by the app itself, so no quarantine flag to clear, but a
    // stale one from a manual install would block the launch.
    let _ = tokio::process::Command::new("xattr").arg("-cr").arg(&app).output().await;
    Ok(app.display().to_string())
}

#[cfg(target_os = "windows")]
async fn install_into(track: Track, tmp: &std::path::Path) -> Result<String, String> {
    let exe = tmp.join("stroke-setup.exe");
    let asset = match track {
        Track::Stable => format!("stroke_{}_x64-setup.exe", stable_version().await?),
        Track::Nightly => "stroke-nightly_x64-setup.exe".to_string(),
    };
    download(track, &asset, &exe).await?;
    // Per-user NSIS installer: /S installs without the wizard or admin.
    let mut cmd = tokio::process::Command::new(&exe);
    cmd.arg("/S");
    crate::proc::quiet(&mut cmd);
    let status = cmd.status().await.map_err(|e| format!("Couldn't run the installer: {e}"))?;
    if !status.success() {
        return Err(format!("The installer exited with {status}."));
    }
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
    Ok(PathBuf::from(local).join(track.name()).display().to_string())
}

#[cfg(target_os = "linux")]
async fn install_into(track: Track, tmp: &std::path::Path) -> Result<String, String> {
    use std::os::unix::fs::PermissionsExt;
    let _ = tmp;
    // The AppImage needs no root and updates itself.
    let home = dirs_home();
    let bin_dir = home.join(".local/bin");
    let app_dir = home.join(".local/share/applications");
    tokio::fs::create_dir_all(&bin_dir).await.map_err(|e| e.to_string())?;
    tokio::fs::create_dir_all(&app_dir).await.map_err(|e| e.to_string())?;
    let (id, asset, comment) = match track {
        Track::Stable => ("stroke", format!("stroke_{}_amd64.AppImage", stable_version().await?), "Database client"),
        Track::Nightly => ("stroke-nightly", "stroke-nightly_amd64.AppImage".to_string(), "Database client (nightly build)"),
    };
    let bin = bin_dir.join(id);
    download(track, &asset, &bin).await?;
    tokio::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).await.map_err(|e| e.to_string())?;
    let desktop = format!(
        "[Desktop Entry]\nName={}\nComment={comment}\nExec={}\nIcon={id}\nTerminal=false\nType=Application\nCategories=Development;Database;\nStartupWMClass={id}\n",
        track.name(),
        bin.display()
    );
    tokio::fs::write(app_dir.join(format!("{id}.desktop")), desktop).await.map_err(|e| e.to_string())?;
    Ok(bin.display().to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
async fn install_into(track: Track, _tmp: &std::path::Path) -> Result<String, String> {
    Err(format!("{} isn't built for this system.", track.name()))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn dirs_home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

fn open_app(track: Track, path: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let _ = track;
        let mut c = std::process::Command::new("open");
        c.arg(path);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let exe = PathBuf::from(path).join(windows_exe(track));
        let mut c = std::process::Command::new(exe);
        crate::proc::quiet_std(&mut c);
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut cmd = {
        let _ = track;
        std::process::Command::new(path)
    };
    cmd.spawn().map(|_| ()).map_err(|e| format!("Couldn't open {}: {e}", track.name()))
}
