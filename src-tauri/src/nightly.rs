//! "Try Stroke Nightly" from a stable build. Nightly is its own app (its own
//! name, identifier and data folder; see tauri.nightly.conf.json), so this
//! installs it beside Stroke rather than switching this install's channel:
//! the stable app, its settings and connections stay exactly as they are.

use std::path::PathBuf;

const BASE: &str = "https://github.com/stroke-app/stroke/releases/download/nightly";

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("stroke-nightly-installer")
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .unwrap_or_default()
}

async fn download(asset: &str, to: &std::path::Path) -> Result<(), String> {
    let resp = client()
        .get(format!("{BASE}/{asset}"))
        .send()
        .await
        .map_err(|e| format!("Couldn't download Stroke Nightly: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Couldn't download Stroke Nightly (HTTP {}).", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    tokio::fs::write(to, &bytes).await.map_err(|e| format!("Couldn't save the download: {e}"))
}

/// Download and install Stroke Nightly; returns where it was installed.
#[tauri::command]
pub async fn nightly_install() -> Result<String, String> {
    let tmp = std::env::temp_dir().join(format!("stroke-nightly-{}", std::process::id()));
    tokio::fs::create_dir_all(&tmp).await.map_err(|e| e.to_string())?;
    let result = install_into(&tmp).await;
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    result
}

#[cfg(target_os = "macos")]
async fn install_into(tmp: &std::path::Path) -> Result<String, String> {
    // The updater's own .app.tar.gz: no disk image to mount, just unpack.
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x64" };
    let archive = tmp.join("nightly.app.tar.gz");
    download(&format!("stroke-nightly_{arch}.app.tar.gz"), &archive).await?;
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
        return Err(format!("Couldn't unpack Stroke Nightly: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let app = dest_dir.join("Stroke Nightly.app");
    // Downloaded by the app itself, so no quarantine flag to clear, but a
    // stale one from a manual install would block the launch.
    let _ = tokio::process::Command::new("xattr").arg("-cr").arg(&app).output().await;
    Ok(app.display().to_string())
}

#[cfg(target_os = "windows")]
async fn install_into(tmp: &std::path::Path) -> Result<String, String> {
    let exe = tmp.join("stroke-nightly-setup.exe");
    download("stroke-nightly_x64-setup.exe", &exe).await?;
    // Per-user NSIS installer: /S installs without the wizard or admin.
    let mut cmd = tokio::process::Command::new(&exe);
    cmd.arg("/S");
    crate::proc::quiet(&mut cmd);
    let status = cmd.status().await.map_err(|e| format!("Couldn't run the installer: {e}"))?;
    if !status.success() {
        return Err(format!("The installer exited with {status}."));
    }
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
    Ok(PathBuf::from(local).join("Stroke Nightly").display().to_string())
}

#[cfg(target_os = "linux")]
async fn install_into(tmp: &std::path::Path) -> Result<String, String> {
    use std::os::unix::fs::PermissionsExt;
    let _ = tmp;
    // The AppImage needs no root and updates itself, like the stable one.
    let home = dirs_home();
    let bin_dir = home.join(".local/bin");
    let app_dir = home.join(".local/share/applications");
    tokio::fs::create_dir_all(&bin_dir).await.map_err(|e| e.to_string())?;
    tokio::fs::create_dir_all(&app_dir).await.map_err(|e| e.to_string())?;
    let bin = bin_dir.join("stroke-nightly");
    download("stroke-nightly_amd64.AppImage", &bin).await?;
    tokio::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).await.map_err(|e| e.to_string())?;
    let desktop = format!(
        "[Desktop Entry]\nName=Stroke Nightly\nComment=Database client (nightly build)\nExec={}\nIcon=stroke-nightly\nTerminal=false\nType=Application\nCategories=Development;Database;\nStartupWMClass=stroke-nightly\n",
        bin.display()
    );
    tokio::fs::write(app_dir.join("stroke-nightly.desktop"), desktop).await.map_err(|e| e.to_string())?;
    Ok(bin.display().to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
async fn install_into(_tmp: &std::path::Path) -> Result<String, String> {
    Err("Stroke Nightly isn't built for this system.".into())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn dirs_home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Start the installed Nightly.
#[tauri::command]
pub async fn nightly_open(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = tokio::process::Command::new("open");
        c.arg(&path);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let exe = PathBuf::from(&path).join("stroke-nightly.exe");
        let mut c = tokio::process::Command::new(exe);
        crate::proc::quiet(&mut c);
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut cmd = tokio::process::Command::new(&path);
    cmd.spawn().map(|_| ()).map_err(|e| format!("Couldn't open Stroke Nightly: {e}"))
}
