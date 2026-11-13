//! External plugins: discovery, validation, install and removal.
//!
//! A plugin is a folder under `<app_data_dir>/plugins/<id>/` holding a
//! `manifest.json` and one JavaScript entry file. This module never executes
//! any of it - it reads the manifest, decides whether the folder is a plugin at
//! all, and hands the source up to the frontend, which runs it inside a Web
//! Worker with the network globals removed.
//!
//! Everything here is deliberately suspicious of what it reads. A manifest is a
//! file a stranger wrote: the id has to be a plain slug because it becomes a
//! folder name, the entry has to be a bare filename because it is joined onto a
//! path, sizes are capped because the source is read into memory, and every
//! resolved path is checked to still be inside the plugins directory.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Component, Path, PathBuf};
use tauri::Manager;

/// Plugin API generation this build implements. A plugin declaring anything else
/// is listed with an error rather than loaded, so an older app never runs a
/// manifest written against a contract it does not have.
pub const PLUGIN_API_VERSION: u32 = 1;

/// Largest entry file we will read. Formatters are small; a megabyte is already
/// a hundred times more than the built-ins.
const MAX_SOURCE_BYTES: u64 = 1024 * 1024;
/// Largest folder we will install, counting every file in it.
const MAX_PLUGIN_BYTES: u64 = 4 * 1024 * 1024;
/// Files allowed inside a plugin folder. Anything else is refused at install.
const ALLOWED_EXTENSIONS: [&str; 6] = ["js", "mjs", "json", "md", "txt", "svg"];
/// Hooks this version knows how to run.
const KNOWN_KINDS: [&str; 1] = ["formatter"];
/// Permissions this version knows how to *enforce*. Nothing else is accepted:
/// listing a permission the host cannot police would be a promise it cannot
/// keep. `cells:read` is the only capability a formatter needs - the values of
/// the columns it says it applies to.
const KNOWN_PERMISSIONS: [&str; 1] = ["cells:read"];

/// What the manifest is allowed to say.
#[derive(Debug, Deserialize)]
struct RawManifest {
    id: String,
    name: String,
    version: String,
    #[serde(rename = "apiVersion")]
    api_version: u32,
    kind: String,
    entry: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    homepage: String,
    #[serde(default)]
    permissions: Vec<String>,
}

/// One plugin folder as the frontend sees it. A folder that fails validation is
/// still returned, with `error` set and `loadable` false, so the Extensions
/// panel can show why instead of silently ignoring it.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExternalPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub kind: String,
    pub description: String,
    pub author: String,
    pub homepage: String,
    pub permissions: Vec<String>,
    /// Absolute path of the plugin folder.
    pub dir: String,
    /// Entry filename, relative to `dir`.
    pub entry: String,
    /// SHA-256 of the entry file, so the UI can show that code changed on disk.
    pub source_hash: String,
    pub bytes: u64,
    pub loadable: bool,
    pub error: String,
}

fn plugins_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data directory: {e}"))?
        .join("plugins");
    fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    Ok(dir)
}

/// A plugin id is a folder name, so it may only be a lowercase slug. This is the
/// check that stops `../../etc` from ever becoming a path.
fn valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (2..=48).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.contains("--")
        && !id.ends_with('-')
}

/// An entry must be a bare filename with a JavaScript extension: no directory
/// components, no traversal, nothing that resolves outside the folder.
fn valid_entry(entry: &str) -> bool {
    let p = Path::new(entry);
    p.components().count() == 1
        && matches!(p.components().next(), Some(Component::Normal(_)))
        && matches!(
            p.extension().and_then(|e| e.to_str()),
            Some("js") | Some("mjs")
        )
}

/// True when `path` really sits inside `root` after both are canonicalized.
/// Guards every destructive operation: a symlinked plugin folder must not let a
/// remove escape the plugins directory.
fn inside(root: &Path, path: &Path) -> bool {
    match (root.canonicalize(), path.canonicalize()) {
        (Ok(r), Ok(p)) => p.starts_with(r),
        _ => false,
    }
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            total = total.saturating_add(dir_size(&entry.path()));
        } else {
            total = total.saturating_add(meta.len());
        }
    }
    total
}

/// Read and validate one plugin folder. `Err` means "not a plugin folder at
/// all" (no manifest, unreadable); a validation failure comes back as an
/// `ExternalPlugin` with `loadable: false`.
fn read_plugin(dir: &Path) -> Result<ExternalPlugin, String> {
    let manifest_path = dir.join("manifest.json");
    let raw = fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
    let folder = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let mut plugin = ExternalPlugin {
        id: folder.clone(),
        name: folder.clone(),
        version: String::new(),
        api_version: 0,
        kind: String::new(),
        description: String::new(),
        author: String::new(),
        homepage: String::new(),
        permissions: Vec::new(),
        dir: dir.to_string_lossy().to_string(),
        entry: String::new(),
        source_hash: String::new(),
        bytes: dir_size(dir),
        loadable: false,
        error: String::new(),
    };

    let m: RawManifest = match serde_json::from_str(&raw) {
        Ok(m) => m,
        Err(e) => {
            plugin.error = format!("manifest.json is not valid: {e}");
            return Ok(plugin);
        }
    };

    plugin.id = m.id.clone();
    plugin.name = if m.name.trim().is_empty() {
        m.id.clone()
    } else {
        m.name.trim().to_string()
    };
    plugin.version = m.version.clone();
    plugin.api_version = m.api_version;
    plugin.kind = m.kind.clone();
    plugin.description = m.description.clone();
    plugin.author = m.author.clone();
    plugin.homepage = m.homepage.clone();
    plugin.permissions = m.permissions.clone();
    plugin.entry = m.entry.clone();

    // Validation, most-fundamental first. One reason is reported, because the
    // first one is the one worth fixing.
    if !valid_id(&m.id) {
        plugin.error = "id must be 2-48 characters of lowercase letters, digits and single hyphens"
            .to_string();
        return Ok(plugin);
    }
    if m.id != folder {
        plugin.error = format!("id \"{}\" does not match its folder \"{folder}\"", m.id);
        return Ok(plugin);
    }
    if m.api_version != PLUGIN_API_VERSION {
        plugin.error = format!(
            "needs plugin API {} and this build implements {PLUGIN_API_VERSION}",
            m.api_version
        );
        return Ok(plugin);
    }
    if !KNOWN_KINDS.contains(&m.kind.as_str()) {
        plugin.error = format!(
            "kind \"{}\" is not one this build runs ({})",
            m.kind,
            KNOWN_KINDS.join(", ")
        );
        return Ok(plugin);
    }
    if !valid_entry(&m.entry) {
        plugin.error = "entry must be a .js file in the plugin folder, with no path".to_string();
        return Ok(plugin);
    }
    if let Some(bad) = m
        .permissions
        .iter()
        .find(|p| !KNOWN_PERMISSIONS.contains(&p.as_str()))
    {
        plugin.error = format!("permission \"{bad}\" is not one this build can enforce");
        return Ok(plugin);
    }

    let entry_path = dir.join(&m.entry);
    let meta = match fs::metadata(&entry_path) {
        Ok(meta) => meta,
        Err(_) => {
            plugin.error = format!("entry file \"{}\" is missing", m.entry);
            return Ok(plugin);
        }
    };
    if meta.len() > MAX_SOURCE_BYTES {
        plugin.error = format!(
            "entry file is {} bytes, over the {MAX_SOURCE_BYTES} byte limit",
            meta.len()
        );
        return Ok(plugin);
    }
    let source = match fs::read(&entry_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            plugin.error = format!("entry file could not be read: {e}");
            return Ok(plugin);
        }
    };
    plugin.source_hash = hex::encode(Sha256::digest(&source));
    plugin.loadable = true;
    Ok(plugin)
}

/// Every plugin folder found, sorted by name. Unreadable folders are skipped;
/// invalid ones are included with their error.
#[tauri::command(async)]
pub fn plugins_list(app: tauri::AppHandle) -> Result<Vec<ExternalPlugin>, String> {
    let root = plugins_root(&app)?;
    let mut out = Vec::new();
    for entry in fs::read_dir(&root).map_err(|e| e.to_string())?.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        if let Ok(plugin) = read_plugin(&entry.path()) {
            out.push(plugin);
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

/// The entry file's source, for the frontend to hand to a Worker. Refuses
/// anything that does not validate, so unvalidated code never reaches the host.
#[tauri::command(async)]
pub fn plugins_read_source(app: tauri::AppHandle, id: String) -> Result<String, String> {
    if !valid_id(&id) {
        return Err("not a valid plugin id".into());
    }
    let root = plugins_root(&app)?;
    let dir = root.join(&id);
    if !inside(&root, &dir) {
        return Err("plugin folder is outside the plugins directory".into());
    }
    let plugin = read_plugin(&dir).map_err(|e| format!("{id}: {e}"))?;
    if !plugin.loadable {
        return Err(plugin.error);
    }
    fs::read_to_string(dir.join(&plugin.entry)).map_err(|e| e.to_string())
}

/// Copy a folder into the plugins directory. The source is validated *before*
/// anything is written, and only allowed file types are copied, so installing
/// cannot drop an executable into the app's data directory.
#[tauri::command(async)]
pub fn plugins_install(app: tauri::AppHandle, path: String) -> Result<ExternalPlugin, String> {
    let src = PathBuf::from(&path);
    if !src.is_dir() {
        return Err("pick the folder holding manifest.json".into());
    }
    let candidate = read_plugin(&src).map_err(|_| {
        "no manifest.json in that folder - pick the folder holding it".to_string()
    })?;
    if !candidate.loadable {
        return Err(candidate.error);
    }
    let size = dir_size(&src);
    if size > MAX_PLUGIN_BYTES {
        return Err(format!(
            "the folder is {size} bytes, over the {MAX_PLUGIN_BYTES} byte limit"
        ));
    }

    let root = plugins_root(&app)?;
    let dest = root.join(&candidate.id);
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| format!("could not replace the existing copy: {e}"))?;
    }
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    copy_allowed(&src, &dest)?;

    let installed = read_plugin(&dest).map_err(|e| e)?;
    if !installed.loadable {
        // Never leave a half-copied folder behind to be listed as broken.
        let _ = fs::remove_dir_all(&dest);
        return Err(installed.error);
    }
    Ok(installed)
}

/// Copy one level of a plugin folder, skipping file types that have no business
/// being there. Deliberately not recursive: a plugin is a manifest, an entry
/// file and a readme, and a nested tree is a sign of a packed node_modules.
fn copy_allowed(src: &Path, dest: &Path) -> Result<(), String> {
    for entry in fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        if !meta.is_file() {
            continue;
        }
        let name = entry.file_name();
        let ext = Path::new(&name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }
        fs::copy(entry.path(), dest.join(&name)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Delete an installed plugin folder. Only ever inside the plugins directory.
#[tauri::command(async)]
pub fn plugins_uninstall(app: tauri::AppHandle, id: String) -> Result<(), String> {
    if !valid_id(&id) {
        return Err("not a valid plugin id".into());
    }
    let root = plugins_root(&app)?;
    let dir = root.join(&id);
    if !dir.exists() {
        return Ok(());
    }
    if !inside(&root, &dir) {
        return Err("plugin folder is outside the plugins directory".into());
    }
    fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

/// Absolute path of the plugins directory, for "show me where these live".
#[tauri::command(async)]
pub fn plugins_dir(app: tauri::AppHandle) -> Result<String, String> {
    Ok(plugins_root(&app)?.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_folder_safe_slugs() {
        assert!(valid_id("status-badge"));
        assert!(valid_id("ab"));
        assert!(!valid_id("a"));
        assert!(!valid_id("Status"));
        assert!(!valid_id("../etc"));
        assert!(!valid_id("with space"));
        assert!(!valid_id("double--hyphen"));
        assert!(!valid_id("trailing-"));
        assert!(!valid_id("1leading"));
    }

    #[test]
    fn entries_are_bare_js_filenames() {
        assert!(valid_entry("main.js"));
        assert!(valid_entry("index.mjs"));
        assert!(!valid_entry("src/main.js"));
        assert!(!valid_entry("../main.js"));
        assert!(!valid_entry("main.ts"));
        assert!(!valid_entry("main"));
    }
}
