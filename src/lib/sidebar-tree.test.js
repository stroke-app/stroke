import { describe, it, expect } from 'vitest'
import {
  SEP, childId, parentIdOf, ancestorIds, makeNode, filterTree, flattenTree,
  rowIndex, treeKeyAction, typeAheadIndex, stickyIndices, branchRange,
  tableSections, keyNodes,
} from './sidebar-tree.js'

/** @typedef {import('./sidebar-tree.js').TreeNode} TreeNode */
/** @typedef {import('./sidebar-tree.js').ChildEntry} ChildEntry */

/**
 * conn
 *   main (schema)
 *     tables (2)
 *       sqlite_sequence
 *       superheroes
 *         id, name
 *     views (0)
 */
function fixture() {
  const conn = makeNode(childId('', 'c', 'local'), 'connection', 'SQLite', { branch: true })
  const main = makeNode(childId(conn.id, 's', 'main'), 'schema', 'main', { branch: true })
  const tables = makeNode(childId(main.id, 'g', 'tables'), 'group', 'tables', { branch: true, count: 2 })
  const views = makeNode(childId(main.id, 'g', 'views'), 'group', 'views', { branch: true, count: 0 })
  const seq = makeNode(childId(tables.id, 't', 'sqlite_sequence'), 'table', 'sqlite_sequence', { branch: true })
  const heroes = makeNode(childId(tables.id, 't', 'superheroes'), 'table', 'superheroes', { branch: true })
  const id = makeNode(childId(heroes.id, 'col', 'id'), 'column', 'id')
  const name = makeNode(childId(heroes.id, 'col', 'name'), 'column', 'name')
  /** @type {Map<string, ChildEntry>} */
  const cache = new Map([
    [conn.id, { status: 'ready', nodes: [main] }],
    [main.id, { status: 'ready', nodes: [tables, views] }],
    [tables.id, { status: 'ready', nodes: [seq, heroes] }],
    [views.id, { status: 'ready', nodes: [] }],
    [heroes.id, { status: 'ready', nodes: [id, name] }],
  ])
  return { conn, main, tables, views, seq, heroes, id, name, cache, get: (/** @type {string} */ k) => cache.get(k) }
}

const labels = (/** @type {import('./sidebar-tree.js').FlatRow[]} */ rows) =>
  rows.map((r) => (r.t === 'node' ? `${'  '.repeat(r.depth)}${r.node.label}` : `${'  '.repeat(r.depth)}<${r.t}>`))

describe('ids are paths', () => {
  it('builds, splits and walks them', () => {
    const id = childId(childId(childId('', 'c', 'x'), 's', 'public'), 't', 'users')
    expect(id).toBe(`c:x${SEP}s:public${SEP}t:users`)
    expect(parentIdOf(id)).toBe(`c:x${SEP}s:public`)
    expect(parentIdOf('c:x')).toBe('')
    expect(ancestorIds(id)).toEqual(['c:x', `c:x${SEP}s:public`])
  })
})

describe('flattenTree', () => {
  it('draws only what is open, with depth, parent and aria positions', () => {
    const f = fixture()
    const open = new Set([f.conn.id, f.main.id, f.tables.id])
    const { rows, missing } = flattenTree([f.conn], f.get, (id) => open.has(id))
    expect(labels(rows)).toEqual(['SQLite', '  main', '    tables', '      sqlite_sequence', '      superheroes', '    views'])
    expect(missing).toEqual([])
    const heroes = rows[4]
    expect(heroes.t === 'node' && [heroes.depth, heroes.parent, heroes.pos, heroes.size]).toEqual([3, 2, 2, 2])
  })

  it('draws a loading row for an open branch with nothing cached, and reports it', () => {
    const f = fixture()
    f.cache.delete(f.main.id)
    const open = new Set([f.conn.id, f.main.id])
    const { rows, missing } = flattenTree([f.conn], f.get, (id) => open.has(id))
    expect(labels(rows)).toEqual(['SQLite', '  main', '    <loading>'])
    expect(missing).toEqual([f.main.id])
  })

  it('draws an error row that names its owner, and an empty row for an empty branch', () => {
    const f = fixture()
    f.cache.set(f.heroes.id, { status: 'error', nodes: [], error: 'boom' })
    const open = new Set([f.conn.id, f.main.id, f.tables.id, f.heroes.id, f.views.id])
    const { rows } = flattenTree([f.conn], f.get, (id) => open.has(id))
    const err = rows.find((r) => r.t === 'error')
    expect(err && err.t === 'error' && [err.owner, err.error]).toEqual([f.heroes.id, 'boom'])
    expect(rows.at(-1)?.t).toBe('empty')
  })

  it('keeps the old children up while a refresh is loading', () => {
    const f = fixture()
    f.cache.set(f.tables.id, { status: 'loading', nodes: [f.seq, f.heroes] })
    const open = new Set([f.conn.id, f.main.id, f.tables.id])
    const { rows } = flattenTree([f.conn], f.get, (id) => open.has(id))
    expect(labels(rows)).toContain('      superheroes')
    const tablesRow = rows[2]
    expect(tablesRow.t === 'node' && tablesRow.loading).toBe(true)
  })
})

describe('filterTree', () => {
  it('keeps matches and their ancestors, opens the ancestors, ignores the expanded set', () => {
    const f = fixture()
    const filter = filterTree([f.conn], f.get, 'nam')
    expect(filter.matches).toBe(1)
    const { rows } = flattenTree([f.conn], f.get, () => false, filter)
    expect(labels(rows)).toEqual(['SQLite', '  main', '    tables', '      superheroes', '        name'])
    // Positions count the visible siblings only.
    const name = rows.at(-1)
    expect(name?.t === 'node' && [name.pos, name.size]).toEqual([1, 1])
  })

  it('never searches children that are not loaded', () => {
    const f = fixture()
    f.cache.delete(f.heroes.id)
    const filter = filterTree([f.conn], f.get, 'nam')
    expect(filter.matches).toBe(0)
    expect(flattenTree([f.conn], f.get, () => true, filter).rows).toEqual([])
  })
})

describe('treeKeyAction', () => {
  const f = fixture()
  const open = new Set([f.conn.id, f.main.id, f.tables.id])
  const { rows } = flattenTree([f.conn], f.get, (id) => open.has(id))
  // 0 SQLite, 1 main, 2 tables, 3 sqlite_sequence, 4 superheroes, 5 views

  it('moves up, down, home, end and by page, clamped', () => {
    expect(treeKeyAction(rows, 1, 'ArrowDown')).toEqual({ type: 'move', index: 2 })
    expect(treeKeyAction(rows, 1, 'ArrowUp')).toEqual({ type: 'move', index: 0 })
    expect(treeKeyAction(rows, 0, 'ArrowUp')).toEqual({ type: 'none' })
    expect(treeKeyAction(rows, 5, 'ArrowDown')).toEqual({ type: 'none' })
    expect(treeKeyAction(rows, 3, 'Home')).toEqual({ type: 'move', index: 0 })
    expect(treeKeyAction(rows, 0, 'End')).toEqual({ type: 'move', index: 5 })
    expect(treeKeyAction(rows, 0, 'PageDown', 4)).toEqual({ type: 'move', index: 4 })
    expect(treeKeyAction(rows, 0, 'PageDown', 40)).toEqual({ type: 'move', index: 5 })
    expect(treeKeyAction(rows, -1, 'ArrowDown')).toEqual({ type: 'move', index: 0 })
  })

  it('→ opens a closed branch, then steps into it; ← closes, then climbs', () => {
    expect(treeKeyAction(rows, 4, 'ArrowRight')).toEqual({ type: 'expand', id: f.heroes.id })
    expect(treeKeyAction(rows, 2, 'ArrowRight')).toEqual({ type: 'move', index: 3 })
    expect(treeKeyAction(rows, 2, 'ArrowLeft')).toEqual({ type: 'collapse', id: f.tables.id })
    expect(treeKeyAction(rows, 4, 'ArrowLeft')).toEqual({ type: 'move', index: 2 })
    expect(treeKeyAction(rows, 0, 'ArrowLeft')).toEqual({ type: 'collapse', id: f.conn.id })
  })

  it('Enter activates, Space toggles, Enter on an error retries', () => {
    expect(treeKeyAction(rows, 4, 'Enter')).toEqual({ type: 'activate', index: 4 })
    expect(treeKeyAction(rows, 5, ' ')).toEqual({ type: 'toggle', id: f.views.id })
    const err = [...rows, { t: /** @type {const} */ ('error'), key: 'e', owner: f.views.id, depth: 3, parent: 5, error: 'x' }]
    expect(treeKeyAction(err, 6, 'Enter')).toEqual({ type: 'retry', id: f.views.id })
    expect(treeKeyAction(err, 6, 'ArrowLeft')).toEqual({ type: 'move', index: 5 })
  })
})

describe('typeAheadIndex', () => {
  const f = fixture()
  const open = new Set([f.conn.id, f.main.id, f.tables.id])
  const { rows } = flattenTree([f.conn], f.get, (id) => open.has(id))

  it('jumps to the next label starting with the letter, wrapping', () => {
    expect(typeAheadIndex(rows, 0, 's')).toBe(3)
    expect(typeAheadIndex(rows, 3, 's')).toBe(4)
    expect(typeAheadIndex(rows, 4, 's')).toBe(0) // SQLite, wrapped
  })

  it('stays put while a longer prefix still matches', () => {
    expect(typeAheadIndex(rows, 4, 'su')).toBe(4)
    expect(typeAheadIndex(rows, 4, 'zz')).toBe(-1)
  })
})

describe('stickyIndices and branchRange', () => {
  const f = fixture()
  const open = new Set([f.conn.id, f.main.id, f.tables.id, f.heroes.id])
  const { rows } = flattenTree([f.conn], f.get, (id) => open.has(id))
  // 0 SQLite, 1 main, 2 tables, 3 seq, 4 heroes, 5 id, 6 name, 7 views

  it('pins the ancestors of the row under the stack', () => {
    expect(stickyIndices(rows, 0, 4)).toEqual([])
    // Top row is sqlite_sequence. Pinning SQLite, main and tables covers rows
    // 3-5, so the row under the stack is `name`, whose parent superheroes (4)
    // is now hidden under the stack too - it is pinned fourth.
    expect(stickyIndices(rows, 3, 4)).toEqual([0, 1, 2, 4])
    // Scrolled so `views` (7) is at the top: it is a sibling of tables, so
    // only SQLite and main pin.
    expect(stickyIndices(rows, 7, 4)).toEqual([0, 1])
    expect(stickyIndices(rows, 3, 2)).toEqual([0, 1])
  })

  it('finds the run of rows under the active row’s parent', () => {
    expect(branchRange(rows, 5)).toEqual({ level: 3, lo: 5, hi: 6 })
    expect(branchRange(rows, 3)).toEqual({ level: 2, lo: 3, hi: 6 })
    expect(branchRange(rows, 0)).toBe(null)
  })
})

describe('scale (bench)', () => {
  it('flattens and filters 20,000 nodes fast, and the window draws a handful', () => {
    const conn = makeNode('c:big', 'connection', 'big', { branch: true })
    const schema = makeNode(childId(conn.id, 's', 'public'), 'schema', 'public', { branch: true })
    const group = makeNode(childId(schema.id, 'g', 'tables'), 'group', 'tables', { branch: true })
    /** @type {TreeNode[]} */
    const tables = []
    for (let i = 0; i < 20_000; i++) {
      const name = `table_${String(i).padStart(5, '0')}`
      tables.push(makeNode(childId(group.id, 't', name), 'table', name, { branch: true, count: i }))
    }
    /** @type {Map<string, ChildEntry>} */
    const cache = new Map([
      [conn.id, { status: 'ready', nodes: [schema] }],
      [schema.id, { status: 'ready', nodes: [group] }],
      [group.id, { status: 'ready', nodes: tables }],
    ])
    const open = new Set([conn.id, schema.id, group.id])
    const get = (/** @type {string} */ k) => cache.get(k)

    // Warm the JIT the way a session would, then time a steady-state pass.
    for (let i = 0; i < 3; i++) flattenTree([conn], get, (id) => open.has(id))
    let t0 = performance.now()
    const { rows } = flattenTree([conn], get, (id) => open.has(id))
    const flattenMs = performance.now() - t0

    t0 = performance.now()
    const filter = filterTree([conn], get, '1234')
    const filtered = flattenTree([conn], get, (id) => open.has(id), filter)
    const filterMs = performance.now() - t0

    // What the component's window draws for a 600px sidebar at 28px rows with
    // an overscan of 10 either side - the DOM row count is this, not 20,003.
    const ROW = 28, VIEW = 600, OVERSCAN = 10, scrollTop = 250_000
    const start = Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN)
    const end = Math.min(rows.length, Math.ceil((scrollTop + VIEW) / ROW) + OVERSCAN)
    const domRows = end - start

    console.info(`[sidebar-tree bench] 20,003 rows: flatten ${flattenMs.toFixed(2)}ms, filter+flatten ${filterMs.toFixed(2)}ms (${filtered.rows.length} rows), window draws ${domRows} DOM rows`)
    expect(rows.length).toBe(20_003)
    // table_01234, table_11234 and table_12340..12349, under their 3 ancestors.
    expect(filtered.rows.length).toBe(3 + 12)
    expect(domRows).toBeLessThan(60)
    // Generous ceilings: CI machines are slow; the point is "milliseconds, not frames".
    expect(flattenMs).toBeLessThan(50)
    expect(filterMs).toBeLessThan(80)
  })
})

import { stickyTops, subtreeEnd } from './sidebar-tree.js'

describe('pinned rows slide out', () => {
  // 0 a ┐ 1 a1, 2 a2   3 b ┐ 4 b1
  const rows = /** @type {any[]} */ ([
    { depth: 0, parent: -1 }, { depth: 1, parent: 0 }, { depth: 1, parent: 0 },
    { depth: 0, parent: -1 }, { depth: 1, parent: 3 },
  ])
  it('finds where a branch ends', () => {
    expect(subtreeEnd(rows, 0)).toBe(2)
    expect(subtreeEnd(rows, 3)).toBe(4)
  })
  it('stays put until the next branch reaches it, then moves up with it', () => {
    expect(stickyTops(rows, [0], 28, 28)).toEqual([0])
    expect(stickyTops(rows, [0], 70, 28)).toEqual([-14])
  })
})

describe('tableSections', () => {
  const at = { schema: 'public', table: 'members' }
  const id = childId(childId('', 'c', 'x'), 't', 'members')
  /** @param {string} name @param {string} type @param {any} [extra] */
  const col = (name, type, extra = {}) => ({ name, dataType: type, isNullable: true, isPrimaryKey: false, foreignKey: null, fkConstraintName: null, ...extra })
  const cols = [
    col('id', 'int8', { isPrimaryKey: true, isNullable: false }),
    col('user_id', 'int4', { foreignKey: 'public.users.id', fkConstraintName: 'members_user_id_fkey' }),
    col('org_a', 'int4', { foreignKey: 'public.orgs.a', fkConstraintName: 'members_org_fk' }),
    col('org_b', 'int4', { foreignKey: 'public.orgs.b', fkConstraintName: 'members_org_fk' }),
    col('note', 'varchar(255)'),
  ]
  const idx = [
    { name: 'members_pkey', tableName: 'members', columns: 'id', indexType: 'btree', isUnique: true, isPrimary: true },
    { name: 'members_user_idx', tableName: 'members', columns: 'user_id', indexType: 'btree', isUnique: false, isPrimary: false },
    { name: 'members_note_key', tableName: 'members', columns: 'note', indexType: 'btree', isUnique: true, isPrimary: false, condition: 'note IS NOT NULL' },
  ]

  it('splits a table into columns, keys and indexes, each with its count', () => {
    const { nodes, sections } = tableSections(id, at, cols, idx, true)
    expect(nodes.map((n) => [n.kind, n.label, n.count])).toEqual([['section', 'columns', 5], ['section', 'keys', 3], ['section', 'indexes', 3]])
    expect(nodes.every((n) => n.branch)).toBe(true)
    const [colsOf, keysOf, idxOf] = sections.map(([, list]) => list)
    expect(sections.map(([gid]) => gid)).toEqual(nodes.map((n) => n.id))
    expect(colsOf.map((n) => `${n.label} ${n.meta}`)).toEqual(['id int8', 'user_id int4', 'org_a int4', 'org_b int4', 'note varchar(255)'])
    expect(colsOf[0].data.pk).toBe(true)
    expect(colsOf[1].data.fk).toBe('public.users.id')
    // One row per key: the composite foreign key is one row, its targets in order.
    expect(keysOf.map((n) => `${n.label} ${n.meta}`)).toEqual([
      'members_pkey (id)',
      'members_user_id_fkey (user_id) → users(id)',
      'members_org_fk (org_a, org_b) → orgs(a, b)',
    ])
    expect(idxOf.map((n) => `${n.label} ${n.meta}`)).toEqual(['members_pkey (id)', 'members_user_idx (user_id)', 'members_note_key unique (note)'])
    expect(idxOf[2].data.condition).toBe('note IS NOT NULL')
    // Ids nest under the table, so the expanded set and the flatten find them.
    expect(keysOf.every((n) => n.id.startsWith(nodes[1].id + SEP))).toBe(true)
  })

  it('leaves out a section with nothing in it', () => {
    // No keys, no indexes (a ClickHouse Log table, a heap with no constraints).
    const bare = tableSections(id, at, [col('a', 'String'), col('b', 'Nullable(String)')], [], true)
    expect(bare.nodes.map((n) => n.label)).toEqual(['columns'])
    expect(bare.sections).toHaveLength(1)
    // A MongoDB collection: no columns, only indexes.
    const mongo = tableSections(id, at, [], [{ name: '_id_', tableName: 'members', columns: '_id', indexType: 'btree', isUnique: true, isPrimary: true }], true)
    expect(mongo.nodes.map((n) => `${n.label} ${n.count}`)).toEqual(['indexes 1'])
    // Nothing at all: no sections, so the table draws its Empty row.
    expect(tableSections(id, at, [], [], true).nodes).toEqual([])
  })

  it('opens a view straight into its columns', () => {
    const { nodes, sections } = tableSections(id, at, cols.slice(0, 2), [], false)
    expect(nodes.map((n) => n.kind)).toEqual(['column', 'column'])
    expect(nodes[0].id.startsWith(id + SEP + 'col:')).toBe(true)
    expect(sections).toEqual([])
  })

  it('takes declared key rows as the keys and keeps them out of indexes', () => {
    // Cassandra: partition and clustering keys come from the backend as `key` rows.
    const ccols = [col('tenant', 'uuid', { isPrimaryKey: true }), col('at', 'timestamp', { isPrimaryKey: true }), col('payload', 'text')]
    const cidx = [
      { name: 'partition key', tableName: 'events', columns: 'tenant', indexType: 'key', isPrimary: true },
      { name: 'clustering key', tableName: 'events', columns: 'at desc', indexType: 'key' },
      { name: 'events_payload_idx', tableName: 'events', columns: 'payload', indexType: 'composites' },
    ]
    const { nodes, sections } = tableSections(id, at, ccols, cidx, true)
    expect(nodes.map((n) => `${n.label} ${n.count}`)).toEqual(['columns 3', 'keys 2', 'indexes 1'])
    expect(sections[1][1].map((n) => `${n.label} ${n.meta}`)).toEqual(['partition key (tenant)', 'clustering key (at desc)'])
  })

  it('names a primary key by its index, in key order, but not after a SQLite autoindex', () => {
    const pk = [col('b', 'int', { isPrimaryKey: true }), col('a', 'int', { isPrimaryKey: true })]
    const gid = childId(id, 'g', 'keys')
    expect(keyNodes(gid, pk, [{ name: 'orgs_pkey', columns: 'a, b', isPrimary: true }], at).map((n) => `${n.label} ${n.meta}`)).toEqual(['orgs_pkey (a, b)'])
    expect(keyNodes(gid, pk, [{ name: 'sqlite_autoindex_orgs_1', columns: 'a, b', isPrimary: true }], at).map((n) => n.label)).toEqual(['primary key'])
    // A rowid table has no index behind its key: the columns stand in.
    expect(keyNodes(gid, pk, [], at).map((n) => `${n.label} ${n.meta}`)).toEqual(['primary key (b, a)'])
  })

  it('flattens an open table with its sections', () => {
    const { nodes, sections } = tableSections(id, at, cols, idx, true)
    const table = makeNode(id, 'table', 'members', { branch: true, count: 12300 })
    const cache = new Map([[id, { status: /** @type {const} */ ('ready'), nodes }], ...sections.map(([gid, list]) => [gid, { status: /** @type {const} */ ('ready'), nodes: list }])])
    const open = new Set([id, nodes[1].id])
    const { rows } = flattenTree([table], (k) => /** @type {any} */ (cache.get(k)), (k) => open.has(k))
    expect(labels(rows)).toEqual(['members', '  columns', '  keys', '    members_pkey', '    members_user_id_fkey', '    members_org_fk', '  indexes'])
  })
})
