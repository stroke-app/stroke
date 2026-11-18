// The migrations a Prisma or Drizzle project has on disk, for the Migrations
// page. What ran comes from the database (`_prisma_migrations`,
// `__drizzle_migrations`), read by the page with plain SELECTs; this side only
// reads the project folder, so the page can show what's still pending and spot
// a migration whose file changed after it ran.
//
// Prisma: `<dir>/migrations/<name>/migration.sql`, its checksum the SHA-256 of
// the file (what Prisma stores in `_prisma_migrations.checksum`).
// Drizzle: `<out>/meta/_journal.json` lists the migrations in order ({ idx,
// tag, when }); `<out>/<tag>.sql` is the SQL and its SHA-256 is the `hash`
// Drizzle stores in `__drizzle_migrations`.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MigrationFile {
    /// Prisma: the folder name. Drizzle: the journal's `tag`.
    pub name: String,
    pub sql: String,
    /// SHA-256 of the SQL, hex.
    pub checksum: String,
    /// Drizzle's journal `when` (ms); None for Prisma.
    pub when: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationProject {
    /// prisma | drizzle
    pub tool: String,
    /// The folder the migrations were read from.
    pub dir: String,
    pub files: Vec<MigrationFile>,
}

const MAX_SQL: usize = 2 * 1024 * 1024;

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn read_sql(path: &Path) -> Option<(String, String)> {
    let bytes = std::fs::read(path).ok()?;
    let checksum = sha256_hex(&bytes);
    let mut sql = String::from_utf8_lossy(&bytes).into_owned();
    if sql.len() > MAX_SQL {
        let mut cut = MAX_SQL;
        while !sql.is_char_boundary(cut) {
            cut -= 1;
        }
        sql.truncate(cut);
        sql.push_str("\n-- (cut: the file is larger than 2 MB)");
    }
    Some((sql, checksum))
}

/// `migrations/` folders that hold Prisma migrations (a `migration_lock.toml`,
/// or subfolders with a `migration.sql`).
fn is_prisma_dir(dir: &Path) -> bool {
    if dir.join("migration_lock.toml").is_file() {
        return true;
    }
    std::fs::read_dir(dir)
        .map(|rd| rd.flatten().any(|e| e.path().join("migration.sql").is_file()))
        .unwrap_or(false)
}

/// Drizzle output folders: they hold `meta/_journal.json`.
fn is_drizzle_dir(dir: &Path) -> bool {
    dir.join("meta").join("_journal.json").is_file()
}

/// Folders worth looking into for a project root, a few levels down, skipping
/// the ones that are never migrations.
fn walk(root: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    out.push(root.to_path_buf());
    if depth == 0 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(root) else { return };
    let mut dirs: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            !n.starts_with('.') && !matches!(n, "node_modules" | "target" | "dist" | "build" | ".next" | "coverage" | "vendor")
        })
        .collect();
    dirs.sort();
    for d in dirs {
        walk(&d, depth - 1, out);
    }
}

/// Find the migrations of `tool` (or either) in a project folder: the folder
/// itself, or one up to four levels below it.
pub fn find(root: &Path, tool: Option<&str>) -> Option<(String, PathBuf)> {
    let mut dirs = Vec::new();
    walk(root, 4, &mut dirs);
    let want_prisma = tool.is_none_or(|t| t == "prisma");
    let want_drizzle = tool.is_none_or(|t| t == "drizzle");
    // Prefer the shallowest match: a monorepo's app folder over a fixture deep down.
    dirs.sort_by_key(|d| d.components().count());
    for d in &dirs {
        if want_drizzle && is_drizzle_dir(d) {
            return Some(("drizzle".into(), d.clone()));
        }
        if want_prisma && d.file_name().is_some_and(|n| n == "migrations") && is_prisma_dir(d) {
            return Some(("prisma".into(), d.clone()));
        }
    }
    None
}

pub fn read_prisma(dir: &Path) -> Vec<MigrationFile> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().join("migration.sql").is_file())
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .collect();
    // Prisma applies them in folder-name order (timestamp prefixes).
    names.sort();
    names
        .into_iter()
        .filter_map(|name| {
            let (sql, checksum) = read_sql(&dir.join(&name).join("migration.sql"))?;
            Some(MigrationFile { name, sql, checksum, when: None })
        })
        .collect()
}

pub fn read_drizzle(dir: &Path) -> Result<Vec<MigrationFile>, String> {
    let journal = std::fs::read_to_string(dir.join("meta").join("_journal.json"))
        .map_err(|e| format!("Could not read meta/_journal.json: {e}"))?;
    let v: serde_json::Value = serde_json::from_str(&journal).map_err(|e| format!("meta/_journal.json isn't valid JSON: {e}"))?;
    let mut entries: Vec<(i64, String, Option<i64>)> = v
        .get("entries")
        .and_then(|e| e.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let tag = e.get("tag")?.as_str()?.to_string();
                    let idx = e.get("idx").and_then(|i| i.as_i64()).unwrap_or(0);
                    Some((idx, tag, e.get("when").and_then(|w| w.as_i64())))
                })
                .collect()
        })
        .unwrap_or_default();
    entries.sort_by_key(|e| e.0);
    Ok(entries
        .into_iter()
        .map(|(_, tag, when)| {
            let (sql, checksum) = read_sql(&dir.join(format!("{tag}.sql"))).unwrap_or_else(|| ("-- The file for this migration is missing".into(), String::new()));
            MigrationFile { name: tag, sql, checksum, when }
        })
        .collect())
}

/// The migrations in a project folder. `tool` narrows the search when a
/// project has both.
#[tauri::command]
pub async fn migrations_project(dir: String, tool: Option<String>) -> Result<MigrationProject, String> {
    tokio::task::spawn_blocking(move || {
        let root = PathBuf::from(&dir);
        if !root.is_dir() {
            return Err(format!("{dir} isn't a folder"));
        }
        let (tool, found) = find(&root, tool.as_deref()).ok_or_else(|| {
            "No Prisma or Drizzle migrations in this folder. Pick the project folder (the one with prisma/ or drizzle.config).".to_string()
        })?;
        let files = if tool == "prisma" { read_prisma(&found) } else { read_drizzle(&found)? };
        Ok(MigrationProject { tool, dir: found.to_string_lossy().into_owned(), files })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("stroke-mig-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn prisma_migrations_are_found_and_read_in_order() {
        let root = tmp("prisma");
        let m = root.join("app").join("prisma").join("migrations");
        for (n, sql) in [("20240102_add_posts", "CREATE TABLE posts();"), ("20240101_init", "CREATE TABLE users();")] {
            std::fs::create_dir_all(m.join(n)).unwrap();
            std::fs::write(m.join(n).join("migration.sql"), sql).unwrap();
        }
        std::fs::write(m.join("migration_lock.toml"), "provider = \"postgresql\"").unwrap();
        let (tool, dir) = find(&root, None).unwrap();
        assert_eq!((tool.as_str(), dir.clone()), ("prisma", m.clone()));
        let files = read_prisma(&dir);
        assert_eq!(files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["20240101_init", "20240102_add_posts"]);
        assert_eq!(files[0].checksum, sha256_hex(b"CREATE TABLE users();"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn drizzle_journal_drives_the_order_and_hashes() {
        let root = tmp("drizzle");
        let out = root.join("drizzle");
        std::fs::create_dir_all(out.join("meta")).unwrap();
        std::fs::write(
            out.join("meta").join("_journal.json"),
            r#"{"entries":[{"idx":1,"tag":"0001_posts","when":1700000002000},{"idx":0,"tag":"0000_init","when":1700000001000},{"idx":2,"tag":"0002_gone","when":1700000003000}]}"#,
        )
        .unwrap();
        std::fs::write(out.join("0000_init.sql"), "CREATE TABLE users();").unwrap();
        std::fs::write(out.join("0001_posts.sql"), "CREATE TABLE posts();").unwrap();
        // node_modules is never searched.
        std::fs::create_dir_all(root.join("node_modules").join("x").join("meta")).unwrap();
        let (tool, dir) = find(&root, None).unwrap();
        assert_eq!((tool.as_str(), dir.clone()), ("drizzle", out.clone()));
        let files = read_drizzle(&dir).unwrap();
        assert_eq!(files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["0000_init", "0001_posts", "0002_gone"]);
        assert_eq!(files[1].when, Some(1700000002000));
        assert_eq!(files[0].checksum, sha256_hex(b"CREATE TABLE users();"));
        assert!(files[2].checksum.is_empty());
        assert!(find(&root, Some("prisma")).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
