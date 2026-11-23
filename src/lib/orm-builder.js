/**
 * ORM query builder utilities - Drizzle and Prisma query simulation + SQL conversion.
 */

// ── Helpers ──────────────────────────────────────────────────────────────────

function quoteIdent(name) {
  return `"${String(name).replace(/"/g, '""')}"`
}

/** @param {unknown} v */
function literalSql(v) {
  if (v === null || v === undefined) return 'NULL'
  if (typeof v === 'boolean') return v ? 'true' : 'false'
  if (typeof v === 'number') return String(v)
  if (v && typeof v === 'object') {
    // SQL expression (sql`...`, count(), eq(), …) - embed as-is
    if ('__sql' in /** @type {any} */ (v)) return /** @type {any} */ (v).__sql
    // Column reference - use qualified name for column-to-column comparisons
    if ('col' in /** @type {any} */ (v)) {
      const r = /** @type {any} */ (v)
      return r.table ? `${quoteIdent(r.table)}.${quoteIdent(r.col)}` : quoteIdent(r.col)
    }
  }
  return `'${String(v).replace(/'/g, "''")}'`
}

/**
 * @param {Record<string, unknown>} where
 * @returns {string}
 */
function whereClauseFromObject(where) {
  const parts = []
  /** A LIKE pattern's text as a string literal: the value's own quotes doubled.
   *  It was spliced in raw, so contains: "O'Neil" ended the string early. */
  const likeLit = (/** @type {string} */ pattern) => `'${pattern.replace(/'/g, "''")}'`
  for (const [key, v] of Object.entries(where)) {
    // Quoted like the table is: a camelCase field (createdAt) folded to
    // createdat on Postgres and the column was not found.
    const k = key === 'AND' || key === 'OR' || key === 'NOT' ? key : quoteIdent(key)
    if (key === 'AND' && Array.isArray(v)) {
      const inner = v.map(w => `(${whereClauseFromObject(w)})`).join(' AND ')
      if (inner) parts.push(inner)
    } else if (key === 'OR' && Array.isArray(v)) {
      const inner = v.map(w => `(${whereClauseFromObject(w)})`).join(' OR ')
      if (inner) parts.push(`(${inner})`)
    } else if (key === 'NOT' && v && typeof v === 'object') {
      parts.push(`NOT (${whereClauseFromObject(/** @type {any} */ (v))})`)
    } else if (v !== null && typeof v === 'object' && !Array.isArray(v)) {
      const ops = /** @type {Record<string, unknown>} */ (v)
      if ('equals' in ops)      parts.push(`${k} = ${literalSql(ops.equals)}`)
      else if ('not' in ops)    parts.push(`${k} != ${literalSql(ops.not)}`)
      else if ('gt' in ops)     parts.push(`${k} > ${literalSql(ops.gt)}`)
      else if ('gte' in ops)    parts.push(`${k} >= ${literalSql(ops.gte)}`)
      else if ('lt' in ops)     parts.push(`${k} < ${literalSql(ops.lt)}`)
      else if ('lte' in ops)    parts.push(`${k} <= ${literalSql(ops.lte)}`)
      else if ('contains' in ops) parts.push(`${k} LIKE ${likeLit(`%${ops.contains}%`)}`)
      else if ('startsWith' in ops) parts.push(`${k} LIKE ${likeLit(`${ops.startsWith}%`)}`)
      else if ('endsWith' in ops) parts.push(`${k} LIKE ${likeLit(`%${ops.endsWith}`)}`)
      else if ('in' in ops && Array.isArray(ops.in)) parts.push(`${k} IN (${ops.in.map(literalSql).join(', ')})`)
      else if ('notIn' in ops && Array.isArray(ops.notIn)) parts.push(`${k} NOT IN (${ops.notIn.map(literalSql).join(', ')})`)
    } else {
      parts.push(`${k} = ${literalSql(v)}`)
    }
  }
  return parts.join(' AND ')
}

// ── Drizzle mock builder ──────────────────────────────────────────────────────

function makeTableProxy(tableName) {
  return new Proxy(
    { __tableName: tableName },
    {
      get(target, prop) {
        if (prop === '__tableName') return tableName
        if (prop === Symbol.toPrimitive || prop === 'toString' || prop === 'valueOf') {
          return () => tableName
        }
        return { __col: true, table: tableName, col: String(prop), toString: () => String(prop) }
      },
    },
  )
}

function makeDrizzleBuilder() {
  /** @type {'select' | 'insert' | 'update' | 'delete' | null} */
  let _op = null
  let _table = null
  /** @type {Record<string, any> | null} */
  let _selectCols = null
  let _insertValues = null
  let _setValues = null
  let _where = null
  /** @type {number | null} */
  let _limit = null
  /** @type {number | null} */
  let _offset = null
  /** @type {{ col: string, dir: 'ASC' | 'DESC' }[]} */
  let _orderBy = []
  /** @type {{ type: string, table: string, on: string }[]} */
  let _joins = []
  /** @type {string[]} */
  let _groupBy = []
  let _having = null
  let _onConflict = null
  let _returning = false

  function toSQL() {
    if (_op === 'select') {
      let cols = '*'
      if (_selectCols) {
        const parts = []
        for (const [alias, ref] of Object.entries(_selectCols)) {
          if (ref && typeof ref === 'object' && '__sql' in ref) {
            parts.push(`${ref.__sql} AS ${alias}`)
          } else if (ref && typeof ref === 'object' && 'col' in ref) {
            parts.push(ref.col === alias ? ref.col : `${ref.col} AS ${alias}`)
          } else {
            parts.push(String(alias))
          }
        }
        if (parts.length) cols = parts.join(', ')
      }
      let sql = `SELECT ${cols} FROM ${quoteIdent(_table)}`
      for (const j of _joins) sql += ` ${j.type} JOIN ${quoteIdent(j.table)} ON ${j.on}`
      if (_where) sql += ` WHERE ${_where}`
      if (_groupBy.length) sql += ` GROUP BY ${_groupBy.join(', ')}`
      if (_having) sql += ` HAVING ${_having}`
      if (_orderBy.length) sql += ` ORDER BY ${_orderBy.map(o => `${o.col} ${o.dir}`).join(', ')}`
      if (_limit !== null) sql += ` LIMIT ${_limit}`
      if (_offset !== null) sql += ` OFFSET ${_offset}`
      return { sql, params: [] }
    }

    if (_op === 'insert') {
      const rows = Array.isArray(_insertValues) ? _insertValues : [_insertValues ?? {}]
      const keys = Object.keys(rows[0] ?? {})
      const cols = keys.join(', ')
      const valSets = rows.map(r => `(${keys.map(k => literalSql(r[k])).join(', ')})`).join(', ')
      let sql = `INSERT INTO ${quoteIdent(_table)} (${cols}) VALUES ${valSets}`
      if (_onConflict) sql += ` ${_onConflict}`
      if (_returning) sql += ` RETURNING *`
      return { sql, params: [] }
    }

    if (_op === 'update') {
      const setCols = Object.entries(_setValues ?? {})
        .map(([k, v]) => `${k} = ${literalSql(v)}`).join(', ')
      let sql = `UPDATE ${quoteIdent(_table)} SET ${setCols}`
      if (_where) sql += ` WHERE ${_where}`
      if (_returning) sql += ` RETURNING *`
      return { sql, params: [] }
    }

    if (_op === 'delete') {
      let sql = `DELETE FROM ${quoteIdent(_table)}`
      if (_where) sql += ` WHERE ${_where}`
      if (_returning) sql += ` RETURNING *`
      return { sql, params: [] }
    }

    throw new Error('Incomplete query: call db.select(), db.insert(), db.update(), or db.delete()')
  }

  /** @param {any} col */
  function colStr(col) {
    if (col && typeof col === 'object' && 'col' in col) return col.col
    if (col && typeof col === 'object' && '__sql' in col) return col.__sql
    return String(col)
  }

  /** @param {any} table */
  function tableName(table) {
    return table?.__tableName ?? String(table)
  }

  /** @param {any} on */
  function onStr(on) {
    return on?.__sql ?? String(on)
  }

  const chain = {
    toSQL,
    from(tableProxy) {
      _table = tableProxy?.__tableName ?? String(tableProxy)
      return chain
    },
    where(condition) {
      if (typeof condition === 'string') _where = condition
      else if (condition && typeof condition === 'object' && '__sql' in condition) _where = condition.__sql
      return chain
    },
    limit(n) { _limit = Number(n); return chain },
    offset(n) { _offset = Number(n); return chain },
    orderBy(...args) {
      for (const arg of args) {
        if (arg && typeof arg === 'object' && '__orderBy' in arg) {
          _orderBy.push({ col: arg.__orderBy.col, dir: arg.__orderBy.dir })
        }
      }
      return chain
    },
    groupBy(...cols) {
      for (const col of cols) _groupBy.push(colStr(col))
      return chain
    },
    having(condition) {
      if (typeof condition === 'string') _having = condition
      else if (condition && '__sql' in condition) _having = condition.__sql
      return chain
    },
    set(values) { _setValues = values; return chain },
    values(vals) { _insertValues = vals; return chain },
    returning() { _returning = true; return chain },
    leftJoin(table, on) { _joins.push({ type: 'LEFT', table: tableName(table), on: onStr(on) }); return chain },
    innerJoin(table, on) { _joins.push({ type: 'INNER', table: tableName(table), on: onStr(on) }); return chain },
    rightJoin(table, on) { _joins.push({ type: 'RIGHT', table: tableName(table), on: onStr(on) }); return chain },
    fullJoin(table, on) { _joins.push({ type: 'FULL', table: tableName(table), on: onStr(on) }); return chain },
    onConflictDoNothing(opts = {}) {
      if (opts?.target) {
        const cols = Array.isArray(opts.target)
          ? opts.target.map(c => c?.col ?? String(c)).join(', ')
          : opts.target?.col ?? String(opts.target)
        _onConflict = `ON CONFLICT (${cols}) DO NOTHING`
      } else {
        _onConflict = `ON CONFLICT DO NOTHING`
      }
      return chain
    },
    onConflictDoUpdate(opts = {}) {
      const setCols = Object.entries(opts.set ?? {}).map(([k, v]) => `${k} = ${literalSql(v)}`).join(', ')
      const target = opts.target
      if (target) {
        const cols = Array.isArray(target)
          ? target.map(c => c?.col ?? String(c)).join(', ')
          : target?.col ?? String(target)
        _onConflict = `ON CONFLICT (${cols}) DO UPDATE SET ${setCols}`
      } else {
        _onConflict = `ON CONFLICT DO UPDATE SET ${setCols}`
      }
      return chain
    },
  }

  return { chain, setOp(op) { _op = op }, setTable(t) { _table = t }, setSelectCols(c) { _selectCols = c } }
}

/**
 * @param {string[]} tableNames
 */
function buildDrizzleContext(tableNames) {
  const tableProxies = Object.fromEntries(tableNames.map(n => [n, makeTableProxy(n)]))

  /** @param {any} col */
  function colExpr(col) {
    if (col && typeof col === 'object' && '__sql' in col) return col.__sql
    // Quoted, as the Prisma side is: users.createdAt folded to createdat.
    if (col && typeof col === 'object' && 'col' in col) return col.table ? `${quoteIdent(col.table)}.${quoteIdent(col.col)}` : quoteIdent(col.col)
    return col ? String(col) : '*'
  }

  const helpers = {
    eq:         (col, val) => ({ __sql: `${colExpr(col)} = ${literalSql(val)}` }),
    ne:         (col, val) => ({ __sql: `${colExpr(col)} != ${literalSql(val)}` }),
    gt:         (col, val) => ({ __sql: `${colExpr(col)} > ${literalSql(val)}` }),
    gte:        (col, val) => ({ __sql: `${colExpr(col)} >= ${literalSql(val)}` }),
    lt:         (col, val) => ({ __sql: `${colExpr(col)} < ${literalSql(val)}` }),
    lte:        (col, val) => ({ __sql: `${colExpr(col)} <= ${literalSql(val)}` }),
    like:       (col, pattern) => ({ __sql: `${colExpr(col)} LIKE ${literalSql(pattern)}` }),
    ilike:      (col, pattern) => ({ __sql: `${colExpr(col)} ILIKE ${literalSql(pattern)}` }),
    notIlike:   (col, pattern) => ({ __sql: `${colExpr(col)} NOT ILIKE ${literalSql(pattern)}` }),
    isNull:     (col) => ({ __sql: `${colExpr(col)} IS NULL` }),
    isNotNull:  (col) => ({ __sql: `${colExpr(col)} IS NOT NULL` }),
    inArray:    (col, vals) => ({ __sql: `${colExpr(col)} IN (${(vals ?? []).map(literalSql).join(', ')})` }),
    notInArray: (col, vals) => ({ __sql: `${colExpr(col)} NOT IN (${(vals ?? []).map(literalSql).join(', ')})` }),
    between:    (col, lo, hi) => ({ __sql: `${colExpr(col)} BETWEEN ${literalSql(lo)} AND ${literalSql(hi)}` }),
    notBetween: (col, lo, hi) => ({ __sql: `${colExpr(col)} NOT BETWEEN ${literalSql(lo)} AND ${literalSql(hi)}` }),
    and:        (...conds) => ({ __sql: conds.filter(Boolean).map(c => (c?.__sql ? `(${c.__sql})` : String(c))).join(' AND ') }),
    or:         (...conds) => ({ __sql: conds.filter(Boolean).map(c => (c?.__sql ? `(${c.__sql})` : String(c))).join(' OR ') }),
    not:        (cond) => ({ __sql: `NOT (${cond?.__sql ?? cond})` }),
    asc:        (col) => ({ __orderBy: { col: colExpr(col), dir: 'ASC' } }),
    desc:       (col) => ({ __orderBy: { col: colExpr(col), dir: 'DESC' } }),
    // Aggregate functions - usable in db.select({ alias: count() })
    count:      (col) => ({ __sql: col ? `COUNT(${colExpr(col)})` : 'COUNT(*)' }),
    sum:        (col) => ({ __sql: `SUM(${colExpr(col)})` }),
    avg:        (col) => ({ __sql: `AVG(${colExpr(col)})` }),
    max:        (col) => ({ __sql: `MAX(${colExpr(col)})` }),
    min:        (col) => ({ __sql: `MIN(${colExpr(col)})` }),
    // SQL template tag
    sql:        (strings, ...vals) => {
      if (Array.isArray(strings)) {
        let raw = ''
        strings.forEach((s, i) => { raw += s; if (i < vals.length) raw += literalSql(vals[i]) })
        return { __sql: raw }
      }
      return { __sql: String(strings) }
    },
  }

  const db = {
    select(cols) {
      const { chain, setOp, setSelectCols } = makeDrizzleBuilder()
      setOp('select')
      if (cols && typeof cols === 'object' && !Array.isArray(cols)) {
        const mapped = {}
        for (const [alias, ref] of Object.entries(cols)) {
          if (ref && typeof ref === 'object' && '__sql' in ref) {
            mapped[alias] = { __sql: ref.__sql }
          } else if (ref && typeof ref === 'object' && 'col' in ref) {
            mapped[alias] = ref
          } else {
            mapped[alias] = { col: alias, table: '' }
          }
        }
        setSelectCols(mapped)
      }
      return chain
    },
    insert(tableProxy) {
      const { chain, setOp, setTable } = makeDrizzleBuilder()
      setOp('insert')
      setTable(tableProxy?.__tableName ?? String(tableProxy))
      return chain
    },
    update(tableProxy) {
      const { chain, setOp, setTable } = makeDrizzleBuilder()
      setOp('update')
      setTable(tableProxy?.__tableName ?? String(tableProxy))
      return chain
    },
    delete(tableProxy) {
      const { chain, setOp, setTable } = makeDrizzleBuilder()
      setOp('delete')
      setTable(tableProxy?.__tableName ?? String(tableProxy))
      return chain
    },
  }

  return { db, tableProxies, helpers }
}

// ── Prisma mock builder ───────────────────────────────────────────────────────

/** @param {string[]} tableNames */
function buildPrismaContext(tableNames) {
  function makeModelProxy(modelName) {
    const qt = quoteIdent(modelName)

    function selectClause(opts = {}) {
      if (opts.select && typeof opts.select === 'object') {
        const cols = Object.entries(opts.select)
          .filter(([, v]) => v === true)
          .map(([k]) => quoteIdent(k))
        if (cols.length) return cols.join(', ')
      }
      return '*'
    }

    function applyWhere(sql, where) {
      if (where && typeof where === 'object' && Object.keys(where).length) {
        sql += ` WHERE ${whereClauseFromObject(where)}`
      }
      return sql
    }

    function applyOrderBy(sql, orderBy) {
      if (orderBy) {
        const ob = Array.isArray(orderBy) ? orderBy : [orderBy]
        const parts = ob.flatMap(o => Object.entries(o).map(([col, dir]) => `${quoteIdent(col)} ${String(dir).toUpperCase() === 'DESC' ? 'DESC' : 'ASC'}`))
        if (parts.length) sql += ` ORDER BY ${parts.join(', ')}`
      }
      return sql
    }

    return {
      findMany(opts = {}) {
        let sql = `SELECT ${selectClause(opts)} FROM ${qt}`
        sql = applyWhere(sql, opts.where)
        sql = applyOrderBy(sql, opts.orderBy)
        if (opts.take != null) sql += ` LIMIT ${Number(opts.take)}`
        if (opts.skip != null) sql += ` OFFSET ${Number(opts.skip)}`
        if (opts.cursor) sql += ` /* cursor pagination */`
        return { sql, params: [] }
      },
      findFirst(opts = {}) {
        let sql = `SELECT ${selectClause(opts)} FROM ${qt}`
        sql = applyWhere(sql, opts.where)
        sql = applyOrderBy(sql, opts.orderBy)
        sql += ` LIMIT 1`
        return { sql, params: [] }
      },
      findFirstOrThrow(opts = {}) {
        return this.findFirst(opts)
      },
      findUnique(opts = {}) {
        let sql = `SELECT ${selectClause(opts)} FROM ${qt}`
        sql = applyWhere(sql, opts.where ?? {})
        sql += ` LIMIT 1`
        return { sql, params: [] }
      },
      findUniqueOrThrow(opts = {}) {
        return this.findUnique(opts)
      },
      create(opts = {}) {
        const data = opts.data ?? {}
        const keys = Object.keys(data)
        const cols = keys.map(quoteIdent).join(', ')
        const vals = keys.map(k => literalSql(data[k])).join(', ')
        let sql = `INSERT INTO ${qt} (${cols}) VALUES (${vals})`
        if (opts.select) sql += ` RETURNING ${selectClause(opts)}`
        return { sql, params: [] }
      },
      createMany(opts = {}) {
        const data = Array.isArray(opts.data) ? opts.data : [opts.data ?? {}]
        const keys = Object.keys(data[0] ?? {})
        const cols = keys.map(quoteIdent).join(', ')
        const valSets = data.map(r => `(${keys.map(k => literalSql(r[k])).join(', ')})`).join(', ')
        const sql = `INSERT INTO ${qt} (${cols}) VALUES ${valSets}${opts.skipDuplicates ? ' ON CONFLICT DO NOTHING' : ''}`
        return { sql, params: [] }
      },
      update(opts = {}) {
        const data = opts.data ?? {}
        const setCols = Object.entries(data).map(([k, v]) => `${quoteIdent(k)} = ${literalSql(v)}`).join(', ')
        let sql = `UPDATE ${qt} SET ${setCols}`
        sql = applyWhere(sql, opts.where ?? {})
        if (opts.select) sql += ` RETURNING ${selectClause(opts)}`
        return { sql, params: [] }
      },
      updateMany(opts = {}) {
        const data = opts.data ?? {}
        const setCols = Object.entries(data).map(([k, v]) => `${quoteIdent(k)} = ${literalSql(v)}`).join(', ')
        let sql = `UPDATE ${qt} SET ${setCols}`
        sql = applyWhere(sql, opts.where ?? {})
        return { sql, params: [] }
      },
      delete(opts = {}) {
        let sql = `DELETE FROM ${qt}`
        sql = applyWhere(sql, opts.where ?? {})
        if (opts.select) sql += ` RETURNING ${selectClause(opts)}`
        return { sql, params: [] }
      },
      deleteMany(opts = {}) {
        let sql = `DELETE FROM ${qt}`
        sql = applyWhere(sql, opts?.where ?? {})
        return { sql, params: [] }
      },
      count(opts = {}) {
        let sql = `SELECT COUNT(*) FROM ${qt}`
        sql = applyWhere(sql, opts?.where ?? {})
        return { sql, params: [] }
      },
      aggregate(opts = {}) {
        const parts = []
        if (opts._count) {
          if (opts._count === true) parts.push(`COUNT(*) AS _count`)
          else Object.keys(opts._count).forEach(k => parts.push(`COUNT(${k}) AS _count_${k}`))
        }
        if (opts._sum) Object.keys(opts._sum).forEach(k => parts.push(`SUM(${k}) AS _sum_${k}`))
        if (opts._avg) Object.keys(opts._avg).forEach(k => parts.push(`AVG(${k}) AS _avg_${k}`))
        if (opts._min) Object.keys(opts._min).forEach(k => parts.push(`MIN(${k}) AS _min_${k}`))
        if (opts._max) Object.keys(opts._max).forEach(k => parts.push(`MAX(${k}) AS _max_${k}`))
        if (!parts.length) parts.push('COUNT(*) AS _count')
        let sql = `SELECT ${parts.join(', ')} FROM ${qt}`
        sql = applyWhere(sql, opts.where ?? {})
        return { sql, params: [] }
      },
      groupBy(opts = {}) {
        const by = Array.isArray(opts.by) ? opts.by : (opts.by ? [opts.by] : [])
        const selParts = [...by]
        if (opts._count) {
          if (opts._count === true) selParts.push(`COUNT(*) AS _count`)
          else Object.keys(opts._count).forEach(k => selParts.push(`COUNT(${k}) AS _count_${k}`))
        }
        if (opts._sum) Object.keys(opts._sum).forEach(k => selParts.push(`SUM(${k}) AS _sum_${k}`))
        if (opts._avg) Object.keys(opts._avg).forEach(k => selParts.push(`AVG(${k}) AS _avg_${k}`))
        if (opts._min) Object.keys(opts._min).forEach(k => selParts.push(`MIN(${k}) AS _min_${k}`))
        if (opts._max) Object.keys(opts._max).forEach(k => selParts.push(`MAX(${k}) AS _max_${k}`))
        let sql = `SELECT ${selParts.join(', ')} FROM ${qt}`
        sql = applyWhere(sql, opts.where ?? {})
        if (by.length) sql += ` GROUP BY ${by.join(', ')}`
        if (opts.having) sql += ` HAVING ${whereClauseFromObject(opts.having)}`
        sql = applyOrderBy(sql, opts.orderBy)
        if (opts.take != null) sql += ` LIMIT ${Number(opts.take)}`
        if (opts.skip != null) sql += ` OFFSET ${Number(opts.skip)}`
        return { sql, params: [] }
      },
      upsert(opts = {}) {
        const create = opts.create ?? {}
        const keys = Object.keys(create)
        const cols = keys.join(', ')
        const vals = keys.map(k => literalSql(create[k])).join(', ')
        const update = opts.update ?? {}
        const setCols = Object.entries(update).map(([k, v]) => `${k} = ${literalSql(v)}`).join(', ')
        const whereStr = whereClauseFromObject(opts.where ?? {})
        let sql = `INSERT INTO ${qt} (${cols}) VALUES (${vals})`
        if (setCols) sql += ` ON CONFLICT DO UPDATE SET ${setCols}`
        if (whereStr) sql += ` -- where: ${whereStr}`
        return { sql, params: [] }
      },
    }
  }

  const prisma = new Proxy(
    {},
    { get(_, prop) { return makeModelProxy(String(prop)) } },
  )

  return { prisma }
}

// ── Public API ────────────────────────────────────────────────────────────────

/**
 * @param {string} code
 * @param {string[]} tableNames
 * @returns {{ sql: string, params: unknown[] }}
 */
export function evalDrizzleQuery(code, tableNames) {
  const { db, tableProxies, helpers } = buildDrizzleContext(tableNames)

  const argNames = ['db', ...Object.keys(tableProxies), ...Object.keys(helpers)]
  const argValues = [db, ...Object.values(tableProxies), ...Object.values(helpers)]

  const wrappedCode = `
    "use strict";
    let __result;
    try { __result = (function() { return (${code.trim()}); })(); } catch(e) { throw e; }
    return __result;
  `

  let result
  try {
    // eslint-disable-next-line no-new-func
    const fn = new Function(...argNames, wrappedCode)
    result = fn(...argValues)
  } catch (e) {
    throw new Error(`Drizzle eval error: ${e instanceof Error ? e.message : String(e)}`)
  }

  if (!result) {
    throw new Error('Query did not return a value. Make sure your last expression is the query chain (e.g. db.select().from(users))')
  }

  if (typeof result.toSQL === 'function') return result.toSQL()
  if (result.sql && typeof result.sql === 'string') return { sql: result.sql, params: result.params ?? [] }

  throw new Error('Query result does not have a .toSQL() method or .sql property')
}

/**
 * @param {string} code
 * @param {string[]} tableNames
 * @returns {{ sql: string, params: unknown[] }}
 */
export function evalPrismaQuery(code, tableNames) {
  const { prisma } = buildPrismaContext(tableNames)

  const wrappedCode = `
    "use strict";
    let __result;
    try { __result = (function() { return (${code.trim()}); })(); } catch(e) { throw e; }
    return __result;
  `

  let result
  try {
    // eslint-disable-next-line no-new-func
    const fn = new Function('prisma', wrappedCode)
    result = fn(prisma)
  } catch (e) {
    throw new Error(`Prisma eval error: ${e instanceof Error ? e.message : String(e)}`)
  }

  if (!result) {
    throw new Error('Query did not return a value. Make sure your last expression is the query call (e.g. prisma.user.findMany({}))')
  }

  if (result.sql && typeof result.sql === 'string') return { sql: result.sql, params: result.params ?? [] }

  throw new Error('Query result does not have a .sql property')
}

// ── SQL → Drizzle ─────────────────────────────────────────────────────────────

/** @param {string} sql */
export function sqlToDrizzle(sql) {
  const trimmed = sql.trim().replace(/;$/, '')

  if (!/^SELECT\b/i.test(trimmed)) {
    return `// Cannot auto-convert: use db.insert/update/delete`
  }

  try {
    const upper = trimmed.toUpperCase()

    const fromIdx = upper.search(/\bFROM\b/)
    if (fromIdx < 0) return `// Cannot auto-convert: no FROM clause`

    const selectPart = trimmed.slice(6, fromIdx).trim()
    const afterFrom = trimmed.slice(fromIdx + 4).trim()

    const whereIdx = afterFrom.search(/\bWHERE\b/i)
    const orderIdx = afterFrom.search(/\bORDER\s+BY\b/i)
    const limitIdx = afterFrom.search(/\bLIMIT\b/i)
    const offsetIdx = afterFrom.search(/\bOFFSET\b/i)

    const tableEndIdx = [whereIdx, orderIdx, limitIdx, offsetIdx]
      .filter(i => i >= 0).sort((a, b) => a - b)[0] ?? afterFrom.length
    const tableName = afterFrom.slice(0, tableEndIdx).trim().replace(/^"([^"]+)"$/, '$1')

    let whereStr = ''
    if (whereIdx >= 0) {
      const afterWhere = afterFrom.slice(whereIdx + 5).trim()
      const whereEndIdx = [orderIdx, limitIdx, offsetIdx]
        .map(i => (i >= 0 ? i - (whereIdx + 5) : Infinity))
        .filter(i => i > 0).sort((a, b) => a - b)[0]
      whereStr = whereEndIdx != null && isFinite(whereEndIdx)
        ? afterWhere.slice(0, whereEndIdx).trim()
        : afterWhere
    }

    let orderStr = ''
    if (orderIdx >= 0) {
      const afterOrder = afterFrom.slice(orderIdx).replace(/ORDER\s+BY\s+/i, '').trim()
      const orderEndIdx = [limitIdx, offsetIdx]
        .map(i => (i >= 0 ? i - orderIdx : Infinity))
        .filter(i => i > 0).sort((a, b) => a - b)[0]
      orderStr = orderEndIdx != null && isFinite(orderEndIdx)
        ? afterOrder.slice(0, orderEndIdx).trim()
        : afterOrder
    }

    let limitStr = ''
    if (limitIdx >= 0) {
      const afterLimit = afterFrom.slice(limitIdx + 5).trim()
      const limitEndIdx = offsetIdx >= 0 ? offsetIdx - limitIdx - 5 : undefined
      limitStr = limitEndIdx != null ? afterLimit.slice(0, limitEndIdx).trim() : afterLimit.split(/\s+/)[0]
    }

    let offsetStr = ''
    if (offsetIdx >= 0) {
      offsetStr = afterFrom.slice(offsetIdx + 6).trim().split(/\s+/)[0]
    }

    let selectArg = ''
    if (selectPart !== '*') {
      const cols = selectPart.split(',').map(c => c.trim())
      const colParts = cols.map(c => {
        const asMatch = c.match(/^(.+?)\s+AS\s+(.+)$/i)
        if (asMatch) {
          const raw = asMatch[1].trim().replace(/^"([^"]+)"$/, '$1')
          const alias = asMatch[2].trim().replace(/^"([^"]+)"$/, '$1')
          return `${alias}: ${tableName}.${raw}`
        }
        const colName = c.replace(/^"([^"]+)"$/, '$1')
        return `${colName}: ${tableName}.${colName}`
      })
      selectArg = `{ ${colParts.join(', ')} }`
    }

    let chain = `db.select(${selectArg}).from(${tableName})`

    if (whereStr) chain += `.where(${convertWhereToEq(whereStr, tableName)})`

    if (orderStr) {
      const orderParts = orderStr.split(',').map(part => {
        const segments = part.trim().split(/\s+/)
        const col = segments[0].replace(/^"([^"]+)"$/, '$1')
        const dir = (segments[1] ?? 'ASC').toUpperCase() === 'DESC' ? 'desc' : 'asc'
        return `${dir}(${tableName}.${col})`
      })
      chain += `.orderBy(${orderParts.join(', ')})`
    }

    if (limitStr) chain += `.limit(${limitStr})`
    if (offsetStr) chain += `.offset(${offsetStr})`

    return chain
  } catch {
    return `// Cannot auto-convert: ${sql}`
  }
}

/** @param {string} where @param {string} tableName */
function convertWhereToEq(where, tableName) {
  const eqMatch = where.match(/^(\w+)\s*=\s*(.+)$/)
  if (eqMatch) {
    const col = eqMatch[1]
    const rawVal = eqMatch[2].trim()
    const val = rawVal.match(/^'(.+)'$/) ? `'${rawVal.slice(1, -1)}'` : rawVal
    return `eq(${tableName}.${col}, ${val})`
  }
  return `sql\`${where}\``
}

// ── SQL → Prisma ──────────────────────────────────────────────────────────────

/**
 * @param {string} sql
 * @param {string[]} [tableNames]
 * @returns {string}
 */
export function sqlToPrisma(sql, tableNames) {
  const trimmed = sql.trim().replace(/;$/, '')

  if (!/^SELECT\b/i.test(trimmed)) {
    return `// Cannot auto-convert: use prisma.model.create/update/delete`
  }

  try {
    const upper = trimmed.toUpperCase()
    const fromIdx = upper.search(/\bFROM\b/)
    if (fromIdx < 0) return `// Cannot auto-convert: no FROM clause`

    const afterFrom = trimmed.slice(fromIdx + 4).trim()

    const whereIdx = afterFrom.search(/\bWHERE\b/i)
    const orderIdx = afterFrom.search(/\bORDER\s+BY\b/i)
    const limitIdx = afterFrom.search(/\bLIMIT\b/i)
    const offsetIdx = afterFrom.search(/\bOFFSET\b/i)

    const tableEndIdx = [whereIdx, orderIdx, limitIdx, offsetIdx]
      .filter(i => i >= 0).sort((a, b) => a - b)[0] ?? afterFrom.length
    const tableName = afterFrom.slice(0, tableEndIdx).trim().replace(/^"([^"]+)"$/, '$1')

    const opts = []

    if (whereIdx >= 0) {
      const afterWhere = afterFrom.slice(whereIdx + 5).trim()
      const whereEndIdx = [orderIdx, limitIdx, offsetIdx]
        .map(i => (i >= 0 ? i - (whereIdx + 5) : Infinity))
        .filter(i => i > 0).sort((a, b) => a - b)[0]
      const whereStr = whereEndIdx != null && isFinite(whereEndIdx)
        ? afterWhere.slice(0, whereEndIdx).trim()
        : afterWhere
      const eqMatch = whereStr.match(/^(\w+)\s*=\s*(.+)$/)
      if (eqMatch) {
        const col = eqMatch[1]
        const rawVal = eqMatch[2].trim()
        const val = rawVal.match(/^'(.+)'$/) ? `'${rawVal.slice(1, -1)}'` : rawVal
        opts.push(`where: { ${col}: ${val} }`)
      }
    }

    if (orderIdx >= 0) {
      const afterOrder = afterFrom.slice(orderIdx).replace(/ORDER\s+BY\s+/i, '').trim()
      const orderEndIdx = [limitIdx, offsetIdx]
        .map(i => (i >= 0 ? i - orderIdx : Infinity))
        .filter(i => i > 0).sort((a, b) => a - b)[0]
      const orderStr = orderEndIdx != null && isFinite(orderEndIdx)
        ? afterOrder.slice(0, orderEndIdx).trim()
        : afterOrder
      const parts = orderStr.split(',').map(p => {
        const segs = p.trim().split(/\s+/)
        const col = segs[0].replace(/^"([^"]+)"$/, '$1')
        const dir = (segs[1] ?? 'asc').toLowerCase() === 'desc' ? 'desc' : 'asc'
        return `{ ${col}: '${dir}' }`
      })
      opts.push(`orderBy: ${parts.length === 1 ? parts[0] : `[${parts.join(', ')}]`}`)
    }

    if (limitIdx >= 0) {
      const afterLimit = afterFrom.slice(limitIdx + 5).trim()
      const limitEndIdx = offsetIdx >= 0 ? offsetIdx - limitIdx - 5 : undefined
      const limitVal = limitEndIdx != null ? afterLimit.slice(0, limitEndIdx).trim() : afterLimit.split(/\s+/)[0]
      opts.push(`take: ${limitVal}`)
    }

    if (offsetIdx >= 0) {
      const offsetVal = afterFrom.slice(offsetIdx + 6).trim().split(/\s+/)[0]
      opts.push(`skip: ${offsetVal}`)
    }

    const optsStr = opts.length ? `{\n  ${opts.join(',\n  ')}\n}` : '{}'
    return `prisma.${tableName}.findMany(${optsStr})`
  } catch {
    return `// Cannot auto-convert: ${sql}`
  }
}
