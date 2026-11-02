//! How much a SQL script is allowed to do, for the agent access levels:
//! read, read & write, modify schema, dangerous. The MCP server refuses a
//! script above the session's level, so the limit holds whatever the model
//! decides to send.

/// Levels in increasing order of what they allow.
pub const READ: u8 = 0;
pub const WRITE: u8 = 1;
pub const MODIFY: u8 = 2;
pub const DANGEROUS: u8 = 3;

pub fn level_name(level: u8) -> &'static str {
    match level {
        READ => "Read only",
        WRITE => "Read & write",
        MODIFY => "Modify schema",
        _ => "Dangerous",
    }
}

/// The level a whole script needs: the highest of its statements.
pub fn script_level(sql: &str) -> u8 {
    split_statements(sql)
        .iter()
        .map(|s| statement_level(s))
        .max()
        .unwrap_or(READ)
}

/// Upper-cased words of a statement, with comments and string literals gone.
fn words(stmt: &str) -> Vec<String> {
    stmt.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .map(|w| w.to_ascii_uppercase())
        .collect()
}

fn statement_level(stmt: &str) -> u8 {
    let w = words(stmt);
    let Some(first) = w.first() else { return READ };
    let has = |k: &str| w.iter().any(|x| x == k);
    match first.as_str() {
        "SELECT" | "SHOW" | "DESCRIBE" | "DESC" | "VALUES" | "TABLE" | "PRAGMA" => READ,
        "BEGIN" | "START" | "COMMIT" | "END" | "ROLLBACK" | "SAVEPOINT" | "RELEASE" => READ,
        // EXPLAIN only runs the statement with ANALYZE; then it needs what that does.
        "EXPLAIN" => {
            if has("ANALYZE") {
                let rest = w.iter().skip(1).skip_while(|x| matches!(x.as_str(), "ANALYZE" | "VERBOSE" | "BUFFERS" | "COSTS" | "TIMING" | "SETTINGS" | "WAL" | "FORMAT" | "TEXT" | "JSON" | "YAML" | "XML" | "TRUE" | "FALSE" | "ON" | "OFF"));
                let inner: Vec<String> = rest.cloned().collect();
                statement_level(&inner.join(" "))
            } else {
                READ
            }
        }
        // A CTE can end in, or contain, a write.
        "WITH" => {
            if has("DELETE") || has("UPDATE") {
                if has("WHERE") { WRITE } else { DANGEROUS }
            } else if has("INSERT") || has("MERGE") {
                WRITE
            } else {
                READ
            }
        }
        "INSERT" | "UPSERT" | "REPLACE" | "MERGE" => WRITE,
        // Without a WHERE every row goes.
        "UPDATE" | "DELETE" => if has("WHERE") { WRITE } else { DANGEROUS },
        "ALTER" => if has("DROP") || has("RENAME") { DANGEROUS } else { MODIFY },
        "CREATE" | "COMMENT" | "GRANT" | "REVOKE" | "VACUUM" | "ANALYZE" | "REINDEX"
        | "CLUSTER" | "REFRESH" | "ATTACH" => MODIFY,
        "DROP" | "TRUNCATE" | "DETACH" => DANGEROUS,
        // Unknown statements are treated as the worst case.
        _ => DANGEROUS,
    }
}

/// Split on `;` outside quotes and comments, dropping the comments and the
/// contents of string literals so their words cannot change the verdict.
fn split_statements(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            '-' if next == Some('-') => {
                while i < chars.len() && chars[i] != '\n' { i += 1; }
                cur.push(' ');
            }
            '/' if next == Some('*') => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') { i += 1; }
                i += 1;
                cur.push(' ');
            }
            '\'' | '"' | '`' => {
                // Quoted identifiers keep their text; string literals do not.
                let q = c;
                i += 1;
                let mut body = String::new();
                while i < chars.len() {
                    if chars[i] == q {
                        if chars.get(i + 1) == Some(&q) { body.push(q); i += 2; continue; }
                        break;
                    }
                    body.push(chars[i]);
                    i += 1;
                }
                if q != '\'' { cur.push_str(&body); }
                cur.push(' ');
            }
            '$' => {
                // Postgres dollar quoting: $$...$$ or $tag$...$tag$.
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') { j += 1; }
                if chars.get(j) == Some(&'$') {
                    let tag: String = chars[i..=j].iter().collect();
                    let rest: String = chars[j + 1..].iter().collect();
                    match rest.find(&tag) {
                        Some(end) => i = j + 1 + rest[..end].chars().count() + tag.chars().count() - 1,
                        None => i = chars.len(),
                    }
                    cur.push(' ');
                } else {
                    cur.push(c);
                }
            }
            ';' => {
                if !cur.trim().is_empty() { out.push(std::mem::take(&mut cur)); }
                cur.clear();
            }
            _ => cur.push(c),
        }
        i += 1;
    }
    if !cur.trim().is_empty() { out.push(cur); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads() {
        assert_eq!(script_level("SELECT * FROM users"), READ);
        assert_eq!(script_level("  -- why\n/* x */ select 1"), READ);
        assert_eq!(script_level("WITH t AS (SELECT 1) SELECT * FROM t"), READ);
        assert_eq!(script_level("EXPLAIN DELETE FROM users"), READ);
        assert_eq!(script_level("SELECT 'DROP TABLE users'"), READ);
    }

    #[test]
    fn writes() {
        assert_eq!(script_level("INSERT INTO t VALUES (1)"), WRITE);
        assert_eq!(script_level("UPDATE t SET a = 1 WHERE id = 2"), WRITE);
        assert_eq!(script_level("DELETE FROM t WHERE id = 2"), WRITE);
        assert_eq!(script_level("WITH d AS (DELETE FROM t WHERE x RETURNING *) SELECT * FROM d"), WRITE);
    }

    #[test]
    fn schema_changes() {
        assert_eq!(script_level("CREATE INDEX i ON t (a)"), MODIFY);
        assert_eq!(script_level("ALTER TABLE t ADD COLUMN b int"), MODIFY);
    }

    #[test]
    fn dangerous() {
        assert_eq!(script_level("DROP TABLE t"), DANGEROUS);
        assert_eq!(script_level("TRUNCATE t"), DANGEROUS);
        assert_eq!(script_level("DELETE FROM t"), DANGEROUS);
        assert_eq!(script_level("UPDATE t SET a = 1"), DANGEROUS);
        assert_eq!(script_level("ALTER TABLE t DROP COLUMN b"), DANGEROUS);
        assert_eq!(script_level("EXPLAIN ANALYZE DELETE FROM t"), DANGEROUS);
        assert_eq!(script_level("FROBNICATE t"), DANGEROUS);
    }

    #[test]
    fn the_worst_statement_wins() {
        assert_eq!(script_level("SELECT 1; DROP TABLE t"), DANGEROUS);
        assert_eq!(script_level("SELECT ';'; INSERT INTO t VALUES (1)"), WRITE);
        assert_eq!(script_level("SELECT $$; DROP TABLE t$$"), READ);
    }
}
