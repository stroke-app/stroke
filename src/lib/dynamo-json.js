// DynamoDB typed JSON (`{"S": "x"}`, `{"N": "1"}`, ...) <-> plain JSON, plus
// the request bodies the DynamoDB page sends. Pure: no Svelte, no IPC.
//
// Plain JSON loses types a value can't carry on its own: sets look like lists,
// binary looks like a string, and a 30-digit number doesn't fit a JS number.
// So `marshall` takes the item's original typed value as a hint and keeps
// each attribute's type wherever the edited value still fits it.

/** @typedef {Record<string, any>} AttrValue */
/** @typedef {Record<string, AttrValue>} Item */

/** @param {string} n */
export function plainNumber(n) {
  const x = Number(n)
  if (!Number.isFinite(x)) return n
  if (/^-?\d+$/.test(n)) return Number.isSafeInteger(x) ? x : n
  return String(x) === n ? x : n
}

/**
 * Typed attribute -> plain JSON value.
 * @param {AttrValue} av
 * @returns {any}
 */
export function unmarshall(av) {
  if (!av || typeof av !== 'object') return av
  const [t, v] = Object.entries(av)[0] ?? []
  switch (t) {
    case 'S':
    case 'B':
      return v
    case 'N':
      return plainNumber(v)
    case 'BOOL':
      return !!v
    case 'NULL':
      return null
    case 'SS':
    case 'BS':
      return [...v]
    case 'NS':
      return v.map(plainNumber)
    case 'L':
      return v.map(unmarshall)
    case 'M':
      return unmarshallItem(v)
    default:
      return v
  }
}

/** @param {Item} item */
export function unmarshallItem(item) {
  /** @type {Record<string, any>} */
  const out = {}
  for (const [k, v] of Object.entries(item ?? {})) out[k] = unmarshall(v)
  return out
}

const NUMERIC = /^-?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?$/i

/** @param {AttrValue | undefined} hint */
const typeOf = (hint) => (hint && typeof hint === 'object' ? Object.keys(hint)[0] : undefined)

/**
 * Plain JSON value -> typed attribute. `hint` is the attribute's previous
 * typed value, when there was one.
 * @param {any} v
 * @param {AttrValue} [hint]
 * @returns {AttrValue}
 */
export function marshall(v, hint) {
  const ht = typeOf(hint)
  if (v === null || v === undefined) return { NULL: true }
  if (typeof v === 'boolean') return { BOOL: v }
  if (typeof v === 'number') {
    if (!Number.isFinite(v)) throw new Error(`${v} is not a number DynamoDB can store`)
    return { N: String(v) }
  }
  if (typeof v === 'string') {
    if (ht === 'N' && NUMERIC.test(v.trim())) return { N: v.trim() }
    if (ht === 'B') return { B: v }
    return { S: v }
  }
  if (Array.isArray(v)) {
    const unique = new Set(v.map((x) => (typeof x === 'number' ? String(x) : x))).size === v.length
    if (v.length > 0 && unique) {
      if (ht === 'SS' && v.every((x) => typeof x === 'string')) return { SS: v }
      if (ht === 'BS' && v.every((x) => typeof x === 'string')) return { BS: v }
      if (ht === 'NS' && v.every((x) => typeof x === 'number' || (typeof x === 'string' && NUMERIC.test(x)))) {
        return { NS: v.map((x) => String(x)) }
      }
    }
    const hl = ht === 'L' ? hint?.L : undefined
    return { L: v.map((x, i) => marshall(x, hl?.[i])) }
  }
  if (typeof v === 'object') {
    const hm = ht === 'M' ? hint?.M : undefined
    return { M: marshallItem(v, hm) }
  }
  throw new Error(`Can't store a ${typeof v} in DynamoDB`)
}

/**
 * @param {Record<string, any>} obj
 * @param {Item} [hint]
 * @returns {Item}
 */
export function marshallItem(obj, hint) {
  /** @type {Item} */
  const out = {}
  for (const [k, v] of Object.entries(obj)) {
    if (v === undefined) continue
    out[k] = marshall(v, hint?.[k])
  }
  return out
}

// ── Display ─────────────────────────────────────────────────────────────────

/** Short type tag for a typed value (`S`, `N`, `SS`, `M`, ...). @param {AttrValue | undefined} av */
export const attrType = (av) => typeOf(av) ?? ''

/**
 * One-line text for a grid cell.
 * @param {AttrValue | undefined} av
 * @param {number} [max]
 */
export function cellText(av, max = 200) {
  if (av === undefined) return ''
  const t = typeOf(av)
  /** @type {string} */
  let s
  if (t === 'S' || t === 'N' || t === 'B') s = String(av[t])
  else if (t === 'BOOL') s = av.BOOL ? 'true' : 'false'
  else if (t === 'NULL') s = 'null'
  else if (t === 'SS' || t === 'NS' || t === 'BS') s = `{${av[t].join(', ')}}`
  else s = JSON.stringify(unmarshall(av))
  return s.length > max ? s.slice(0, max) + '…' : s
}

/**
 * Columns for a page of items: key attributes first, then every other
 * attribute in the order it first appears.
 * @param {Item[]} items
 * @param {string[]} keys
 */
export function columnsFor(items, keys) {
  const cols = [...keys]
  const seen = new Set(keys)
  for (const it of items) {
    for (const k of Object.keys(it)) {
      if (!seen.has(k)) {
        seen.add(k)
        cols.push(k)
      }
    }
  }
  return cols
}

// ── Table description ───────────────────────────────────────────────────────

/**
 * @typedef {{ name: string, type: string }} KeyAttr
 * @typedef {{ name: string, kind: 'table' | 'gsi' | 'lsi', hash: KeyAttr, range: KeyAttr | null }} KeyShape
 */

/**
 * The table's own key and each index's key, with attribute types.
 * @param {any} table DescribeTable's `Table`
 * @returns {KeyShape[]}
 */
export function keyShapes(table) {
  /** @type {Record<string, string>} */
  const types = {}
  for (const d of table?.AttributeDefinitions ?? []) types[d.AttributeName] = d.AttributeType
  /** @param {any[]} schema */
  const shape = (schema) => {
    const h = schema.find((k) => k.KeyType === 'HASH')
    const r = schema.find((k) => k.KeyType === 'RANGE')
    return {
      hash: { name: h?.AttributeName ?? '', type: types[h?.AttributeName] ?? 'S' },
      range: r ? { name: r.AttributeName, type: types[r.AttributeName] ?? 'S' } : null,
    }
  }
  /** @type {KeyShape[]} */
  const out = [{ name: table?.TableName ?? '', kind: 'table', ...shape(table?.KeySchema ?? []) }]
  for (const g of table?.GlobalSecondaryIndexes ?? []) out.push({ name: g.IndexName, kind: 'gsi', ...shape(g.KeySchema) })
  for (const l of table?.LocalSecondaryIndexes ?? []) out.push({ name: l.IndexName, kind: 'lsi', ...shape(l.KeySchema) })
  return out
}

/** The primary-key attributes of an item. @param {Item} item @param {KeyShape} tableKey */
export function keyOf(item, tableKey) {
  /** @type {Item} */
  const k = { [tableKey.hash.name]: item[tableKey.hash.name] }
  if (tableKey.range) k[tableKey.range.name] = item[tableKey.range.name]
  return k
}

/** A stable string id for an item's key. @param {Item} item @param {KeyShape} tableKey */
export function keyId(item, tableKey) {
  const k = keyOf(item, tableKey)
  return JSON.stringify(Object.keys(k).sort().map((n) => [n, k[n]]))
}

/**
 * A key value typed for its attribute (`S`, `N` or `B`).
 * @param {string} text @param {string} type
 */
export function keyValue(text, type) {
  if (type === 'N') {
    const t = text.trim()
    if (!NUMERIC.test(t)) throw new Error(`"${text}" is not a number`)
    return { N: t }
  }
  return { [type === 'B' ? 'B' : 'S']: text }
}

// ── Requests ────────────────────────────────────────────────────────────────

export const SORT_OPS = [
  { value: '=', label: '=' },
  { value: '<', label: '<' },
  { value: '<=', label: '≤' },
  { value: '>', label: '>' },
  { value: '>=', label: '≥' },
  { value: 'begins_with', label: 'begins with' },
  { value: 'between', label: 'between' },
]

/**
 * Body for a Query on the table or one of its indexes.
 * @param {string} table
 * @param {KeyShape} shape
 * @param {{ hash: string, sortOp?: string, sort?: string, sort2?: string, descending?: boolean, limit?: number, startKey?: Item | null }} q
 */
export function queryBody(table, shape, q) {
  /** @type {Record<string, string>} */
  const names = { '#h': shape.hash.name }
  /** @type {Item} */
  const values = { ':h': keyValue(q.hash, shape.hash.type) }
  let expr = '#h = :h'
  const op = q.sortOp || ''
  if (shape.range && op && (q.sort ?? '') !== '') {
    names['#r'] = shape.range.name
    values[':r'] = keyValue(q.sort ?? '', shape.range.type)
    if (op === 'begins_with') expr += ' AND begins_with(#r, :r)'
    else if (op === 'between') {
      values[':r2'] = keyValue(q.sort2 ?? '', shape.range.type)
      expr += ' AND #r BETWEEN :r AND :r2'
    } else expr += ` AND #r ${op} :r`
  }
  /** @type {Record<string, any>} */
  const body = {
    TableName: table,
    KeyConditionExpression: expr,
    ExpressionAttributeNames: names,
    ExpressionAttributeValues: values,
    ScanIndexForward: !q.descending,
    Limit: q.limit ?? 100,
  }
  if (shape.kind !== 'table') body.IndexName = shape.name
  if (q.startKey) body.ExclusiveStartKey = q.startKey
  return body
}

/**
 * Body for a Scan of the table or an index.
 * @param {string} table @param {KeyShape} shape @param {{ limit?: number, startKey?: Item | null }} [o]
 */
export function scanBody(table, shape, o = {}) {
  /** @type {Record<string, any>} */
  const body = { TableName: table, Limit: o.limit ?? 100 }
  if (shape.kind !== 'table') body.IndexName = shape.name
  if (o.startKey) body.ExclusiveStartKey = o.startKey
  return body
}

/**
 * The write that saves an edited item. Same key: a plain PutItem. A new item:
 * PutItem that refuses to overwrite. A changed key: put the new item and
 * delete the old one in one transaction.
 * @param {string} table
 * @param {KeyShape} tableKey
 * @param {Item} next the item to store
 * @param {Item | null} prev the item as loaded, or null for a new one
 * @returns {{ op: 'PutItem' | 'TransactWriteItems', body: any }}
 */
export function saveRequest(table, tableKey, next, prev) {
  for (const k of [tableKey.hash, tableKey.range]) {
    if (k && next[k.name] === undefined) throw new Error(`The item needs its key attribute "${k.name}"`)
    if (k && attrType(next[k.name]) !== k.type) {
      throw new Error(`Key "${k.name}" must be ${k.type === 'N' ? 'a number' : k.type === 'B' ? 'binary' : 'a string'}`)
    }
  }
  const notExists = {
    ConditionExpression: 'attribute_not_exists(#h)',
    ExpressionAttributeNames: { '#h': tableKey.hash.name },
  }
  if (!prev) return { op: 'PutItem', body: { TableName: table, Item: next, ...notExists } }
  if (keyId(next, tableKey) === keyId(prev, tableKey)) return { op: 'PutItem', body: { TableName: table, Item: next } }
  return {
    op: 'TransactWriteItems',
    body: {
      TransactItems: [
        { Put: { TableName: table, Item: next, ...notExists } },
        { Delete: { TableName: table, Key: keyOf(prev, tableKey) } },
      ],
    },
  }
}

/** Starter text for a new item: its key attributes, empty. @param {KeyShape} tableKey */
export function newItemTemplate(tableKey) {
  /** @type {Record<string, any>} */
  const o = {}
  for (const k of [tableKey.hash, tableKey.range]) if (k) o[k.name] = k.type === 'N' ? 0 : ''
  return o
}

const TYPE_NAMES = /** @type {Record<string, string>} */ ({
  S: 'string', N: 'number', B: 'binary', BOOL: 'bool', NULL: 'null',
  SS: 'string set', NS: 'number set', BS: 'binary set', L: 'list', M: 'map',
})

/**
 * Items as the data grid's columns and rows: key attributes first, a column
 * per attribute typed by its DynamoDB type (`mixed` when items disagree), and
 * values as plain JSON (sets as arrays, maps as objects).
 * @param {Item[]} items @param {string[]} keys
 * @returns {{ columns: { name: string, data_type: string, nullable: boolean }[], rows: any[][] }}
 */
export function dynamoGrid(items, keys) {
  const names = columnsFor(items, keys)
  const types = names.map(() => '')
  const rows = items.map((it) =>
    names.map((n, i) => {
      const av = it[n]
      if (av === undefined) return null
      const t = TYPE_NAMES[attrType(av)] ?? attrType(av)
      types[i] = !types[i] || types[i] === t ? t : 'mixed'
      return unmarshall(av)
    }),
  )
  return { columns: names.map((name, i) => ({ name, data_type: types[i] || 'null', nullable: !keys.includes(name) })), rows }
}
