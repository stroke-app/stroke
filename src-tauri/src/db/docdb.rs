//! Document and wide-column engines that share one connection slot
//! (`ActiveConnection::Doc`): CouchDB, Cassandra, Firestore, MongoDB,
//! Elasticsearch/OpenSearch, Convex. Each keeps its
//! own module; this file only routes the calls every engine answers.

use super::query::SqlResult;
use super::schema::TableInfo;

#[derive(Clone)]
pub enum DocConn {
    Couch(super::couchdb::CouchConfig),
    Cassandra(super::cassandra::CassandraHandle),
    Firestore(super::firestore::FirestoreConfig),
    Mongo(super::mongo::MongoHandle),
    Elastic(super::elastic::EsConfig),
    Neo4j(super::neo4j::Neo4jConfig),
    Influx(super::influx::InfluxConfig),
    Convex(super::convex::ConvexConfig),
}

impl DocConn {
    pub fn driver(&self) -> &'static str {
        match self {
            Self::Couch(_) => "couchdb",
            Self::Cassandra(_) => "cassandra",
            Self::Firestore(_) => "firestore",
            Self::Mongo(_) => "mongodb",
            Self::Elastic(c) if c.is_opensearch() => "opensearch",
            Self::Elastic(_) => "elasticsearch",
            Self::Neo4j(_) => "neo4j",
            Self::Influx(_) => "influxdb",
            Self::Convex(_) => "convex",
        }
    }
}

/// A statement from the query console (Mango shell syntax, CQL, ...).
pub async fn query(conn: &DocConn, stmt: &str) -> Result<SqlResult, String> {
    match conn {
        DocConn::Couch(c) => super::couchdb::query(c, stmt).await,
        DocConn::Cassandra(s) => super::cassandra::query(s, stmt).await,
        DocConn::Firestore(c) => super::firestore::query(c, stmt).await,
        DocConn::Mongo(m) => super::mongo::query(m, stmt).await,
        DocConn::Elastic(c) => super::elastic::query(c, stmt).await,
        DocConn::Neo4j(c) => super::neo4j::query(c, stmt).await,
        DocConn::Influx(c) => super::influx::query(c, stmt).await,
        DocConn::Convex(c) => super::convex::query(c, stmt).await,
    }
}

/// A statement from an agent: like `query`, but Elasticsearch and OpenSearch
/// may write when the MCP gate allowed it (the other consoles have their own
/// write paths or only read).
pub async fn query_as(conn: &DocConn, stmt: &str, allow_write: bool) -> Result<SqlResult, String> {
    match conn {
        DocConn::Elastic(c) => super::elastic::query_as(c, stmt, allow_write).await,
        DocConn::Convex(c) => super::convex::query_as(c, stmt, allow_write).await,
        _ => query(conn, stmt).await,
    }
}

/// Collections/tables as the schema cache's table list (names only).
pub async fn list_tables(conn: &DocConn) -> Result<Vec<TableInfo>, String> {
    let names = match conn {
        DocConn::Couch(c) => super::couchdb::list_databases(c).await?,
        DocConn::Cassandra(s) => super::cassandra::list_tables(s).await?,
        DocConn::Firestore(c) => super::firestore::list_collections(c).await?,
        DocConn::Mongo(m) => super::mongo::list_collections(m).await?,
        DocConn::Elastic(c) => super::elastic::list_indices(c).await?.into_iter().map(|i| i.name).collect(),
        DocConn::Neo4j(c) => super::neo4j::labels(c).await?.into_iter().map(|(n, _)| n).collect(),
        DocConn::Influx(c) => super::influx::measurements(c).await?,
        DocConn::Convex(c) => super::convex::list_tables(c).await?,
    };
    Ok(names
        .into_iter()
        .map(|name| TableInfo { name, row_count: 0, kind: "table".into(), rls_enabled: None, created_at: None, create_order: None, system: None })
        .collect())
}

// ── Sidebar catalog: schemas, then tables in one schema ──────────────────────

/// The single schema of engines without one (CouchDB, Firestore), like SQLite's `main`.
pub const FLAT_SCHEMA: &str = "main";

/// MongoDB databases, Cassandra keyspaces; `main` for the flat engines.
pub async fn list_schemas(conn: &DocConn) -> Result<Vec<String>, String> {
    match conn {
        DocConn::Mongo(m) => super::mongo::list_databases(m).await,
        DocConn::Cassandra(s) => super::cassandra::list_keyspaces(s).await,
        DocConn::Couch(_) | DocConn::Firestore(_) | DocConn::Elastic(_) | DocConn::Neo4j(_) | DocConn::Influx(_) | DocConn::Convex(_) => Ok(vec![FLAT_SCHEMA.to_string()]),
    }
}

fn info(name: String, row_count: i64) -> TableInfo {
    TableInfo { name, row_count, kind: "table".into(), rls_enabled: None, created_at: None, create_order: None, system: None }
}

/// Tables in one schema, with counts where they're cheap (-1 = unknown).
pub async fn list_tables_in(conn: &DocConn, schema: &str) -> Result<Vec<TableInfo>, String> {
    use futures::stream::{self, StreamExt};
    match conn {
        DocConn::Mongo(m) => {
            let names = super::mongo::collections_in(m, schema).await?;
            let db = m.client.database(schema);
            Ok(stream::iter(names.into_iter().map(|n| {
                let c = db.collection::<mongodb::bson::Document>(&n);
                async move {
                    let count = c.estimated_document_count().await.map(|x| x as i64).unwrap_or(-1);
                    info(n, count)
                }
            }))
            .buffered(8)
            .collect()
            .await)
        }
        DocConn::Cassandra(s) => Ok(super::cassandra::tables_in(s, schema).await?.into_iter().map(|n| info(n, -1)).collect()),
        DocConn::Couch(c) => {
            let names = super::couchdb::list_databases(c).await?;
            Ok(stream::iter(names.into_iter().map(|n| async move {
                let count = super::couchdb::request(c, "GET", &super::couchdb::enc(&n), None)
                    .await
                    .ok()
                    .and_then(|v| v["doc_count"].as_i64())
                    .unwrap_or(-1);
                info(n, count)
            }))
            .buffered(8)
            .collect()
            .await)
        }
        DocConn::Firestore(c) => Ok(super::firestore::list_collections(c).await?.into_iter().map(|n| info(n, -1)).collect()),
        DocConn::Elastic(c) => Ok(super::elastic::list_indices(c).await?.into_iter().map(|i| info(i.name, i.docs)).collect()),
        DocConn::Neo4j(c) => Ok(super::neo4j::labels(c).await?.into_iter().map(|(n, k)| info(n, k)).collect()),
        DocConn::Influx(c) => Ok(super::influx::measurements(c).await?.into_iter().map(|n| info(n, -1)).collect()),
        DocConn::Convex(c) => {
            let names = super::convex::list_tables(c).await?;
            Ok(stream::iter(names.into_iter().map(|n| async move {
                let count = super::convex::table_size(c, &n).await.unwrap_or(-1);
                info(n, count)
            }))
            .buffered(8)
            .collect()
            .await)
        }
    }
}

/// The schema the query editor's unqualified names refer to.
pub async fn use_schema(conn: &DocConn, schema: &str) -> Result<(), String> {
    match conn {
        DocConn::Mongo(m) => {
            super::mongo::use_db(m, schema);
            Ok(())
        }
        DocConn::Cassandra(s) => {
            // The UI starts every connection on `public` (Postgres's default)
            // before it knows the real keyspaces. Asking Cassandra to USE a
            // keyspace that isn't there failed on every pooled connection, the
            // driver kept retrying it, and the sidebar waited on all of that.
            // A keyspace that doesn't exist is simply not switched to.
            if !super::cassandra::list_keyspaces(s).await?.iter().any(|k| k == schema) {
                return Ok(());
            }
            s.use_keyspace(schema, true).await.map_err(|e| format!("Keyspace \"{schema}\": {e}"))
        }
        DocConn::Couch(_) | DocConn::Firestore(_) | DocConn::Elastic(_) | DocConn::Neo4j(_) | DocConn::Influx(_) | DocConn::Convex(_) => Ok(()),
    }
}

// ── Tree: columns and indexes where the engine keeps them cheaply ────────────

/// Columns of one collection: Cassandra's typed columns; nothing for the
/// schemaless engines.
pub async fn columns_in(conn: &DocConn, schema: &str, table: &str) -> Result<Vec<super::schema::ColumnStructureRow>, String> {
    match conn {
        DocConn::Cassandra(s) => {
            let mut cols = super::cassandra::keyspace_columns(s, schema).await?;
            cols.retain(|c| c.table == table);
            Ok(super::cassandra::columns_by_table(cols).pop().map(|t| t.columns).unwrap_or_default())
        }
        // Mappings are the index's columns: typed fields, nested ones dotted.
        DocConn::Elastic(c) => super::elastic::index_columns(c, table).await,
        DocConn::Mongo(_) | DocConn::Couch(_) | DocConn::Firestore(_) | DocConn::Neo4j(_) | DocConn::Influx(_) | DocConn::Convex(_) => Ok(vec![]),
    }
}

/// Every table's columns in a keyspace (Cassandra, one statement); the
/// schemaless engines have none.
pub async fn schema_columns_in(conn: &DocConn, schema: &str) -> Result<Vec<super::schema::TableColumnStructure>, String> {
    match conn {
        DocConn::Cassandra(s) => Ok(super::cassandra::columns_by_table(super::cassandra::keyspace_columns(s, schema).await?)),
        DocConn::Mongo(_) | DocConn::Couch(_) | DocConn::Firestore(_) | DocConn::Elastic(_) | DocConn::Neo4j(_) | DocConn::Influx(_) | DocConn::Convex(_) => Ok(vec![]),
    }
}

/// Indexes: a whole keyspace for Cassandra; one collection (when named) for
/// MongoDB and CouchDB, which keep them per collection; none for Firestore,
/// whose composite indexes live behind the admin API.
pub async fn indexes_in(conn: &DocConn, schema: &str, table: Option<&str>) -> Result<Vec<super::schema::IndexInfo>, String> {
    match (conn, table) {
        (DocConn::Cassandra(s), _) => super::cassandra::keyspace_indexes(s, schema).await,
        (DocConn::Mongo(m), Some(t)) => super::mongo::index_list(m, schema, t).await,
        (DocConn::Couch(c), Some(t)) => super::couchdb::index_list(c, t).await,
        _ => Ok(vec![]),
    }
}
