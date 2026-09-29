//! Wide columns: preview in SQL, value on demand.
//!
//! A `jsonb` column holding an uploaded file averages half a megabyte per row.
//! `SELECT *` over a 200-row page then detoasts ~100MB, hands it to sqlx,
//! serialises it across the IPC bridge and parses it again in the webview - for
//! a grid that can draw about forty characters of it. The 256KB cap in
//! `sql_util` trims that payload, but only after every byte has already crossed
//! the network: it protects the UI, not the fetch.
//!
//! So the cap moves into the query. Columns whose average width is over
//! `WIDE_COLUMN_AVG_BYTES` are selected as a stand-in instead of a value:
//!
//! ```sql
//! CASE WHEN pg_column_size("resume") > 262144
//!   THEN jsonb_build_object('__strokeOversize', true, 'dataType', 'jsonb', 'bytes', pg_column_size("resume"))
//!   ELSE jsonb_build_object('__strokeInline', to_jsonb("resume"))
//! END AS "resume"
//! ```
//!
//! Note what the oversize branch does NOT do: build a preview. The first cut of
//! this shipped `left(col::text, 16384)` in that branch and the page still took
//! eleven seconds, because `left` cannot take a prefix of a compressed
//! out-of-line value - it detoasts and decompresses the whole thing first, per
//! row. At 600KB a row that is most of a gigabyte of server-side work for text
//! nobody reads until they open the cell. So an over-cap value reports its size
//! and nothing else: `pg_column_size` reads the varlena header and touches no
//! TOAST chunk at all, and the text arrives through `fetch_cell_value` when
//! someone actually opens the cell.
//!
//! Rows under the cap come back whole (`__strokeInline` is unwrapped here, so
//! the frontend never sees it); rows over it come back as the sentinel the UI
//! already knows how to render, and `fetch_cell_value` loads the real value when
//! someone actually asks for it.
//!
//! Which columns are wide is measured, not assumed - and NOT taken from
//! `pg_stats.avg_width`, which is the obvious answer and the wrong one. ANALYZE
//! measures the datum as it sits in the tuple, so a column whose values are all
//! pushed out to TOAST reports the width of the 18-byte pointer. On the table
//! this was written for, `resume` averages 601KB and `pg_stats` says 18.
//!
//! So: the catalog names the candidates (a toastable type, or an inline width
//! already over the line), and one sampling query settles it -
//! `avg(pg_column_size(col))` over the first 500 rows. `pg_column_size` reads
//! the stored size out of the varlena header and does not fetch the value, so
//! the sample costs about half a millisecond and a few dozen buffers. The
//! answer is cached per table.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::sql_util::CELL_VALUE_CAP;

/// Average bytes per value past which a column is fetched as a preview.
///
/// 32KB is the point where one column outweighs a whole page of ordinary ones:
/// at 200 rows it is 6.4MB on the wire for a single column. Below it, shipping
/// the value is cheaper than the round trip to fetch it later.
pub const WIDE_COLUMN_AVG_BYTES: i64 = 32 * 1024;

/// Types that can hold a value worth capping and survive `::text` / `to_jsonb`
/// without changing meaning. Deliberately not `bytea` - it has its own hex path
/// through `cell_to_json`, and re-encoding it here would change what the grid
/// receives for the small rows too.
const WIDE_TYPES: &[&str] = &["json", "jsonb", "text", "varchar", "bpchar", "xml"];

/// Rows read to measure a candidate column. `pg_column_size` reads the stored
/// size from the varlena header rather than fetching the value, so this is a
/// few dozen buffers and under a millisecond even on the table that motivated
/// the feature.
const SAMPLE_ROWS: i64 = 500;

/// Below this much TOAST there is nowhere for a large value to be hiding, so
/// the sample is skipped entirely - which is the path almost every table takes.
const TOAST_SAMPLE_FLOOR: i64 = 1024 * 1024;

/// Stored size above which a row's TEXT length is checked as well.
///
/// `pg_column_size` is the COMPRESSED size, and these payloads compress
/// extraordinarily well - a jsonb holding a file as an array of byte integers
/// ran 8.4MB stored against 18.1MB of text, and rows well under the cap on disk
/// were shipping multiple megabytes each. The text check materialises the value,
/// so it is worth about 260ms a page on the table it was measured against, and
/// it is what makes the page size bounded rather than hopeful. Below the floor
/// no amount of compression can reach the cap, so those rows skip it.
const TEXT_CHECK_FLOOR: usize = 4 * 1024;

#[derive(Clone, Debug)]
pub struct WideColumn {
    pub name: String,
    /// Average bytes per value, from `pg_stats`.
    pub avg_width: i64,
    /// The column's declared type. The stand-in returns `jsonb` whatever the
    /// column is, so the real name has to travel with it or a wide `text`
    /// column would reach the grid claiming to be JSON.
    pub type_name: String,
}

/// The wide columns and the SELECT list they produce, cached together: the
/// column list a projection is built from is itself a round trip, and on a
/// server 80ms away one avoided round trip is worth more than the query it
/// replaces.
type CacheEntry = (Instant, Vec<WideColumn>, Option<String>);
static WIDE_CACHE: OnceLock<std::sync::Mutex<HashMap<String, CacheEntry>>> = OnceLock::new();
/// How long the decision about a table survives.
///
/// Short on purpose. What is cached here is the SELECT LIST, not data - but a
/// column list that is one minute out of date is a column the grid would not
/// draw, and "the row I just added a column for is missing" is indistinguishable
/// from stale data to the person looking at it. A minute costs one catalog round
/// trip per table per minute and removes the question.
const WIDE_CACHE_TTL: Duration = Duration::from_secs(60);

/// How long "nothing wide here" survives. That answer means a plain `SELECT *`,
/// which holds no column list and so can never drop a new column: the only thing
/// it can miss is a column that has grown past 32KB on average since, which
/// takes far longer than this.
const CLEAN_CACHE_TTL: Duration = Duration::from_secs(600);

fn fresh(at: &Instant, cols: &[WideColumn]) -> bool {
    at.elapsed() < if cols.is_empty() { CLEAN_CACHE_TTL } else { WIDE_CACHE_TTL }
}

fn cache_key(pool: &sqlx::PgPool, schema: &str, table: &str) -> String {
    let opts = pool.connect_options();
    format!(
        "{}:{}/{}\u{0}{schema}.{table}",
        opts.get_host(),
        opts.get_port(),
        opts.get_database().unwrap_or_default()
    )
}

/// The wide columns of a table, newest-stats-first and cached for five minutes.
/// An unanalyzed table (or a stats read that fails) reports none, which is the
/// pre-existing `SELECT *` behaviour.
pub async fn wide_columns(
    pool: &sqlx::PgPool,
    schema: &str,
    table: &str,
) -> Vec<WideColumn> {
    let key = cache_key(pool, schema, table);
    let cache = WIDE_CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    {
        if let Ok(map) = cache.lock() {
            if let Some((at, cols, _)) = map.get(&key) {
                if fresh(at, cols) {
                    return cols.clone();
                }
            }
        }
    }

    // Candidates: a toastable type, plus whatever the catalog already knows
    // about their inline width and the table's TOAST relation. One round trip.
    let candidates = sqlx::query(
        r#"
        SELECT
            a.attname::text,
            t.typname::text,
            COALESCE(s.avg_width, 0)::bigint,
            COALESCE(pg_total_relation_size(c.reltoastrelid), 0)::bigint
        FROM pg_catalog.pg_attribute a
        JOIN pg_catalog.pg_class c ON c.oid = a.attrelid
        JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_catalog.pg_type t ON t.oid = a.atttypid
        LEFT JOIN pg_catalog.pg_stats s
               ON s.schemaname = n.nspname AND s.tablename = c.relname AND s.attname = a.attname
        WHERE n.nspname = $1 AND c.relname = $2
          AND a.attnum > 0 AND NOT a.attisdropped
          AND t.typname = ANY($3)
        ORDER BY a.attnum
        "#,
    )
    .bind(schema)
    .bind(table)
    .bind(WIDE_TYPES)
    .fetch_all(pool)
    .await;

    let candidates = candidates.unwrap_or_default();
    let mut cols: Vec<WideColumn> = Vec::new();
    let mut to_sample: Vec<(String, String)> = Vec::new();
    let mut toast_size = 0_i64;
    for r in &candidates {
        use sqlx::Row;
        let (Ok(name), Ok(type_name), Ok(avg_width)) = (
            r.try_get::<String, _>(0),
            r.try_get::<String, _>(1),
            r.try_get::<i64, _>(2),
        ) else {
            continue;
        };
        toast_size = toast_size.max(r.try_get::<i64, _>(3).unwrap_or(0));
        if avg_width > WIDE_COLUMN_AVG_BYTES {
            // Wide without leaving the tuple - no sample needed to know.
            cols.push(WideColumn { name, avg_width, type_name });
        } else {
            to_sample.push((name, type_name));
        }
    }

    // Only sample when there is somewhere for the bytes to hide. A table with no
    // TOAST relation (or a trivial one) cannot be holding half-megabyte values,
    // and that is almost every table - so almost every table pays one catalog
    // query and nothing else.
    if !to_sample.is_empty() && toast_size > TOAST_SAMPLE_FLOOR {
        let selects: Vec<String> = to_sample
            .iter()
            .enumerate()
            .map(|(i, (name, _))| {
                format!(
                    "avg(pg_column_size(s.\"{}\"))::bigint AS c{i}",
                    name.replace('"', "\"\"")
                )
            })
            .collect();
        let cols_list: Vec<String> = to_sample
            .iter()
            .map(|(name, _)| format!("\"{}\"", name.replace('"', "\"\"")))
            .collect();
        let sql = format!(
            r#"SELECT {} FROM (SELECT {} FROM "{}"."{}" LIMIT {}) s"#,
            selects.join(", "),
            cols_list.join(", "),
            schema.replace('"', "\"\""),
            table.replace('"', "\"\""),
            SAMPLE_ROWS,
        );
        if let Ok(Some(row)) = sqlx::query(&sql).fetch_optional(pool).await {
            use sqlx::Row;
            for (i, (name, type_name)) in to_sample.into_iter().enumerate() {
                let avg = row.try_get::<Option<i64>, _>(i).ok().flatten().unwrap_or(0);
                if avg > WIDE_COLUMN_AVG_BYTES {
                    cols.push(WideColumn { name, avg_width: avg, type_name });
                }
            }
        }
    }

    if let Ok(mut map) = cache.lock() {
        if map.len() > 4096 {
            map.clear();
        }
        map.insert(key, (Instant::now(), cols.clone(), None));
    }
    cols
}

/// The SELECT list for a page of this table, and the columns it stands in for.
/// `None` when nothing is wide - which is the answer for almost every table,
/// and the one that leaves `SELECT *` alone.
///
/// Cached whole: after the first page, deciding what to select costs nothing.
pub async fn page_projection(
    pool: &sqlx::PgPool,
    schema: &str,
    table: &str,
) -> (Option<String>, Vec<WideColumn>) {
    let key = cache_key(pool, schema, table);
    {
        if let Some(cache) = WIDE_CACHE.get() {
            if let Ok(map) = cache.lock() {
                if let Some((at, cols, sql)) = map.get(&key) {
                    // A cached entry with no SQL yet still has to build one; a
                    // cached entry with no WIDE COLUMNS is already the answer.
                    if fresh(at, cols) && (sql.is_some() || cols.is_empty()) {
                        return (sql.clone(), cols.clone());
                    }
                }
            }
        }
    }

    let cols = wide_columns(pool, schema, table).await;
    if cols.is_empty() {
        return (None, cols);
    }
    let ordinal = super::query::fetch_table_column_names(pool, schema, table)
        .await
        .unwrap_or_default();
    let sql = projection(&ordinal, &cols);
    if let Some(cache) = WIDE_CACHE.get() {
        if let Ok(mut map) = cache.lock() {
            map.insert(key, (Instant::now(), cols.clone(), sql.clone()));
        }
    }
    (sql, cols)
}

/// Decide "nothing wide" for a whole schema in one catalog query, ahead of the
/// first open of any of its tables.
///
/// The first page of a table otherwise waits on `wide_columns` before its rows
/// query can even be sent: two round trips in a row, which to a far host
/// (Neon, Supabase, Nile at 265-535ms each) is the slow first open. Run in the
/// background when the table list loads. A table with no candidate columns, or
/// whose candidates are all narrow with no TOAST to sample, gets the same empty
/// answer `wide_columns` would give; anything else is left for the per-table
/// path, which samples it properly.
pub async fn prefetch_schema(pool: &sqlx::PgPool, schema: &str, tables: &[String]) {
    use sqlx::Row;
    let Ok(rows) = sqlx::query(
        r#"
        SELECT c.relname::text,
               MAX(COALESCE(s.avg_width, 0))::bigint,
               MAX(COALESCE(pg_total_relation_size(c.reltoastrelid), 0))::bigint
        FROM pg_catalog.pg_attribute a
        JOIN pg_catalog.pg_class c ON c.oid = a.attrelid
        JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_catalog.pg_type t ON t.oid = a.atttypid
        LEFT JOIN pg_catalog.pg_stats s
               ON s.schemaname = n.nspname AND s.tablename = c.relname AND s.attname = a.attname
        WHERE n.nspname = $1
          AND a.attnum > 0 AND NOT a.attisdropped
          AND t.typname = ANY($2)
        GROUP BY c.relname
        "#,
    )
    .bind(schema)
    .bind(WIDE_TYPES)
    .fetch_all(pool)
    .await
    else {
        return;
    };
    let mut needs_look: std::collections::HashSet<String> = std::collections::HashSet::new();
    for r in &rows {
        let (Ok(name), Ok(avg), Ok(toast)) =
            (r.try_get::<String, _>(0), r.try_get::<i64, _>(1), r.try_get::<i64, _>(2))
        else {
            continue;
        };
        if avg > WIDE_COLUMN_AVG_BYTES || toast > TOAST_SAMPLE_FLOOR {
            needs_look.insert(name);
        }
    }
    let cache = WIDE_CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let Ok(mut map) = cache.lock() else { return };
    if map.len() + tables.len() > 4096 {
        map.clear();
    }
    let now = Instant::now();
    for table in tables.iter().filter(|t| !needs_look.contains(*t)) {
        map.entry(cache_key(pool, schema, table)).or_insert_with(|| (now, Vec::new(), None));
    }
}

/// First open of a table: drop a cached PROJECTION, which names columns and can
/// go stale when one is added, but keep a cached "nothing wide", which means
/// `SELECT *` and cannot. Keeping it is what lets `prefetch_schema` save the
/// first open its extra round trip.
pub fn invalidate_projection(pool: &sqlx::PgPool, schema: &str, table: &str) {
    if let Some(cache) = WIDE_CACHE.get() {
        if let Ok(mut map) = cache.lock() {
            let key = cache_key(pool, schema, table);
            if map.get(&key).is_some_and(|(_, cols, _)| !cols.is_empty()) {
                map.remove(&key);
            }
        }
    }
}

/// Forget the cached stats for a table - after an ANALYZE, or a schema change
/// that could have changed what is wide.
pub fn invalidate(pool: &sqlx::PgPool, schema: &str, table: &str) {
    if let Some(cache) = WIDE_CACHE.get() {
        if let Ok(mut map) = cache.lock() {
            map.remove(&cache_key(pool, schema, table));
        }
    }
}

/// The SELECT list for a page, or `None` when no column is wide enough to be
/// worth rewriting the query for (the overwhelmingly common case, which keeps
/// the plain `SELECT *` path untouched).
///
/// `all_columns` must be in ordinal order - the grid's column order is the
/// result set's order.
pub fn projection(all_columns: &[String], wide: &[WideColumn]) -> Option<String> {
    if wide.is_empty() || all_columns.is_empty() {
        return None;
    }
    let mut any = false;
    let parts: Vec<String> = all_columns
        .iter()
        .map(|name| {
            let quoted = format!("\"{}\"", name.replace('"', "\"\""));
            if wide.iter().any(|w| &w.name == name) {
                any = true;
                let col_type = wide
                    .iter()
                    .find(|w| &w.name == name)
                    .map(|w| w.type_name.replace('\'', "''"))
                    .unwrap_or_default();
                format!(
                    "CASE WHEN pg_column_size({quoted}) > {cap} \
                     THEN jsonb_build_object('__strokeOversize', true, 'dataType', '{col_type}', \
                     'bytes', pg_column_size({quoted})) \
                     WHEN pg_column_size({quoted}) > {floor} AND length({quoted}::text) > {cap} \
                     THEN jsonb_build_object('__strokeOversize', true, 'dataType', '{col_type}', \
                     'bytes', octet_length({quoted}::text)) \
                     ELSE jsonb_build_object('__strokeInline', to_jsonb({quoted})) END AS {quoted}",
                    cap = CELL_VALUE_CAP,
                    floor = TEXT_CHECK_FLOOR,
                )
            } else {
                quoted
            }
        })
        .collect();
    any.then(|| parts.join(", "))
}

/// Unwrap the `__strokeInline` envelope a stand-in column wraps small values in.
/// Anything else - a real value, or the oversize sentinel - passes through.
///
/// One case needs care. The SQL branch is decided on `pg_column_size`, which is
/// the COMPRESSED stored size, while the row cap downstream measures the JSON
/// TEXT. A 550KB value that compresses to 120KB therefore takes the inline
/// branch and is then capped anyway - and the sentinel that produces carries the
/// envelope inside its preview, which is how `{"__strokeInline": {…` ended up
/// rendered in a grid cell. Recognise that and hand back a clean sentinel.
pub fn unwrap_inline(v: Value) -> Value {
    match v {
        Value::Object(ref map) if map.len() == 1 && map.contains_key("__strokeInline") => {
            map.get("__strokeInline").cloned().unwrap_or(Value::Null)
        }
        Value::Object(ref map)
            if map.get("__strokeOversize") == Some(&Value::Bool(true))
                && map
                    .get("preview")
                    .and_then(Value::as_str)
                    .is_some_and(|p| p.trim_start().starts_with("{\"__strokeInline\"")) =>
        {
            let mut out = map.clone();
            // The preview is of the envelope, not of the value: worse than
            // nothing, because it reads as the value's first bytes.
            out.remove("preview");
            Value::Object(out)
        }
        _ => v,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(name: &str) -> WideColumn {
        WideColumn { name: name.into(), avg_width: 529 * 1024, type_name: "jsonb".into() }
    }

    #[test]
    fn no_wide_columns_means_no_rewrite() {
        assert!(projection(&["id".into(), "name".into()], &[]).is_none());
    }

    #[test]
    fn wide_column_becomes_a_stand_in_and_the_rest_stay_themselves() {
        let sql = projection(
            &["id".into(), "resume".into(), "status".into()],
            &[wide("resume")],
        )
        .expect("a projection");
        assert!(sql.starts_with("\"id\", CASE WHEN pg_column_size(\"resume\")"));
        assert!(sql.contains("'__strokeOversize', true"));
        assert!(sql.contains("'dataType', 'jsonb'"));
        // No preview: building one would detoast every over-cap value on the page.
        assert!(!sql.contains("left("));
        // The compressed size is the cheap gate; the text length is the honest one.
        assert!(sql.contains("length(\"resume\"::text) >"));
        assert!(sql.contains("AS \"resume\""));
        assert!(sql.ends_with(", \"status\""));
        // Every column is still selected, in order.
        assert_eq!(sql.matches(" AS ").count(), 1);
    }

    #[test]
    fn a_quote_in_a_column_name_cannot_escape_the_identifier() {
        let sql = projection(&["we\"ird".into()], &[wide("we\"ird")]).expect("a projection");
        assert!(sql.contains("\"we\"\"ird\""));
    }

    #[test]
    fn a_capped_envelope_becomes_a_clean_sentinel() {
        // The compressed size took the inline branch; the text cap then wrapped
        // the envelope. The preview would otherwise render as `{"__strokeInline"…`
        // in a grid cell.
        let capped = serde_json::json!({
            "__strokeOversize": true,
            "bytes": 563_712,
            "dataType": "jsonb",
            "preview": "{\"__strokeInline\": {\"size\": 123524",
        });
        let out = unwrap_inline(capped);
        assert_eq!(out["__strokeOversize"], serde_json::json!(true));
        assert_eq!(out["bytes"], serde_json::json!(563_712));
        assert!(out.get("preview").is_none());
    }

    #[test]
    fn inline_values_unwrap_and_everything_else_passes_through() {
        let inline = serde_json::json!({ "__strokeInline": { "a": 1 } });
        assert_eq!(unwrap_inline(inline), serde_json::json!({ "a": 1 }));

        let null_inline = serde_json::json!({ "__strokeInline": null });
        assert_eq!(unwrap_inline(null_inline), Value::Null);

        let sentinel = serde_json::json!({ "__strokeOversize": true, "bytes": 9 });
        assert_eq!(unwrap_inline(sentinel.clone()), sentinel);

        let plain = serde_json::json!({ "__strokeInline": 1, "other": 2 });
        assert_eq!(unwrap_inline(plain.clone()), plain);
    }
}
