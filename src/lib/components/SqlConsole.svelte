<script>
  import FieldSelect from './FieldSelect.svelte';
  import { rowsToCsv, rowsToJson, rowsToSql, rowsToTsv, rowsToMarkdown, rowsToJsonl, rowsToObjects, saveExportFile, buildExportFilename } from '$lib/export.js'
  import Play from "@lucide/svelte/icons/play";
  import WifiOff from "@lucide/svelte/icons/wifi-off";
  import Braces from "@lucide/svelte/icons/braces";
  import Wand2 from "@lucide/svelte/icons/wand-2";
  import CheckCheck from "@lucide/svelte/icons/check-check";
  import Copy from "@lucide/svelte/icons/copy";
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Loader2 from "@lucide/svelte/icons/loader-2";
  import History from "@lucide/svelte/icons/history";
  import Bookmark from "@lucide/svelte/icons/bookmark";
  import Code2 from "@lucide/svelte/icons/code-2";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Download from "@lucide/svelte/icons/download";
  import Table2 from "@lucide/svelte/icons/table-2";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import { cn, isNetworkError } from "$lib/utils.js";
  import { hasPro } from '$lib/stores/license.js'
  import SqlEditor from "./SqlEditor.svelte";
  import { sqlToDrizzle, sqlToPrisma } from "$lib/orm-builder.js";
  import QueryHistoryPanel from "./QueryHistoryPanel.svelte";
  import * as Dialog from "$lib/components/ui/dialog/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { queryTitle } from "$lib/stores/query-history.js";
  import DataTable from "./DataTable.svelte";
  import TableLoading from "./TableLoading.svelte";
  import JsonViewer from "./JsonViewer.svelte";
  import ChartView from "./ChartView.svelte";
  import BarChart2 from "@lucide/svelte/icons/bar-chart-2";
  import ScanSearch from "@lucide/svelte/icons/scan-search";
  import ResizeHandle from "./ResizeHandle.svelte";
  import ExplainPlan from "./ExplainPlan.svelte";
  import { explainSql, cancelQuery } from "$lib/api.js";
  import Square from "@lucide/svelte/icons/square";
  import Variable from "@lucide/svelte/icons/variable";
  import X from "@lucide/svelte/icons/x";
  import TextCursorInput from "@lucide/svelte/icons/text-cursor-input";
  import TextSelect from "@lucide/svelte/icons/text-select";
  import { Button } from "$lib/components/ui/button/index.js";
  import {
    extractSqlParams,
    missingSqlParams,
    substituteSqlParams,
    dialectForEngine,
    loadStoredParamValues,
    saveStoredParamValues,
  } from "$lib/sql-params.js";
  import {
    clampSqlEditorHeight,
    loadLayout,
    saveLayout,
  } from "$lib/stores/layout.js";
  import { untrack, onDestroy } from "svelte";
  import { formatCompactCount } from "$lib/table-list.js";

  /** @typedef {import('$lib/monaco-sql-complete.js').SqlSchemaHints} SqlSchemaHints */

  let {
    /** Whether the SQL tab is the active/visible tab - gates global hotkeys. */
    active = true,
    /** Connection family, so parameter values are quoted the way this engine reads them. */
    engine = "postgres",
    sql = $bindable("SELECT 1;"),
    columns = [],
    rows = [],
    queryMs = 0,
    message = "",
    loading = false,
    /** Cancel handle for the run in flight in THIS tab - so Stop can't cancel
     *  a query another tab happens to be running at the same time. */
    runningQueryId = /** @type {string | null} */ (null),
    error = "",
    /** @type {any[]} */
    multiResults = [],
    schemaHints = /** @type {SqlSchemaHints} */ ({}),
    /** Run SQL - receives a single-statement override, or undefined to run the whole buffer. */
    onrun = (/** @type {string | undefined} */ statementSql) => {},
    /** Open transaction for this tab, or null when running in autocommit. */
    txStatus = null,
    txBusy = false,
    onbegintransaction = () => {},
    oncommittransaction = () => {},
    onrollbacktransaction = () => {},
    onmodk = undefined,
    onmods = undefined,
    onmodi = undefined,
    onmodw = undefined,
    onmodn = undefined,
    onmodm = undefined,
    onmodt = undefined,
    onmodshifte = undefined,
    onmodshiftd = undefined,
    onmodaltd = undefined,
    onmodshifto = undefined,
    onmodshiftb = undefined,
    queryHistoryVisible = $bindable(false),
    /** @type {import('$lib/stores/query-history.js').QueryHistoryEntry[]} */
    queryHistory = [],
    /** @type {import('$lib/stores/query-history.js').SavedQuery[]} */
    savedQueries = [],
    onqueryrefresh = async () => {},
    /** @param {string} sql */
    onhistoryselect = (sql) => {},
    /** @param {string} name @param {string} sql */
    onsavequery = async (name, sql) => {},
    /** Called when user clicks "Fix with AI" - parent opens sidebar and sends the message */
    /** @param {{ error: string, sql: string }} detail */
    onfixwithai = /** @type {((detail: { error: string, sql: string }) => void) | undefined} */ (undefined),
    onprorequired = /** @type {() => void} */ (() => {}),
  } = $props();

  /** @type {{ focus: () => void, markExecuted: (ranStatement?: string | null) => void, getStatementAtCursor: () => string, getSelectionText: () => string } | null} */
  let sqlEditorRef = $state(null)

  /** Focus the SQL editor - called by the parent when this tab becomes active. */
  export function focusEditor() {
    sqlEditorRef?.focus()
  }

  /**
   * Replace the editor content with `content` and focus it. Used by
   * "Open in SQL editor" and history/AI flows.
   * @param {string} content
   */
  export function openQuery(content) {
    sql = content
    queueMicrotask(() => sqlEditorRef?.focus())
  }

  /** The statement that ran last (⌘R), or null when the whole buffer ran. */
  let lastRanStatement = /** @type {string | null} */ (null)

  // ── Named parameters (:name) ────────────────────────────────────────────────
  /** @type {Record<string, import('$lib/sql-params.js').SqlParamValue>} */
  let paramValues = $state(loadStoredParamValues())
  let paramsPanelOpen = $state(false)
  const sqlParams = $derived(extractSqlParams(sql))

  /** @param {string} name @param {import('$lib/sql-params.js').SqlParamValue} next */
  function setParam(name, next) {
    paramValues = { ...paramValues, [name]: next }
    saveStoredParamValues(paramValues)
  }

  /** @param {string | undefined} statementSql */
  function handleRun(statementSql) {
    const single = typeof statementSql === 'string' && statementSql.trim() ? statementSql : undefined
    const target = single ?? sql
    if (extractSqlParams(target).length > 0) {
      // Block the run until every parameter has a usable value, then inline
      // them as escaped literals - the substituted text is what executes (and
      // what history records), so runs stay reproducible.
      if (missingSqlParams(target, paramValues).length > 0) {
        paramsPanelOpen = true
        return
      }
      lastRanStatement = single ?? null
      onrun(substituteSqlParams(target, paramValues, dialectForEngine(engine)))
      return
    }
    lastRanStatement = single ?? null
    onrun(single)
  }

  // ── Run split-button dropdown ───────────────────────────────────────────────
  let runMenuOpen = $state(false)
  let cursorStmtPreview = $state('')
  let selectionPreview = $state('')

  /** @param {boolean} open */
  function captureRunPreviews(open) {
    runMenuOpen = open
    if (!open) return
    cursorStmtPreview = sqlEditorRef?.getStatementAtCursor?.() ?? ''
    selectionPreview = sqlEditorRef?.getSelectionText?.() ?? ''
  }

  /** One-line, length-capped preview of a SQL snippet for menu items. */
  function clipSql(/** @type {string} */ s, max = 64) {
    const one = s.replace(/\s+/g, ' ').trim()
    return one.length > max ? one.slice(0, max - 1) + '…' : one
  }

  // ── Result view state ───────────────────────────────────────────────────────
  /** @type {'table' | 'chart' | 'json' | 'explain' | 'error'} */
  let outputView = $state('table')
  let chartType = $state('bar')
  let activeResultIdx = $state(0)
  /** @type {object | null} */
  let explainResult = $state(null)
  let explainLoading = $state(false)
  let explainError = $state('')
  let selected = $state(new Set())

  /** @typedef {{ columns: any[], rows: any[][], message: string, queryMs: number }} ResultSet */

  /** One entry per statement when the last run executed multiple statements. */
  const resultSets = $derived(
    (multiResults?.length ?? 0) > 1
      ? multiResults.map((res) => /** @type {ResultSet} */ ({
          columns: res.columns ?? [],
          rows: res.rows ?? [],
          message: res.message ?? '',
          queryMs: res.query_ms ?? res.queryMs ?? 0,
        }))
      : []
  )

  // A new run replaces the previous results: reset selection and result-set
  // index, and leave the explain view (it shows the previous query's plan).
  $effect(() => {
    if (!loading) return
    untrack(() => {
      selected = new Set()
      activeResultIdx = 0
      resultSort = null
      if (outputView === 'explain') outputView = 'table'
    })
  })

  // When a run finishes without an error, mark the executed statement(s) with
  // a ✓ in the editor gutter (⌘R marks one, Run marks all).
  let wasLoading = false
  $effect(() => {
    const l = loading
    const err = error
    untrack(() => {
      if (wasLoading && !l && !err) sqlEditorRef?.markExecuted?.(lastRanStatement)
      wasLoading = l
    })
  })

  // Route a failed run into its own "Error" results tab - like a real editor's
  // problems pane - and open the panel so it's visible. When the next run clears
  // the error, fall back to the table (the now-empty Error tab drops out too).
  let hadError = false
  $effect(() => {
    const err = error
    untrack(() => {
      if (err) {
        outputView = 'error'
        if (!outputVisible) outputVisible = true
      } else if (hadError && outputView === 'error') {
        outputView = 'table'
      }
      hadError = !!err
    })
  })

  /**
   * Header-click sort for result rows. Ad-hoc results have no table to re-query,
   * so sorting is done client-side over the returned rows.
   * @type {{ column: string, direction: 'asc' | 'desc' } | null}
   */
  let resultSort = $state(null)

  // Sort text for object cells is computed once per value (row references are
  // stable until a re-run) - stringifying inside the comparator would redo the
  // work n·log n times and stall on large JSON cells.
  /** @type {WeakMap<object, string>} */
  const _sortTextCache = new WeakMap()

  // One collator for the whole sort - localeCompare with an options object
  // resolves locale/options once per comparison, the slow path on big results.
  const cellCollator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' })
  /** @param {unknown} v */
  function cellSortText(v) {
    if (v === null || typeof v !== 'object') return String(v)
    const hit = _sortTextCache.get(v)
    if (hit !== undefined) return hit
    let s
    try { s = JSON.stringify(v) } catch { s = String(v) }
    _sortTextCache.set(v, s)
    return s
  }

  /** Compare two cell values: numbers numerically, everything else as text. */
  function compareCellValues(a, b) {
    if (typeof a === 'number' && typeof b === 'number') return a - b
    if (typeof a === 'boolean' && typeof b === 'boolean') return a === b ? 0 : a ? 1 : -1
    const sa = cellSortText(a)
    const sb = cellSortText(b)
    if (sa.trim() !== '' && sb.trim() !== '') {
      const na = Number(sa)
      const nb = Number(sb)
      if (!Number.isNaN(na) && !Number.isNaN(nb)) return na - nb
    }
    return cellCollator.compare(sa, sb)
  }

  const currentDisplay = $derived.by(() => {
    const base = resultSets.length > 1
      ? (() => {
          const idx = Math.min(activeResultIdx, resultSets.length - 1)
          const s = resultSets[idx]
          return { columns: s.columns, rows: s.rows, queryMs: s.queryMs, message: s.message }
        })()
      : { columns, rows, queryMs, message }
    if (!resultSort) return base
    const colIdx = base.columns.findIndex((c) => (c.name ?? c) === resultSort.column)
    if (colIdx < 0) return base
    const dir = resultSort.direction === 'desc' ? -1 : 1
    const sorted = [...base.rows].sort((ra, rb) => {
      const a = ra[colIdx]
      const b = rb[colIdx]
      const aNull = a === null || a === undefined
      const bNull = b === null || b === undefined
      // NULLs sort last in either direction.
      if (aNull || bNull) return aNull && bNull ? 0 : aNull ? 1 : -1
      return dir * compareCellValues(a, b)
    })
    return { ...base, rows: sorted }
  })

  /** @param {{ column: string, direction: 'asc' | 'desc' } | null} sort */
  function handleResultSort(sorts) {
    // DataTable now emits the full ordered key list; SQL result sorting is
    // single-column (client-side), so take the primary key.
    resultSort = Array.isArray(sorts) ? (sorts[0] ?? null) : sorts
    // Row indices change with the order - selection would point at the wrong rows.
    selected = new Set()
  }

  async function handleExplain() {
    const querySql = sql.trim()
    if (!querySql) return
    outputView = 'explain'
    explainLoading = true
    explainError = ''
    explainResult = null
    if (!outputVisible) outputVisible = true
    try {
      explainResult = await explainSql(querySql)
    } catch (e) {
      explainError = String(e)
    } finally {
      explainLoading = false
    }
  }

  let outputVisible = $state(
    (() => { try { return localStorage.getItem('sql-output-visible') !== 'false' } catch { return true } })()
  )

  function toggleOutput() {
    outputVisible = !outputVisible
    try { localStorage.setItem('sql-output-visible', String(outputVisible)) } catch {}
  }

  $effect(() => {
    /** @param {KeyboardEvent} e */
    function onKey(e) {
      // SqlConsole stays mounted (keep-alive) on other tabs; only handle these
      // window-level shortcuts when the SQL tab is actually visible.
      if (!active) return
      const mod = e.metaKey || e.ctrlKey
      if (!mod || e.altKey) return
      if (e.key === 'j' && !e.shiftKey) {
        e.preventDefault()
        toggleOutput()
      } else if ((e.key === 'b' || e.key === 'B') && e.shiftKey) {
        e.preventDefault()
        queryHistoryVisible = !queryHistoryVisible
        onmodshiftb?.()
      } else if (e.key === 's' && !e.shiftKey) {
        e.preventDefault()
        openSaveDialog()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })

  // The JSON view materialises a real object per row AND a formatted string of
  // all of them - both O(rows × columns) - while the table beside it is canvas-
  // virtualised and pays for neither. Unbounded, a `SELECT *` over a million
  // rows spent its time building a string far too large to read, and often ran
  // the webview out of memory before it could show any of it.
  //
  // Bounded here. The whole result still leaves through Export, which streams
  // from Rust instead of assembling it in the webview.
  const JSON_VIEW_ROWS = 1000

  const jsonRows = $derived(
    currentDisplay.rows.length > JSON_VIEW_ROWS
      ? currentDisplay.rows.slice(0, JSON_VIEW_ROWS)
      : currentDisplay.rows,
  )
  const rowObjects = $derived(
    currentDisplay.columns.length > 0 && jsonRows.length > 0
      ? rowsToObjects(currentDisplay.columns, jsonRows)
      : []
  )

  const jsonText = $derived(rowObjects.length > 0 ? JSON.stringify(rowObjects, null, 2) : '[]')

  // ── Export helpers ──────────────────────────────────────────────────────────
  /** Unified export via the shared generators + native Save dialog.
   * @param {'csv'|'json'|'sql'|'tsv'|'md'|'jsonl'} format */
  async function exportAs(format) {
    const columns = currentDisplay.columns.map((c) => ({ name: c.name ?? String(c) }))
    const rows = /** @type {any[][]} */ (currentDisplay.rows)
    const content =
      format === 'csv' ? rowsToCsv(columns, rows)
        : format === 'json' ? rowsToJson(columns, rows)
        : format === 'sql' ? rowsToSql(columns, rows, 'query_result')
        : format === 'tsv' ? rowsToTsv(columns, rows)
        : format === 'md' ? rowsToMarkdown(columns, rows)
        : rowsToJsonl(columns, rows)
    await saveExportFile(content, buildExportFilename('query_result', format), format)
  }

  /** @type {HTMLElement | null} */
  let consoleEl = $state(null);
  const initialLayout = loadLayout();
  let editorHeight = $state(initialLayout.sqlEditorHeight);
  let resizeStartHeight = initialLayout.sqlEditorHeight;
  /** @type {(() => Promise<void>) | null} */
  let formatSql = $state(null);

  let saveDialogOpen = $state(false);
  let saveQueryName = $state('');
  let savingQuery = $state(false);

  function fixWithAi() {
    if (!error || !sql.trim()) return
    onfixwithai?.({ error, sql: sql.trim() })
  }

  // Stop is fire-and-forget on the backend; `stopping` only keeps a second
  // click from queuing another cancel while the first one lands.
  let stopping = $state(false)
  $effect(() => { if (!loading) stopping = false })
  function stopRun() {
    if (!loading || stopping) return
    stopping = true
    void cancelQuery(runningQueryId ?? undefined)
  }
  // The backend reports a stopped run as an error string. It is the user's own
  // action, not a failure, so it gets its own quiet view instead of the red one.
  const stopped = $derived(/Query cancelled/i.test(error))
  // Tauri errors arrive as `Error: <message>`; the prefix repeats the header.
  const errorText = $derived(error.replace(/^Error:\s*/, ''))

  let errorCopied = $state(false)
  /** @type {ReturnType<typeof setTimeout> | null} */
  let errorCopyTimer = null
  async function copyError() {
    if (!error) return
    try {
      await navigator.clipboard.writeText(error)
      errorCopied = true
      if (errorCopyTimer) clearTimeout(errorCopyTimer)
      errorCopyTimer = setTimeout(() => { errorCopied = false }, 1600)
    } catch { /* clipboard unavailable - no-op */ }
  }

  const isMac =
    typeof navigator !== "undefined" &&
    navigator.platform.toUpperCase().includes("MAC");
  const mod = isMac ? "⌘" : "Ctrl";

  /** Plain-text tooltip string for GlobalTooltip: "Label (⌘↵)" with an optional description on a second line. */
  function tipText(/** @type {string} */ label, /** @type {string} */ desc = '', /** @type {string[]} */ keys = []) {
    const head = keys.length ? `${label} (${keys.join('')})` : label;
    return desc ? `${head}\n${desc}` : head;
  }

  /** @param {number} height */
  function clampEditorHeight(height) {
    return clampSqlEditorHeight(height, consoleEl?.clientHeight ?? 0);
  }

  function openSaveDialog() {
    saveQueryName = queryTitle(sql);
    saveDialogOpen = true;
  }

  let ormCopied = $state(/** @type {'drizzle' | 'prisma' | null} */ (null))
  /** @type {ReturnType<typeof setTimeout> | null} */
  let ormCopiedTimer = null

  /** @param {'drizzle' | 'prisma'} kind */
  function copyAsOrm(kind) {
    const code = kind === 'drizzle' ? sqlToDrizzle(sql) : sqlToPrisma(sql, [])
    navigator.clipboard.writeText(code).then(() => {
      ormCopied = kind
      if (ormCopiedTimer) clearTimeout(ormCopiedTimer)
      ormCopiedTimer = setTimeout(() => { ormCopied = null }, 2000)
    })
  }

  async function confirmSaveQuery() {
    if (!sql.trim() || savingQuery) return;
    savingQuery = true;
    try {
      await onsavequery(saveQueryName, sql);
      saveDialogOpen = false;
    } finally {
      savingQuery = false;
    }
  }

  onDestroy(() => {
    // Clear copy-feedback timers so they don't fire state writes after unmount
    if (ormCopiedTimer) clearTimeout(ormCopiedTimer)
    if (errorCopyTimer) clearTimeout(errorCopyTimer)
  })
</script>

<div class="flex min-h-0 flex-1 overflow-hidden">
  <QueryHistoryPanel
    bind:visible={queryHistoryVisible}
    history={queryHistory}
    saved={savedQueries}
    onselect={(text) => onhistoryselect(text)}
    onrefresh={onqueryrefresh}
    onclose={() => (queryHistoryVisible = false)}
  />

  <div bind:this={consoleEl} class="flex min-h-0 min-w-0 flex-1 flex-col">
  <div
    class="studio-chrome flex h-9 shrink-0 items-center gap-1.5 border-b border-border bg-panel px-2"
    data-studio-chrome
  >
    {#if loading}
      <!-- Same footprint as Run + its menu (h-7, ~76px), so the toolbar doesn't
           shift when a run starts. The spinner says "running", the square
           inside it says what a click does. -->
      <button
        type="button"
        class="group inline-flex h-7 min-w-[4.75rem] shrink-0 select-none items-center gap-2 rounded-md border border-border bg-muted/40 pl-2 pr-2.5 text-ui-2xs font-medium text-foreground transition-[background-color,border-color,scale] duration-150 hover:border-destructive/40 hover:bg-destructive/10 active:scale-[0.96] disabled:pointer-events-none disabled:opacity-60"
        disabled={stopping}
        onclick={stopRun}
        title={tipText('Stop', 'Cancel the running query.')}
      >
        <span class="relative grid size-3.5 shrink-0 place-items-center">
          <Loader2 class="size-3.5 animate-spin text-muted-foreground" />
          <Square class="absolute size-1.5 fill-current text-destructive" />
        </span>
        {stopping ? 'Stopping' : 'Stop'}
      </button>
    {:else}
      {#if txStatus?.open}
        <!-- An open transaction changes what Run means, so it is said next to
             Run, and its two exits sit beside it.
             -
             The state and the actions are separate controls now. They were one
             amber pill with a neutral Commit and a red Roll back inside it:
             three semantic colours in a 28px box, with the destructive red
             measuring 4.80:1 against the amber wash it sat on - technically
             legible, and still two alarm hues arguing inside one chip. The chip
             now carries one hue and says one thing; the actions are ordinary
             `h-7` toolbar buttons with real hit areas, and red appears on the
             one control that destroys work, on hover, where it means something.
             (Measured on --panel in the dark theme: warning text on the wash
             8.69:1, the count 6.68:1.) -->
        <div class="flex h-7 shrink-0 items-center gap-1.5 rounded-md border border-warning/25 bg-warning/10 px-2">
          <span class="size-1.5 shrink-0 rounded-full bg-warning" aria-hidden="true"></span>
          <span class="whitespace-nowrap text-ui-2xs font-medium text-warning">In transaction</span>
          <span class="whitespace-nowrap font-mono text-ui-2xs tabular-nums text-muted-foreground">
            {txStatus.statements}<span class="ml-0.5">{txStatus.statements === 1 ? 'stmt' : 'stmts'}</span>
          </span>
        </div>
        <Button
          variant="outline"
          size="sm"
          class="h-7 shrink-0"
          disabled={txBusy}
          onclick={() => oncommittransaction()}
          title={tipText('Commit', `Save everything this transaction has done: ${txStatus.statements} statement(s), ${txStatus.rowsAffected} row(s) changed.`)}
        >
          Commit
        </Button>
        <Button
          variant="ghost"
          size="sm"
          class="h-7 shrink-0 text-muted-foreground hover:bg-destructive/10 hover:text-destructive focus-visible:bg-destructive/10 focus-visible:text-destructive"
          disabled={txBusy}
          onclick={() => onrollbacktransaction()}
          title={tipText('Roll back', 'Undo everything this transaction has done.')}
        >
          Roll back
        </Button>
      {:else}
        <Button
          variant="outline"
          size="sm"
          class="h-7 shrink-0"
          disabled={txBusy}
          onclick={() => onbegintransaction()}
          title={tipText(
            'Begin transaction',
            'Run statements without saving them, then commit or roll back.',
          )}
        >
          Begin
        </Button>
      {/if}
      <!-- Split button: one wrapper owns the radius + shadow; the halves are
           plain buttons (the Button component's transparent border,
           bg-clip-padding and elevate shadow would each paint a seam). -->
      <div class="flex shrink-0 items-stretch overflow-hidden rounded-md elevate-1">
        <button
          type="button"
          class="inline-flex h-7 shrink-0 select-none items-center gap-2 bg-primary pl-2.5 pr-2 text-ui-2xs font-medium text-primary-foreground transition-[background-color,opacity] hover:bg-primary/90 disabled:pointer-events-none disabled:opacity-50"
          disabled={!sql.trim()}
          onclick={() => handleRun(undefined)}
          title={tipText(
            'Run query',
            `Runs every statement, each gets its own result tab. ${mod}R runs only the statement under the cursor, ${mod}L selects it.`,
            [mod, '↵'],
          )}
        >
          <Play class="size-3.5 shrink-0" />
          Run
        </button>
        <DropdownMenu.Root bind:open={runMenuOpen} onOpenChange={captureRunPreviews}>
          <DropdownMenu.Trigger
            class="inline-flex h-7 w-6 shrink-0 items-center justify-center border-l border-primary-foreground/20 bg-primary text-primary-foreground transition-opacity hover:opacity-90 disabled:pointer-events-none disabled:opacity-50"
            disabled={!sql.trim()}
            aria-label="Run options"
          >
            <ChevronDown class="size-3" />
          </DropdownMenu.Trigger>
          <DropdownMenu.Content align="start" class="min-w-72 text-ui-sm">
            <DropdownMenu.Item onSelect={() => handleRun(undefined)}>
              <Play class="size-3.5 shrink-0 text-muted-foreground" />
              <span class="whitespace-nowrap">Run all statements</span>
              <DropdownMenu.Shortcut combo="Mod+Enter" />
            </DropdownMenu.Item>
            <DropdownMenu.Item
              class="items-start"
              disabled={!cursorStmtPreview}
              onSelect={() => handleRun(cursorStmtPreview)}
            >
              <TextCursorInput class="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
              <div class="flex w-full min-w-0 flex-col gap-0.5">
                <span class="flex w-full items-center whitespace-nowrap">
                  Run statement at cursor
                  <DropdownMenu.Shortcut combo="Mod+R" />
                </span>
                {#if cursorStmtPreview}
                  <span class="truncate font-mono text-ui-2xs leading-4 text-muted-foreground">{clipSql(cursorStmtPreview)}</span>
                {/if}
              </div>
            </DropdownMenu.Item>
            {#if selectionPreview}
              <DropdownMenu.Item class="items-start" onSelect={() => handleRun(selectionPreview)}>
                <TextSelect class="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
                <div class="flex w-full min-w-0 flex-col gap-0.5">
                  <span class="whitespace-nowrap">Run selection</span>
                  <span class="truncate font-mono text-ui-2xs leading-4 text-muted-foreground">{clipSql(selectionPreview)}</span>
                </div>
              </DropdownMenu.Item>
            {/if}
          </DropdownMenu.Content>
        </DropdownMenu.Root>
      </div>
    {/if}

    {#if sqlParams.length > 0}
      <Button
        type="button"
        variant="ghost"
        size="sm"
        class={cn(
          'h-7 shrink-0 gap-1.5 px-2 font-normal',
          paramsPanelOpen ? 'bg-accent text-foreground' : 'text-muted-foreground hover:text-foreground',
        )}
        onclick={() => (paramsPanelOpen = !paramsPanelOpen)}
        title={tipText('Query parameters', 'Set values for :name placeholders, they are inlined as escaped literals when the query runs.')}
      >
        <Variable class="size-3.5 shrink-0" />
        Parameters
        <span class="rounded bg-muted/70 px-1 font-mono text-ui-3xs tabular-nums text-muted-foreground">{sqlParams.length}</span>
      </Button>
    {/if}

    <div class="mx-0.5 h-4 w-px shrink-0 bg-border" aria-hidden="true"></div>

    <div class="flex min-w-0 items-center gap-0.5">
      <Button
        type="button"
        variant="ghost"
        size="sm"
        class="size-7 p-0 text-muted-foreground hover:text-foreground"
        disabled={!sql.trim()}
        onclick={() => void formatSql?.()}
        title={tipText('Format SQL', 'Reformat the whole editor with consistent casing and indentation.')}
      >
        <Braces class="size-3.5 shrink-0" />
      </Button>

      <Button
        type="button"
        variant="ghost"
        size="sm"
        class="size-7 p-0 text-muted-foreground hover:text-foreground"
        disabled={!sql.trim()}
        onclick={openSaveDialog}
        title={tipText('Save query', 'Keep this query for later, saved queries live in History → Saved, per connection.', [mod, 'S'])}
      >
        <Bookmark class="size-3.5 shrink-0" />
      </Button>

      <Button
        type="button"
        variant="ghost"
        size="sm"
        class={cn(
          'size-7 p-0 hover:text-foreground',
          queryHistoryVisible ? 'text-foreground' : 'text-muted-foreground',
        )}
        onclick={() => (queryHistoryVisible = !queryHistoryVisible)}
        title={tipText('Query history', 'Browse and re-run everything you have executed, plus your saved queries.', [mod, '⇧', 'B'])}
      >
        <History class="size-3.5 shrink-0" />
      </Button>

      <Button
        type="button"
        variant="ghost"
        size="sm"
        class={cn(
          'size-7 p-0 hover:text-foreground',
          outputView === 'explain' ? 'text-foreground' : 'text-muted-foreground',
        )}
        disabled={!sql.trim() || explainLoading}
        onclick={handleExplain}
        title={tipText('Explain plan', 'Visualize how the database executes this query, spot slow scans and missing indexes.')}
      >
        {#if explainLoading}
          <Loader2 class="size-3.5 shrink-0 animate-spin" />
        {:else}
          <ScanSearch class="size-3.5 shrink-0" />
        {/if}
      </Button>

      <DropdownMenu.Root>
        <DropdownMenu.Trigger
          class="flex h-7 items-center gap-1 rounded-md px-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-50"
          disabled={!sql.trim()}
          title="Copy as ORM query, Drizzle or Prisma"
        >
          {#if ormCopied}
            <CheckCheck class="size-3.5 shrink-0 text-success" />
          {:else}
            <Code2 class="size-3.5 shrink-0" />
          {/if}
          <ChevronDown class="size-3 shrink-0 opacity-50" />
        </DropdownMenu.Trigger>
        <DropdownMenu.Content align="start" class="min-w-44">
          <DropdownMenu.Item class="gap-2 whitespace-nowrap font-mono text-ui-xs" onclick={() => copyAsOrm('drizzle')}>
            <Code2 class="size-3.5 shrink-0 text-muted-foreground" />
            Copy as Drizzle
          </DropdownMenu.Item>
          <DropdownMenu.Item class="gap-2 whitespace-nowrap font-mono text-ui-xs" onclick={() => copyAsOrm('prisma')}>
            <Code2 class="size-3.5 shrink-0 text-muted-foreground" />
            Copy as Prisma
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Root>
    </div>
  </div>

  {#if paramsPanelOpen && sqlParams.length > 0}
    <div class="shrink-0 border-b border-border/60 bg-panel px-3 py-2">
      <div class="flex w-full max-w-2xl items-center gap-1.5 pb-1.5">
        <Variable class="size-3 text-muted-foreground" />
        <span class="select-none text-ui-3xs font-medium uppercase tracking-[0.08em] text-muted-foreground">Parameters</span>
        <span
          class="select-none text-ui-3xs text-muted-foreground"
          title="Auto detects numbers, booleans and NULL, everything else runs as a quoted string."
        >· Enter runs</span>
        <button
          type="button"
          class="ml-auto inline-flex size-5 items-center justify-center rounded text-muted-foreground transition-[background-color,color] hover:bg-accent hover:text-foreground"
          aria-label="Close parameters"
          onclick={() => (paramsPanelOpen = false)}
        >
          <X class="size-3" />
        </button>
      </div>
      <div class="flex w-full max-w-2xl flex-col gap-1">
        {#each sqlParams as p (p.name)}
          {@const v = paramValues[p.name] ?? { value: '', mode: 'auto' }}
          <div class="grid grid-cols-[minmax(5rem,8.5rem)_5.25rem_minmax(0,1fr)] items-center gap-1.5">
            <span
              class="justify-self-start truncate rounded bg-muted/50 px-1.5 py-0.5 font-mono text-ui-2xs text-foreground/75"
              title=":{p.name}"
            ><span class="text-muted-foreground">:</span>{p.name}</span>
            <div class="relative">
              <FieldSelect
                size="sm"
                class="w-full bg-input/30 text-ui-xs"
                aria-label="Parameter type for {p.name}"
                value={v.mode}
                onchange={(mode) => setParam(p.name, { ...v, mode: /** @type {any} */ (mode) })}
                options={[
                  { value: 'auto', label: 'Auto' },
                  { value: 'text', label: 'Text' },
                  { value: 'raw', label: 'Raw SQL' },
                  { value: 'null', label: 'NULL' },
                ]}
              />
            </div>
            <input
              type="text"
              value={v.value}
              disabled={v.mode === 'null'}
              placeholder={v.mode === 'null' ? 'NULL' : v.mode === 'raw' ? 'now(), inserted verbatim' : 'value'}
              aria-label="Value for {p.name}"
              class= "field-surface h-7 w-full min-w-0 border-transparent bg-input/30 px-2 font-mono text-ui-xs text-foreground transition-colors placeholder:text-muted-foreground hover: focus:outline-none disabled:opacity-40"
              oninput={(e) => setParam(p.name, { ...v, value: e.currentTarget.value })}
              onkeydown={(e) => { if (e.key === 'Enter') handleRun(undefined) }}
            />
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <div
    class={outputVisible
      ? "relative shrink-0 overflow-hidden bg-panel"
      : "relative min-h-0 flex-1 overflow-hidden bg-panel"}
    style={outputVisible ? `height: ${editorHeight}px` : undefined}
  >
    <SqlEditor
      bind:this={sqlEditorRef}
      bind:value={sql}
      class="absolute inset-0"
      {schemaHints}
      {onmodk}
      onmodenter={() => handleRun(undefined)}
      onrunstatement={(stmt) => handleRun(stmt)}
      onmods={openSaveDialog}
      {onmodi}
      {onmodw}
      {onmodn}
      {onmodm}
      {onmodt}
      {onmodshifte}
      {onmodshiftd}
      {onmodaltd}
      {onmodshifto}
      onmodj={toggleOutput}
      onmodshiftb={() => { queryHistoryVisible = !queryHistoryVisible; onmodshiftb?.() }}
      onactionsready={(actions) => {
        formatSql = actions.format;
      }}
    />
  </div>

  {#if outputVisible}
  <ResizeHandle
    axis="y"
    edge="end"
    onresizestart={() => {
      resizeStartHeight = editorHeight;
    }}
    onresize={(dy) => {
      editorHeight = clampEditorHeight(resizeStartHeight + dy);
    }}
    onresizeend={() => {
      resizeStartHeight = editorHeight;
      saveLayout({ sqlEditorHeight: editorHeight });
    }}
  />
  {/if}

  <!-- Output panel: header always visible, content toggles with Cmd+J
       (a failed run surfaces in the "Error" view tab below, not a banner). -->
  <div class={outputVisible ? "flex min-h-0 flex-1 flex-col overflow-hidden" : "flex shrink-0 flex-col"}>
    <!-- Output tab bar -->
    <div
      class="studio-chrome flex h-8 shrink-0 items-stretch border-b border-border bg-panel"
      data-studio-chrome
    >
      <!-- Result-set selector tabs (only when multi-result) -->
      {#if resultSets.length > 1}
        {#each resultSets as _rs, i (i)}
          {@const rsActive = Math.min(activeResultIdx, resultSets.length - 1) === i}
          <button
            type="button"
            onclick={() => { activeResultIdx = i; resultSort = null; selected = new Set() }}
            class={cn(
              'relative flex items-center border-b-2 px-2.5 font-mono text-ui-xs transition-colors',
              rsActive ? 'border-primary text-foreground' : 'border-transparent text-muted-foreground hover:text-muted-foreground',
            )}
          >
            Result {i + 1}
          </button>
        {/each}
        <div class="mx-1.5 h-3.5 w-px shrink-0 self-center bg-border/60"></div>
      {/if}

      <!-- View tabs (icon-only). The Error tab appears with a label only after a
           failed run, tinted destructive so it reads at a glance. -->
      <div class="flex items-center gap-0.5 px-1">
        {#each [
          { id: 'table',   label: 'Table',   Icon: Table2,     pro: false },
          { id: 'chart',   label: 'Chart',   Icon: BarChart2,  pro: true },
          { id: 'json',    label: 'JSON',    Icon: Braces,     pro: true },
          { id: 'explain', label: 'Explain', Icon: ScanSearch, pro: true },
          ...(error ? [{ id: 'error', label: stopped ? 'Stopped' : 'Error', Icon: stopped ? Square : CircleAlert, pro: false }] : []),
        ] as tab (tab.id)}
          {@const locked = tab.pro && !$hasPro}
          {@const tabActive = !locked && outputVisible && outputView === tab.id}
          {@const isError = tab.id === 'error' && !stopped}
          {@const Icon = tab.Icon}
          <button
            type="button"
            onclick={() => {
              if (locked) { onprorequired(); return }
              if (tab.id === 'explain') { void handleExplain() }
              else { outputView = /** @type {'table'|'chart'|'json'|'error'} */ (tab.id); if (!outputVisible) toggleOutput() }
            }}
            class={cn(
              'flex size-7 items-center justify-center rounded transition-colors',
              locked
                ? 'cursor-not-allowed opacity-40 text-muted-foreground'
                : isError
                  ? tabActive ? 'bg-muted/70 text-destructive' : 'text-destructive hover:bg-muted/40 hover:text-destructive'
                  : tabActive ? 'bg-muted/70 text-foreground' : 'text-muted-foreground hover:bg-muted/40 hover:text-muted-foreground',
            )}
            title="{tab.label} view{locked ? ' · Stroke Pro' : ''}"
          >
            <Icon class="size-3.5 shrink-0" />
          </button>
        {/each}
      </div>

      <!-- Right: metadata + toggle -->
      <div class="ml-auto flex shrink-0 items-center gap-3 pr-1.5">
        {#if outputVisible && currentDisplay.rows.length > 0}
          <span class="font-mono text-ui-2xs tabular-nums text-muted-foreground">{formatCompactCount(currentDisplay.rows.length)} rows</span>
        {/if}
        {#if outputVisible && currentDisplay.queryMs > 0}
          <span class="font-mono text-ui-2xs tabular-nums text-muted-foreground">{currentDisplay.queryMs}ms</span>
        {/if}
        {#if outputVisible && currentDisplay.message}
          <span class="max-w-[160px] truncate font-mono text-ui-2xs text-muted-foreground">{currentDisplay.message}</span>
        {/if}

        <!-- Export dropdown, only when there are results -->
        {#if outputVisible && currentDisplay.rows.length > 0}
          <DropdownMenu.Root>
            <DropdownMenu.Trigger
              class="flex size-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground"
              title="Export results"
            >
              <Download class="size-3.5" />
            </DropdownMenu.Trigger>
            <DropdownMenu.Content align="end" class="min-w-36">
              <DropdownMenu.Item class="gap-2 font-mono text-ui-xs" onclick={() => exportAs('csv')}>
                <Download class="size-3.5 shrink-0 text-muted-foreground" />CSV
              </DropdownMenu.Item>
              <DropdownMenu.Item class="gap-2 font-mono text-ui-xs" onclick={() => exportAs('json')}>
                <Download class="size-3.5 shrink-0 text-muted-foreground" />JSON
              </DropdownMenu.Item>
              <DropdownMenu.Item class="gap-2 font-mono text-ui-xs" onclick={() => exportAs('sql')}>
                <Download class="size-3.5 shrink-0 text-muted-foreground" />SQL (INSERT)
              </DropdownMenu.Item>
              <DropdownMenu.Item class="gap-2 font-mono text-ui-xs" onclick={() => exportAs('tsv')}>
                <Download class="size-3.5 shrink-0 text-muted-foreground" />TSV
              </DropdownMenu.Item>
              <DropdownMenu.Item class="gap-2 font-mono text-ui-xs" onclick={() => exportAs('md')}>
                <Download class="size-3.5 shrink-0 text-muted-foreground" />Markdown
              </DropdownMenu.Item>
              <DropdownMenu.Item class="gap-2 font-mono text-ui-xs" onclick={() => exportAs('jsonl')}>
                <Download class="size-3.5 shrink-0 text-muted-foreground" />JSON Lines
              </DropdownMenu.Item>
            </DropdownMenu.Content>
          </DropdownMenu.Root>
        {/if}

        <button
          type="button"
          onclick={toggleOutput}
          class="inline-flex size-5 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground"
          title={tipText(outputVisible ? 'Hide results' : 'Show results', 'Collapse the results panel to give the editor the full height.', [mod, 'J'])}
        >
          <ChevronDown class={cn('size-3.5 transition-transform duration-150', outputVisible ? '' : 'rotate-180')} />
        </button>
      </div>
    </div>

    {#if outputVisible}
      <div class="relative flex min-h-0 flex-1 flex-col overflow-hidden bg-panel">
        {#key `${outputView}:${Math.min(activeResultIdx, Math.max(resultSets.length - 1, 0))}`}
          {#if outputView === 'error'}
            {#if stopped}
              <div class="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
                <span class="grid size-8 place-items-center rounded-full bg-muted/60">
                  <Square class="size-3 fill-current text-muted-foreground" />
                </span>
                <div class="flex flex-col gap-1">
                  <p class="text-ui-sm font-medium text-foreground">Query stopped</p>
                  <p class="text-ui-xs text-muted-foreground">It was cancelled before the results came back.</p>
                </div>
                <Button type="button" variant="outline" size="sm" disabled={!sql.trim()} onclick={() => handleRun(undefined)}>
                  <Play class="size-3.5 shrink-0" data-icon="inline-start" />
                  Run again
                </Button>
              </div>
            {:else if isNetworkError(error)}
              <div class="flex h-full flex-col items-center justify-center gap-2.5 px-6 text-center">
                <WifiOff class="size-6 text-muted-foreground" />
                <p class="font-mono text-ui-sm text-muted-foreground">Cannot reach database, check your connection and try again.</p>
              </div>
            {:else}
              <!-- SQL error as a console pane (VS Code / Postman feel): neutral
                   monospace output on the panel surface, colour reserved for a
                   small severity marker and a thin gutter rail, never a red wash.
                   Body is fully SELECTABLE (the app is select-none by default;
                   data-studio-selectable re-enables selection here). -->
              <div data-studio-selectable="text" class="flex h-full min-h-0 flex-col font-mono">
                <!-- Console toolbar, neutral chrome, ghost actions -->
                <div class="flex h-10 shrink-0 items-center gap-2 border-b border-border/60 pl-3 pr-1.5 font-sans select-none">
                  <CircleAlert class="size-3.5 shrink-0 text-destructive" />
                  <span class="text-ui-xs font-medium text-foreground">Query failed</span>
                  {#if currentDisplay.queryMs > 0}
                    <span class="text-ui-2xs tabular-nums text-muted-foreground">after {currentDisplay.queryMs}ms</span>
                  {/if}
                  <div class="ml-auto flex shrink-0 items-center gap-1">
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon-sm"
                      onclick={copyError}
                      title="Copy error"
                      aria-label="Copy error"
                      class="text-muted-foreground hover:text-foreground"
                    >
                      {#if errorCopied}<Check class="size-3.5 shrink-0" />{:else}<Copy class="size-3.5 shrink-0" />{/if}
                    </Button>
                    {#if onfixwithai}
                      <Button type="button" variant="outline" size="sm" onclick={fixWithAi}>
                        <Wand2 class="size-3.5 shrink-0" data-icon="inline-start" />
                        Fix with AI
                      </Button>
                    {/if}
                  </div>
                </div>
                <!-- Console output, neutral text, red only in the thin left rail.
                     overflow-wrap:anywhere wraps at word boundaries first and only
                     breaks inside a token when it can't fit (unlike break-all,
                     which chopped ordinary words mid-character). -->
                <div class="min-h-0 flex-1 overflow-auto px-3 py-3">
                  <div class="border-l-2 border-destructive/40 pl-3">
                    <pre class="select-text whitespace-pre-wrap [overflow-wrap:anywhere] text-ui-xs leading-relaxed text-foreground/85">{errorText}</pre>
                    {#if /statement timeout|canceling statement due to/i.test(error)}
                      <p class="mt-3 text-ui-2xs leading-relaxed text-muted-foreground">
                        The query timed out. If this table has large JSON/text columns, select just the
                        columns you need instead of <code class="rounded bg-muted/60 px-1 py-px text-foreground/70">*</code>, or add a smaller
                        <code class="rounded bg-muted/60 px-1 py-px text-foreground/70">LIMIT</code>.
                      </p>
                    {:else if /relation "[^"]*" does not exist|column "[^"]*" does not exist/i.test(error)}
                      <p class="mt-3 text-ui-2xs leading-relaxed text-muted-foreground">
                        PostgreSQL folds unquoted names to lowercase, so a table like
                        <code class="rounded bg-muted/60 px-1 py-px text-foreground/70">Products</code> only matches when quoted -
                        <code class="rounded bg-muted/60 px-1 py-px text-foreground/70">SELECT * FROM "Products"</code>. Pick the table from
                        autocomplete and it inserts the quoted form for you.
                      </p>
                    {/if}
                  </div>
                </div>
              </div>
            {/if}
          {:else if outputView === 'explain'}
            {#if explainLoading}
              <TableLoading />
            {:else if explainError}
              <div class="flex h-full flex-col items-center justify-center gap-2 px-6 text-center">
                <p class="font-mono text-ui-xs text-destructive">{explainError}</p>
              </div>
            {:else if explainResult}
              <ExplainPlan result={explainResult} />
            {:else}
              <div class="flex h-full flex-col items-center justify-center gap-2 text-center">
                <ScanSearch class="size-6 text-muted-foreground" />
                <p class="font-mono text-ui-sm text-muted-foreground">Click Explain to analyze the query plan</p>
              </div>
            {/if}
          {:else if currentDisplay.columns.length > 0}
            {#if outputView === 'json'}
              <JsonViewer json={jsonText} data={rowObjects} shownRows={rowObjects.length} rowCount={currentDisplay.rows.length} onshowtable={() => (outputView = 'table')} />
            {:else if outputView === 'chart'}
              <ChartView
                columns={currentDisplay.columns}
                rows={currentDisplay.rows}
                {sql}
                initialChartType={chartType}
                oncharttypechange={(t) => (chartType = t)}
              />
            {:else}
              <DataTable
                columns={currentDisplay.columns}
                rows={currentDisplay.rows}
                {loading}
                bind:selected
                rowSort={resultSort}
                onsortchange={handleResultSort}
                readonly
              />
            {/if}
          {:else if loading}
            <TableLoading />
          {:else}
            <div class="flex h-full flex-col items-center justify-center gap-2 text-center">
              <Play class="size-6 text-muted-foreground" />
              <p class="font-mono text-ui-sm text-muted-foreground">Run a query to see results</p>
            </div>
          {/if}
        {/key}
      </div>
    {/if}
  </div>
  </div>
</div>

<Dialog.Root bind:open={saveDialogOpen}>
  <Dialog.Content class="max-w-md gap-4">
    <Dialog.Header>
      <Dialog.Title class="text-ui-sm font-semibold">Save query</Dialog.Title>
      <Dialog.Description class="text-ui-xs text-muted-foreground">
        Saved queries are stored per connection and appear in History → Saved.
      </Dialog.Description>
    </Dialog.Header>
    <div class="flex flex-col gap-2">
      <Label for="save-query-name" class="text-ui-xs">Name</Label>
      <Input
        id="save-query-name"
        bind:value={saveQueryName}
        class="font-mono text-ui-sm"
        placeholder="Query name"
        onkeydown={(e) => {
          if (e.key === 'Enter') void confirmSaveQuery()
          // Explicit Escape close: this dialog is opened programmatically from
          // Monaco (⌘S), and bits-ui's default escape-to-close doesn't fire
          // reliably through that focus path in the WebKit/Tauri webview.
          else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); saveDialogOpen = false }
        }}
      />
    </div>
    <Dialog.Footer class="gap-2 sm:justify-end">
      <Button type="button" variant="outline" size="sm" onclick={() => (saveDialogOpen = false)}>
        Cancel
      </Button>
      <Button
        type="button"
        size="sm"
        disabled={!sql.trim() || savingQuery}
        onclick={() => void confirmSaveQuery()}
      >
        {savingQuery ? 'Saving…' : 'Save'}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
