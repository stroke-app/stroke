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
    /// Every `.sql` file in the folder by file name, journal or not: what
    /// `wrangler d1 migrations apply` runs and records in `d1_migrations`.
    pub sql_files: Vec<MigrationFile>,
    /// The D1 database a `wrangler.toml` / `wrangler.json(c)` next to the
    /// migrations names, for the apply command.
    pub wrangler_db: Option<String>,
}

/// The first `database_name = "x"` (TOML) or `"database_name": "x"` (JSON).
fn database_name(text: &str) -> Option<String> {
    for (i, _) in text.match_indices("database_name") {
        let rest = text[i + "database_name".len()..].trim_start_matches('"').trim_start();
        let Some(rest) = rest.strip_prefix([':', '=']) else { continue };
        let rest = rest.trim_start();
        let q = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let v: String = rest[1..].chars().take_while(|c| *c != q).collect();
        if !v.is_empty() {
            return Some(v);
        }
    }
    None
}

/// `database_name` from the first wrangler config in `from` or a folder above
/// it, up to `root`.
fn wrangler_db(root: &Path, from: &Path) -> Option<String> {
    let mut dir = Some(from);
    while let Some(d) = dir {
        for f in ["wrangler.toml", "wrangler.jsonc", "wrangler.json"] {
            if let Ok(text) = std::fs::read_to_string(d.join(f)) {
                if let Some(name) = database_name(&text) {
                    return Some(name);
                }
            }
        }
        if d == root {
            break;
        }
        dir = d.parent();
    }
    None
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

/// A plain `migrations/` folder of `.sql` files, as Wrangler (D1) keeps them.
fn is_sql_dir(dir: &Path) -> bool {
    dir.file_name().is_some_and(|n| n == "migrations")
        && std::fs::read_dir(dir)
            .map(|rd| rd.flatten().any(|e| e.path().extension().is_some_and(|x| x == "sql")))
            .unwrap_or(false)
}

/// Every `.sql` file directly in `dir`, by name; `name` keeps the `.sql`
/// (that's what `d1_migrations.name` holds).
pub fn read_sql_dir(dir: &Path) -> Vec<MigrationFile> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().is_file() && e.path().extension().is_some_and(|x| x == "sql"))
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .collect();
    names.sort();
    names
        .into_iter()
        .filter_map(|name| {
            let (sql, checksum) = read_sql(&dir.join(&name))?;
            Some(MigrationFile { name, sql, checksum, when: None })
        })
        .collect()
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
    // No journal anywhere: a Wrangler migrations folder reads like Drizzle's
    // SQL files, applied by name.
    if want_drizzle {
        if let Some(d) = dirs.iter().find(|d| is_sql_dir(d) && !is_prisma_dir(d)) {
            return Some(("drizzle".into(), d.clone()));
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
    if !is_drizzle_dir(dir) {
        // A Wrangler folder: its files are the migrations, in name order.
        return Ok(read_sql_dir(dir).into_iter().map(|f| MigrationFile { name: f.name.trim_end_matches(".sql").to_string(), ..f }).collect());
    }
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
        let sql_files = if tool == "drizzle" { read_sql_dir(&found) } else { Vec::new() };
        let wrangler_db = wrangler_db(&root, &found);
        Ok(MigrationProject { tool, dir: found.to_string_lossy().into_owned(), files, sql_files, wrangler_db })
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
        // Every .sql file, journal or not, for D1's d1_migrations.
        std::fs::write(out.join("0001b_hand_written.sql"), "SELECT 1;").unwrap();
        std::fs::write(root.join("wrangler.jsonc"), "{\n  \"d1_databases\": [{ \"database_name\": \"shop-db\", \"binding\": \"DB\" }]\n}").unwrap();
        assert_eq!(wrangler_db(&root, &dir).as_deref(), Some("shop-db"));
        std::fs::remove_file(root.join("wrangler.jsonc")).unwrap();
        std::fs::write(root.join("wrangler.toml"), "[[d1_databases]]\nbinding = \"DB\"\ndatabase_name = \"shop-db\"\n").unwrap();
        assert_eq!(wrangler_db(&root, &dir).as_deref(), Some("shop-db"));
        let all = read_sql_dir(&dir);
        assert_eq!(all.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["0000_init.sql", "0001_posts.sql", "0001b_hand_written.sql"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_wrangler_migrations_folder_reads_as_drizzle() {
        let root = tmp("wrangler");
        let m = root.join("migrations");
        std::fs::create_dir_all(&m).unwrap();
        std::fs::write(m.join("0002_b.sql"), "SELECT 2;").unwrap();
        std::fs::write(m.join("0001_a.sql"), "SELECT 1;").unwrap();
        let (tool, dir) = find(&root, None).unwrap();
        assert_eq!((tool.as_str(), dir.clone()), ("drizzle", m.clone()));
        let files = read_drizzle(&dir).unwrap();
        assert_eq!(files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["0001_a", "0002_b"]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
