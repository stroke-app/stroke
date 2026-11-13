//! Shared SQL string helpers used across the engine backends.
//!
//! Consolidates the identifier quoting, LIKE-pattern escaping, string-literal
//! escaping, and statement-head parsing that were previously copy-pasted in
//! nearly identical form across `mysql.rs`, `sqlite.rs`, `mssql.rs`,
//! `clickhouse.rs`, `duckdb.rs`, and `query.rs`.
//!
//! Dialect-specific *keyword sets* (e.g. MSSQL `exec`, SQLite `pragma`) stay in
//! each engine; only the mechanical, byte-for-byte identical logic lives here.

/// Lowercased first token of a statement, splitting on whitespace or `(`.
/// Used to classify a statement's kind (`select`, `insert`, …).
pub fn statement_head(sql: &str) -> String {
    sql.trim_start()
        .split(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Quote an identifier for double-quote dialects (`"id"`), doubling embedded
/// quotes. (PostgreSQL, SQLite, DuckDB, D1/LibSQL.)
pub fn quote_double(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

/// Quote an identifier with backticks (`` `id` ``), doubling embedded backticks.
/// (MySQL, ClickHouse.)
pub fn quote_backtick(ident: &str) -> String {
    format!("`{}`", ident.replace('`', "``"))
}

/// Quote an identifier with square brackets (`[id]`), doubling embedded `]`.
/// (SQL Server.)
pub fn quote_bracket(ident: &str) -> String {
    format!("[{}]", ident.replace(']', "]]"))
}

/// Escape a `LIKE` pattern using `\` as the escape character (used with
/// `ESCAPE '\'`). (MySQL, SQLite, D1.)
pub fn escape_like_backslash(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Escape a string literal by doubling single quotes (`'` → `''`). (SQL Server,
/// ANSI SQL.)
pub fn esc_single_quote(s: &str) -> String {
    s.replace('\'', "''")
}

/// Escape a string literal with backslash escapes (`\` → `\\`, `'` → `\'`).
/// (ClickHouse.)
pub fn esc_backslash_quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

// ── Oversized-cell capping ────────────────────────────────────────────────────
//
// Cells whose payload exceeds `CELL_VALUE_CAP` are replaced with a small
// sentinel object instead of being shipped to the webview whole. Multi-MB
// cells (file buffers stored in jsonb/text/blob columns) otherwise freeze the
// app: the IPC JSON payload, the webview-side parse, and the grid's per-cell
// stringify all scale with cell size (GitHub issue: "App freezes").
// The frontend detects the sentinel via `__strokeOversize` (see
// `src/lib/cell-value.js`), renders the preview, and blocks editing so a
// truncated value can never be written back.

/// Per-cell payload cap, in bytes of serialized text.
pub const CELL_VALUE_CAP: usize = 256 * 1024;

/// How much of an oversized cell ships as a preview (grid text + JSON lightbox).
pub const CELL_PREVIEW_BYTES: usize = 16 * 1024;

/// Build the oversize sentinel for a cell too large to ship. `body` is the
/// value's UTF-8 text (JSON text, plain text, or hex for blobs).
pub fn oversize_cell(data_type: &str, total_bytes: usize, body: &[u8]) -> serde_json::Value {
    let end = CELL_PREVIEW_BYTES.min(body.len());
    let preview = String::from_utf8_lossy(&body[..end]);
    serde_json::json!({
        "__strokeOversize": true,
        "dataType": data_type,
        "bytes": total_bytes,
        "preview": preview,
    })
}

/// Cap an already-decoded JSON value (engines where the raw wire bytes can't
/// be peeked cheaply). Containers are measured by serializing; oversized ones
/// collapse to the sentinel. Scalars below the cap pass through untouched.
pub fn cap_json_value(data_type: &str, v: serde_json::Value) -> serde_json::Value {
    match &v {
        serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
            let text = v.to_string();
            if text.len() > CELL_VALUE_CAP {
                return oversize_cell(data_type, text.len(), text.as_bytes());
            }
            v
        }
        serde_json::Value::String(s) if s.len() > CELL_VALUE_CAP => {
            oversize_cell(data_type, s.len(), s.as_bytes())
        }
        _ => v,
    }
}

/// Filter op for a typed SQL `WHERE` expression in the grid's filter bar
/// (`id > 200 AND name LIKE 'a%'`). Its text arrives in `RowFilter::value`.
pub const RAW_SQL_FILTER_OP: &str = "sql";

/// The SQL family a raw filter expression is checked against: which bind
/// placeholders and comment markers the engine understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawFilterDialect {
    Postgres,
    Mysql,
    Sqlite,
    Duckdb,
    /// Engines whose filters are inlined literals with no bind placeholders
    /// (SQL Server, ClickHouse, HogQL).
    Literal,
}

/// Check a typed `WHERE` expression and return it wrapped in parentheses, or
/// `None` when it is blank.
///
/// The expression is the user's own SQL against their own database - it may
/// call any function the connection may call, just as the SQL editor can. What
/// this guards is the query it is spliced into: it must stay one boolean term
/// inside `WHERE (...)`. So, outside string literals and quoted identifiers:
/// parentheses must balance (`1=1) OR (1=1` would escape the wrapper), and `;`
/// (a second statement), comments (`--`, `/*`, MySQL `#` - they would swallow
/// the LIMIT after it) and the engine's bind placeholders (they would steal the
/// query's own parameters) are refused.
///
/// Backslash escapes inside strings differ by engine and setting, so the text
/// is scanned both ways and must pass both: no reading of an ambiguous string
/// lets a quote end somewhere the check did not see.
pub fn raw_where_expr(expr: &str, dialect: RawFilterDialect) -> Result<Option<String>, String> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    scan_raw_where(trimmed, dialect, false)?;
    scan_raw_where(trimmed, dialect, true)?;
    Ok(Some(format!("({trimmed})")))
}

fn scan_raw_where(expr: &str, dialect: RawFilterDialect, backslash_escapes: bool) -> Result<(), String> {
    use RawFilterDialect::*;
    let chars: Vec<char> = expr.chars().collect();
    let mut depth = 0_i32;
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if let Some(q) = quote {
            if backslash_escapes && c == '\\' && q == '\'' {
                i += 2;
                continue;
            }
            if c == q {
                // A doubled quote is an escaped quote, not the end.
                if next == Some(q) {
                    i += 2;
                    continue;
                }
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            '\'' | '"' | '`' => quote = Some(c),
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return Err("Unbalanced parentheses in the filter: a ')' has no matching '('".into());
                }
            }
            ';' => return Err("A filter is one condition: remove the ';'".into()),
            '-' if next == Some('-') => return Err("Comments (--) aren't allowed in a filter".into()),
            '/' if next == Some('*') => return Err("Comments (/* */) aren't allowed in a filter".into()),
            '#' if dialect == Mysql => return Err("Comments (#) aren't allowed in a filter".into()),
            '?' if matches!(dialect, Mysql | Sqlite | Duckdb) => {
                return Err("'?' is a query parameter here; write the value itself".into())
            }
            '$' if matches!(dialect, Postgres | Duckdb) && next.is_some_and(|n| n.is_ascii_digit()) => {
                return Err("$1-style parameters aren't allowed in a filter; write the value itself".into())
            }
            '$' | '@' if dialect == Sqlite => {
                return Err(format!("'{c}' starts a query parameter here; write the value itself"))
            }
            ':' if dialect == Sqlite && next.is_some_and(|n| n.is_ascii_alphanumeric() || n == '_') => {
                return Err("':name' is a query parameter here; write the value itself".into())
            }
            _ => {}
        }
        i += 1;
    }
    if quote.is_some() {
        return Err("Unclosed quote in the filter".into());
    }
    if depth != 0 {
        return Err("Unbalanced parentheses in the filter: a '(' is never closed".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_head_handles_paren_and_whitespace() {
        assert_eq!(statement_head("  SELECT * FROM t"), "select");
        assert_eq!(statement_head("WITH(x) AS ..."), "with");
        assert_eq!(statement_head("insert(into"), "insert");
        assert_eq!(statement_head(""), "");
    }

    #[test]
    fn quoting_doubles_delimiters() {
        assert_eq!(quote_double(r#"a"b"#), r#""a""b""#);
        assert_eq!(quote_backtick("a`b"), "`a``b`");
        assert_eq!(quote_bracket("a]b"), "[a]]b]");
    }

    #[test]
    fn like_and_literal_escaping() {
        assert_eq!(escape_like_backslash("50%_x\\"), "50\\%\\_x\\\\");
        assert_eq!(esc_single_quote("O'Brien"), "O''Brien");
        assert_eq!(esc_backslash_quote("a'b\\c"), "a\\'b\\\\c");
    }

    #[test]
    fn small_values_pass_through_uncapped() {
        let s = serde_json::json!("hello");
        assert_eq!(cap_json_value("text", s.clone()), s);
        let n = serde_json::json!(42);
        assert_eq!(cap_json_value("int4", n.clone()), n);
        let small_obj = serde_json::json!({ "a": 1, "b": [1, 2, 3] });
        assert_eq!(cap_json_value("jsonb", small_obj.clone()), small_obj);
    }

    #[test]
    fn oversized_string_collapses_to_sentinel() {
        let big = "a".repeat(CELL_VALUE_CAP + 1_000);
        let capped = cap_json_value("text", serde_json::Value::String(big.clone()));
        assert_eq!(capped["__strokeOversize"], serde_json::json!(true));
        assert_eq!(capped["dataType"], serde_json::json!("text"));
        assert_eq!(capped["bytes"], serde_json::json!(big.len()));
        // Preview is truncated to the preview budget, not the whole payload.
        assert_eq!(
            capped["preview"].as_str().unwrap().len(),
            CELL_PREVIEW_BYTES
        );
    }

    #[test]
    fn oversized_container_collapses_to_sentinel() {
        let arr = serde_json::Value::Array(
            (0..100_000).map(|i| serde_json::json!(i)).collect(),
        );
        let capped = cap_json_value("jsonb", arr);
        assert_eq!(capped["__strokeOversize"], serde_json::json!(true));
        assert!(capped["bytes"].as_u64().unwrap() as usize > CELL_VALUE_CAP);
    }

    #[test]
    fn oversize_cell_reports_total_bytes_and_bounded_preview() {
        let body = vec![b'x'; CELL_PREVIEW_BYTES * 2];
        let cell = oversize_cell("bytea", 9_999_999, &body);
        assert_eq!(cell["bytes"], serde_json::json!(9_999_999));
        assert_eq!(cell["dataType"], serde_json::json!("bytea"));
        assert_eq!(cell["preview"].as_str().unwrap().len(), CELL_PREVIEW_BYTES);
    }

    #[test]
    fn raw_where_accepts_ordinary_conditions() {
        use RawFilterDialect::*;
        assert_eq!(raw_where_expr("  ", Postgres).unwrap(), None);
        assert_eq!(raw_where_expr("id > 200", Postgres).unwrap().as_deref(), Some("(id > 200)"));
        let ok = [
            "name LIKE 'a%' AND (age BETWEEN 1 AND 5 OR age IS NULL)",
            "note = 'it''s; -- not a comment (really'",
            r#""weird;col" = 1"#,
            "doc ->> 'k' = 'v' AND tags @> '{a}'",
            "created_at > now() - interval '1 day'",
            "x::int = 3",
        ];
        for e in ok {
            assert!(raw_where_expr(e, Postgres).is_ok(), "{e}");
        }
        assert!(raw_where_expr("`a;b` = 1 AND c = 'x''y'", Mysql).is_ok());
        // A backslash-escaped quote reads differently by engine and setting, so
        // it is refused; the doubled quote above means the same thing everywhere.
        assert!(raw_where_expr("c = 'x\\'y'", Mysql).is_err());
    }

    #[test]
    fn raw_where_refuses_escapes_from_the_wrapper() {
        use RawFilterDialect::*;
        let bad = [
            ("1=1) OR (1=1", Postgres),
            ("(id = 1", Postgres),
            ("id = 1; DROP TABLE users", Postgres),
            ("id = 1 --", Postgres),
            ("id = 1 /* x */", Postgres),
            ("id = $1", Postgres),
            ("id = 'unclosed", Postgres),
            ("id = 1 # x", Mysql),
            ("id = ?", Mysql),
            ("id = ?", Sqlite),
            ("id = :id", Sqlite),
            ("id = @id", Sqlite),
            ("id = $1", Duckdb),
            // Under MySQL's backslash escapes the string never ends; under the
            // standard reading the ')' escapes. Either way it is refused.
            (r"name = 'a\' ) OR (1=1 --'", Mysql),
            (r"name = 'a\' ) OR (1=1 --'", Postgres),
        ];
        for (e, d) in bad {
            assert!(raw_where_expr(e, d).is_err(), "{e} ({d:?}) should be refused");
        }
        // '#' is an operator in Postgres, not a comment.
        assert!(raw_where_expr("flags # 1 = 0", Postgres).is_ok());
    }
}
