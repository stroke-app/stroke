/**
 * Quoting names in SQL the app writes for you (Open in SQL editor), under
 * Settings → SQL editor → Quote object names: only where needed, always, or
 * never.
 *
 * "Where needed" quotes a name that is not a plain identifier, that a
 * Postgres-family engine would fold to lower case (`Orders` reads as
 * `orders` unquoted), or that is a reserved word there. An engine without a
 * list of its own gets every list at once: quoting a name that did not need
 * it is harmless, leaving one bare that did is a syntax error.
 */

/** @typedef {'auto' | 'always' | 'never'} QuoteMode */

/** Engines that fold unquoted names to lower case. */
const FOLDS_LOWER = new Set(['postgres', 'cockroachdb', 'redshift'])

/** @param {string} list */
const words = (list) => new Set(list.trim().split(/\s+/))

// Each engine's own reserved list (SQLite's keyword list in full: it reads
// most of them as names, but not in every position).
const PG_RESERVED = words(`
  all analyse analyze and any array as asc asymmetric authorization binary both case cast check collate
  collation column concurrently constraint create cross current_catalog current_date current_role
  current_schema current_time current_timestamp current_user default deferrable desc distinct do else end
  except false fetch for foreign freeze from full grant group having ilike in initially inner intersect into is
  isnull join lateral leading left like limit localtime localtimestamp natural not notnull null offset on only
  or order outer overlaps placing primary references returning right select session_user similar some symmetric
  system_user table tablesample then to trailing true union unique user using variadic verbose when where
  window with
`)
const MYSQL_RESERVED = words(`
  accessible add all alter analyze and as asc asensitive before between bigint binary blob both by call cascade
  case change char character check collate column condition constraint continue convert create cross cube
  cume_dist current_date current_time current_timestamp current_user cursor database databases day_hour
  day_microsecond day_minute day_second dec decimal declare default delayed delete dense_rank desc describe
  deterministic distinct distinctrow div double drop dual each else elseif empty enclosed escaped except exists
  exit explain false fetch first_value float float4 float8 for force foreign from fulltext function generated
  get grant group grouping groups having high_priority hour_microsecond hour_minute hour_second if ignore in
  index infile inner inout insensitive insert int int1 int2 int3 int4 int8 integer intersect interval into is
  iterate join json_table key keys kill lag last_value lateral lead leading leave left like limit linear lines
  load localtime localtimestamp lock long longblob longtext loop low_priority match maxvalue mediumblob
  mediumint mediumtext middleint minute_microsecond minute_second mod modifies natural not no_write_to_binlog
  nth_value ntile null numeric of on optimize option optionally or order out outer outfile over partition
  percent_rank precision primary procedure purge range rank read reads read_write real recursive references
  regexp release rename repeat replace require resignal restrict return revoke right rlike row rows row_number
  schema schemas second_microsecond select sensitive separator set show signal smallint spatial specific sql
  sqlexception sqlstate sqlwarning sql_big_result sql_calc_found_rows sql_small_result ssl starting stored
  straight_join system table terminated then tinyblob tinyint tinytext to trailing trigger true undo union
  unique unlock unsigned update usage use using utc_date utc_time utc_timestamp values varbinary varchar
  varcharacter varying virtual when where while window with write xor year_month zerofill
`)
const MSSQL_RESERVED = words(`
  add all alter and any as asc authorization backup begin between break browse bulk by cascade case check
  checkpoint close clustered coalesce collate column commit compute constraint contains containstable continue
  convert create cross current current_date current_time current_timestamp current_user cursor database dbcc
  deallocate declare default delete deny desc disk distinct distributed double drop dump else end errlvl escape
  except exec execute exists exit external fetch file fillfactor for foreign freetext freetexttable from full
  function goto grant group having holdlock identity identity_insert identitycol if in index inner insert
  intersect into is join key kill left like lineno load merge national nocheck nonclustered not null nullif of
  off offsets on open opendatasource openquery openrowset openxml option or order outer over percent pivot plan
  precision primary print proc procedure public raiserror read readtext reconfigure references replication
  restore restrict return revert revoke right rollback rowcount rowguidcol rule save schema securityaudit
  select session_user set setuser shutdown some statistics system_user table tablesample textsize then to top
  tran transaction trigger truncate try_convert tsequal union unique unpivot update updatetext use user values
  varying view waitfor when where while with within writetext
`)
const SQLITE_RESERVED = words(`
  abort action add after all alter always analyze and as asc attach autoincrement before begin between by
  cascade case cast check collate column commit conflict constraint create cross current current_date
  current_time current_timestamp database default deferrable deferred delete desc detach distinct do drop each
  else end escape except exclude exclusive exists explain fail filter first following for foreign from full
  generated glob group groups having if ignore immediate in index indexed initially inner insert instead
  intersect into is isnull join key last left like limit match materialized natural no not nothing notnull null
  nulls of offset on or order others outer over partition plan pragma preceding primary query raise range
  recursive references regexp reindex release rename replace restrict returning right rollback row rows
  savepoint select set table temp temporary then ties to transaction trigger unbounded union unique update
  using vacuum values view virtual when where window with without
`)
/** For an engine without its own list (ClickHouse, anything new): all of them. */
const ANY_RESERVED = new Set([...PG_RESERVED, ...MYSQL_RESERVED, ...MSSQL_RESERVED, ...SQLITE_RESERVED])

/** @param {string | null | undefined} engine */
function reservedFor(engine) {
  switch (engine) {
    case 'postgres': case 'cockroachdb': case 'redshift': case 'duckdb': return PG_RESERVED
    case 'mysql': case 'mariadb': return MYSQL_RESERVED
    case 'mssql': return MSSQL_RESERVED
    case 'sqlite': case 'd1': case 'libsql': return SQLITE_RESERVED
    default: return ANY_RESERVED
  }
}

/** @param {string | null | undefined} engine */
function quoteChars(engine) {
  if (engine === 'mysql' || engine === 'mariadb') return ['`', '`']
  if (engine === 'mssql') return ['[', ']']
  return ['"', '"']
}

/**
 * Whether a name has to be quoted to mean itself.
 * @param {string} name @param {string | null | undefined} engine
 */
export function needsQuotes(name, engine) {
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) return true
  if (FOLDS_LOWER.has(String(engine ?? 'postgres')) && name !== name.toLowerCase()) return true
  return reservedFor(engine ?? 'postgres').has(name.toLowerCase())
}

/**
 * One name, quoted the engine's way when `mode` asks for it.
 * @param {string} name @param {string | null | undefined} engine @param {QuoteMode} [mode]
 */
export function quoteName(name, engine, mode = 'always') {
  if (mode === 'never' || (mode === 'auto' && !needsQuotes(name, engine))) return name
  const [open, close] = quoteChars(engine)
  return open + name.split(close).join(close + close) + close
}

/**
 * The schema a bare table name resolves to, per engine. Only that one may be
 * left out: any other (another Postgres schema, an attached SQLite database,
 * a MySQL database other than the connection's) would read a different table.
 */
const DEFAULT_SCHEMA = /** @type {Record<string, string>} */ ({
  postgres: 'public', cockroachdb: 'public', mssql: 'dbo', duckdb: 'main', sqlite: 'main', d1: 'main', libsql: 'main',
})

/**
 * `schema.table`, or just `table` when qualifying is off and the schema is
 * the engine's default.
 * @param {{ schema?: string | null, table: string, engine?: string | null, mode?: QuoteMode, qualify?: boolean }} opts
 */
export function tableRef({ schema, table, engine, mode = 'always', qualify = true }) {
  const t = quoteName(table, engine, mode)
  if (!schema) return t
  const e = String(engine ?? 'postgres')
  if (!qualify && DEFAULT_SCHEMA[e] === schema) return t
  return `${quoteName(schema, engine, mode)}.${t}`
}
