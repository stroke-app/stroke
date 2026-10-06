/**
 * Where the caret is in a SQL statement, for completion - no editor involved.
 *
 * lang-sql's own completion reads the syntax tree and knows nothing about
 * what a DML statement is being written: it offered nothing inside `""` (the
 * very place a column name goes in quoted SQL) and ranked UPPER() above
 * UPDATE at the start of a statement. This reads the text before the caret
 * with a small tokenizer instead, which is enough to tell "a table goes here"
 * from "a column goes here" from "the next clause goes here".
 */

/** Keywords a table name follows. */
const TABLE_KEYWORDS = new Set(['UPDATE', 'FROM', 'INTO', 'JOIN', 'TABLE', 'TRUNCATE', 'REFERENCES', 'DESCRIBE'])

/** Functions whose `AS` is followed by a type, not an alias. */
const CAST_FUNCTIONS = new Set(['CAST', 'TRY_CAST', 'SAFE_CAST'])
/** SQL Server functions that take the type first: CONVERT(type, value). */
const TYPE_FIRST_FUNCTIONS = new Set(['CONVERT', 'TRY_CONVERT'])
/** Words that open a table constraint in a column list, not a column. */
const TABLE_CONSTRAINT_WORDS = new Set(['CONSTRAINT', 'PRIMARY', 'FOREIGN', 'UNIQUE', 'CHECK', 'EXCLUDE', 'KEY', 'INDEX', 'LIKE', 'FULLTEXT', 'SPATIAL', 'PERIOD'])
/** What a column list entry can start with besides a new column's name. */
const DEFINITION_STARTS = ['CONSTRAINT', 'PRIMARY', 'FOREIGN', 'UNIQUE', 'CHECK']
/** What follows a column's type. */
const COLUMN_CONSTRAINTS = [
  'NOT', 'NULL', 'DEFAULT', 'PRIMARY', 'KEY', 'UNIQUE', 'REFERENCES', 'CHECK', 'GENERATED', 'COLLATE', 'CONSTRAINT',
  'AUTO_INCREMENT', 'AUTOINCREMENT', 'IDENTITY', 'UNSIGNED',
]
/** What ALTER TABLE name can do next. */
const ALTER_ACTIONS = ['ADD', 'DROP', 'ALTER', 'RENAME', 'MODIFY', 'CHANGE', 'SET', 'OWNER']
/** After ALTER COLUMN name: Postgres goes on with TYPE / SET / DROP, SQL Server with the type. */
const ALTER_COLUMN_NEXT = ['TYPE', 'SET', 'DROP']

/** Keywords that open a clause, i.e. decide what the next thing is. */
const CLAUSES = new Set([
  'UPDATE', 'SET', 'WHERE', 'AND', 'OR', 'NOT', 'FROM', 'INTO', 'JOIN', 'ON', 'VALUES',
  'SELECT', 'DELETE', 'INSERT', 'RETURNING', 'ORDER', 'GROUP', 'BY', 'HAVING', 'LIMIT',
  'TABLE', 'WITH', 'TRUNCATE',
])

/** Words that leave an expression unfinished: the next token continues it. */
const OPEN_WORDS = new Set([
  ...CLAUSES, 'IS', 'IN', 'LIKE', 'ILIKE', 'BETWEEN', 'AS', 'DISTINCT', 'CASE', 'WHEN',
  'THEN', 'ELSE', 'EXISTS', 'ALL', 'ANY',
])

/** The keywords worth putting first, by the clause the caret is in. */
const NEXT = /** @type {Record<string, string[]>} */ ({
  '': ['UPDATE', 'INSERT', 'DELETE', 'SELECT', 'WITH'],
  UPDATE: ['SET'],
  SET: ['WHERE', 'RETURNING'],
  WHERE: ['AND', 'OR', 'IS', 'NOT', 'NULL', 'IN', 'LIKE', 'RETURNING'],
  AND: ['AND', 'OR', 'IS', 'NOT', 'NULL', 'IN', 'LIKE', 'RETURNING'],
  OR: ['AND', 'OR', 'IS', 'NOT', 'NULL', 'IN', 'LIKE', 'RETURNING'],
  NOT: ['NULL', 'IN', 'LIKE', 'EXISTS'],
  INSERT: ['INTO'],
  INTO: ['VALUES', 'DEFAULT', 'SELECT'],
  VALUES: ['RETURNING', 'DEFAULT', 'NULL'],
  DELETE: ['FROM'],
  FROM: ['WHERE', 'JOIN', 'ORDER', 'LIMIT'],
  SELECT: ['FROM'],
  RETURNING: [],
})

/**
 * @typedef {{ t: 'word' | 'qid' | 'str' | 'num' | 'punct', v: string }} Token
 * @typedef {{
 *   kind: 'statement' | 'tables' | 'columns' | 'keywords' | 'qualified' | 'types' | 'ddl',
 *   from: number,
 *   prefix: string,
 *   quote: string | null,
 *   qualifier: string | null,
 *   clause: string,
 *   next: string[],
 *   afterExpr: boolean,
 *   tables: string[],
 *   predicateColumn: ColumnRef | null,
 *   comparedColumn: (ColumnRef & { operator: string }) | null,
 *   verb: string,
 *   rowTable: string | null,
 *   routine: string | null,
 *   phrases: Phrase[],
 *   eager: boolean,
 *   columnsOf: string | null,
 *   names: 'schemas' | null,
 *   typesFor: string[] | null,
 *   lower: boolean,
 * }} SqlCompletionContext
 * `predicateColumn`: the column just written in a condition, a space behind
 * it (`WHERE price |`): an operator comes next. `comparedColumn`: the column
 * and operator before the caret (`WHERE price >= |`): a value comes next.
 * `verb`: the statement's first keyword (SELECT, UPDATE ...).
 * `rowTable`: inside CREATE TRIGGER, the table it fires on - what NEW. and
 * OLD. are rows of. `routine`: inside CREATE FUNCTION, its name (a Postgres
 * trigger function's table is named by the CREATE TRIGGER that runs it).
 * `types`: a data type goes here (a column definition, ALTER ... TYPE,
 * CAST(x AS ...), x::...); `next` holds keywords that can stand there too.
 * `ddl`: only keywords go here, `next` first (a new column's name, the action
 * after ALTER TABLE name, a column's constraints); no names are offered.
 * `phrases`: what the grammar says follows (followAt), offered first;
 * `eager`: the list opens after a space by itself, the next word being
 * certain. `columnsOf`: only this table's columns are names here. `names`:
 * schemas, not tables, are the names here. `lower`: the statement is written
 * in lower case (its first keyword is), so phrases are too.
 * @typedef {{ name: string, qualifier: string | null }} ColumnRef
 */

/**
 * Tokens of the statement the text ends in, or where the text ends inside
 * something that is not SQL to complete.
 * @param {string} text
 * @returns {{ tokens: Token[], open: { quote: string, start: number } | null, head: Token[] | null } | null}
 *   null inside a string literal or a comment.
 */
function scan(text) {
  /** @type {Token[]} */
  let tokens = []
  /** The tokens before the first `;`: a trigger or routine's head, once its body has statements. @type {Token[] | null} */
  let head = null
  const n = text.length
  let i = 0
  while (i < n) {
    const c = text[i]
    if (c === ' ' || c === '\t' || c === '\n' || c === '\r') { i++; continue }
    if (c === '-' && text[i + 1] === '-') {
      const nl = text.indexOf('\n', i)
      if (nl === -1) return null
      i = nl + 1
      continue
    }
    if (c === '/' && text[i + 1] === '*') {
      const end = text.indexOf('*/', i + 2)
      if (end === -1) return null
      i = end + 2
      continue
    }
    if (c === "'" || c === '"' || c === '`') {
      // Doubled quote is an escaped one: 'it''s', "a""b".
      let j = i + 1
      for (;;) {
        const k = text.indexOf(c, j)
        if (k === -1) {
          if (c === "'") return null
          return { tokens, open: { quote: c, start: i }, head }
        }
        if (text[k + 1] === c) { j = k + 2; continue }
        j = k
        break
      }
      if (c === "'") tokens.push({ t: 'str', v: '' })
      else tokens.push({ t: 'qid', v: text.slice(i + 1, j).replaceAll(c + c, c) })
      i = j + 1
      continue
    }
    if (/[A-Za-z_]/.test(c)) {
      let j = i + 1
      while (j < n && /[\w$]/.test(text[j])) j++
      tokens.push({ t: 'word', v: text.slice(i, j) })
      i = j
      continue
    }
    if (/[0-9]/.test(c)) {
      let j = i + 1
      while (j < n && /[\w.]/.test(text[j])) j++
      tokens.push({ t: 'num', v: text.slice(i, j) })
      i = j
      continue
    }
    if (c === ';') { head ??= tokens; tokens = [] }
    else tokens.push({ t: 'punct', v: c })
    i++
  }
  return { tokens, open: null, head }
}

/** The last head scanned, and its tokens: never changed by readers. */
let headText = /** @type {string | null} */ (null)
let headScan = /** @type {ReturnType<typeof scan>} */ (null)

/** scan() for the text before the word being typed, kept for the next keystroke. @param {string} text */
function scanHead(text) {
  if (text !== headText) {
    headText = text
    headScan = scan(text)
  }
  return headScan
}

/** @param {Token | undefined} tok */
const isName = (tok) => tok?.t === 'word' || tok?.t === 'qid'
/** @param {Token | undefined} tok */
const kw = (tok) => (tok?.t === 'word' ? tok.v.toUpperCase() : '')

/** Clauses whose body is a condition: a column there is compared to something. */
const PREDICATE_CLAUSES = new Set(['WHERE', 'AND', 'OR', 'ON', 'HAVING', 'WHEN', 'NOT'])
/** What a condition's column can follow. */
const PREDICATE_STARTS = new Set(['WHERE', 'AND', 'OR', 'ON', 'HAVING', 'WHEN', 'NOT'])
/** Words that look like names but are values or keywords. */
const NOT_A_COLUMN = new Set(['NULL', 'TRUE', 'FALSE', 'DEFAULT'])

/**
 * The column (`name` or `qualifier.name`) ending at token `end`, or null.
 * @param {Token[]} tokens @param {number} end exclusive
 * @returns {(ColumnRef & { start: number }) | null}
 */
function columnBefore(tokens, end) {
  const tok = tokens[end - 1]
  if (!isName(tok)) return null
  const k = kw(tok)
  if (k && (CLAUSES.has(k) || OPEN_WORDS.has(k) || NOT_A_COLUMN.has(k))) return null
  let start = end - 1
  let qualifier = null
  if (tokens[start - 1]?.v === '.' && isName(tokens[start - 2])) {
    qualifier = /** @type {Token} */ (tokens[start - 2]).v
    start -= 2
  }
  return { name: /** @type {Token} */ (tok).v, qualifier, start }
}

/**
 * Tables the statement names, so their columns can be offered.
 * @param {Token[]} tokens @param {boolean} indexOn CREATE INDEX: ON names the table
 */
function referencedTables(tokens, indexOn) {
  const out = []
  for (let i = 0; i < tokens.length - 1; i++) {
    const k = kw(tokens[i])
    if (!(TABLE_KEYWORDS.has(k) || (indexOn && k === 'ON')) || !isName(tokens[i + 1])) continue
    // schema.table: the table is the last name.
    if (tokens[i + 2]?.v === '.' && isName(tokens[i + 3])) out.push(tokens[i + 3].v)
    else out.push(tokens[i + 1].v)
  }
  return out
}

/** @param {Token | undefined} tok */
const punct = (tok) => (tok?.t === 'punct' ? tok.v : '')

/**
 * Index of the `(` that `tokens[end - 1]` sits inside, or -1 at the top level.
 * @param {Token[]} tokens @param {number} end exclusive
 */
function openParen(tokens, end) {
  let depth = 0
  for (let i = end - 1; i >= 0; i--) {
    const p = punct(tokens[i])
    if (p === ')') depth++
    else if (p === '(') { if (depth === 0) return i; depth-- }
  }
  return -1
}

/**
 * The current entry of a comma-separated list starting at `from`: the tokens
 * after its last comma at that level. `numeric(10, 2)` is one entry.
 * @param {Token[]} tokens @param {number} from
 */
function listEntry(tokens, from) {
  let start = from
  let depth = 0
  for (let i = from; i < tokens.length; i++) {
    const p = punct(tokens[i])
    if (p === '(') depth++
    else if (p === ')') depth--
    else if (p === ',' && depth === 0) start = i + 1
  }
  return tokens.slice(start)
}

/**
 * Where `[schema.]name` starting at `i` ends (exclusive), or -1.
 * @param {Token[]} tokens @param {number} i
 */
function nameEnd(tokens, i) {
  if (!isName(tokens[i])) return -1
  return punct(tokens[i + 1]) === '.' && isName(tokens[i + 2]) ? i + 3 : i + 1
}

/** `IF NOT EXISTS` / `IF EXISTS` at `i`: how many tokens it takes. @param {Token[]} tokens @param {number} i */
function ifExists(tokens, i) {
  if (kw(tokens[i]) !== 'IF') return 0
  if (kw(tokens[i + 1]) === 'EXISTS') return 2
  return kw(tokens[i + 1]) === 'NOT' && kw(tokens[i + 2]) === 'EXISTS' ? 3 : 0
}

/**
 * The tokens end with TABLE / TRUNCATE [IF EXISTS] [schema.]name.
 * @param {Token[]} tokens
 */
function endsWithTableName(tokens) {
  for (let i = tokens.length - 1; i >= 0; i--) {
    const k = kw(tokens[i])
    if (k !== 'TABLE' && k !== 'TRUNCATE') continue
    return nameEnd(tokens, i + 1 + ifExists(tokens, i + 1)) === tokens.length
  }
  return false
}

/**
 * A data type goes right here, whatever the statement: x::|, CAST(x AS |,
 * CONVERT(|, RETURNS |.
 * @param {Token[]} tokens
 */
function typeGoesHere(tokens) {
  const n = tokens.length
  const last = tokens[n - 1]
  if (punct(last) === ':' && punct(tokens[n - 2]) === ':') return true
  if (kw(last) === 'AS') {
    const open = openParen(tokens, n - 1)
    if (open > 0 && CAST_FUNCTIONS.has(kw(tokens[open - 1]))) return true
  }
  if (punct(last) === '(' && TYPE_FIRST_FUNCTIONS.has(kw(tokens[n - 2]))) return true
  return kw(last) === 'RETURNS'
}

/**
 * One column definition (CREATE TABLE's list, ALTER TABLE ADD): a new name
 * or a table constraint first, then the type, then the column's constraints.
 * Null where the entry is something else (a constraint's column list, a
 * DEFAULT value) and the general rules apply.
 * @param {Token[]} entry the definition so far, the word being typed excluded
 * @returns {{ kind: 'types' | 'ddl', next: string[] } | null}
 */
function columnDefinition(entry) {
  if (!entry.length) return { kind: 'ddl', next: DEFINITION_STARTS }
  if (TABLE_CONSTRAINT_WORDS.has(kw(entry[0]))) return null
  if (entry.length === 1 && isName(entry[0])) return { kind: 'types', next: [] }
  const k = kw(entry.at(-1))
  if (k === 'DEFAULT' || k === 'CHECK' || k === 'AS') return null
  return { kind: 'ddl', next: COLUMN_CONSTRAINTS }
}

/**
 * Inside CREATE TABLE's column list: what the current entry wants.
 * @param {Token[]} tokens
 */
function createTableEntry(tokens) {
  if (kw(tokens[0]) !== 'CREATE') return null
  const open = openParen(tokens, tokens.length)
  if (open < 0) return null
  // CREATE [OR REPLACE] [TEMP | UNLOGGED ...] TABLE [IF NOT EXISTS] name (
  let t = 1
  while (t < open && kw(tokens[t]) !== 'TABLE') t++
  if (t >= open) return null
  const at = t + 1 + ifExists(tokens, t + 1)
  if (nameEnd(tokens, at) !== open) return null
  return columnDefinition(listEntry(tokens, open + 1))
}

/**
 * ALTER TABLE name ...: what the action being written wants.
 * @param {Token[]} tokens
 * @returns {{ kind: 'types' | 'ddl', next: string[] } | null}
 */
function alterTable(tokens) {
  if (kw(tokens[0]) !== 'ALTER' || kw(tokens[1]) !== 'TABLE') return null
  let i = 2 + ifExists(tokens, 2)
  if (kw(tokens[i]) === 'ONLY') i++
  const end = nameEnd(tokens, i)
  if (end < 0) return null
  // Postgres and MySQL take several actions, comma-separated.
  const act = listEntry(tokens, end)
  if (!act.length) return { kind: 'ddl', next: ALTER_ACTIONS }
  const verb = kw(act[0])
  let k = kw(act[1]) === 'COLUMN' ? 2 : 1
  if (verb === 'ADD') {
    if (act.length === 1) return { kind: 'ddl', next: ['COLUMN', ...DEFINITION_STARTS] }
    k += ifExists(act, k)
    // ADD COLUMN |: a new name, never a constraint.
    if (k === act.length && k > 1) return { kind: 'ddl', next: k === 2 ? ['IF'] : [] }
    return columnDefinition(act.slice(k))
  }
  const rest = act.slice(k)
  if (!rest.length) return null // the column to change: its table's columns
  if (verb === 'ALTER') {
    if (rest.length === 1) return { kind: 'types', next: ALTER_COLUMN_NEXT }
    const last = kw(rest.at(-1))
    if (last === 'TYPE') return { kind: 'types', next: [] }
    if (last === 'SET') return { kind: 'ddl', next: ['DEFAULT', 'NOT', 'DATA'] }
    if (last === 'DROP') return { kind: 'ddl', next: ['DEFAULT', 'NOT'] }
    if (last === 'NOT') return { kind: 'ddl', next: ['NULL'] }
    return null
  }
  if (verb === 'MODIFY') return columnDefinition(rest)
  // CHANGE old new type: the new name first.
  if (verb === 'CHANGE') return rest.length === 1 ? { kind: 'ddl', next: [] } : columnDefinition(rest.slice(1))
  return null
}

/** Trigger events. */
const TRIGGER_EVENTS = ['INSERT', 'UPDATE', 'DELETE']
/** What starts a trigger's body: past it the general rules apply. */
const TRIGGER_BODY = new Set(['BEGIN', 'AS', 'EXECUTE', 'DO'])

/**
 * Index of TRIGGER in `CREATE [OR REPLACE] [TEMP] [CONSTRAINT] TRIGGER`, or -1.
 * @param {Token[]} tokens
 */
function triggerKeyword(tokens) {
  if (kw(tokens[0]) !== 'CREATE') return -1
  for (let i = 1; i < Math.min(tokens.length, 6); i++) {
    const k = kw(tokens[i])
    if (k === 'TRIGGER') return i
    if (!['OR', 'REPLACE', 'ALTER', 'TEMP', 'TEMPORARY', 'CONSTRAINT'].includes(k) && !k.startsWith('DEFINER')) return -1
  }
  return -1
}

/**
 * The table a CREATE TRIGGER fires on: the name after its ON.
 * @param {Token[]} tokens
 */
function triggerTable(tokens) {
  const t = triggerKeyword(tokens)
  if (t < 0) return null
  for (let i = t + 1; i < tokens.length; i++) {
    if (TRIGGER_BODY.has(kw(tokens[i]))) return null
    if (kw(tokens[i]) !== 'ON' || !isName(tokens[i + 1])) continue
    const end = nameEnd(tokens, i + 1)
    return /** @type {Token} */ (tokens[end - 1]).v
  }
  return null
}

/**
 * Inside a CREATE TRIGGER's head (before its body): what goes at the caret.
 * @param {Token[]} tokens
 * @returns {{ kind: 'tables' | 'ddl', next: string[] } | null}
 */
function triggerHead(tokens) {
  const t = triggerKeyword(tokens)
  if (t < 0 || tokens.slice(t).some((x) => TRIGGER_BODY.has(kw(x)))) return null
  const last = tokens.at(-1)
  const k = kw(last)
  const at = t + 1 + ifExists(tokens, t + 1)
  // CREATE TRIGGER |: its new name.
  if (tokens.length <= at) return { kind: 'ddl', next: [] }
  const nameEndAt = nameEnd(tokens, at)
  if (tokens.length === nameEndAt) return { kind: 'ddl', next: ['BEFORE', 'AFTER', 'INSTEAD', 'ON'] }
  if (k === 'ON') return { kind: 'tables', next: [] }
  if (k === 'BEFORE' || k === 'AFTER') return { kind: 'ddl', next: TRIGGER_EVENTS }
  if (k === 'INSTEAD') return { kind: 'ddl', next: ['OF'] }
  if (k === 'OF' && kw(tokens.at(-2)) === 'INSTEAD') return { kind: 'ddl', next: TRIGGER_EVENTS }
  if (k === 'OR') return { kind: 'ddl', next: TRIGGER_EVENTS }
  if (TRIGGER_EVENTS.includes(k)) return { kind: 'ddl', next: k === 'UPDATE' ? ['ON', 'OR', 'OF'] : ['ON', 'OR'] }
  if (k === 'FOR') return { kind: 'ddl', next: ['EACH'] }
  if (k === 'EACH') return { kind: 'ddl', next: ['ROW', 'STATEMENT'] }
  if (k === 'ROW' || k === 'STATEMENT') return { kind: 'ddl', next: ['BEGIN', 'WHEN', 'EXECUTE'] }
  // CREATE TRIGGER t AFTER UPDATE ON table |
  const on = tokens.findLastIndex((x) => kw(x) === 'ON')
  if (on > t && nameEnd(tokens, on + 1) === tokens.length) {
    return { kind: 'ddl', next: ['FOR', 'BEGIN', 'WHEN', 'EXECUTE', 'REFERENCING', 'AFTER', 'INSTEAD'] }
  }
  return null
}

// ── Grammar: what can come next ──────────────────────────────────────────────
// DataGrip-style: at each point of a statement, the words that grammatically
// follow (whole phrases: IF EXISTS, ORDER BY, DO UPDATE SET), and which names.
// Read from the statement's own tokens, mostly its last few, so it costs next
// to nothing per keystroke.

/**
 * A phrase offered as one item. `only`: the engine families that have it.
 * `reopen`: taking it writes a space and opens the list again (names follow).
 * @typedef {{ text: string, only: string[] | null, reopen: boolean }} Phrase
 * @typedef {{
 *   phrases: Phrase[],
 *   eager: boolean,
 *   only: boolean,
 *   kind?: SqlCompletionContext['kind'],
 *   columnsOf?: string,
 *   names?: 'schemas',
 *   next?: string[],
 *   typesFor?: string[],
 * }} Follow
 * `eager`: what comes next is certain, so the list opens after a space by
 * itself. `only`: the phrases are all that fits here (no names, no other
 * clauses). `columnsOf`: only this table's columns are names here. `next`:
 * the single words that stand alone here, replacing the clause's. `typesFor`:
 * the engines where a bare type goes here (SQL Server's ALTER COLUMN c int).
 */

/** @param {string} text @param {string} [only] space-separated families @param {boolean} [reopen] @returns {Phrase} */
const P = (text, only, reopen = false) => ({ text, only: only ? only.split(' ') : null, reopen })

const PG_LIKE = 'postgres duckdb'
const DROP_OBJECTS = [
  P('TABLE', '', true), P('VIEW', '', true), P('MATERIALIZED VIEW', 'postgres clickhouse', true), P('INDEX', '', true),
  P('SCHEMA', 'postgres mysql mssql duckdb', true), P('SEQUENCE', 'postgres mssql duckdb', true),
  P('FUNCTION', 'postgres mysql mssql duckdb'), P('PROCEDURE', 'postgres mysql mssql'), P('TRIGGER', 'postgres mysql sqlite mssql'),
  P('TYPE', 'postgres mssql duckdb'), P('DATABASE', 'postgres mysql mssql clickhouse'), P('EXTENSION', 'postgres'),
]
const CREATE_OBJECTS = [
  P('TABLE'), P('OR REPLACE', 'postgres mysql clickhouse duckdb'), P('OR ALTER', 'mssql'), P('VIEW'),
  P('MATERIALIZED VIEW', 'postgres clickhouse'), P('INDEX'), P('UNIQUE INDEX', 'postgres mysql sqlite mssql duckdb'),
  P('SCHEMA', 'postgres mysql mssql duckdb'), P('SEQUENCE', 'postgres mssql duckdb'), P('TYPE', 'postgres mssql duckdb'),
  P('TRIGGER', 'postgres mysql sqlite mssql'), P('FUNCTION', 'postgres mysql mssql duckdb'), P('PROCEDURE', 'postgres mysql mssql'),
  P('EXTENSION', 'postgres'), P('DATABASE', 'postgres mysql mssql clickhouse'), P('TEMPORARY TABLE', 'postgres mysql sqlite duckdb'),
]
const OR_REPLACE_OBJECTS = [P('VIEW'), P('FUNCTION', PG_LIKE), P('PROCEDURE', 'postgres mssql'), P('TRIGGER', 'postgres mssql'), P('MATERIALIZED VIEW', 'clickhouse')]
const ALTER_OBJECTS = [
  P('TABLE', '', true), P('VIEW', 'postgres mysql mssql'), P('INDEX', 'postgres mssql'), P('SEQUENCE', 'postgres mssql duckdb'),
  P('SCHEMA', 'postgres mssql'), P('TYPE', 'postgres'), P('FUNCTION', 'postgres mysql mssql'), P('DATABASE', 'postgres mysql mssql'),
  P('MATERIALIZED VIEW', 'postgres'),
]
/** ALTER TABLE name |: the actions, as their usual phrases. */
const ALTER_TABLE_ACTIONS = [
  P('ADD COLUMN', 'postgres mysql sqlite duckdb clickhouse'), P('DROP COLUMN', '', true), P('ALTER COLUMN', 'postgres mssql duckdb', true),
  P('RENAME COLUMN', 'postgres mysql sqlite duckdb clickhouse', true), P('RENAME TO', 'postgres mysql sqlite duckdb clickhouse'),
  P('ADD CONSTRAINT', 'postgres mysql mssql duckdb'), P('DROP CONSTRAINT', 'postgres mysql mssql duckdb'),
  P('ADD PRIMARY KEY', 'postgres mysql mssql duckdb'), P('ADD FOREIGN KEY', 'postgres mysql mssql'),
  P('MODIFY COLUMN', 'mysql clickhouse', true), P('CHANGE COLUMN', 'mysql', true), P('OWNER TO', 'postgres'),
  P('SET SCHEMA', 'postgres duckdb'), P('ENABLE TRIGGER', 'postgres mssql'), P('DISABLE TRIGGER', 'postgres mssql'),
]
/** ALTER TABLE t ALTER COLUMN c |. */
const ALTER_COLUMN_ACTIONS = [
  P('TYPE', 'postgres duckdb'), P('SET DATA TYPE', 'postgres duckdb'), P('SET DEFAULT', 'postgres mysql duckdb'),
  P('DROP DEFAULT', 'postgres mysql duckdb'), P('SET NOT NULL', 'postgres duckdb'), P('DROP NOT NULL', 'postgres duckdb'),
]
/** The engines with `IF [NOT] EXISTS` on a given statement. */
const IF_EXISTS_DROP = '' // every engine
const IF_NOT_EXISTS = {
  TABLE: 'postgres mysql sqlite duckdb clickhouse', INDEX: 'postgres sqlite duckdb', SCHEMA: 'postgres mysql duckdb',
  SEQUENCE: 'postgres duckdb', VIEW: 'sqlite duckdb clickhouse', 'MATERIALIZED VIEW': 'postgres clickhouse',
  DATABASE: 'mysql clickhouse', EXTENSION: 'postgres', TRIGGER: 'mysql sqlite', TYPE: '',
}
/** What a DROP / CREATE / ALTER object word can be, MATERIALIZED VIEW read as one. */
const OBJECT_WORDS = new Set(['TABLE', 'VIEW', 'INDEX', 'SCHEMA', 'SEQUENCE', 'FUNCTION', 'PROCEDURE', 'TRIGGER', 'TYPE', 'DATABASE', 'EXTENSION'])
/** Objects whose names the hints list: tables (and views, the sidebar lists both), schemas. */
const TABLE_LIKE = new Set(['TABLE', 'VIEW', 'MATERIALIZED VIEW'])

/**
 * The object word at `i` (MATERIALIZED VIEW as one), and where it ends.
 * @param {Token[]} tokens @param {number} i
 */
function objectAt(tokens, i) {
  if (kw(tokens[i]) === 'MATERIALIZED' && kw(tokens[i + 1]) === 'VIEW') return { type: 'MATERIALIZED VIEW', end: i + 2 }
  const k = kw(tokens[i])
  return OBJECT_WORDS.has(k) ? { type: k, end: i + 1 } : null
}

/** @param {Phrase[]} phrases @param {Partial<Follow>} [rest] @returns {Follow} */
const forced = (phrases, rest = {}) => ({ phrases, eager: true, only: true, ...rest })
/** @param {Phrase[]} phrases @param {Partial<Follow>} [rest] @returns {Follow} */
const offered = (phrases, rest = {}) => ({ phrases, eager: false, only: false, ...rest })
/** The last part of the name ending at `end` (exclusive). @param {Token[]} tokens @param {number} end */
const lastName = (tokens, end) => /** @type {Token} */ (tokens[end - 1]).v

/** DROP …  @param {Token[]} tokens @returns {Follow | null} */
function dropFollow(tokens) {
  const n = tokens.length
  if (n === 1) return forced(DROP_OBJECTS)
  const o = objectAt(tokens, 1)
  if (!o) return null
  const names = TABLE_LIKE.has(o.type) ? 'tables' : o.type === 'SCHEMA' ? 'schemas' : null
  let i = o.end
  if (o.type === 'INDEX' && kw(tokens[i]) === 'CONCURRENTLY') i++
  if (n === i) {
    const extra = o.type === 'INDEX' && i === o.end ? [P('CONCURRENTLY', 'postgres')] : []
    return offered([P('IF EXISTS', IF_EXISTS_DROP, true), ...extra], names === 'schemas' ? { kind: 'ddl', names } : names ? { kind: 'tables' } : { kind: 'ddl' })
  }
  if (kw(tokens[i]) === 'IF') {
    if (n === i + 1) return forced([P('EXISTS', '', true)])
    if (kw(tokens[i + 1]) !== 'EXISTS') return null
    i += 2
    if (n === i) {
      return names === 'schemas' ? { phrases: [], eager: true, only: false, kind: 'ddl', names }
        : names ? { phrases: [], eager: true, only: false, kind: 'tables' } : null
    }
  }
  // DROP TABLE a, b CASCADE: after the names.
  let j = i
  for (;;) {
    const e = nameEnd(tokens, j)
    if (e < 0) return null
    if (e === n) break
    if (punct(tokens[e]) !== ',') return null
    j = e + 1
    if (j === n) return names === 'tables' ? { phrases: [], eager: true, only: false, kind: 'tables' } : null
  }
  const after = o.type === 'INDEX' ? [P('ON', 'mysql mssql', true)] : []
  return offered([P('CASCADE', 'postgres duckdb'), P('RESTRICT', 'postgres duckdb'), ...after], { kind: 'ddl' })
}

/** CREATE …  @param {Token[]} tokens @returns {Follow | null} */
function createFollow(tokens) {
  const n = tokens.length
  if (n === 1) return forced(CREATE_OBJECTS)
  let i = 1
  if (kw(tokens[i]) === 'OR') {
    if (n === 2) return forced([P('REPLACE', 'postgres mysql clickhouse duckdb'), P('ALTER', 'mssql')])
    i = 3
    if (n === 3) return forced(OR_REPLACE_OBJECTS)
  }
  while (['TEMP', 'TEMPORARY', 'UNLOGGED', 'GLOBAL', 'LOCAL'].includes(kw(tokens[i]))) {
    i++
    if (n === i) return forced([P('TABLE'), P('VIEW', 'postgres sqlite duckdb'), P('SEQUENCE', 'postgres')])
  }
  if (kw(tokens[i]) === 'UNIQUE') {
    i++
    if (n === i) return forced([P('INDEX')])
  }
  if (kw(tokens[i]) === 'MATERIALIZED' && n === i + 1) return forced([P('VIEW')])
  const o = objectAt(tokens, i)
  if (!o) return null
  i = o.end
  if (o.type === 'INDEX' && kw(tokens[i]) === 'CONCURRENTLY') i++
  const ifNot = /** @type {Record<string, string>} */ (IF_NOT_EXISTS)[o.type]
  if (n === i) {
    // A new name goes here: no existing names, just the words that can come first.
    const words = ifNot !== undefined ? [P('IF NOT EXISTS', ifNot)] : []
    if (o.type === 'INDEX') words.push(P('CONCURRENTLY', 'postgres'), P('ON', 'postgres duckdb', true))
    if (o.type === 'SCHEMA') words.push(P('AUTHORIZATION', 'postgres mssql'))
    return { phrases: words, eager: false, only: true }
  }
  if (kw(tokens[i]) === 'IF') {
    if (n === i + 1) return forced([P('NOT EXISTS')])
    if (kw(tokens[i + 1]) === 'NOT' && n === i + 2) return forced([P('EXISTS')])
    if (kw(tokens[i + 1]) === 'NOT' && kw(tokens[i + 2]) === 'EXISTS') {
      i += 3
      if (n === i) return { phrases: [], eager: false, only: true }
    }
  }
  const e = nameEnd(tokens, i)
  if (e !== n) return null
  if (o.type === 'INDEX') return forced([P('ON', '', true)])
  if (o.type === 'VIEW' || o.type === 'MATERIALIZED VIEW') return forced([P('AS')])
  return null
}

/**
 * ALTER …  (ALTER TABLE's own positions mostly come from alterTable(); this
 * adds the phrases and which table's columns go where.)
 * @param {Token[]} tokens @returns {Follow | null}
 */
function alterFollow(tokens) {
  const n = tokens.length
  if (n === 1) return forced(ALTER_OBJECTS)
  if (kw(tokens[1]) !== 'TABLE') return null
  let i = 2
  if (n === 2) return offered([P('IF EXISTS', 'postgres mssql duckdb', true), P('ONLY', 'postgres', true)], { kind: 'tables' })
  if (kw(tokens[i]) === 'IF') {
    if (n === 3) return forced([P('EXISTS', '', true)])
    if (kw(tokens[3]) !== 'EXISTS') return null
    i = 4
    if (n === 4) return { phrases: [P('ONLY', 'postgres', true)], eager: true, only: false, kind: 'tables' }
  }
  if (kw(tokens[i]) === 'ONLY') {
    i++
    if (n === i) return { phrases: [], eager: true, only: false, kind: 'tables' }
  }
  const end = nameEnd(tokens, i)
  if (end < 0) return null
  const table = lastName(tokens, end)
  const act = listEntry(tokens, end)
  // Bare ADD / DROP / ALTER / RENAME take a column straight after; OWNER,
  // MODIFY and CHANGE only come as their phrases.
  if (!act.length) return { phrases: ALTER_TABLE_ACTIONS, eager: true, only: false, next: ['ADD', 'DROP', 'ALTER', 'RENAME', 'SET'] }
  const verb = kw(act[0])
  const said = kw(act[1])
  const cols = { kind: /** @type {const} */ ('columns'), columnsOf: table }
  if (verb === 'ADD') {
    if (act.length === 1) return offered([P('COLUMN'), P('CONSTRAINT'), P('PRIMARY KEY'), P('FOREIGN KEY'), P('UNIQUE'), P('CHECK')])
    if (said === 'COLUMN' && act.length === 2) return offered([P('IF NOT EXISTS', 'postgres duckdb')])
    if (said === 'COLUMN' && kw(act[2]) === 'IF') {
      if (act.length === 3) return forced([P('NOT EXISTS')])
      if (act.length === 4 && kw(act[3]) === 'NOT') return forced([P('EXISTS')])
    }
    return null
  }
  if (verb === 'DROP') {
    if (act.length === 1) return { phrases: [P('COLUMN', '', true), P('CONSTRAINT'), P('IF EXISTS', 'postgres duckdb', true)], eager: true, only: false, ...cols }
    let k = said === 'COLUMN' ? 2 : 1
    if (kw(act[k]) === 'IF') {
      if (act.length === k + 1) return forced([P('EXISTS', '', true)])
      if (kw(act[k + 1]) !== 'EXISTS') return null
      k += 2
    }
    if (said === 'CONSTRAINT') return act.length === 2 ? offered([P('IF EXISTS', 'postgres mssql duckdb')], { kind: 'ddl' }) : null
    if (act.length === k) return { phrases: k === 2 && said === 'COLUMN' ? [P('IF EXISTS', 'postgres mssql duckdb', true)] : [], eager: true, only: false, ...cols }
    if (act.length === k + 1 && isName(act[k])) return offered([P('CASCADE', 'postgres duckdb'), P('RESTRICT', 'postgres duckdb')], { kind: 'ddl' })
    return null
  }
  if (verb === 'RENAME') {
    if (act.length === 1) return { phrases: [P('TO'), P('COLUMN', '', true), P('CONSTRAINT', 'postgres')], eager: true, only: false, ...cols }
    if (said === 'TO') return act.length === 2 ? { phrases: [], eager: false, only: true } : null
    const k = said === 'COLUMN' ? 2 : 1
    if (act.length === k) return { phrases: [], eager: true, only: false, ...cols }
    if (act.length === k + 1 && isName(act[k])) return forced([P('TO')])
    if (act.length === k + 2 && kw(act[k + 1]) === 'TO') return { phrases: [], eager: false, only: true }
    return null
  }
  if (verb === 'ALTER' || verb === 'MODIFY' || verb === 'CHANGE') {
    if (act.length === 1) return { phrases: [P('COLUMN', '', true)], eager: true, only: false, ...cols }
    const k = said === 'COLUMN' ? 2 : 1
    if (act.length === k) return { phrases: [], eager: true, only: false, ...cols }
    if (verb !== 'ALTER') return null
    if (act.length === k + 1) return { phrases: ALTER_COLUMN_ACTIONS, eager: true, only: false, next: ['TYPE'], typesFor: ['mssql'] }
    const last = kw(act.at(-1))
    const before = kw(act.at(-2))
    if (act.length === k + 2 && last === 'SET') return forced([P('DEFAULT'), P('NOT NULL'), P('DATA TYPE')])
    if (act.length === k + 2 && last === 'DROP') return forced([P('DEFAULT'), P('NOT NULL')])
    if (last === 'NOT' && (before === 'SET' || before === 'DROP')) return forced([P('NULL')])
    if (last === 'DATA' && before === 'SET') return forced([P('TYPE')])
    return null
  }
  if (verb === 'OWNER' && act.length === 1) return forced([P('TO')])
  if (verb === 'SET' && act.length === 1) return offered([P('SCHEMA', 'postgres duckdb'), P('TABLESPACE', 'postgres'), P('LOGGED', 'postgres'), P('UNLOGGED', 'postgres')], { kind: 'ddl' })
  return null
}

/**
 * The table an INSERT writes to, by its last name part, or null.
 * @param {Token[]} tokens
 */
function insertTable(tokens) {
  const into = tokens.findIndex((t) => kw(t) === 'INTO')
  if (into < 0) return null
  const e = nameEnd(tokens, into + 1)
  return e < 0 ? null : lastName(tokens, e)
}

/**
 * Queries and DML, read from the last few tokens: the pairs (ORDER BY,
 * IS NOT NULL, LEFT JOIN, ON CONFLICT DO …) and an INSERT's column list.
 * @param {Token[]} tokens @param {string} clause @param {string} verb
 * @returns {Follow | null}
 */
function queryFollow(tokens, clause, verb) {
  const n = tokens.length
  const last = kw(tokens[n - 1])
  const prev = kw(tokens[n - 2])
  if (n === 1 && verb === 'INSERT') return { phrases: [P('INTO', '', true), P('IGNORE INTO', 'mysql', true), P('OR IGNORE INTO', 'sqlite', true), P('OR REPLACE INTO', 'sqlite', true)], eager: true, only: true }
  if (n === 1 && verb === 'DELETE') return forced([P('FROM', '', true)])
  if (n === 1 && verb === 'TRUNCATE') return offered([P('TABLE', 'postgres mysql mssql duckdb clickhouse', true)], { kind: 'tables' })
  if (n === 1 && verb === 'WITH') return offered([P('RECURSIVE', 'postgres mysql sqlite duckdb')], { kind: 'ddl' })
  if (verb === 'UPDATE') {
    // UPDATE [ONLY] t |
    const at = kw(tokens[1]) === 'ONLY' ? 2 : 1
    const e = nameEnd(tokens, at)
    if (e === n || (e > 0 && e + 1 === n && isName(tokens[e]) && !CLAUSES.has(kw(tokens[e])))) return { phrases: [P('SET', '', true)], eager: true, only: false }
    if (last === 'SET' && clause === 'SET') return { phrases: [], eager: true, only: false }
  }
  if (verb === 'INSERT') {
    const table = insertTable(tokens)
    const into = tokens.findIndex((t) => kw(t) === 'INTO')
    const e = into < 0 ? -1 : nameEnd(tokens, into + 1)
    // INSERT INTO t |
    if (e === n) return { phrases: [P('VALUES'), P('SELECT'), P('DEFAULT VALUES', 'postgres sqlite mssql duckdb')], eager: true, only: false }
    // INSERT INTO t (a, |: only t's columns.
    if (e > 0 && punct(tokens[e]) === '(' && openParen(tokens, n) === e && (punct(tokens[n - 1]) === '(' || punct(tokens[n - 1]) === ',') && table) {
      return { phrases: [], eager: true, only: false, kind: 'columns', columnsOf: table }
    }
    // INSERT INTO t (a, b) |
    if (e > 0 && punct(tokens[n - 1]) === ')' && openParen(tokens, n - 1) === e) return { phrases: [P('VALUES'), P('SELECT')], eager: true, only: false }
    // … VALUES (…) ON |
    if (last === 'ON' && tokens.some((t) => kw(t) === 'VALUES')) {
      return forced([P('CONFLICT', 'postgres sqlite duckdb'), P('DUPLICATE KEY UPDATE', 'mysql', true)])
    }
    const conflict = tokens.findLastIndex((t) => kw(t) === 'CONFLICT')
    if (conflict > 0 && kw(tokens[conflict - 1]) === 'ON') {
      const doPhrases = [P('DO NOTHING'), P('DO UPDATE SET', '', true)]
      if (n === conflict + 1) return forced([...doPhrases, P('ON CONSTRAINT', 'postgres')])
      if (punct(tokens[n - 1]) === ')' && openParen(tokens, n - 1) === conflict + 1) return forced(doPhrases)
      if (last === 'DO') return forced([P('NOTHING'), P('UPDATE SET', '', true)])
      if (last === 'UPDATE' && prev === 'DO') return forced([P('SET', '', true)])
      if (last === 'SET' && prev === 'UPDATE' && table) return { phrases: [], eager: true, only: false, kind: 'columns', columnsOf: table }
    }
    if (last === 'DUPLICATE' && prev === 'ON') return forced([P('KEY UPDATE', '', true)])
    if (last === 'KEY' && prev === 'DUPLICATE') return forced([P('UPDATE', '', true)])
    if (last === 'UPDATE' && prev === 'KEY' && table) return { phrases: [], eager: true, only: false, kind: 'columns', columnsOf: table }
  }
  if (last === 'ORDER' || last === 'GROUP' || last === 'PARTITION') return forced([P('BY', '', true)])
  if (last === 'IS') return forced([P('NULL'), P('NOT NULL'), P('DISTINCT FROM'), P('TRUE'), P('FALSE')])
  if (last === 'NOT' && prev === 'IS') return forced([P('NULL'), P('DISTINCT FROM'), P('TRUE'), P('FALSE')])
  if (last === 'DISTINCT' && (prev === 'IS' || (prev === 'NOT' && kw(tokens[n - 3]) === 'IS'))) return forced([P('FROM')])
  if (last === 'NULLS') return forced([P('FIRST'), P('LAST')])
  if (last === 'UNION' || last === 'EXCEPT' || last === 'INTERSECT') return forced([P('ALL'), P('SELECT'), P('DISTINCT')])
  if (last === 'NOT' && PREDICATE_CLAUSES.has(clause)) {
    return offered([P('NULL'), P('IN'), P('LIKE'), P('ILIKE', 'postgres duckdb'), P('BETWEEN'), P('EXISTS')])
  }
  if (last === 'DISTINCT' && prev === 'SELECT') return offered([P('ON', 'postgres duckdb')])
  if (clause === 'FROM' || clause === 'JOIN' || clause === 'ON' || clause === 'WHERE') {
    if (last === 'LEFT' || last === 'RIGHT' || last === 'FULL') return forced([P('JOIN', '', true), P('OUTER JOIN', '', true)])
    if (last === 'INNER' || last === 'CROSS' || last === 'NATURAL' || (last === 'OUTER' && ['LEFT', 'RIGHT', 'FULL'].includes(prev))) return forced([P('JOIN', '', true)])
  }
  if (clause === 'JOIN') {
    // JOIN t [AS] [alias] |
    const join = tokens.findLastIndex((t) => kw(t) === 'JOIN')
    let e = nameEnd(tokens, join + 1)
    if (e > 0 && kw(tokens[e]) === 'AS') e++
    if (e > 0 && e < n && isName(tokens[e]) && !OPEN_WORDS.has(kw(tokens[e]))) e++
    if (e === n) return offered([P('ON'), P('USING')], { kind: 'ddl' })
  }
  if (last === 'CASE') return offered([P('WHEN')])
  return null
}

/**
 * What can come next at the end of `tokens`, by the statement's grammar.
 * @param {Token[]} tokens @param {string} clause @param {string} verb
 * @returns {Follow | null}
 */
function followAt(tokens, clause, verb) {
  if (!tokens.length) return null
  if (verb === 'DROP') return dropFollow(tokens)
  if (verb === 'CREATE') return createFollow(tokens)
  if (verb === 'ALTER') return alterFollow(tokens)
  return queryFollow(tokens, clause, verb)
}

/**
 * The name of the CREATE FUNCTION being written, or null.
 * @param {Token[]} tokens
 */
function routineName(tokens) {
  if (kw(tokens[0]) !== 'CREATE') return null
  const f = tokens.findIndex((x, i) => i < 5 && kw(x) === 'FUNCTION')
  if (f < 0 || !isName(tokens[f + 1])) return null
  return /** @type {Token} */ (tokens[nameEnd(tokens, f + 1) - 1]).v
}

/**
 * @param {string} text the document up to the caret (a bounded slice is fine)
 * @returns {SqlCompletionContext | null} null where nothing should be offered
 */
export function sqlCompletionContext(text) {
  // Typing a word changes only that word: the text before it scans the same,
  // so a long statement is tokenized once per word, not once per keystroke.
  const word = /[A-Za-z_][\w$]*$/.exec(text)?.[0] ?? ''
  const wordy = word !== '' && !/[\w$]/.test(text[text.length - word.length - 1] ?? '')
  const scanned = wordy ? scanHead(text.slice(0, text.length - word.length)) : scan(text)
  if (!scanned) return null
  const { open } = scanned
  let tokens = scanned.tokens
  // A body's statements end in `;`, which clears the tokens: the head the
  // statement opened with (CREATE TRIGGER ... ON t) is kept apart.
  const head = scanned.head ?? tokens

  let prefix = ''
  let from = text.length
  /** @type {string | null} */
  let quote = null
  if (open) {
    quote = open.quote
    from = open.start + 1
    prefix = text.slice(from)
  } else if (wordy) {
    // `1.e`: the word is the tail of a number.
    if (tokens.at(-1)?.t === 'num' && text[text.length - word.length - 1] === '.') return null
    prefix = word
    from = text.length - word.length
  } else {
    const m = /[\w$]+$/.exec(text)
    if (m && tokens.at(-1)?.t === 'word') {
      prefix = m[0]
      from = text.length - prefix.length
      tokens = tokens.slice(0, -1)
    } else if (m) {
      // A number being typed, e.g. `= 4`.
      return null
    }
  }

  const last = tokens.at(-1)
  let clause = ''
  for (let i = tokens.length - 1; i >= 0; i--) {
    const k = kw(tokens[i])
    if (CLAUSES.has(k)) { clause = k; break }
  }
  // ORDER BY / GROUP BY: the clause is the pair.
  if (clause === 'BY') clause = kw(tokens.findLast((t) => kw(t) === 'ORDER' || kw(t) === 'GROUP')) || 'BY'

  const verb = kw(tokens[0])
  // CREATE INDEX name ON table: ON names a table there, not a join condition.
  const indexOn = verb === 'CREATE' && tokens.some((t) => kw(t) === 'INDEX')

  /** @type {SqlCompletionContext['kind']} */
  let kind
  let qualifier = null
  /** Keywords for this position when the clause table does not know it (DDL). @type {string[] | null} */
  let nextHere = null
  /** @type {{ kind: 'types' | 'ddl' | 'tables', next: string[] } | null} */
  let ddl = null
  if (last?.v === '.' && isName(tokens.at(-2))) {
    kind = 'qualified'
    qualifier = /** @type {Token} */ (tokens.at(-2)).v
  } else if (!tokens.length) {
    kind = 'statement'
  } else if (!scanned.head && (ddl = triggerHead(tokens))) {
    // CREATE TRIGGER's head: AFTER UPDATE | is an event list, ON | a table.
    kind = ddl.kind
    nextHere = ddl.next
  } else if (TABLE_KEYWORDS.has(kw(last)) || (clause === 'FROM' && last?.v === ',') || (indexOn && kw(last) === 'ON')) {
    kind = 'tables'
  } else if ((ddl = typeGoesHere(tokens) ? { kind: 'types', next: [] } : createTableEntry(tokens) ?? alterTable(tokens))) {
    kind = ddl.kind
    nextHere = ddl.next
  } else if ((clause === 'TABLE' || clause === 'TRUNCATE') && endsWithTableName(tokens)) {
    // DROP TABLE name |, CREATE TABLE name |: the statement's own words, not names.
    kind = 'ddl'
    nextHere = verb === 'CREATE' ? ['AS'] : verb === 'DROP' || verb === 'TRUNCATE' ? ['CASCADE', 'RESTRICT'] : []
  } else if (
    (clause === 'UPDATE' || clause === 'DELETE' || clause === 'INSERT') ||
    (clause === 'INTO' && (isName(last) || last?.v === ')')) ||
    (clause === 'FROM' && isName(last)) ||
    clause === 'VALUES'
  ) {
    // Past the table: what comes next is a keyword (SET, FROM, VALUES...).
    kind = 'keywords'
  } else {
    kind = 'columns'
  }
  // The grammar's view of what follows: phrases, and sometimes a narrower kind.
  /** @type {Follow | null} */
  const follow = quote || kind === 'qualified' ? null : followAt(tokens, clause, verb)
  if (follow?.kind) kind = follow.kind
  if (follow?.next) nextHere = follow.next
  if (follow?.only) { kind = 'ddl'; nextHere = [] }

  // A quote only ever holds a name.
  if (quote && (kind === 'keywords' || kind === 'statement' || kind === 'types' || kind === 'ddl')) kind = 'columns'

  // Is the thing before the caret a finished value? Then the clause's next
  // word (FROM after `SELECT id`, WHERE after `SET a = 1`) is the likely one;
  // right after SELECT or `=` it is a column instead.
  const afterExpr =
    last?.t === 'qid' || last?.t === 'str' || last?.t === 'num' ||
    last?.v === ')' || last?.v === '*' ||
    (last?.t === 'word' && !OPEN_WORDS.has(kw(last)))

  /** @type {SqlCompletionContext['predicateColumn']} */
  let predicateColumn = null
  /** @type {SqlCompletionContext['comparedColumn']} */
  let comparedColumn = null
  if (!quote && kind === 'columns' && /\s$/.test(text.slice(0, from))) {
    if (PREDICATE_CLAUSES.has(clause)) {
      // `WHERE price |`: the column, with what a condition can start with before it.
      const col = columnBefore(tokens, tokens.length)
      const before = col ? tokens[col.start - 1] : undefined
      if (col && (PREDICATE_STARTS.has(kw(before)) || before?.v === '(')) {
        predicateColumn = { name: col.name, qualifier: col.qualifier }
      }
    }
    if (PREDICATE_CLAUSES.has(clause) || clause === 'SET') {
      // `WHERE price >= |`, `SET status = |`, `WHERE name LIKE |`.
      let j = tokens.length
      let operator = ''
      while (j > 0 && tokens[j - 1].t === 'punct' && /^[=<>!~]$/.test(tokens[j - 1].v)) operator = tokens[--j].v + operator
      if (!operator && (kw(tokens[j - 1]) === 'LIKE' || kw(tokens[j - 1]) === 'ILIKE')) operator = kw(tokens[--j])
      const col = operator ? columnBefore(tokens, j) : null
      if (col) comparedColumn = { name: col.name, qualifier: col.qualifier, operator }
    }
  }

  return {
    kind,
    from,
    prefix,
    quote,
    qualifier,
    clause,
    // RETURNING only ends a write: after a SELECT's WHERE it is an error.
    next: nextHere ?? (NEXT[clause] ?? []).filter((k) => k !== 'RETURNING' || ['UPDATE', 'DELETE', 'INSERT'].includes(verb)),
    afterExpr,
    tables: referencedTables(tokens, indexOn),
    predicateColumn,
    comparedColumn,
    verb,
    rowTable: triggerTable(head),
    routine: routineName(head),
    phrases: follow?.phrases ?? [],
    eager: follow?.eager ?? false,
    columnsOf: follow?.columnsOf ?? null,
    names: follow?.names ?? null,
    typesFor: follow?.typesFor ?? null,
    lower: tokens[0]?.t === 'word' && /[a-z]/.test(tokens[0].v) && tokens[0].v === tokens[0].v.toLowerCase(),
  }
}
