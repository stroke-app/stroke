/**
 * The sidebar's Objects groups: which kinds an engine has, how each is shown,
 * and the CREATE template the group's + opens in a new editor tab.
 *
 * Templates are CodeMirror snippets: `${1:name}` is a field Tab walks to,
 * fields with the same number are edited together, and `${0}` is where the
 * cursor ends. `snippetText` gives the plain text the snippet expands to, so a
 * tab can show the template before the editor has turned it into fields.
 */

/** @typedef {'view' | 'matview' | 'function' | 'procedure' | 'trigger' | 'sequence' | 'type' | 'event'} ObjectKind */
/** @typedef {'postgres' | 'mysql' | 'mssql' | 'sqlite' | 'duckdb' | 'clickhouse'} ObjectFamily */

/** @type {Record<ObjectKind, { label: string, one: string, icon: string, tone: string }>} */
export const OBJECT_KIND_META = {
  // `tone` colours the kind's icon in the sidebar, from the semantic tokens,
  // so a kind is told apart at a glance in a long tree.
  view: { label: 'Views', one: 'view', icon: 'table-view', tone: 'text-warning' },
  matview: { label: 'Materialized views', one: 'materialized view', icon: 'layers', tone: 'text-warning' },
  function: { label: 'Functions', one: 'function', icon: 'function-square', tone: 'text-info' },
  procedure: { label: 'Procedures', one: 'procedure', icon: 'cog', tone: 'text-success' },
  trigger: { label: 'Triggers', one: 'trigger', icon: 'zap', tone: 'text-destructive' },
  sequence: { label: 'Sequences', one: 'sequence', icon: 'list-ordered', tone: 'text-muted-foreground' },
  type: { label: 'Types', one: 'type', icon: 'box', tone: 'text-primary' },
  event: { label: 'Events', one: 'event', icon: 'clock', tone: 'text-warning' },
}

/** Every kind, in the order the groups stack. @type {ObjectKind[]} */
export const OBJECT_KINDS = ['view', 'matview', 'function', 'procedure', 'trigger', 'sequence', 'type', 'event']

/**
 * The engine family a connection type writes its DDL in, or null for the ones
 * with no objects (Redis, PostHog).
 * @param {string | null | undefined} type
 * @returns {ObjectFamily | null}
 */
export function objectFamily(type) {
  switch (type) {
    case 'postgres': case 'cockroachdb': return 'postgres'
    case 'mysql': case 'mariadb': return 'mysql'
    case 'mssql': return 'mssql'
    case 'sqlite': case 'd1': case 'libsql': return 'sqlite'
    case 'duckdb': return 'duckdb'
    case 'clickhouse': return 'clickhouse'
    default: return null
  }
}

/**
 * What each family keeps. The backend answers the same question for the live
 * connection (`listDbObjects().kinds`); this copy decides which templates exist.
 * @type {Record<ObjectFamily, ObjectKind[]>}
 */
export const FAMILY_KINDS = {
  postgres: ['view', 'matview', 'function', 'procedure', 'trigger', 'sequence', 'type'],
  mysql: ['view', 'function', 'procedure', 'trigger', 'event'],
  mssql: ['view', 'function', 'procedure', 'trigger', 'sequence'],
  sqlite: ['view', 'trigger'],
  duckdb: ['view', 'function', 'sequence'],
  clickhouse: ['view', 'function'],
}

/** Snippet syntax CodeMirror reads: `${1:text}`, `${name}`, `#{…}`. */
const FIELD = /[#$]\{(?:(\d+)(?::([^{}]*))?|((?:\\[{}]|[^{}])*))\}/g

/**
 * The text a snippet expands to, fields replaced by their placeholders.
 * @param {string} snippet
 */
export function snippetText(snippet) {
  return snippet.replace(FIELD, (_, _n, text, name) => text || name || '').replace(/\\([{}])/g, '$1')
}

/** Braces are snippet syntax, so a name carrying one is escaped. @param {string} s */
const escapeSnippet = (s) => s.replace(/[{}]/g, '\\$&')

/**
 * `schema.` in front of a new object's name, quoted where the dialect needs it,
 * or nothing where the schema is the default one anyway (Postgres `public`,
 * SQLite and DuckDB `main`) or there are no schemas (SQLite, ClickHouse functions).
 * @param {ObjectFamily} family @param {string} schema
 */
function prefix(family, schema) {
  const s = String(schema ?? '').trim()
  if (!s || family === 'sqlite') return ''
  if ((family === 'postgres' && s === 'public') || (family === 'duckdb' && s === 'main')) return ''
  if (family === 'mysql' || family === 'clickhouse') {
    return escapeSnippet(/^[A-Za-z_][A-Za-z0-9_$]*$/.test(s) ? s : `\`${s.replaceAll('`', '``')}\``) + '.'
  }
  if (family === 'mssql') return escapeSnippet(/^[A-Za-z_][A-Za-z0-9_]*$/.test(s) ? s : `[${s.replaceAll(']', ']]')}]`) + '.'
  return escapeSnippet(/^[a-z_][a-z0-9_$]*$/.test(s) ? s : `"${s.replaceAll('"', '""')}"`) + '.'
}

/**
 * The CREATE template per family and kind. `q` is the schema prefix.
 * @type {Record<ObjectFamily, Partial<Record<ObjectKind, (q: string) => string>>>}
 */
const TEMPLATES = {
  postgres: {
    view: (q) => `CREATE OR REPLACE VIEW ${q}\${1:new_view} AS
SELECT \${2:*}
FROM \${3:table_name}
WHERE \${0:true};`,
    matview: (q) => `CREATE MATERIALIZED VIEW ${q}\${1:new_view} AS
SELECT \${2:*}
FROM \${3:table_name}
WITH DATA;`,
    function: (q) => `CREATE OR REPLACE FUNCTION ${q}\${1:new_function}(\${2:a integer})
RETURNS \${3:integer}
LANGUAGE plpgsql
AS $$
BEGIN
    \${0:RETURN a;}
END;
$$;`,
    procedure: (q) => `CREATE OR REPLACE PROCEDURE ${q}\${1:new_procedure}(\${2:a integer})
LANGUAGE plpgsql
AS $$
BEGIN
    \${0:RAISE NOTICE 'a = %', a;}
END;
$$;`,
    // A Postgres trigger runs a function, so the template writes both.
    trigger: (q) => `CREATE OR REPLACE FUNCTION ${q}\${1:set_updated_at}()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    \${2:NEW.updated_at := now();}
    RETURN NEW;
END;
$$;

CREATE TRIGGER \${3:trg_set_updated_at}
\${4:BEFORE} \${5:UPDATE} ON ${q}\${6:table_name}
FOR EACH ROW
EXECUTE FUNCTION ${q}\${1:set_updated_at}();`,
    sequence: (q) => `CREATE SEQUENCE ${q}\${1:new_sequence}
    AS \${2:bigint}
    START WITH \${3:1}
    INCREMENT BY \${4:1};`,
    type: (q) => `CREATE TYPE ${q}\${1:new_type} AS ENUM (\${2:'draft', 'active', 'archived'});`,
  },
  mysql: {
    view: (q) => `CREATE OR REPLACE VIEW ${q}\${1:new_view} AS
SELECT \${2:*}
FROM \${3:table_name};`,
    function: (q) => `CREATE FUNCTION ${q}\${1:new_function}(\${2:a INT})
RETURNS \${3:INT}
DETERMINISTIC
BEGIN
    \${0:RETURN a * 2;}
END;`,
    procedure: (q) => `CREATE PROCEDURE ${q}\${1:new_procedure}(\${2:IN a INT})
BEGIN
    \${0:SELECT a;}
END;`,
    trigger: (q) => `CREATE TRIGGER ${q}\${1:trg_before_insert}
\${2:BEFORE} \${3:INSERT} ON ${q}\${4:table_name}
FOR EACH ROW
BEGIN
    \${0:SET NEW.created_at = NOW();}
END;`,
    event: (q) => `CREATE EVENT ${q}\${1:new_event}
ON SCHEDULE EVERY \${2:1 DAY}
DO
BEGIN
    \${0:DELETE FROM sessions WHERE expires_at < NOW();}
END;`,
  },
  mssql: {
    view: (q) => `CREATE OR ALTER VIEW ${q}\${1:new_view} AS
SELECT \${2:*}
FROM \${3:table_name};`,
    function: (q) => `CREATE OR ALTER FUNCTION ${q}\${1:new_function}(\${2:@a INT})
RETURNS \${3:INT}
AS
BEGIN
    \${0:RETURN @a * 2;}
END;`,
    procedure: (q) => `CREATE OR ALTER PROCEDURE ${q}\${1:new_procedure}
    \${2:@a INT}
AS
BEGIN
    SET NOCOUNT ON;
    \${0:SELECT @a;}
END;`,
    trigger: (q) => `CREATE OR ALTER TRIGGER ${q}\${1:trg_after_insert}
ON ${q}\${2:table_name}
\${3:AFTER} \${4:INSERT}
AS
BEGIN
    SET NOCOUNT ON;
    \${0:SELECT * FROM inserted;}
END;`,
    sequence: (q) => `CREATE SEQUENCE ${q}\${1:new_sequence}
    AS \${2:BIGINT}
    START WITH \${3:1}
    INCREMENT BY \${4:1};`,
  },
  sqlite: {
    view: () => `CREATE VIEW \${1:new_view} AS
SELECT \${2:*}
FROM \${3:table_name};`,
    trigger: () => `CREATE TRIGGER \${1:trg_after_update}
\${2:AFTER} \${3:UPDATE} ON \${4:table_name}
FOR EACH ROW
BEGIN
    UPDATE \${4:table_name} SET \${5:updated_at = CURRENT_TIMESTAMP} WHERE rowid = NEW.rowid;\${0}
END;`,
  },
  duckdb: {
    view: (q) => `CREATE OR REPLACE VIEW ${q}\${1:new_view} AS
SELECT \${2:*}
FROM \${3:table_name};`,
    function: (q) => `CREATE OR REPLACE MACRO ${q}\${1:new_macro}(\${2:a, b}) AS \${0:a + b};`,
    sequence: (q) => `CREATE SEQUENCE ${q}\${1:new_sequence} START \${2:1} INCREMENT BY \${3:1};`,
  },
  clickhouse: {
    view: (q) => `CREATE OR REPLACE VIEW ${q}\${1:new_view} AS
SELECT \${2:*}
FROM \${3:table_name};`,
    // SQL functions are server-wide in ClickHouse: no database in front.
    function: () => `CREATE FUNCTION \${1:new_function} AS (\${2:a, b}) -> \${0:a + b};`,
  },
}

/**
 * The template the + on a group opens, for a connection type, a kind and the
 * schema the sidebar shows. Null where the engine has no such kind.
 * @param {string | null | undefined} type connection type
 * @param {ObjectKind} kind
 * @param {string} [schema]
 * @returns {{ title: string, snippet: string, text: string } | null}
 */
export function objectTemplate(type, kind, schema = '') {
  const family = objectFamily(type)
  const make = family ? TEMPLATES[family][kind] : undefined
  if (!family || !make) return null
  const snippet = make(prefix(family, schema))
  return { title: `New ${OBJECT_KIND_META[kind].one}`, snippet, text: snippetText(snippet) }
}

/**
 * How a row names its object: a routine with its arguments, so overloads
 * differ; a trigger with the table it fires on.
 * @param {{ kind: string, name: string, args?: string, table?: string }} obj
 */
export function objectLabel(obj) {
  if (obj.kind === 'function' || obj.kind === 'procedure') return `${obj.name}(${obj.args ?? ''})`
  return obj.name
}

/**
 * A stable key for one object: two overloads, or two triggers of the same
 * name on different tables, are different rows.
 * @param {{ kind: string, name: string, args?: string, table?: string }} obj
 */
export function objectKey(obj) {
  return `${obj.kind}\u0000${obj.name}\u0000${obj.args ?? ''}\u0000${obj.table ?? ''}`
}

/**
 * The editor tab title for an object's definition.
 * @param {{ kind: string, name: string, args?: string, table?: string }} obj
 */
export function definitionTitle(obj) {
  return obj.kind === 'trigger' && obj.table ? `${obj.name} on ${obj.table}` : objectLabel(obj)
}

/** @param {string} s */
const dq = (s) => `"${String(s).replaceAll('"', '""')}"`
/** @param {string} s */
const bt = (s) => `\`${String(s).replaceAll('`', '``')}\``
/** @param {string} s */
const br = (s) => `[${String(s).replaceAll(']', ']]')}]`

/**
 * The DROP the backend runs for an object (`drop_object` in objects.rs), for
 * the confirmation to show. Null where the engine cannot drop that kind.
 * @param {string | null | undefined} type connection type
 * @param {string} schema
 * @param {{ kind: string, name: string, args?: string, table?: string, subtype?: string }} obj
 * @param {boolean} [cascade] Postgres only
 */
export function dropObjectSql(type, schema, obj, cascade = false) {
  const family = objectFamily(type)
  const kind = /** @type {ObjectKind} */ (obj.kind)
  if (!family || !FAMILY_KINDS[family].includes(kind)) return null
  if (family === 'postgres') {
    const qn = `${dq(schema)}.${dq(obj.name)}`
    const tail = cascade ? ' CASCADE' : ''
    switch (kind) {
      case 'function': return `DROP ${obj.subtype === 'aggregate' ? 'AGGREGATE' : 'FUNCTION'} ${qn}(${obj.args ?? ''})${tail}`
      case 'procedure': return `DROP PROCEDURE ${qn}(${obj.args ?? ''})${tail}`
      case 'trigger': return `DROP TRIGGER ${dq(obj.name)} ON ${dq(schema)}.${dq(obj.table ?? '')}${tail}`
      case 'view': return `DROP VIEW ${qn}${tail}`
      case 'matview': return `DROP MATERIALIZED VIEW ${qn}${tail}`
      case 'sequence': return `DROP SEQUENCE ${qn}${tail}`
      case 'type': return `DROP ${obj.subtype === 'domain' ? 'DOMAIN' : 'TYPE'} ${qn}${tail}`
      default: return null
    }
  }
  const keyword = kind === 'matview' ? 'MATERIALIZED VIEW' : kind.toUpperCase()
  switch (family) {
    case 'mysql': return `DROP ${keyword} ${bt(schema)}.${bt(obj.name)}`
    case 'mssql': return `DROP ${keyword} ${br(schema)}.${br(obj.name)}`
    case 'sqlite': return `DROP ${keyword} ${dq(obj.name)}`
    case 'duckdb':
      return kind === 'function'
        ? `DROP MACRO ${obj.subtype === 'table_macro' ? 'TABLE ' : ''}${dq(schema)}.${dq(obj.name)}`
        : `DROP ${keyword} ${dq(schema)}.${dq(obj.name)}`
    case 'clickhouse':
      return kind === 'function' ? `DROP FUNCTION ${bt(obj.name)}` : `DROP VIEW ${bt(schema)}.${bt(obj.name)}`
    default: return null
  }
}
