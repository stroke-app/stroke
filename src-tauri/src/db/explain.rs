use super::query::SqlResult;
use serde::Serialize;
use sqlx::Row;

#[derive(Debug, Serialize)]
pub struct ExplainResult {
    /// The root Plan node (PostgreSQL) or equivalent tree (MySQL/SQLite).
    pub plan: serde_json::Value,
    /// Milliseconds spent planning (PostgreSQL only; 0 for others).
    pub planning_time: f64,
    /// Milliseconds spent executing (PostgreSQL only; 0 for others).
    pub execution_time: f64,
    /// Which driver produced this result - helps the frontend pick the renderer.
    pub driver: String,
}

/// Build an ExplainResult from an engine's textual EXPLAIN output (one plan line
/// per string). Leading whitespace defines nesting, so an indented plan
/// (DuckDB / ClickHouse) becomes the same `{Node Type, Plans}` tree the renderer
/// already draws for Postgres/SQLite. Never executes anything itself.
pub fn explain_from_text_lines(lines: Vec<String>, driver: &str) -> ExplainResult {
    struct N {
        text: String,
        children: Vec<usize>,
    }
    let mut arena: Vec<N> = Vec::new();
    let mut roots: Vec<usize> = Vec::new();
    // Stack of (indent, node index) - a node attaches under the nearest
    // strictly-shallower ancestor still on the stack.
    let mut stack: Vec<(usize, usize)> = Vec::new();

    for raw in lines {
        let line = raw.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let idx = arena.len();
        arena.push(N {
            text: line.trim().to_string(),
            children: Vec::new(),
        });
        while matches!(stack.last(), Some(&(ind, _)) if ind >= indent) {
            stack.pop();
        }
        match stack.last() {
            Some(&(_, parent)) => arena[parent].children.push(idx),
            None => roots.push(idx),
        }
        stack.push((indent, idx));
    }

    fn to_json(arena: &[N], idx: usize) -> serde_json::Value {
        let n = &arena[idx];
        let mut obj = serde_json::json!({
            "Node Type": n.text,
            "Total Cost": 0.0,
            "Startup Cost": 0.0,
            "Plan Rows": 0,
        });
        if !n.children.is_empty() {
            obj["Plans"] =
                serde_json::Value::Array(n.children.iter().map(|&c| to_json(arena, c)).collect());
        }
        obj
    }

    let plan = match roots.len() {
        1 => to_json(&arena, roots[0]),
        0 => serde_json::json!({ "Node Type": "Query Plan", "Total Cost": 0.0, "Startup Cost": 0.0, "Plan Rows": 0 }),
        _ => serde_json::json!({
            "Node Type": "Query Plan",
            "Total Cost": 0.0,
            "Startup Cost": 0.0,
            "Plan Rows": 0,
            "Plans": roots.iter().map(|&r| to_json(&arena, r)).collect::<Vec<_>>(),
        }),
    };

    ExplainResult {
        plan,
        planning_time: 0.0,
        execution_time: 0.0,
        driver: driver.to_string(),
    }
}

fn strip_sql(sql: &str) -> &str {
    let s = sql.trim().trim_end_matches(';').trim();
    // Strip any existing EXPLAIN prefix so callers never double-wrap
    let upper = s.to_ascii_uppercase();
    if upper.starts_with("EXPLAIN") {
        let rest = &s[7..].trim_start();
        // Skip optional ANALYZE / QUERY PLAN / FORMAT ... words
        let upper_rest = rest.to_ascii_uppercase();
        let rest = if upper_rest.starts_with("ANALYZE") {
            rest[7..].trim_start()
        } else if upper_rest.starts_with("QUERY PLAN") {
            rest[10..].trim_start()
        } else if upper_rest.starts_with('(') {
            // EXPLAIN (ANALYZE, ...) - skip to closing paren
            rest.find(')').map(|i| rest[i + 1..].trim_start()).unwrap_or(rest)
        } else {
            rest
        };
        return rest;
    }
    s
}

/// Postgres plan as JSON. With `analyze` the statement really runs (that is
/// what gives actual rows and times), so it runs inside a transaction that is
/// always rolled back: explaining a DELETE never deletes anything. Without it
/// only the planner's estimates come back and nothing runs.
/// A Postgres plan that Stop can end. ANALYZE runs the statement for real, so a
/// plan of a 20-minute query took 20 minutes with nothing to press: it now runs
/// in the console's own transaction shape (the statement timeout, and a tag
/// Stop finds the backend by), and Stop cancels it on the server.
pub async fn explain_pg_stoppable(
    pool: &sqlx::PgPool,
    sql: &str,
    analyze: bool,
    cancel_rx: Option<tokio::sync::oneshot::Receiver<()>>,
) -> Result<ExplainResult, String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    let stripped = strip_sql(sql);
    let q = if analyze {
        format!("EXPLAIN (ANALYZE, BUFFERS, VERBOSE, FORMAT JSON) {stripped}")
    } else {
        format!("EXPLAIN (VERBOSE, FORMAT JSON) {stripped}")
    };

    // PG returns [{Plan: {...}, "Planning Time": x, "Execution Time": y}]
    let cancelled = std::sync::Arc::new(AtomicBool::new(false));
    let fetched = async {
        let tag = super::query::run_tag();
        let (mut tx, target) = super::query::begin_console_tx(pool, &tag).await?;
        super::query::arm_pg_cancel(pool, cancel_rx, cancelled.clone(), target);
        let out = sqlx::query_as::<_, (serde_json::Value,)>(&q).fetch_one(&mut *tx).await;
        // Roll back whatever ANALYZE did, success or not.
        let _ = tx.rollback().await;
        out.map_err(|e| e.to_string())
    }
    .await;
    if cancelled.load(Ordering::Relaxed) {
        return Err(super::query::QUERY_CANCELLED.into());
    }
    let arr = match fetched {
        Ok(row) => row.0,
        // CockroachDB speaks the PG wire protocol but rejects PG's EXPLAIN option
        // syntax (`at or near "analyze"`). Fall back to its indented text plan; if
        // that also fails, surface the original error - it's the meaningful one for
        // a genuine query fault on real PostgreSQL.
        Err(e) => {
            return match explain_cockroach(pool, sql).await {
                Ok(res) => Ok(res),
                Err(_) => Err(format!("EXPLAIN failed: {e}")),
            };
        }
    };
    let entry = arr
        .as_array()
        .and_then(|a| a.first())
        .cloned()
        .ok_or_else(|| "Empty EXPLAIN result".to_string())?;

    let plan = entry["Plan"].clone();
    let planning_time = entry["Planning Time"].as_f64().unwrap_or(0.0);
    let execution_time = entry["Execution Time"].as_f64().unwrap_or(0.0);

    Ok(ExplainResult {
        plan,
        planning_time,
        execution_time,
        driver: "postgres".into(),
    })
}

/// CockroachDB has no JSON plan format, so parse its indented `• node` text tree
/// into the same `{ "Node Type", "Plans" }` shape the PG/SQLite renderer draws.
/// Uses plain `EXPLAIN` (not `ANALYZE`) so the query is never executed.
pub async fn explain_cockroach(pool: &sqlx::PgPool, sql: &str) -> Result<ExplainResult, String> {
    let sql = strip_sql(sql);
    let q = format!("EXPLAIN {sql}");

    let rows = sqlx::query(&q)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("EXPLAIN failed: {e}"))?;

    let lines: Vec<String> = rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>(0).ok())
        .collect();

    let plan = parse_cockroach_plan(&lines).ok_or_else(|| "Empty EXPLAIN result".to_string())?;

    Ok(ExplainResult {
        plan,
        planning_time: 0.0,
        execution_time: 0.0,
        driver: "cockroach".into(),
    })
}

/// Turn Cockroach's `EXPLAIN` text (one plan line per row) into a nested plan
/// tree. Node lines carry a `•` bullet whose indentation encodes depth; the
/// `key: value` lines that follow describe the node just above them.
fn parse_cockroach_plan(lines: &[String]) -> Option<serde_json::Value> {
    use serde_json::{json, Map, Value};

    struct Frame {
        depth: usize,
        node: Map<String, Value>,
        children: Vec<Value>,
    }
    fn finalize(mut f: Frame) -> Value {
        if !f.children.is_empty() {
            f.node.insert("Plans".into(), Value::Array(f.children));
        }
        Value::Object(f.node)
    }
    // Attach a finished frame to its parent (or the root list if there is none).
    fn attach(stack: &mut Vec<Frame>, roots: &mut Vec<Value>, done: Value) {
        match stack.last_mut() {
            Some(p) => p.children.push(done),
            None => roots.push(done),
        }
    }

    let mut stack: Vec<Frame> = Vec::new();
    let mut roots: Vec<Value> = Vec::new();

    for line in lines {
        if let Some((prefix, rest)) = line.split_once('•') {
            let depth = prefix.chars().count();
            let name = rest.trim();
            if name.is_empty() {
                continue;
            }
            while stack.last().is_some_and(|f| f.depth >= depth) {
                let done = finalize(stack.pop().unwrap());
                attach(&mut stack, &mut roots, done);
            }
            let mut node = Map::new();
            node.insert("Node Type".into(), json!(name));
            node.insert("Total Cost".into(), json!(0.0));
            node.insert("Startup Cost".into(), json!(0.0));
            node.insert("Plan Rows".into(), json!(0));
            stack.push(Frame { depth, node, children: Vec::new() });
        } else if let Some((k, v)) = line.split_once(':') {
            // A detail line for the current node (header lines like
            // "distribution: full" arrive before any bullet and are ignored).
            if let Some(f) = stack.last_mut() {
                let key = k.trim_matches(|c: char| "│└├─ \t".contains(c)).to_ascii_lowercase();
                let val = v.trim();
                match key.as_str() {
                    "table" => {
                        f.node.insert("Relation Name".into(), json!(val.split('@').next().unwrap_or(val)));
                    }
                    "estimated row count" => {
                        let digits: String =
                            val.chars().take_while(|c| c.is_ascii_digit() || *c == ',').filter(|c| *c != ',').collect();
                        if let Ok(n) = digits.parse::<i64>() {
                            f.node.insert("Plan Rows".into(), json!(n));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    while let Some(f) = stack.pop() {
        let done = finalize(f);
        attach(&mut stack, &mut roots, done);
    }

    match roots.len() {
        0 => None,
        1 => roots.into_iter().next(),
        _ => Some(json!({
            "Node Type": "Query Plan",
            "Total Cost": 0.0,
            "Startup Cost": 0.0,
            "Plan Rows": 0,
            "Plans": roots,
        })),
    }
}

pub async fn explain_mysql(pool: &sqlx::MySqlPool, sql: &str) -> Result<ExplainResult, String> {
    let sql = strip_sql(sql);
    let q = format!("EXPLAIN FORMAT=JSON {sql}");

    let row: (String,) = sqlx::query_as(&q)
        .fetch_one(pool)
        .await
        .map_err(|e| format!("EXPLAIN failed: {e}"))?;

    let plan: serde_json::Value =
        serde_json::from_str(&row.0).map_err(|e| format!("Failed to parse EXPLAIN: {e}"))?;

    Ok(ExplainResult {
        plan,
        planning_time: 0.0,
        execution_time: 0.0,
        driver: "mysql".into(),
    })
}

pub async fn explain_sqlite(pool: &sqlx::SqlitePool, sql: &str) -> Result<ExplainResult, String> {
    let sql = strip_sql(sql);
    let q = format!("EXPLAIN QUERY PLAN {sql}");

    let rows = sqlx::query(&q)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("EXPLAIN failed: {e}"))?;

    // SQLite returns (id, parent, notused, detail)
    let flat: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id":     r.try_get::<i64, _>(0).unwrap_or(0),
                "parent": r.try_get::<i64, _>(1).unwrap_or(0),
                "detail": r.try_get::<String, _>(3).unwrap_or_default(),
            })
        })
        .collect();

    let plan = build_sqlite_tree(&flat);

    Ok(ExplainResult {
        plan,
        planning_time: 0.0,
        execution_time: 0.0,
        driver: "sqlite".into(),
    })
}

/// Convert SQLite's flat (id, parent, detail) rows into a nested plan tree
/// matching the shape of a PostgreSQL plan node so the same renderer handles both.
fn build_sqlite_tree(nodes: &[serde_json::Value]) -> serde_json::Value {
    fn children(all: &[serde_json::Value], parent_id: i64) -> Vec<serde_json::Value> {
        all.iter()
            .filter(|n| n["parent"].as_i64().unwrap_or(0) == parent_id)
            .map(|n| {
                let id = n["id"].as_i64().unwrap_or(0);
                let detail = n["detail"].as_str().unwrap_or("").to_string();
                let kids = children(all, id);
                let mut obj = serde_json::json!({
                    "Node Type": detail,
                    "Total Cost": 0.0,
                    "Startup Cost": 0.0,
                    "Plan Rows": 0,
                });
                if !kids.is_empty() {
                    obj["Plans"] = serde_json::Value::Array(kids);
                }
                obj
            })
            .collect()
    }

    let roots = children(nodes, 0);
    match roots.len() {
        1 => roots.into_iter().next().unwrap(),
        _ => serde_json::json!({
            "Node Type": "Query Plan",
            "Total Cost": 0.0,
            "Startup Cost": 0.0,
            "Plan Rows": 0,
            "Plans": roots,
        }),
    }
}

/// Build an ExplainResult from the rows of a `EXPLAIN QUERY PLAN` statement that
/// was run through a non-SQLx path (Cloudflare D1). D1 is SQLite under the hood
/// and returns the same `(id, parent, notused, detail)` shape, so we reuse
/// `build_sqlite_tree`. Columns are matched by name because the transport
/// (JSON object per row) doesn't guarantee positional ordering.
pub fn explain_from_sqlite_plan(res: &SqlResult, driver: &str) -> ExplainResult {
    let col = |name: &str| res.columns.iter().position(|c| c.name.eq_ignore_ascii_case(name));
    let (id_i, parent_i, detail_i) = (col("id"), col("parent"), col("detail"));

    let flat: Vec<serde_json::Value> = res
        .rows
        .iter()
        .map(|r| {
            let as_i64 = |i: Option<usize>| {
                i.and_then(|i| r.get(i))
                    .map(|v| v.as_i64().unwrap_or_else(|| v.as_str().and_then(|s| s.parse().ok()).unwrap_or(0)))
                    .unwrap_or(0)
            };
            let detail = detail_i
                .and_then(|i| r.get(i))
                .map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))
                .unwrap_or_default();
            serde_json::json!({
                "id": as_i64(id_i),
                "parent": as_i64(parent_i),
                "detail": detail,
            })
        })
        .collect();

    ExplainResult {
        plan: build_sqlite_tree(&flat),
        planning_time: 0.0,
        execution_time: 0.0,
        driver: driver.into(),
    }
}

#[cfg(test)]
mod stop_tests {
    //   STROKE_TEST_PG_URL=postgres://postgres@127.0.0.1:5433/postgres cargo test --lib explain::stop_tests
    #[tokio::test]
    async fn explain_analyze_stops_on_the_server() {
        let Ok(url) = std::env::var("STROKE_TEST_PG_URL") else { return };
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(3).connect(&url).await.unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            let _ = tx.send(());
        });
        let t = std::time::Instant::now();
        let r = super::explain_pg_stoppable(&pool, "SELECT pg_sleep(30)", true, Some(rx)).await;
        assert_eq!(r.unwrap_err(), crate::db::query::QUERY_CANCELLED);
        assert!(t.elapsed() < std::time::Duration::from_secs(5), "took {:?}", t.elapsed());
        // Nothing of it is still running on the server.
        let left: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE query LIKE 'EXPLAIN (ANALYZE%pg_sleep(30)%' AND state = 'active'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 0);
        // And without Stop it still returns a plan.
        let ok = super::explain_pg_stoppable(&pool, "SELECT 1", true, None).await.unwrap();
        assert!(ok.plan.get("Node Type").is_some());
    }
}
