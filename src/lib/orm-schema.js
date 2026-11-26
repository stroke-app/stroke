/**
 * The live database, written out as ORM schema code.
 *
 * Reading a schema in the shape your ORM uses it is a different job from reading
 * DDL: you want the model names, the field types you'll actually type, and above
 * all the *relations* spelled out in both directions. So this takes what the
 * introspection commands already return - columns, indexes, enums - and renders
 * a Prisma schema or a Drizzle schema module from it.
 *
 * Output targets current syntax: Prisma with `@map`/`@@map` so DB names survive
 * camelCasing, and Drizzle with the array-returning table extras callback
 * (`(table) => [...]`) and a `relations()` block per table.
 *
 * Everything here is pure: introspection rows in, source text out.
 */

// ── Model ─────────────────────────────────────────────────────────────────────

/**
 * @typedef {object} OrmColumn
 * @property {string} name          - column name in the database
 * @property {string} field         - camelCased name used in code
 * @property {string} dataType      - raw type from introspection, e.g. `varchar(255)`
 * @property {string} baseType      - lowercased type with length/precision stripped
 * @property {number[]} args        - numeric type arguments, e.g. [10, 2] for numeric(10,2)
 * @property {boolean} isArray
 * @property {boolean} nullable
 * @property {string | null} defaultValue
 * @property {boolean} autoIncrement
 * @property {boolean} isPrimary
 * @property {boolean} isUnique
 * @property {string | null} comment
 * @property {{ table: string, column: string } | null} references
 * @property {string | null} enumName
 *
 * @typedef {object} OrmTable
 * @property {string} name
 * @property {string} model         - PascalCase model / camelCase export base
 * @property {OrmColumn[]} columns
 * @property {string[]} primaryKey
 * @property {Array<{ name: string, columns: string[], unique: boolean }>} indexes
 *
 * @typedef {object} OrmSchemaModel
 * @property {string} schema
 * @property {'postgres'|'mysql'|'sqlite'} family
 * @property {string} dialect       - the connection's own type, e.g. `cockroachdb`
 * @property {OrmTable[]} tables
 * @property {Array<{ name: string, values: string[] }>} enums
 * @property {string[]} skipped     - views and other non-tables left out
 */

/** Engines each ORM can actually describe. */
export const PRISMA_ENGINES = ['postgres', 'cockroachdb', 'mysql', 'mariadb', 'sqlite', 'libsql', 'd1', 'mssql']
export const DRIZZLE_ENGINES = ['postgres', 'cockroachdb', 'mysql', 'mariadb', 'sqlite', 'libsql', 'd1']

/** @param {string} dbType */
function ormEngineFamily(dbType) {
  if (dbType === 'mysql' || dbType === 'mariadb') return 'mysql'
  if (dbType === 'sqlite' || dbType === 'libsql' || dbType === 'd1') return 'sqlite'
  if (dbType === 'mssql') return 'mssql'
  return 'postgres'
}

// ── Naming ────────────────────────────────────────────────────────────────────

const RESERVED_FIELD = new Set(['model', 'enum', 'type', 'datasource', 'generator'])

/** `created_at` / `CreatedAt` / `created at` → `createdAt`. */
export function toCamel(name) {
  const parts = String(name).split(/[^A-Za-z0-9]+/).filter(Boolean)
  if (parts.length === 0) return 'field'
  // An already-camel or PascalCase single word keeps its internal casing.
  const head = parts[0]
  const first = /[a-z]/.test(head) ? head[0].toLowerCase() + head.slice(1) : head.toLowerCase()
  return first + parts.slice(1).map((p) => p[0].toUpperCase() + p.slice(1)).join('')
}

/**
 * The Drizzle const for a Postgres enum: `order_status` → `orderStatusEnum`.
 * A type already named for what it is (`order_status_enum`, the usual
 * TypeORM name) stays `orderStatusEnum`, not `orderStatusEnumEnum`.
 * @param {string} name
 */
export function enumConstName(name) {
  const base = toCamel(name)
  return /Enum$/.test(base) ? base : `${base}Enum`
}

/** `user_profiles` → `UserProfiles`. */
export function toPascal(name) {
  const camel = toCamel(name)
  return camel[0].toUpperCase() + camel.slice(1)
}

/** True when a name can appear unquoted as an identifier in generated code. */
function isPlainIdent(name) {
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(name)
}

/** Property key for an object literal - quoted only when it has to be. */
function propKey(name) {
  return isPlainIdent(name) ? name : JSON.stringify(name)
}

function quote(text) {
  return `'${String(text).replace(/\\/g, '\\\\').replace(/'/g, "\\'")}'`
}

// ── Type parsing ──────────────────────────────────────────────────────────────

/** Splits `numeric(10,2)` into `numeric` + [10, 2], and unwraps `text[]`. */
function parseType(raw) {
  let type = String(raw ?? '').trim()
  let isArray = false
  if (type.endsWith('[]')) {
    isArray = true
    type = type.slice(0, -2)
  }
  const m = type.match(/^([^(]+)\(([^)]*)\)\s*(.*)$/)
  let args = []
  if (m) {
    args = m[2].split(',').map((a) => Number(a.trim())).filter((n) => Number.isFinite(n))
    type = (m[1] + (m[3] ? ` ${m[3]}` : '')).trim()
  }
  return { baseType: type.toLowerCase(), args, isArray }
}

/**
 * Builds the intermediate model every renderer reads.
 *
 * @param {object} input
 * @param {string} input.schema
 * @param {string} input.dbType                     connection type (`postgres`, `mariadb`, …)
 * @param {Array<{ table: string, columns: any[] }>} input.structures  getSchemaColumnStructure()
 * @param {any[]} [input.indexes]                   listIndexes()
 * @param {Array<{ name: string, values: string[] }>} [input.enums]    listEnums()
 * @param {Array<{ name: string, kind?: string }>} [input.tables]      listTables(), to drop views
 * @returns {OrmSchemaModel}
 */
export function buildOrmSchemaModel({ schema, dbType, structures, indexes = [], enums = [], tables = [] }) {
  const family = ormEngineFamily(dbType)
  const enumNames = new Set(enums.map((e) => e.name))

  // Views and materialized views have no primary key or relations to describe,
  // so they're listed as skipped rather than rendered as broken models.
  const kindByName = new Map(tables.map((t) => [t.name, t.kind ?? 'table']))
  const isTable = (name) => {
    const kind = kindByName.get(name)
    return kind === undefined || kind === 'table' || kind === 'partitioned_table' || kind === 'BASE TABLE'
  }

  const byTable = new Map()
  for (const idx of indexes) {
    const list = byTable.get(idx.tableName) ?? []
    list.push(idx)
    byTable.set(idx.tableName, list)
  }

  /** @type {OrmTable[]} */
  const out = []
  /** @type {string[]} */
  const skipped = []
  const usedModels = new Set()

  for (const entry of structures) {
    if (!isTable(entry.table)) {
      skipped.push(entry.table)
      continue
    }
    const tableIndexes = byTable.get(entry.table) ?? []
    const splitCols = (idx) => String(idx.columns ?? '').split(',').map((c) => c.trim()).filter(Boolean)
    const primaryKey = tableIndexes.filter((i) => i.isPrimary).flatMap(splitCols)
    const uniqueSingles = new Set(
      tableIndexes
        .filter((i) => i.isUnique && !i.isPrimary)
        .map(splitCols)
        .filter((cols) => cols.length === 1)
        .map((cols) => cols[0]),
    )

    let model = toPascal(entry.table)
    // Two tables can PascalCase to the same model (`user_role` / `userRole`);
    // the second keeps its raw name so the output still compiles.
    if (usedModels.has(model)) model = toPascal(`${entry.table}_${out.length}`)
    usedModels.add(model)

    const usedFields = new Set()
    const columns = (entry.columns ?? []).map((c) => {
      const { baseType, args, isArray } = parseType(c.dataType)
      let field = toCamel(c.name)
      if (RESERVED_FIELD.has(field) || usedFields.has(field)) field = `${field}_`
      usedFields.add(field)
      const def = c.columnDefault ?? null
      return /** @type {OrmColumn} */ ({
        name: c.name,
        field,
        dataType: c.dataType ?? '',
        baseType,
        args,
        isArray,
        nullable: !!c.isNullable,
        defaultValue: def,
        autoIncrement: isAutoIncrement(def, baseType, family),
        isPrimary: primaryKey.includes(c.name),
        isUnique: uniqueSingles.has(c.name),
        comment: c.comment ?? null,
        references: parseForeignKey(c.foreignKey),
        enumName: enumNames.has(c.dataType) ? c.dataType : null,
      })
    })

    out.push({
      name: entry.table,
      model,
      columns,
      // SQLite's index list carries no columns, so `rowid`-style tables fall back
      // to whichever column the introspection marked as the key.
      primaryKey: primaryKey.length ? primaryKey : columns.filter((c) => c.isPrimary).map((c) => c.name),
      indexes: tableIndexes
        .filter((i) => !i.isPrimary)
        .map((i) => ({ name: i.name, columns: splitCols(i), unique: !!i.isUnique }))
        .filter((i) => i.columns.length > 0),
    })
  }

  const known = new Set(out.map((t) => t.name))
  // A relation to a view (or to another schema) can't be expressed, so drop it
  // rather than emit a reference to a model that isn't in the file.
  for (const t of out) {
    for (const c of t.columns) {
      if (c.references && !known.has(c.references.table)) c.references = null
    }
  }

  return {
    schema,
    family: family === 'mssql' ? 'postgres' : family,
    dialect: dbType,
    tables: out,
    enums: enums.filter((e) => e.values?.length),
    skipped,
  }
}

/** `public.users.id` → `{ table: 'users', column: 'id' }`. */
function parseForeignKey(fk) {
  if (!fk) return null
  const parts = String(fk).split('.')
  if (parts.length < 2) return null
  return { table: parts[parts.length - 2], column: parts[parts.length - 1] }
}

function isAutoIncrement(def, baseType, family) {
  if (family === 'mysql') return /auto_increment/i.test(String(def ?? ''))
  if (family === 'sqlite') return false // AUTOINCREMENT lives in the DDL, not the default
  return /^nextval\(/i.test(String(def ?? '')) || /^(serial|bigserial|smallserial)$/.test(baseType)
}

// ── Relations ─────────────────────────────────────────────────────────────────

/**
 * Every foreign key as a pair of endpoints, with names that don't collide.
 * A relation is one-to-one when the child's FK column is itself unique.
 *
 * @param {OrmSchemaModel} model
 */
function relationsOf(model) {
  const byName = new Map(model.tables.map((t) => [t.name, t]))
  /** @type {Array<{ child: OrmTable, parent: OrmTable, column: OrmColumn, childField: string, parentField: string, one: boolean, name: string | null }>} */
  const rels = []

  for (const child of model.tables) {
    for (const column of child.columns) {
      if (!column.references) continue
      const parent = byName.get(column.references.table)
      if (!parent) continue
      rels.push({
        child,
        parent,
        column,
        childField: '',
        parentField: '',
        one: column.isUnique || (child.primaryKey.length === 1 && child.primaryKey[0] === column.name),
        name: null,
      })
    }
  }

  // Name both ends only once every pair is known: a second FK between the same
  // two tables forces explicit relation names and disambiguated field names.
  const pairCount = new Map()
  for (const r of rels) {
    const key = `${r.child.name}→${r.parent.name}`
    pairCount.set(key, (pairCount.get(key) ?? 0) + 1)
  }

  const taken = new Map() // table name → Set of field names already used
  const claim = (table, wanted) => {
    const used = taken.get(table.name) ?? new Set(table.columns.map((c) => c.field))
    taken.set(table.name, used)
    let field = wanted
    let n = 2
    while (used.has(field)) field = `${wanted}${n++}`
    used.add(field)
    return field
  }

  for (const r of rels) {
    const key = `${r.child.name}→${r.parent.name}`
    const ambiguous = (pairCount.get(key) ?? 0) > 1
    // `author_id` reads as `author`; a column that isn't an `_id` falls back to
    // the parent's own name.
    const stem = r.column.name.replace(/_?id$/i, '')
    const base = (stem && stem !== r.column.name ? toCamel(stem) : toCamel(r.parent.name)) || toCamel(r.parent.name)
    r.childField = claim(r.child, base)
    // The back-reference is named after the child table - unless two keys point
    // here, in which case the key's own name is what tells them apart
    // (`authorPosts` / `reviewerPosts`, never `posts` / `posts2`).
    const childName = r.one ? toCamel(r.child.name) : pluralField(r.child.name)
    r.parentField = claim(
      r.parent,
      ambiguous ? `${base}${childName[0].toUpperCase()}${childName.slice(1)}` : childName,
    )
    r.name = ambiguous ? `${r.child.model}_${r.column.field}` : null
  }
  return rels
}

/**
 * A list-shaped field name: `post` → `posts`, `address` → `addresses`.
 * A table that is already plural keeps its name - `posts` must not become
 * `postses`. `ss` is the tell that a trailing `s` is part of the word.
 */
function pluralField(tableName) {
  const base = toCamel(tableName)
  if (/ss$/.test(base)) return `${base}es`
  if (/s$/.test(base)) return base
  if (/(x|z|ch|sh)$/.test(base)) return `${base}es`
  if (/[^aeiou]y$/.test(base)) return `${base.slice(0, -1)}ies`
  return `${base}s`
}

// ── Prisma ────────────────────────────────────────────────────────────────────

const PRISMA_PROVIDER = {
  postgres: 'postgresql',
  cockroachdb: 'cockroachdb',
  mysql: 'mysql',
  mariadb: 'mysql',
  sqlite: 'sqlite',
  libsql: 'sqlite',
  d1: 'sqlite',
  mssql: 'sqlserver',
}

/** @param {OrmColumn} c @param {OrmSchemaModel} model */
function prismaType(c, model) {
  if (c.enumName) return toPascal(c.enumName)
  const t = c.baseType
  const map = {
    // integers
    int2: 'Int', smallint: 'Int', int4: 'Int', int: 'Int', integer: 'Int', mediumint: 'Int', serial: 'Int', smallserial: 'Int',
    int8: 'BigInt', bigint: 'BigInt', bigserial: 'BigInt',
    tinyint: model.family === 'mysql' && c.args[0] === 1 ? 'Boolean' : 'Int',
    // exact + floating point
    numeric: 'Decimal', decimal: 'Decimal', money: 'Decimal',
    float4: 'Float', real: 'Float', float8: 'Float', 'double precision': 'Float', double: 'Float', float: 'Float',
    // text
    text: 'String', varchar: 'String', 'character varying': 'String', bpchar: 'String', char: 'String', character: 'String',
    citext: 'String', name: 'String', tinytext: 'String', mediumtext: 'String', longtext: 'String', nvarchar: 'String', clob: 'String',
    uuid: 'String', inet: 'String', cidr: 'String', macaddr: 'String', xml: 'String',
    // booleans, dates, structured
    bool: 'Boolean', boolean: 'Boolean',
    date: 'DateTime', timestamp: 'DateTime', timestamptz: 'DateTime', 'timestamp with time zone': 'DateTime',
    'timestamp without time zone': 'DateTime', datetime: 'DateTime', time: 'DateTime', timetz: 'DateTime', year: 'Int',
    json: 'Json', jsonb: 'Json',
    bytea: 'Bytes', blob: 'Bytes', bytes: 'Bytes', binary: 'Bytes', varbinary: 'Bytes', longblob: 'Bytes', image: 'Bytes',
    // SQL Server spellings
    bit: 'Boolean', datetime2: 'DateTime', smalldatetime: 'DateTime', datetimeoffset: 'DateTime',
    uniqueidentifier: 'String', nchar: 'String', ntext: 'String',
  }
  return map[t] ?? null
}

/** `@db.VarChar(255)` and friends - only where they carry real information. */
function prismaNativeType(c, model) {
  // Native-type attributes are per-provider; only the Postgres set is emitted.
  if (model.dialect !== 'postgres' && model.dialect !== 'cockroachdb') return ''
  const t = c.baseType
  if ((t === 'varchar' || t === 'character varying') && c.args[0]) return ` @db.VarChar(${c.args[0]})`
  if ((t === 'bpchar' || t === 'char') && c.args[0]) return ` @db.Char(${c.args[0]})`
  if ((t === 'numeric' || t === 'decimal') && c.args.length === 2) return ` @db.Decimal(${c.args[0]}, ${c.args[1]})`
  if (t === 'uuid') return ' @db.Uuid'
  if (t === 'timestamptz') return ' @db.Timestamptz'
  if (t === 'timestamp') return ' @db.Timestamp'
  if (t === 'date') return ' @db.Date'
  if (t === 'time' || t === 'timetz') return ' @db.Time'
  if (t === 'inet') return ' @db.Inet'
  if (t === 'money') return ' @db.Money'
  return ''
}

/** @param {OrmColumn} c */
function prismaDefault(c, type) {
  const raw = c.defaultValue
  if (c.autoIncrement) return ' @default(autoincrement())'
  if (raw == null || raw === '') return ''
  const v = String(raw).trim()
  if (/^(now\(\)|current_timestamp|CURRENT_TIMESTAMP.*)$/i.test(v)) return ' @default(now())'
  if (/^(gen_random_uuid|uuid_generate_v4)\(\)/i.test(v)) return ' @default(uuid())'
  if (/^(true|false)$/i.test(v)) return ` @default(${v.toLowerCase()})`
  if (/^-?\d+(\.\d+)?$/.test(v) && (type === 'Int' || type === 'BigInt' || type === 'Float' || type === 'Decimal')) {
    return ` @default(${v})`
  }
  // `'draft'::text` / `'draft'::"Status"` - a literal wearing a cast.
  const lit = v.match(/^'((?:[^']|'')*)'(::.+)?$/)
  if (lit) {
    const text = lit[1].replace(/''/g, "'")
    if (c.enumName) return isPlainIdent(text) ? ` @default(${text})` : ''
    if (type === 'String') return ` @default(${JSON.stringify(text)})`
    if (type === 'Int' || type === 'Float' || type === 'Decimal' || type === 'BigInt') {
      return /^-?\d+(\.\d+)?$/.test(text) ? ` @default(${text})` : ''
    }
    if (type === 'Boolean' && /^(true|false|0|1)$/i.test(text)) {
      return ` @default(${/^(true|1)$/i.test(text)})`
    }
    if (type === 'Json') return ` @default("${text.replace(/"/g, '\\"')}")`
  }
  // Anything else is a database expression; Prisma keeps it verbatim.
  return ` @default(dbgenerated(${JSON.stringify(v)}))`
}

/**
 * Render the model as a Prisma schema.
 * @param {OrmSchemaModel} model
 * @param {{ url?: string }} [opts]
 */
export function renderPrismaSchema(model, opts = {}) {
  const provider = PRISMA_PROVIDER[model.dialect] ?? 'postgresql'
  const rels = relationsOf(model)
  const lines = []

  lines.push(`// Prisma schema generated by Stroke from the live database.`)
  lines.push(`// Schema: ${model.schema}`)
  if (model.skipped.length) {
    lines.push(`// Views not shown (${model.skipped.length}): ${model.skipped.slice(0, 8).join(', ')}${model.skipped.length > 8 ? ', …' : ''}`)
  }
  lines.push('')
  lines.push('generator client {')
  lines.push('  provider = "prisma-client-js"')
  lines.push('}')
  lines.push('')
  lines.push('datasource db {')
  lines.push(`  provider = "${provider}"`)
  lines.push(`  url      = ${opts.url ? JSON.stringify(opts.url) : 'env("DATABASE_URL")'}`)
  lines.push('}')

  for (const e of model.enums) {
    lines.push('')
    lines.push(`enum ${toPascal(e.name)} {`)
    for (const v of e.values) {
      lines.push(isPlainIdent(v) ? `  ${v}` : `  ${toCamel(v) || 'value'} @map(${JSON.stringify(v)})`)
    }
    if (toPascal(e.name) !== e.name) lines.push(`\n  @@map(${JSON.stringify(e.name)})`)
    lines.push('}')
  }

  for (const t of model.tables) {
    lines.push('')
    lines.push(`model ${t.model} {`)
    /** @type {Array<[string, string, string]>} */
    const fields = []

    for (const c of t.columns) {
      const base = prismaType(c, model)
      const type = base
        ? `${base}${c.isArray ? '[]' : ''}${c.nullable ? '?' : ''}`
        : `Unsupported(${JSON.stringify(c.dataType)})${c.nullable ? '?' : ''}`
      const attrs = []
      if (c.isPrimary && t.primaryKey.length === 1) attrs.push('@id')
      if (c.isUnique && !(c.isPrimary && t.primaryKey.length === 1)) attrs.push('@unique')
      if (base) {
        const def = prismaDefault(c, base).trim()
        if (def) attrs.push(def)
        const native = prismaNativeType(c, model).trim()
        if (native) attrs.push(native)
      }
      if (c.field !== c.name) attrs.push(`@map(${JSON.stringify(c.name)})`)
      fields.push([c.field, type, attrs.join(' ')])
    }

    for (const r of rels.filter((r) => r.child.name === t.name)) {
      const named = r.name ? `${JSON.stringify(r.name)}, ` : ''
      const optional = r.column.nullable ? '?' : ''
      fields.push([
        r.childField,
        `${r.parent.model}${optional}`,
        `@relation(${named}fields: [${r.column.field}], references: [${fieldNameFor(r.parent, r.column.references.column)}])`,
      ])
    }
    for (const r of rels.filter((r) => r.parent.name === t.name)) {
      const named = r.name ? `(${JSON.stringify(r.name)})` : ''
      fields.push([r.parentField, r.one ? `${r.child.model}?` : `${r.child.model}[]`, named ? `@relation${named}` : ''])
    }

    // Column-aligned like a hand-written schema: the types and attributes line
    // up, which is most of what makes a Prisma file skimmable.
    const nameW = Math.max(...fields.map((f) => f[0].length), 1)
    const typeW = Math.max(...fields.map((f) => f[1].length), 1)
    for (const [name, type, attrs] of fields) {
      lines.push(`  ${name.padEnd(nameW)} ${attrs ? type.padEnd(typeW) : type}${attrs ? ` ${attrs}` : ''}`.trimEnd())
    }

    const extras = []
    if (t.primaryKey.length > 1) {
      extras.push(`  @@id([${t.primaryKey.map((c) => fieldNameFor(t, c)).join(', ')}])`)
    }
    for (const idx of t.indexes) {
      const cols = idx.columns.map((c) => fieldNameFor(t, c)).filter(Boolean)
      if (cols.length !== idx.columns.length) continue // expression index - no field to point at
      if (idx.unique && cols.length === 1) continue    // already `@unique` on the field
      extras.push(`  @@${idx.unique ? 'unique' : 'index'}([${cols.join(', ')}])`)
    }
    if (t.model !== t.name) extras.push(`  @@map(${JSON.stringify(t.name)})`)
    if (extras.length) {
      lines.push('')
      lines.push(...extras)
    }
    lines.push('}')
  }

  return `${lines.join('\n')}\n`
}

/** The code-side name of a database column. */
function fieldNameFor(table, columnName) {
  return table.columns.find((c) => c.name === columnName)?.field ?? toCamel(columnName)
}

// ── Drizzle ───────────────────────────────────────────────────────────────────

const DRIZZLE_CORE = { postgres: 'pg-core', mysql: 'mysql-core', sqlite: 'sqlite-core' }
const DRIZZLE_TABLE_FN = { postgres: 'pgTable', mysql: 'mysqlTable', sqlite: 'sqliteTable' }

/**
 * One Drizzle column expression, e.g. `varchar('email', { length: 255 })`.
 * Returns the helper it used so the import list stays exact.
 * @param {OrmColumn} c @param {OrmSchemaModel} model
 * @returns {{ expr: string, helpers: string[] }}
 */
function drizzleColumn(c, model) {
  const name = quote(c.name)
  const helpers = []
  const use = (fn, args = '') => {
    helpers.push(fn)
    return `${fn}(${name}${args})`
  }
  const t = c.baseType

  // The enum constant is declared in this same file, so it needs no import.
  if (c.enumName && model.family === 'postgres') {
    return { expr: `${enumConstName(c.enumName)}(${name})`, helpers: [] }
  }

  if (model.family === 'sqlite') {
    // SQLite stores everything in four classes, so the mode flag is what makes
    // the column read back as the type the app expects.
    if (/bool/.test(t)) return { expr: use('integer', `, { mode: 'boolean' }`), helpers }
    if (/int/.test(t)) return { expr: use('integer'), helpers }
    if (/real|floa|doub|numeric|decimal/.test(t)) return { expr: use('real'), helpers }
    if (/blob/.test(t)) return { expr: use('blob'), helpers }
    if (/date|time/.test(t)) return { expr: use('integer', `, { mode: 'timestamp' }`), helpers }
    return { expr: use('text'), helpers }
  }

  if (model.family === 'mysql') {
    const map = {
      tinyint: c.args[0] === 1 ? 'boolean' : 'tinyint', smallint: 'smallint', mediumint: 'mediumint',
      int: 'int', integer: 'int', bigint: 'bigint', bool: 'boolean', boolean: 'boolean',
      float: 'float', double: 'double', real: 'double',
      date: 'date', datetime: 'datetime', timestamp: 'timestamp', time: 'time', year: 'year',
      text: 'text', tinytext: 'tinytext', mediumtext: 'mediumtext', longtext: 'longtext', json: 'json',
      blob: 'blob', binary: 'binary', varbinary: 'varbinary',
    }
    if (t === 'varchar' || t === 'char') return { expr: use(t, `, { length: ${c.args[0] ?? 255} }`), helpers }
    if (t === 'decimal' || t === 'numeric') {
      const p = c.args.length === 2 ? `, { precision: ${c.args[0]}, scale: ${c.args[1]} }` : ''
      return { expr: use('decimal', p), helpers }
    }
    if (t === 'bigint') return { expr: use('bigint', `, { mode: 'number' }`), helpers }
    if (map[t]) return { expr: use(map[t]), helpers }
    return { expr: use('text'), helpers }
  }

  // PostgreSQL
  if (c.autoIncrement) {
    const fn = t === 'int8' || t === 'bigint' ? 'bigserial' : t === 'int2' ? 'smallserial' : 'serial'
    const args = fn === 'bigserial' ? `, { mode: 'number' }` : ''
    return { expr: use(fn, args), helpers }
  }
  const pg = {
    int2: 'smallint', smallint: 'smallint', int4: 'integer', int: 'integer', integer: 'integer',
    bool: 'boolean', boolean: 'boolean', text: 'text', uuid: 'uuid', date: 'date',
    json: 'json', jsonb: 'jsonb', float4: 'real', real: 'real',
    float8: 'doublePrecision', 'double precision': 'doublePrecision',
    inet: 'inet', cidr: 'cidr', macaddr: 'macaddr', interval: 'interval', time: 'time', money: 'text',
  }
  // No built-in bytea: keep the column honest instead of silently typing it text.
  if (t === 'bytea') return { expr: `text(${name}) /* bytea - declare a customType */`, helpers: ['text'] }

  let expr
  if (t === 'varchar' || t === 'character varying') expr = use('varchar', c.args[0] ? `, { length: ${c.args[0]} }` : '')
  else if (t === 'bpchar' || t === 'char') expr = use('char', c.args[0] ? `, { length: ${c.args[0]} }` : '')
  else if (t === 'numeric' || t === 'decimal') {
    expr = use('numeric', c.args.length === 2 ? `, { precision: ${c.args[0]}, scale: ${c.args[1]} }` : '')
  } else if (t === 'int8' || t === 'bigint') expr = use('bigint', `, { mode: 'number' }`)
  else if (t === 'timestamptz' || t === 'timestamp with time zone') expr = use('timestamp', `, { withTimezone: true }`)
  else if (t === 'timestamp' || t === 'timestamp without time zone') expr = use('timestamp')
  else if (pg[t]) expr = use(pg[t])
  else expr = use('text')

  return { expr, helpers }
}

/** @param {OrmColumn} c */
function drizzleDefault(c, model) {
  const raw = c.defaultValue
  if (raw == null || raw === '' || c.autoIncrement) return { chain: '', helpers: [] }
  const v = String(raw).trim()
  // A column read back in boolean mode needs a boolean default, not the 0/1 the
  // database stores.
  if (isBooleanColumn(c, model) && /^'?[01]'?$|^(true|false)$/i.test(v)) {
    return { chain: `.default(${/^'?1'?$|^true$/i.test(v)})`, helpers: [] }
  }
  if (/^(now\(\)|current_timestamp)/i.test(v)) return { chain: '.defaultNow()', helpers: [] }
  if (/^(gen_random_uuid|uuid_generate_v4)\(\)/i.test(v)) {
    return model.family === 'postgres'
      ? { chain: '.defaultRandom()', helpers: [] }
      : { chain: '', helpers: [] }
  }
  if (/^(true|false)$/i.test(v)) return { chain: `.default(${v.toLowerCase()})`, helpers: [] }
  if (/^-?\d+(\.\d+)?$/.test(v)) return { chain: `.default(${v})`, helpers: [] }
  const lit = v.match(/^'((?:[^']|'')*)'(::.+)?$/)
  if (lit) return { chain: `.default(${quote(lit[1].replace(/''/g, "'"))})`, helpers: [] }
  return { chain: `.default(sql\`${v.replace(/`/g, '\\`')}\`)`, helpers: ['sql'] }
}

/** @param {OrmColumn} c */
function isBooleanColumn(c, model) {
  if (/^bool/.test(c.baseType)) return true
  return model.family === 'mysql' && c.baseType === 'tinyint' && c.args[0] === 1
}

/**
 * Render the model as a Drizzle schema module.
 * @param {OrmSchemaModel} model
 */
export function renderDrizzleSchema(model) {
  const core = DRIZZLE_CORE[model.family]
  const tableFn = DRIZZLE_TABLE_FN[model.family]
  const rels = relationsOf(model)
  const helpers = new Set([tableFn])
  const ormImports = new Set()
  const body = []

  const exportName = new Map()
  const usedExports = new Set()
  for (const t of model.tables) {
    let name = toCamel(t.name)
    while (usedExports.has(name)) name = `${name}Table`
    usedExports.add(name)
    exportName.set(t.name, name)
  }

  if (model.family === 'postgres' && model.enums.length) {
    helpers.add('pgEnum')
    for (const e of model.enums) {
      body.push(`export const ${enumConstName(e.name)} = pgEnum(${quote(e.name)}, [${e.values.map(quote).join(', ')}])`)
    }
    body.push('')
  }

  for (const t of model.tables) {
    const lines = [`export const ${exportName.get(t.name)} = ${tableFn}(${quote(t.name)}, {`]
    for (const c of t.columns) {
      const { expr, helpers: used } = drizzleColumn(c, model)
      used.forEach((h) => helpers.add(h))
      let chain = expr
      if (c.isPrimary && t.primaryKey.length === 1) chain += '.primaryKey()'
      if (!c.nullable && !(c.isPrimary && t.primaryKey.length === 1)) chain += '.notNull()'
      if (c.isUnique && !c.isPrimary) chain += '.unique()'
      const def = drizzleDefault(c, model)
      def.helpers.forEach((h) => ormImports.add(h))
      chain += def.chain
      if (c.isArray) chain += '.array()'
      if (c.references) {
        const parentExport = exportName.get(c.references.table)
        const parentTable = model.tables.find((x) => x.name === c.references.table)
        chain += `.references(() => ${parentExport}.${fieldNameFor(parentTable, c.references.column)})`
      }
      lines.push(`  ${propKey(c.field)}: ${chain},`)
    }

    // Table extras take the array form the current drizzle-kit emits; the object
    // form is deprecated.
    const extras = []
    if (t.primaryKey.length > 1) {
      helpers.add('primaryKey')
      extras.push(`  primaryKey({ columns: [${t.primaryKey.map((c) => `table.${fieldNameFor(t, c)}`).join(', ')}] }),`)
    }
    for (const idx of t.indexes) {
      const cols = idx.columns.map((c) => t.columns.find((x) => x.name === c)?.field).filter(Boolean)
      if (cols.length !== idx.columns.length) continue
      if (idx.unique && cols.length === 1) continue
      const fn = idx.unique ? 'uniqueIndex' : 'index'
      helpers.add(fn)
      extras.push(`  ${fn}(${quote(idx.name)}).on(${cols.map((c) => `table.${c}`).join(', ')}),`)
    }
    lines.push(extras.length ? `}, (table) => [` : '})')
    if (extras.length) {
      lines.push(...extras)
      lines.push('])')
    }
    body.push(lines.join('\n'))
    body.push('')
  }

  // Relations are the reason to read this file at all, so they get their own
  // block per table rather than being implied by `.references()`.
  const withRels = model.tables.filter((t) => rels.some((r) => r.child.name === t.name || r.parent.name === t.name))
  if (withRels.length) ormImports.add('relations')
  for (const t of withRels) {
    const mine = rels.filter((r) => r.child.name === t.name)
    const theirs = rels.filter((r) => r.parent.name === t.name)
    // A one-to-one back-reference is also an `one()`, so the destructure has to
    // follow what the body actually calls.
    const needsOne = mine.length > 0 || theirs.some((r) => r.one)
    const args = [needsOne ? 'one' : '', theirs.some((r) => !r.one) ? 'many' : ''].filter(Boolean).join(', ')
    const lines = [`export const ${exportName.get(t.name)}Relations = relations(${exportName.get(t.name)}, ({ ${args} }) => ({`]
    for (const r of mine) {
      lines.push(
        `  ${propKey(r.childField)}: one(${exportName.get(r.parent.name)}, {` +
          ` fields: [${exportName.get(t.name)}.${r.column.field}],` +
          ` references: [${exportName.get(r.parent.name)}.${fieldNameFor(r.parent, r.column.references.column)}]` +
          `${r.name ? `, relationName: ${quote(r.name)}` : ''} }),`,
      )
    }
    for (const r of theirs) {
      const opts = r.name ? `, { relationName: ${quote(r.name)} }` : ''
      const fn = r.one ? 'one' : 'many'
      lines.push(`  ${propKey(r.parentField)}: ${fn}(${exportName.get(r.child.name)}${opts}),`)
    }
    lines.push('}))')
    body.push(lines.join('\n'))
    body.push('')
  }

  const header = [
    `// Drizzle schema generated by Stroke from the live database.`,
    `// Schema: ${model.schema}`,
  ]
  if (model.skipped.length) {
    header.push(`// Views not shown (${model.skipped.length}): ${model.skipped.slice(0, 8).join(', ')}${model.skipped.length > 8 ? ', …' : ''}`)
  }
  header.push('')
  header.push(`import { ${[...helpers].sort().join(', ')} } from 'drizzle-orm/${core}'`)
  if (ormImports.size) header.push(`import { ${[...ormImports].sort().join(', ')} } from 'drizzle-orm'`)
  return `${header.join('\n')}\n\n${body.join('\n').trimEnd()}\n`
}

// ── SQL (DDL) ────────────────────────────────────────────────────────────────
//
// The schema as the statements that would rebuild it: CREATE TABLE, the
// indexes, and the foreign keys. Unlike the ORM targets this one describes
// every engine, because it is the engine's own language.
//
// Foreign keys are emitted as ALTER TABLE after every table exists, rather than
// inline REFERENCES. Inline constraints only work if the parent table is
// created first, and a schema with a cycle (or simply alphabetical ordering
// that puts the child first) cannot satisfy that at all.

/** Every engine can be described as DDL - it is their own language. */
export const SQL_ENGINES = [
  'postgres', 'cockroachdb', 'mysql', 'mariadb',
  'sqlite', 'libsql', 'd1', 'mssql', 'duckdb', 'clickhouse',
]

/** Identifier quoting per family. @param {string} name @param {string} family */
function sqlIdent(name, family) {
  if (family === 'mysql') return `\`${name.replace(/`/g, '``')}\``
  if (family === 'mssql') return `[${name.replace(/]/g, ']]')}]`
  return `"${name.replace(/"/g, '""')}"`
}

/** A literal safe to paste into DDL. @param {string} text */
function sqlText(text) {
  return `'${String(text).replace(/'/g, "''")}'`
}

/**
 * The live schema as the DDL that would recreate it.
 * @param {OrmSchemaModel} model
 * @param {{ qualify?: boolean }} [opts] qualify - prefix names with the schema,
 *   which a whole-database dump needs and a single-schema one does not.
 * @returns {string}
 */
export function renderSqlSchema(model, opts = {}) {
  const fam = model.family
  const id = (/** @type {string} */ n) => sqlIdent(n, fam)
  // Postgres and MSSQL namespace tables; MySQL's "schema" IS the database and
  // SQLite has none, so qualifying there would name something that isn't real.
  const canQualify = fam === 'postgres' || fam === 'mssql'
  const qualify = !!opts.qualify && canQualify
  const rel = (/** @type {string} */ t) => (qualify ? `${id(model.schema)}.${id(t)}` : id(t))

  const out = []
  out.push(`-- SQL schema generated by Stroke from the live database.`)
  out.push(`-- Schema: ${model.schema}`)
  if (model.skipped.length) {
    out.push(`-- Views not shown (${model.skipped.length}): ${model.skipped.slice(0, 8).join(', ')}${model.skipped.length > 8 ? ', …' : ''}`)
  }
  out.push('')

  if (qualify) {
    out.push(`CREATE SCHEMA IF NOT EXISTS ${id(model.schema)};`)
    out.push('')
  }

  // Enum types have to exist before any column can be declared with one.
  if (fam === 'postgres' && model.enums.length) {
    for (const e of model.enums) {
      const name = qualify ? `${id(model.schema)}.${id(e.name)}` : id(e.name)
      out.push(`CREATE TYPE ${name} AS ENUM (${e.values.map(sqlText).join(', ')});`)
    }
    out.push('')
  }

  for (const t of model.tables) {
    const lines = [`CREATE TABLE ${rel(t.name)} (`]
    const parts = []
    for (const c of t.columns) {
      let def = `  ${id(c.name)} ${c.dataType || c.baseType || 'text'}`
      if (c.isArray && fam === 'postgres' && !/\[\]$/.test(def)) def += '[]'
      if (!c.nullable) def += ' NOT NULL'
      // The introspected default is already engine-native (`now()`,
      // `nextval(...)`), so it is emitted verbatim rather than re-quoted.
      if (c.defaultValue != null && c.defaultValue !== '') def += ` DEFAULT ${c.defaultValue}`
      parts.push(def)
    }
    if (t.primaryKey.length) {
      parts.push(`  PRIMARY KEY (${t.primaryKey.map(id).join(', ')})`)
    }
    lines.push(parts.join(',\n'))
    lines.push(');')
    out.push(lines.join('\n'))

    // Single-column uniques are already on the column; anything else is an index.
    for (const idx of t.indexes) {
      if (idx.columns.some((c) => !t.columns.find((x) => x.name === c))) continue
      if (t.primaryKey.length && idx.columns.join(',') === t.primaryKey.join(',')) continue
      const kind = idx.unique ? 'CREATE UNIQUE INDEX' : 'CREATE INDEX'
      out.push(`${kind} ${id(idx.name)} ON ${rel(t.name)} (${idx.columns.map(id).join(', ')});`)
    }
    out.push('')
  }

  const fks = []
  for (const t of model.tables) {
    for (const c of t.columns) {
      if (!c.references) continue
      fks.push(
        `ALTER TABLE ${rel(t.name)}\n` +
        `  ADD CONSTRAINT ${id(`${t.name}_${c.name}_fkey`)}\n` +
        `  FOREIGN KEY (${id(c.name)}) REFERENCES ${rel(c.references.table)} (${id(c.references.column)});`,
      )
    }
  }
  if (fks.length) {
    out.push('-- Foreign keys, after every table exists.')
    out.push(...fks)
    out.push('')
  }

  return `${out.join('\n').trimEnd()}\n`
}

/**
 * Every schema in the database, as one script.
 *
 * Concatenation is valid here in a way it is not for the ORM targets: DDL
 * names are schema-qualified, so two schemas owning a `users` table produce two
 * distinct statements rather than a collision.
 * @param {OrmSchemaModel[]} models
 */
export function renderSqlDatabase(models) {
  if (!models.length) return '-- No schemas to describe.\n'
  const head = [
    '-- SQL schema generated by Stroke from the live database.',
    `-- Whole database: ${models.length} ${models.length === 1 ? 'schema' : 'schemas'} (${models.map((m) => m.schema).join(', ')})`,
    '',
  ]
  const parts = models.map((m) => {
    const banner = `-- ${'─'.repeat(70)}\n-- ${m.schema}\n-- ${'─'.repeat(70)}`
    // Drop the per-schema preamble: the header above already said all of it.
    const body = renderSqlSchema(m, { qualify: true })
      .split('\n')
      .filter((l) => !l.startsWith('-- SQL schema generated') && !l.startsWith('-- Schema: '))
      .join('\n')
      .trim()
    return `${banner}\n\n${body}`
  })
  return `${head.join('\n')}\n${parts.join('\n\n')}\n`
}
