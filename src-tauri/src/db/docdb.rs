//! Document and wide-column engines that share one connection slot
//! (`ActiveConnection::Doc`): CouchDB, Cassandra, Firestore. Each keeps its
//! own module; this file only routes the calls every engine answers.

use super::query::SqlResult;
use super::schema::TableInfo;

#[derive(Clone)]
pub enum DocConn {
    Couch(super::couchdb::CouchConfig),
    Cassandra(super::cassandra::CassandraHandle),
    Firestore(super::firestore::FirestoreConfig),
    Mongo(super::mongo::MongoHandle),
}

impl DocConn {
    pub fn driver(&self) -> &'static str {
        match self {
            Self::Couch(_) => "couchdb",
            Self::Cassandra(_) => "cassandra",
            Self::Firestore(_) => "firestore",
            Self::Mongo(_) => "mongodb",
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
    }
}

/// Collections/tables as the schema cache's table list (names only).
pub async fn list_tables(conn: &DocConn) -> Result<Vec<TableInfo>, String> {
    let names = match conn {
        DocConn::Couch(c) => super::couchdb::list_databases(c).await?,
        DocConn::Cassandra(s) => super::cassandra::list_tables(s).await?,
        DocConn::Firestore(c) => super::firestore::list_collections(c).await?,
        DocConn::Mongo(m) => super::mongo::list_collections(m).await?,
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
        DocConn::Couch(_) | DocConn::Firestore(_) => Ok(vec![FLAT_SCHEMA.to_string()]),
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
        DocConn::Couch(_) | DocConn::Firestore(_) => Ok(()),
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
        DocConn::Mongo(_) | DocConn::Couch(_) | DocConn::Firestore(_) => Ok(vec![]),
    }
}

/// Every table's columns in a keyspace (Cassandra, one statement); the
/// schemaless engines have none.
pub async fn schema_columns_in(conn: &DocConn, schema: &str) -> Result<Vec<super::schema::TableColumnStructure>, String> {
    match conn {
        DocConn::Cassandra(s) => Ok(super::cassandra::columns_by_table(super::cassandra::keyspace_columns(s, schema).await?)),
        DocConn::Mongo(_) | DocConn::Couch(_) | DocConn::Firestore(_) => Ok(vec![]),
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
