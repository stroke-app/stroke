<script module>
  /**
   * Child listings that outlive the panel (switching sidebar tabs, toggling
   * the sidebar), keyed by connection + database so a reconnect draws at once.
   * @type {Map<string, { schemaData: Map<string, any>, cache: Map<string, import('$lib/sidebar-tree.js').ChildEntry> }>}
   */
  const places = new Map()

  /** NoSQL engines whose collections expand: MongoDB and CouchDB into indexes, Cassandra and DynamoDB into typed key columns, keys and indexes. Firestore and Redis stay flat. */
  const SHAPED_NOSQL = new Set(['mongodb', 'couchdb', 'cassandra', 'dynamodb'])
  const PER_TABLE_INDEXES = new Set(['mongodb', 'couchdb', 'dynamodb'])
</script>

<script>
  /**
   * The Tree sidebar (beta): one explorer tree in place of the Tables, Objects
   * and Databases tabs -
   *
   *   connection > database > schema > tables / views / functions / ... > table > columns
   *
   * The model is pure and tested (sidebar-tree.js): nodes are named by path, the
   * visible tree is one flat array derived from the roots, a children cache and
   * an expanded set, and this component only draws a window of it. Rows are
   * absolutely placed at index x row height with an explicit pixel height, so
   * the painted row and the virtual stride are the same number - the mismatch
   * between the two is what broke the old windowed sidebar. Focus stays on the
   * tree and `aria-activedescendant` names the active row, so rows coming and
   * going under the window never drop it.
   *
   * Children load on expand (one loading row, one error row with Retry), are
   * cached per connection + database, and refresh when the objects version is
   * bumped (Refresh, any CREATE / DROP), when the database list is invalidated,
   * and - for the open schema - whenever the shell re-lists its tables.
   */
  import { onMount, tick, untrack } from 'svelte'
  import Icon from './Icon.svelte'
  import DbIcon from './DbIcon.svelte'
  import * as ContextMenu from '$lib/components/ui/context-menu/index.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { listTables, listDbObjects, getTableColumnStructure, getSchemaColumnStructure, listIndexes } from '$lib/api.js'
  import { listDatabases, canSwitchDatabase, currentDatabaseKey, isCurrentDatabase } from '$lib/databases.js'
  import { isSystemSchema } from '$lib/system-schemas.js'
  import { formatTableRowCount, normalizeTableRowCount } from '$lib/table-list.js'
  import { OBJECT_KIND_META, objectFamily, FAMILY_KINDS } from '$lib/object-templates.js'
  import { objectsVersion, openDefinition, openTemplate } from '$lib/stores/sidebar-objects.svelte.js'
  import { readOnlyMode, guardWrite, READ_ONLY_HINT } from '$lib/stores/read-only.js'
  import { setConnectionMeta } from '$lib/stores/connections.js'
  import { dbAdminKind } from '$lib/database-admin.js'
  import { appNativeScroll } from '$lib/stores/settings.js'
  import { workspaces, activeWorkspaceId, inWorkspace, colorOf } from '$lib/stores/workspaces.js'
  import { loadTreeExpanded, saveTreeExpanded } from '$lib/stores/sidebar-tree.js'
  import { smoothScroll } from '$lib/smooth-scroll.js'
  import {
    childId, ancestorIds, makeNode, filterTree, flattenTree, rowIndex,
    treeKeyAction, typeAheadIndex, stickyIndices, stickyTops, branchRange, tableSections,
  } from '$lib/sidebar-tree.js'
  import { cn } from '$lib/utils.js'

  /** @typedef {import('$lib/sidebar-tree.js').TreeNode} TreeNode */
  /** @typedef {import('$lib/sidebar-tree.js').ChildEntry} ChildEntry */
  /** @typedef {import('$lib/sidebar-tree.js').FlatRow} FlatRow */
  /** @typedef {import('$lib/stores/connections.js').SavedConnection} SavedConnection */
  /** @typedef {{ tables: any[] | null, objects: import('$lib/api.js').ObjectListing | null, objectsLoading?: boolean }} SchemaData */

  let {
    /** The live connection. @type {SavedConnection | null} */
    connection = null,
    /** Saved connections: every one is a root; only the live one opens. @type {SavedConnection[]} */
    connections = [],
    /** @type {string[]} */
    schemas = [],
    /** The open schema's table list, from the shell (counts included). @type {any[]} */
    tables = [],
    activeSchema = '',
    /** @type {string | null} */
    activeTable = null,
    loadingTables = false,
    /** False for NoSQL engines: no columns, no routines. */
    sqlTables = true,
    /** The sidebar's filter box, already debounced. */
    filter = '',
    /** Bumped by the shell after a database is created, renamed or dropped. */
    databasesRefreshKey = 0,
    /** The classic sidebar's table menu, for tables in the open schema. @type {import('svelte').Snippet<[string]> | undefined} */
    tableMenu = undefined,
    /** Open a table or view in the open schema. @type {(name: string) => void} */
    ontableselect = () => {},
    /** Open a table or view in any schema. @type {(schema: string, name: string) => void} */
    onopentable = () => {},
    /** @type {(schema: string) => void} */
    onschemachange = () => {},
    /** @type {(c: SavedConnection) => void} */
    onswitchconnection = () => {},
    /** Clear the sidebar's filter (a click on a match opens it in the full tree). */
    onclearfilter = () => {},
    /** @type {(entry: { key: string, label: string }) => void} */
    onswitchdatabase = () => {},
    /** @type {(sql: import('$lib/stores/sidebar-objects.svelte.js').ObjectSql) => void} */
    onopenobjectsql = () => {},
    /** ArrowUp off the first row: back to the filter box. */
    onexittop = () => {},
    /** The shell's create flows: CreateTableDialog, CreateSchemaDialog, CreateDatabaseDialog. */
    onnewtable = () => {},
    onnewschema = () => {},
    onnewdatabase = () => {},
    /** A connection's pin changed; carries the saved list. @type {(list: SavedConnection[]) => void} */
    onconnectionmeta = () => {},
  } = $props()

  const uid = $props.id()

  // ── Where we are ────────────────────────────────────────────────────────
  const connKey = $derived(connection?.id ?? (connection ? 'live' : ''))
  const canSwitchDb = $derived(canSwitchDatabase(connection))
  const activeDbKey = $derived(currentDatabaseKey(connection) ?? '')
  // Keyed by the database's key, not its name: a D1 connection may carry no
  // name at all, and two of its databases then shared one cache.
  const placeKey = $derived(`${connKey}\u0000${activeDbKey || connection?.database || ''}`)
  const activeRootId = $derived(connKey ? childId('', 'c', connKey) : '')
  /** The current database's human name: D1 keys by uuid, so the key is never shown. */
  const activeDbLabel = $derived(connection?.database || (connection?.type === 'd1' ? connection?.name : '') || activeDbKey)
  /** Where schemas hang: the current database's node, or the root itself. */
  const schemaParentId = $derived(canSwitchDb && activeRootId ? childId(activeRootId, 'd', activeDbKey) : activeRootId)
  const family = $derived(sqlTables ? objectFamily(connection?.type) : null)
  const engine = $derived(String(connection?.type ?? ''))
  /** Collections open into keys and indexes (and columns, where typed): every SQL engine, and the NoSQL ones that keep them cheaply. */
  const shaped = $derived(sqlTables || SHAPED_NOSQL.has(engine))
  /** Engines whose indexes are listed per collection, never per schema. */
  const perTableIndexes = $derived(!sqlTables && PER_TABLE_INDEXES.has(engine))
  /** Routine / trigger / ... kinds this engine keeps (views come from the table list). */
  const objectKinds = $derived(family ? FAMILY_KINDS[family].filter((/** @type {string} */ k) => k !== 'view' && k !== 'matview') : [])
  /** Every kind this engine has a CREATE template for, views included. @type {import('$lib/object-templates.js').ObjectKind[]} */
  const templateKinds = $derived(family ? FAMILY_KINDS[family] : [])
  /** Same rules as the classic sidebar's + menu. */
  const supportsSchemas = $derived(connection?.type === 'postgres')
  const dbAdmin = $derived(dbAdminKind(connection))

  const userSchemas = $derived(schemas.filter((s) => !isSystemSchema(s)))
  const systemSchemas = $derived(schemas.filter((s) => isSystemSchema(s)))
  /** System schemas go in their own folder only when there is something to set them apart from. */
  const foldSystem = $derived(systemSchemas.length > 0 && userSchemas.length > 0)

  /** The node that holds a schema's groups. With no schemas at all, the groups hang off the parent. @param {string} schema */
  function schemaNodeId(schema) {
    if (!schemas.length) return schemaParentId
    if (foldSystem && isSystemSchema(schema)) return childId(childId(schemaParentId, 'f', 'system'), 's', schema)
    return childId(schemaParentId, 's', schema)
  }

  // ── Cache ───────────────────────────────────────────────────────────────
  // Plain maps plus one counter: the flatten depends on the counter, and a
  // write is a set plus a bump - no per-node stores, no deep proxies over
  // twenty thousand nodes.
  /** Per-schema table lists (other than the open one) and object listings. @type {Map<string, SchemaData>} */
  let schemaData = new Map()
  /** Async child listings: the database list, columns, and schema load states. @type {Map<string, ChildEntry>} */
  let cache = new Map()
  let tick$ = $state(0)
  /**
   * Bumped only when the schema listings change, so `live` (every loaded
   * schema's groups, thousands of nodes on a big one) is not rebuilt for a
   * table's columns arriving - only the flatten is.
   */
  let liveTick = $state(0)
  function bumpSchemas() { liveTick++; tick$++ }
  /** Bumped when the place changes: replies from the old place are dropped. */
  let epoch = 0

  /** @param {string} id @param {ChildEntry} entry */
  function setEntry(id, entry) { cache.set(id, entry); tick$++ }

  $effect(() => {
    const key = placeKey
    untrack(() => {
      epoch++
      let place = places.get(key)
      if (!place) {
        place = { schemaData: new Map(), cache: new Map() }
        places.set(key, place)
      }
      schemaData = place.schemaData
      cache = place.cache
      indexLists = new Map()
      tableIndexes = new Map()
      columnLists = new Map()
      tableColumns = new Map()
      // Anything left loading by a reply that will now be dropped reads again.
      for (const [id, e] of cache) if (e.status === 'loading') cache.delete(id)
      for (const d of schemaData.values()) d.objectsLoading = false
      shapes = new Map()
      bumpSchemas()
    })
  })

  // ── Expanded set, persisted per connection ──────────────────────────────
  let expanded = $state.raw(/** @type {Set<string>} */ (new Set()))
  let restoredFor = ''
  $effect(() => {
    const root = activeRootId
    const conn = connKey
    if (!root) return
    untrack(() => {
      if (restoredFor === conn) return
      restoredFor = conn
      const saved = loadTreeExpanded(conn)
      const next = new Set(expanded)
      if (saved) for (const id of saved) next.add(id)
      else {
        // First visit: open down to the open schema's tables.
        next.add(root)
        if (schemaParentId !== root) next.add(schemaParentId)
        if (activeSchema) {
          const s = schemaNodeId(activeSchema)
          for (const a of ancestorIds(s)) next.add(a)
          next.add(s)
          next.add(childId(s, 'g', 'tables'))
        }
      }
      expanded = next
    })
  })

  let saveTimer = /** @type {ReturnType<typeof setTimeout> | null} */ (null)
  function persistExpanded() {
    if (saveTimer) clearTimeout(saveTimer)
    const conn = connKey
    const root = activeRootId
    saveTimer = setTimeout(() => {
      saveTimer = null
      if (!conn || !root) return
      const mine = [...expanded].filter((id) => id === root || id.startsWith(root + '\u001f'))
      saveTreeExpanded(conn, mine)
    }, 250)
  }

  /** @param {string} id @param {boolean} open */
  function setOpen(id, open) {
    if (expanded.has(id) === open) return
    const next = new Set(expanded)
    if (open) next.add(id)
    else next.delete(id)
    expanded = next
    persistExpanded()
  }

  /** Collapse everything but the live connection's root. */
  export function collapseAll() {
    expanded = new Set(activeRootId ? [activeRootId] : [])
    persistExpanded()
    active = activeRootId
    if (scroller) scroller.scrollTop = 0
    scrollTop = 0
  }

  // ── Roots ───────────────────────────────────────────────────────────────
  const shownConnections = $derived.by(() => {
    const list = inWorkspace(connections, $activeWorkspaceId, $workspaces)
    if (connection && !list.some((c) => c.id === connection?.id)) {
      const live = connections.find((c) => c.id === connection?.id) ?? connection
      return [live, ...list]
    }
    return list
  })

  /** Pinned connections first; the sort is stable, so the rest keep their order. @type {TreeNode[]} */
  const roots = $derived(
    shownConnections
      .map((c) => {
        const id = childId('', 'c', c.id ?? 'live')
        const live = id === activeRootId
        // The saved copy owns the pin: the live object may predate it.
        const pinned = !!(c.pinned ?? connections.find((x) => x.id === c.id)?.pinned)
        return makeNode(id, 'connection', c.name || c.database || c.host || c.filePath || 'Connection', {
          branch: true,
          data: { conn: c, live, pinned },
        })
      })
      .sort((a, b) => Number(b.data.pinned) - Number(a.data.pinned)),
  )

  /** @param {SavedConnection} c */
  function togglePinned(c) {
    if (!c?.id) return
    const now = !!connections.find((x) => x.id === c.id)?.pinned
    onconnectionmeta(setConnectionMeta(c.id, { pinned: !now }))
  }

  // ── Live children: built from what the shell already holds ──────────────
  /**
   * @param {string} schema
   * @param {any[]} list table list rows ({ name, kind, rowCount })
   * @param {import('$lib/api.js').ObjectListing | null} listing
   * @param {Map<string, ChildEntry>} out
   */
  function addSchemaGroups(schema, list, listing, out) {
    const sid = schemaNodeId(schema)
    if (!sqlTables) {
      // NoSQL: collections straight under the schema; they open into what the
      // engine keeps (indexes, typed keys) where it keeps it cheaply.
      out.set(sid, {
        status: 'ready',
        nodes: list.map((t) => makeNode(childId(sid, 't', t.name), 'table', t.name, {
          branch: shaped, count: t.rowCount ?? null, data: { schema, name: t.name, kind: 'table' },
        })),
      })
      return
    }
    /** @type {Record<string, any[]>} */
    const by = { table: [], view: [], matview: [] }
    for (const t of list) {
      const k = t.kind === 'view' ? 'view' : t.kind === 'materialized_view' ? 'matview' : 'table'
      by[k].push(t)
    }
    /** @type {TreeNode[]} */
    const groups = []
    /** @param {string} key @param {string} label @param {TreeNode[]} nodes @param {number | null} [count] */
    const group = (key, label, nodes, count = nodes.length) => {
      const gid = childId(sid, 'g', key)
      groups.push(makeNode(gid, 'group', label, { branch: true, count, data: { schema, group: key } }))
      out.set(gid, { status: 'ready', nodes })
    }
    /** @param {string} gid @param {any} t @param {'table' | 'view' | 'matview'} kind */
    const tableNode = (gid, t, kind) =>
      makeNode(childId(gid, 't', t.name), kind, t.name, {
        branch: true,
        count: kind === 'table' ? (t.rowCount ?? null) : null,
        data: { schema, name: t.name, kind, rls: !!t.rlsEnabled },
      })
    const tg = childId(sid, 'g', 'tables')
    group('tables', 'tables', by.table.map((t) => tableNode(tg, t, 'table')))
    const vg = childId(sid, 'g', 'views')
    group('views', 'views', by.view.map((t) => tableNode(vg, t, 'view')))
    if (by.matview.length) {
      const mg = childId(sid, 'g', 'matviews')
      group('matviews', 'materialized views', by.matview.map((t) => tableNode(mg, t, 'matview')))
    }
    for (const kind of objectKinds) {
      const meta = OBJECT_KIND_META[/** @type {import('$lib/object-templates.js').ObjectKind} */ (kind)]
      const gid = childId(sid, 'g', kind)
      if (!listing) {
        // Still reading: the group is there, its count arrives with the listing.
        groups.push(makeNode(gid, 'group', meta.label.toLowerCase(), { branch: true, count: null, data: { schema, group: kind } }))
        continue
      }
      const objs = listing.objects.filter((o) => o.kind === kind)
      group(kind, meta.label.toLowerCase(), objs.map((o, i) =>
        makeNode(childId(gid, 'o', `${o.name}(${o.args ?? ''})${o.table ? '@' + o.table : ''}#${i}`), 'object', o.name, {
          meta: kind === 'function' || kind === 'procedure' ? `(${o.args ?? ''})` : kind === 'trigger' && o.table ? `on ${o.table}` : '',
          data: { schema, obj: o, kind },
        }),
      ))
    }
    out.set(sid, { status: 'ready', nodes: groups })
  }

  /** @type {Map<string, ChildEntry>} */
  const live = $derived.by(() => {
    void liveTick
    /** @type {Map<string, ChildEntry>} */
    const out = new Map()
    if (!activeRootId) return out
    const schemaNodes = (/** @type {string[]} */ list, /** @type {string} */ parent) =>
      list.map((s) => makeNode(childId(parent, 's', s), 'schema', s, { branch: true, data: { schema: s } }))
    if (schemas.length) {
      /** @type {TreeNode[]} */
      const top = schemaNodes(foldSystem ? userSchemas : schemas, schemaParentId)
      if (foldSystem) {
        const fid = childId(schemaParentId, 'f', 'system')
        top.push(makeNode(fid, 'folder', 'system schemas', { branch: true, count: systemSchemas.length }))
        out.set(fid, { status: 'ready', nodes: schemaNodes(systemSchemas, fid) })
      }
      out.set(schemaParentId, { status: 'ready', nodes: top })
    }
    // The open schema, from the shell's list.
    if (activeSchema || !schemas.length) {
      const d = schemaData.get(activeSchema)
      if (loadingTables && tables.length === 0) out.set(schemaNodeId(activeSchema), { status: 'loading', nodes: [] })
      else addSchemaGroups(activeSchema, tables, d?.objects ?? null, out)
    }
    // Every other schema we have read.
    for (const [s, d] of schemaData) {
      if (s === activeSchema || !d.tables) continue
      if (schemas.length && !schemas.includes(s)) continue
      addSchemaGroups(s, d.tables, d.objects, out)
    }
    return out
  })

  /** @param {string} id */
  const getChildren = (id) => live.get(id) ?? cache.get(id)

  /** A connection that is not live, or a database that is not current, opens by switching to it. @param {TreeNode} n */
  const opensBySwitching = (n) =>
    (n.kind === 'connection' && !n.data?.live) || (n.kind === 'database' && !n.data?.current)

  // ── The rows ────────────────────────────────────────────────────────────
  const q = $derived(filter.trim().toLowerCase())
  const filtered = $derived.by(() => {
    void tick$
    return q ? filterTree(roots, getChildren, q) : null
  })
  const flat = $derived.by(() => {
    void tick$
    const isOpen = (/** @type {string} */ id) => expanded.has(id)
    return flattenTree(roots, (id) => {
      // Roots that are not live never open: their children are another server's.
      if (id.indexOf('\u001f') < 0 && id !== activeRootId) return { status: 'ready', nodes: [] }
      return getChildren(id)
    }, (id) => {
      if (!isOpen(id)) return false
      if (id.indexOf('\u001f') < 0) return id === activeRootId
      // A database other than the current one opens by switching.
      if (canSwitchDb && id.startsWith(activeRootId + '\u001fd:') && id.split('\u001f').length === 2) return id === schemaParentId
      return true
    }, filtered)
  })
  const rows = $derived(flat.rows)

  // ── Loading ─────────────────────────────────────────────────────────────
  $effect(() => {
    const missing = flat.missing
    if (!missing.length) return
    untrack(() => {
      for (const id of missing) {
        const i = rowIndex(rows, id)
        const r = rows[i]
        if (r?.t === 'node') void load(r.node)
      }
    })
  })

  // The open schema's routines, when it is open and not read yet.
  $effect(() => {
    if (!family || !activeRootId) return
    const sid = schemaNodeId(activeSchema)
    if (!expanded.has(sid) && sid !== schemaParentId) return
    void tick$
    untrack(() => {
      const d = schemaData.get(activeSchema)
      if (!d?.objects && !d?.objectsLoading) void loadObjects(activeSchema)
    })
  })

  /** @param {any[]} list */
  function normalizeTables(list) {
    return (list ?? [])
      .map((t) => ({
        name: t.name ?? t.table_name ?? '',
        kind: t.kind ?? 'table',
        rowCount: normalizeTableRowCount(t.rowCount ?? t.row_count),
        rlsEnabled: t.rlsEnabled ?? null,
      }))
      .filter((t) => t.name)
  }

  /** @param {string} schema @param {boolean} [quiet] keep what is shown while it reads */
  async function loadObjects(schema, quiet = false) {
    if (!family) return
    const my = epoch
    const d = schemaData.get(schema) ?? { tables: null, objects: null }
    d.objectsLoading = true
    if (!quiet) d.objects = d.objects ?? null
    schemaData.set(schema, d)
    try {
      const listing = await listDbObjects(schema)
      if (my !== epoch) return
      // A reply without a list (an engine with no catalog) is an empty one, not a crash.
      d.objects = { kinds: listing?.kinds ?? [], cascade: !!listing?.cascade, objects: Array.isArray(listing?.objects) ? listing.objects : [] }
    } catch {
      // Routines are extra: a failure leaves their groups empty, the tables stay.
      if (my === epoch && !d.objects) d.objects = { kinds: [], cascade: false, objects: [] }
    } finally {
      if (my === epoch) { d.objectsLoading = false; bumpSchemas() }
    }
  }

  /** @param {string} schema @param {string} sid @param {boolean} [quiet] */
  async function loadSchema(schema, sid, quiet = false) {
    const my = epoch
    if (!quiet) setEntry(sid, { status: 'loading', nodes: [] })
    try {
      const [list] = await Promise.all([listTables(schema), family ? loadObjects(schema, quiet) : null])
      if (my !== epoch) return
      const d = schemaData.get(schema) ?? { tables: null, objects: null }
      d.tables = normalizeTables(list)
      schemaData.set(schema, d)
      warmSchema(schema, d.tables.length)
      cache.delete(sid)
      bumpSchemas()
    } catch (e) {
      if (my !== epoch) return
      setEntry(sid, { status: 'error', nodes: [], error: String(e) })
    }
  }

  /** One index listing per schema (the command is schema-wide), grouped by table. @type {Map<string, Promise<Map<string, any[]>>>} */
  let indexLists = new Map()
  /** Per-collection index reads, for the engines that keep them per collection. @type {Map<string, Promise<any[]>>} */
  let tableIndexes = new Map()
  /**
   * Columns, read for a whole schema in one call and kept, so expanding a table
   * is a cache read rather than a round trip (on a remote database that round
   * trip was most of the wait). Very large schemas skip the bulk read and go
   * per table, warmed on hover. @type {Map<string, Promise<Map<string, any[]> | null>>}
   */
  let columnLists = new Map()
  /** Per-table column reads, for big schemas and anything the bulk read missed. @type {Map<string, Promise<any[]>>} */
  let tableColumns = new Map()
  /**
   * The same reads once they have landed, readable without an await, so a
   * warmed table opens in the click's own task with no loading row between.
   * Keys: `c|schema␀table` / `i|schema␀table` per table, `C|schema` /
   * `I|schema` for the schema-wide maps. @type {Map<string, any>}
   */
  let shapes = new Map()
  const BULK_COLUMNS_MAX_TABLES = 1500
  /** @param {string} schema @param {string} table */
  const tkey = (schema, table) => `${schema}\u0000${table}`

  /** @param {string} schema */
  function schemaColumns(schema) {
    let p = columnLists.get(schema)
    if (!p) {
      const store = shapes
      p = /** @type {Promise<any[]>} */ (getSchemaColumnStructure(schema))
        .then((rows) => {
          const map = new Map((rows ?? []).map((r) => [r.table, r.columns ?? []]))
          store.set(`C|${schema}`, map)
          return map
        })
        .catch(() => null)
      columnLists.set(schema, p)
    }
    return p
  }

  /** @param {string} schema @param {string} table @returns {Promise<any[]>} */
  async function columnsOf(schema, table) {
    if (columnLists.has(schema)) {
      const all = await schemaColumns(schema)
      const hit = all?.get(table)
      if (hit) return hit
    }
    const k = tkey(schema, table)
    let p = tableColumns.get(k)
    if (!p) {
      const store = shapes
      p = /** @type {Promise<any[]>} */ (getTableColumnStructure(schema, table)).then((cols) => {
        store.set(`c|${k}`, cols ?? [])
        return cols ?? []
      })
      p.catch(() => tableColumns.delete(k))
      tableColumns.set(k, p)
    }
    return p
  }

  /** A table's indexes. A failed listing just means none shown, not a failed expand. @param {string} schema @param {string} table @returns {Promise<any[]>} */
  function indexesOf(schema, table) {
    const store = shapes
    if (perTableIndexes) {
      if (!table) return Promise.resolve([])
      const k = tkey(schema, table)
      let p = tableIndexes.get(k)
      if (!p) {
        p = /** @type {Promise<any[]>} */ (listIndexes(schema, table))
          .then((v) => v ?? [], () => [])
          .then((v) => { store.set(`i|${k}`, v); return v })
        tableIndexes.set(k, p)
      }
      return p
    }
    let p = indexLists.get(schema)
    if (!p) {
      // Grouped by table once: each expand is a lookup, not a scan of every index in the schema.
      p = /** @type {Promise<any[]>} */ (listIndexes(schema))
        .then((all) => {
          /** @type {Map<string, any[]>} */
          const by = new Map()
          for (const ix of all ?? []) {
            const t = ix.tableName ?? ix.table_name
            const list = by.get(t)
            if (list) list.push(ix)
            else by.set(t, [ix])
          }
          return by
        }, () => /** @type {Map<string, any[]>} */ (new Map()))
        .then((by) => { store.set(`I|${schema}`, by); return by })
      indexLists.set(schema, p)
    }
    return p.then((by) => by.get(table) ?? [])
  }

  /** What has landed for a table, or null while something is still out. @param {string} schema @param {string} table @param {boolean} wantIndexes */
  function shapeNow(schema, table, wantIndexes) {
    const k = tkey(schema, table)
    const cols = shapes.get(`C|${schema}`)?.get(table) ?? shapes.get(`c|${k}`)
    if (!cols) return null
    if (!wantIndexes) return { cols, idx: [] }
    const idx = perTableIndexes
      ? shapes.get(`i|${k}`)
      : shapes.has(`I|${schema}`) ? (shapes.get(`I|${schema}`).get(table) ?? []) : undefined
    return idx ? { cols, idx } : null
  }

  /**
   * Warm a schema once its tables are on screen: one bulk column read and the
   * index listing, in the background, so the first table you open is instant.
   * @param {string} schema @param {number} tableCount
   */
  function warmSchema(schema, tableCount) {
    if (!shaped) return
    // Of the NoSQL engines only Cassandra keeps a keyspace-wide catalog; the rest read per collection.
    if (!sqlTables && engine !== 'cassandra') return
    if (tableCount <= BULK_COLUMNS_MAX_TABLES) void schemaColumns(schema)
    void indexesOf(schema, '')
  }

  /**
   * Hovering a closed table starts its read, so the click finds it loaded. Only
   * once the pointer rests on it: rows sliding under a still cursor during a
   * scroll each fired a request, and that work landed in the middle of the scroll.
   * @type {ReturnType<typeof setTimeout> | null}
   */
  let warmTimer = null
  /** @param {TreeNode} n */
  function warmTable(n) {
    if (warmTimer) clearTimeout(warmTimer)
    warmTimer = null
    if (!isTableish(n)) return
    if (cache.has(n.id)) return
    warmTimer = setTimeout(() => {
      warmTimer = null
      if (performance.now() - lastScrollAt < 200) return
      warmNow(n)
    }, 150)
  }
  /** One listener for the whole tree, not one closure per row. @type {string} */
  let hoverKey = ''
  /** @param {PointerEvent} e */
  function onPointerOver(e) {
    const hit = rowAt(e)
    const key = hit?.r.key ?? ''
    if (key === hoverKey) return
    hoverKey = key
    if (hit?.r.t === 'node' && hit.el?.dataset.sticky === undefined) warmTable(hit.r.node)
    else if (warmTimer) { clearTimeout(warmTimer); warmTimer = null }
  }

  /** @param {TreeNode} n */
  function warmNow(n) {
    if (cache.has(n.id)) return
    void columnsOf(n.data.schema, n.data.name).catch(() => {})
    if (n.kind === 'table') void indexesOf(n.data.schema, n.data.name)
  }

  /** A table, view or collection that opens into its columns / keys / indexes. @param {TreeNode} n */
  const isTableish = (n) => n.branch && (n.kind === 'table' || n.kind === 'view' || n.kind === 'matview')

  /** Fill a table's children, and its sections', from its columns and indexes. @param {TreeNode} node @param {any[]} cols @param {any[]} idx */
  function applyShape(node, cols, idx) {
    const { schema, name } = node.data
    const { nodes, sections } = tableSections(node.id, { schema, table: name }, cols, idx, node.kind === 'table')
    for (const [gid, list] of sections) cache.set(gid, { status: 'ready', nodes: list })
    setEntry(node.id, { status: 'ready', nodes })
  }

  /** @param {TreeNode} node @param {boolean} [quiet] */
  async function load(node, quiet = false) {
    const my = epoch
    const id = node.id
    const prev = cache.get(id)
    if (prev?.status === 'loading' && !quiet) return
    if (node.kind === 'schema') return loadSchema(node.data.schema, id, quiet)
    if (node.kind === 'connection') {
      if (!node.data?.live || !canSwitchDb) return
      setEntry(id, { status: 'loading', nodes: quiet ? (prev?.nodes ?? []) : [] })
      const dbs = await listDatabases(connection)
      if (my !== epoch) return
      // The current database's node id must be the one schemas hang from
      // (activeDbKey), even when the listing matched it by name.
      const listed = dbs.map((d) => (isCurrentDatabase(d, connection) ? { ...d, key: activeDbKey || d.key } : d))
      const list = listed.some((d) => d.key === activeDbKey) || !activeDbKey ? listed : [{ key: activeDbKey, label: activeDbLabel }, ...listed]
      setEntry(id, {
        status: 'ready',
        nodes: list.map((d) => makeNode(childId(id, 'd', d.key), 'database', d.label, {
          branch: true, data: { entry: d, current: d.key === activeDbKey },
        })),
      })
      return
    }
    if (isTableish(node)) {
      const { schema, name } = node.data
      const isTable = node.kind === 'table'
      if (quiet) {
        const k = tkey(schema, name)
        indexLists.delete(schema)
        columnLists.delete(schema)
        tableColumns.delete(k)
        tableIndexes.delete(k)
        for (const sk of [`C|${schema}`, `I|${schema}`, `c|${k}`, `i|${k}`]) shapes.delete(sk)
      } else {
        // Warmed: build it now, in this task - no loading row, no second pass.
        const ready = shapeNow(schema, name, isTable)
        if (ready) {
          applyShape(node, ready.cols, ready.idx)
          return
        }
      }
      setEntry(id, { status: 'loading', nodes: quiet ? (prev?.nodes ?? []) : [] })
      try {
        const [cols, idx] = await Promise.all([
          columnsOf(schema, name),
          isTable ? indexesOf(schema, name) : Promise.resolve([]),
        ])
        if (my !== epoch) return
        applyShape(node, cols, idx)
      } catch (e) {
        if (my !== epoch) return
        setEntry(id, { status: 'error', nodes: quiet ? (prev?.nodes ?? []) : [], error: String(e) })
      }
    }
  }

  // The open schema's tables come from the shell: warm them the same way, once
  // per schema per place, after the list has arrived.
  let warmedOpen = ''
  $effect(() => {
    const schema = activeSchema
    const n = tables.length
    if (loadingTables || !n) return
    const key = `${placeKey}\u0000${schema}`
    if (key === warmedOpen) return
    warmedOpen = key
    untrack(() => warmSchema(schema, n))
  })

  /** Retry an error row. @param {string} id */
  function retry(id) {
    cache.delete(id)
    tick$++
  }

  // The databases on this server changed (create / rename / drop).
  let lastDbRefresh = untrack(() => databasesRefreshKey)
  $effect(() => {
    const k = databasesRefreshKey
    if (k === lastDbRefresh) return
    lastDbRefresh = k
    untrack(() => {
      const root = roots.find((r) => r.id === activeRootId)
      if (root && expanded.has(root.id)) void load(root, true)
      else { cache.delete(activeRootId); tick$++ }
    })
  })

  // Refresh, or a CREATE / DROP anywhere: re-read what is open, forget the rest.
  let lastObjects = untrack(() => $objectsVersion)
  $effect(() => {
    const v = $objectsVersion
    if (v === lastObjects) return
    lastObjects = v
    untrack(() => refreshOpen())
  })

  /** Re-read every open listing in place and drop the closed ones. */
  export function refreshOpen() {
    for (const [s, d] of schemaData) {
      const sid = schemaNodeId(s)
      if (!expanded.has(sid) && sid !== schemaParentId) { schemaData.delete(s); continue }
      if (s === activeSchema) void loadObjects(s, true)
      else void loadSchema(s, sid, true)
    }
    for (const [id, e] of cache) {
      if (e.status === 'loading') continue
      if (!expanded.has(id)) { cache.delete(id); continue }
      const i = rowIndex(rows, id)
      const r = rows[i]
      if (r?.t === 'node') void load(r.node, true)
      else cache.delete(id)
    }
    bumpSchemas()
  }

  // ── Window ──────────────────────────────────────────────────────────────
  let scroller = $state(/** @type {HTMLDivElement | null} */ (null))
  let probe = $state(/** @type {HTMLDivElement | null} */ (null))
  /** One row in whole px: measured from an h-7 probe (it follows the zoom) and
   *  rounded, then used for BOTH the stride and the painted height. */
  let rowH = $state(28)
  let scrollTop = $state(0)
  let viewH = $state(400)
  /** Rows drawn past each edge. The scroll handler is synchronous, so a short
   *  margin never shows a gap, and every row it saves is one fewer to build on
   *  an expand. */
  const OVERSCAN = 10
  const start = $derived(Math.max(0, Math.floor(scrollTop / rowH) - OVERSCAN))
  const end = $derived(Math.min(rows.length, Math.ceil((scrollTop + viewH) / rowH) + OVERSCAN))
  const visible = $derived(rows.slice(start, end))
  /** Rows to pin while scrolling: at most a third of the view, at most four. */
  const maxSticky = $derived(Math.max(0, Math.min(4, Math.floor(viewH / rowH / 3))))
  const sticky = $derived(q ? [] : stickyIndices(rows, Math.floor(scrollTop / rowH), maxSticky))
  const stickyTop = $derived(stickyTops(rows, sticky, scrollTop, rowH))

  // Read the offset in the scroll event itself. The browser already sends at
  // most one per frame, and deferring to the next animation frame drew the
  // window (and the pinned rows) a frame behind the content: the lag you could
  // feel against a trackpad.
  let scrollFrame = 0
  let lastScrollAt = 0
  function onScroll() {
    lastScrollAt = performance.now()
    if (scroller) scrollTop = scroller.scrollTop
  }

  onMount(() => {
    const measure = () => {
      const h = probe?.getBoundingClientRect().height
      if (h && h > 0) rowH = Math.max(16, Math.round(h))
      // Hidden (another tab is up) measures 0: keep the last real height.
      if (scroller && scroller.clientHeight > 0) viewH = scroller.clientHeight
    }
    measure()
    const ro = new ResizeObserver(measure)
    if (probe) ro.observe(probe)
    if (scroller) ro.observe(scroller)
    return () => {
      ro.disconnect()
      if (scrollFrame) cancelAnimationFrame(scrollFrame)
      if (warmTimer) clearTimeout(warmTimer)
      if (saveTimer) clearTimeout(saveTimer)
    }
  })

  // A list that shrank under the offset (collapse, filter) must not leave the
  // window past its end - that was the blank-list-behind-a-spacer bug.
  $effect(() => {
    const max = Math.max(0, rows.length * rowH - viewH)
    if (scroller && scrollTop > max) {
      scroller.scrollTop = max
      scrollTop = scroller.scrollTop
    }
  })

  // ── Focus and keyboard ──────────────────────────────────────────────────
  /** The active row's key (a node id, or a status row's key). */
  let active = $state('')
  let kbd = $state(false)
  let focused = $state(false)
  const activeIndex = $derived(rowIndex(rows, active))
  const guide = $derived(activeIndex >= 0 ? branchRange(rows, activeIndex) : null)
  /** @param {number} i */
  const rowDomId = (i) => `${uid}-row-${i}`

  /** @param {number} i */
  function ensureVisible(i) {
    if (!scroller || i < 0) return
    // From state, not the element: reading scrollTop / clientHeight back after
    // a write forces a layout on every arrow key.
    const pinned = Math.min(sticky.length, maxSticky) * rowH
    const top = i * rowH
    let next = scrollTop
    if (top < scrollTop + pinned) next = Math.max(0, top - pinned)
    else if (top + rowH > scrollTop + viewH) next = top + rowH - viewH
    next = Math.max(0, Math.min(next, rows.length * rowH - viewH))
    if (next === scrollTop) return
    scroller.scrollTop = next
    scrollTop = next
  }

  /** @param {number} i */
  function setActive(i) {
    const r = rows[Math.max(0, Math.min(rows.length - 1, i))]
    if (!r) return
    active = r.key
    ensureVisible(rowIndex(rows, r.key))
  }

  /** Open or close a branch, switching connection / database where that is what opening means. @param {TreeNode} n @param {boolean} [to] */
  function toggleNode(n, to = !expanded.has(n.id) || opensBySwitching(n)) {
    if (!n.branch) return
    // While filtering, a click still has to do something. A connection or
    // database you are not on connects; anything else opens in the full tree:
    // the filter clears and the branch is revealed, expanded and selected.
    if (q && !opensBySwitching(n)) {
      expanded = new Set([...expanded, ...ancestorIds(n.id), n.id])
      persistExpanded()
      active = n.id
      onclearfilter()
      void tick().then(() => ensureVisible(rowIndex(rows, n.id)))
      return
    }
    if (to && opensBySwitching(n)) {
      setOpen(n.id, true)
      if (n.kind === 'connection') onswitchconnection(n.data.conn)
      else onswitchdatabase(n.data.entry)
      return
    }
    // A warmed table builds its children before it opens: one flatten, not three.
    if (to && isTableish(n) && !cache.has(n.id)) void load(n)
    setOpen(n.id, to)
  }

  /** Enter, or a click on a row that is not a folder. @param {TreeNode} n */
  function activate(n) {
    switch (n.kind) {
      case 'table':
      case 'view':
      case 'matview':
        openTable(n.data.schema, n.data.name)
        return
      case 'column':
      case 'key':
      case 'index':
        openTable(n.data.schema, n.data.table)
        return
      case 'object':
        void openDefinition(n.data.schema, n.data.obj, onopenobjectsql)
        return
      default:
        toggleNode(n)
    }
  }

  /** @param {string} schema @param {string} name */
  function openTable(schema, name) {
    if (schema === activeSchema) ontableselect(name)
    else onopentable(schema, name)
  }

  let typed = ''
  let typedAt = 0

  /** @param {KeyboardEvent} e */
  function onKey(e) {
    if (!rows.length) return
    const stop = () => { e.preventDefault(); e.stopPropagation(); kbd = true }
    const i = activeIndex
    if ((e.key === 'F10' && e.shiftKey) || e.key === 'ContextMenu') {
      stop()
      if (i < 0) return
      void tick().then(() => {
        const el = scroller?.querySelector(`#${CSS.escape(rowDomId(i))}`)
        if (!el) return
        const box = el.getBoundingClientRect()
        el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: box.left + 40, clientY: box.bottom - 4 }))
      })
      return
    }
    if (e.ctrlKey || e.metaKey || e.altKey) return
    if (e.key === 'ArrowUp' && i === 0) { stop(); kbd = false; onexittop(); return }
    const page = Math.max(1, Math.floor(viewH / rowH) - 1)
    const action = treeKeyAction(rows, i, e.key, page)
    switch (action.type) {
      case 'move': stop(); setActive(action.index); return
      case 'expand':
      case 'collapse':
      case 'toggle': {
        stop()
        const r = rows[i]
        if (r?.t === 'node') toggleNode(r.node, action.type === 'toggle' ? undefined : action.type === 'expand')
        return
      }
      case 'activate': {
        stop()
        const r = rows[action.index]
        if (r?.t === 'node') activate(r.node)
        return
      }
      case 'retry': stop(); retry(action.id); return
    }
    // Type-ahead: letters jump to the next row whose name starts with them.
    if (e.key.length === 1 && e.key !== ' ') {
      stop()
      const now = performance.now()
      typed = now - typedAt > 700 ? e.key : typed + e.key
      typedAt = now
      const hit = typeAheadIndex(rows, i, typed)
      if (hit >= 0) setActive(hit)
    }
  }

  /** @param {MouseEvent} e */
  function rowAt(e) {
    const el = e.target instanceof Element ? /** @type {HTMLElement | null} */ (e.target.closest('[data-index]')) : null
    const i = el ? Number(el.dataset.index) : -1
    return i >= 0 && rows[i] ? { i, r: rows[i], el } : null
  }

  /** @param {MouseEvent} e */
  function onClick(e) {
    const hit = rowAt(e)
    if (!hit) return
    kbd = false
    active = hit.r.key
    scroller?.focus({ preventScroll: true })
    const target = /** @type {Element} */ (e.target)
    if (hit.el?.dataset.sticky !== undefined && !target.closest('[data-chevron]')) {
      // A pinned row is a way back to its place: scroll there, leaving the
      // row exactly where it was pinned, under its own pinned ancestors.
      const k = sticky.indexOf(hit.i)
      if (scroller) scroller.scrollTop = Math.max(0, (hit.i - Math.max(0, k)) * rowH)
      if (scroller) scrollTop = scroller.scrollTop
      return
    }
    if (hit.r.t === 'error') { if (target.closest('[data-retry]')) retry(hit.r.owner); return }
    if (hit.r.t !== 'node') return
    const n = hit.r.node
    // The chevron always folds; the rest of the row opens leaves and folds folders.
    if (target.closest('[data-chevron]')) toggleNode(n)
    else activate(n)
  }

  /** Focus the tree from outside (ArrowDown in the filter box). */
  export function focusTree() {
    if (!scroller || !rows.length) return false
    if (activeIndex < 0) {
      const firstMatch = q ? rows.findIndex((r) => r.t === 'node' && r.node.lname.includes(q)) : -1
      active = rows[Math.max(0, firstMatch)].key
    }
    kbd = true
    scroller.focus({ preventScroll: true })
    ensureVisible(Math.max(0, rowIndex(rows, active)))
    return true
  }

  /** Enter in the filter box: open the one match, if the filter left exactly one. */
  export function openSole() {
    if (!q || filtered?.matches !== 1) return false
    const r = rows.find((x) => x.t === 'node' && x.node.lname.includes(q))
    if (r?.t !== 'node') return false
    active = r.key
    activate(r.node)
    return true
  }

  // ── Selection follows the active tab ────────────────────────────────────
  $effect(() => {
    const name = activeTable
    const schema = activeSchema
    void connKey
    if (!name || !activeRootId) return
    untrack(() => {
      const t = tables.find((x) => x.name === name)
      const kind = t?.kind === 'view' ? 'views' : t?.kind === 'materialized_view' ? 'matviews' : 'tables'
      const sid = schemaNodeId(schema)
      const id = sqlTables ? childId(childId(sid, 'g', kind), 't', name) : childId(sid, 't', name)
      if (active === id) return
      const need = ancestorIds(id).filter((a) => !expanded.has(a))
      if (need.length) {
        expanded = new Set([...expanded, ...need])
        persistExpanded()
      }
      active = id
      void tick().then(() => { if (!q) ensureVisible(rowIndex(rows, id)) })
    })
  })

  // ── Context menu ────────────────────────────────────────────────────────
  /** @type {FlatRow | null} */
  let menuRow = $state(null)

  /** @param {string} text */
  async function copyText(text) {
    try {
      await navigator.clipboard.writeText(text)
      toast.success('Copied')
    } catch (e) {
      toast.error('Copy failed', { description: String(e) })
    }
  }

  // ── Create ──────────────────────────────────────────────────────────────
  // The shell's own flows: the Create table / schema / database dialogs, and
  // the CREATE templates the Objects tab opens in an editor.

  /** A group's kind: `tables` creates a table, the rest their object kind. @param {string} group */
  const groupKind = (group) => group === 'tables' ? 'table' : group === 'views' ? 'view' : group === 'matviews' ? 'matview' : group

  /** @param {string} kind */
  const kindOne = (kind) => kind === 'table' ? 'table' : (OBJECT_KIND_META[/** @type {import('$lib/object-templates.js').ObjectKind} */ (kind)]?.one ?? kind)

  /** @param {string} kind */
  const kindIcon = (kind) => kind === 'table' ? 'table-2' : (OBJECT_KIND_META[/** @type {import('$lib/object-templates.js').ObjectKind} */ (kind)]?.icon ?? 'box')

  /** Whether this engine can create `kind` from the tree. @param {string} kind */
  const canCreateKind = (kind) => sqlTables && (kind === 'table' || templateKinds.includes(/** @type {any} */ (kind)))

  /** @param {string} kind @param {string} schema */
  function createIn(kind, schema) {
    if (!guardWrite(`create a ${kindOne(kind)}`)) return
    if (kind === 'table') {
      // The dialog creates in the open schema.
      if (schemas.length && schema && schema !== activeSchema) onschemachange(schema)
      onnewtable()
      return
    }
    openTemplate(connection?.type, /** @type {import('$lib/object-templates.js').ObjectKind} */ (kind), schema, onopenobjectsql)
  }

  /** The schema a row stands for, when it holds a schema's groups. @param {TreeNode} n */
  function schemaOf(n) {
    if (n.kind === 'schema') return n.data.schema
    if (!schemas.length && n.id === schemaParentId) return activeSchema
    return null
  }

  // ── Drawing ─────────────────────────────────────────────────────────────
  /** Left padding, and the width of one level (the chevron's column). */
  const PAD = 0.375
  const INDENT = 1
  /** @param {number} depth */
  const padFor = (depth) => `${PAD + depth * INDENT}rem`
  /** Centre of the chevron column at `level`, where that level's guide runs. @param {number} level */
  const guideAt = (level) => `calc(${PAD + level * INDENT + INDENT / 2}rem - 0.5px)`

  /** @param {TreeNode} n @param {boolean} open */
  function iconFor(n, open) {
    switch (n.kind) {
      case 'database': return { name: 'database', tone: n.data?.current ? 'text-primary' : 'text-muted-foreground' }
      case 'schema': return { name: 'box', tone: 'text-muted-foreground' }
      case 'folder':
      case 'group': return { name: open ? 'folder-open' : 'folder', tone: 'text-muted-foreground' }
      case 'table': return { name: 'table-2', tone: 'text-warning' }
      case 'view': return { name: OBJECT_KIND_META.view.icon, tone: OBJECT_KIND_META.view.tone }
      case 'matview': return { name: OBJECT_KIND_META.matview.icon, tone: OBJECT_KIND_META.matview.tone }
      case 'object': {
        const m = OBJECT_KIND_META[/** @type {import('$lib/object-templates.js').ObjectKind} */ (n.data?.kind)]
        return { name: m?.icon ?? 'box', tone: m?.tone ?? 'text-muted-foreground' }
      }
      case 'column':
        return n.data?.pk ? { name: 'key-round', tone: 'text-warning' } : n.data?.fk ? { name: 'link-2', tone: 'text-info' } : { name: 'columns-3', tone: 'text-muted-foreground' }
      case 'section':
        return n.data?.section === 'keys'
          ? { name: 'key-round', tone: 'text-muted-foreground' }
          : n.data?.section === 'indexes'
            ? { name: 'list-tree', tone: 'text-muted-foreground' }
            : { name: 'columns-3', tone: 'text-muted-foreground' }
      case 'key':
        return n.data?.pk ? { name: 'key-round', tone: 'text-warning' } : { name: 'link-2', tone: 'text-info' }
      case 'index':
        return { name: 'list-tree', tone: n.data?.unique ? 'text-primary' : 'text-muted-foreground' }
    }
    return { name: 'box', tone: 'text-muted-foreground' }
  }


  /** @param {TreeNode} n */
  function countText(n) {
    if (n.count == null) return ''
    return n.kind === 'table' ? formatTableRowCount(n.count) : String(n.count)
  }

  /** The filter's match inside a label. @param {string} label */
  function parts(label) {
    if (!q) return null
    const i = label.toLowerCase().indexOf(q)
    if (i < 0) return null
    return [label.slice(0, i), label.slice(i, i + q.length), label.slice(i + q.length)]
  }

  /** One formatter for every row: `toLocaleString` builds a new one per call. */
  const ROWS_FMT = new Intl.NumberFormat('en-US')

  /** @param {TreeNode} n */
  function tooltip(n) {
    if (n.kind === 'column') return `${n.label} ${n.meta}${n.data?.pk ? ' · primary key' : ''}${n.data?.fk ? ` · references ${n.data.fk}` : ''}${n.data?.nullable === false ? ' · not null' : ''}`
    if (n.kind === 'key') return `${n.label} ${n.meta}${n.data?.pk ? ' · primary key' : ' · foreign key'}`
    if (n.kind === 'index') return `${n.label} ${n.meta}${n.data?.type ? ` · ${n.data.type}` : ''}${n.data?.primary ? ' · primary' : ''}${n.data?.condition ? ` · where ${n.data.condition}` : ''}`
    if (n.kind === 'connection' && !n.data?.live) return `Connect to ${n.label}`
    if (n.kind === 'database' && !n.data?.current) return `Switch to ${n.label}`
    if (n.kind === 'table' && n.count != null) return `${n.label} · ${ROWS_FMT.format(Number(n.count))} rows`
    return n.label
  }

  const GUIDE_OFF = 'pointer-events-none absolute inset-y-0 w-px bg-border'
  const GUIDE_ON = 'pointer-events-none absolute inset-y-0 w-px bg-muted-foreground/45'
  const ROW_IDLE = 'text-foreground/80 hover:bg-sidebar-accent/50 hover:text-foreground'
  const ROW_ACTIVE = 'bg-sidebar-accent text-sidebar-accent-foreground'
  const ROW_RING = 'outline-2 -outline-offset-2 outline-ring'
  const ROW = 'absolute inset-x-0 flex cursor-default select-none items-center gap-1.5 pr-2 text-left contain-layout contain-paint contain-style'
</script>

{#snippet newItem(/** @type {string} */ kind, /** @type {string} */ schema, /** @type {boolean} */ short)}
  <ContextMenu.Item
    disabled={$readOnlyMode}
    title={$readOnlyMode ? READ_ONLY_HINT : undefined}
    onSelect={() => createIn(kind, schema)}
  >
    <Icon name={kindIcon(kind)} />
    {short ? kindOne(kind)[0].toUpperCase() + kindOne(kind).slice(1) : `New ${kindOne(kind)}`}…
  </ContextMenu.Item>
{/snippet}

{#snippet adminItem(/** @type {string} */ label, /** @type {string} */ icon, /** @type {() => void} */ run)}
  <ContextMenu.Item
    disabled={$readOnlyMode}
    title={$readOnlyMode ? READ_ONLY_HINT : undefined}
    onSelect={() => { if (guardWrite(`create a ${label.replace(/^New /, '').toLowerCase()}`)) run() }}
  >
    <Icon name={icon} />
    {label}…
  </ContextMenu.Item>
{/snippet}

{#snippet rowBody(/** @type {FlatRow} */ r, /** @type {number} */ i, /** @type {boolean} */ pinned)}
  <!-- Indent guides: one hairline per level, the active branch's a step stronger. -->
  {#each { length: r.depth } as _, level}
    <!-- Plain strings, not cn(): this runs per level per row on every expand. -->
    <span
      class={!pinned && guide && level === guide.level && i >= guide.lo && i <= guide.hi ? GUIDE_ON : GUIDE_OFF}
      style:left={guideAt(level)}
      aria-hidden="true"
    ></span>
  {/each}
  {#if r.t === 'node'}
    {@const n = r.node}
    {@const ic = iconFor(n, r.open)}
    {@const hit = parts(n.label)}
    <span data-chevron class="flex h-full w-4 shrink-0 items-center justify-center" aria-hidden="true">
      {#if n.branch}
        {#if r.loading}
          <Icon name="loader-2" class="size-3 shrink-0 animate-spin text-muted-foreground" />
        {:else}
          <Icon name="chevron-right" class={cn('size-3.5 shrink-0 text-muted-foreground transition-transform duration-100', r.open && 'rotate-90')} />
        {/if}
      {/if}
    </span>
    {#if n.kind === 'connection'}
      <span style={colorOf(n.data?.conn?.color)?.style} class={cn('relative flex size-4 shrink-0 items-center justify-center', colorOf(n.data?.conn?.color)?.text ?? 'text-muted-foreground')}>
        <DbIcon id={n.data?.conn?.type ?? ''} class="size-4" />
        {#if n.data?.live}
          <span class="absolute -right-0.5 -bottom-0.5 size-1.5 rounded-full bg-primary ring-2 ring-sidebar"></span>
        {/if}
      </span>
    {:else}
      <Icon name={ic.name} class={cn('size-4 shrink-0', ic.tone)} />
    {/if}
    <span class="flex min-w-0 flex-1 items-baseline gap-1.5">
      <span class={cn('min-w-0 shrink truncate text-ui-sm leading-4', (n.kind === 'connection' && n.data?.live) || (n.kind === 'database' && n.data?.current) ? 'font-medium text-foreground' : '')}>
        {#if hit}{hit[0]}<mark class="rounded-[2px] bg-primary/25 text-inherit">{hit[1]}</mark>{hit[2]}{:else}{n.label}{/if}
      </span>
      {#if n.meta}
        <span class="min-w-0 shrink-[2] truncate text-ui-2xs text-muted-foreground">{n.meta}</span>
      {/if}
    </span>
    {#if n.kind === 'connection' && n.data?.pinned}
      <Icon name="pin" class="size-3 shrink-0 text-muted-foreground" aria-label="Pinned" />
    {/if}
    {#if n.kind === 'table' && n.data?.rls}
      <Icon name="lock" class="size-3 shrink-0 text-muted-foreground" aria-label="Row-level security enabled" />
    {/if}
    {#if countText(n)}
      <span class="shrink-0 font-mono text-ui-2xs tabular-nums text-muted-foreground">{countText(n)}</span>
    {/if}
  {:else}
    <span class="w-4 shrink-0" aria-hidden="true"></span>
    {#if r.t === 'loading'}
      <Icon name="loader-2" class="loading-reveal size-3.5 shrink-0 animate-spin text-muted-foreground" />
      <span class="loading-reveal text-ui-xs text-muted-foreground">Loading…</span>
    {:else if r.t === 'error'}
      <Icon name="alert-circle" class="size-3.5 shrink-0 text-destructive" />
      <span class="min-w-0 flex-1 truncate text-ui-xs text-destructive" title={r.error}>{r.error}</span>
      <button
        type="button"
        tabindex="-1"
        data-retry
        class="hit-area shrink-0 rounded-md px-1.5 text-ui-xs text-muted-foreground underline-offset-2 hover:bg-accent hover:text-foreground hover:underline"
      >Retry</button>
    {:else}
      <span class="text-ui-xs text-muted-foreground">Empty</span>
    {/if}
  {/if}
{/snippet}

<div class="flex min-h-0 flex-1 flex-col">
  {#if !connection && roots.length === 0}
    <div class="flex flex-col items-center gap-3 px-6 py-14 text-center">
      <div class="flex size-10 items-center justify-center rounded-lg border border-border bg-muted/30">
        <Icon name="list-tree" class="size-5 shrink-0 text-muted-foreground" />
      </div>
      <p class="max-w-[16rem] text-ui-xs font-medium text-foreground">No connections yet</p>
    </div>
  {:else}
    <ContextMenu.Root>
      <ContextMenu.Trigger>
        {#snippet child({ props })}
          {@const openMenu = props.oncontextmenu}
          <div
            {...props}
            bind:this={scroller}
            role="tree"
            tabindex="0"
            aria-label="Database explorer"
            aria-activedescendant={activeIndex >= start && activeIndex < end ? rowDomId(activeIndex) : undefined}
            class="app-scroll relative min-h-0 w-full flex-1 overflow-y-auto overscroll-y-contain outline-none"
            use:smoothScroll={{ enabled: !$appNativeScroll }}
            onscroll={onScroll}
            onkeydown={onKey}
            onfocus={() => (focused = true)}
            onblur={() => (focused = false)}
            onpointerdown={(e) => { kbd = false; /** @type {any} */ (props).onpointerdown?.(e) }}
            onclick={onClick}
            onpointerover={onPointerOver}
            onpointerleave={() => { hoverKey = ''; if (warmTimer) { clearTimeout(warmTimer); warmTimer = null } }}
            oncontextmenu={(e) => {
              const hit = rowAt(e)
              if (!hit || hit.r.t !== 'node') { e.preventDefault(); return }
              menuRow = hit.r
              active = hit.r.key
              openMenu?.(e)
            }}
          >
            <div bind:this={probe} class="pointer-events-none invisible absolute h-7 w-px" aria-hidden="true"></div>

            <!-- Pinned ancestors. A zero-height sticky strip, so the rows below
                 keep their absolute offsets; the pinned rows are copies (hidden
                 from assistive tech) that scroll their own row into view. -->
            {#if sticky.length}
              <div class="pointer-events-none sticky top-0 z-10 h-0" aria-hidden="true">
                {#each sticky as si, k (rows[si].key)}
                  {@const r = rows[si]}
                  <div
                    data-index={si}
                    data-sticky
                    class={cn(ROW, 'pointer-events-auto bg-sidebar hover:bg-sidebar-accent', k === sticky.length - 1 && 'border-b border-border/50', r.key === active && 'bg-sidebar-accent text-sidebar-accent-foreground')}
                    style:top="{stickyTop[k] ?? k * rowH}px"
                    style:z-index={sticky.length - k}
                    style:height="{rowH}px"
                    style:padding-left={padFor(r.depth)}
                  >
                    {@render rowBody(r, si, true)}
                  </div>
                {/each}
              </div>
            {/if}

            <div class="relative w-full" style:height="{rows.length * rowH}px">
              {#each visible as r, n (r.key)}
                {@const i = start + n}
                {@const isActive = r.key === active}
                <div
                  id={rowDomId(i)}
                  role="treeitem"
                  aria-level={r.depth + 1}
                  aria-setsize={r.t === 'node' ? r.size : 1}
                  aria-posinset={r.t === 'node' ? r.pos : 1}
                  aria-expanded={r.t === 'node' && r.node.branch ? r.open : undefined}
                  aria-selected={isActive}
                  aria-busy={r.t === 'loading' || (r.t === 'node' && r.loading) ? true : undefined}
                  aria-label={r.t === 'loading' ? 'Loading' : r.t === 'empty' ? 'Empty' : undefined}
                  data-index={i}
                  tabindex={-1}
                  title={r.t === 'node' ? tooltip(r.node) : undefined}
                  class="{ROW} {isActive ? ROW_ACTIVE : ROW_IDLE} {isActive && focused && kbd ? ROW_RING : ''}"
                  style:top="{i * rowH}px"
                  style:height="{rowH}px"
                  style:padding-left={padFor(r.depth)}
                >
                  {@render rowBody(r, i, false)}
                </div>
              {/each}
            </div>
            {#if q && rows.length === 0}
              <p class="px-3.5 py-1.5 text-ui-2xs text-muted-foreground">Nothing loaded matches “{filter.trim()}”. The filter searches what has been opened.</p>
            {/if}
          </div>
        {/snippet}
      </ContextMenu.Trigger>
      <ContextMenu.Content class="min-w-48">
        {#if menuRow?.t === 'node'}
          {@const n = menuRow.node}
          {@const open = expanded.has(n.id)}
          {#if n.kind === 'table' && n.data.schema === activeSchema && tableMenu}
            <ContextMenu.Item onSelect={() => openTable(n.data.schema, n.data.name)}>
              <Icon name="external-link" />
              Open table
            </ContextMenu.Item>
            <ContextMenu.Separator />
            {@render tableMenu(n.data.name)}
          {:else if n.kind === 'table' || n.kind === 'view' || n.kind === 'matview'}
            <ContextMenu.Item onSelect={() => openTable(n.data.schema, n.data.name)}>
              <Icon name="external-link" />
              {n.kind === 'table' ? 'Open table' : 'Open rows'}
            </ContextMenu.Item>
            <ContextMenu.Item onSelect={() => void copyText(n.data.name)}>
              <Icon name="clipboard-copy" />
              Copy name
            </ContextMenu.Item>
            <ContextMenu.Item onSelect={() => void copyText(`${n.data.schema}.${n.data.name}`)}>
              <Icon name="copy" />
              Copy qualified name
            </ContextMenu.Item>
            {#if n.kind !== 'table' && family}
              <ContextMenu.Item onSelect={() => void openDefinition(n.data.schema, { kind: n.kind, name: n.data.name }, onopenobjectsql)}>
                <Icon name="file-code" />
                Open definition
              </ContextMenu.Item>
            {/if}
            {#if n.data.schema !== activeSchema}
              <ContextMenu.Separator />
              <ContextMenu.Item onSelect={() => onschemachange(n.data.schema)}>
                <Icon name="box" />
                Switch to schema {n.data.schema}
              </ContextMenu.Item>
            {/if}
          {:else if n.kind === 'object'}
            <ContextMenu.Item onSelect={() => void openDefinition(n.data.schema, n.data.obj, onopenobjectsql)}>
              <Icon name="file-code" />
              Open definition
            </ContextMenu.Item>
            <ContextMenu.Item onSelect={() => void copyText(n.label)}>
              <Icon name="clipboard-copy" />
              Copy name
            </ContextMenu.Item>
          {:else if n.kind === 'column'}
            <ContextMenu.Item onSelect={() => openTable(n.data.schema, n.data.table)}>
              <Icon name="external-link" />
              Open {n.data.table}
            </ContextMenu.Item>
            <ContextMenu.Item onSelect={() => void copyText(n.label)}>
              <Icon name="clipboard-copy" />
              Copy name
            </ContextMenu.Item>
          {:else}
            {@const ownSchema = schemaOf(n)}
            {#if n.kind === 'group' && canCreateKind(groupKind(n.data.group))}
              {@render newItem(groupKind(n.data.group), n.data.schema, false)}
              <ContextMenu.Separator />
            {/if}
            {#if n.kind === 'connection'}
              <ContextMenu.Item onSelect={() => togglePinned(n.data.conn)}>
                <Icon name={n.data.pinned ? 'pin-off' : 'pin'} />
                {n.data.pinned ? 'Unpin connection' : 'Pin connection'}
              </ContextMenu.Item>
            {/if}
            {#if opensBySwitching(n)}
              <ContextMenu.Item onSelect={() => toggleNode(n, true)}>
                <Icon name={n.kind === 'connection' ? 'plug' : 'database'} />
                {n.kind === 'connection' ? 'Connect' : 'Switch to this database'}
              </ContextMenu.Item>
            {:else if n.branch}
              <ContextMenu.Item onSelect={() => toggleNode(n, !open)}>
                <Icon name={open ? 'chevrons-down-up' : 'chevron-right'} />
                {open ? 'Collapse' : 'Expand'}
              </ContextMenu.Item>
            {/if}
            {#if n.kind === 'schema' && n.data.schema !== activeSchema}
              <ContextMenu.Item onSelect={() => onschemachange(n.data.schema)}>
                <Icon name="box" />
                Make active schema
              </ContextMenu.Item>
            {/if}
            {#if ownSchema !== null && sqlTables}
              <ContextMenu.Sub>
                <ContextMenu.SubTrigger>
                  <Icon name="plus" />
                  New
                </ContextMenu.SubTrigger>
                <ContextMenu.SubContent class="min-w-44">
                  {@render newItem('table', ownSchema, true)}
                  {#each templateKinds as kind (kind)}
                    {@render newItem(kind, ownSchema, true)}
                  {/each}
                  {#if supportsSchemas && n.kind === 'schema'}
                    <ContextMenu.Separator />
                    {@render adminItem('Schema', 'box', onnewschema)}
                  {/if}
                </ContextMenu.SubContent>
              </ContextMenu.Sub>
            {/if}
            {#if !opensBySwitching(n) && ((n.kind === 'database' && (dbAdmin || supportsSchemas)) || (n.kind === 'connection' && n.data.live && dbAdmin && !canSwitchDb))}
              {#if dbAdmin}{@render adminItem('New database', 'database', onnewdatabase)}{/if}
              {#if supportsSchemas && n.kind === 'database'}{@render adminItem('New schema', 'box', onnewschema)}{/if}
            {/if}
            <ContextMenu.Item onSelect={() => void copyText(n.label)}>
              <Icon name="clipboard-copy" />
              Copy name
            </ContextMenu.Item>
            {#if !opensBySwitching(n)}
              <ContextMenu.Separator />
              <ContextMenu.Item onSelect={refreshOpen}>
                <Icon name="refresh-cw" />
                Refresh
              </ContextMenu.Item>
            {/if}
          {/if}
        {/if}
      </ContextMenu.Content>
    </ContextMenu.Root>
  {/if}
</div>
