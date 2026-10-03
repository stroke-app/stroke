pub mod backup;
pub mod clickhouse;
pub mod posthog;
pub mod connection;
pub mod d1;
pub mod duckdb;
pub mod mssql;
pub mod explain;
pub mod advisor;
pub mod insights;
pub mod libsql;
pub mod live;
pub mod mysql;
pub mod redis;
#[cfg(test)]
mod dialect_matrix;
mod query;
pub mod result_store;
mod schema;
pub mod sql_util;
pub mod wide_columns;
pub mod sqlite;
pub mod ssh_tunnel;
pub mod local_scan;
pub mod pg_ext_types;
pub mod geo;
pub mod import;
pub mod tx;

pub use connection::{
    connect, connect_clickhouse, connect_d1, connect_duckdb, connect_libsql, connect_mssql, connect_mysql, connect_redis, connect_sqlite, disconnect,
    test_clickhouse_connection, test_connection, test_d1_connection, test_duckdb_connection, test_libsql_connection, test_mssql_connection, test_mysql_connection, test_redis_connection, test_sqlite_connection,
    ActiveConnection, AnyConnectionConfig, ConnectionConfig, D1Config, DbState, LibSqlConfig, SqliteConfig,
};
pub use explain::{explain_pg, explain_mysql, explain_sqlite, explain_from_text_lines, explain_from_sqlite_plan, ExplainResult};
pub use insights::{
    instance_activity, instance_config, instance_replication, instance_set_config, instance_state,
    instance_version, ConfigSetting, InstanceActivity, InstanceReplication, InstanceState,
    InstanceVersion, SetConfigResult,
};
pub use geo::{geo_features, geo_overview, GeoBbox, GeoFeatures, GeoOverview};
pub use ssh_tunnel::TunnelState;
pub use query::{
    delete_table_row, delete_table_rows, execute_ddl, execute_sql, execute_sql_multi, execute_sql_on_conn, execute_sql_script, StatementOutcome,
    fetch_cell_value, CellValueResult, get_column_stats, get_table_rows, count_table_rows, insert_table_row, update_table_cell, ping_connection,
    ColumnStats, InsertRowResult, KeysetCursor, RowChunk, RowFilter, RowSink, SortSpec, SqlResult, TableRows,
};
pub use schema::{
    list_schemas, list_tables, list_schemas_on_conn, list_tables_on_conn,
    list_indexes, list_enums, list_functions, list_triggers, list_sequences,
    table_row_counts, truncate_table, drop_table, get_table_column_structure, get_schema_column_structure, get_incoming_foreign_keys,
    get_table_ddl, get_table_ddl_on_conn,
    TableInfo, TableRowCount, IndexInfo, EnumInfo, FunctionInfo, TriggerInfo, SequenceInfo, ColumnStructureRow, TableColumnStructure, IncomingForeignKey,
};
