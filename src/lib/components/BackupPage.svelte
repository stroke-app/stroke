<script>
  import EmptyState from './EmptyState.svelte'
  import { onDestroy, untrack } from 'svelte'
  import FieldSelect from './FieldSelect.svelte'
  import ScheduledBackups from './ScheduledBackups.svelte'
  import LockKeyhole from '@lucide/svelte/icons/lock-keyhole'
  import CalendarClock from '@lucide/svelte/icons/calendar-clock'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Checkbox } from '$lib/components/ui/checkbox/index.js'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import {
    backupExport,
    backupImport,
    backupCancel,
    backupInspectFile,
    pickBackupSavePath,
    pickBackupFile,
    revealInFolder,
  } from '$lib/api.js'
  import { readOnlyMode, guardWrite, READ_ONLY_HINT } from '$lib/stores/read-only.js'
  import { IS_MAC } from '$lib/shortcuts.js'
  import { cn } from '$lib/utils.js'
  import Download from '@lucide/svelte/icons/download'
  import Upload from '@lucide/svelte/icons/upload'
  import Check from '@lucide/svelte/icons/check'
  import X from '@lucide/svelte/icons/x'
  import Circle from '@lucide/svelte/icons/circle'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert'
  import CircleAlert from '@lucide/svelte/icons/circle-alert'
  import FileText from '@lucide/svelte/icons/file-text'
  import ScrollText from '@lucide/svelte/icons/scroll-text'
  import FolderOpen from '@lucide/svelte/icons/folder-open'
  import Copy from '@lucide/svelte/icons/copy'
  import Search from '@lucide/svelte/icons/search'
  import Square from '@lucide/svelte/icons/square'

  /** @typedef {{ name: string, rowCount?: number }} TableRow */
  /** @typedef {'pending'|'running'|'done'|'error'} TableStatus */
  /** @typedef {{ key: string, schema: string|null, name: string, est: number|null, status: TableStatus, rows: number, error: string|null }} RunTable */
  /** @typedef {import('$lib/api.js').BackupExportResult} ExportResult */
  /** @typedef {import('$lib/api.js').BackupImportResult} ImportResult */
  /** @typedef {import('$lib/api.js').BackupFileInfo} FileInfo */

  let {
    /** @type {string|null} */
    dbType = null,
    activeSchema = /** @type {string|null} */ (null),
    schemas = /** @type {string[]} */ ([]),
    /** @type {TableRow[]} */
    tables = [],
    /** After a restore that applied anything: reload the schema tree and tables. */
    onrestored = /** @type {() => void | Promise<void>} */ (() => {}),
    /** The saved connection behind this database, for scheduled backups. */
    savedConnection = /** @type {{ id: string, name?: string, type?: string } | null} */ (null),
  } = $props()

  // ── Live log ────────────────────────────────────────────────────────────────
  /** @typedef {{ id: number, at: number, level: string, text: string }} LogLine */
  /** Lines kept on the page; a long run drops the oldest. */
  const LOG_CAP = 2000
  /** @type {LogLine[]} */
  let logLines = $state([])
  let logSeq = 0
  let showLog = $state(false)
  /** @param {string} level @param {string} text */
  function addLog(level, text) {
    logLines.push({ id: logSeq++, at: Date.now(), level, text })
    if (logLines.length > LOG_CAP) logLines.splice(0, logLines.length - LOG_CAP)
  }
  /** @type {HTMLElement|null} */
  let logEl = $state(null)
  // Follow new lines while the reader is at the bottom; stay put when they scrolled up.
  let followLog = true
  $effect(() => {
    void logLines.length
    if (logEl && followLog) logEl.scrollTop = logEl.scrollHeight
  })
  const logTime = (/** @type {number} */ t) => new Date(t).toLocaleTimeString([], { hour12: false })

  /** @type {'backup'|'restore'|'schedule'} */
  let activeTab = $state('backup')

  // Engines whose dump covers one schema (or dataset) at a time.
  const supportsSchemaFilter = $derived(['postgres', 'mysql', 'spanner', 'bigquery', 'snowflake', 'odbc'].includes(dbType ?? ''))
  const isPostgres = $derived(dbType === 'postgres')
  const isSqliteFamily = $derived(dbType === 'sqlite' || dbType === 'd1' || dbType === 'libsql')
  const dbLabel = $derived(
    /** @type {Record<string, string>} */ ({
      postgres: 'PostgreSQL',
      mysql: 'MySQL',
      sqlite: 'SQLite',
      d1: 'Cloudflare D1',
      libsql: 'libSQL / Turso',
      duckdb: 'DuckDB',
      mssql: 'SQL Server',
      spanner: 'Spanner',
      bigquery: 'BigQuery',
      snowflake: 'Snowflake',
      odbc: 'ODBC',
    })[dbType ?? ''] ?? 'Database',
  )
  const cliName = $derived(isPostgres ? 'psql' : dbType === 'mysql' ? 'mysql' : isSqliteFamily ? 'sqlite3' : '')
  const revealLabel = IS_MAC
    ? 'Reveal in Finder'
    : typeof navigator !== 'undefined' && /Windows/i.test(navigator.userAgent)
      ? 'Show in Explorer'
      : 'Show in folder'

  const sectionLabel = 'text-ui-3xs font-semibold uppercase tracking-[0.06em] text-muted-foreground'

  // ── Formatting ────────────────────────────────────────────────────────────
  /** @param {number} n */
  const fmtInt = (n) => Math.round(n).toLocaleString()
  /** @param {number} n */
  const rowsText = (n) => `${fmtInt(n)} ${n === 1 ? 'row' : 'rows'}`
  /** @param {number} n */
  const tablesText = (n) => `${fmtInt(n)} ${n === 1 ? 'table' : 'tables'}`
  /** @param {number} n */
  const statementsText = (n) => `${fmtInt(n)} ${n === 1 ? 'statement' : 'statements'}`
  /** @param {number} n */
  function fmtCompact(n) {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
    if (n >= 10_000) return `${Math.round(n / 1000)}k`
    if (n >= 1_000) return `${(n / 1000).toFixed(1)}k`
    return String(n)
  }
  /** @param {number} b */
  function fmtBytes(b) {
    if (b >= 1_073_741_824) return `${(b / 1_073_741_824).toFixed(1)} GB`
    if (b >= 1_048_576) return `${(b / 1_048_576).toFixed(1)} MB`
    if (b >= 1_024) return `${(b / 1_024).toFixed(1)} KB`
    return `${b} B`
  }
  /** @param {number} ms */
  function fmtElapsed(ms) {
    const s = ms / 1000
    if (s < 60) return `${s.toFixed(1)} s`
    const m = Math.floor(s / 60)
    return `${m}:${String(Math.floor(s % 60)).padStart(2, '0')}`
  }
  /** @param {string} p */
  const baseName = (p) => p.split(/[\\/]/).pop() || p
  /** @param {unknown} e */
  const errText = (e) => String(/** @type {any} */ (e)?.message ?? e)

  // ── Shared run clock ──────────────────────────────────────────────────────
  let now = $state(Date.now())
  /** @type {ReturnType<typeof setInterval> | null} */
  let clock = null
  function startClock() {
    stopClock()
    now = Date.now()
    clock = setInterval(() => (now = Date.now()), 200)
  }
  function stopClock() {
    if (clock) clearInterval(clock)
    clock = null
  }

  /**
   * Subscribe to a Tauri event; resolves to an unlisten function (a no-op
   * outside the desktop app, e.g. the Vite-only dev server).
   * @param {string} name
   * @param {(payload: any) => void} handler
   */
  async function listenTo(name, handler) {
    try {
      const { listen } = await import('@tauri-apps/api/event')
      return await listen(name, (/** @type {any} */ e) => handler(e.payload))
    } catch {
      return () => {}
    }
  }
  /** @type {(() => void)[]} */
  let unlisteners = []
  function stopListening() {
    for (const u of unlisteners) u()
    unlisteners = []
  }

  onDestroy(() => {
    stopListening()
    stopClock()
  })

  // ── Export: options ───────────────────────────────────────────────────────
  let exportSchema = $state('')
  $effect(() => {
    exportSchema = activeSchema ?? ''
  })

  let optIncludeSchema = $state(true)
  let optIncludeData = $state(true)
  let optIncludeSequences = $state(true)
  let optIncludeEnums = $state(true)
  let optIncludeFunctions = $state(true)
  let optIncludeTriggers = $state(true)
  let optIncludeViews = $state(true)

  /** @type {{ id: string, label: string, get: () => boolean, set: (v: boolean) => void, show: boolean }[]} */
  const includeOptions = $derived([
    { id: 'schema', label: 'Schema and indexes', get: () => optIncludeSchema, set: (v) => (optIncludeSchema = v), show: true },
    { id: 'data', label: 'Data', get: () => optIncludeData, set: (v) => (optIncludeData = v), show: true },
    { id: 'views', label: 'Views', get: () => optIncludeViews, set: (v) => (optIncludeViews = v), show: true },
    { id: 'functions', label: 'Functions and procedures', get: () => optIncludeFunctions, set: (v) => (optIncludeFunctions = v), show: !isSqliteFamily },
    { id: 'triggers', label: 'Triggers', get: () => optIncludeTriggers, set: (v) => (optIncludeTriggers = v), show: true },
    { id: 'sequences', label: 'Sequences', get: () => optIncludeSequences, set: (v) => (optIncludeSequences = v), show: isPostgres },
    { id: 'enums', label: 'Enum types', get: () => optIncludeEnums, set: (v) => (optIncludeEnums = v), show: isPostgres },
  ].filter((o) => o.show))

  // Table selection. Reset to "all selected" only when the table list itself
  // changes (new connection/schema) - not on every toggle.
  /** @type {Set<string>} */
  let selectedTables = $state(new Set())
  let lastTableSig = ''
  $effect(() => {
    const sig = tables.map((t) => t.name).join('\u0000')
    if (sig !== lastTableSig) {
      lastTableSig = sig
      selectedTables = new Set(tables.map((t) => t.name))
    }
  })

  let tableQuery = $state('')
  const showTableSearch = $derived(tables.length > 8)
  const visibleTables = $derived.by(() => {
    const q = tableQuery.trim().toLowerCase()
    return q ? tables.filter((t) => t.name.toLowerCase().includes(q)) : tables
  })
  const estSelectedRows = $derived(
    tables.reduce((sum, t) => sum + (selectedTables.has(t.name) && t.rowCount != null && t.rowCount > 0 ? t.rowCount : 0), 0),
  )

  /** @param {string} name @param {boolean} on */
  function setTable(name, on) {
    const next = new Set(selectedTables)
    on ? next.add(name) : next.delete(name)
    selectedTables = next
  }
  function selectAll() {
    const next = new Set(selectedTables)
    for (const t of visibleTables) next.add(t.name)
    selectedTables = next
  }
  function selectNone() {
    const next = new Set(selectedTables)
    for (const t of visibleTables) next.delete(t.name)
    selectedTables = next
  }

  // ── Export: run state ─────────────────────────────────────────────────────
  /** @type {'idle'|'running'|'done'|'error'|'cancelled'} */
  let exportPhase = $state('idle')
  /** @type {ExportResult|null} */
  let exportResult = $state(null)
  let exportError = $state('')
  let exportStopping = $state(false)
  let exportPath = $state('')
  let exportStartedAt = $state(0)
  /** Warnings the backend logged (e.g. a catalog query it had to skip). */
  /** @type {string[]} */
  let exportNotes = $state([])
  /** @type {RunTable[]} */
  let runTables = $state([])
  /** @type {Map<string, number>} */
  let runIndex = new Map()

  const exportRunning = $derived(exportPhase === 'running')
  const nothingToExport = $derived(selectedTables.size === 0 || (!optIncludeSchema && !optIncludeData))
  const tablesDone = $derived(runTables.filter((t) => t.status === 'done' || t.status === 'error').length)
  const rowsDone = $derived(runTables.reduce((s, t) => s + t.rows, 0))
  const failedTables = $derived(runTables.filter((t) => t.status === 'error'))
  const multiSchema = $derived(new Set(runTables.map((t) => t.schema)).size > 1)
  const exportElapsed = $derived(
    exportResult ? exportResult.elapsedMs : exportStartedAt ? Math.max(0, now - exportStartedAt) : 0,
  )
  /** Rows-weighted progress; a table with no estimate still counts as one step. */
  const exportProgress = $derived.by(() => {
    if (exportPhase === 'done') return 1
    let total = 0
    let done = 0
    for (const t of runTables) {
      const est = Math.max(t.est ?? 0, 0)
      const w = est + 200
      total += w
      if (t.status === 'done' || t.status === 'error') done += w
      else if (t.status === 'running' && est > 0) done += Math.min(t.rows / est, 0.95) * w
    }
    return total ? done / total : 0
  })

  /** @param {string|null} schema @param {string} name */
  const tableKey = (schema, name) => `${schema ?? ''}\u0000${name}`

  /** @param {any} p */
  function onBackupProgress(p) {
    if (p?.kind === 'plan') {
      const known = new Map(tables.map((t) => [t.name, t.rowCount]))
      runIndex = new Map()
      runTables = (p.tables ?? []).map((/** @type {any} */ t, /** @type {number} */ i) => {
        const key = tableKey(t.schema, t.name)
        runIndex.set(key, i)
        const fallback = known.get(t.name)
        return {
          key,
          schema: t.schema ?? null,
          name: t.name,
          est: t.estRows ?? (fallback != null && fallback >= 0 ? fallback : null),
          status: /** @type {TableStatus} */ ('pending'),
          rows: 0,
          error: null,
        }
      })
      return
    }
    if (p?.kind !== 'table') return
    const key = tableKey(p.schema, p.name)
    let i = runIndex.get(key)
    if (i == null) {
      i = runTables.length
      runIndex.set(key, i)
      runTables.push({ key, schema: p.schema ?? null, name: p.name, est: null, status: 'pending', rows: 0, error: null })
    }
    const row = runTables[i]
    if (p.status !== row.status) {
      const label = p.schema && multiSchema ? `${p.schema}.${p.name}` : p.name
      if (p.status === 'running') addLog('info', `${label}: exporting`)
      else if (p.status === 'done') addLog('ok', `${label}: ${rowsText(p.rows ?? 0)}`)
      else if (p.status === 'error') addLog('error', `${label}: ${p.error ?? 'failed'}`)
    }
    row.status = p.status
    row.rows = p.rows ?? 0
    row.error = p.error ?? null
  }

  /** @param {any} p */
  function onBackupLog(p) {
    if (p?.message) addLog(p.level ?? 'info', p.message)
    if (p?.level === 'warn' && !/^Cancelled/.test(p.message)) exportNotes.push(p.message)
  }

  function defaultFileName() {
    const ts = new Date().toISOString().slice(0, 19).replace('T', '_').replace(/:/g, '-')
    const slug = dbLabel.toLowerCase().replace(/\s+/g, '_')
    const schemaSlug = supportsSchemaFilter && exportSchema ? `_${exportSchema}` : ''
    return `backup_${slug}${schemaSlug}_${ts}.sql`
  }

  async function runExport() {
    if (nothingToExport || exportRunning) return
    /** @type {string|null} */
    let path = null
    try {
      path = await pickBackupSavePath(defaultFileName())
    } catch (e) {
      toast.error('Could not open the save dialog', { description: errText(e) })
      return
    }
    if (!path) return

    exportPhase = 'running'
    exportResult = null
    exportError = ''
    exportStopping = false
    exportNotes = []
    exportPath = path
    runTables = []
    runIndex = new Map()
    logLines = []
    exportStartedAt = Date.now()
    startClock()
    stopListening()
    unlisteners = await Promise.all([listenTo('backup-progress', onBackupProgress), listenTo('backup-log', onBackupLog)])
    try {
      const schema = supportsSchemaFilter && exportSchema ? exportSchema : null
      const tableList = tables.length > 0 && selectedTables.size < tables.length ? [...selectedTables] : null
      const result = await backupExport(
        schema,
        tableList,
        {
          includeSchema: optIncludeSchema,
          includeData: optIncludeData,
          includeSequences: optIncludeSequences,
          includeEnums: optIncludeEnums,
          includeFunctions: optIncludeFunctions,
          includeTriggers: optIncludeTriggers,
          includeViews: optIncludeViews,
        },
        path,
      )
      if (result.cancelled) {
        // The partial file is deleted by the backend; nothing to offer.
        exportPhase = 'cancelled'
        return
      }
      exportResult = result
      exportPhase = 'done'
    } catch (e) {
      exportError = errText(e)
      exportPhase = 'error'
    } finally {
      stopListening()
      stopClock()
      // A stop or failure leaves tables mid-copy; they were never finished.
      for (const t of runTables) if (t.status === 'running') t.status = 'pending'
    }
  }

  function stopExport() {
    // Hold `running` until the in-flight export returns, so a new run can't
    // start while the old one is still going and reset the shared cancel flag.
    exportStopping = true
    backupCancel().catch(() => {})
  }

  async function revealExport() {
    if (!exportResult?.path) return
    try {
      await revealInFolder(exportResult.path)
    } catch (e) {
      toast.error(`Could not open the folder`, { description: errText(e) })
    }
  }

  async function copyExportPath() {
    if (!exportResult?.path) return
    try {
      await navigator.clipboard.writeText(exportResult.path)
      toast.success('Path copied')
    } catch {
      toast.error('Could not copy the path')
    }
  }

  // ── Restore ───────────────────────────────────────────────────────────────
  /** @type {'idle'|'running'|'done'|'error'} */
  let importPhase = $state('idle')
  /** @type {ImportResult|null} */
  let importResult = $state(null)
  let importError = $state('')
  let importPath = $state('')
  /** @type {FileInfo|null} */
  let importInfo = $state(null)
  let importConfirmed = $state(false)
  /** The encoding to read the file with; 'auto' is what the file looks like. */
  let importEncoding = $state('auto')

  /** Encodings dumps come in, by WHATWG label. */
  const ENCODINGS = [
    { value: 'utf-8', label: 'UTF-8' },
    { value: 'utf-16le', label: 'UTF-16 LE' },
    { value: 'utf-16be', label: 'UTF-16 BE' },
    { value: 'windows-1252', label: 'Latin-1 (Windows-1252)' },
    { value: 'iso-8859-2', label: 'Central European (ISO-8859-2)' },
    { value: 'windows-1251', label: 'Cyrillic (Windows-1251)' },
    { value: 'shift_jis', label: 'Japanese (Shift_JIS)' },
    { value: 'gbk', label: 'Chinese (GBK)' },
    { value: 'euc-kr', label: 'Korean (EUC-KR)' },
  ]
  const encodingName = (/** @type {string} */ v) => ENCODINGS.find((e) => e.value === v)?.label ?? v.toUpperCase()
  const encodingOptions = $derived([
    { value: 'auto', label: importInfo?.encoding ? `Detected: ${encodingName(importInfo.encoding)}` : 'Detect' },
    ...ENCODINGS,
  ])

  /** Read the file again with another encoding, so the preview shows what will run. */
  async function changeEncoding(/** @type {string} */ v) {
    importEncoding = v
    if (!importPath || importInfo?.encrypted) return
    const path = importPath
    try {
      const info = await backupInspectFile(path, v === 'auto' ? null : v)
      if (importPath === path) importInfo = info
    } catch (e) {
      toast.error('Could not read the file', { description: errText(e) })
    }
  }
  let importStopping = $state(false)
  let importStartedAt = $state(0)
  let importProgress = $state({ bytesDone: 0, bytesTotal: 0, ok: 0, failed: 0 })

  const importRunning = $derived(importPhase === 'running')
  const importElapsed = $derived(
    importResult ? importResult.elapsedMs : importStartedAt ? Math.max(0, now - importStartedAt) : 0,
  )
  const importFraction = $derived(
    importPhase === 'done' ? 1 : importProgress.bytesTotal ? importProgress.bytesDone / importProgress.bytesTotal : 0,
  )
  /** The file's engine when it differs from the connection's. */
  const engineMismatch = $derived.by(() => {
    const e = importInfo?.engine?.toLowerCase()
    if (!e || !dbType) return null
    const ok =
      dbType === 'postgres' ? e.includes('postgres')
      : dbType === 'mysql' ? e.includes('mysql') || e.includes('maria')
      : dbType === 'sqlite' || dbType === 'd1' || dbType === 'libsql' ? e.includes('sqlite') || e.includes('d1') || e.includes('libsql')
      : dbType === 'spanner' || dbType === 'bigquery' || dbType === 'snowflake' || dbType === 'odbc' ? e.includes(dbType)
      : true
    return ok ? null : importInfo?.engine ?? null
  })

  async function chooseFile() {
    /** @type {string|null} */
    let path = null
    try {
      path = await pickBackupFile()
    } catch (e) {
      toast.error('Could not open the file dialog', { description: errText(e) })
      return
    }
    if (!path) return
    importPath = path
    importInfo = null
    importConfirmed = false
    importEncoding = 'auto'
    try {
      importInfo = await backupInspectFile(path)
    } catch (e) {
      toast.error('Could not read the file', { description: errText(e) })
      importPath = ''
    }
  }

  function clearFile() {
    importPath = ''
    importInfo = null
    importConfirmed = false
  }

  async function runImport() {
    // Restoring a dump is the largest write the app can make.
    if (!guardWrite('restore a backup into this database')) return
    if (!importPath || importRunning) return
    importPhase = 'running'
    importResult = null
    importError = ''
    importStopping = false
    importProgress = { bytesDone: 0, bytesTotal: importInfo?.size ?? 0, ok: 0, failed: 0 }
    importStartedAt = Date.now()
    startClock()
    stopListening()
    logLines = []
    let lastTenth = -1
    unlisteners = await Promise.all([
      listenTo('restore-progress', (p) => {
        importProgress = p
        // A line every 10% of the file, so the log moves on a clean run too.
        const tenth = p?.bytesTotal ? Math.floor((p.bytesDone / p.bytesTotal) * 10) : -1
        if (tenth > lastTenth && tenth < 10) {
          lastTenth = tenth
          addLog('info', `${tenth * 10}% of the file · ${statementsText(p.ok)} applied${p.failed ? ` · ${fmtInt(p.failed)} failed` : ''}`)
        }
      }),
      listenTo('restore-log', (p) => p?.message && addLog(p.level ?? 'info', p.message)),
    ])
    try {
      const result = await backupImport({ path: importPath, encoding: importEncoding === 'auto' ? null : importEncoding })
      importResult = result
      importPhase = 'done'
      if (result.statementsOk > 0) {
        addLog('info', 'Refreshing the schema')
        try { await onrestored() } catch { /* the sidebar's own refresh still works */ }
      }
    } catch (e) {
      importError = errText(e)
      importPhase = 'error'
    } finally {
      stopListening()
      stopClock()
    }
  }

  function stopImport() {
    // Stay in `running` until the backend settles: the cancel flag is global,
    // and the backend reports what it actually did (Postgres rolls back).
    importStopping = true
    backupCancel().catch(() => {})
  }

  function resetImport() {
    importPhase = 'idle'
    importResult = null
    importError = ''
    importConfirmed = false
    importProgress = { bytesDone: 0, bytesTotal: 0, ok: 0, failed: 0 }
  }

  // One page serves every connection. After switching, it showed the last
  // database's "Export complete · products · 3 rows" over a database with no
  // products table, and kept a dump file chosen for the other one. What has
  // finished is put away; a run still going stays on screen.
  // svelte-ignore state_referenced_locally
  let shownFor = savedConnection?.id ?? ''
  $effect(() => {
    const id = savedConnection?.id ?? ''
    if (id === shownFor) return
    shownFor = id
    untrack(() => {
      if (exportPhase !== 'running') { exportPhase = 'idle'; exportResult = null; exportError = '' }
      if (importPhase !== 'running') { resetImport(); importPath = ''; importInfo = null }
    })
  })
</script>

{#snippet logToggle()}
  <Button
    variant={showLog ? 'secondary' : 'ghost'}
    size="sm"
    class="h-7 gap-1.5"
    aria-pressed={showLog}
    onclick={() => (showLog = !showLog)}
  >
    <ScrollText class="size-3.5 shrink-0" />Log
  </Button>
{/snippet}

{#snippet logPanel()}
  <!-- Focusable so the log can be scrolled from the keyboard. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    bind:this={logEl}
    onscroll={() => { if (logEl) followLog = logEl.scrollHeight - logEl.scrollTop - logEl.clientHeight < 24 }}
    class="app-scroll min-h-0 flex-1 overflow-y-auto border-t border-border/60 bg-background/40 px-4 py-2 font-mono text-ui-xs leading-relaxed"
    role="log"
    aria-label="Live log"
    tabindex="0"
  >
    {#each logLines as line (line.id)}
      <div class="flex gap-3">
        <span class="shrink-0 tabular-nums text-muted-foreground">{logTime(line.at)}</span>
        <span
          class={cn(
            'min-w-0 break-words whitespace-pre-wrap',
            line.level === 'error' ? 'text-destructive' : line.level === 'warn' ? 'text-warning' : line.level === 'ok' ? 'text-success' : 'text-foreground/85',
          )}
        >{line.text}</span>
      </div>
    {:else}
      <p class="text-muted-foreground">Waiting for the first line…</p>
    {/each}
  </div>
{/snippet}

{#snippet progressBar(/** @type {number} */ fraction, /** @type {'running'|'done'|'warn'|'error'} */ tone)}
  <div
    class="h-1.5 w-full overflow-hidden rounded-full bg-muted"
    role="progressbar"
    aria-valuemin="0"
    aria-valuemax="100"
    aria-valuenow={Math.round(fraction * 100)}
  >
    <div
      class={cn(
        'h-full rounded-full transition-[width] duration-200 ease-out',
        tone === 'error' ? 'bg-destructive' : tone === 'warn' ? 'bg-warning' : tone === 'done' ? 'bg-success' : 'bg-primary',
      )}
      style:width="{Math.max(0, Math.min(1, fraction)) * 100}%"
    ></div>
  </div>
{/snippet}

{#snippet statusIcon(/** @type {TableStatus} */ status)}
  {#if status === 'running'}
    <LoaderCircle class="size-3.5 shrink-0 animate-spin text-primary" aria-label="Exporting" />
  {:else if status === 'done'}
    <Check class="size-3.5 shrink-0 text-success" aria-label="Done" />
  {:else if status === 'error'}
    <X class="size-3.5 shrink-0 text-destructive" aria-label="Failed" />
  {:else}
    <Circle class="size-3.5 shrink-0 text-muted-foreground" aria-label="Waiting" />
  {/if}
{/snippet}

{#snippet emptyState(/** @type {any} */ Icon, /** @type {string} */ title, /** @type {string} */ body)}
  <EmptyState class="flex-1 justify-center" icon={Icon} {title} description={body} />
{/snippet}

<div class="flex min-h-0 flex-1 flex-col overflow-hidden bg-panel">
  <!-- Header: Backup / Restore -->
  <div class="studio-chrome flex h-11 shrink-0 items-center gap-3 border-b border-border/60 px-3" data-studio-chrome>
    <Tabs.Root bind:value={activeTab}>
      <Tabs.List class="h-8">
        <Tabs.Trigger value="backup" class="px-3 text-ui-xs" disabled={importRunning}>
          <Download class="size-3.5 shrink-0" />Backup
        </Tabs.Trigger>
        <Tabs.Trigger value="restore" class="px-3 text-ui-xs" disabled={exportRunning}>
          <Upload class="size-3.5 shrink-0" />Restore
        </Tabs.Trigger>
        <Tabs.Trigger value="schedule" class="px-3 text-ui-xs">
          <CalendarClock class="size-3.5 shrink-0" />Scheduled
        </Tabs.Trigger>
      </Tabs.List>
    </Tabs.Root>
    <span class="min-w-0 flex-1"></span>
    <span class="min-w-0 truncate text-ui-xs text-muted-foreground">{dbLabel}</span>
  </div>

  {#if activeTab === 'schedule'}
    <ScheduledBackups connection={savedConnection} />
  {:else if activeTab === 'backup'}
    <div class="flex min-h-0 flex-1 overflow-hidden">
      <!-- Left: what to export -->
      <aside class="flex w-80 shrink-0 flex-col border-r border-border/60" aria-label="Backup options">
        <div class="app-scroll flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto p-4">
          {#if supportsSchemaFilter && schemas.length > 0}
            <section class="flex flex-col gap-2">
              <label for="backup-schema" class={sectionLabel}>Schema</label>
              <FieldSelect
                id="backup-schema"
                class="w-full"
                bind:value={exportSchema}
                disabled={exportRunning}
                placeholder="All schemas"
                options={[{ value: '', label: 'All schemas' }, ...schemas.map((s) => ({ value: s, label: s }))]}
              />
            </section>
          {/if}

          <section class="flex flex-col gap-1">
            <h2 class={cn(sectionLabel, 'mb-1')}>Include</h2>
            {#each includeOptions as opt (opt.id)}
              <label class="flex h-7 cursor-pointer items-center gap-2.5 text-ui-sm text-foreground">
                <Checkbox
                  checked={opt.get()}
                  onCheckedChange={(v) => opt.set(v === true)}
                  disabled={exportRunning}
                />
                {opt.label}
              </label>
            {/each}
          </section>

          <!-- The list takes the column's remaining height: capped at eight rows,
               it scrolled in a small box above an empty column. min-h keeps a
               usable list on a short window, where the column scrolls instead. -->
          <section class="flex min-h-56 flex-1 flex-col gap-2">
            <div class="flex h-6 shrink-0 items-center gap-2">
              <h2 class={sectionLabel}>Tables</h2>
              {#if tables.length > 0}
                <span class="font-mono text-ui-2xs tabular-nums text-muted-foreground">
                  {selectedTables.size} of {tables.length}
                </span>
                <span class="flex-1"></span>
                <Button variant="ghost" size="xs" class="text-muted-foreground" onclick={selectAll} disabled={exportRunning}>All</Button>
                <Button variant="ghost" size="xs" class="text-muted-foreground" onclick={selectNone} disabled={exportRunning}>None</Button>
              {/if}
            </div>

            {#if tables.length === 0}
              <p class="text-ui-xs text-muted-foreground">No tables in this {supportsSchemaFilter ? 'schema' : 'database'}.</p>
            {:else}
              <div class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border/60">
                {#if showTableSearch}
                  <div class="relative shrink-0 border-b border-border/50">
                    <Search class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 shrink-0 -translate-y-1/2 text-muted-foreground" />
                    <input
                      bind:value={tableQuery}
                      type="search"
                      placeholder="Filter tables"
                      aria-label="Filter tables"
                      spellcheck="false"
                      autocomplete="off"
                      class="no-field-frame no-focus-ring h-8 w-full bg-transparent pr-2.5 pl-8 text-ui-sm text-foreground outline-none placeholder:text-muted-foreground"
                    />
                  </div>
                {/if}
                <ul class="app-scroll min-h-0 flex-1 overflow-y-auto" aria-label="Tables to export">
                  {#each visibleTables as t (t.name)}
                    {@const checked = selectedTables.has(t.name)}
                    <li>
                      <label class="flex h-8 cursor-pointer items-center gap-2.5 px-2.5 transition-colors hover:bg-muted/50">
                        <Checkbox {checked} onCheckedChange={(v) => setTable(t.name, v === true)} disabled={exportRunning} />
                        <span class={cn('min-w-0 flex-1 truncate font-mono text-ui-xs', checked ? 'text-foreground' : 'text-muted-foreground')}>{t.name}</span>
                        {#if t.rowCount != null && t.rowCount >= 0}
                          <span class="shrink-0 font-mono text-ui-2xs tabular-nums text-muted-foreground">{fmtCompact(t.rowCount)}</span>
                        {/if}
                      </label>
                    </li>
                  {:else}
                    <li class="flex h-8 items-center px-2.5 text-ui-xs text-muted-foreground">No tables match</li>
                  {/each}
                </ul>
              </div>
            {/if}
          </section>
        </div>

        <div class="flex shrink-0 items-center gap-2 border-t border-border/60 p-3">
          {#if exportRunning}
            <Button variant="outline" class="flex-1" onclick={stopExport} disabled={exportStopping}>
              <Square class="size-3.5 shrink-0" />{exportStopping ? 'Stopping…' : 'Stop export'}
            </Button>
          {:else}
            <Button class="flex-1" onclick={() => void runExport()} disabled={nothingToExport}>
              <Download class="size-3.5 shrink-0" />
              Export {tablesText(selectedTables.size)}…
            </Button>
          {/if}
        </div>
      </aside>

      <!-- Right: the run -->
      <section class="flex min-w-0 flex-1 flex-col" aria-label="Export progress">
        {#if exportPhase === 'idle'}
          {@render emptyState(
            Download,
            'Export to a .sql file',
            `${tablesText(selectedTables.size)}${estSelectedRows ? `, about ${rowsText(estSelectedRows)}` : ''}. The file restores here${cliName ? ` or with ${cliName}` : ''}.`,
          )}
        {:else}
          <div class="flex shrink-0 flex-col gap-3 border-b border-border/60 p-4">
            <div class="flex items-baseline gap-3">
              <p class="text-ui-sm font-medium text-foreground">
                {#if exportPhase === 'running'}{exportStopping ? 'Stopping…' : 'Exporting…'}
                {:else if exportPhase === 'done'}{exportResult?.failedTables ? 'Exported with errors' : 'Export complete'}
                {:else if exportPhase === 'cancelled'}Export stopped
                {:else}Export failed{/if}
              </p>
              <span class="flex-1"></span>
              {@render logToggle()}
              <span class="font-mono text-ui-xs tabular-nums text-muted-foreground">{fmtElapsed(exportElapsed)}</span>
            </div>
            {@render progressBar(
              exportProgress,
              exportPhase === 'error' ? 'error' : exportPhase === 'done' ? (exportResult?.failedTables || exportNotes.length ? 'warn' : 'done') : 'running',
            )}
            <p class="font-mono text-ui-xs tabular-nums text-muted-foreground">
              {#if runTables.length}
                {[
                  `${fmtInt(tablesDone)} of ${tablesText(runTables.length)}`,
                  rowsText(exportResult?.rowCount ?? rowsDone),
                  exportResult ? fmtBytes(exportResult.bytes) : '',
                ].filter(Boolean).join(' · ')}
              {:else}Reading the catalog…{/if}
            </p>

            {#if exportPhase === 'done' && exportResult?.path}
              <div class="flex items-center gap-3 rounded-lg border border-border/60 bg-background/40 py-2 pr-2 pl-3">
                <FileText class="size-4 shrink-0 text-muted-foreground" />
                <div class="min-w-0 flex-1">
                  <p class="truncate text-ui-sm text-foreground">Saved to {baseName(exportResult.path)}</p>
                  <p class="truncate font-mono text-ui-2xs text-muted-foreground" title={exportResult.path}>{exportResult.path}</p>
                </div>
                <Button variant="ghost" size="icon-sm" onclick={() => void copyExportPath()} aria-label="Copy path" title="Copy path">
                  <Copy class="size-3.5 shrink-0" />
                </Button>
                <Button variant="secondary" size="sm" onclick={() => void revealExport()}>
                  <FolderOpen class="size-3.5 shrink-0" />{revealLabel}
                </Button>
              </div>
            {:else if exportPhase === 'cancelled'}
              <p class="text-ui-xs text-muted-foreground">Nothing was saved: a partial dump is not a backup, so it was discarded.</p>
            {/if}

            {#if exportPhase === 'error'}
              <Alert.Root variant="destructive">
                <CircleAlert />
                <Alert.Title>Could not export</Alert.Title>
                <Alert.Description class="break-words whitespace-pre-wrap">{exportError}</Alert.Description>
              </Alert.Root>
            {/if}
            {#if failedTables.length}
              <Alert.Root variant="destructive">
                <CircleAlert />
                <Alert.Title>{tablesText(failedTables.length)} could not be exported</Alert.Title>
                <Alert.Description>
                  <ul class="flex flex-col gap-1">
                    {#each failedTables as t (t.key)}
                      <li class="break-words"><span class="font-mono">{t.schema && multiSchema ? `${t.schema}.` : ''}{t.name}</span>: {t.error}</li>
                    {/each}
                  </ul>
                </Alert.Description>
              </Alert.Root>
            {/if}
            {#if exportNotes.length}
              <Alert.Root>
                <TriangleAlert class="text-warning" />
                <Alert.Title>{exportNotes.length === 1 ? 'One thing was left out' : `${exportNotes.length} things were left out`}</Alert.Title>
                <Alert.Description>
                  <ul class="flex flex-col gap-1">
                    {#each exportNotes as note, i (i)}<li class="break-words">{note}</li>{/each}
                  </ul>
                </Alert.Description>
              </Alert.Root>
            {/if}
          </div>

          {#if showLog}
            {@render logPanel()}
          {:else if runTables.length}
            <ul class="app-scroll min-h-0 flex-1 overflow-y-auto py-1" aria-label="Tables">
              {#each runTables as t (t.key)}
                <li class="flex h-8 items-center gap-2.5 px-4" title={t.error ?? undefined}>
                  {@render statusIcon(t.status)}
                  <span class={cn('min-w-0 flex-1 truncate font-mono text-ui-xs', t.status === 'pending' ? 'text-muted-foreground' : 'text-foreground')}>
                    {#if t.schema && multiSchema}<span class="text-muted-foreground">{t.schema}.</span>{/if}{t.name}
                  </span>
                  <span class={cn('shrink-0 font-mono text-ui-2xs tabular-nums', t.status === 'error' ? 'text-destructive' : 'text-muted-foreground')}>
                    {#if t.status === 'error'}failed{:else if t.status === 'pending'}{t.est ? `~${fmtCompact(t.est)}` : ''}{:else}{rowsText(t.rows)}{/if}
                  </span>
                </li>
              {/each}
            </ul>
          {/if}
        {/if}
      </section>
    </div>
  {:else}
    <div class="flex min-h-0 flex-1 overflow-hidden">
      <!-- Left: file + confirm -->
      <aside class="flex w-80 shrink-0 flex-col border-r border-border/60" aria-label="Restore options">
        <div class="app-scroll flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto p-4">
          <section class="flex flex-col gap-2">
            <h2 class={sectionLabel}>Backup file</h2>
            {#if importPath}
              <div class="flex items-start gap-3 rounded-lg border border-border/60 bg-background/40 p-3">
                <FileText class="mt-0.5 size-4 shrink-0 text-muted-foreground" />
                <div class="min-w-0 flex-1">
                  <p class="truncate text-ui-sm text-foreground" title={importPath}>{baseName(importPath)}</p>
                  <p class="truncate text-ui-xs text-muted-foreground">
                    {importInfo ? [fmtBytes(importInfo.size), importInfo.engine, importInfo.stroke ? 'Stroke backup' : ''].filter(Boolean).join(' · ') : '…'}
                  </p>
                </div>
                {#if !importRunning}
                  <Button variant="ghost" size="icon-xs" class="text-muted-foreground" onclick={clearFile} aria-label="Remove file" title="Remove file">
                    <X class="size-3.5 shrink-0" />
                  </Button>
                {/if}
              </div>
              {#if !importRunning}
                <Button variant="ghost" size="sm" class="-ml-2 self-start text-muted-foreground" onclick={() => void chooseFile()}>
                  Choose another file…
                </Button>
              {/if}
            {:else}
              <Button variant="outline" class="self-start" onclick={() => void chooseFile()}>
                <FolderOpen class="size-3.5 shrink-0" />Choose file…
              </Button>
              <p class="text-ui-xs leading-relaxed text-muted-foreground">
                A .sql dump from Stroke{isPostgres ? ', pg_dump' : dbType === 'mysql' ? ', mysqldump' : isSqliteFamily ? ' or sqlite3 .dump' : ''}{isPostgres || dbType === 'mysql' ? ' or another tool' : ''}.
              </p>
            {/if}

            {#if importInfo?.encrypted}
              <Alert.Root>
                <LockKeyhole />
                <Alert.Title>End-to-end encrypted</Alert.Title>
                <Alert.Description>A scheduled backup. It opens on this computer with your sync key; unlock sync first if it's locked.</Alert.Description>
              </Alert.Root>
            {/if}
            {#if importInfo?.incomplete}
              <Alert.Root variant="destructive">
                <CircleAlert />
                <Alert.Title>Incomplete backup</Alert.Title>
                <Alert.Description>This export was cancelled before it finished. Restoring it loads only part of the data.</Alert.Description>
              </Alert.Root>
            {/if}
            {#if engineMismatch}
              <Alert.Root>
                <TriangleAlert class="text-warning" />
                <Alert.Title>Different engine</Alert.Title>
                <Alert.Description>This looks like a {engineMismatch} dump, and you're connected to {dbLabel}.</Alert.Description>
              </Alert.Root>
            {/if}
          </section>

          {#if importPath && importInfo && !importInfo.encrypted && importPhase === 'idle'}
            <section class="flex flex-col gap-2">
              <label for="restore-encoding" class={sectionLabel}>Encoding</label>
              <FieldSelect
                id="restore-encoding"
                class="w-full"
                value={importEncoding}
                options={encodingOptions}
                onchange={(v) => void changeEncoding(v)}
              />
            </section>
          {/if}

          {#if importPath && importPhase === 'idle'}
            <section class="flex flex-col gap-2">
              <h2 class={sectionLabel}>Confirm</h2>
              <label class="flex cursor-pointer items-start gap-2.5 text-ui-sm leading-snug text-foreground">
                <Checkbox class="mt-px" checked={importConfirmed} onCheckedChange={(v) => (importConfirmed = v === true)} />
                <span>
                  Run this file against <span class="font-medium">{dbLabel}</span>. Rows that already exist with the same keys are {isSqliteFamily ? 'replaced' : 'kept'}{isPostgres ? '; the restore is one transaction, so stopping it undoes everything' : ''}.
                </span>
              </label>
            </section>
          {/if}
        </div>

        <div class="flex shrink-0 items-center gap-2 border-t border-border/60 p-3">
          {#if importRunning}
            <Button variant="outline" class="flex-1" onclick={stopImport} disabled={importStopping}>
              <Square class="size-3.5 shrink-0" />{importStopping ? 'Stopping…' : 'Stop restore'}
            </Button>
          {:else if importPhase === 'done' || importPhase === 'error'}
            <Button variant="outline" class="flex-1" onclick={resetImport}>New restore</Button>
          {:else}
            <Button
              class="flex-1"
              onclick={() => void runImport()}
              disabled={!importPath || !importConfirmed || $readOnlyMode}
              title={$readOnlyMode ? READ_ONLY_HINT : undefined}
            >
              <Upload class="size-3.5 shrink-0" />Restore
            </Button>
          {/if}
        </div>
      </aside>

      <!-- Right: the run -->
      <section class="flex min-w-0 flex-1 flex-col" aria-label="Restore progress">
        {#if importPhase === 'idle' && importInfo?.preview}
          <div class="flex h-9 shrink-0 items-center gap-2 border-b border-border/60 px-4">
            <FileText class="size-3.5 shrink-0 text-muted-foreground" />
            <p class="min-w-0 flex-1 truncate text-ui-xs text-muted-foreground">
              {importInfo.previewTruncated ? 'First 100,000 characters' : 'The whole file'}
            </p>
          </div>
          <div class="flex min-h-0 flex-1 flex-col">
            {#await import('./CodeEditor.svelte') then { default: CodeEditor }}
              <CodeEditor value={importInfo.preview} readOnly lang="sql" ariaLabel="Preview of the file" />
            {/await}
          </div>
        {:else if importPhase === 'idle'}
          {@render emptyState(
            Upload,
            'Restore from a .sql file',
            `Statements run against the connected ${dbLabel} database in order. Errors are collected, not fatal, and listed here when it finishes.`,
          )}
        {:else}
          <div class="flex shrink-0 flex-col gap-3 p-4">
            <div class="flex items-baseline gap-3">
              <p class="text-ui-sm font-medium text-foreground">
                {#if importPhase === 'running'}{importStopping ? 'Stopping…' : 'Restoring…'}
                {:else if importPhase === 'error'}Restore failed
                {:else if importResult?.cancelled}Restore stopped
                {:else if importResult?.statementsErr}Restored with errors
                {:else}Restore complete{/if}
              </p>
              <span class="flex-1"></span>
              {@render logToggle()}
              <span class="font-mono text-ui-xs tabular-nums text-muted-foreground">{fmtElapsed(importElapsed)}</span>
            </div>
            {@render progressBar(
              importFraction,
              importPhase === 'error' ? 'error' : importPhase !== 'done' ? 'running' : importResult?.cancelled || importResult?.statementsErr ? 'warn' : 'done',
            )}
            <p class="font-mono text-ui-xs tabular-nums text-muted-foreground">
              {#if importResult}
                {[
                  `${statementsText(importResult.statementsOk)} applied`,
                  importResult.statementsErr ? `${fmtInt(importResult.statementsErr)} failed` : '',
                  importResult.statementsSkipped ? `${fmtInt(importResult.statementsSkipped)} not run` : '',
                ].filter(Boolean).join(' · ')}
              {:else}
                {[
                  `${fmtBytes(importProgress.bytesDone)} of ${fmtBytes(importProgress.bytesTotal)}`,
                  `${statementsText(importProgress.ok)} applied`,
                  importProgress.failed ? `${fmtInt(importProgress.failed)} failed` : '',
                ].filter(Boolean).join(' · ')}
              {/if}
            </p>

            {#if importResult?.cancelled}
              <p class="text-ui-xs text-muted-foreground">
                {importResult.statementsOk === 0 ? 'Nothing was applied: the restore was rolled back.' : `${statementsText(importResult.statementsOk)} were applied before it stopped.`}
              </p>
            {/if}
            {#if importPhase === 'error'}
              <Alert.Root variant="destructive">
                <CircleAlert />
                <Alert.Title>Could not restore</Alert.Title>
                <Alert.Description class="break-words whitespace-pre-wrap">{importError}</Alert.Description>
              </Alert.Root>
            {/if}
          </div>

          {#if showLog}
            {@render logPanel()}
          {:else if importResult?.errors.length}
            <div class="flex min-h-0 flex-1 flex-col px-4 pb-4">
              <Alert.Root variant="destructive" class="flex min-h-0 flex-col">
                <div class="flex items-center gap-2">
                  <CircleAlert class="size-4 shrink-0" />
                  <Alert.Title>{statementsText(importResult.errors.length)} failed</Alert.Title>
                </div>
                <ul class="app-scroll mt-1 min-h-0 flex-1 overflow-y-auto">
                  {#each importResult.errors as err, i (i)}
                    <li class="border-t border-border/40 py-1.5 font-mono text-ui-xs break-words whitespace-pre-wrap first:border-t-0">{err}</li>
                  {/each}
                </ul>
              </Alert.Root>
            </div>
          {/if}
        {/if}
      </section>
    </div>
  {/if}
</div>
