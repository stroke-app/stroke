import { describe, it, expect, vi } from 'vitest'
import { couchAdapter, cassandraAdapter, firestoreAdapter, convexAdapter, cxFilter, CX_UNDEFINED, mongoAdapter, neo4jAdapter, influxAdapter, uriHost, docGrid, fsPlainFields, fsTypedFields, fsTyped, fsFilter, fsDoc, docColumns, docCell, humanBytes, PAGE_SIZE } from './doc-adapters.js'

/** A fake CouchDB that records calls and answers from a script. */
function fake(/** @type {Record<string, any>} */ answers = {}) {
  /** @type {[string, string, any][]} */
  const calls = []
  const req = async (/** @type {string} */ m, /** @type {string} */ p, /** @type {any} */ b) => {
    calls.push([m, p, b])
    const a = answers[`${m} ${p.split('?')[0]}`]
    if (a instanceof Error) throw a
    return typeof a === 'function' ? a(p, b) : a ?? { ok: true, id: b?._id ?? 'gen-1', rev: '2-x' }
  }
  return { req, calls }
}

describe('couch adapter', () => {
  it('hides system databases', async () => {
    const { req } = fake({ 'GET _all_dbs': ['_users', 'products', '_replicator', 'orders'] })
    expect(await couchAdapter(req).listCollections()).toEqual(['products', 'orders'])
  })

  it('pages _all_docs by id, skipping design docs', async () => {
    const rows = Array.from({ length: PAGE_SIZE + 1 }, (_, i) => ({ id: `d${String(i).padStart(3, '0')}`, doc: { _id: `d${i}` } }))
    rows[3] = { id: '_design/v', doc: { _id: '_design/v' } }
    const { req, calls } = fake({ 'GET products/_all_docs': { rows } })
    const a = couchAdapter(req)
    const page = await a.load('products', { cursor: null, filter: '' })
    expect(page.docs.length).toBe(PAGE_SIZE - 1)
    expect(page.cursor).toBe(rows[PAGE_SIZE].id)
    await a.load('products', { cursor: 'd100', filter: '' })
    expect(calls[1][1]).toContain('startkey=%22d100%22')
  })

  it('filters with a Mango selector and follows the bookmark', async () => {
    const docs = Array.from({ length: PAGE_SIZE }, (_, i) => ({ _id: String(i) }))
    const { req, calls } = fake({ 'POST products/_find': { docs, bookmark: 'bm1' } })
    const a = couchAdapter(req)
    const page = await a.load('products', { cursor: null, filter: '{"type":"product"}' })
    expect(calls[0][2]).toEqual({ selector: { type: 'product' }, limit: PAGE_SIZE })
    expect(page.cursor).toBe('bm1')
    await a.load('products', { cursor: 'bm1', filter: '{"type":"product"}' })
    expect(calls[1][2].bookmark).toBe('bm1')
    await expect(a.load('products', { cursor: null, filter: '{nope' })).rejects.toThrow('Mango selector')
    await expect(a.load('products', { cursor: null, filter: '[1]' })).rejects.toThrow('JSON object')
  })

  it('saves: update keeps the rev, new id creates then deletes the old', async () => {
    const { req, calls } = fake()
    const a = couchAdapter(req)
    const prev = { _id: 'p1', _rev: '1-a', name: 'x' }
    const saved = await a.save('db', { ...prev, name: 'y' }, prev)
    expect(calls[0]).toEqual(['PUT', 'db/p1', { _id: 'p1', _rev: '1-a', name: 'y' }])
    expect(saved._rev).toBe('2-x')

    calls.length = 0
    await a.save('db', { ...prev, _id: 'p2' }, prev)
    expect(calls[0]).toEqual(['PUT', 'db/p2', { _id: 'p2', name: 'x' }])
    expect(calls[1]).toEqual(['DELETE', 'db/p1?rev=1-a', undefined])

    calls.length = 0
    const created = await a.save('db', { _id: '', name: 'n' }, null)
    expect(calls[0]).toEqual(['POST', 'db', { name: 'n' }])
    expect(created._id).toBe('gen-1')
  })

  it('encodes ids with slashes', async () => {
    const { req, calls } = fake()
    await couchAdapter(req).remove('my db', { _id: 'a/b', _rev: '3-c' })
    expect(calls[0][1]).toBe('my%20db/a%2Fb?rev=3-c')
  })
})

describe('helpers', () => {
  it('columns, cells, sizes', () => {
    expect(docColumns([{ b: 1, _id: 'x' }, { a: 2 }], ['_id'])).toEqual(['_id', 'b', 'a'])
    expect(docCell(undefined)).toEqual({ text: '—', kind: 'missing' })
    expect(docCell({ a: [1] })).toEqual({ text: '{"a":[1]}', kind: 'object' })
    expect(docCell('x'.repeat(5), 2)).toEqual({ text: 'xx…', kind: 'string' })
    expect(humanBytes(2048)).toBe('2.0 KB')
  })
})

describe('cassandra adapter', () => {
  const shape = { keys: ['user_id', 'ts'], partition_keys: ['user_id'], columns: [['user_id', 'uuid'], ['ts', 'timestamp'], ['total', 'decimal']] }
  /** @type {any[]} */
  const calls = []
  const a = cassandraAdapter({
    tables: async () => ['shop.users'],
    describe: async () => shape,
    page: async (t, f, c) => { calls.push(['page', t, f, c]); return { rows: [{ user_id: 'u', ts: 1 }], next: 'cur' } },
    save: async (t, r, p) => { calls.push(['save', t, r, p]) },
    remove: async (t, r) => { calls.push(['remove', t, r]) },
  })
  it('describes keys and summary', async () => {
    const d = await a.describe('shop.orders')
    expect(d.keys).toEqual(['user_id', 'ts'])
    expect(d.summary).toEqual(['3 columns', 'partition key (user_id)', 'clustering (ts)'])
  })
  it('ids by primary key, pages with the cursor', async () => {
    expect(a.idOf({ ts: 2, user_id: 'u', total: 5 }, shape)).toBe('["u",2]')
    const p = await a.load('shop.orders', { cursor: 'c1', filter: 'user_id = x' })
    expect(p).toEqual({ docs: [{ user_id: 'u', ts: 1 }], cursor: 'cur' })
    expect(calls.at(-1)).toEqual(['page', 'shop.orders', 'user_id = x', 'c1'])
  })
  it('template has the key columns', () => {
    expect(a.template('t', shape)).toEqual({ user_id: null, ts: null })
  })
})

describe('firestore', () => {
  const typed = {
    n: { integerValue: '31' }, big: { integerValue: '9007199254740993' }, d: { doubleValue: 2 },
    t: { timestampValue: '2026-01-14T10:00:00Z' }, g: { geoPointValue: { latitude: 1, longitude: 2 } },
    r: { referenceValue: 'projects/p/databases/(default)/documents/users/u1' }, z: { nullValue: null },
    m: { mapValue: { fields: { a: { arrayValue: { values: [{ stringValue: 'x' }, { booleanValue: true }] } } } } },
  }
  it('round-trips through plain JSON with hints', () => {
    const plain = fsPlainFields(typed)
    expect(plain).toEqual({
      n: 31, big: '9007199254740993', d: 2, t: '2026-01-14T10:00:00Z', g: { latitude: 1, longitude: 2 },
      r: 'projects/p/databases/(default)/documents/users/u1', z: null, m: { a: ['x', true] },
    })
    expect(fsTypedFields(plain, typed)).toEqual(typed)
  })
  it('without hints: integers vs doubles, strings stay strings', () => {
    expect(fsTyped(3)).toEqual({ integerValue: '3' })
    expect(fsTyped(3.5)).toEqual({ doubleValue: 3.5 })
    expect(fsTyped('2026-01-01')).toEqual({ stringValue: '2026-01-01' })
  })
  it('parses the filter box', () => {
    expect(fsFilter('')).toBe(null)
    expect(fsFilter('age > 30')).toEqual({ fieldFilter: { field: { fieldPath: 'age' }, op: 'GREATER_THAN', value: { integerValue: '30' } } })
    const f = fsFilter('plan == pro AND tags array-contains "beta"')
    expect(f.compositeFilter.filters.map((/** @type {any} */ x) => x.fieldFilter.op)).toEqual(['EQUAL', 'ARRAY_CONTAINS'])
    expect(f.compositeFilter.filters[0].fieldFilter.value).toEqual({ stringValue: 'pro' })
    expect(fsFilter('plan in ["a","b"]').fieldFilter.op).toBe('IN')
    expect(() => fsFilter('nonsense')).toThrow("Can't read")
  })
  it('adapter: save paths for update, create, auto id, id change', async () => {
    /** @type {any[]} */
    const calls = []
    const req = async (/** @type {string} */ m, /** @type {string} */ p, /** @type {any} */ b) => {
      calls.push([m, p, b])
      return { name: `${p.split('?')[0]}/auto`.replace(/\/auto$/, m === 'POST' ? '/auto' : ''), fields: b?.fields ?? {} }
    }
    const a = firestoreAdapter(req, { projectId: 'p' })
    const root = 'projects/p/databases/(default)/documents'
    const prev = fsDoc({ name: `${root}/users/u1`, fields: { n: { integerValue: '1' } } })
    await a.save('users', { ...prev, n: 2 }, prev)
    expect(calls[0]).toEqual(['PATCH', `${root}/users/u1`, { fields: { n: { integerValue: '2' } } }])
    await a.save('users', { _id: 'u9', n: 1 }, null)
    expect(calls[1][1]).toBe(`${root}/users/u9?currentDocument.exists=false`)
    const created = await a.save('users', { _id: '', n: 1 }, null)
    expect(calls[2][0]).toBe('POST')
    expect(created._id).toBe('auto')
    calls.length = 0
    await a.save('users', { ...prev, _id: 'u2' }, prev)
    expect(calls.map((c) => c[0])).toEqual(['PATCH', 'DELETE'])
    expect(calls[1][1]).toBe(`${root}/users/u1`)
  })
})

describe('mongo adapter', () => {
  /** @type {any[]} */
  const calls = []
  const a = mongoAdapter({
    collections: async () => ['shop.users'],
    describe: async () => ({ count: 1200, indexes: ['_id_', 'plan_1'], size: 4096, capped: false }),
    find: async (c, f, s) => { calls.push(['find', c, f, s]); return { docs: [{ _id: { $oid: 'a' } }], next: 100 } },
    save: async (c, d, p) => { calls.push(['save', c, d, p]); return d },
    remove: async (c, id) => { calls.push(['remove', c, id]) },
  })
  it('summarises and pages by skip', async () => {
    expect((await a.describe('shop.orders')).summary).toEqual(['~1,200 documents', '2 indexes', '4.0 KB'])
    expect(await a.load('shop.users', { cursor: 100, filter: '{a: 1}' })).toEqual({ docs: [{ _id: { $oid: 'a' } }], cursor: 100 })
    expect(calls.at(-1)).toEqual(['find', 'shop.users', '{a: 1}', 100])
  })
  it('ids by _id and passes the old id on save', async () => {
    const prev = { _id: { $oid: 'a' }, n: 1 }
    expect(a.idOf(prev)).toBe('{"$oid":"a"}')
    await a.save('shop.users', { ...prev, n: 2 }, prev)
    expect(calls.at(-1)[3]).toEqual({ $oid: 'a' })
    await a.remove('shop.users', prev)
    expect(calls.at(-1)).toEqual(['remove', 'shop.users', { $oid: 'a' }])
  })
  it('seeds the editor with use + find, quoting odd names', () => {
    expect(a.querySeed('shop.users')).toBe('db.users.find({}).limit(50)')
    expect(a.querySeed('shop.my-coll.v2')).toBe('db.getCollection("my-coll.v2").find({}).limit(50)')
  })
})

describe('uriHost', () => {
  it('drops credentials and options', () => {
    expect(uriHost('mongodb+srv://u:p%40ss@cluster0.x.net/shop?retryWrites=true')).toBe('cluster0.x.net/shop')
    expect(uriHost('mongodb://localhost:27017')).toBe('localhost:27017')
    expect(uriHost('mongodb://a:b@h1:1,h2:2/?replicaSet=rs')).toBe('h1:1,h2:2')
  })
})

describe('docGrid', () => {
  it('types columns and flattens Extended JSON scalars', () => {
    const g = docGrid([
      { _id: { $oid: 'a1' }, n: 1, at: { $date: '2026-01-01T00:00:00Z' }, tags: ['x'] },
      { _id: { $oid: 'a2' }, n: 2.5, extra: { k: 1 }, at: 'not a date' },
    ], ['_id'])
    expect(g.columns.map((c) => `${c.name}:${c.data_type}`)).toEqual(['_id:objectId', 'n:double', 'at:mixed', 'tags:array', 'extra:object'])
    expect(g.rows[0]).toEqual(['a1', 1, '2026-01-01T00:00:00Z', ['x'], null])
    expect(g.rows[1][4]).toEqual({ k: 1 })
  })
})

describe('elasticAdapter', async () => {
  const { elasticAdapter, esQuery, esSort, esDoc, ES_WINDOW } = await import('./doc-adapters.js')

  it('reads the filter as a query string or Query DSL', () => {
    expect(esQuery('')).toEqual({ match_all: {} })
    expect(esQuery('level:error')).toEqual({ query_string: { query: 'level:error' } })
    expect(esQuery('{"term": {"level": "error"}}')).toEqual({ term: { level: 'error' } })
    expect(esQuery('{"query": {"match_all": {}}, "size": 5}')).toEqual({ match_all: {} })
    expect(() => esQuery('{bad')).toThrow(/Query DSL/)
  })

  it('reads sort as field:dir or JSON', () => {
    expect(esSort('')).toBeUndefined()
    expect(esSort('@timestamp:desc, _id')).toEqual([{ '@timestamp': { order: 'desc' } }, '_id'])
    expect(esSort('[{"ms": "asc"}]')).toEqual([{ ms: 'asc' }])
  })

  it('keeps version fields out of the columns', () => {
    const d = esDoc({ _id: '1', _source: { a: 1 }, _seq_no: 4, _primary_term: 1 })
    expect(Object.keys(d)).toEqual(['_id', 'a'])
    expect(/** @type {any} */ (d).__es).toEqual({ seq: 4, term: 1 })
  })

  it('lists indices without system ones and pages with from/size', async () => {
    /** @type {any[]} */
    const calls = []
    const req = vi.fn(async (/** @type {string} */ m, /** @type {string} */ p, /** @type {any} */ b) => {
      calls.push([m, p, b])
      if (p.startsWith('_cat/indices')) return [{ index: '.kibana' }, { index: 'logs' }]
      if (p.endsWith('/_search')) return { hits: { total: { value: 3 }, hits: [{ _id: '1', _source: { x: 1 } }, { _id: '2', _source: { x: 2 } }] } }
      return {}
    })
    const a = elasticAdapter(req, 'opensearch')
    expect(a.engine).toBe('opensearch')
    expect(await a.listCollections()).toEqual(['logs'])
    const page = await a.load('logs', { cursor: null, filter: 'x:1', size: 2, sort: 'x:desc' })
    expect(page.docs.map((d) => d._id)).toEqual(['1', '2'])
    expect(page.cursor).toBe(2)
    expect(calls.at(-1)[2]).toMatchObject({ from: 0, size: 2, query: { query_string: { query: 'x:1' } }, sort: [{ x: { order: 'desc' } }] })
    const last = await a.load('logs', { cursor: ES_WINDOW, filter: '', size: 100 })
    expect(last).toEqual({ docs: [], cursor: null })
  })

  it('saves with the loaded version and moves a renamed id', async () => {
    /** @type {any[]} */
    const calls = []
    const req = vi.fn(async (/** @type {string} */ m, /** @type {string} */ p, /** @type {any} */ b) => {
      calls.push([m, p, b])
      return { _id: 'new', _seq_no: 9, _primary_term: 1 }
    })
    const a = elasticAdapter(req)
    const prev = esDoc({ _id: 'a', _source: { n: 1 }, _seq_no: 3, _primary_term: 1 })
    await a.save('logs', { ...prev, n: 2 }, prev)
    expect(calls[0]).toEqual(['PUT', 'logs/_doc/a?refresh=wait_for&if_seq_no=3&if_primary_term=1', { n: 2 }])
    calls.length = 0
    await a.save('logs', { _id: 'b', n: 2 }, prev)
    expect(calls.map((c) => `${c[0]} ${c[1]}`)).toEqual(['PUT logs/_create/b?refresh=wait_for', 'DELETE logs/_doc/a?refresh=wait_for'])
    calls.length = 0
    await a.save('logs', { _id: '', n: 1 }, null)
    expect(calls[0].slice(0, 2)).toEqual(['POST', 'logs/_doc?refresh=wait_for'])
  })
})

describe('neo4j adapter', () => {
  const api = {
    labels: vi.fn(async () => [['Person', 2], ['My Label', 0]]),
    describe: vi.fn(async () => ({ keys: ['name', 'born'], count: 2 })),
    page: vi.fn(async () => ({ docs: [{ _id: '4:a:1', _labels: ['Person'], name: 'Ada' }], next: null })),
    save: vi.fn(async (_l, doc) => ({ ...doc, _id: doc._id ?? '4:a:9' })),
    remove: vi.fn(async () => {}),
  }
  const a = neo4jAdapter(api)

  it('lists labels and pages nodes by skip', async () => {
    expect(await a.listCollections()).toEqual(['Person', 'My Label'])
    const p = await a.load('Person', { cursor: null, filter: 'n.born > 1900', size: 50 })
    expect(api.page).toHaveBeenCalledWith('Person', 'n.born > 1900', 0, 50)
    expect(p.docs[0].name).toBe('Ada')
    expect(a.idOf(p.docs[0])).toBe('4:a:1')
  })

  it('saves to the node that was opened, and creates without an id', async () => {
    await a.save('Person', { _id: 'edited', name: 'Ada L' }, { _id: '4:a:1', name: 'Ada' })
    expect(api.save).toHaveBeenLastCalledWith('Person', { _id: '4:a:1', name: 'Ada L' })
    await a.save('Person', { name: 'Grace' }, null)
    expect(api.save).toHaveBeenLastCalledWith('Person', { _id: null, name: 'Grace' })
    await a.remove('Person', { _id: '4:a:1' })
    expect(api.remove).toHaveBeenCalledWith('4:a:1')
  })

  it('quotes unusual labels in the seed query', () => {
    expect(a.querySeed('Person')).toBe('MATCH (n:Person) RETURN n LIMIT 100')
    expect(a.querySeed('My Label')).toBe('MATCH (n:`My Label`) RETURN n LIMIT 100')
  })
})

describe('influx adapter', () => {
  const api = {
    measurements: vi.fn(async () => ['cpu', 'mem']),
    describe: vi.fn(async () => ({ tags: ['host'], fields: [{ name: 'usage', type: 'float' }] })),
    page: vi.fn(async () => ({ docs: [{ time: 't', host: 'a', usage: 1 }], next: 1 })),
  }

  it('reads measurements and pages of points, and never writes', async () => {
    const a = influxAdapter(api, '2')
    expect(a.readOnly).toBe(true)
    expect(await a.listCollections()).toEqual(['cpu', 'mem'])
    expect((await a.describe('cpu')).summary).toEqual(['1 tags', '1 fields'])
    const p = await a.load('cpu', { cursor: null, filter: "host = 'a'", size: 1 })
    expect(api.page).toHaveBeenCalledWith('cpu', "host = 'a'", 0, 1)
    expect(p.cursor).toBe(1)
    await expect(a.save('cpu', {}, null)).rejects.toThrow(/read-only/)
    await expect(a.remove('cpu', {})).rejects.toThrow(/read-only/)
  })

  it('seeds SQL or InfluxQL by version', () => {
    expect(influxAdapter(api, '3').querySeed('my"m')).toBe('SELECT * FROM "my""m" ORDER BY time DESC LIMIT 100')
    expect(influxAdapter(api, '1').querySeed('my"m')).toBe('SELECT * FROM "my\\"m" ORDER BY time DESC LIMIT 100')
    expect(influxAdapter(api, '3').consoleLabel).toBe('SQL')
  })
})

describe('docGrid time columns', () => {
  it("types an engine's named time column as a timestamp, and only that", async () => {
    const { docGrid } = await import('./doc-adapters.js')
    const docs = [{ time: '2026-11-21T11:04:44Z', at: '2026-11-21T11:04:44Z', v: 1 }]
    const g = docGrid(docs, [], ['time'])
    expect(g.columns.map((c) => c.data_type)).toEqual(['timestamp', 'string', 'int'])
  })
})

describe('convex adapter', () => {
  /** A fake deployment: records system calls and answers per function name. */
  function cx(/** @type {Record<string, any>} */ answers = {}) {
    /** @type {[string, any][]} */
    const calls = []
    const call = async (/** @type {string} */ path, /** @type {any} */ args) => {
      calls.push([path, args])
      const a = answers[path.split('/').pop() ?? '']
      if (a instanceof Error) throw a
      return typeof a === 'function' ? a(args) : a ?? { success: true }
    }
    return { a: convexAdapter(call), calls }
  }

  it('filter text becomes the dashboard filter expression', () => {
    expect(cxFilter('  ')).toBeNull()
    const decoded = JSON.parse(atob(/** @type {string} */ (cxFilter('age > 30 AND plan == "pro" AND name != Ada'))))
    expect(decoded.order).toBe('desc')
    expect(decoded.clauses).toEqual([
      { id: '0', field: 'age', op: 'gt', value: 30, enabled: true },
      { id: '1', field: 'plan', op: 'eq', value: 'pro', enabled: true },
      { id: '2', field: 'name', op: 'neq', value: 'Ada', enabled: true },
    ])
    // Non-ASCII survives the base64 step.
    expect(JSON.parse(new TextDecoder().decode(Uint8Array.from(atob(/** @type {string} */ (cxFilter('name == "Zoë"'))), (c) => c.charCodeAt(0)))).clauses[0].value).toBe('Zoë')
    expect(() => cxFilter('age ~ 3')).toThrow(/field op value/)
  })

  it('lists user tables first, then the shown system tables', async () => {
    const { a } = cx({ getTableMapping: { 1: 'users', 2: '_scheduled_jobs', 3: '_secret', 4: 'messages', 5: '_file_storage' } })
    expect(await a.listCollections()).toEqual(['messages', 'users', '_file_storage', '_scheduled_jobs'])
  })

  it('pages with the cursor and stops when Convex says done', async () => {
    const { a, calls } = cx({ paginatedTableDocuments: (/** @type {any} */ args) => (args.paginationOpts.cursor ? { page: [{ _id: 'b' }], isDone: true, continueCursor: 'end' } : { page: [{ _id: 'a' }], isDone: false, continueCursor: 'c1' }) })
    const first = await a.load('users', { cursor: null, filter: '', size: 1 })
    expect(first).toEqual({ docs: [{ _id: 'a' }], cursor: 'c1' })
    expect(calls[0][1]).toEqual({ table: 'users', filters: null, paginationOpts: { numItems: 1, cursor: null } })
    const second = await a.load('users', { cursor: first.cursor, filter: 'n > 1', size: 1 })
    expect(second.cursor).toBeNull()
    expect(typeof calls[1][1].filters).toBe('string')
  })

  it('turns a rejected filter into an error', async () => {
    const { a } = cx({ paginatedTableDocuments: { page: [{ error: 'Invalid field', filter: {} }], isDone: true, continueCursor: '' } })
    await expect(a.load('users', { cursor: null, filter: 'x == 1', size: 10 })).rejects.toThrow('Invalid field')
  })

  it('counts only without a filter', async () => {
    const { a } = cx({ tableSize: 42 })
    expect(await a.count?.('users', '')).toBe(42)
    expect(await a.count?.('users', 'a == 1')).toBeNull()
    expect((await a.describe('users')).summary).toEqual(['42 documents'])
  })

  it('saves: replace without system fields, insert then read back the newest', async () => {
    const { a, calls } = cx({ getById: { _id: 'j1', n: 2 }, paginatedTableDocuments: { page: [{ _id: 'j9', n: 1 }], isDone: false } })
    const prev = { _id: 'j1', _creationTime: 1, n: 1 }
    expect(await a.save('users', { ...prev, n: 2 }, prev)).toEqual({ _id: 'j1', n: 2 })
    expect(calls[0]).toEqual(['_system/frontend/replaceDocument', { id: 'j1', document: { n: 2 } }])
    await expect(a.save('users', { ...prev, _id: 'j2' }, prev)).rejects.toThrow(/assigns document ids/)
    calls.length = 0
    expect(await a.save('users', { n: 1 }, null)).toEqual({ _id: 'j9', n: 1 })
    expect(calls[0]).toEqual(['_system/frontend/addDocument', { table: 'users', documents: [{ n: 1 }] }])
  })

  it('patches one field by id, int64 wrapped and unset as the placeholder', async () => {
    const { a, calls } = cx({ getById: { _id: 'j1' } })
    await a.updateField?.('users', { _id: 'j1' }, 'big', '9007199254740993', { typeHint: 'int64' })
    expect(calls[0]).toEqual(['_system/frontend/patchDocumentsFields', { table: 'users', ids: ['j1'], fields: { big: { $integer: '9007199254740993' } } }])
    await a.updateField?.('users', { _id: 'j1' }, 'gone', null, { unset: true })
    expect(calls[2][1].fields).toEqual({ gone: CX_UNDEFINED })
    await a.remove('users', { _id: 'j1' })
    expect(calls.at(-1)).toEqual(['_system/frontend/deleteDocuments', { toDelete: [{ id: 'j1', tableName: 'users' }] }])
  })

  it('shows int64 as its decimal value', () => {
    const g = docGrid([{ _id: 'a', big: { $integer: '7' } }], ['_id'])
    expect(g.rows[0][1]).toBe('7')
    expect(g.columns[1].data_type).toBe('int64')
  })
})
