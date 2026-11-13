<script>
  import Camera from '@lucide/svelte/icons/camera'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import ArrowLeftRight from '@lucide/svelte/icons/arrow-left-right'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import Loader2 from '@lucide/svelte/icons/loader-2'
  import Check from '@lucide/svelte/icons/check'
  import X from '@lucide/svelte/icons/x'
  import Ellipsis from '@lucide/svelte/icons/ellipsis'
  import { onMount } from 'svelte'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { cn } from '$lib/utils.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import FileCode from '@lucide/svelte/icons/file-code'
  import Copy from '@lucide/svelte/icons/copy'
  import { buildMigration, formatMigrationScript } from '$lib/schema-migration.js'
  import { listSchemas, listSchemasOnConnection, executeSql, executeSqlOnConnection } from '$lib/api.js'
  import {
    captureSnapshot,
    listAllSnapshots,
    deleteSnapshot,
    deleteAllSnapshots,
    diffSnapshots,
    isDiffEmpty,
  } from '$lib/stores/schema-snapshots.js'

  /** @typedef {import('$lib/stores/schema-snapshots.js').SchemaSnapshot} SchemaSnapshot */
  /** @typedef {import('$lib/stores/schema-snapshots.js').SnapshotDiff} SnapshotDiff */
  /** @typedef {import('$lib/stores/schema-snapshots.js').SnapshotColumn} SnapshotColumn */
  /** @typedef {import('$lib/stores/connections.js').SavedConnection} SavedConnection */
  /** @typedef {{ type: 'same'|'add'|'remove', content: string, lineA?: number, lineB?: number }} DiffLine */
  /** @typedef {DiffLine | { type: 'ellipsis', count: number }} CollapsedLine */

  /**
   * @type {{
   *   connectionId: string,
   *   connectionLabel: string,
   *   dbType?: string,
   *   connections?: SavedConnection[],
   * }}
   */
  let { connectionId, connectionLabel, dbType = 'postgres', connections = [] } = $props()

  /** @type {SchemaSnapshot[]} */
  let snapshots = $state([])
  let capturing = $state(false)
  let loading = $state(true)

  /** @type {string | null} */
  let beforeId = $state(null)
  /** @type {string | null} */
  let afterId = $state(null)
  /** @type {string | null} */
  let selectedTable = $state(null)
  /** @type {Set<string>} */
  let collapsedSchemas = $state(new Set())

  // ── More menu ──────────────────────────────────────────────────────────────
  let moreMenuOpen = $state(false)
  let clearStep = $state(0)

  $effect(() => {
    if (!moreMenuOpen) clearStep = 0
  })

  // ── Capture modal ──────────────────────────────────────────────────────────
  let captureOpen = $state(false)
  let captureTitle = $state('')
  /** @type {SavedConnection | null} null = active */
  let captureConn = $state(null)
  /** @type {string[]} */
  let captureDatabases = $state([])
  let captureDb = $state('')
  let captureLoadingDbs = $state(false)
  /** @type {string[]} */
  let captureSchemaList = $state([])
  /** @type {Set<string>} */
  let captureChecked = $state(new Set())
  let captureLoadingSchemas = $state(false)

  const SYSTEM_SCHEMAS = new Set([
    'information_schema', 'pg_catalog', 'pg_toast', 'pg_temp_1', 'pg_toast_temp_1',
  ])

  const beforeSnap = $derived(snapshots.find((s) => s.id === beforeId) ?? null)
  const afterSnap = $derived(snapshots.find((s) => s.id === afterId)  ?? null)
  const diff = $derived(beforeSnap && afterSnap ? diffSnapshots(beforeSnap, afterSnap) : null)

  // ── Migration ──────────────────────────────────────────────────────────────
  // The diff already knows what changed; this renders it as SQL to review.
  let migrationOpen = $state(false)

  /** The dialect only decides syntax, so anything not MySQL/SQLite reads as Postgres. */
  const migrationDialect = $derived(
    dbType === 'mysql' ? 'mysql' : dbType === 'sqlite' ? 'sqlite' : 'postgres',
  )

  const migration = $derived(
    diff ? buildMigration(diff, { dialect: migrationDialect }) : null,
  )

  const migrationScript = $derived(
    migration
      ? formatMigrationScript(migration, {
          dialect: migrationDialect,
          title: `${beforeSnap?.title || 'earlier snapshot'} → ${afterSnap?.title || 'later snapshot'}`,
        })
      : '',
  )

  async function copyMigration() {
    try {
      await navigator.clipboard.writeText(migrationScript)
      toast.success('Migration copied')
    } catch (e) {
      toast.error('Could not copy', { description: String(e) })
    }
  }
  const totalChanges = $derived(
    diff ? diff.addedTables.length + diff.removedTables.length + diff.modifiedTables.length : 0,
  )

  $effect(() => {
    if (snapshots.length >= 2 && !beforeId && !afterId) {
      afterId = snapshots[0].id
      beforeId = snapshots[1].id
    }
  })

  $effect(() => {
    if (!diff) { selectedTable = null; return }
    const allKeys = [
      ...diff.addedTables.map((t) => `${t.schema}.${t.name}`),
      ...diff.modifiedTables.map((t) => `${t.schema}.${t.name}`),
      ...diff.removedTables.map((t) => `${t.schema}.${t.name}`),
    ]
    if (!selectedTable || !allKeys.includes(selectedTable)) {
      selectedTable = allKeys[0] ?? null
    }
  })

  const selectedDdlDiff = $derived.by(() => {
    if (!selectedTable || !beforeSnap || !afterSnap) return null
    const dot = selectedTable.indexOf('.')
    const schema = selectedTable.slice(0, dot)
    const name = selectedTable.slice(dot + 1)

    const bDdl = beforeSnap.ddl?.[selectedTable]
      ?? pseudoDdl(schema, name, (beforeSnap.tables?.[schema] ?? []).find((t) => t.name === name)?.columns ?? [])
    const aDdl = afterSnap.ddl?.[selectedTable]
      ?? pseudoDdl(schema, name, (afterSnap.tables?.[schema] ?? []).find((t) => t.name === name)?.columns ?? [])

    if (!bDdl && !aDdl) return null
    let lines
    if (!bDdl) {
      lines = aDdl.split('\n').map((content, i) => /** @type {DiffLine} */ ({ type: 'add',    content, lineB: i + 1 }))
    } else if (!aDdl) {
      lines = bDdl.split('\n').map((content, i) => /** @type {DiffLine} */ ({ type: 'remove', content, lineA: i + 1 }))
    } else {
      lines = computeLineDiff(bDdl.split('\n'), aDdl.split('\n'))
    }
    return collapseContext(lines, 3)
  })

  onMount(load)

  async function load() {
    loading = true
    try { snapshots = await listAllSnapshots() }
    finally { loading = false }
  }

  // ── Clear all ──────────────────────────────────────────────────────────────

  function onClearClick() {
    if (clearStep === 0) { clearStep = 1 }
    else { void doDeleteAll() }
  }

  async function doDeleteAll() {
    await deleteAllSnapshots()
    snapshots = []; beforeId = null; afterId = null
    moreMenuOpen = false; clearStep = 0
    toast.success('All snapshots deleted')
  }

  function swapSnapshots() {
    const tmp = beforeId
    beforeId = afterId
    afterId = tmp
  }

  // ── Capture modal ──────────────────────────────────────────────────────────

  async function openCaptureModal() {
    captureTitle = ''
    captureConn = null
    captureDb = ''
    captureOpen = true
    await loadCaptureData(null)
  }

  /** @param {SavedConnection | null} conn */
  async function selectCaptureConn(conn) {
    if (captureConn?.id === conn?.id || (captureConn === null && conn === null)) return
    captureConn = conn
    captureDb = ''
    captureSchemaList = []
    captureChecked = new Set()
    await loadCaptureData(conn)
  }

  /** @param {SavedConnection | null} conn */
  async function loadCaptureData(conn) {
    captureLoadingDbs = true
    captureDatabases = []
    captureSchemaList = []
    captureChecked = new Set()
    try {
      const dbs = await fetchDatabases(conn)
      captureDatabases = dbs
      const defaultDb = conn?.database || dbs[0] || ''
      captureDb = defaultDb
      if (defaultDb) await loadCaptureSchemas(conn, defaultDb)
    } finally {
      captureLoadingDbs = false
    }
  }

  /**
   * @param {SavedConnection | null} conn
   * @param {string} db
   */
  async function loadCaptureSchemas(conn, db) {
    captureLoadingSchemas = true
    captureSchemaList = []
    captureChecked = new Set()
    try {
      let cfg = null
      if (conn) {
        cfg = (db && db !== conn.database) ? { ...conn, database: db } : conn
      } else {
        // Active connection - if a different DB is selected, we must use a config override
        const activeConn = connections.find((c) => c.id === connectionId)
        if (activeConn && db && db !== activeConn.database) {
          cfg = { ...activeConn, database: db }
        }
      }
      const raw = cfg ? await listSchemasOnConnection(cfg) : await listSchemas()
      const filtered = (raw ?? []).filter((s) => !SYSTEM_SCHEMAS.has(s))
      captureSchemaList = filtered
      captureChecked = new Set(filtered)
    } catch {
      captureSchemaList = []
    } finally {
      captureLoadingSchemas = false
    }
  }

  /** @param {string} db */
  async function onCaptureDbChange(db) {
    captureDb = db
    await loadCaptureSchemas(captureConn, db)
  }

  /** @param {string} schema */
  function toggleCaptureSchema(schema) {
    const next = new Set(captureChecked)
    if (next.has(schema)) next.delete(schema)
    else next.add(schema)
    captureChecked = next
  }

  async function doCapture() {
    captureOpen = false
    if (capturing) return
    capturing = true
    const conn = captureConn
    const id = conn?.id ?? connectionId
    const label = conn?.name ?? connectionLabel
    const type = /** @type {string} */ (conn?.type ?? dbType)
    let cfgConn = null
    if (conn) {
      cfgConn = (captureDb && captureDb !== conn.database) ? { ...conn, database: captureDb } : conn
    } else {
      // Active connection - if a different DB was selected, pass a config with the override
      const activeConn = connections.find((c) => c.id === connectionId)
      if (activeConn && captureDb && captureDb !== activeConn.database) {
        cfgConn = { ...activeConn, database: captureDb }
      }
    }
    // Pass null when all schemas selected - avoids name-mismatch silently dropping schemas
    const schemasFilter = (captureChecked.size > 0 && captureChecked.size < captureSchemaList.length)
      ? captureChecked
      : null
    const title = captureTitle.trim()
    try {
      await captureSnapshot(id, label, type, cfgConn, schemasFilter, title)
      snapshots = await listAllSnapshots()
      toast.success('Snapshot captured')
    } catch (err) {
      toast.error(`Capture failed: ${/** @type {Error} */ (err).message}`)
    } finally {
      capturing = false
    }
  }

  /**
   * Fetch available databases for a connection.
   * @param {SavedConnection | null} conn
   * @returns {Promise<string[]>}
   */
  async function fetchDatabases(conn) {
    if (!conn) {
      const activeConn = connections.find((c) => c.id === connectionId)
      if (!activeConn) return []
      conn = activeConn
    }
    const t = conn.type
    if (t === 'sqlite' || t === 'd1' || t === 'libsql') return ['main']
    try {
      const q = t === 'postgres'
        ? "SELECT datname FROM pg_database WHERE datallowconn=true AND datname NOT IN ('template0','template1') ORDER BY datname"
        : 'SHOW DATABASES'
      const isActive = conn.id === connectionId
      const res = isActive ? await executeSql(q) : await executeSqlOnConnection(conn, q)
      return (res.rows ?? []).map((r) => String(r[0]))
    } catch {
      return conn.database ? [conn.database] : []
    }
  }

  /** @param {string} id */
  async function remove(id) {
    try {
      await deleteSnapshot(id)
    } catch (err) {
      toast.error(`Delete failed: ${/** @type {Error} */ (err).message}`)
      return
    }
    snapshots = snapshots.filter((s) => s.id !== id)
    if (beforeId === id) beforeId = null
    if (afterId === id) afterId = null
  }

  /** @param {number} ts */
  function fmtDate(ts) {
    return new Date(ts).toLocaleString(undefined, {
      month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
    })
  }

  function shortName(label) {
    if (!label) return '(unnamed)'
    if (label.includes('/')) { const p = label.split('/').pop(); if (p) return p }
    if (label.includes('.') && label.length > 30) return label.split('.')[0]
    return label
  }

  /** @param {string} key */
  function toggleSchemaGroup(key) {
    const next = new Set(collapsedSchemas)
    if (next.has(key)) next.delete(key)
    else next.add(key)
    collapsedSchemas = next
  }

  function pseudoDdl(schema, tableName, /** @type {SnapshotColumn[]} */ columns) {
    if (!columns.length) return ''
    const lines = columns.map((col) => {
      let line = `  "${col.name}" ${col.dataType}`
      if (col.defaultValue != null) line += ` DEFAULT ${col.defaultValue}`
      if (!col.nullable) line += ` NOT NULL`
      return line
    })
    return `CREATE TABLE "${schema}"."${tableName}" (\n${lines.join(',\n')}\n);`
  }

  function groupBySchema(/** @type {SnapshotDiff} */ d) {
    /** @type {Record<string, Array<{schema: string, name: string, status: 'added'|'removed'|'modified'}>>} */
    const groups = {}
    const push = (schema, name, status) => { ;(groups[schema] ??= []).push({ schema, name, status }) }
    for (const t of d.addedTables)   push(t.schema, t.name, 'added')
    for (const t of d.modifiedTables) push(t.schema, t.name, 'modified')
    for (const t of d.removedTables)  push(t.schema, t.name, 'removed')
    for (const list of Object.values(groups)) {
      const order = { added: 0, modified: 1, removed: 2 }
      list.sort((a, b) => (order[a.status] - order[b.status]) || a.name.localeCompare(b.name))
    }
    return groups
  }

  function getTableStatus(/** @type {SnapshotDiff | null} */ d, key) {
    if (!d) return /** @type {'modified'} */ ('modified')
    const dot = key.indexOf('.')
    const schema = key.slice(0, dot), name = key.slice(dot + 1)
    if (d.addedTables.some((t) => t.schema === schema && t.name === name))   return /** @type {'added'} */   ('added')
    if (d.removedTables.some((t) => t.schema === schema && t.name === name)) return /** @type {'removed'} */ ('removed')
    return /** @type {'modified'} */ ('modified')
  }

  function computeLineDiff(/** @type {string[]} */ a, /** @type {string[]} */ b) {
    // Normalize lines for comparison: strip trailing commas so that a column
    // gaining/losing a comma (due to being last vs. non-last) is not a diff.
    const norm = (/** @type {string} */ s) => s.trimEnd().replace(/,$/, '')
    const na = a.map(norm), nb = b.map(norm)
    const m = a.length, n = b.length
    // Pathological pairs (m×n in the millions) would allocate a huge LCS matrix
    // and stall the UI - render those as remove-all/add-all instead.
    if (m * n > 4_000_000) {
      return [
        ...a.map((content, i) => /** @type {DiffLine} */ ({ type: 'remove', content, lineA: i + 1 })),
        ...b.map((content, i) => /** @type {DiffLine} */ ({ type: 'add', content, lineB: i + 1 })),
      ]
    }
    // Typed-array rows (zero-initialized) - plain arrays box every slot.
    const dp = Array.from({ length: m + 1 }, () => new Uint32Array(n + 1))
    for (let i = 1; i <= m; i++)
      for (let j = 1; j <= n; j++)
        dp[i][j] = na[i-1] === nb[j-1] ? dp[i-1][j-1] + 1 : Math.max(dp[i-1][j], dp[i][j-1])
    /** @type {DiffLine[]} */
    const result = []
    let i = m, j = n
    while (i > 0 || j > 0) {
      if (i > 0 && j > 0 && na[i-1] === nb[j-1]) {
        result.unshift({ type: 'same', content: a[i-1], lineA: i, lineB: j }); i--; j--
      } else if (j > 0 && (i === 0 || dp[i][j-1] >= dp[i-1][j])) {
        result.unshift({ type: 'add',    content: b[j-1], lineB: j }); j--
      } else {
        result.unshift({ type: 'remove', content: a[i-1], lineA: i }); i--
      }
    }
    return result
  }

  function collapseContext(/** @type {DiffLine[]} */ lines, ctx) {
    const changeset = new Set()
    lines.forEach((l, i) => { if (l.type !== 'same') changeset.add(i) })
    if (changeset.size === 0) return lines
    const visible = new Set()
    for (const ci of changeset)
      for (let k = Math.max(0, ci - ctx); k <= Math.min(lines.length - 1, ci + ctx); k++)
        visible.add(k)
    /** @type {CollapsedLine[]} */
    const out = []
    let gap = 0
    for (let i = 0; i < lines.length; i++) {
      if (visible.has(i)) {
        if (gap > 0) out.push({ type: 'ellipsis', count: gap })
        gap = 0
        out.push(lines[i])
      } else { gap++ }
    }
    if (gap > 0) out.push({ type: 'ellipsis', count: gap })
    return out
  }
</script>

<!-- ════════════════════════════════════════════════════════════════════════ -->
<div class="flex h-full min-h-0 overflow-hidden">

  <!-- ── Sidebar ────────────────────────────────────────────────────────── -->
  <div class="flex w-64 shrink-0 flex-col overflow-hidden border-r border-border/30 bg-panel">

    <!-- Sidebar header -->
    <div class="flex h-12 shrink-0 items-center gap-2 px-4">
      <span class="text-ui-sm font-medium text-foreground">Snapshots</span>
      {#if snapshots.length > 0}
        <span class="rounded-full bg-muted px-1.5 text-ui-3xs tabular-nums text-muted-foreground">{snapshots.length}</span>
      {/if}
      <span class="flex-1"></span>

      {#if snapshots.length > 0}
        <div class="relative">
          {#if moreMenuOpen}
            <div class="fixed inset-0 z-10" role="presentation" onclick={() => { moreMenuOpen = false }}></div>
          {/if}
          <button
            onclick={() => { moreMenuOpen = !moreMenuOpen }}
            class="flex size-7 items-center justify-center rounded-full text-muted-foreground hover:bg-accent/50 hover:text-foreground"
            aria-label="More"
          >
            <Ellipsis class="size-3.5 shrink-0" />
          </button>
          {#if moreMenuOpen}
            <div class="absolute right-0 top-full z-20 mt-1 min-w-40 overflow-hidden rounded-[10px] border border-border/60 bg-popover p-1 elevate-2-rim">
              <button
                onclick={onClearClick}
                class={cn(
                  'flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-ui-xs hover:bg-accent',
                  clearStep === 1 ? 'text-destructive' : 'text-muted-foreground hover:text-destructive',
                )}
              >
                <Trash2 class="size-3.5 shrink-0" />
                {clearStep === 1 ? 'Confirm clear all' : 'Clear all'}
              </button>
            </div>
          {/if}
        </div>
      {/if}

      <button
        onclick={openCaptureModal}
        disabled={capturing}
        class="flex h-7 items-center gap-1.5 rounded-full bg-muted/60 px-3 text-ui-xs text-foreground/85 hover:bg-accent hover:text-foreground disabled:opacity-40"
        title="Capture snapshot"
      >
        {#if capturing}
          <Loader2 class="size-3.5 shrink-0 animate-spin" />
        {:else}
          <Camera class="size-3.5 shrink-0" />
        {/if}
        New
      </button>
    </div>

    <!-- Snapshot list -->
    <div class="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
      {#if loading}
        <div class="loading-reveal flex items-center justify-center py-12">
          <Loader2 class="size-3.5 shrink-0 animate-spin text-muted-foreground" />
        </div>

      {:else if snapshots.length === 0}
        <p class="px-2 py-6 text-ui-xs leading-relaxed text-muted-foreground">
          No snapshots yet. Capture the schema to start a timeline.
        </p>

      {:else}
        {#each snapshots as snap (snap.id)}
          {@const isBefore = snap.id === beforeId}
          {@const isAfter = snap.id === afterId}
          {@const isActive = isBefore || isAfter}
          {@const tableCount = Object.values(snap.tables ?? {}).flat().length}
          {@const displayName = snap.title || shortName(snap.connectionLabel)}

          <div class={cn(
            'group mb-0.5 rounded-lg px-2.5 py-2 transition-colors',
            isActive ? 'bg-accent' : 'hover:bg-accent/50',
          )}>
            <div class="flex min-w-0 items-center gap-2">
              <span class="min-w-0 flex-1 truncate text-ui-xs font-medium text-foreground" title={snap.connectionLabel}>
                {displayName}
              </span>
              <button
                onclick={() => remove(snap.id)}
                class="flex size-5 shrink-0 items-center justify-center rounded text-transparent group-hover:text-muted-foreground hover:!text-destructive"
                aria-label="Delete snapshot"
              >
                <X class="size-3 shrink-0" />
              </button>
            </div>
            <div class="mt-1 flex items-center gap-1.5 text-ui-2xs text-muted-foreground">
              <span class="truncate">{fmtDate(snap.capturedAt)}</span>
              <span aria-hidden="true">·</span>
              <span class="shrink-0 tabular-nums">{tableCount} tables</span>
              <span class="flex-1"></span>
              <!-- Base / Compare slots -->
              <button
                onclick={() => { beforeId = snap.id }}
                class={cn(
                  'h-5 shrink-0 rounded-full px-2 text-ui-3xs transition-colors',
                  isBefore ? 'bg-foreground text-background' : 'text-muted-foreground opacity-0 group-hover:opacity-100 hover:bg-background/60 hover:text-foreground',
                )}
                title="Use as the baseline"
              >Base</button>
              <button
                onclick={() => { afterId = snap.id }}
                class={cn(
                  'h-5 shrink-0 rounded-full px-2 text-ui-3xs transition-colors',
                  isAfter ? 'bg-foreground text-background' : 'text-muted-foreground opacity-0 group-hover:opacity-100 hover:bg-background/60 hover:text-foreground',
                )}
                title="Compare against the baseline"
              >Compare</button>
            </div>
          </div>
        {/each}
      {/if}
    </div>
  </div>

  <!-- ── Right: comparison ──────────────────────────────────────────────── -->
  <div class="flex min-w-0 flex-1 flex-col overflow-hidden">
    {#if !beforeSnap || !afterSnap}
      <div class="flex flex-1 flex-col items-center justify-center px-8">
        <div class="w-full max-w-sm">
          <h2 class="text-ui-lg font-semibold tracking-tight text-foreground">Compare schema snapshots</h2>
          <p class="mt-1.5 text-ui-xs leading-relaxed text-muted-foreground">
            {#if snapshots.length === 0}
              Capture the schema now, change it, then capture again to see what moved.
            {:else if snapshots.length === 1}
              One snapshot so far. Capture another after a change, then compare the two.
            {:else}
              Hover a snapshot and pick <span class="text-foreground">Base</span>, then another as <span class="text-foreground">Compare</span>.
            {/if}
          </p>
          {#if snapshots.length < 2}
            <button
              onclick={openCaptureModal}
              disabled={capturing}
              class="mt-4 inline-flex h-8 items-center gap-1.5 rounded-full bg-foreground px-3.5 text-ui-xs font-medium text-background hover:bg-foreground/90 disabled:opacity-40"
            >
              <Camera class="size-3.5 shrink-0" />
              Capture snapshot
            </button>
          {/if}
        </div>
      </div>

    {:else}
      <!-- Comparison header bar -->
      <div class="flex h-12 shrink-0 items-center gap-2 overflow-hidden border-b border-border/30 px-5 text-ui-xs">
        <div class="flex min-w-0 shrink items-center gap-1.5">
          <span class="shrink-0 text-ui-2xs text-muted-foreground">Base</span>
          <span class="min-w-0 truncate font-medium text-foreground" title={beforeSnap.connectionLabel}>
            {beforeSnap.title || shortName(beforeSnap.connectionLabel)}
          </span>
          <span class="shrink-0 text-muted-foreground">{fmtDate(beforeSnap.capturedAt)}</span>
        </div>

        <button
          onclick={swapSnapshots}
          title="Swap base and compare"
          class="flex size-7 shrink-0 items-center justify-center rounded-full text-muted-foreground hover:bg-accent/50 hover:text-foreground"
        >
          <ArrowLeftRight class="size-3.5 shrink-0" />
        </button>

        <div class="flex min-w-0 shrink items-center gap-1.5">
          <span class="shrink-0 text-ui-2xs text-muted-foreground">Compare</span>
          <span class="min-w-0 truncate font-medium text-foreground" title={afterSnap.connectionLabel}>
            {afterSnap.title || shortName(afterSnap.connectionLabel)}
          </span>
          <span class="shrink-0 text-muted-foreground">{fmtDate(afterSnap.capturedAt)}</span>
        </div>

        {#if diff}
          <div class="ml-auto flex shrink-0 items-center gap-3 pl-3">
            {#if diff.addedTables.length}
              <span class="font-medium text-success">+{diff.addedTables.length}</span>
            {/if}
            {#if diff.removedTables.length}
              <span class="font-medium text-destructive">−{diff.removedTables.length}</span>
            {/if}
            {#if diff.modifiedTables.length}
              <span class="font-medium text-warning">~{diff.modifiedTables.length}</span>
            {/if}
            {#if isDiffEmpty(diff)}
              <span class="text-muted-foreground">Identical</span>
            {:else}
              <Button
                variant="outline"
                size="sm"
                class="h-7 shrink-0"
                onclick={() => (migrationOpen = true)}
                title="Generate the SQL that turns the earlier snapshot into the later one"
              >
                <FileCode class="size-3.5 shrink-0" />
                Migration
              </Button>
            {/if}
          </div>
        {/if}
      </div>

      {#if !diff}
        <div class="loading-reveal flex flex-1 items-center justify-center">
          <Loader2 class="size-4 shrink-0 animate-spin text-muted-foreground" />
        </div>

      {:else if isDiffEmpty(diff)}
        <div class="flex flex-1 flex-col items-center justify-center gap-4">
          <div class="flex size-14 items-center justify-center rounded-lg border border-success/20 bg-success/5">
            <Check class="size-6 shrink-0 text-success" />
          </div>
          <div class="text-center">
            <p class="text-ui-sm font-semibold text-foreground/65">Schemas are identical</p>
            <p class="mt-1 text-ui-xs text-muted-foreground">No structural differences between these two snapshots</p>
          </div>
        </div>

      {:else}
        <div class="flex min-h-0 flex-1 overflow-hidden">
          <!-- Changed tables tree -->
          <div class="flex w-52 shrink-0 flex-col overflow-hidden border-r border-border/30">
            <div class="flex h-8 shrink-0 items-center border-b border-border/25 px-3">
              <span class="flex-1 text-ui-3xs font-medium uppercase tracking-widest text-muted-foreground">Changed</span>
              <span class="font-mono text-ui-3xs text-muted-foreground tabular-nums">{totalChanges}</span>
            </div>
            <div class="min-h-0 flex-1 overflow-y-auto">
              {@render FileTree(diff)}
            </div>
          </div>

          <!-- DDL diff -->
          <div class="flex min-w-0 flex-1 flex-col overflow-hidden">
            {#if !selectedTable}
              <div class="flex flex-1 items-center justify-center">
                <p class="text-ui-xs text-muted-foreground">Select a table to view its DDL diff</p>
              </div>
            {:else}
              {@render DdlDiff(selectedTable)}
            {/if}
          </div>
        </div>
      {/if}
    {/if}
  </div>
</div>

<!-- ── Capture modal ─────────────────────────────────────────────────────── -->
{#if captureOpen}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/65"
    role="presentation"
    onclick={(e) => { if (e.target === e.currentTarget) captureOpen = false }}
  >
    <div class="flex w-[460px] flex-col overflow-hidden rounded-2xl border border-border/60 bg-popover elevate-3-rim">

      <!-- Modal header -->
      <div class="flex shrink-0 items-center gap-2.5 px-5 pt-5 pb-4">
        <Camera class="size-4 shrink-0 text-foreground/40" />
        <p class="flex-1 text-ui-sm font-semibold text-foreground/85">New snapshot</p>
        <button
          onclick={() => { captureOpen = false }}
          class="rounded p-1 text-muted-foreground hover:text-foreground"
        >
          <X class="size-3.5 shrink-0" />
        </button>
      </div>

      <div class="overflow-y-auto">
        <!-- Title input -->
        <div class="border-t border-border/25 px-5 py-3">
          <input
            bind:value={captureTitle}
            placeholder="Snapshot title (optional), e.g. Before migration v2"
            class="w-full bg-transparent text-ui-xs text-foreground/75 outline-none placeholder:text-muted-foreground"
          />
        </div>

        <!-- Connection list -->
        <div class="border-t border-border/25">
          <div class="flex items-center px-5 py-2">
            <span class="text-ui-3xs font-semibold uppercase tracking-widest text-muted-foreground">Connection</span>
          </div>
          <div>
            {#if connectionId}
              <button
                onclick={() => selectCaptureConn(null)}
                class={cn(
                  'flex w-full items-center gap-3 px-5 py-2 text-left text-ui-xs transition-colors',
                  captureConn === null ? 'bg-muted/[0.08]' : 'hover:bg-muted/[0.05]',
                )}
              >
                <!-- Radio dot -->
                <span class={cn(
                  'flex size-3.5 shrink-0 items-center justify-center rounded-full border',
                  captureConn === null ? 'border-primary/60 bg-primary/20' : 'border-border/50',
                )}>
                  {#if captureConn === null}<span class="size-1.5 rounded-full bg-primary/80"></span>{/if}
                </span>
                <span class="min-w-0 flex-1 truncate font-medium text-foreground/75" title={connectionLabel}>{shortName(connectionLabel)}</span>
                <span class="shrink-0 rounded bg-success/10 px-1.5 py-0.5 text-ui-3xs font-medium text-success">active</span>
              </button>
            {/if}
            {#each connections.filter((c) => c.id !== connectionId) as conn (conn.id)}
              <button
                onclick={() => selectCaptureConn(conn)}
                class={cn(
                  'flex w-full items-center gap-3 px-5 py-2 text-left text-ui-xs transition-colors',
                  captureConn?.id === conn.id ? 'bg-muted/[0.08]' : 'hover:bg-muted/[0.05]',
                )}
              >
                <span class={cn(
                  'flex size-3.5 shrink-0 items-center justify-center rounded-full border',
                  captureConn?.id === conn.id ? 'border-primary/60 bg-primary/20' : 'border-border/50',
                )}>
                  {#if captureConn?.id === conn.id}<span class="size-1.5 rounded-full bg-primary/80"></span>{/if}
                </span>
                <span class="min-w-0 flex-1 truncate text-foreground/65" title={conn.name}>{shortName(conn.name)}</span>
                <span class="shrink-0 text-ui-3xs uppercase text-muted-foreground">{conn.type}</span>
              </button>
            {/each}
            {#if !connectionId && connections.length === 0}
              <p class="px-5 py-3 text-ui-xs text-muted-foreground">No connections configured</p>
            {/if}
          </div>
        </div>

        <!-- Database + Schema side by side -->
        <div class="border-t border-border/25">
          <div class="flex divide-x divide-border/25">
            <!-- Database -->
            <div class="w-44 shrink-0">
              <div class="flex items-center border-b border-border/20 px-4 py-2">
                <span class="text-ui-3xs font-semibold uppercase tracking-widest text-muted-foreground">Database</span>
                {#if captureLoadingDbs}<Loader2 class="ml-auto size-3 shrink-0 animate-spin text-muted-foreground" />{/if}
              </div>
              <div class="max-h-44 overflow-y-auto py-1">
                {#if !captureLoadingDbs && captureDatabases.length === 0}
                  <p class="px-4 py-2 text-ui-3xs italic text-muted-foreground">None found</p>
                {/if}
                {#each captureDatabases as db (db)}
                  <button
                    onclick={() => onCaptureDbChange(db)}
                    class={cn(
                      'flex w-full items-center gap-2 px-4 py-1.5 text-left text-ui-xs',
                      captureDb === db
                        ? 'bg-muted/[0.08] font-medium text-foreground/80'
                        : 'text-muted-foreground hover:bg-muted/[0.05] hover:text-foreground/65',
                    )}
                  >
                    {#if captureDb === db}
                      <span class="size-1 shrink-0 rounded-full bg-primary/70"></span>
                    {:else}
                      <span class="size-1 shrink-0"></span>
                    {/if}
                    <span class="truncate font-mono text-ui-2xs">{db}</span>
                  </button>
                {/each}
              </div>
            </div>

            <!-- Schemas -->
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2 border-b border-border/20 px-4 py-2">
                <span class="text-ui-3xs font-semibold uppercase tracking-widest text-muted-foreground">Schemas</span>
                {#if captureLoadingSchemas}
                  <Loader2 class="ml-auto size-3 shrink-0 animate-spin text-muted-foreground" />
                {:else if captureSchemaList.length > 0}
                  <div class="ml-auto flex items-center gap-2">
                    <button onclick={() => { captureChecked = new Set(captureSchemaList) }} class="text-ui-3xs text-muted-foreground hover:text-foreground/60">All</button>
                    <button onclick={() => { captureChecked = new Set() }} class="text-ui-3xs text-muted-foreground hover:text-foreground/60">None</button>
                  </div>
                {/if}
              </div>
              <div class="max-h-44 overflow-y-auto py-1">
                {#if !captureLoadingSchemas && captureSchemaList.length === 0}
                  <p class="px-4 py-2 text-ui-3xs italic text-muted-foreground">
                    {captureDb ? 'No schemas found' : 'Select a database first'}
                  </p>
                {/if}
                {#each captureSchemaList as schema (schema)}
                  <button
                    type="button"
                    onclick={() => toggleCaptureSchema(schema)}
                    class={cn(
                      'flex w-full cursor-pointer items-center gap-2.5 px-4 py-1.5 text-ui-xs',
                      captureChecked.has(schema)
                        ? 'text-foreground/75'
                        : 'text-muted-foreground hover:text-foreground/55',
                    )}
                  >
                    <span class={cn(
                      'flex size-3.5 shrink-0 items-center justify-center rounded border',
                      captureChecked.has(schema)
                        ? 'border-primary/40 bg-primary/15'
                        : 'border-border/40',
                    )}>
                      {#if captureChecked.has(schema)}<Check class="size-3 shrink-0 text-primary" />{/if}
                    </span>
                    <span class="truncate font-mono text-ui-2xs">{schema}</span>
                  </button>
                {/each}
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Footer -->
      <div class="flex shrink-0 items-center justify-between border-t border-border/25 px-5 py-3">
        <p class="text-ui-3xs text-muted-foreground">
          {#if captureChecked.size > 0 && captureSchemaList.length > 0}
            {captureChecked.size} / {captureSchemaList.length} schemas
          {:else if captureSchemaList.length > 0}
            All schemas
          {/if}
        </p>
        <div class="flex items-center gap-2">
          <button
            onclick={() => { captureOpen = false }}
            class="rounded px-3 py-1.5 text-ui-xs text-muted-foreground hover:text-foreground/80"
          >Cancel</button>
          <button
            onclick={doCapture}
            disabled={captureLoadingSchemas || captureLoadingDbs || (!connectionId && !captureConn)}
            class="flex items-center gap-1.5 rounded-lg bg-foreground px-4 py-1.5 text-ui-xs font-medium text-background hover:opacity-85 disabled:opacity-30"
          >
            <Camera class="size-3 shrink-0" />
            Capture
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<Dialog.Root bind:open={migrationOpen}>
  <Dialog.Content class="flex max-h-[85vh] max-w-3xl flex-col gap-0 overflow-hidden p-0">
    <Dialog.Header class="border-b border-border/60 px-5 py-4">
      <Dialog.Title class="text-ui-sm">Migration SQL</Dialog.Title>
      <Dialog.Description class="text-ui-2xs">
        {migration?.statementCount ?? 0} statement{(migration?.statementCount ?? 0) === 1 ? '' : 's'}
        taking the earlier snapshot to the later one. Review it before running — the
        rollback below is commented out.
      </Dialog.Description>
    </Dialog.Header>

    <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
      {#if migration && migration.warnings.length > 0}
        <div class="mb-3 rounded-lg border border-warning/30 bg-warning/8 px-3 py-2.5">
          <p class="text-ui-2xs font-semibold text-foreground">Read before running</p>
          <ul class="mt-1.5 flex flex-col gap-1">
            {#each migration.warnings as warning (warning)}
              <li class="text-ui-3xs text-muted-foreground">• {warning}</li>
            {/each}
          </ul>
        </div>
      {/if}
      <pre class="overflow-x-auto rounded-lg border border-border/60 bg-background p-3 font-mono text-ui-3xs leading-relaxed text-foreground"><code>{migrationScript}</code></pre>
    </div>

    <Dialog.Footer class="border-t border-border/60 px-5 py-3">
      <Button variant="outline" size="sm" onclick={() => (migrationOpen = false)}>Close</Button>
      <Button size="sm" onclick={copyMigration}>
        <Copy class="size-3.5 shrink-0" />
        Copy
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>

<!-- ── Snippets ──────────────────────────────────────────────────────────── -->

{#snippet FileTree(d)}
  {@const grouped = groupBySchema(d)}
  <ul class="flex w-full min-w-full flex-col gap-0.5 px-1.5 pb-1">
    {#each Object.entries(grouped) as [schema, items] (schema)}
      {@const collapsed = collapsedSchemas.has(schema)}
      <li>
        <!-- Schema group header, mirrors Sidebar section header -->
        <button
          onclick={() => toggleSchemaGroup(schema)}
          class="flex w-full items-center gap-1 px-2.5 pb-1 pt-2 text-left"
        >
          <ChevronRight
            class={cn(
              'size-3 shrink-0 text-muted-foreground transition-transform duration-100',
              !collapsed && 'rotate-90',
            )}
          />
          <span class="min-w-0 flex-1 truncate font-mono text-ui-2xs font-medium uppercase tracking-[0.06em] text-muted-foreground">
            {schema}
          </span>
          <span class="ml-1 shrink-0 font-mono text-ui-2xs text-muted-foreground tabular-nums">{items.length}</span>
        </button>

        {#if !collapsed}
          <!-- Table rows, mirrors Sidebar table rows exactly -->
          <ul class="flex flex-col gap-0.5">
            {#each items as item (item.name)}
              {@const key = `${item.schema}.${item.name}`}
              {@const isActive = selectedTable === key}
              {@const statusColor =
                item.status === 'added'   ? 'text-success' :
                item.status === 'removed' ? 'text-destructive' :
                'text-warning'}
              <li class="[contain-intrinsic-size:auto_30px] [content-visibility:auto]">
                <button
                  onclick={() => { selectedTable = key }}
                  class={cn(
                    'grid w-full grid-cols-[auto_minmax(0,1fr)] items-center gap-x-2 rounded-md px-2 py-1.5 text-left',
                    isActive
                      ? 'bg-sidebar-accent text-sidebar-accent-foreground'
                      : 'text-muted-foreground hover:bg-sidebar-accent/50 hover:text-foreground',
                  )}
                >
                  <!-- Status glyph col -->
                  <span class={cn('w-3 shrink-0 text-center font-mono text-ui-3xs font-semibold leading-none', statusColor)}>
                    {item.status === 'added' ? '+' : item.status === 'removed' ? '−' : '~'}
                  </span>
                  <!-- Table name col -->
                  <span class="min-w-0 truncate font-mono text-ui-sm leading-none">{item.name}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </li>
    {/each}
  </ul>
{/snippet}

{#snippet DdlDiff(tableKey)}
  {@const status = getTableStatus(diff, tableKey)}
  {@const statusColor = status === 'added' ? 'text-success' : status === 'removed' ? 'text-destructive' : 'text-warning'}
  {@const addCount = selectedDdlDiff?.filter((l) => l.type === 'add').length    ?? 0}
  {@const removeCount = selectedDdlDiff?.filter((l) => l.type === 'remove').length ?? 0}

  <div class="flex h-8 shrink-0 items-center gap-2 border-b border-border/40 px-4">
    <span class={cn('font-mono text-ui-xs font-semibold', statusColor)}>
      {status === 'added' ? '+' : status === 'removed' ? '−' : '~'}
    </span>
    <span class="min-w-0 flex-1 truncate font-mono text-ui-xs font-medium text-foreground/75">{tableKey}</span>
    <div class="ml-auto flex shrink-0 items-center gap-2 text-ui-2xs">
      {#if removeCount > 0}<span class="font-medium text-destructive">−{removeCount}</span>{/if}
      {#if addCount > 0}<span class="font-medium text-success">+{addCount}</span>{/if}
    </div>
  </div>

  <div class="min-h-0 flex-1 overflow-auto">
    {#if !selectedDdlDiff}
      <div class="flex items-center justify-center py-10 text-ui-xs text-muted-foreground">No DDL available</div>
    {:else}
      <table class="w-full border-collapse font-mono text-ui-xs leading-5">
        <tbody>
          {#each selectedDdlDiff as line, i (i)}
            {#if line.type === 'ellipsis'}
              <tr class="bg-muted/10">
                <td class="w-10 select-none border-r border-border/15 px-2 text-right text-ui-3xs text-muted-foreground"></td>
                <td class="w-10 select-none border-r border-border/15 px-2 text-right text-ui-3xs text-muted-foreground"></td>
                <td class="w-5 select-none"></td>
                <td class="px-4 py-0.5 text-ui-3xs text-muted-foreground">··· {line.count} unchanged line{line.count !== 1 ? 's' : ''}</td>
              </tr>
            {:else if line.type === 'add'}
              <tr class="bg-success/[0.06] hover:bg-success/[0.1]">
                <td class="w-10 select-none border-r border-success/10 px-2 text-right text-ui-3xs text-muted-foreground"></td>
                <td class="w-10 select-none border-r border-success/10 px-2 text-right text-ui-3xs text-success">{line.lineB}</td>
                <td class="w-5 select-none text-center text-success">+</td>
                <td class="px-4 py-px text-success"><span class="whitespace-pre">{line.content}</span></td>
              </tr>
            {:else if line.type === 'remove'}
              <tr class="bg-destructive/[0.06] hover:bg-destructive/[0.1]">
                <td class="w-10 select-none border-r border-destructive/10 px-2 text-right text-ui-3xs text-destructive">{line.lineA}</td>
                <td class="w-10 select-none border-r border-destructive/10 px-2 text-right text-ui-3xs text-muted-foreground"></td>
                <td class="w-5 select-none text-center text-destructive">−</td>
                <td class="px-4 py-px text-destructive"><span class="whitespace-pre">{line.content}</span></td>
              </tr>
            {:else}
              <tr class="hover:bg-muted/8">
                <td class="w-10 select-none border-r border-border/10 px-2 text-right text-ui-3xs text-muted-foreground">{line.lineA}</td>
                <td class="w-10 select-none border-r border-border/10 px-2 text-right text-ui-3xs text-muted-foreground">{line.lineB}</td>
                <td class="w-5 select-none"></td>
                <td class="px-4 py-px text-muted-foreground"><span class="whitespace-pre">{line.content}</span></td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
{/snippet}
