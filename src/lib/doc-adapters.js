// Per-engine adapters for DocBrowser.svelte: the one page that browses and
// edits CouchDB, Cassandra and Firestore. Each adapter turns that engine's API
// into the same few verbs (list collections, load a page, save, delete). The
// API calls are injected so the logic is unit-tested without Tauri.

/**
 * @typedef {{
 *   engine: string,
 *   noun: { collection: string, collections: string, item: string, items: string },
 *   consoleLabel: string,
 *   filterPlaceholder: string,
 *   filterHint: string,
 *   listCollections: () => Promise<string[]>,
 *   describe: (coll: string) => Promise<{ summary: string[], keys: string[], meta?: any }>,
 *   load: (coll: string, o: { cursor: any, filter: string, size: number, sort?: string, projection?: string, meta?: any }) => Promise<{ docs: Record<string, any>[], cursor: any }>,
 *   count?: (coll: string, filter: string, meta?: any) => Promise<number | null>,
 *   options?: { key: 'sort' | 'projection', label: string, placeholder: string }[],
 *   idOf: (doc: Record<string, any>, meta?: any) => string,
 *   save: (coll: string, next: Record<string, any>, prev: Record<string, any> | null, meta?: any) => Promise<Record<string, any>>,
 *   remove: (coll: string, doc: Record<string, any>, meta?: any) => Promise<void>,
 *   template: (coll: string, meta?: any) => Record<string, any>,
 *   keyChangeWarning: string,
 *   querySeed: (coll: string) => string,
 *   explain?: (coll: string, o: { filter: string, sort?: string, projection?: string }) => Promise<MongoExplain>,
 *   schema?: (coll: string, filter: string, size?: number) => Promise<MongoSchema>,
 *   indexes?: { list: (coll: string) => Promise<MongoIndex[]>, create: (coll: string, keys: string, options: Record<string, any>) => Promise<string>, drop: (coll: string, name: string) => Promise<void> },
 *   updateField?: (coll: string, doc: Record<string, any>, path: string, value: any, o?: { typeHint?: string, unset?: boolean }) => Promise<Record<string, any>>,
 *   aggregate?: (coll: string, pipeline: string, limit?: number) => Promise<Record<string, any>[]>,
 *   import?: { pick: () => Promise<string | null>, preview: (path: string, o?: { format?: string, infer?: boolean }) => Promise<{ fileName: string, format: string, total: number, preview: Record<string, any>[] }>, run: (coll: string, path: string, o?: { format?: string, infer?: boolean }) => Promise<{ inserted: number, failed: number, errors: string[] }> },
 * }} DocAdapter
 */

/**
 * @typedef {{ stage: string, depth: number, indexName: string | null, keyPattern: any, keysExamined: number | null, docsExamined: number | null, nReturned: number | null, ms: number | null }} MongoExplainStage
 * @typedef {{ stages: MongoExplainStage[], indexes: string[], collscan: boolean, nReturned: number | null, keysExamined: number | null, docsExamined: number | null, ms: number | null, verdict: string, raw: any }} MongoExplain
 * @typedef {{ path: string, count: number, presence: number, types: [string, number][], top: [any, number][], distinct: number, distinctCapped: boolean, min: any, max: any }} MongoField
 * @typedef {{ sampled: number, total: number, fields: MongoField[] }} MongoSchema
 * @typedef {{ name: string, keys: [string, any][], unique: boolean, sparse: boolean, hidden: boolean, ttlSeconds: number | null, partialFilter: any, ops: number | null, since: any, size: number | null }} MongoIndex
 */

export const PAGE_SIZE = 100
/** Page sizes the browser offers. */
export const PAGE_SIZES = [10, 20, 50, 100, 250, 500, 1000, -1]
/** "All": every page, fetched in chunks, up to a ceiling that keeps the grid usable. */
export const PAGE_SIZE_ALL = -1
export const ALL_CHUNK = 1000
export const ALL_CAP = 50_000

// ── CouchDB ─────────────────────────────────────────────────────────────────

/** @param {string} s */
const enc = (s) => encodeURIComponent(s)

/**
 * @param {(method: 'GET'|'POST'|'PUT'|'DELETE', path: string, body?: any) => Promise<any>} req
 * @returns {DocAdapter}
 */
export function couchAdapter(req) {
  return {
    engine: 'couchdb',
    noun: { collection: 'Database', collections: 'Databases', item: 'Document', items: 'documents' },
    consoleLabel: 'Mango',
    filterPlaceholder: '{"type": "product", "price": {"$gt": 50}}',
    filterHint: 'A Mango selector; empty lists every document by id',

    async listCollections() {
      const all = await req('GET', '_all_dbs')
      return all.filter((/** @type {string} */ d) => !d.startsWith('_'))
    },

    async describe(db) {
      const info = await req('GET', enc(db))
      const size = info.sizes?.active ?? info.sizes?.file
      return {
        keys: ['_id'],
        summary: [
          `${(info.doc_count ?? 0).toLocaleString()} docs`,
          ...(info.doc_del_count ? [`${info.doc_del_count.toLocaleString()} deleted`] : []),
          ...(size != null ? [humanBytes(size)] : []),
        ],
      }
    },

    async load(db, { cursor, filter, size = PAGE_SIZE }) {
      const f = filter.trim()
      if (f) {
        let selector
        try {
          selector = JSON.parse(f)
        } catch (e) {
          throw new Error(`The filter is a Mango selector in JSON: ${e instanceof Error ? e.message : e}`)
        }
        if (!selector || typeof selector !== 'object' || Array.isArray(selector)) throw new Error('A Mango selector is a JSON object')
        /** @type {Record<string, any>} */
        const body = { selector, limit: size }
        if (cursor) body.bookmark = cursor
        const out = await req('POST', `${enc(db)}/_find`, body)
        const docs = out.docs ?? []
        return { docs, cursor: docs.length === size ? out.bookmark : null }
      }
      // By id, one extra row to know whether there is a next page. Design
      // documents hold views and indexes, not data, and are skipped.
      const start = cursor ? `&startkey=${enc(JSON.stringify(cursor))}` : ''
      const out = await req('GET', `${enc(db)}/_all_docs?include_docs=true&limit=${size + 1}${start}`)
      const rows = out.rows ?? []
      const next = rows.length > size ? rows[size].id : null
      const docs = rows.slice(0, size).filter((/** @type {any} */ r) => !r.id.startsWith('_design/')).map((/** @type {any} */ r) => r.doc)
      return { docs, cursor: next }
    },

    // Unfiltered: the database's doc count less its design documents.
    async count(db, filter) {
      if (filter.trim()) return null
      const info = await req('GET', enc(db))
      const design = await req('GET', `${enc(db)}/_design_docs`).then((r) => r.rows?.length ?? 0, () => 0)
      return Math.max(0, (info.doc_count ?? 0) - design)
    },

    idOf: (doc) => String(doc._id ?? ''),

    async save(db, next, prev) {
      const id = typeof next._id === 'string' ? next._id.trim() : ''
      const doc = { ...next }
      if (!prev || id !== prev._id) {
        // A new document (or a new id): it must not carry the old revision.
        delete doc._rev
        if (!id) delete doc._id
        const out = id ? await req('PUT', `${enc(db)}/${enc(id)}`, doc) : await req('POST', enc(db), doc)
        if (prev) await req('DELETE', `${enc(db)}/${enc(prev._id)}?rev=${enc(prev._rev)}`)
        return { ...doc, _id: out.id, _rev: out.rev }
      }
      if (!doc._rev) doc._rev = prev._rev
      const out = await req('PUT', `${enc(db)}/${enc(id)}`, doc)
      return { ...doc, _rev: out.rev }
    },

    async remove(db, doc) {
      await req('DELETE', `${enc(db)}/${enc(doc._id)}?rev=${enc(doc._rev)}`)
    },

    template: () => ({ _id: '' }),
    keyChangeWarning: 'The _id changed. This creates a new document and deletes the old one. Continue?',
    querySeed: (db) => `${db}.find({})`,
  }
}

// ── Cassandra ───────────────────────────────────────────────────────────────

/**
 * @param {{
 *   tables: () => Promise<string[]>,
 *   describe: (t: string) => Promise<{ keys: string[], partition_keys: string[], columns: [string, string][] }>,
 *   page: (t: string, filter: string, cursor: string | null, size?: number) => Promise<{ rows: Record<string, any>[], next: string | null }>,
 *   save: (t: string, row: Record<string, any>, prev: Record<string, any> | null) => Promise<any>,
 *   remove: (t: string, row: Record<string, any>) => Promise<any>,
 * }} api
 * @returns {DocAdapter}
 */
export function cassandraAdapter(api) {
  return {
    engine: 'cassandra',
    noun: { collection: 'Table', collections: 'Tables', item: 'Row', items: 'rows' },
    consoleLabel: 'CQL',
    filterPlaceholder: "user_id = 6ab09bec-… AND ts > '2026-01-01'",
    filterHint: 'A CQL WHERE clause; add ALLOW FILTERING to filter on non-key columns',
    listCollections: () => api.tables(),
    async describe(table) {
      const shape = await api.describe(table)
      return {
        keys: shape.keys,
        meta: shape,
        summary: [
          `${shape.columns.length} columns`,
          `partition key (${shape.partition_keys.join(', ')})`,
          ...(shape.keys.length > shape.partition_keys.length ? [`clustering (${shape.keys.slice(shape.partition_keys.length).join(', ')})`] : []),
        ],
      }
    },
    async load(table, { cursor, filter, size = PAGE_SIZE }) {
      const p = await api.page(table, filter, cursor, size)
      return { docs: p.rows, cursor: p.next }
    },
    idOf: (doc, meta) => JSON.stringify((meta?.keys ?? []).map((/** @type {string} */ k) => doc[k] ?? null)),
    async save(table, next, prev) {
      await api.save(table, next, prev)
      return next
    },
    async remove(table, row) {
      await api.remove(table, row)
    },
    template: (_t, meta) => Object.fromEntries((meta?.keys ?? []).map((/** @type {string} */ k) => [k, null])),
    keyChangeWarning: 'The primary key changed. This writes a new row and deletes the old one in one batch. Continue?',
    querySeed: (table) => `SELECT * FROM ${table} LIMIT 100`,
  }
}

// ── MongoDB ─────────────────────────────────────────────────────────────────

/**
 * @param {{
 *   collections: () => Promise<string[]>,
 *   describe: (c: string) => Promise<{ count: number, indexes: string[], size: number | null, capped: boolean }>,
 *   find: (c: string, filter: string, skip: number, opts?: { limit?: number, sort?: string, projection?: string }) => Promise<{ docs: Record<string, any>[], next: number | null }>,
 *   count?: (c: string, filter: string) => Promise<number>,
 *   save: (c: string, doc: Record<string, any>, prevId: any) => Promise<Record<string, any>>,
 *   remove: (c: string, id: any) => Promise<any>,
 *   explain?: (c: string, filter: string, opts?: { sort?: string, projection?: string }) => Promise<MongoExplain>,
 *   schema?: (c: string, filter: string, size?: number) => Promise<MongoSchema>,
 *   indexes?: DocAdapter['indexes'],
 *   updateField?: (c: string, id: any, path: string, value: any, opts?: { typeHint?: string, unset?: boolean }) => Promise<Record<string, any>>,
 *   aggregate?: (c: string, pipeline: string, limit?: number) => Promise<Record<string, any>[]>,
 *   import?: DocAdapter['import'],
 * }} api
 * @returns {DocAdapter}
 */
export function mongoAdapter(api) {
  return {
    engine: 'mongodb',
    noun: { collection: 'Collection', collections: 'Collections', item: 'Document', items: 'documents' },
    consoleLabel: 'Query',
    filterPlaceholder: '{plan: "pro", age: {$gt: 30}}',
    filterHint: 'A query filter in shell syntax: {field: value}, $gt, $in, ObjectId("…"), ISODate("…")',
    listCollections: () => api.collections(),
    async describe(coll) {
      const i = await api.describe(coll)
      return {
        keys: ['_id'],
        summary: [
          `~${i.count.toLocaleString()} documents`,
          `${i.indexes.length} ${i.indexes.length === 1 ? 'index' : 'indexes'}`,
          ...(i.size != null ? [humanBytes(i.size)] : []),
          ...(i.capped ? ['capped'] : []),
        ],
      }
    },
    async load(coll, { cursor, filter, size = 20, sort, projection }) {
      const p = await api.find(coll, filter, Number(cursor) || 0, { limit: size, sort, projection })
      return { docs: p.docs, cursor: p.next }
    },
    count: api.count ? (coll, filter) => /** @type {any} */ (api.count)(coll, filter) : undefined,
    options: [
      { key: 'sort', label: 'Sort', placeholder: '{createdAt: -1}' },
      { key: 'projection', label: 'Project', placeholder: '{name: 1, email: 1}' },
    ],
    idOf: (doc) => JSON.stringify(doc._id ?? null),
    save: (coll, next, prev) => api.save(coll, next, prev ? prev._id : null),
    async remove(coll, doc) {
      await api.remove(coll, doc._id)
    },
    template: () => ({}),
    keyChangeWarning: 'The _id changed. This inserts a new document and deletes the old one. Continue?',
    // The editor's `db` already points at this collection's database (the
    // sidebar's schema), so no `use` line: it made an empty first result.
    querySeed(coll) {
      const name = coll.slice(coll.indexOf('.') + 1)
      const ref = /^[A-Za-z_$][\w$]*$/.test(name) ? `db.${name}` : `db.getCollection(${JSON.stringify(name)})`
      return `${ref}.find({}).limit(50)`
    },
    explain: api.explain ? (coll, o) => /** @type {any} */ (api.explain)(coll, o.filter, o) : undefined,
    schema: api.schema,
    indexes: api.indexes,
    updateField: api.updateField ? (coll, doc, path, value, o) => /** @type {any} */ (api.updateField)(coll, doc._id, path, value, o) : undefined,
    aggregate: api.aggregate,
    import: api.import,
  }
}

// ── Firestore ───────────────────────────────────────────────────────────────

/** @param {any} v @returns {any} Firestore typed value -> plain JSON */
export function fsPlain(v) {
  if (!v || typeof v !== 'object') return v
  const [t, x] = Object.entries(v)[0] ?? []
  switch (t) {
    case 'nullValue': return null
    case 'integerValue': {
      const n = Number(x)
      return Number.isSafeInteger(n) ? n : x
    }
    case 'mapValue': return fsPlainFields(x?.fields ?? {})
    case 'arrayValue': return (x?.values ?? []).map(fsPlain)
    case 'geoPointValue': return { latitude: x?.latitude ?? 0, longitude: x?.longitude ?? 0 }
    default: return x
  }
}

/** @param {Record<string, any>} fields */
export function fsPlainFields(fields) {
  /** @type {Record<string, any>} */
  const o = {}
  for (const [k, v] of Object.entries(fields)) o[k] = fsPlain(v)
  return o
}

/** @param {any} hint */
const fsType = (hint) => (hint && typeof hint === 'object' ? Object.keys(hint)[0] : undefined)

/**
 * Plain JSON -> Firestore typed value, keeping the previous value's type
 * (timestamp, reference, bytes, geo point, integer vs double) where it fits.
 * @param {any} v @param {any} [hint]
 */
export function fsTyped(v, hint) {
  const ht = fsType(hint)
  if (v === null || v === undefined) return { nullValue: null }
  if (typeof v === 'boolean') return { booleanValue: v }
  if (typeof v === 'number') {
    if (!Number.isFinite(v)) throw new Error(`${v} can't be stored in Firestore`)
    return Number.isInteger(v) && ht !== 'doubleValue' ? { integerValue: String(v) } : { doubleValue: v }
  }
  if (typeof v === 'string') {
    if (ht === 'integerValue' && /^-?\d+$/.test(v)) return { integerValue: v }
    if (ht === 'timestampValue' && !Number.isNaN(Date.parse(v))) return { timestampValue: v }
    if (ht === 'referenceValue') return { referenceValue: v }
    if (ht === 'bytesValue') return { bytesValue: v }
    return { stringValue: v }
  }
  if (Array.isArray(v)) {
    const hv = ht === 'arrayValue' ? hint.arrayValue?.values : undefined
    return { arrayValue: { values: v.map((x, i) => fsTyped(x, hv?.[i])) } }
  }
  if (typeof v === 'object') {
    const keys = Object.keys(v)
    if (ht === 'geoPointValue' && keys.length === 2 && typeof v.latitude === 'number' && typeof v.longitude === 'number') {
      return { geoPointValue: { latitude: v.latitude, longitude: v.longitude } }
    }
    return { mapValue: { fields: fsTypedFields(v, ht === 'mapValue' ? hint.mapValue?.fields : undefined) } }
  }
  throw new Error(`Can't store a ${typeof v} in Firestore`)
}

/** @param {Record<string, any>} obj @param {Record<string, any>} [hints] */
export function fsTypedFields(obj, hints) {
  /** @type {Record<string, any>} */
  const out = {}
  for (const [k, v] of Object.entries(obj)) if (v !== undefined) out[k] = fsTyped(v, hints?.[k])
  return out
}

export const FS_OPS = ['==', '!=', '<=', '>=', '<', '>', 'array-contains-any', 'array-contains', 'not-in', 'in']
const FS_OP_NAMES = /** @type {Record<string, string>} */ ({
  '==': 'EQUAL', '!=': 'NOT_EQUAL', '<': 'LESS_THAN', '<=': 'LESS_THAN_OR_EQUAL', '>': 'GREATER_THAN',
  '>=': 'GREATER_THAN_OR_EQUAL', 'array-contains': 'ARRAY_CONTAINS', 'array-contains-any': 'ARRAY_CONTAINS_ANY',
  in: 'IN', 'not-in': 'NOT_IN',
})

/**
 * The browser's filter, `age > 30 AND plan == "pro"`, as a Firestore filter.
 * Values are JSON; a bare word is a string.
 * @param {string} text
 */
export function fsFilter(text) {
  const parts = text.trim().split(/\s+AND\s+/i).filter(Boolean)
  const filters = parts.map((p) => {
    const m = /^([\w.`$-]+)\s*(==|!=|<=|>=|<|>|array-contains-any|array-contains|not-in|in)\s*(.+)$/i.exec(p.trim())
    if (!m) throw new Error(`Can't read "${p}". Write field op value, e.g. age > 30 AND plan == "pro"`)
    const op = m[2].toLowerCase()
    let value
    try {
      value = JSON.parse(m[3])
    } catch {
      value = m[3].trim()
    }
    return { fieldFilter: { field: { fieldPath: m[1].replaceAll('`', '') }, op: FS_OP_NAMES[op], value: fsTyped(value) } }
  })
  if (filters.length === 0) return null
  return filters.length === 1 ? filters[0] : { compositeFilter: { op: 'AND', filters } }
}

/**
 * A REST document as the browser's plain doc: `_id` plus fields. The typed
 * fields ride along, hidden, as the type hints for saving it back.
 * @param {any} doc
 */
export function fsDoc(doc) {
  const out = { _id: String(doc.name ?? '').split('/').pop() ?? '', ...fsPlainFields(doc.fields ?? {}) }
  Object.defineProperty(out, '__typed', { value: doc.fields ?? {}, enumerable: false })
  return out
}

/**
 * @param {(method: 'GET'|'POST'|'PATCH'|'DELETE', path: string, body?: any) => Promise<any>} req
 * @param {{ projectId: string, database?: string }} conn
 * @returns {DocAdapter}
 */
export function firestoreAdapter(req, conn) {
  const root = `projects/${conn.projectId}/databases/${conn.database || '(default)'}/documents`
  /** @param {string} coll */
  const parentOf = (coll) => {
    const i = coll.lastIndexOf('/')
    return i < 0 ? { parent: root, id: coll } : { parent: `${root}/${coll.slice(0, i)}`, id: coll.slice(i + 1) }
  }
  /** @param {string} coll @param {string} id */
  const docPath = (coll, id) => `${root}/${coll}/${encodeURIComponent(id)}`
  return {
    engine: 'firestore',
    noun: { collection: 'Collection', collections: 'Collections', item: 'Document', items: 'documents' },
    consoleLabel: 'Query',
    filterPlaceholder: 'age > 30 AND plan == "pro"',
    filterHint: 'field op value, joined with AND; ops: == != < <= > >= in not-in array-contains',

    async listCollections() {
      /** @type {string[]} */
      const out = []
      let pageToken = ''
      do {
        const r = await req('POST', `${root}:listCollectionIds`, { pageSize: 300, ...(pageToken && { pageToken }) })
        out.push(...(r.collectionIds ?? []))
        pageToken = r.nextPageToken ?? ''
      } while (pageToken)
      return out.sort()
    },

    async describe(coll) {
      const { parent, id } = parentOf(coll)
      let summary = /** @type {string[]} */ ([])
      try {
        const r = await req('POST', `${parent}:runAggregationQuery`, {
          structuredAggregationQuery: { structuredQuery: { from: [{ collectionId: id }] }, aggregations: [{ alias: 'n', count: {} }] },
        })
        const n = r?.[0]?.result?.aggregateFields?.n?.integerValue
        if (n != null) summary = [`${Number(n).toLocaleString()} documents`]
      } catch {
        // Counting is a nicety; some setups refuse aggregations.
      }
      return { keys: ['_id'], summary }
    },

    async count(coll, filter) {
      const { parent, id } = parentOf(coll)
      const where = fsFilter(filter)
      const r = await req('POST', `${parent}:runAggregationQuery`, {
        structuredAggregationQuery: { structuredQuery: { from: [{ collectionId: id }], ...(where && { where }) }, aggregations: [{ alias: 'n', count: {} }] },
      })
      const n = r?.[0]?.result?.aggregateFields?.n?.integerValue
      return n == null ? null : Number(n)
    },

    async load(coll, { cursor, filter, size = PAGE_SIZE }) {
      const f = fsFilter(filter)
      if (!f) {
        const tok = cursor ? `&pageToken=${encodeURIComponent(cursor)}` : ''
        const r = await req('GET', `${root}/${coll}?pageSize=${size}${tok}`)
        return { docs: (r.documents ?? []).map(fsDoc), cursor: r.nextPageToken || null }
      }
      const { parent, id } = parentOf(coll)
      const offset = Number(cursor) || 0
      const r = await req('POST', `${parent}:runQuery`, {
        structuredQuery: { from: [{ collectionId: id }], where: f, limit: size, ...(offset && { offset }) },
      })
      const docs = (r ?? []).filter((/** @type {any} */ x) => x.document).map((/** @type {any} */ x) => fsDoc(x.document))
      return { docs, cursor: docs.length === size ? offset + size : null }
    },

    idOf: (doc) => String(doc._id ?? ''),

    async save(coll, next, prev) {
      const { _id, ...rest } = next
      const id = typeof _id === 'string' ? _id.trim() : ''
      const fields = fsTypedFields(rest, prev ? /** @type {any} */ (prev).__typed : undefined)
      /** @type {any} */
      let out
      if (prev && id === prev._id) {
        out = await req('PATCH', docPath(coll, id), { fields })
      } else if (id) {
        out = await req('PATCH', `${docPath(coll, id)}?currentDocument.exists=false`, { fields })
        if (prev) await req('DELETE', docPath(coll, prev._id))
      } else {
        out = await req('POST', `${root}/${coll}`, { fields })
        if (prev) await req('DELETE', docPath(coll, prev._id))
      }
      return fsDoc(out)
    },

    async remove(coll, doc) {
      await req('DELETE', docPath(coll, doc._id))
    },

    template: () => ({ _id: '' }),
    keyChangeWarning: 'The document id changed. This creates a new document and deletes the old one. Continue?',
    querySeed: (coll) => `${coll}.limit(50)`,
  }
}

// ── Shared helpers ──────────────────────────────────────────────────────────

/**
 * A connection URI without its credentials or options, for display:
 * `mongodb+srv://u:p@cluster0.x.net/shop?retryWrites=true` -> `cluster0.x.net/shop`.
 * @param {string} uri
 */
export function uriHost(uri) {
  const rest = String(uri).replace(/^[a-z+]+:\/\//i, '')
  const end = rest.search(/[/?]/)
  const hosts = end < 0 ? rest : rest.slice(0, end)
  const path = end < 0 ? '' : rest.slice(end).split('?')[0]
  return (hosts.slice(hosts.lastIndexOf('@') + 1) + path).replace(/\/$/, '')
}

/** @param {number} n */
export function humanBytes(n) {
  if (!Number.isFinite(n)) return ''
  const u = ['B', 'KB', 'MB', 'GB', 'TB']
  let i = 0
  while (n >= 1024 && i < u.length - 1) {
    n /= 1024
    i++
  }
  return `${i ? n.toFixed(1) : n} ${u[i]}`
}

/**
 * Grid columns: key fields first, then every field in first-seen order.
 * @param {Record<string, any>[]} docs @param {string[]} keys
 */
export function docColumns(docs, keys) {
  const cols = [...keys]
  const seen = new Set(keys)
  for (const d of docs) {
    for (const k of Object.keys(d ?? {})) {
      if (!seen.has(k)) {
        seen.add(k)
        cols.push(k)
      }
    }
  }
  return cols
}

/**
 * One-line text for a grid cell, and its JSON kind (for colour).
 * @param {any} v @param {number} [max]
 * @returns {{ text: string, kind: 'missing' | 'null' | 'string' | 'number' | 'boolean' | 'object' }}
 */
export function docCell(v, max = 200) {
  if (v === undefined) return { text: '—', kind: 'missing' }
  if (v === null) return { text: 'null', kind: 'null' }
  /** @type {any} */
  const kind = typeof v === 'object' ? 'object' : typeof v
  const s = typeof v === 'object' ? JSON.stringify(v) : String(v)
  return { text: s.length > max ? s.slice(0, max) + '…' : s, kind }
}

const EXT_TYPES = /** @type {Record<string, string>} */ ({ $oid: 'objectId', $date: 'date', $numberLong: 'long', $numberDecimal: 'decimal', $uuid: 'uuid' })

/**
 * A field's display value and type: Extended JSON scalars (`{"$oid": …}`) as
 * their value, everything else as is.
 * @param {any} v
 * @returns {{ value: any, type: string }}
 */
export function docField(v) {
  if (v === undefined || v === null) return { value: null, type: '' }
  if (Array.isArray(v)) return { value: v, type: 'array' }
  if (typeof v === 'object') {
    const k = Object.keys(v)
    if (k.length === 1 && EXT_TYPES[k[0]] && typeof v[k[0]] === 'string') return { value: v[k[0]], type: EXT_TYPES[k[0]] }
    return { value: v, type: 'object' }
  }
  if (typeof v === 'number') return { value: v, type: Number.isInteger(v) ? 'int' : 'double' }
  return { value: v, type: typeof v === 'boolean' ? 'bool' : 'string' }
}

/**
 * Documents as the data grid's columns and rows: key fields first, a column
 * per field, each typed by its values (`mixed` when they disagree).
 * @param {Record<string, any>[]} docs @param {string[]} keys
 * @returns {{ columns: { name: string, data_type: string, nullable: boolean }[], rows: any[][] }}
 */
export function docGrid(docs, keys) {
  const names = docColumns(docs, keys)
  const types = names.map(() => '')
  const rows = docs.map((d) =>
    names.map((n, i) => {
      const f = docField(d?.[n])
      if (f.type) {
        const t = types[i]
        types[i] = !t || t === f.type ? f.type : (t === 'int' && f.type === 'double') || (t === 'double' && f.type === 'int') ? 'double' : 'mixed'
      }
      return f.value
    }),
  )
  return { columns: names.map((name, i) => ({ name, data_type: types[i] || 'null', nullable: true })), rows }
}
