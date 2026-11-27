<script>
  import AlertTriangle from '@lucide/svelte/icons/triangle-alert'
  import EmptyState from './EmptyState.svelte'
  import PenLine from '@lucide/svelte/icons/pen-line'
  import SelectMenu from './SelectMenu.svelte'
  import DbIcon from './DbIcon.svelte'
  import { engineLabel } from '$lib/db-capabilities.js'
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
  import { cn, errorText } from '$lib/utils.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import FileCode from '@lucide/svelte/icons/file-code'
  import Copy from '@lucide/svelte/icons/copy'
  import { buildMigration, formatMigrationScript } from '$lib/schema-migration.js'
  import { diffLines, pairWordDiffs } from '$lib/line-diff.js'
  import { listSchemas, listSchemasOnConnection, executeSql, executeSqlOnConnection } from '$lib/api.js'
  import {
    captureSnapshot,
    listAllSnapshots,
    deleteSnapshot,
    deleteAllSnapshots,
    restoreSnapshots,
    diffSnapshots,
    isDiffEmpty,
  } from '$lib/stores/schema-snapshots.js'

  /** @typedef {import('$lib/stores/schema-snapshots.js').SchemaSnapshot} SchemaSnapshot */
  /** @typedef {import('$lib/stores/schema-snapshots.js').SnapshotDiff} SnapshotDiff */
  /** @typedef {import('$lib/stores/schema-snapshots.js').SnapshotColumn} SnapshotColumn */
  /** @typedef {import('$lib/stores/connections.js').SavedConnection} SavedConnection */
  /** @typedef {{ type: 'same'|'add'|'remove', content: string, lineA?: number, lineB?: number }} DiffLine */
  /** @typedef {DiffLine | { type: 'ellipsis', count: number, from: number }} CollapsedLine */

  /**
   * @type {{
   *   connectionId: string,
   *   connectionLabel: string,
   *   dbType?: string,
   *   connections?: SavedConnection[],
   * }}
   */
  let { connectionId, connectionLabel, dbType = 'postgres', connections = [], onopensql = /** @type {((sql: string) => void) | null} */ (null) } = $props()
  /** The "Read before running" list, folded to one line until opened. */
  let warningsOpen = $state(false)

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
      toast.error('Could not copy', { description: errorText(e) })
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
    /** @type {DiffLine[]} */
    let lines
    if (!bDdl) {
      lines = aDdl.split('\n').map((content, i) => /** @type {DiffLine} */ ({ type: 'add',    content, lineB: i + 1 }))
    } else if (!aDdl) {
      lines = bDdl.split('\n').map((content, i) => /** @type {DiffLine} */ ({ type: 'remove', content, lineA: i + 1 }))
    } else {
      lines = diffLines(bDdl.split('\n'), aDdl.split('\n'))
    }
    let adds = 0, removes = 0
    for (const l of lines) { if (l.type === 'add') adds++; else if (l.type === 'remove') removes++ }
    // Word parts keyed by the line object itself, so the view finds them in O(1).
    const pw = pairWordDiffs(lines)
    /** @type {Map<DiffLine, import('$lib/line-diff.js').WordPart[]>} */
    const words = new Map()
    for (const [idx, parts] of pw) words.set(lines[idx], parts)
    return { lines, words, adds, removes }
  })

  /** Unchanged runs the reader opened (keyed by their first line), per table. */
  let expandedGaps = $state(/** @type {Set<number>} */ (new Set()))
  $effect(() => { void selectedTable; expandedGaps = new Set() })
  const shownDdlLines = $derived(selectedDdlDiff ? collapseContext(selectedDdlDiff.lines, 3, expandedGaps) : null)

  /** Status of every changed table, built once per diff (the row list asked
   *  three linear scans per row before). */
  const tableStatus = $derived.by(() => {
    /** @type {Map<string, 'added' | 'removed' | 'modified'>} */
    const m = new Map()
    if (!diff) return m
    for (const t of diff.addedTables) m.set(`${t.schema}.${t.name}`, 'added')
    for (const t of diff.removedTables) m.set(`${t.schema}.${t.name}`, 'removed')
    for (const t of diff.modifiedTables) m.set(`${t.schema}.${t.name}`, 'modified')
    return m
  })

  onMount(load)

  async function load() {
    loading = true
    try { snapshots = await listAllSnapshots() }
    finally { loading = false }
  }

  // ── Clear all ──────────────────────────────────────────────────────────────

  /** Deleted snapshots go back on Undo, with the comparison they were in. @param {SchemaSnapshot[]} gone @param {string} title */
  function offerUndo(gone, title) {
    const was = { beforeId, afterId }
    // Plain copies: IndexedDB cannot store the page's reactive proxies.
    const keep = $state.snapshot(gone)
    toast.message(title, {
      duration: 8000,
      action: {
        label: 'Undo',
        onClick: async () => {
          try { await restoreSnapshots(keep) } catch (err) { toast.error(`Couldn't undo: ${errorText(err)}`); return }
          await load()
          if (!beforeId && !afterId) ({ beforeId, afterId } = was)
        },
      },
    })
  }

  // No confirm: it goes at once and the toast offers Undo.
  async function onClearClick() {
    const gone = snapshots
    try { await deleteAllSnapshots() } catch (err) { toast.error(`Delete failed: ${errorText(err)}`); return }
    snapshots = []
    offerUndo(gone, gone.length === 1 ? 'Snapshot deleted' : `${gone.length} snapshots deleted`)
    beforeId = null; afterId = null
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
    const gone = snapshots.filter((s) => s.id === id)
    snapshots = snapshots.filter((s) => s.id !== id)
    offerUndo(gone, 'Snapshot deleted')
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

  function getTableStatus(/** @type {SnapshotDiff | null} */ d, /** @type {string} */ key) {
    void d
    return tableStatus.get(key) ?? /** @type {'modified'} */ ('modified')
  }

  function collapseContext(/** @type {DiffLine[]} */ lines, ctx, /** @type {Set<number>} */ open = new Set()) {
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
    let gapFrom = 0
    for (let i = 0; i < lines.length; i++) {
      if (visible.has(i)) {
        if (gap > 0) {
          if (open.has(gapFrom)) for (let k = gapFrom; k < i; k++) out.push(lines[k])
          else out.push({ type: 'ellipsis', count: gap, from: gapFrom })
        }
        gap = 0
        out.push(lines[i])
      } else { if (gap === 0) gapFrom = i; gap++ }
    }
    if (gap > 0) {
      if (open.has(gapFrom)) for (let k = gapFrom; k < lines.length; k++) out.push(lines[k])
      else out.push({ type: 'ellipsis', count: gap, from: gapFrom })
    }
    return out
  }
</script>

<!-- ════════════════════════════════════════════════════════════════════════ -->
<div class="flex h-full min-h-0 overflow-hidden">

  <!-- ── Sidebar ────────────────────────────────────────────────────────── -->
  <!-- Only once there is a snapshot to list: before that it was an empty
       column with a second New button beside the page's own Capture. -->
  {#if loading || snapshots.length > 0}
  <div class="flex w-64 shrink-0 flex-col overflow-hidden border-r border-border/30 bg-panel">

    <!-- Sidebar header -->
    <div class="flex h-12 shrink-0 items-center gap-2 px-4">
      <span class="text-ui-sm font-medium text-foreground">Snapshots</span>
      {#if snapshots.length > 0}
        <span class="rounded-full bg-muted px-1.5 text-ui-3xs tabular-nums text-muted-foreground">{snapshots.length}</span>
      {/if}
      <span class="flex-1"></span>

      {#if snapshots.length > 0}
        <DropdownMenu.DropdownMenu>
          <DropdownMenu.DropdownMenuTrigger
            class="flex size-7 items-center justify-center rounded-full text-muted-foreground hover:bg-accent/50 hover:text-foreground data-[state=open]:bg-accent/50 data-[state=open]:text-foreground"
            aria-label="More"
          >
            <Ellipsis class="size-3.5 shrink-0" />
          </DropdownMenu.DropdownMenuTrigger>
          <DropdownMenu.DropdownMenuContent align="end" class="min-w-40">
            <DropdownMenu.DropdownMenuItem variant="destructive" onclick={() => void onClearClick()}>
              <Trash2 class="size-3.5 shrink-0" />Delete all snapshots
            </DropdownMenu.DropdownMenuItem>
          </DropdownMenu.DropdownMenuContent>
        </DropdownMenu.DropdownMenu>
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
        <!-- The page beside the list explains an empty timeline and offers
             Capture snapshot; the list saying it too was the same thing twice. -->

      {:else}
        {#each snapshots as snap (snap.id)}
          {@const isBefore = snap.id === beforeId}
          {@const isAfter = snap.id === afterId}
          {@const isActive = isBefore || isAfter}
          {@const tableCount = Object.values(snap.tables ?? {}).flat().length}
          {@const displayName = snap.title || shortName(snap.connectionLabel)}

          <!-- One row per snapshot: the title gets the full width; a lettered
               badge says which side it is (A the baseline, B the comparison);
               Base and Compare sit on the date line, shown on hover or focus. -->
          <div class={cn(
            'group mb-1 rounded-lg px-2.5 py-2 transition-colors focus-within:bg-accent/50',
            isActive ? 'bg-accent' : 'hover:bg-accent/50',
          )}>
            <div class="flex min-w-0 items-center gap-2">
              <span
                class={cn(
                  'flex size-5 shrink-0 items-center justify-center rounded-full text-ui-3xs font-semibold',
                  isBefore ? 'bg-warning/20 text-warning' : isAfter ? 'bg-info/20 text-info' : 'border border-border/70 text-transparent',
                )}
                aria-label={isBefore ? 'Baseline' : isAfter ? 'Compared' : undefined}
              >{isBefore ? 'A' : isAfter ? 'B' : ''}</span>
              <span class="min-w-0 flex-1 truncate text-ui-sm font-medium text-foreground" title={snap.title ? `${snap.title} · ${snap.connectionLabel}` : snap.connectionLabel}>
                {displayName}
              </span>
              <button
                type="button"
                onclick={() => remove(snap.id)}
                class="flex size-6 shrink-0 items-center justify-center rounded-md text-transparent transition-colors group-hover:text-muted-foreground group-focus-within:text-muted-foreground hover:bg-muted hover:!text-destructive"
                aria-label="Delete snapshot"
                title="Delete snapshot"
              >
                <X class="size-3.5 shrink-0" />
              </button>
            </div>
            <!-- The date line and Base / Compare share one place: the buttons
                 fade in over it on hover or focus. Side by side, the hidden
                 buttons kept their width and squeezed the table count out. -->
            <div class="mt-1 grid ps-7 text-ui-2xs text-muted-foreground *:col-start-1 *:row-start-1">
              <span class="flex min-w-0 items-center gap-1.5 transition-opacity group-hover:opacity-0 group-focus-within:opacity-0">
                <span class="shrink-0 tabular-nums">{fmtDate(snap.capturedAt)}</span>
                <span aria-hidden="true">·</span>
                <span class="min-w-0 truncate tabular-nums">{tableCount} {tableCount === 1 ? 'table' : 'tables'}</span>
              </span>
              <span class="-ms-1.5 flex items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100">
                <button
                  type="button"
                  onclick={() => { beforeId = snap.id }}
                  class={cn('h-5 rounded px-1.5 font-medium transition-colors', isBefore ? 'text-warning' : 'hover:bg-muted hover:text-foreground')}
                  title="Use as the baseline (A)"
                  aria-pressed={isBefore}
                >Base</button>
                <button
                  type="button"
                  onclick={() => { afterId = snap.id }}
                  class={cn('h-5 rounded px-1.5 font-medium transition-colors', isAfter ? 'text-info' : 'hover:bg-muted hover:text-foreground')}
                  title="Compare against the baseline (B)"
                  aria-pressed={isAfter}
                >Compare</button>
              </span>
            </div>
          </div>
        {/each}
      {/if}
    </div>
  </div>
  {/if}

  <!-- ── Right: comparison ──────────────────────────────────────────────── -->
  <div class="flex min-w-0 flex-1 flex-col overflow-hidden">
    {#if !beforeSnap || !afterSnap}
      <EmptyState class="flex-1 justify-center" icon={ArrowLeftRight} title="Compare schema snapshots">
        {#snippet body()}
          {#if snapshots.length === 0}
            Capture the schema now, change it, then capture again to see what moved.
          {:else if snapshots.length === 1}
            One snapshot so far. Capture another after a change, then compare the two.
          {:else}
            Hover a snapshot and pick <span class="text-foreground">Base</span>, then another as <span class="text-foreground">Compare</span>.
          {/if}
        {/snippet}
        {#if snapshots.length < 2}
          <Button size="sm" onclick={openCaptureModal} disabled={capturing}>
            <Camera class="size-3.5 shrink-0" />
            Capture snapshot
          </Button>
        {/if}
      </EmptyState>

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
              <span class="flex-1 text-ui-xs font-medium text-muted-foreground">Changed</span>
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
<!-- The shared dialog: it was a hand-built overlay with no dialog role,
     no focus inside it and no Escape. -->
<Dialog.Root bind:open={captureOpen}>
  <Dialog.Content class="flex max-h-[85vh] max-w-[28.75rem] flex-col gap-0 overflow-hidden p-0">
      <Dialog.Header class="px-5 pt-5 pb-4">
        <Dialog.Title class="text-ui-sm font-semibold">New snapshot</Dialog.Title>
      </Dialog.Header>

      <div class="overflow-y-auto">
        <!-- Title input -->
        <div class="border-t border-border/25 px-5 py-3">
          <p class="mb-1.5 text-ui-2xs font-medium text-muted-foreground">Title</p>
          <input
            bind:value={captureTitle}
            placeholder="Optional, e.g. Before migration v2"
            onkeydown={(e) => { if (e.key === 'Enter' && !(captureLoadingSchemas || captureLoadingDbs || (!connectionId && !captureConn))) { e.preventDefault(); void doCapture() } }}
            class="no-field-frame h-9 w-full rounded-lg border-0 bg-muted/55 px-3 text-ui-sm text-foreground outline-none placeholder:text-muted-foreground focus:bg-muted/80"
          />
        </div>

        <!-- Connection: one searchable picker instead of a radio row per
             saved connection (twenty-odd rows pushed the schema list off the
             bottom of the window). -->
        <div class="border-t border-border/25 px-5 py-3">
          <p class="mb-1.5 text-ui-2xs font-medium text-muted-foreground">Connection</p>
          {#if !connectionId && connections.length === 0}
            <p class="text-ui-xs text-muted-foreground">No connections configured</p>
          {:else}
            <SelectMenu
              ariaLabel="Connection"
              triggerClass="h-9 w-full"
              contentClass="w-[var(--bits-popover-anchor-width)]"
              searchPlaceholder="Search connections"
              value={captureConn?.id ?? '__active'}
              onValueChange={(v) => selectCaptureConn(v === '__active' ? null : (connections.find((c) => c.id === v) ?? null))}
              items={[
                ...(connectionId ? [{ value: '__active', label: shortName(connectionLabel), hint: 'Active', engine: connections.find((c) => c.id === connectionId)?.type ?? '', keywords: [connectionLabel] }] : []),
                ...connections.filter((c) => c.id !== connectionId).map((c) => ({ value: c.id, label: shortName(c.name), hint: engineLabel(c.type), engine: c.type, keywords: [c.name, c.type] })),
              ]}
            >
              {#snippet lead(it)}<DbIcon id={it.engine} class="size-4 shrink-0" />{/snippet}
            </SelectMenu>
          {/if}
        </div>

        <!-- Database + Schema side by side -->
        <div class="border-t border-border/25">
          <div class="flex divide-x divide-border/25">
            <!-- Database -->
            <div class="w-44 shrink-0">
              <div class="flex items-center border-b border-border/20 px-4 py-2">
                <span class="text-ui-xs font-medium text-muted-foreground">Database</span>
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
                <span class="text-ui-xs font-medium text-muted-foreground">Schemas</span>
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
          <Button variant="ghost" size="sm" onclick={() => { captureOpen = false }}>Cancel</Button>
          <Button size="sm" onclick={doCapture} disabled={captureLoadingSchemas || captureLoadingDbs || (!connectionId && !captureConn)}>
            <Camera class="size-3.5 shrink-0" />
            Capture
          </Button>
        </div>
      </div>
  </Dialog.Content>
</Dialog.Root>

<Dialog.Root bind:open={migrationOpen}>
  <Dialog.Content class="flex h-[min(44rem,85vh)] max-w-3xl flex-col gap-0 overflow-hidden p-0">
    <Dialog.Header class="gap-1 px-6 pt-5 pb-4">
      <Dialog.Title class="text-ui-lg font-semibold tracking-tight">Migration SQL</Dialog.Title>
      <Dialog.Description class="text-ui-xs text-muted-foreground">
        {migration?.statementCount ?? 0} statement{(migration?.statementCount ?? 0) === 1 ? '' : 's'} taking the earlier snapshot to the later one. Review before running; the rollback at the end is commented out.
      </Dialog.Description>
    </Dialog.Header>

    {#if migration && migration.warnings.length > 0}
      <!-- One line until opened: eight near-identical warnings took half the
           dialog before a single line of SQL. -->
      <div class="mx-6 mb-3 overflow-hidden rounded-lg border border-warning/30 bg-warning/8">
        <button type="button" class="flex w-full items-center gap-2 px-3 py-2 text-left" onclick={() => (warningsOpen = !warningsOpen)} aria-expanded={warningsOpen}>
          <AlertTriangle class="size-3.5 shrink-0 text-warning" />
          <span class="min-w-0 flex-1 text-ui-xs font-medium text-foreground">
            {migration.warnings.length} {migration.warnings.length === 1 ? 'thing' : 'things'} to read before running
          </span>
          <ChevronRight class={cn('size-3.5 shrink-0 text-muted-foreground transition-transform duration-150', warningsOpen && 'rotate-90')} />
        </button>
        {#if warningsOpen}
          <ul class="app-scroll flex max-h-40 flex-col gap-1 overflow-y-auto border-t border-warning/20 px-3 py-2">
            {#each migration.warnings as warning (warning)}
              <li class="text-ui-2xs leading-relaxed text-muted-foreground">{warning}</li>
            {/each}
          </ul>
        {/if}
      </div>
    {/if}

    <div class="mx-6 mb-4 flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border/60">
      {#await import('./CodeEditor.svelte') then { default: CodeEditor }}
        <CodeEditor value={migrationScript} lang="sql" readOnly ariaLabel="Migration SQL" />
      {/await}
    </div>

    <Dialog.Footer class="mx-0 mb-0 flex-row items-center gap-2 border-t border-border/40 px-6 py-3 sm:justify-end">
      <Button variant="ghost" size="sm" onclick={() => (migrationOpen = false)}>Close</Button>
      {#if onopensql}
        <Button variant="outline" size="sm" class="gap-1.5" onclick={() => { onopensql?.(migrationScript); migrationOpen = false }}>
          <PenLine class="size-3.5 shrink-0" />Open in editor
        </Button>
      {/if}
      <Button size="sm" class="gap-1.5" onclick={copyMigration}>
        <Copy class="size-3.5 shrink-0" />Copy
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
          <span class="min-w-0 flex-1 truncate font-mono text-ui-2xs font-medium text-muted-foreground">
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
                  <span class="min-w-0 truncate font-mono text-ui-sm leading-normal">{item.name}</span>
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
  {@const addCount = selectedDdlDiff?.adds ?? 0}
  {@const removeCount = selectedDdlDiff?.removes ?? 0}

  <!-- One quiet header: what kind of change, which table, how many lines. -->
  <div class="flex h-9 shrink-0 items-center gap-2.5 border-b border-border/40 px-4">
    <span
      class={cn(
        'inline-flex h-5 shrink-0 items-center rounded-md px-1.5 text-ui-3xs font-medium',
        status === 'added' ? 'bg-success/12 text-success' : status === 'removed' ? 'bg-destructive/12 text-destructive' : 'bg-warning/12 text-warning',
      )}
    >{status === 'added' ? 'Added' : status === 'removed' ? 'Removed' : 'Changed'}</span>
    <span class="min-w-0 flex-1 truncate font-mono text-ui-xs font-medium text-foreground">{tableKey}</span>
    <div class="ml-auto flex shrink-0 items-center gap-2.5 font-mono text-ui-2xs tabular-nums">
      {#if removeCount > 0}<span class="text-destructive">−{removeCount}</span>{/if}
      {#if addCount > 0}<span class="text-success">+{addCount}</span>{/if}
    </div>
  </div>

  <div class="min-h-0 flex-1 overflow-auto bg-panel">
    {#if !shownDdlLines}
      <div class="flex items-center justify-center py-10 text-ui-xs text-muted-foreground">No DDL available</div>
    {:else}
      <!-- A unified diff: two line-number gutters, a sign column, the line.
           Changed lines carry a 2px accent at the edge and a light tint; the
           part of a line that actually changed is marked inside it. -->
      <table class="w-full border-collapse font-mono text-ui-xs leading-[1.6]">
        <tbody>
          {#each shownDdlLines as line, i (i)}
            {#if line.type === 'ellipsis'}
              <tr>
                <td colspan="4" class="p-0">
                  <button
                    type="button"
                    class="flex h-7 w-full items-center gap-2 border-y border-border/30 bg-muted/25 px-4 text-left text-ui-3xs text-muted-foreground transition-colors hover:bg-muted/45 hover:text-foreground"
                    onclick={() => { const n = new Set(expandedGaps); n.add(line.from); expandedGaps = n }}
                  >
                    <ChevronDown class="size-3 shrink-0" aria-hidden="true" />
                    Show {line.count} unchanged line{line.count !== 1 ? 's' : ''}
                  </button>
                </td>
              </tr>
            {:else}
              {@const parts = selectedDdlDiff?.words.get(line)}
              {@const tone = line.type === 'add' ? 'success' : line.type === 'remove' ? 'destructive' : ''}
              <tr class={cn(tone === 'success' && 'bg-success/[0.07]', tone === 'destructive' && 'bg-destructive/[0.07]')}>
                <td class={cn('w-11 select-none py-0 pr-2 pl-3 text-right text-ui-3xs tabular-nums text-muted-foreground', tone && 'shadow-[inset_2px_0_0_var(--accent-bar)]')} style={tone ? `--accent-bar: var(--${tone})` : ''}>{line.lineA ?? ''}</td>
                <td class="w-10 select-none py-0 pr-2 text-right text-ui-3xs tabular-nums text-muted-foreground">{line.lineB ?? ''}</td>
                <td class={cn('w-5 select-none text-center', tone === 'success' ? 'text-success' : tone === 'destructive' ? 'text-destructive' : 'text-transparent')}>{line.type === 'add' ? '+' : line.type === 'remove' ? '−' : ' '}</td>
                <td class={cn('py-0 pr-4 pl-1', tone ? 'text-foreground' : 'text-muted-foreground')}>
                  <span class="whitespace-pre">{#if parts}{#each parts as p, k (k)}{#if p.changed}<mark class={cn('rounded-sm px-px text-inherit', tone === 'success' ? 'bg-success/25' : 'bg-destructive/25')}>{p.text}</mark>{:else}{p.text}{/if}{/each}{:else}{line.content}{/if}</span>
                </td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
{/snippet}
