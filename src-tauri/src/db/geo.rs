//! Geo view - PostGIS layer discovery and map feature fetching.
//!
//! The map is fed by two commands. [`geo_overview`] answers "is this a spatial
//! database, and what can be mapped?" - cheap enough to run on connect.
//! [`geo_features`] fetches what belongs in the current viewport.
//!
//! Two constraints shape everything here:
//!
//! - **The table can be enormous.** A GPS ping table with a million rows must not
//!   send a million features to a canvas. When the viewport holds more rows than
//!   the caller asked for, the query collapses them onto a grid server-side and
//!   returns one weighted point per cell, so the shape of the data is visible at
//!   every zoom level and only the detail changes.
//! - **The index must be usable.** The bounding-box predicate is always applied
//!   to the *stored* column in its *own* SRID, never to a reprojected expression,
//!   so the GiST index still answers it. Reprojection to WGS84 happens only on
//!   the rows that survive.

use crate::db::connection::Db;
use super::connection::{require_conn, ActiveConnection};
use super::query::{build_where, RowFilter};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};

/// Hard ceiling on features returned in one fetch, whatever the caller asks for.
/// Above this the canvas is drawing more points than the viewport has pixels and
/// the payload is the bottleneck, so clustering is strictly better.
const MAX_FEATURES: i64 = 20_000;

/// One mappable column.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GeoLayer {
    pub schema: String,
    pub table: String,
    pub column: String,
    /// "geometry" or "geography" - they need different SQL, so the UI keeps it.
    pub kind: String,
    /// 0 when the column is untyped (`geometry` with no typmod), which PostGIS
    /// treats as "unknown CRS" and this module reads as already-WGS84.
    pub srid: i32,
    /// Declared geometry type: "POINT", "MULTIPOLYGON", or "GEOMETRY" when mixed.
    pub geom_type: String,
    /// Planner estimate, -1 when the table has never been analyzed.
    pub rows: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoOverview {
    /// False for every non-Postgres engine and for a Postgres without PostGIS.
    pub available: bool,
    pub version: Option<String>,
    pub layers: Vec<GeoLayer>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct GeoBbox {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoFeatures {
    /// "features" - real geometries, one per row; each carries its whole row in
    /// `properties`, so clicking one can show the record.
    /// "clusters" - grid-collapsed points carrying a `count`. The viewport held
    /// more rows than could be drawn.
    pub mode: String,
    pub features: Vec<Value>,
    /// Rows matching the filter inside the viewport.
    pub matched: i64,
    pub returned: i64,
    /// [min_x, min_y, max_x, max_y] in WGS84 for the whole filtered layer, for
    /// "zoom to fit". Only computed when asked for.
    pub extent: Option<[f64; 4]>,
    /// The table's column names, for the map's filter bar. Sent alongside the
    /// extent - i.e. once per layer selection, not per pan. Cluster rows carry
    /// only a count, so without this a table large enough to *always* cluster
    /// could never offer a filter at all.
    pub columns: Vec<String>,
    pub query_ms: u64,
    pub sql: String,
}

/// PostGIS puts its own tables in these schemas; they are plumbing, not data.
const HIDDEN_SCHEMAS: &str = "('information_schema', 'pg_catalog', 'topology', 'tiger', 'tiger_data')";

fn pg_pool(state: &Db<'_>) -> Result<Option<PgPool>, String> {
    match require_conn(state)? {
        ActiveConnection::Postgres(pool) => Ok(Some(pool)),
        _ => Ok(None),
    }
}

/// Bind a scoped query's parameters in the order the SQL is built with: the
/// user's filter values, then the viewport corners.
fn bind_scope<'q>(
    sql: &'q str,
    filter_binds: &'q [String],
    bbox: Option<GeoBbox>,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    let mut q = sqlx::query(sql);
    for value in filter_binds {
        q = q.bind(value.as_str());
    }
    if let Some(b) = bbox {
        q = q.bind(b.min_x).bind(b.min_y).bind(b.max_x).bind(b.max_y);
    }
    q
}

fn unavailable() -> GeoOverview {
    GeoOverview { available: false, version: None, layers: Vec::new() }
}

/// Is PostGIS installed, and what can be mapped? Safe to call on any connection:
/// a non-Postgres engine, or a Postgres without the extension, reports
/// `available: false` rather than failing.
pub async fn geo_overview(state: Db<'_>) -> Result<GeoOverview, String> {
    let Some(pool) = pg_pool(&state)? else { return Ok(unavailable()) };

    let version: Option<String> =
        sqlx::query_scalar("SELECT extversion FROM pg_extension WHERE extname = 'postgis'")
            .fetch_optional(&pool)
            .await
            .map_err(|e| format!("Failed to check for PostGIS: {e}"))?
            .flatten();

    let Some(version) = version else { return Ok(unavailable()) };

    // geometry_columns and geography_columns are PostGIS's own catalog views and
    // already resolve typmod into srid + type, including for untyped columns.
    let sql = format!(
        r#"
        SELECT schema_name, table_name, column_name, kind, srid, geom_type,
               COALESCE(c.reltuples, -1)::bigint AS row_estimate
        FROM (
            SELECT f_table_schema AS schema_name, f_table_name AS table_name,
                   f_geometry_column AS column_name, 'geometry' AS kind,
                   srid, type AS geom_type
            FROM geometry_columns
            UNION ALL
            SELECT f_table_schema, f_table_name, f_geography_column, 'geography',
                   srid, type
            FROM geography_columns
        ) g
        LEFT JOIN pg_catalog.pg_namespace n ON n.nspname = g.schema_name
        LEFT JOIN pg_catalog.pg_class c
               ON c.relname = g.table_name AND c.relnamespace = n.oid
        WHERE g.schema_name NOT IN {HIDDEN_SCHEMAS}
        ORDER BY row_estimate DESC NULLS LAST, g.schema_name, g.table_name, g.column_name
        "#
    );

    let rows = sqlx::query(&sql)
        .fetch_all(&pool)
        .await
        .map_err(|e| format!("Failed to list spatial columns: {e}"))?;

    let layers = rows
        .iter()
        .filter_map(|r| {
            Some(GeoLayer {
                schema: r.try_get::<String, _>(0).ok()?,
                table: r.try_get::<String, _>(1).ok()?,
                column: r.try_get::<String, _>(2).ok()?,
                kind: r.try_get::<String, _>(3).ok()?,
                srid: r.try_get::<i32, _>(4).unwrap_or(0),
                geom_type: r.try_get::<String, _>(5).unwrap_or_else(|_| "GEOMETRY".into()),
                rows: r.try_get::<i64, _>(6).unwrap_or(-1),
            })
        })
        .collect();

    Ok(GeoOverview { available: true, version: Some(version), layers })
}

/// The stored column reprojected to WGS84, which is what GeoJSON must be in.
///
/// `geography` is WGS84 by definition and only needs the cast. A `geometry` with
/// SRID 0 has no declared CRS; PostGIS refuses to transform it, and treating its
/// coordinates as degrees is the only reading that can put it on a map.
fn wgs84_expr(col: &str, kind: &str, srid: i32) -> String {
    match (kind, srid) {
        ("geography", _) => format!("{col}::geometry"),
        (_, 4326) => col.to_string(),
        (_, 0) => format!("ST_SetSRID({col}, 4326)"),
        (_, _) => format!("ST_Transform({col}, 4326)"),
    }
}

/// The GeoJSON-representable form of a geometry expression.
///
/// A column with a declared type can only ever hold that type, so it needs
/// nothing but dropping Z/M - GeoJSON has no third ordinate, and PostGIS emits
/// one if it isn't asked otherwise.
///
/// An *untyped* `geometry` column can hold anything PostGIS can parse, including
/// shapes GeoJSON cannot express at all: curves, polyhedral surfaces, TINs, and
/// collections nested inside collections. `ST_AsGeoJSON` raises on each of them,
/// and one such row would fail the entire viewport query rather than its own
/// feature. `ST_Dump` flattens recursively and `ST_CurveToLine` approximates the
/// curves; together they cover every geometry type PostGIS has.
fn geojson_expr(g: &str, geom_type: &str) -> String {
    let simple = matches!(
        geom_type.to_ascii_uppercase().as_str(),
        "POINT" | "LINESTRING" | "POLYGON" | "MULTIPOINT" | "MULTILINESTRING" | "MULTIPOLYGON"
    );
    if simple {
        format!("ST_Force2D({g})")
    } else {
        format!("ST_Force2D(ST_CurveToLine(ST_Collect(ARRAY(SELECT (ST_Dump({g})).geom))))")
    }
}

/// A bounding-box predicate against the *stored* column, so the GiST index can
/// answer it. The viewport arrives in WGS84 and is pushed into the column's own
/// SRID - the opposite direction to [`wgs84_expr`], and the reason a map over a
/// SRID-3857 table is still index-driven.
fn bbox_predicate(col: &str, kind: &str, srid: i32, p: usize) -> String {
    let env = format!("ST_MakeEnvelope(${p}, ${}, ${}, ${}, 4326)", p + 1, p + 2, p + 3);
    match (kind, srid) {
        ("geography", _) => format!("{col} && {env}::geography"),
        (_, 4326) => format!("{col} && {env}"),
        // No declared CRS: compare in the column's own bare coordinate space.
        (_, 0) => format!("{col} && ST_SetSRID({env}, 0)"),
        (_, s) => format!("{col} && ST_Transform({env}, {s})"),
    }
}

/// Column name → Postgres type name, for filling in a filter's missing type.
///
/// Without a type, `build_where` falls back to comparing the column as text, so
/// `population > 900000` becomes a *lexicographic* comparison in which "9…"
/// sorts after "1…" and the map quietly shows the wrong rows. `typname` is the
/// short form (`int4`, `timestamptz`, `bool`), which is one of the two spellings
/// `pg_param_cast` already understands.
async fn column_types(
    pool: &PgPool,
    schema: &str,
    table: &str,
) -> std::collections::HashMap<String, String> {
    let rows = sqlx::query(
        r#"
        SELECT a.attname::text, t.typname::text
        FROM pg_catalog.pg_attribute a
        JOIN pg_catalog.pg_class c ON c.oid = a.attrelid
        JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_catalog.pg_type t ON t.oid = a.atttypid
        WHERE n.nspname = $1 AND c.relname = $2
          AND a.attnum > 0 AND NOT a.attisdropped
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.iter()
        .filter_map(|r| Some((r.try_get::<String, _>(0).ok()?, r.try_get::<String, _>(1).ok()?)))
        .collect()
}

/// Every geometry/geography column of a table. They are stripped from the JSON
/// properties: a geometry serialises as its full hex WKB, which would dwarf the
/// feature it is attached to and says nothing the map isn't already showing.
async fn geom_column_names(
    pool: &PgPool,
    schema: &str,
    table: &str,
) -> Result<Vec<String>, String> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT a.attname::text
        FROM pg_catalog.pg_attribute a
        JOIN pg_catalog.pg_class c ON c.oid = a.attrelid
        JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_catalog.pg_type t ON t.oid = a.atttypid
        WHERE n.nspname = $1 AND c.relname = $2
          AND a.attnum > 0 AND NOT a.attisdropped
          AND t.typname IN ('geometry', 'geography', 'raster')
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to inspect spatial columns: {e}"))
}

/// The layer's full WGS84 extent, for "zoom to fit".
///
/// Unfiltered, this reads the GiST index's own statistics - instant, and the
/// only affordable answer on a large table. It is an estimate and can be missing
/// (never analyzed, no index), in which case, and whenever a filter is active,
/// the exact extent is measured. `ST_EstimatedExtent` returns `box2d`, a type
/// with no binary output, so the corners are pulled out as plain floats.
async fn layer_extent(
    pool: &PgPool,
    schema: &str,
    table: &str,
    column: &str,
    kind: &str,
    srid: i32,
    table_ref: &str,
    where_clause: &super::query::WhereClause,
) -> Option<[f64; 4]> {
    let quoted = format!(r#""{column}""#);
    let g = wgs84_expr(&quoted, kind, srid);

    if where_clause.sql.is_empty() && kind == "geometry" {
        let sql = if srid == 4326 || srid == 0 {
            "SELECT ST_XMin(e), ST_YMin(e), ST_XMax(e), ST_YMax(e) \
             FROM ST_EstimatedExtent($1, $2, $3) e"
                .to_string()
        } else {
            format!(
                "SELECT ST_XMin(e), ST_YMin(e), ST_XMax(e), ST_YMax(e) \
                 FROM ST_Transform(ST_SetSRID(ST_EstimatedExtent($1, $2, $3), {srid}), 4326) e"
            )
        };
        if let Ok(Some(row)) = sqlx::query(&sql)
            .bind(schema)
            .bind(table)
            .bind(column)
            .fetch_optional(pool)
            .await
        {
            let corners = (
                row.try_get::<f64, _>(0),
                row.try_get::<f64, _>(1),
                row.try_get::<f64, _>(2),
                row.try_get::<f64, _>(3),
            );
            if let (Ok(a), Ok(b), Ok(c), Ok(d)) = corners {
                return Some([a, b, c, d]);
            }
        }
    }

    let sql = format!(
        "SELECT ST_XMin(e), ST_YMin(e), ST_XMax(e), ST_YMax(e) \
         FROM (SELECT ST_Extent({g}) AS e FROM {table_ref}{}) x",
        where_clause.sql
    );
    let mut q = sqlx::query(&sql);
    for value in &where_clause.binds {
        q = q.bind(value.as_str());
    }
    let row = q.fetch_optional(pool).await.ok()??;
    let corners = (
        row.try_get::<f64, _>(0).ok()?,
        row.try_get::<f64, _>(1).ok()?,
        row.try_get::<f64, _>(2).ok()?,
        row.try_get::<f64, _>(3).ok()?,
    );
    Some([corners.0, corners.1, corners.2, corners.3])
}

/// Features for the current viewport.
///
/// `simplify` is a tolerance in degrees, normally "one screen pixel" - dropping
/// vertices the viewer could not resolve anyway is what keeps a 30k-row road
/// layer interactive. `cluster_cell` is the grid size used when the viewport
/// holds more than `limit` rows.
#[allow(clippy::too_many_arguments)]
pub async fn geo_features(
    state: Db<'_>,
    schema: String,
    table: String,
    column: String,
    kind: String,
    srid: i32,
    // The column's declared geometry type, straight from `GeoLayer` - it decides
    // whether the rows need the defensive GeoJSON conversion.
    geom_type: String,
    bbox: Option<GeoBbox>,
    limit: i64,
    simplify: f64,
    cluster_cell: f64,
    filters: Option<Vec<RowFilter>>,
    include_extent: bool,
) -> Result<GeoFeatures, String> {
    let Some(pool) = pg_pool(&state)? else {
        return Err("The map view is only available on PostgreSQL with PostGIS".into());
    };
    let started = std::time::Instant::now();

    super::schema::validate_ident(&schema)?;
    super::schema::validate_ident(&table)?;
    super::schema::validate_ident(&column)?;
    if kind != "geometry" && kind != "geography" {
        return Err(format!("Unknown spatial column kind: {kind}"));
    }
    let limit = limit.clamp(1, MAX_FEATURES);

    let quoted = format!(r#""{column}""#);
    let table_ref = format!(r#""{schema}"."{table}""#);
    let filters = filters.unwrap_or_default();
    let where_clause = if filters.is_empty() {
        super::query::WhereClause { sql: String::new(), binds: Vec::new() }
    } else {
        let (columns, types) = tokio::join!(
            super::query::fetch_table_column_names(&pool, &schema, &table),
            column_types(&pool, &schema, &table),
        );
        // The map's filter bar knows column names but not their types; supplying
        // the type here is what makes a numeric or date comparison numeric or
        // date rather than a string sort.
        let filters: Vec<RowFilter> = filters
            .into_iter()
            .map(|mut f| {
                if f.data_type.is_none() {
                    f.data_type = types.get(&f.column).cloned();
                }
                f
            })
            .collect();
        build_where(&columns?, None, false, false, &filters)?
    };

    // Viewport clause, appended to the user's filter. A missing bbox means "the
    // whole layer" and adds no predicate at all.
    let bbox_param = where_clause.binds.len() + 1;
    let viewport = match &bbox {
        Some(_) => {
            let connector = if where_clause.sql.is_empty() { " WHERE" } else { " AND" };
            format!("{connector} {}", bbox_predicate(&quoted, &kind, srid, bbox_param))
        }
        None => String::new(),
    };
    // A NULL geometry has nothing to draw and would otherwise cost a feature slot.
    let connector = if where_clause.sql.is_empty() && bbox.is_none() { " WHERE" } else { " AND" };
    let scope = format!("{}{viewport}{connector} {quoted} IS NOT NULL", where_clause.sql);

    let count_sql = format!("SELECT COUNT(*)::bigint FROM {table_ref}{scope}");
    let matched: i64 = bind_scope(&count_sql, &where_clause.binds, bbox)
        .fetch_one(&pool)
        .await
        .and_then(|r| r.try_get::<i64, _>(0))
        .map_err(|e| format!("Failed to count features: {e}"))?;

    let safe = geojson_expr(&wgs84_expr(&quoted, &kind, srid), &geom_type);
    let cluster = matched > limit;

    let data_sql = if cluster {
        // One weighted point per grid cell. Snapping the centroid rather than the
        // geometry means lines and polygons cluster by where they are, not by how
        // many vertices they happen to have. The cell is computed in a subquery so
        // the expression is evaluated once per row instead of once per reference.
        let cell = if cluster_cell.is_finite() && cluster_cell > 0.0 { cluster_cell } else { 1.0 };
        // The caller's cell size is "so many screen pixels wide", expressed in
        // degrees of LONGITUDE. Snapping latitude by the same number of degrees
        // makes every cell taller than it is wide once the map is drawn, because
        // Mercator stretches latitude by 1/cos(φ) - 1.6x across Europe, 2x by
        // 60°N. The visible result is each place breaking into a vertical stack
        // of separate blobs with gaps the marks can't bridge.
        //
        // So the grid is built on the Mercator ordinate, where a cell is square
        // on screen at every latitude. `ln(tan(π/4 + φ/2))` is that projection;
        // one degree of longitude is `radians(1)` of it. Latitude is clamped to
        // the Mercator cut first, or a pole-adjacent row sends tan() to infinity.
        let cell_merc = cell.to_radians();
        format!(
            "SELECT ST_AsGeoJSON(ST_SetSRID(ST_MakePoint(avg(x), avg(y)), 4326), 6) AS geometry, \
                    COUNT(*)::bigint AS n \
             FROM (SELECT x, y, \
                          ST_SnapToGrid(ST_MakePoint(x, ln(tan(pi()/4 + radians(y)/2))), \
                                        {cell}, {cell_merc}) AS cell \
                   FROM (SELECT ST_X(c) AS x, \
                                LEAST(85.05, GREATEST(-85.05, ST_Y(c))) AS y \
                         FROM (SELECT ST_Centroid({safe}) AS c \
                               FROM {table_ref} t{scope}) p0) p) s \
             GROUP BY cell \
             ORDER BY n DESC \
             LIMIT {limit}"
        )
    } else {
        // `preserveCollapsed` keeps a degenerate stand-in for shapes smaller than
        // the tolerance. Without it ST_Simplify returns NULL for them and small
        // features silently vanish as you zoom out - the worst possible failure
        // for a map, because nothing looks wrong.
        let geom = if simplify.is_finite() && simplify > 0.0 {
            format!("ST_Simplify({safe}, {simplify}, true)")
        } else {
            safe.clone()
        };
        let drop_cols = geom_column_names(&pool, &schema, &table).await?;
        let drop_list = drop_cols
            .iter()
            .map(|c| format!("'{}'", c.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(", ");
        let props = if drop_list.is_empty() {
            "to_jsonb(t)".to_string()
        } else {
            format!("to_jsonb(t) - ARRAY[{drop_list}]::text[]")
        };
        format!(
            "SELECT ST_AsGeoJSON({geom}, 6) AS geometry, {props} AS properties \
             FROM {table_ref} t{scope} \
             LIMIT {limit}"
        )
    };

    let rows = bind_scope(&data_sql, &where_clause.binds, bbox)
        .fetch_all(&pool)
        .await
        .map_err(|e| format!("Failed to fetch map features: {e}"))?;

    let features: Vec<Value> = rows
        .iter()
        .filter_map(|r| {
            let raw: String = r.try_get(0).ok()?;
            let geometry: Value = serde_json::from_str(&raw).ok()?;
            let properties = if cluster {
                json!({ "count": r.try_get::<i64, _>(1).unwrap_or(0) })
            } else {
                r.try_get::<Value, _>(1).unwrap_or_else(|_| json!({}))
            };
            Some(json!({ "type": "Feature", "geometry": geometry, "properties": properties }))
        })
        .collect();

    let (extent, columns) = if include_extent {
        tokio::join!(
            layer_extent(&pool, &schema, &table, &column, &kind, srid, &table_ref, &where_clause),
            async { super::query::fetch_table_column_names(&pool, &schema, &table).await.unwrap_or_default() },
        )
    } else {
        (None, Vec::new())
    };

    Ok(GeoFeatures {
        mode: if cluster { "clusters".into() } else { "features".into() },
        returned: features.len() as i64,
        features,
        matched,
        extent,
        columns,
        query_ms: started.elapsed().as_millis() as u64,
        sql: format!("{data_sql}\n{count_sql}"),
    })
}
