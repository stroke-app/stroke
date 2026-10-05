<script>
  import GitCompare from '@lucide/svelte/icons/git-compare'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import Loader2 from '@lucide/svelte/icons/loader-2'
  import ArrowUpDown from '@lucide/svelte/icons/arrow-up-down'
  import Plus from '@lucide/svelte/icons/plus'
  import Minus from '@lucide/svelte/icons/minus'
  import X from '@lucide/svelte/icons/x'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import Search from '@lucide/svelte/icons/search'
  import { onMount, untrack } from 'svelte'
  import { cn } from '$lib/utils.js'
  import CodeEditor from './CodeEditor.svelte'
  import {
    getTableRows,
    executeSql,
    executeSqlOnConnection,
    listSchemasOnConnection,
    listTablesOnConnection,
  } from '$lib/api.js'

  /**
   * @typedef {import('$lib/stores/connections.js').SavedConnection} SavedConnection
   * @typedef {{ name: string, dataType?: string }} ColInfo
   * @typedef {{ status: 'added'|'removed'|'modified'|'unchanged', left: unknown[]|null, right: unknown[]|null, changedCols?: Set<number> }} DiffRow
   * @typedef {{
   *   connId: string, database: string, databases: string[], loadingDbs: boolean,
   *   schema: string, schemas: string[], loadingSchemas: boolean,
   *   table: string, tables: string[], loadingTables: boolean,
   *   mode: 'table'|'sql', sql: string, error: string,
   * }} SourceState
   *
   * @type {{
   *   schemas: string[], tables: Array<{ name: string }>, activeSchema: string,
   *   connections: SavedConnection[], currentConnectionId: string,
   * }}
   */
  let { schemas, tables, activeSchema, connections = [], currentConnectionId = '' } = $props()

  // svelte-ignore state_referenced_locally
  const _init = activeSchema

  // System/catalog schemas that sort ahead of the user's real schema (e.g.
  // Cockroach's crdb_internal, PG's pg_catalog) - picking schemas[0] would land
  // on one of these and every table lookup fails ("relation X does not exist").
  const SYSTEM_SCHEMAS = new Set([
    'crdb_internal', 'information_schema', 'pg_catalog', 'pg_toast', 'pg_extension',
    'sys', 'mysql', 'performance_schema', 'sys_catalog',
  ])
  /** Best default schema: prefer `public`, then any non-system schema, else the first. */
  function pickDefaultSchema(/** @type {string[]} */ list) {
    if (!list.length) return ''
    if (list.includes('public')) return 'public'
    return list.find((s) => !SYSTEM_SCHEMAS.has(s.toLowerCase())) ?? list[0]
  }

  /** @param {string} connId @param {string} database @param {string} schema @returns {SourceState} */
  function makeSource(connId, database, schema) {
    return { connId, database, databases: [], loadingDbs: false, schema, schemas: [], loadingSchemas: false, table: '', tables: [], loadingTables: false, mode: 'table', sql: '', error: '' }
  }

  /** @type {SourceState} */
  // svelte-ignore state_referenced_locally
  let L = $state(makeSource(currentConnectionId, '', _init))
  /** @type {SourceState} */
  // svelte-ignore state_referenced_locally
  let R = $state(makeSource(currentConnectionId, '', _init))

  // ── Dropdown ──────────────────────────────────────────────────────────────────
  let openDropdown = $state('')
  let dropdownSearch = $state('')
  let ddTop = $state(0)
  let ddLeft = $state(0)
  let ddFlipUp = $state(false)
  const DD_HEIGHT = 280

  /** @param {string} id @param {MouseEvent} e */
  function openDd(id, e) {
    const rect = /** @type {HTMLElement} */ (e.currentTarget).getBoundingClientRect()
    const spaceBelow = window.innerHeight - rect.bottom
    ddFlipUp = spaceBelow < DD_HEIGHT && rect.top > DD_HEIGHT
    ddTop = ddFlipUp ? rect.top - 4 : rect.bottom + 4
    ddLeft = Math.min(rect.left, window.innerWidth - 248)
    openDropdown = id
    dropdownSearch = ''
  }
  function closeDd() { openDropdown = ''; dropdownSearch = '' }

  /** @param {HTMLElement} node */
  function focusNode(node) { setTimeout(() => node.focus(), 0) }

  // ── Key columns ───────────────────────────────────────────────────────────────
  /** @type {string[]} */
  let keyColSuggestions = $state([])
  /** @type {Set<string>} */
  let selectedKeyCols = $state(new Set())

  // ── Results ───────────────────────────────────────────────────────────────────
  let comparing = $state(false)
  let error = $state('')
  /** @type {ColInfo[]} */
  let columns = $state([])
  /** @type {DiffRow[]} */
  let diffRows = $state([])

  // ── Filter: single-select radio ───────────────────────────────────────────────
  /** @type {'all'|'changed'|'added'|'modified'|'removed'|'unchanged'} */
  let activeFilter = $state('changed')
  let searchQuery = $state('')
  // Debounced copy that feeds the row filter - filtering rescans every cell of
  // every diff row (up to ~20k × N cols), far too heavy to run per keystroke.
  let debouncedQuery = $state('')
  $effect(() => {
    const q = searchQuery
    const id = setTimeout(() => { debouncedQuery = q }, 150)
    return () => clearTimeout(id)
  })

  // ── Virtual scroll ────────────────────────────────────────────────────────────
  const ROW_HEIGHT = 36
  const BUFFER = 10
  let scrollTop = $state(0)
  let clientHeight = $state(600)
  let scrollRaf = 0

  function handleTableScroll(/** @type {Event} */ e) {
    const el = /** @type {HTMLElement} */ (e.currentTarget)
    if (scrollRaf) return
    scrollRaf = requestAnimationFrame(() => {
      scrollTop = el.scrollTop
      clientHeight = el.clientHeight
      scrollRaf = 0
    })
  }

  // ── Column widths (resizable) ─────────────────────────────────────────────────
  let colWidths = $state(/** @type {number[]} */ ([]))
  $effect(() => { if (columns.length) colWidths = Array(columns.length).fill(200) })
  const totalWidth = $derived(24 + colWidths.reduce((a, b) => a + b, 0))

  let resizingCol = $state(-1)
  let resizeStartX = 0
  let resizeStartWidth = 0

  function startResize(/** @type {number} */ ci, /** @type {MouseEvent} */ e) {
    e.preventDefault(); e.stopPropagation()
    resizingCol = ci; resizeStartX = e.clientX; resizeStartWidth = colWidths[ci]
  }
  function onResizeMove(/** @type {MouseEvent} */ e) {
    if (resizingCol < 0) return
    const w = Math.max(80, resizeStartWidth + e.clientX - resizeStartX)
    colWidths = colWidths.map((v, i) => i === resizingCol ? w : v)
  }
  function onResizeUp() { resizingCol = -1 }

  const stats = $derived.by(() => {
    const s = { added: 0, removed: 0, modified: 0, unchanged: 0 }
    for (const r of diffRows) s[r.status]++
    return s
  })

  const displayRows = $derived.by(() => {
    const q = debouncedQuery.trim().toLowerCase()
    return diffRows.filter((r) => {
      const pass =
        activeFilter === 'all' ? true :
        activeFilter === 'changed' ? r.status !== 'unchanged' :
        r.status === activeFilter
      if (!pass) return false
      if (!q) return true
      const cells = r.left ?? r.right ?? []
      return cells.some((c) => c !== null && String(c).toLowerCase().includes(q))
    })
  })

  const vStart = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - BUFFER))
  const vEnd = $derived(Math.min(displayRows.length, Math.ceil((scrollTop + clientHeight) / ROW_HEIGHT) + BUFFER))
  const topPad = $derived(vStart * ROW_HEIGHT)
  const bottomPad = $derived(Math.max(0, (displayRows.length - vEnd) * ROW_HEIGHT))

  // ── Helpers ───────────────────────────────────────────────────────────────────
  const connById = (/** @type {string} */ id) => connections.find((c) => c.id === id) ?? null
  const isCurrent = (/** @type {string} */ id) => id === currentConnectionId

  /** @param {SourceState} src @returns {SavedConnection|null} */
  function effectiveConfig(src) {
    const conn = connById(src.connId)
    if (!conn) return null
    if (src.database && src.database !== conn.database) return /** @type {SavedConnection} */ ({ ...conn, database: src.database })
    return conn
  }

  /** @param {SavedConnection} conn */
  async function fetchDatabases(conn) {
    const t = conn.type
    if (t === 'sqlite' || t === 'd1' || t === 'libsql') return ['main']
    try {
      const q = t === 'postgres'
        ? "SELECT datname FROM pg_database WHERE datallowconn=true AND datname NOT IN ('template0','template1') ORDER BY datname"
        : 'SHOW DATABASES'
      const res = isCurrent(conn.id) ? await executeSql(q) : await executeSqlOnConnection(conn, q)
      return (res.rows ?? []).map((r) => String(r[0]))
    } catch { return conn.database ? [conn.database] : [] }
  }

  onMount(() => {
    const cL = connById(L.connId), cR = connById(R.connId)
    if (cL) void loadDatabases({ ...L }, cL, (s) => { L = s })
    if (cR) void loadDatabases({ ...R }, cR, (s) => { R = s })
  })

  async function loadDatabases(/** @type {SourceState} */ src, /** @type {SavedConnection} */ conn, /** @type {(s:SourceState)=>void} */ set) {
    set({ ...src, loadingDbs: true, databases: [], database: '', schemas: [], schema: '', tables: [], table: '' })
    const dbs = await fetchDatabases(conn).catch(() => /** @type {string[]} */ ([]))
    const defaultDb = conn.database || dbs[0] || ''
    const next = { ...src, loadingDbs: false, databases: dbs, database: defaultDb, schemas: [], schema: '', tables: [], table: '' }
    set(next)
    await loadSchemas(next, dbs.length ? { ...conn, database: defaultDb } : conn, set)
  }

  async function loadSchemas(/** @type {SourceState} */ src, /** @type {SavedConnection} */ cfg, /** @type {(s:SourceState)=>void} */ set) {
    if (!cfg) return
    if (isCurrent(src.connId) && cfg.database === connById(src.connId)?.database) {
      const schema = pickDefaultSchema(schemas)
      const next = { ...src, schemas, schema, tables: [], table: '', loadingSchemas: false }
      set(next); await loadTables(next, cfg, set); return
    }
    set({ ...src, loadingSchemas: true, schemas: [], schema: '', tables: [], table: '', error: '' })
    try {
      const s = await listSchemasOnConnection(cfg)
      const schema = pickDefaultSchema(s)
      const next = { ...src, loadingSchemas: false, schemas: s, schema, tables: [], table: '', error: '' }
      set(next)
      if (schema) await loadTables(next, cfg, set)
    } catch (e) {
      // Swallowing this left an empty list, and an empty list renders as a chip
      // with `pointer-events-none` - indistinguishable from "this connection has
      // no schemas" and unclickable either way. A dropped connection is the
      // common cause and the one worth naming.
      set({ ...src, loadingSchemas: false, error: `Could not list schemas: ${String(e)}` })
    }
  }

  async function loadTables(/** @type {SourceState} */ src, /** @type {SavedConnection} */ cfg, /** @type {(s:SourceState)=>void} */ set) {
    if (!src.schema || !cfg) return
    // The `tables` prop is the OPEN connection's list for the schema it is
    // currently on, so reusing it also requires the schema to match - without
    // that check, picking a different schema on the current connection listed
    // the active schema's tables under it.
    if (isCurrent(src.connId) && cfg.database === connById(src.connId)?.database && src.schema === activeSchema) {
      set({ ...src, loadingTables: false, tables: tables.map((t) => t.name), error: '' }); return
    }
    set({ ...src, loadingTables: true, tables: [], table: '', error: '' })
    try {
      const t = await listTablesOnConnection(cfg, src.schema)
      set({ ...src, loadingTables: false, tables: t, table: '', error: '' })
    } catch (e) {
      set({ ...src, loadingTables: false, error: `Could not list tables in ${src.schema}: ${String(e)}` })
    }
  }

  function onConnChange(/** @type {'L'|'R'} */ side) {
    const src = side === 'L' ? L : R, set = side === 'L' ? (s) => { L = s } : (s) => { R = s }
    const conn = connById(src.connId)
    if (conn) void loadDatabases(src, conn, set)
  }
  function onDatabaseChange(/** @type {'L'|'R'} */ side) {
    const src = side === 'L' ? L : R, set = side === 'L' ? (s) => { L = s } : (s) => { R = s }
    const conn = connById(src.connId)
    if (conn) void loadSchemas(src, /** @type {SavedConnection} */ ({ ...conn, database: src.database }), set)
  }
  function onSchemaChange(/** @type {'L'|'R'} */ side) {
    const src = side === 'L' ? L : R, set = side === 'L' ? (s) => { L = s } : (s) => { R = s }
    const conn = connById(src.connId)
    if (conn) void loadTables(src, /** @type {SavedConnection} */ ({ ...conn, database: src.database }), set)
  }

  /** True while either side is re-listing, so the button can show it. */
  let refreshing = $state(false)
  const modKey = typeof navigator !== 'undefined' && /mac/i.test(navigator.platform) ? '\u2318' : 'Ctrl+'

  /**
   * Re-list databases, schemas and tables for both sides from scratch.
   *
   * Exported so the global Mod+R lands here like it does on every other page.
   * Rebuilds from the connection down rather than retrying the one call that
   * failed: a dropped connection usually takes the whole chain with it, and the
   * chips below the failure are stale anyway. Selections are preserved where the
   * new lists still contain them - see restore() below.
   */
  export async function refresh() {
    if (refreshing) return
    refreshing = true
    try {
      const keep = { L: { db: L.database, schema: L.schema, table: L.table }, R: { db: R.database, schema: R.schema, table: R.table } }
      await Promise.all(/** @type {const} */ (['L', 'R']).map(async (side) => {
        const src = side === 'L' ? L : R
        const set = side === 'L' ? (/** @type {SourceState} */ s2) => { L = s2 } : (/** @type {SourceState} */ s2) => { R = s2 }
        const conn = connById(src.connId)
        if (!conn) return
        await loadDatabases(src, conn, set)
        // Put back what the user had chosen, if it still exists.
        const now = side === 'L' ? L : R
        const want = keep[side]
        if (want.db && now.databases.includes(want.db) && now.database !== want.db) {
          const next = { ...now, database: want.db }
          set(next)
          await loadSchemas(next, /** @type {SavedConnection} */ ({ ...conn, database: want.db }), set)
        }
        const afterDb = side === 'L' ? L : R
        if (want.schema && afterDb.schemas.includes(want.schema) && afterDb.schema !== want.schema) {
          const next = { ...afterDb, schema: want.schema }
          set(next)
          await loadTables(next, /** @type {SavedConnection} */ ({ ...conn, database: afterDb.database }), set)
        }
        const afterSchema = side === 'L' ? L : R
        if (want.table && afterSchema.tables.includes(want.table)) {
          set({ ...afterSchema, table: want.table })
        }
      }))
    } finally {
      refreshing = false
    }
  }

  function swapSources() {
    const tmp = L; L = R; R = tmp
  }

  // Source/target share one input mode - a single toggle drives both.
  const mode = $derived(L.mode)
  function setMode(/** @type {'table'|'sql'} */ m) {
    L = { ...L, mode: m }
    R = { ...R, mode: m }
  }

  $effect(() => {
    if (!columns.length) return
    const suggested = columns.map((c) => c.name).filter((n) => {
      const lo = n.toLowerCase()
      return lo === 'id' || lo === 'uuid' || lo === 'uid' || lo.endsWith('_id') || lo === 'key'
    })
    const next = suggested.length ? suggested : columns.slice(0, 2).map((c) => c.name)
    keyColSuggestions = next
    if (next.length && untrack(() => selectedKeyCols.size === 0)) selectedKeyCols = new Set([next[0]])
  })

  function toggleKeyCol(/** @type {string} */ col) {
    const next = new Set(selectedKeyCols)
    if (next.has(col)) { if (next.size > 1) next.delete(col) } else next.add(col)
    selectedKeyCols = next
  }

  async function runFetch(/** @type {SourceState} */ src) {
    if (src.mode === 'sql') {
      const res = isCurrent(src.connId) ? await executeSql(src.sql) : await executeSqlOnConnection(effectiveConfig(src), src.sql)
      return { columns: res.columns ?? [], rows: res.rows ?? [] }
    }
    if (isCurrent(src.connId) && effectiveConfig(src)?.database === connById(src.connId)?.database) {
      const res = await getTableRows(src.schema, src.table, 10000, 0)
      return { columns: res.columns ?? [], rows: res.rows ?? [] }
    }
    const sql = `SELECT * FROM "${src.schema}"."${src.table}" LIMIT 10000`
    const res = await executeSqlOnConnection(effectiveConfig(src), sql)
    return { columns: res.columns ?? [], rows: res.rows ?? [] }
  }

  async function compare() {
    if (comparing) return
    error = ''
    const lReady = L.mode === 'table' ? !!L.table : !!L.sql.trim()
    const rReady = R.mode === 'table' ? !!R.table : !!R.sql.trim()
    if (!lReady || !rReady) { error = 'Configure both sources first.'; return }
    comparing = true
    try {
      const [lRes, rRes] = await Promise.all([runFetch(L), runFetch(R)])
      columns = lRes.columns.length ? lRes.columns : rRes.columns
      const keyNames = selectedKeyCols.size ? [...selectedKeyCols] : null
      const keyIdx = keyNames ? keyNames.map((n) => columns.findIndex((c) => c.name === n)).filter((i) => i >= 0) : [0]
      if (!keyIdx.length) { error = 'Key columns not found in result set.'; return }
      diffRows = await computeDiff(lRes.rows, rRes.rows, columns.length, keyIdx)
      activeFilter = 'changed'
      searchQuery = ''
      debouncedQuery = ''
    } catch (e) {
      error = String(/** @type {Error} */ (e).message ?? e)
    } finally { comparing = false }
  }

  // Compare a single cell: primitives as text, non-primitives via JSON so
  // JSON/array/object cells don't all collapse to `[object Object]`.
  function cellCompareText(/** @type {unknown} */ v) {
    if (v === null || v === undefined) return ''
    if (typeof v !== 'object') return String(v)
    try { return JSON.stringify(v) } catch { return String(v) }
  }

  async function computeDiff(/** @type {unknown[][]} */ lRows, /** @type {unknown[][]} */ rRows, /** @type {number} */ colCount, /** @type {number[]} */ keyIdx) {
    const key = (/** @type {unknown[]} */ r) => keyIdx.map((i) => cellCompareText(r[i])).join('\0')
    // Bucket rows per key so duplicate keys aren't silently dropped; pair the
    // buckets positionally (index 0↔0, 1↔1, …), extras become added/removed.
    /** @type {Map<string, unknown[][]>} */
    const lMap = new Map()
    for (const r of lRows) { const k = key(r); (lMap.get(k) ?? lMap.set(k, []).get(k)).push(r) }
    /** @type {Map<string, unknown[][]>} */
    const rMap = new Map()
    for (const r of rRows) { const k = key(r); (rMap.get(k) ?? rMap.set(k, []).get(k)).push(r) }
    /** @type {DiffRow[]} */
    const out = []
    let processed = 0
    for (const [k, lBucket] of lMap) {
      // Yield to the event loop periodically so a 10k+10k compare doesn't
      // freeze the UI - compare() keeps the spinner up around the await.
      if (++processed % 500 === 0) await new Promise((r) => setTimeout(r))
      const rBucket = rMap.get(k) ?? []
      const n = Math.max(lBucket.length, rBucket.length)
      for (let p = 0; p < n; p++) {
        const lr = lBucket[p]
        const rr = rBucket[p]
        if (!rr) { out.push({ status: 'removed', left: lr, right: null }); continue }
        if (!lr) { out.push({ status: 'added', left: null, right: rr }); continue }
        const changed = new Set()
        for (let i = 0; i < colCount; i++) if (cellCompareText(lr[i]) !== cellCompareText(rr[i])) changed.add(i)
        out.push({ status: changed.size ? 'modified' : 'unchanged', left: lr, right: rr, changedCols: changed })
      }
    }
    for (const [k, rBucket] of rMap) if (!lMap.has(k)) for (const rr of rBucket) out.push({ status: 'added', left: null, right: rr })
    out.sort((a, b) => ({ removed: 0, modified: 1, added: 2, unchanged: 3 }[a.status] - { removed: 0, modified: 1, added: 2, unchanged: 3 }[b.status]))
    return out
  }

  /**
   * @param {string} text
   * @param {string} query
   * @returns {Array<{text:string, match:boolean}>}
   */
  function splitHighlight(text, query) {
    if (!query) return [{ text, match: false }]
    const out = [], lo = text.toLowerCase(), ql = query.toLowerCase()
    let i = 0
    while (i < text.length) {
      const idx = lo.indexOf(ql, i)
      if (idx === -1) { out.push({ text: text.slice(i), match: false }); break }
      if (idx > i) out.push({ text: text.slice(i, idx), match: false })
      out.push({ text: text.slice(idx, idx + ql.length), match: true })
      i = idx + ql.length
    }
    return out
  }
</script>

<svelte:window
  onmousemove={onResizeMove}
  onmouseup={onResizeUp}
/>

<!-- Dropdown backdrop -->
{#if openDropdown}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-30" onpointerdown={closeDd}></div>
{/if}

<!-- ── Searchable select ──────────────────────────────────────────────────────── -->
{#snippet SearchSelect({ id, value, options, loading, placeholder = 'select…', onchange })}
  {@const isOpen = openDropdown === id}
  {@const filtered = dropdownSearch ? options.filter((o) => o.toLowerCase().includes(dropdownSearch.toLowerCase())) : options}
  <div class="relative inline-flex">
    <button
      type="button"
      onclick={(e) => { e.stopPropagation(); if (isOpen) closeDd(); else if (!loading && options.length > 0) openDd(id, e) }}
      disabled={loading || (!value && options.length === 0 && !loading)}
      class={cn(
        'flex h-7 items-center gap-1.5 rounded-md border px-2.5 text-ui-xs transition-colors select-none',
        loading
          ? 'border-border/30 bg-muted/15 text-muted-foreground'
          : value
            ? 'border-border/40 bg-muted/25 text-foreground/90 hover:bg-muted/45'
            : options.length
              ? 'border-dashed border-border/40 bg-transparent text-muted-foreground hover:border-border/60 hover:text-muted-foreground'
              : 'cursor-default border-border/20 bg-transparent text-muted-foreground pointer-events-none',
        isOpen && 'border-border/70 bg-muted/50 text-foreground',
      )}
    >
      {#if loading}
        <Loader2 class="size-3 animate-spin text-muted-foreground" />
        <span class="text-muted-foreground">{placeholder}</span>
      {:else}
        <span class={cn('max-w-[140px] truncate', value && 'font-medium')}>{value || placeholder}</span>
        {#if options.length > 0 || value}
          <ChevronDown class={cn('ml-auto size-3 shrink-0 text-muted-foreground transition-transform', isOpen && 'rotate-180')} />
        {/if}
      {/if}
    </button>
    {#if isOpen}
      <div
        role="presentation"
        class={cn('fixed z-40 w-56 overflow-hidden rounded-[10px] border border-border/60 bg-popover elevate-2-rim', ddFlipUp && '-translate-y-full')}
        style="top:{ddTop}px;left:{ddLeft}px"
        onpointerdown={(e) => e.stopPropagation()}
      >
        <div class="flex items-center gap-2 border-b border-border/25 px-3 py-2">
          <Search class="size-3 shrink-0 text-muted-foreground" />
          <input use:focusNode type="text" bind:value={dropdownSearch} placeholder="Search…"
            class="no-focus-ring flex-1 bg-transparent text-ui-xs outline-none placeholder:text-muted-foreground" />
          {#if dropdownSearch}<button onclick={() => { dropdownSearch = '' }} class="text-muted-foreground hover:text-foreground"><X class="size-3" /></button>{/if}
        </div>
        <div class="max-h-52 overflow-y-auto py-1">
          {#if filtered.length === 0}
            <p class="px-3 py-2 text-ui-xs text-muted-foreground">No results</p>
          {:else}
            {#each filtered as opt}
              <button type="button" onclick={() => { onchange(opt); closeDd() }}
                class={cn('flex w-full items-center gap-2 px-3 py-1.5 text-ui-xs transition-colors hover:bg-muted/35', value === opt && 'text-primary')}
              >
                <span class={cn('w-3 shrink-0 text-center text-ui-3xs', value === opt ? 'text-primary' : 'opacity-0')}>✓</span>
                {opt}
              </button>
            {/each}
          {/if}
        </div>
      </div>
    {/if}
  </div>
{/snippet}

<!-- ── Connection select ──────────────────────────────────────────────────────── -->
{#snippet ConnSelect({ id, value, onchange })}
  {@const isOpen = openDropdown === id}
  {@const selected = connections.find((c) => c.id === value)}
  {@const filtered = dropdownSearch ? connections.filter((c) => c.name.toLowerCase().includes(dropdownSearch.toLowerCase())) : connections}
  <div class="relative inline-flex">
    <button type="button"
      onclick={(e) => { e.stopPropagation(); if (isOpen) closeDd(); else openDd(id, e) }}
      class={cn('flex h-7 max-w-[200px] items-center gap-1.5 rounded-md border border-border/40 bg-muted/25 px-2.5 text-ui-xs font-medium text-foreground/90 transition-colors hover:bg-muted/45 select-none', isOpen && 'border-border/70 bg-muted/50 text-foreground')}
    >
      {#if value === currentConnectionId}<span class="size-1.5 shrink-0 rounded-full bg-info" title="Active connection"></span>{/if}
      <span class="max-w-[140px] truncate">{selected?.name ?? 'Connection'}</span>
      <ChevronDown class={cn('ml-auto size-3 shrink-0 text-muted-foreground transition-transform', isOpen && 'rotate-180')} />
    </button>
    {#if isOpen}
      <div
        role="presentation"
        class={cn('fixed z-40 w-64 overflow-hidden rounded-[10px] border border-border/60 bg-popover elevate-2-rim', ddFlipUp && '-translate-y-full')}
        style="top:{ddTop}px;left:{ddLeft}px"
        onpointerdown={(e) => e.stopPropagation()}
      >
        <div class="flex items-center gap-2 border-b border-border/25 px-3 py-2">
          <Search class="size-3 shrink-0 text-muted-foreground" />
          <input use:focusNode type="text" bind:value={dropdownSearch} placeholder="Search connections…"
            class="no-focus-ring flex-1 bg-transparent text-ui-xs outline-none placeholder:text-muted-foreground" />
        </div>
        <div class="max-h-52 overflow-y-auto py-1">
          {#if filtered.length === 0}
            <p class="px-3 py-2 text-ui-xs text-muted-foreground">No results</p>
          {:else}
            {#each filtered as conn}
              <button type="button" onclick={() => { onchange(conn.id); closeDd() }}
                class={cn('flex w-full items-center gap-2 px-3 py-2 text-ui-xs transition-colors hover:bg-muted/35', value === conn.id && 'text-primary')}
              >
                <span class={cn('w-3 shrink-0 text-center text-ui-3xs', value === conn.id ? 'text-primary' : 'opacity-0')}>✓</span>
                <span class="flex-1 truncate text-left">{conn.name}</span>
                {#if conn.id === currentConnectionId}<span class="shrink-0 text-ui-3xs text-info">active</span>{/if}
              </button>
            {/each}
          {/if}
        </div>
      </div>
    {/if}
  </div>
{/snippet}

<!-- ── Mode toggle ────────────────────────────────────────────────────────────── -->
{#snippet ModeToggle({ mode, onset })}
  <div class="flex shrink-0 items-center rounded-md border border-border/40 bg-muted/15 p-0.5 text-ui-2xs">
    <button onclick={() => onset('table')}
      class={cn('rounded px-2 py-0.5 font-medium transition-colors', mode === 'table' ? 'bg-muted text-foreground' : 'text-muted-foreground hover:text-muted-foreground')}>Table</button>
    <button onclick={() => onset('sql')}
      class={cn('rounded px-2 py-0.5 font-medium transition-colors', mode === 'sql' ? 'bg-muted text-foreground' : 'text-muted-foreground hover:text-muted-foreground')}>SQL</button>
  </div>
{/snippet}

<div class="flex h-full min-h-0 flex-col overflow-hidden">

  <!-- ══ Config ════════════════════════════════════════════════════════════════ -->
  <div class="shrink-0 border-b border-border/30 bg-background">

    <div class="flex items-stretch">
      <!-- Source / Target stacked -->
      <div class="min-w-0 flex-1">

        <!-- SOURCE row -->
        <div class="flex items-center gap-2.5 px-5 py-2.5">
          <span class="flex w-[74px] shrink-0 items-center gap-1.5">
            <span class="size-1.5 shrink-0 rounded-full bg-info"></span>
            <span class="text-ui-3xs font-semibold uppercase tracking-wider text-info">Source</span>
          </span>
          {@render ConnSelect({ id: 'L.conn', value: L.connId, onchange: (v) => { L = { ...L, connId: v }; onConnChange('L') } })}
          {#if L.mode === 'table'}
            {@render SearchSelect({ id: 'L.db', value: L.database, options: L.databases, loading: L.loadingDbs, placeholder: 'database', onchange: (v) => { L = { ...L, database: v }; onDatabaseChange('L') } })}
            {@render SearchSelect({ id: 'L.schema', value: L.schema, options: L.schemas, loading: L.loadingSchemas, placeholder: 'schema', onchange: (v) => { L = { ...L, schema: v, table: '', tables: [] }; onSchemaChange('L') } })}
            {@render SearchSelect({ id: 'L.table', value: L.table, options: L.tables, loading: L.loadingTables, placeholder: 'table', onchange: (v) => { L = { ...L, table: v } } })}
          {:else}
            <div class="flex min-h-[64px] flex-1 flex-col overflow-hidden rounded-md border border-border/40">
              <CodeEditor value={L.sql} lang="sql" gutter={false} folding={false} onchange={(t) => { L = { ...L, sql: t } }} ariaLabel="Left query" />
            </div>
          {/if}
        </div>
        {#if L.error}
          <!-- Named, not silent. An empty option list renders as an unclickable
               chip, so without this a dropped connection and "no tables here"
               looked identical - and neither said anything. -->
          <p class="px-5 pb-1 ps-[104px] font-mono text-ui-2xs text-destructive">{L.error}</p>
        {/if}

        <div class="relative flex items-center px-5">
          <div class="h-px flex-1 bg-border/10"></div>
          <button
            onclick={swapSources}
            title="Swap source and target"
            aria-label="Swap source and target"
            class="mx-2 flex size-6 items-center justify-center rounded-md border border-border/40 bg-muted/20 text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
          >
            <ArrowUpDown class="size-3" />
          </button>
          <div class="h-px flex-1 bg-border/10"></div>
        </div>

        <!-- TARGET row -->
        <div class="flex items-center gap-2.5 px-5 py-2.5">
          <span class="flex w-[74px] shrink-0 items-center gap-1.5">
            <span class="size-1.5 shrink-0 rounded-full bg-success"></span>
            <span class="text-ui-3xs font-semibold uppercase tracking-wider text-success">Target</span>
          </span>
          {@render ConnSelect({ id: 'R.conn', value: R.connId, onchange: (v) => { R = { ...R, connId: v }; onConnChange('R') } })}
          {#if R.mode === 'table'}
            {@render SearchSelect({ id: 'R.db', value: R.database, options: R.databases, loading: R.loadingDbs, placeholder: 'database', onchange: (v) => { R = { ...R, database: v }; onDatabaseChange('R') } })}
            {@render SearchSelect({ id: 'R.schema', value: R.schema, options: R.schemas, loading: R.loadingSchemas, placeholder: 'schema', onchange: (v) => { R = { ...R, schema: v, table: '', tables: [] }; onSchemaChange('R') } })}
            {@render SearchSelect({ id: 'R.table', value: R.table, options: R.tables, loading: R.loadingTables, placeholder: 'table', onchange: (v) => { R = { ...R, table: v } } })}
          {:else}
            <div class="flex min-h-[64px] flex-1 flex-col overflow-hidden rounded-md border border-border/40">
              <CodeEditor value={R.sql} lang="sql" gutter={false} folding={false} onchange={(t) => { R = { ...R, sql: t } }} ariaLabel="Right query" />
            </div>
          {/if}
        </div>
        {#if R.error}
          <!-- Named, not silent. An empty option list renders as an unclickable
               chip, so without this a dropped connection and "no tables here"
               looked identical - and neither said anything. -->
          <p class="px-5 pb-1 ps-[104px] font-mono text-ui-2xs text-destructive">{R.error}</p>
        {/if}

      </div>

      <!-- Single shared mode toggle for both rows -->
      <div class="flex shrink-0 items-center border-l border-border/15 px-4">
        {@render ModeToggle({ mode, onset: setMode })}
      </div>
    </div>

    <!-- Key cols + compare -->
    <div class="flex items-center gap-2.5 border-t border-border/15 px-5 py-2.5">
      <span class="w-[74px] shrink-0 text-ui-3xs font-semibold uppercase tracking-wider text-muted-foreground" title="Columns used to match rows between source and target">Key cols</span>
      <div class="flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
        {#each [...selectedKeyCols] as col}
          <button onclick={() => toggleKeyCol(col)}
            class="flex items-center gap-1 rounded-full bg-primary/10 px-2.5 py-0.5 text-ui-2xs font-medium text-primary transition-colors hover:bg-primary/20"
          >{col}<X class="size-2.5" /></button>
        {/each}
        {#each keyColSuggestions.filter((c) => !selectedKeyCols.has(c)) as col}
          <button onclick={() => toggleKeyCol(col)}
            class="flex items-center gap-1 rounded-full border border-dashed border-border/40 px-2.5 py-0.5 text-ui-2xs text-muted-foreground transition-colors hover:border-primary/40 hover:text-primary"
          ><Plus class="size-3" />{col}</button>
        {/each}
        {#if !keyColSuggestions.length && !selectedKeyCols.size}
          <span class="text-ui-2xs italic text-muted-foreground">auto, uses first column</span>
        {/if}
      </div>
      <!-- Beside Compare, not in the header: it re-lists what the pickers above
           offer, and this is the row you are on when one of them turns out to be
           empty. Same Mod+R the rest of the app uses. -->
      <button
        type="button"
        onclick={() => void refresh()}
        disabled={refreshing}
        title="Reload databases, schemas and tables ({modKey}R)"
        aria-label="Reload source and target lists"
        class="flex h-7 shrink-0 items-center gap-1.5 rounded-md border border-border/40 px-2.5 text-ui-xs text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground disabled:pointer-events-none disabled:opacity-50"
      >
        <RefreshCw class={cn('size-3.5', refreshing && 'animate-spin')} />
        Refresh
      </button>
      <button onclick={compare} disabled={comparing}
        class="flex h-7 shrink-0 items-center gap-1.5 rounded-md bg-primary px-4 text-ui-xs font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:pointer-events-none disabled:opacity-50"
      >
        {#if comparing}<Loader2 class="size-3.5 animate-spin" />{:else}<GitCompare class="size-3.5" />{/if}
        Compare
      </button>
    </div>

    {#if error}
      <div class="mx-5 mb-2 rounded-md bg-destructive/10 px-3 py-1.5 text-ui-xs text-destructive">{error}</div>
    {/if}
  </div>

  <!-- ══ Results ════════════════════════════════════════════════════════════════ -->
  {#if diffRows.length > 0}

    <!-- Filter + search bar (tab-style, Vercel/Resend inspired) ── -->
    <!-- The strip carries one continuous baseline and the tabs sit ON it. Each
         button used to draw its own `border-b-2` and nothing drew the line
         between them, so the active tab's underline floated in a gap with no
         rule to belong to - six stubs rather than a tab strip. -->
    <div class="flex shrink-0 items-end gap-0 border-b border-border/40 px-5">

      {#each [
        { key: 'all',       label: 'All',       count: diffRows.length,                          badge: 'bg-muted/30 text-muted-foreground',           active: 'text-foreground' },
        { key: 'changed',   label: 'Changed',   count: stats.added+stats.modified+stats.removed, badge: 'bg-muted/30 text-muted-foreground',           active: 'text-foreground' },
        { key: 'added',     label: 'Added',     count: stats.added,                              badge: 'bg-success/10 text-success',                     active: 'text-success' },
        { key: 'modified',  label: 'Modified',  count: stats.modified,                           badge: 'bg-warning/10 text-warning',                     active: 'text-warning' },
        { key: 'removed',   label: 'Removed',   count: stats.removed,                            badge: 'bg-destructive/10 text-destructive',             active: 'text-destructive' },
        { key: 'unchanged', label: 'Unchanged', count: stats.unchanged,                          badge: 'bg-muted/30 text-muted-foreground',           active: 'text-foreground' },
      ] as f}
        <button
          onclick={() => { activeFilter = f.key }}
          class={cn(
            // No underline marker. The active tab is carried by weight plus
            // colour, which is still two cues rather than colour alone: the
            // label goes medium AND takes its status colour, while every other
            // tab stays regular-weight muted. The count badge behind it picks up
            // the status tint too, so the selection reads at a glance without a
            // rule under it.
            'flex items-center gap-1.5 px-3 pb-2.5 pt-2 text-ui-xs transition-colors',
            activeFilter === f.key
              ? cn('font-medium', f.active)
              : 'text-muted-foreground hover:text-foreground'
          )}
        >
          {f.label}
          {#if f.count > 0}
            <span class={cn(
              'rounded px-1.5 py-0.5 font-mono text-ui-3xs leading-none transition-colors',
              activeFilter === f.key ? f.badge : 'bg-muted/30 text-muted-foreground'
            )}>{f.count}</span>
          {/if}
        </button>
      {/each}

      <!-- The icon, the field and the clear button are ONE control, so they sit
           in one frame. They used to be three siblings in a bare flex row: the
           app-wide field rule framed the <input> alone, leaving the magnifier
           stranded outside a pill it clearly belonged to.
           `ps-4`, not `pl-4` - this strip mirrors under RTL. -->
      <div class="ms-auto flex items-center gap-2 pb-2.5 ps-4">
        <div class="flex h-7 items-center gap-1.5 rounded-md border border-border/40 bg-muted/30 px-2 transition-colors focus-within:border-border">
          <Search class="size-3 shrink-0 text-muted-foreground" />
          <input type="text" bind:value={searchQuery} placeholder="Search rows…"
            aria-label="Search diff rows"
            class="no-focus-ring w-40 bg-transparent text-ui-xs outline-none placeholder:text-muted-foreground" />
          {#if searchQuery}
            <button onclick={() => { searchQuery = '' }} aria-label="Clear search"
              class="shrink-0 text-muted-foreground transition-colors hover:text-foreground">
              <X class="size-3" />
            </button>
          {/if}
        </div>
        <!-- Always on, never swapped out for the clear button. The count IS the
             result of the search, so hiding it the moment you search is backwards -
             and a control that changes width as you type moves everything after it. -->
        <span class="shrink-0 text-ui-2xs tabular-nums text-muted-foreground">
          {displayRows.length}{displayRows.length !== diffRows.length ? `/${diffRows.length}` : ''}
        </span>
      </div>
    </div>

    <!-- Virtual-scroll table ── -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="min-h-0 flex-1 overflow-auto"
      class:cursor-col-resize={resizingCol >= 0}
      onscroll={handleTableScroll}
    >
      <table class="w-full border-separate text-ui-xs" style="table-layout:fixed;min-width:{totalWidth}px;border-spacing:0">
        <colgroup>
          <col style="width:26px" />
          {#each colWidths as w}<col style="width:{w}px" />{/each}
          <!-- Flexible filler so the grid always fills the pane (no empty "hole") -->
          <col />
        </colgroup>
        <thead class="sticky top-0 z-20 bg-card">
          <tr>
            <th class="bg-card select-none"></th>
            {#each columns as col, ci}
              <th
                class="group relative bg-card p-0 text-left select-none overflow-hidden"
                style="width:{colWidths[ci]}px;min-width:{colWidths[ci]}px;max-width:{colWidths[ci]}px"
              >
                <div class="flex min-w-0 items-baseline gap-1.5 px-3 py-2.5 pr-5">
                  <span class="truncate font-semibold text-foreground/75 text-ui-2xs">{col.name}</span>
                  {#if col.dataType && col.dataType.toLowerCase() !== 'null'}<span class="shrink-0 font-normal text-muted-foreground text-ui-3xs">{col.dataType}</span>{/if}
                </div>
                <!-- Resize handle -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="absolute right-0 top-0 h-full w-4 cursor-col-resize"
                  onmousedown={(e) => startResize(ci, e)}
                >
                  <div class={cn('absolute right-1.5 top-1/2 h-3 w-px -translate-y-1/2 transition-opacity', resizingCol === ci ? 'bg-primary/60 opacity-100' : 'bg-border/40 opacity-0 group-hover:opacity-100')}></div>
                </div>
              </th>
            {/each}
            <th class="bg-card select-none"></th>
          </tr>
        </thead>
        <tbody>
          {#if topPad > 0}<tr style="height:{topPad}px"><td colspan={columns.length + 2}></td></tr>{/if}
          {#each displayRows.slice(vStart, vEnd) as row}
            {@const isAdded = row.status === 'added'}
            {@const isRemoved = row.status === 'removed'}
            {@const isModified = row.status === 'modified'}
            {@const rowBg =
              isAdded    ? 'bg-success/[0.04]' :
              isRemoved  ? 'bg-destructive/[0.04]' :
              isModified ? 'bg-warning/[0.025]' : ''}
            {@const statusGlyph =
              isAdded   ? '+' : isRemoved ? '−' : isModified ? '~' : ''}
            {@const statusColor =
              isAdded   ? 'text-success' :
              isRemoved ? 'text-destructive' :
              isModified? 'text-warning' :
              'text-muted-foreground'}
            {@const accentColor =
              isAdded   ? 'bg-success/35' :
              isRemoved ? 'bg-destructive/35' :
              isModified? 'bg-warning/25' : ''}
            <tr class="{rowBg || 'hover:bg-foreground/[0.025]'} transition-colors" style="height:{ROW_HEIGHT}px">
              <td class="relative select-none border-b border-border/8 px-2 text-center font-mono text-ui-3xs font-bold {statusColor}">
                {#if accentColor}<span class="absolute inset-y-0 left-0 w-[2px] {accentColor}"></span>{/if}
                {statusGlyph}
              </td>
              {#each columns as _col, ci}
                {@const isChanged = isModified && row.changedCols?.has(ci)}
                {@const oldVal = row.left?.[ci] ?? null}
                {@const newVal = row.right?.[ci] ?? null}
                {@const dispVal = isRemoved ? oldVal : newVal}
                {@const dispStr = dispVal === null ? '' : String(dispVal)}
                <td class="overflow-hidden border-b border-border/8 px-3 font-mono text-ui-2xs">
                  {#if isChanged}
                    <div class="flex min-w-0 items-center gap-1.5">
                      {#if oldVal === null}
                        <span class="shrink-0 text-ui-3xs italic text-destructive line-through">NULL</span>
                      {:else}
                        <span class="min-w-0 flex-1 truncate text-ui-3xs text-destructive line-through">{String(oldVal)}</span>
                      {/if}
                      <span class="shrink-0 font-sans text-ui-3xs text-muted-foreground">→</span>
                      {#if newVal === null}
                        <span class="shrink-0 text-ui-3xs italic text-success">NULL</span>
                      {:else}
                        <span class="min-w-0 flex-1 truncate text-success">{String(newVal)}</span>
                      {/if}
                    </div>
                  {:else if dispVal === null}
                    <span class="italic text-muted-foreground">NULL</span>
                  {:else if debouncedQuery}
                    <span class="block overflow-hidden text-ellipsis whitespace-nowrap {isAdded ? 'text-success' : isRemoved ? 'text-destructive' : ''}">
                      {#each splitHighlight(dispStr, debouncedQuery) as seg}
                        {#if seg.match}<mark class="rounded-[2px] bg-primary/20 text-primary not-italic">{seg.text}</mark>{:else}{seg.text}{/if}
                      {/each}
                    </span>
                  {:else}
                    <span class="block overflow-hidden text-ellipsis whitespace-nowrap {isAdded ? 'text-success' : isRemoved ? 'text-destructive' : ''}">{dispStr}</span>
                  {/if}
                </td>
              {/each}
              <td class="border-b border-border/8"></td>
            </tr>
          {/each}
          {#if bottomPad > 0}<tr style="height:{bottomPad}px"><td colspan={columns.length + 2}></td></tr>{/if}
        </tbody>
      </table>
    </div>

  {:else if comparing}
    <div class="flex flex-1 items-center justify-center">
      <Loader2 class="size-5 animate-spin text-muted-foreground" />
    </div>
  {:else}
    <div class="flex flex-1 flex-col items-center justify-center gap-4">
      <div class="flex size-14 items-center justify-center rounded-full border border-border/15 bg-muted/8">
        <GitCompare class="size-6 opacity-20" />
      </div>
      <div class="text-center">
        <p class="text-ui-sm font-medium text-foreground/40">Compare any two data sources</p>
        <p class="mt-1 text-ui-xs text-muted-foreground">Tables or SQL, even across different hosts</p>
      </div>
    </div>
  {/if}
</div>
