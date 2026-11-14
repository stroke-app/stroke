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

/// How the connected engine reads a statement. The SQL engines share one
/// classifier; the consoles of the NoSQL engines get their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    /// Any SQL-like engine: MySQL, SQLite, D1, MSSQL, DuckDB, ClickHouse,
    /// Cassandra CQL, DynamoDB PartiQL, ...
    Sql,
    /// Postgres: dollar quoting is real there, so `$$ ... $$` is a string.
    Postgres,
    /// Oracle: `q'[...]'` quoting, and `BEGIN`/`DECLARE` open a PL/SQL block
    /// that can run anything (EXECUTE IMMEDIATE included), not a transaction.
    Oracle,
    /// The Mongo shell subset in `db::mongo`.
    Mongo,
    /// One raw Redis command.
    Redis,
    /// CouchDB and Firestore: their consoles only read.
    ReadOnlyDoc,
}

impl Dialect {
    /// From `ActiveConnection::driver()`.
    pub fn for_driver(driver: &str) -> Self {
        match driver {
            "postgres" => Self::Postgres,
            "oracle" => Self::Oracle,
            "mongodb" => Self::Mongo,
            "redis" => Self::Redis,
            "couchdb" | "firestore" => Self::ReadOnlyDoc,
            _ => Self::Sql,
        }
    }
}

/// The level a whole script needs on an unknown SQL engine: the highest of
/// its statements, under every way an engine might read the quotes and
/// comments.
#[cfg(test)]
pub fn script_level(sql: &str) -> u8 {
    script_level_in(Dialect::Sql, sql)
}

/// The level a whole script needs on this engine.
pub fn script_level_in(dialect: Dialect, sql: &str) -> u8 {
    match dialect {
        Dialect::Mongo => mongo_level(sql),
        Dialect::Redis => redis_level(sql),
        Dialect::ReadOnlyDoc => READ,
        Dialect::Sql | Dialect::Postgres => lexes(dialect)
            .into_iter()
            .flat_map(|lx| split_statements(sql, lx))
            .map(|s| statement_level(&s))
            .max()
            .unwrap_or(READ),
        // The generic readings, and the statements as Oracle itself splits
        // them (q-quotes, `/` lines, whole PL/SQL blocks); the worst wins.
        Dialect::Oracle => {
            let own = crate::db::oracle::split_statements(sql)
                .iter()
                .map(|s| oracle_statement_level(s))
                .max()
                .unwrap_or(READ);
            own.max(script_level_in(Dialect::Sql, sql))
        }
    }
}

/// A PL/SQL block can do anything; the rest reads like any SQL statement once
/// its literals are gone.
fn oracle_statement_level(stmt: &str) -> u8 {
    let bare = crate::db::oracle::strip_literals(stmt);
    match words(&bare).first().map(String::as_str) {
        Some("BEGIN") | Some("DECLARE") => DANGEROUS,
        _ => statement_level(&bare),
    }
}

/// How many statements the engine would run. Taken as the most any reading
/// of the quotes and comments finds, so a statement hidden in what one
/// reading calls a string still counts.
pub fn statement_count(dialect: Dialect, sql: &str) -> usize {
    match dialect {
        Dialect::Sql | Dialect::Postgres => lexes(dialect)
            .into_iter()
            .map(|lx| split_statements(sql, lx).len())
            .max()
            .unwrap_or(0),
        Dialect::Oracle => crate::db::oracle::split_statements(sql).len().max(statement_count(Dialect::Sql, sql)),
        _ => usize::from(!sql.trim().is_empty()),
    }
}

/// One way to read quotes and comments. Engines disagree: MySQL escapes with
/// a backslash inside strings, runs `/*! ... */` and needs a space after
/// `--`; only Postgres has dollar quoting. Each script is classified under
/// every reading that could apply and the worst verdict wins, so text that
/// one engine treats as a string or comment cannot hide a statement that
/// another runs.
#[derive(Clone, Copy)]
struct Lex {
    backslash: bool,
    dollar: bool,
    mysql: bool,
}

fn lexes(dialect: Dialect) -> Vec<Lex> {
    let mut out = Vec::new();
    for backslash in [false, true] {
        if dialect == Dialect::Postgres {
            out.push(Lex { backslash, dollar: true, mysql: false });
            continue;
        }
        for dollar in [false, true] {
            for mysql in [false, true] {
                out.push(Lex { backslash, dollar, mysql });
            }
        }
    }
    out
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
    let level = match first.as_str() {
        "SELECT" | "VALUES" | "TABLE" => hidden_level(&w[1..], false),
        "SHOW" | "DESCRIBE" | "DESC" => READ,
        "PRAGMA" => pragma_level(stmt),
        // Ending or reopening a transaction would step out of the read-only
        // one that Read only scripts run in, so these need write access.
        "BEGIN" | "START" | "COMMIT" | "END" | "ROLLBACK" | "SAVEPOINT" | "RELEASE" => WRITE,
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
            let tail = if has("DELETE") || has("UPDATE") {
                if has("WHERE") { WRITE } else { DANGEROUS }
            } else if has("INSERT") || has("MERGE") {
                WRITE
            } else {
                READ
            };
            tail.max(hidden_level(&w[1..], true))
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
    };
    level.max(call_level(stmt))
}

/// What a query that starts as a read does anyway. T-SQL needs no `;`
/// between statements, so a write can follow a SELECT in the same
/// "statement"; `INTO` creates a table or a file; `FOR UPDATE` locks rows.
/// `dml_ok` is set for a WITH, whose CTEs may legitimately hold DML.
fn hidden_level(w: &[String], dml_ok: bool) -> u8 {
    let mut level = READ;
    for (i, x) in w.iter().enumerate() {
        let prev = if i > 0 { w[i - 1].as_str() } else { "" };
        let next = w.get(i + 1).map(String::as_str).unwrap_or("");
        let l = match x.as_str() {
            // Row locks: FOR UPDATE, FOR NO KEY UPDATE, FOR SHARE, LOCK IN SHARE MODE.
            "UPDATE" | "SHARE" if matches!(prev, "FOR" | "KEY" | "IN") => WRITE,
            "INSERT" | "UPDATE" | "DELETE" | "MERGE" | "UPSERT" if !dml_ok => DANGEROUS,
            "INTO" if matches!(next, "OUTFILE" | "DUMPFILE") => DANGEROUS,
            // INSERT INTO / MERGE INTO inside a CTE is already counted above.
            "INTO" if !matches!(prev, "INSERT" | "MERGE" | "UPSERT" | "REPLACE") => MODIFY,
            "SET" if !dml_ok && prev != "CHARACTER" => DANGEROUS,
            "DROP" | "TRUNCATE" | "ALTER" | "CREATE" | "GRANT" | "REVOKE" | "DENY" | "EXEC"
            | "EXECUTE" | "CALL" | "DBCC" | "SHUTDOWN" | "KILL" | "BACKUP" | "RESTORE" | "BULK"
            | "RECONFIGURE" | "WAITFOR" => DANGEROUS,
            _ => READ,
        };
        level = level.max(l);
    }
    level
}

/// Functions that change something when a SELECT calls them.
const WRITE_FUNCTIONS: &[&str] = &[
    "nextval", "setval", "set_config", "pg_notify", "lo_create", "lo_creat", "lo_unlink",
    "lo_put", "lo_from_bytea", "lo_truncate", "pg_advisory_lock", "pg_advisory_lock_shared",
    "pg_advisory_xact_lock", "pg_advisory_xact_lock_shared", "pg_try_advisory_lock",
    "pg_try_advisory_lock_shared", "pg_try_advisory_xact_lock", "pg_try_advisory_xact_lock_shared",
    "pg_stat_reset", "pg_stat_reset_shared", "pg_stat_reset_single_table_counters",
    "pg_stat_statements_reset", "get_lock", "release_lock", "release_all_locks",
];

/// Functions that reach past the data: other sessions, the server's files,
/// other servers, the server config, or extension loading.
const DANGEROUS_FUNCTIONS: &[&str] = &[
    "pg_terminate_backend", "pg_cancel_backend", "pg_reload_conf", "pg_rotate_logfile",
    "pg_promote", "pg_switch_wal", "pg_switch_xlog", "pg_create_restore_point",
    "pg_create_physical_replication_slot", "pg_create_logical_replication_slot",
    "pg_drop_replication_slot", "pg_copy_physical_replication_slot",
    "pg_copy_logical_replication_slot", "pg_replication_origin_create",
    "pg_replication_origin_drop", "pg_logical_emit_message", "pg_backup_start",
    "pg_backup_stop", "pg_start_backup", "pg_stop_backup", "pg_wal_replay_pause",
    "pg_wal_replay_resume", "lo_import", "lo_export", "pg_file_write", "pg_file_unlink",
    "pg_file_rename", "pg_file_sync", "pg_read_file", "pg_read_binary_file", "pg_ls_dir",
    "dblink", "dblink_exec", "dblink_send_query", "dblink_connect", "dblink_connect_u",
    "dblink_open", "dblink_fetch", "load_file", "sys_exec", "sys_eval", "load_extension",
    "writefile", "readfile", "edit", "fts3_tokenizer", "openrowset", "opendatasource",
    "openquery", "xp_cmdshell",
];

/// The worst function a statement calls (`name(`, schema-qualified or not).
fn call_level(stmt: &str) -> u8 {
    let chars: Vec<char> = stmt.chars().collect();
    let mut level = READ;
    let mut i = 0;
    while i < chars.len() {
        if !(chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') { i += 1; }
        let name: String = chars[start..i].iter().collect::<String>().to_ascii_lowercase();
        let mut j = i;
        while j < chars.len() && chars[j].is_whitespace() { j += 1; }
        if chars.get(j) != Some(&'(') { continue; }
        if DANGEROUS_FUNCTIONS.contains(&name.as_str()) {
            level = DANGEROUS;
        } else if WRITE_FUNCTIONS.contains(&name.as_str()) {
            level = level.max(WRITE);
        }
    }
    level
}

/// SQLite pragmas that only report, even when given an argument.
const PRAGMA_REPORTS: &[&str] = &[
    "table_info", "table_xinfo", "table_list", "index_info", "index_xinfo", "index_list",
    "foreign_key_list", "foreign_key_check", "integrity_check", "quick_check",
];

/// Pragmas that act even without an argument.
const PRAGMA_ACTIONS: &[&str] = &["optimize", "shrink_memory", "wal_checkpoint", "incremental_vacuum"];

/// `PRAGMA x` reads a setting; `PRAGMA x = y` or `PRAGMA x(y)` changes it,
/// except for the pragmas that only report on a table or index.
fn pragma_level(stmt: &str) -> u8 {
    let lower = stmt.to_ascii_lowercase();
    let Some(at) = lower.find("pragma") else { return DANGEROUS };
    let after = lower[at + "pragma".len()..].trim_start();
    let end = after
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
        .unwrap_or(after.len());
    let name = after[..end].rsplit('.').next().unwrap_or("");
    let rest = after[end..].trim_start();
    if name == "writable_schema" && !rest.is_empty() {
        return DANGEROUS;
    }
    if rest.starts_with('=') {
        MODIFY
    } else if rest.starts_with('(') {
        if PRAGMA_REPORTS.contains(&name) { READ } else { MODIFY }
    } else if PRAGMA_ACTIONS.contains(&name) {
        MODIFY
    } else {
        READ
    }
}

/// Mongo shell: classified from what `db::mongo::parse` will actually run.
fn mongo_level(stmt: &str) -> u8 {
    use crate::db::mongo::{parse, Stmt};
    let Ok(parsed) = parse(stmt) else { return DANGEROUS };
    match parsed {
        Stmt::Use(_) | Stmt::ShowDbs | Stmt::ShowCollections => READ,
        Stmt::Coll { calls, .. } => {
            let Some(head) = calls.first() else { return DANGEROUS };
            let args: String = head.args.chars().filter(|c| !c.is_whitespace()).collect();
            let all_rows = args.is_empty() || args.starts_with("{}");
            match head.name.as_str() {
                // $out replaces a whole collection; $merge writes into one.
                "aggregate" if args.contains("$out") || args.contains("$merge") => DANGEROUS,
                "find" | "findOne" | "aggregate" | "countDocuments" | "estimatedDocumentCount"
                | "distinct" | "getIndexes" => READ,
                "insertOne" | "insertMany" | "updateOne" | "replaceOne" | "deleteOne" => WRITE,
                "updateMany" | "deleteMany" => if all_rows { DANGEROUS } else { WRITE },
                _ => DANGEROUS,
            }
        }
    }
}

/// Redis commands that only read.
const REDIS_READS: &[&str] = &[
    "GET", "MGET", "EXISTS", "TYPE", "TTL", "PTTL", "EXPIRETIME", "PEXPIRETIME", "STRLEN",
    "GETRANGE", "SUBSTR", "LCS", "HGET", "HGETALL", "HMGET", "HKEYS", "HVALS", "HLEN", "HEXISTS",
    "HSTRLEN", "HSCAN", "HRANDFIELD", "LRANGE", "LLEN", "LINDEX", "LPOS", "SMEMBERS", "SISMEMBER",
    "SMISMEMBER", "SCARD", "SRANDMEMBER", "SSCAN", "SINTER", "SINTERCARD", "SUNION", "SDIFF",
    "ZRANGE", "ZRANGEBYSCORE", "ZREVRANGE", "ZREVRANGEBYSCORE", "ZRANGEBYLEX", "ZREVRANGEBYLEX",
    "ZCARD", "ZSCORE", "ZMSCORE", "ZRANK", "ZREVRANK", "ZCOUNT", "ZLEXCOUNT", "ZSCAN", "ZRANDMEMBER",
    "ZINTER", "ZUNION", "ZDIFF", "ZINTERCARD", "KEYS", "SCAN", "RANDOMKEY", "DBSIZE", "INFO", "PING",
    "ECHO", "TIME", "LASTSAVE", "ROLE", "XRANGE", "XREVRANGE", "XLEN", "XREAD", "XPENDING", "XINFO",
    "OBJECT", "PFCOUNT", "GETBIT", "BITCOUNT", "BITPOS", "BITFIELD_RO", "GEOPOS", "GEODIST",
    "GEOHASH", "GEOSEARCH", "GEORADIUS_RO", "GEORADIUSBYMEMBER_RO", "SORT_RO", "DUMP", "TOUCH",
    "COMMAND", "JSON.GET", "JSON.MGET", "JSON.TYPE", "JSON.OBJKEYS", "JSON.OBJLEN", "JSON.ARRLEN",
    "JSON.STRLEN", "JSON.RESP", "FT.SEARCH", "FT.INFO", "FT._LIST", "FT.AGGREGATE", "TS.GET",
    "TS.RANGE", "TS.REVRANGE", "TS.MGET", "TS.MRANGE", "TS.INFO",
];

/// Redis commands that change keys (and nothing past them).
const REDIS_WRITES: &[&str] = &[
    "SET", "SETEX", "PSETEX", "SETNX", "SETRANGE", "MSET", "MSETNX", "APPEND", "INCR", "INCRBY",
    "INCRBYFLOAT", "DECR", "DECRBY", "GETSET", "GETDEL", "GETEX", "DEL", "UNLINK", "EXPIRE",
    "PEXPIRE", "EXPIREAT", "PEXPIREAT", "PERSIST", "RENAME", "RENAMENX", "COPY", "MOVE", "RESTORE",
    "HSET", "HSETNX", "HMSET", "HDEL", "HINCRBY", "HINCRBYFLOAT", "LPUSH", "RPUSH", "LPUSHX",
    "RPUSHX", "LPOP", "RPOP", "LSET", "LREM", "LTRIM", "LINSERT", "LMOVE", "LMPOP", "RPOPLPUSH",
    "SADD", "SREM", "SPOP", "SMOVE", "SINTERSTORE", "SUNIONSTORE", "SDIFFSTORE", "ZADD", "ZREM",
    "ZINCRBY", "ZPOPMIN", "ZPOPMAX", "ZMPOP", "ZREMRANGEBYSCORE", "ZREMRANGEBYRANK",
    "ZREMRANGEBYLEX", "ZUNIONSTORE", "ZINTERSTORE", "ZDIFFSTORE", "ZRANGESTORE", "XADD", "XDEL",
    "XTRIM", "XACK", "XCLAIM", "XAUTOCLAIM", "XREADGROUP", "XGROUP", "XSETID", "PFADD", "PFMERGE",
    "SETBIT", "BITOP", "BITFIELD", "GEOADD", "GEOSEARCHSTORE", "SORT", "PUBLISH", "SPUBLISH",
    "JSON.SET", "JSON.DEL", "JSON.FORGET", "JSON.MSET", "JSON.MERGE", "JSON.ARRAPPEND",
    "JSON.ARRINSERT", "JSON.ARRPOP", "JSON.ARRTRIM", "JSON.NUMINCRBY", "JSON.STRAPPEND",
    "JSON.TOGGLE", "JSON.CLEAR", "TS.ADD", "TS.MADD", "TS.INCRBY", "TS.DECRBY", "TS.DEL",
];

/// One Redis command. Anything not known to read or to only change keys
/// (FLUSHALL, CONFIG SET, EVAL, SCRIPT, MODULE, SHUTDOWN, ...) is the worst case.
fn redis_level(command: &str) -> u8 {
    let mut parts = command
        .split_whitespace()
        .map(|t| t.trim_matches(|c| c == '"' || c == '\'').to_ascii_uppercase());
    let Some(cmd) = parts.next() else { return READ };
    let sub = parts.next().unwrap_or_default();
    match cmd.as_str() {
        "CONFIG" => if sub == "GET" { READ } else { DANGEROUS },
        "CLIENT" => if matches!(sub.as_str(), "LIST" | "INFO" | "GETNAME" | "ID") { READ } else { DANGEROUS },
        "MEMORY" => if matches!(sub.as_str(), "USAGE" | "STATS" | "DOCTOR") { READ } else { DANGEROUS },
        c if REDIS_READS.contains(&c) => READ,
        c if REDIS_WRITES.contains(&c) => WRITE,
        _ => DANGEROUS,
    }
}

/// Split on `;` outside quotes and comments, dropping the comments and the
/// contents of string literals so their words cannot change the verdict.
fn split_statements(sql: &str, lx: Lex) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            // MySQL only takes `--` as a comment when whitespace follows:
            // `1--1` is arithmetic there.
            '-' if next == Some('-')
                && !(lx.mysql && chars.get(i + 2).is_some_and(|c| !c.is_whitespace())) =>
            {
                while i < chars.len() && chars[i] != '\n' { i += 1; }
                cur.push(' ');
            }
            '#' if lx.mysql => {
                while i < chars.len() && chars[i] != '\n' { i += 1; }
                cur.push(' ');
            }
            // MySQL runs the body of `/*! ... */`: read it as code.
            '/' if next == Some('*') && lx.mysql && chars.get(i + 2) == Some(&'!') => {
                i += 2;
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
                    if lx.backslash && q != '`' && chars[i] == '\\' {
                        if let Some(&e) = chars.get(i + 1) { body.push(e); }
                        i += 2;
                        continue;
                    }
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
            '$' if lx.dollar => {
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
        // A dollar-quoted body is a string on Postgres only; elsewhere the DROP runs.
        assert_eq!(script_level_in(Dialect::Postgres, "SELECT $$; DROP TABLE t$$"), READ);
        assert_eq!(script_level("SELECT $$; DROP TABLE t$$"), DANGEROUS);
    }

    #[test]
    fn statement_counts() {
        assert_eq!(statement_count(Dialect::Sql, "SELECT 1"), 1);
        assert_eq!(statement_count(Dialect::Sql, "SELECT 1;  -- done"), 1);
        assert_eq!(statement_count(Dialect::Sql, "SELECT 1; DROP TABLE x"), 2);
        assert_eq!(statement_count(Dialect::Sql, "SET SHOWPLAN_ALL OFF; DROP TABLE x"), 2);
        assert_eq!(statement_count(Dialect::Sql, "SELECT ';'"), 1);
        assert_eq!(statement_count(Dialect::Mongo, "db.t.find({a: ';'})"), 1);
        assert_eq!(statement_count(Dialect::Redis, "SET k \"a;b\""), 1);
    }

    #[test]
    fn quoting_tricks_cannot_hide_a_statement() {
        // MySQL reads \' as an escaped quote, so the DROP is outside the string.
        assert_eq!(script_level("SELECT '\\'; DROP TABLE t; -- '"), DANGEROUS);
        assert_eq!(statement_count(Dialect::Sql, "SELECT '\\'; DROP TABLE t; -- '"), 2);
        // Standard SQL reads 'C:\' as a whole string, so the call is code.
        assert_eq!(script_level("SELECT 'C:\\', pg_terminate_backend(1) -- '"), DANGEROUS);
        // MySQL: `1--1` is arithmetic, not a comment.
        assert_eq!(script_level("SELECT 1--1, load_file('/etc/passwd')"), DANGEROUS);
        // MySQL runs the inside of /*! ... */.
        assert_eq!(script_level("SELECT 1 /*! , sys_exec('id') */"), DANGEROUS);
        // A plain comment is still just a comment.
        assert_eq!(script_level("SELECT 1 -- delete this later\n/* drop? */"), READ);
    }

    #[test]
    fn selects_with_side_effects() {
        assert_eq!(script_level("SELECT * INTO new_t FROM t"), MODIFY);
        assert_eq!(script_level("SELECT * FROM t INTO OUTFILE '/tmp/x'"), DANGEROUS);
        assert_eq!(script_level("SELECT 'x' INTO DUMPFILE '/tmp/x'"), DANGEROUS);
        assert_eq!(script_level("SELECT pg_terminate_backend(42)"), DANGEROUS);
        assert_eq!(script_level("SELECT pg_cancel_backend (42)"), DANGEROUS);
        assert_eq!(script_level("SELECT pg_catalog.setval('s', 1)"), WRITE);
        assert_eq!(script_level("SELECT nextval('s')"), WRITE);
        assert_eq!(script_level("SELECT set_config('transaction_read_only', 'off', true)"), WRITE);
        assert_eq!(script_level("SELECT dblink_exec('db', 'DROP TABLE t')"), DANGEROUS);
        assert_eq!(script_level("SELECT * FROM dblink('db', 'SELECT 1') AS x(a int)"), DANGEROUS);
        assert_eq!(script_level("SELECT lo_export(1, '/tmp/x')"), DANGEROUS);
        assert_eq!(script_level("SELECT load_extension('x')"), DANGEROUS);
        assert_eq!(script_level("SELECT * FROM t FOR UPDATE"), WRITE);
        assert_eq!(script_level("SELECT * FROM t LOCK IN SHARE MODE"), WRITE);
        assert_eq!(script_level("WITH c AS (SELECT 1) SELECT * INTO t2 FROM c"), MODIFY);
        assert_eq!(script_level("WITH c AS (SELECT 1) SELECT nextval('s') FROM c"), WRITE);
        // A column that is merely called nextval is not a call.
        assert_eq!(script_level("SELECT nextval FROM t"), READ);
        assert_eq!(script_level("SELECT setval_count, \"pg_ls_dir\" FROM t"), READ);
        // Words in strings do not count.
        assert_eq!(script_level("SELECT 'pg_terminate_backend(1)'"), READ);
        assert_eq!(script_level("SELECT CAST(a AS CHAR CHARACTER SET utf8) FROM t"), READ);
    }

    #[test]
    fn tsql_statements_without_semicolons() {
        assert_eq!(script_level("SELECT 1 DROP TABLE t"), DANGEROUS);
        assert_eq!(script_level("SELECT 1 DELETE FROM t WHERE id = 1"), DANGEROUS);
        assert_eq!(script_level("SELECT 1 EXEC xp_cmdshell 'dir'"), DANGEROUS);
        assert_eq!(script_level("SELECT 1 SET SHOWPLAN_ALL OFF"), DANGEROUS);
    }

    #[test]
    fn transaction_control_needs_write() {
        // COMMIT or a new transaction would leave the read-only transaction.
        assert_eq!(script_level("COMMIT"), WRITE);
        assert_eq!(script_level("START TRANSACTION READ WRITE"), WRITE);
        assert_eq!(script_level("BEGIN; SELECT 1; COMMIT"), WRITE);
        assert_eq!(script_level("SET TRANSACTION READ WRITE"), DANGEROUS);
    }

    #[test]
    fn sqlite_pragmas() {
        assert_eq!(script_level("PRAGMA user_version"), READ);
        assert_eq!(script_level("PRAGMA table_info(users)"), READ);
        assert_eq!(script_level("PRAGMA main.index_list('users')"), READ);
        assert_eq!(script_level("PRAGMA user_version = 7"), MODIFY);
        assert_eq!(script_level("PRAGMA journal_mode=WAL"), MODIFY);
        assert_eq!(script_level("PRAGMA journal_mode(DELETE)"), MODIFY);
        assert_eq!(script_level("PRAGMA foreign_keys = OFF"), MODIFY);
        assert_eq!(script_level("PRAGMA query_only = 0"), MODIFY);
        assert_eq!(script_level("PRAGMA wal_checkpoint"), MODIFY);
        assert_eq!(script_level("PRAGMA writable_schema = ON"), DANGEROUS);
    }

    #[test]
    fn mongo_shell() {
        let m = |s| script_level_in(Dialect::Mongo, s);
        assert_eq!(m("db.users.find({age: {$gt: 3}}).limit(5)"), READ);
        assert_eq!(m("db.users.countDocuments({})"), READ);
        assert_eq!(m("db.users.aggregate([{$match: {}}])"), READ);
        assert_eq!(m("show collections"), READ);
        assert_eq!(m("db.users.insertOne({a: 1})"), WRITE);
        assert_eq!(m("db.users.updateOne({_id: 1}, {$set: {a: 2}})"), WRITE);
        assert_eq!(m("db.users.updateMany({a: 1}, {$set: {a: 2}})"), WRITE);
        assert_eq!(m("db.users.deleteMany({a: 1})"), WRITE);
        assert_eq!(m("db.users.deleteMany({})"), DANGEROUS);
        assert_eq!(m("db.users.updateMany({ }, {$set: {a: 2}})"), DANGEROUS);
        assert_eq!(m("db.users.aggregate([{$match: {}}, {$out: 'other'}])"), DANGEROUS);
        assert_eq!(m("db.users.aggregate([{$merge: {into: 'other'}}])"), DANGEROUS);
        // Anything the console can't parse (drop, eval, ...) is the worst case.
        assert_eq!(m("db.users.drop()"), DANGEROUS);
        assert_eq!(m("db.dropDatabase()"), DANGEROUS);
    }

    #[test]
    fn redis_commands() {
        let r = |s| script_level_in(Dialect::Redis, s);
        assert_eq!(r("GET k"), READ);
        assert_eq!(r("hgetall user:1"), READ);
        assert_eq!(r("SCAN 0 MATCH * COUNT 100"), READ);
        assert_eq!(r("CONFIG GET maxmemory"), READ);
        assert_eq!(r("SET k v"), WRITE);
        assert_eq!(r("\"del\" k"), WRITE);
        assert_eq!(r("EXPIRE k 10"), WRITE);
        assert_eq!(r("FLUSHALL"), DANGEROUS);
        assert_eq!(r("CONFIG SET dir /tmp"), DANGEROUS);
        assert_eq!(r("EVAL \"return redis.call('del','k')\" 0"), DANGEROUS);
        assert_eq!(r("CLIENT KILL ID 3"), DANGEROUS);
        assert_eq!(r("SHUTDOWN"), DANGEROUS);
    }

    #[test]
    fn partiql_and_cql_use_the_sql_rules() {
        assert_eq!(script_level("SELECT * FROM \"Music\" WHERE Artist = 'x'"), READ);
        assert_eq!(script_level("INSERT INTO \"Music\" VALUE {'Artist': 'x'}"), WRITE);
        assert_eq!(script_level("UPDATE \"Music\" SET a = 1 WHERE Artist = 'x'"), WRITE);
        assert_eq!(script_level("DELETE FROM \"Music\" WHERE Artist = 'x'"), WRITE);
        assert_eq!(script_level("TRUNCATE ks.t"), DANGEROUS);
        assert_eq!(script_level_in(Dialect::ReadOnlyDoc, "anything"), READ);
    }

    #[test]
    fn dialects_by_driver() {
        assert_eq!(Dialect::for_driver("postgres"), Dialect::Postgres);
        assert_eq!(Dialect::for_driver("mongodb"), Dialect::Mongo);
        assert_eq!(Dialect::for_driver("redis"), Dialect::Redis);
        assert_eq!(Dialect::for_driver("couchdb"), Dialect::ReadOnlyDoc);
        assert_eq!(Dialect::for_driver("cassandra"), Dialect::Sql);
        assert_eq!(Dialect::for_driver("dynamodb"), Dialect::Sql);
        assert_eq!(Dialect::for_driver("oracle"), Dialect::Oracle);
    }

    #[test]
    fn oracle_scripts() {
        let o = |s| script_level_in(Dialect::Oracle, s);
        assert_eq!(o("SELECT * FROM emp WHERE action = 'DELETE'"), READ);
        assert_eq!(o("select q'[it's]' from dual"), READ);
        assert_eq!(o("INSERT INTO emp (id) VALUES (1)"), WRITE);
        // A q-quote the generic lexer misreads must not hide what follows it.
        assert_eq!(o("SELECT q'[it's]' FROM dual;\nDROP TABLE emp"), DANGEROUS);
        // A PL/SQL block can run anything, DDL through EXECUTE IMMEDIATE included.
        assert_eq!(o("BEGIN\n  EXECUTE IMMEDIATE 'DROP TABLE emp';\nEND;\n/"), DANGEROUS);
        assert_eq!(o("DECLARE n NUMBER; BEGIN SELECT 1 INTO n FROM dual; END;"), DANGEROUS);
        assert_eq!(statement_count(Dialect::Oracle, "SELECT 1 FROM dual;\nSELECT 2 FROM dual"), 2);
    }
}
