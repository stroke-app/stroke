<script>
  import { rowsToCsv, rowsToJson, rowsToSql, rowsToTsv, rowsToMarkdown, rowsToJsonl, rowsToObjects, saveExportFile, buildExportFilename } from '$lib/export.js'
  import Play from "@lucide/svelte/icons/play";
  import WifiOff from "@lucide/svelte/icons/wifi-off";
  import Braces from "@lucide/svelte/icons/braces";
  import Wand2 from "@lucide/svelte/icons/wand-2";
  import Copy from "@lucide/svelte/icons/copy";
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Loader2 from "@lucide/svelte/icons/loader-2";
  import Bookmark from "@lucide/svelte/icons/bookmark";
  import Code2 from "@lucide/svelte/icons/code-2";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Download from "@lucide/svelte/icons/download";
  import Table2 from "@lucide/svelte/icons/table-2";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import { toast } from "$lib/components/ui/sonner/toast.svelte.js";
  import { checkObjectSql, objectProblemSummary } from "$lib/sql-object-check.js";
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
    loadScopedParamValues,
    saveScopedParamValues,
    clearStoredParamValues,
  } from "$lib/sql-params.js";
  import { applyAutoLimit } from "$lib/sql-auto-limit.js";
  import { splitSqlStatements } from "$lib/sql-statements.js";
  import {
    clampSqlEditorHeight,
    loadLayout,
    saveLayout,
  } from "$lib/stores/layout.js";
  import { appSqlEditor, setSqlEditorOption } from "$lib/stores/settings.js";
  import { SQL_EDITOR_FIELDS, SQL_EDITOR_TEXT_SIZES } from "$lib/sql-editor-options.js";
  import { sortRowsByColumn } from "$lib/result-sort.js";
  import SqlErrorConsole from "./SqlErrorConsole.svelte";
  import { untrack, onDestroy, tick } from "svelte";
  import { formatCompactCount } from "$lib/table-list.js";

  /** @typedef {import('$lib/sql-complete-data.js').SqlSchemaHints} SqlSchemaHints */

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
    /** Set true to open the History list in the results pane; it reads back
     *  false once the list is up, so the next request opens it again. */
    queryHistoryVisible = $bindable(false),
    /** @type {import('$lib/stores/query-history.js').QueryHistoryEntry[]} */
    queryHistory = [],
    /** @type {import('$lib/stores/query-history.js').SavedQuery[]} */
    savedQueries = [],
    onqueryrefresh = async () => {},
    /** Load a query into this editor tab. @param {string} sql */
    onhistoryselect = /** @type {(sql: string) => unknown} */ ((sql) => {}),
    /** Save as a new query, named. @param {string} name @param {string} sql */
    onsavequery = async (name, sql) => {},
    /** Save in place when this tab belongs to a saved query, or its text is
     *  saved already. Resolves false when there is nothing to save into, and
     *  the name dialog opens. @type {(sql: string) => Promise<boolean>} */
    onsaveinplace = async (sql) => false,
    /** Name of the saved query this tab belongs to, or ''. */
    savedQueryName = '',
    /** Called when user clicks "Fix with AI" - parent opens sidebar and sends the message */
    /** @param {{ error: string, sql: string }} detail */
    onfixwithai = /** @type {((detail: { error: string, sql: string }) => void) | undefined} */ (undefined),
    /**
     * Revert a run from its statement's lens (sql-undo.js). Resolves whether
     * the revert ran, so the lens drops its button.
     * @type {((undoId: string) => Promise<boolean>) | undefined}
     */
    onrevertrun = undefined,
    onprorequired = /** @type {() => void} */ (() => {}),
    /** The result lives in the backend's result store and `rows` is a sparse
     *  view of it (stored-result-view.js): the grid runs windowed, sorting
     *  happens in the backend, export reads the store. */
    windowed = false,
    dataVersion = 0,
    /** @type {{ slow: boolean, failed: boolean } | null} */
    windowStatus = null,
    /** A stored result is being sorted in the backend. */
    sorting = false,
    onvisiblerange = /** @type {(start: number, end: number) => void} */ (() => {}),
    onretrywindows = /** @type {() => void} */ (() => {}),
    /** @type {(sort: { column: string, direction: 'asc' | 'desc' } | null) => void} */
    onstoredsort = () => {},
    /** Every row of the result, read from the store for a windowed one.
     *  @type {(onprogress?: (n: number) => void) => Promise<any[][]>} */
    onloadallrows = async () => rows,
    /** Each statement of the last run: its text, and its error and where it
     *  failed (a 1-based position into that text) when it failed; its time and
     *  rows (or rows affected) when it ran.
     *  @type {Array<{ sql: string, error: string | null, position: number | null, ms?: number | null, rows?: number | null, affected?: number | null }>} */
    runOutcomes = [],
    /** Where this tab's variable values are kept: its connection, and its saved
     *  query or its title (tab ids restart every session). */
    paramScope = '',
    /** Run SQL in a new editor tab (a statement's New tab action). @type {((sql: string) => void) | undefined} */
    onrunnewtab = undefined,
    /** Ask the AI chat about a statement (its Ask AI action). @type {((sql: string) => void) | undefined} */
    onaskai = undefined,
  } = $props();

  /** @type {{ focus: () => void, markRunning: (ranStatement?: string | null) => void, markExecuted: (ranStatement?: string | null, run?: { ms?: number | null, rows?: number | null }) => void, markOutcomes: (outcomes: Array<{ sql: string, sent?: string, error: string | null, position: number | null, ms?: number | null, rows?: number | null, affected?: number | null }>) => void, clearRunMarks: () => void, markReverted: (undoId: string) => void, replaceInFailed: (name: string, replacement: string) => boolean, getStatementAtCursor: () => string, getSelectionText: () => string } | null} */
  let sqlEditorRef = $state(null)

  /** Mod+R from outside the editor: the selection, else the statement at the cursor. */
  export function runStatementAtCursor() {
    const stmt = sqlEditorRef?.getSelectionText?.() || sqlEditorRef?.getStatementAtCursor?.() || ''
    if (stmt.trim()) handleRun(stmt)
  }

  /** Run the whole editor, as Run does (a statement opened in a new tab). */
  export function runEditor() {
    handleRun(undefined)
  }

  /** Focus the SQL editor - called by the parent when this tab becomes active. */
  export function focusEditor() {
    sqlEditorRef?.focus()
  }

  /**
   * Replace the editor content with a snippet template and walk its fields.
   * @param {string} template
   */
  export function applySnippet(template) {
    return /** @type {any} */ (sqlEditorRef)?.insertSnippet?.(template) ?? false
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

  // ── Variables (:name, $name, ${name}) ───────────────────────────────────────
  /** @type {Record<string, import('$lib/sql-params.js').SqlParamValue>} */
  let paramValues = $state({})
  let paramsPanelOpen = $state(false)
  const paramOpts = $derived({ engine })
  const sqlParams = $derived($appSqlEditor.variables ? extractSqlParams(sql, paramOpts) : [])
  /** Values while Remember is off: until the app closes, per scope. */
  const sessionParamValues = new Map()

  // Each tab has its own values (this one console serves every editor tab).
  $effect(() => {
    const scope = paramScope
    untrack(() => {
      paramValues = $appSqlEditor.rememberVariables ? loadScopedParamValues(scope) : { ...(sessionParamValues.get(scope) ?? {}) }
    })
  })
  // Remember switched off: what is on screen stays for this session, and
  // nothing stays on disk.
  let rememberedBefore = untrack(() => $appSqlEditor.rememberVariables)
  $effect(() => {
    const remember = $appSqlEditor.rememberVariables
    untrack(() => {
      if (rememberedBefore && !remember) {
        sessionParamValues.set(paramScope, paramValues)
        clearStoredParamValues()
      }
      rememberedBefore = remember
    })
  })

  /** @param {string} name @param {import('$lib/sql-params.js').SqlParamValue} next */
  function setParam(name, next) {
    paramValues = { ...paramValues, [name]: next }
    if ($appSqlEditor.rememberVariables) saveScopedParamValues(paramScope, paramValues)
    else sessionParamValues.set(paramScope, paramValues)
  }

  /**
   * The text to send for `target`: variables filled in as escaped literals
   * (the substituted text is what executes and what history records, so runs
   * stay reproducible), then a LIMIT where Settings asks for one. Null when a
   * variable still has no value: the Variables panel opens and the run waits.
   * @param {string} target
   * @returns {{ text: string, changed: boolean } | null}
   */
  function prepareRun(target) {
    let text = target
    let changed = false
    if ($appSqlEditor.variables && extractSqlParams(target, paramOpts).length > 0) {
      if (missingSqlParams(target, paramValues, paramOpts).length > 0) {
        paramsPanelOpen = true
        return null
      }
      text = substituteSqlParams(target, paramValues, dialectForEngine(engine), paramOpts)
      changed = true
    }
    const limited = applyAutoLimit(text, $appSqlEditor.autoLimit, engine)
    if (limited.changed) {
      text = limited.sql
      changed = true
    }
    return { text, changed }
  }

  /**
   * The editor's own text of each statement of the last run, when what was
   * sent differs from it, so the marks land on the statements as written.
   * @type {string[] | null}
   */
  let runOriginals = null
  /** The statement a run waiting on a variable was for (undefined: the whole editor). */
  let pendingRunSql = /** @type {string | undefined} */ (undefined)

  /**
   * A CREATE TRIGGER / FUNCTION / PROCEDURE / VIEW that names a table or
   * column the schema does not have stops here: every engine creates it as
   * written and it fails the first time it runs (for a trigger, on every write
   * to its table). The toast says what is wrong; Run anyway sends it.
   * @param {string} target @param {() => void} runAnyway
   * @returns {boolean} whether the run was stopped
   */
  function stopForObjectProblems(target, runAnyway) {
    const problem = objectProblemSummary(checkObjectSql(target, schemaHints, engine).diags)
    if (!problem) return false
    toast.error(problem, {
      description: 'Not created: it would fail when it runs. Fix the underlined name, or run it as written.',
      action: { label: 'Run anyway', onClick: runAnyway },
    })
    return true
  }

  /**
   * @param {string | undefined} statementSql
   * @param {boolean} [checked] the object check was seen and overridden
   * @returns {boolean} whether it ran
   */
  function handleRun(statementSql, checked = false) {
    const single = typeof statementSql === 'string' && statementSql.trim() ? statementSql : undefined
    const target = single ?? sql
    if (!checked && stopForObjectProblems(target, () => handleRun(statementSql, true))) return false
    const prep = prepareRun(target)
    // Waiting on a variable: Enter in the panel runs this same target.
    pendingRunSql = prep ? undefined : single
    if (!prep) return false
    lastRanStatement = single ?? null
    runOriginals = prep.changed ? splitSqlStatements(target).map((st) => st.text) : null
    onrun(prep.changed ? prep.text : single)
    return true
  }

  /**
   * A statement action from the row above it in the editor.
   * @param {'run' | 'newtab' | 'json' | 'variables' | 'ai'} action @param {string} text
   */
  function onStatementAction(action, text) {
    if (action === 'variables') {
      pendingRunSql = text
      paramsPanelOpen = true
      return
    }
    if (action === 'ai') {
      onaskai?.(text)
      return
    }
    if (action === 'newtab' && onrunnewtab) {
      if (stopForObjectProblems(text, () => { const p = prepareRun(text); if (p) onrunnewtab(p.text) })) return
      const prep = prepareRun(text)
      if (prep) onrunnewtab(prep.text)
      else pendingRunSql = text
      return
    }
    if (action === 'json' && !$hasPro) {
      onprorequired()
      return
    }
    if (handleRun(text) && action === 'json') {
      outputView = 'json'
      if (!outputVisible) toggleOutput()
    }
  }

  /** The last run's outcomes, named by the editor's text where the sent text differs. */
  function outcomesForMarks() {
    const orig = runOriginals
    if (!orig || orig.length !== runOutcomes.length) return runOutcomes
    return runOutcomes.map((o, i) => ({ ...o, sql: orig[i], sent: o.sql }))
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
  /** @type {'table' | 'chart' | 'json' | 'explain' | 'error' | 'history' | 'saved' | 'charts'} */
  let outputView = $state('table')
  /**
   * History, Saved and the saved charts open in the results pane, beside the
   * result views. They were a column beside the editor, which narrowed it.
   */
  const LIST_VIEWS = /** @type {const} */ ([
    { id: 'history', label: 'History', hint: 'Every query run on this connection, from any editor tab' },
    { id: 'saved', label: 'Saved', hint: 'Queries saved on this connection' },
    { id: 'charts', label: 'Charts', hint: 'Charts saved on this connection' },
  ])
  const listView = $derived(outputView === 'history' || outputView === 'saved' || outputView === 'charts')
  /** The result view to go back to when a list closes. */
  let lastResultView = /** @type {'table' | 'chart' | 'json' | 'explain' | 'error'} */ ('table')
  $effect(() => {
    const v = outputView
    if (v !== 'history' && v !== 'saved' && v !== 'charts') lastResultView = v
  })

  /** Result views on screen, rather than a list. */
  const resultShown = $derived(outputVisible && !listView)

  /** @param {'history' | 'saved' | 'charts'} view */
  function openList(view) {
    outputView = view
    if (!outputVisible) toggleOutput()
  }

  /** Open a list, or close the open one back to the result. @param {'history' | 'saved' | 'charts'} view */
  function toggleList(view) {
    if (!outputVisible || outputView !== view) return openList(view)
    closeList()
  }

  function closeList() {
    outputView = lastResultView === 'error' && !shownError ? 'table' : lastResultView
    sqlEditorRef?.focus()
  }

  /** Ctrl/Cmd+Shift+B: the History list, or back to the result. */
  const toggleHistory = () => toggleList('history')

  // The shell asks for the History list (the palette, the menu) by setting the
  // flag; it is a request, not a state, so it is handed back at once.
  $effect(() => {
    if (!queryHistoryVisible) return
    untrack(() => {
      openList('history')
      queryHistoryVisible = false
    })
  })

  /** Load a listed query into the editor, then run it. @param {string} text */
  async function runListed(text) {
    await onhistoryselect(text)
    await tick()
    handleRun(undefined)
  }
  let chartType = $state('bar')
  let activeResultIdx = $state(0)
  /** @type {object | null} */
  let explainResult = $state(null)
  let explainLoading = $state(false)
  let explainError = $state('')
  let selected = $state(new Set())

  /** @typedef {{ columns: any[], rows: any[][], message: string, queryMs: number, sql: string, error: string, errorPosition: number | null }} ResultSet */

  /** One entry per statement when the last run executed multiple statements.
   *  A failed one carries its error: the rest still ran, and show. */
  const resultSets = $derived(
    (multiResults?.length ?? 0) > 1
      ? multiResults.map((res) => /** @type {ResultSet} */ ({
          columns: res.columns ?? [],
          rows: res.rows ?? [],
          message: res.message ?? '',
          queryMs: res.query_ms ?? res.queryMs ?? 0,
          sql: res.sql ?? '',
          error: res.error ?? '',
          errorPosition: res.errorPosition ?? null,
        }))
      : []
  )
  const activeSet = $derived(resultSets.length > 1 ? resultSets[Math.min(activeResultIdx, resultSets.length - 1)] : null)
  /** The error on screen: the selected statement's, or the run's. */
  const shownError = $derived(activeSet?.error || error)
  /** The statement that error belongs to and where it failed, for the console. */
  const failedStatement = $derived.by(() => {
    if (activeSet?.error) return { sql: activeSet.sql, position: activeSet.errorPosition }
    const only = runOutcomes.length === 1 ? runOutcomes[0] : null
    if (error && only?.error) return { sql: only.sql, position: only.position }
    return { sql: '', position: /** @type {number | null} */ (null) }
  })

  // After a script with a failure, open the first failed statement's tab: the
  // error is what needs reading, and the tab strip shows the rest ran.
  $effect(() => {
    const sets = resultSets
    untrack(() => {
      const failed = sets.findIndex((r) => r.error)
      if (failed >= 0) activeResultIdx = failed
    })
  })

  // A new run replaces the previous results: reset selection and result-set
  // index, and leave the explain view (it shows the previous query's plan).
  $effect(() => {
    if (!loading) return
    untrack(() => {
      selected = new Set()
      activeResultIdx = 0
      resultSort = null
      // A run is for its result: a list (History, Saved) gives way to it.
      if (outputView === 'explain' || listView) outputView = 'table'
      // Settings → SQL editor → Show results when a query runs. Not saved as
      // the pane's state: collapsing it again is still remembered.
      if (!outputVisible && $appSqlEditor.revealResultsOnRun) outputVisible = true
    })
  })

  // The editor gutter follows the run: a spinner beside the statement(s) while
  // they run, a ✓ when they finish OK, nothing after an error or a Stop (the
  // results pane says which). ⌘R marks one statement, Run marks all.
  let wasLoading = false
  $effect(() => {
    const l = loading
    const err = error
    untrack(() => {
      if (l && !wasLoading) sqlEditorRef?.markRunning?.(lastRanStatement)
      else if (wasLoading && !l) {
        // Per statement when the run reported them: ✓ or ✗ each, failures
        // underlined where the database says they failed.
        if (runOutcomes.length && !/Query cancelled/i.test(err)) sqlEditorRef?.markOutcomes?.(outcomesForMarks())
        else if (err) sqlEditorRef?.clearRunMarks?.()
        else sqlEditorRef?.markExecuted?.(lastRanStatement, { ms: queryMs, rows: columns.length ? rows.length : null })
      }
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

  /** The query sorts its own rows (checked on the editor text: good enough for a hint). */
  const hasOrderBy = $derived(/\border\s+by\b/i.test(sql))

  const currentDisplay = $derived.by(() => {
    const base = resultSets.length > 1
      ? (() => {
          const idx = Math.min(activeResultIdx, resultSets.length - 1)
          const s = resultSets[idx]
          return { columns: s.columns, rows: s.rows, queryMs: s.queryMs, message: s.message }
        })()
      : { columns, rows, queryMs, message }
    // A stored result is sorted in the backend; the view refetches in order.
    if (!resultSort || windowed) return base
    const colIdx = base.columns.findIndex((c) => (c.name ?? c) === resultSort.column)
    if (colIdx < 0) return base
    // Typed by the column's database type, one key per row (result-sort.js).
    const col = base.columns[colIdx]
    return { ...base, rows: sortRowsByColumn(base.rows, colIdx, col?.dataType, resultSort.direction === 'desc') }
  })

  /** @param {{ column: string, direction: 'asc' | 'desc' } | null} sort */
  function handleResultSort(sorts) {
    // DataTable now emits the full ordered key list; SQL result sorting is
    // single-column (client-side), so take the primary key.
    resultSort = Array.isArray(sorts) ? (sorts[0] ?? null) : sorts
    // Row indices change with the order - selection would point at the wrong rows.
    selected = new Set()
    if (windowed) onstoredsort(resultSort)
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
        toggleHistory()
        onmodshiftb?.()
      }
      // Mod+S is the shell's (it saves this query from anywhere in the tab):
      // handling it here as well saved the query twice.
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

  const jsonRows = $derived.by(() => {
    const head = currentDisplay.rows.length > JSON_VIEW_ROWS
      ? currentDisplay.rows.slice(0, JSON_VIEW_ROWS)
      : currentDisplay.rows
    // A windowed result's array is sparse: only the rows it holds.
    return windowed ? head.filter((r) => r !== undefined) : head
  })
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
    // A windowed result holds only the rows near the viewport here.
    const rows = /** @type {any[][]} */ (windowed ? await onloadallrows() : currentDisplay.rows)
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

  // Results beside the editor: the editor's width, kept like its height is.
  const EDITOR_WIDTH_KEY = 'stroke:sql-editor-width';
  /** @type {HTMLElement | null} */
  let splitEl = $state(null);
  let editorWidth = $state((() => {
    try { const n = Number(localStorage.getItem(EDITOR_WIDTH_KEY)); return Number.isFinite(n) && n > 0 ? n : 560 } catch { return 560 }
  })());
  let resizeStartWidth = 560;
  /** Both sides keep room to work in. @param {number} w */
  function clampEditorWidth(w) {
    const total = splitEl?.clientWidth ?? 0;
    const max = total > 0 ? Math.max(280, total - 320) : 1600;
    return Math.round(Math.min(max, Math.max(280, w)));
  }
  const beside = $derived($appSqlEditor.resultsBeside && outputVisible);
  /** @type {(() => Promise<void>) | null} */
  let formatSql = $state(null);

  let saveDialogOpen = $state(false);
  let saveQueryName = $state('');
  let savingQuery = $state(false);

  /** @param {string} undoId */
  async function revertFromLens(undoId) {
    if (await onrevertrun?.(undoId)) sqlEditorRef?.markReverted?.(undoId)
  }

  /**
   * "Did you mean" in the error console: put the name in the failed statement.
   * The editor refuses once that statement has been edited since the run, and
   * then the name goes to the clipboard instead of nowhere.
   * @param {string} name @param {string} replacement
   */
  async function applySuggestion(name, replacement) {
    if (sqlEditorRef?.replaceInFailed?.(name, replacement)) return
    try {
      await navigator.clipboard.writeText(replacement)
      toast.info(`Copied ${replacement}`, { description: 'The statement changed since it ran, so it was not edited.' })
    } catch { /* clipboard unavailable - no-op */ }
  }

  function fixWithAi() {
    const failedSql = activeSet?.error ? activeSet.sql : sql
    if (!shownError || !failedSql.trim()) return
    onfixwithai?.({ error: shownError, sql: failedSql.trim() })
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
  const stopped = $derived(/Query cancelled/i.test(shownError))
  // Tauri errors arrive as `Error: <message>`; the prefix repeats the header.
  const errorText = $derived(shownError.replace(/^Error:\s*/, ''))

  let errorCopied = $state(false)
  /** @type {ReturnType<typeof setTimeout> | null} */
  let errorCopyTimer = null
  async function copyError() {
    if (!shownError) return
    try {
      await navigator.clipboard.writeText(shownError)
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

  /** Save as a new query: the name dialog. Ctrl/Cmd+Shift+S, Shift+click Save. */
  function openSaveDialog() {
    if (!sql.trim()) return;
    saveQueryName = savedQueryName ? `${savedQueryName} copy` : queryTitle(sql);
    saveDialogOpen = true;
  }

  /**
   * Ctrl/Cmd+S and the Save button. A tab that belongs to a saved query saves
   * into it; only text nobody saved yet asks for a name.
   */
  export async function saveQuery() {
    if (!sql.trim() || savingQuery || saveDialogOpen) return;
    savingQuery = true;
    let handled = false;
    try {
      handled = await onsaveinplace(sql);
    } finally {
      savingQuery = false;
    }
    if (!handled) openSaveDialog();
  }

  let queryCopied = $state(false)
  /** @type {ReturnType<typeof setTimeout> | null} */
  let queryCopiedTimer = null

  /** @param {'sql' | 'drizzle' | 'prisma'} kind */
  function copyQueryAs(kind) {
    const code = kind === 'sql' ? sql : kind === 'drizzle' ? sqlToDrizzle(sql) : sqlToPrisma(sql, [])
    navigator.clipboard.writeText(code).then(() => {
      queryCopied = true
      if (queryCopiedTimer) clearTimeout(queryCopiedTimer)
      queryCopiedTimer = setTimeout(() => { queryCopied = false }, 2000)
    }).catch(() => toast.error('Copy failed', { description: 'The clipboard is not available.' }))
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

  /** How a variable's value goes into the SQL. */
  const PARAM_MODES = [
    { value: 'auto', label: 'Auto', hint: 'Numbers, true / false and NULL as they are; anything else as a quoted string' },
    { value: 'text', label: 'Text', hint: 'Always a quoted string' },
    { value: 'raw', label: 'Raw SQL', hint: 'Inserted as written: a function call, a column, an expression' },
    { value: 'null', label: 'NULL', hint: 'NULL, whatever is typed' },
  ]

  onDestroy(() => {
    // Clear copy-feedback timers so they don't fire state writes after unmount
    if (queryCopiedTimer) clearTimeout(queryCopiedTimer)
    if (errorCopyTimer) clearTimeout(errorCopyTimer)
  })
</script>

<div class="flex min-h-0 flex-1 overflow-hidden">
  <div bind:this={consoleEl} class="flex min-h-0 min-w-0 flex-1 flex-col">
  <div
    class="studio-chrome flex h-9 shrink-0 items-center gap-1.5 border-b border-border bg-panel px-2"
    data-studio-chrome
  >
    <!-- Run leads: it is what this toolbar is for. Stop takes its place in the
         same grid cell, so it is exactly Run's width and nothing after it moves
         when a run starts. Run stays mounted (invisible) to hold the width. -->
    <div class="grid shrink-0">
      <!-- Split button: one wrapper owns the radius + shadow; the halves are
           plain buttons (the Button component's transparent border,
           bg-clip-padding and elevate shadow would each paint a seam). -->
      <div class={cn('col-start-1 row-start-1 flex shrink-0 items-stretch overflow-hidden rounded-md elevate-1', loading && 'invisible')}>
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
      {#if loading}
        <!-- No spinner here: the results pane and the editor gutter already
             say "running". The square says what a click does. -->
        <button
          type="button"
          class="group col-start-1 row-start-1 inline-flex h-7 w-full shrink-0 select-none items-center gap-2 rounded-md border border-border bg-muted/40 pl-2 pr-2.5 text-ui-2xs font-medium text-foreground transition-[background-color,border-color,scale] duration-150 hover:border-destructive/40 hover:bg-destructive/10 active:scale-[0.96] disabled:pointer-events-none disabled:opacity-60"
          disabled={stopping}
          onclick={stopRun}
          title={tipText('Stop', 'Cancel the running query.')}
        >
          <!-- Drawn, not Lucide's square: that one carries its own padding in
               a 24px box and sits off-centre at this size. -->
          <span class="grid size-3.5 shrink-0 place-items-center" aria-hidden="true">
            <span class="size-2.5 rounded-[2px] bg-destructive"></span>
          </span>
          {stopping ? 'Stopping' : 'Stop'}
        </button>
      {/if}
    </div>

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
        disabled={txBusy || loading}
        onclick={() => oncommittransaction()}
        title={tipText('Commit', `Save everything this transaction has done: ${txStatus.statements} statement(s), ${txStatus.rowsAffected} row(s) changed.`)}
      >
        Commit
      </Button>
      <Button
        variant="ghost"
        size="sm"
        class="h-7 shrink-0 text-muted-foreground hover:bg-destructive/10 hover:text-destructive focus-visible:bg-destructive/10 focus-visible:text-destructive"
        disabled={txBusy || loading}
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
        disabled={txBusy || loading}
        onclick={() => onbegintransaction()}
        title={tipText(
          'Begin transaction',
          'Run statements without saving them, then commit or roll back.',
        )}
      >
        Begin
      </Button>
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
        title={tipText('Variables', 'Set values for :name, $name and ${name} variables. They are inlined as escaped literals when the query runs.')}
      >
        <Variable class="size-3.5 shrink-0" />
        Variables
        <span class="rounded bg-muted/70 px-1 font-mono text-ui-3xs tabular-nums text-muted-foreground">{sqlParams.length}</span>
      </Button>
    {/if}

    <!-- Grouped by space, not rules: the run controls lead, then what to do
         with this query (format, explain, copy), then the trailing edge with
         the editor's view options and Save, the last thing a session ends on. -->
    <div class="ml-2 flex min-w-0 items-center gap-0.5">
      <Button
        type="button"
        variant="ghost"
        size="sm"
        class="size-7 p-0 text-muted-foreground hover:text-foreground"
        disabled={!sql.trim()}
        onclick={() => void formatSql?.()}
        aria-label="Format SQL"
        title={tipText('Format SQL', 'Reformat the whole editor with consistent casing and indentation.')}
      >
        <Braces class="size-3.5 shrink-0" />
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
        aria-label="Explain plan"
        title={tipText('Explain plan', 'Visualize how the database executes this query, spot slow scans and missing indexes.')}
      >
        {#if explainLoading}
          <Loader2 class="size-3.5 shrink-0 animate-spin" />
        {:else}
          <ScanSearch class="size-3.5 shrink-0" />
        {/if}
      </Button>

      <!-- Copy reads as copy: the clipboard icon and a caret for the menu,
           the same 28px shape as its neighbours. The icon turns into a check
           for two seconds after a copy, so the click is seen to land. -->
      <DropdownMenu.Root>
        <DropdownMenu.Trigger
          class="inline-flex h-7 shrink-0 items-center gap-0.5 rounded-md pl-1.5 pr-1 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground data-[state=open]:bg-accent data-[state=open]:text-foreground disabled:pointer-events-none disabled:opacity-50"
          disabled={!sql.trim()}
          aria-label="Copy query as"
          title={tipText('Copy', 'Copy the query as SQL, or as a Drizzle or Prisma call.')}
        >
          {#if queryCopied}
            <Check class="size-3.5 shrink-0 text-success" />
          {:else}
            <Copy class="size-3.5 shrink-0" />
          {/if}
          <ChevronDown class="size-3 shrink-0" />
        </DropdownMenu.Trigger>
        <DropdownMenu.Content align="start" class="min-w-48">
          <DropdownMenu.Item onSelect={() => copyQueryAs('sql')}>
            <Copy class="size-3.5 shrink-0 text-muted-foreground" />
            <span data-slot="menu-label">Copy SQL</span>
          </DropdownMenu.Item>
          <DropdownMenu.Separator />
          <DropdownMenu.Item onSelect={() => copyQueryAs('drizzle')}>
            <Code2 class="size-3.5 shrink-0 text-muted-foreground" />
            <span data-slot="menu-label">Copy as Drizzle</span>
          </DropdownMenu.Item>
          <DropdownMenu.Item onSelect={() => copyQueryAs('prisma')}>
            <Code2 class="size-3.5 shrink-0 text-muted-foreground" />
            <span data-slot="menu-label">Copy as Prisma</span>
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Root>
    </div>

    <div class="ml-auto flex shrink-0 items-center gap-1.5">
      <!-- The editor's own view options, the same switches as Settings →
           Database → SQL editor. Toggles keep the menu open, so several can
           be flipped in one visit. -->
      <DropdownMenu.Root>
        <DropdownMenu.Trigger
          class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground data-[state=open]:bg-accent data-[state=open]:text-foreground"
          aria-label="Editor options"
          title={tipText('Editor options', 'Line numbers, wrapping, suggestions and text size. Also in Settings → Database → SQL editor.')}
        >
          <SlidersHorizontal class="size-3.5 shrink-0" />
        </DropdownMenu.Trigger>
        <DropdownMenu.Content align="end" class="min-w-52">
          {#each SQL_EDITOR_FIELDS.filter((f) => f.kind === 'bool' && /** @type {any} */ (f).menu !== false) as field (field.key)}
            <DropdownMenu.CheckboxItem
              checked={$appSqlEditor[field.key] === true}
              closeOnSelect={false}
              onCheckedChange={(on) => setSqlEditorOption(/** @type {any} */ (field.key), on)}
            >
              <span data-slot="menu-label">{field.label}</span>
              {#if field.key === 'wrap'}<DropdownMenu.Shortcut combo="Alt+Z" />{/if}
            </DropdownMenu.CheckboxItem>
          {/each}
          <DropdownMenu.Separator />
          <DropdownMenu.Item onSelect={() => sqlEditorRef?.foldAll?.()}>
            <span data-slot="menu-label">Fold all statements</span>
            <DropdownMenu.Shortcut combo="Ctrl+Alt+[" />
          </DropdownMenu.Item>
          <DropdownMenu.Item onSelect={() => sqlEditorRef?.unfoldAll?.()}>
            <span data-slot="menu-label">Unfold all</span>
            <DropdownMenu.Shortcut combo="Ctrl+Alt+]" />
          </DropdownMenu.Item>
          <DropdownMenu.Separator />
          <DropdownMenu.Label>Text size</DropdownMenu.Label>
          <DropdownMenu.RadioGroup
            value={$appSqlEditor.textSize}
            onValueChange={(v) => setSqlEditorOption('textSize', /** @type {any} */ (v))}
          >
            {#each SQL_EDITOR_TEXT_SIZES as size (size.id)}
              <DropdownMenu.RadioItem value={size.id} closeOnSelect={false}>{size.label}</DropdownMenu.RadioItem>
            {/each}
          </DropdownMenu.RadioGroup>
        </DropdownMenu.Content>
      </DropdownMenu.Root>

      <!-- Save at the trailing edge, labelled: it is the one action here that
           keeps work, and an icon alone read as a bookmark toggle. -->
      <Button
        type="button"
        variant="outline"
        size="sm"
        class="h-7 shrink-0 gap-1.5"
        disabled={!sql.trim() || savingQuery}
        onclick={(e) => (e.shiftKey ? openSaveDialog() : void saveQuery())}
        title={savedQueryName
          ? tipText(`Save "${savedQueryName}"`, `Writes this tab into the saved query. Shift+click or ${mod}⇧S saves a copy under a new name.`, [mod, 'S'])
          : tipText('Save query', 'Keep this query for later, under Saved in the results pane, per connection.', [mod, 'S'])}
      >
        {#if savingQuery}
          <Loader2 class="size-3.5 shrink-0 animate-spin" />
        {:else}
          <Bookmark class={cn('size-3.5 shrink-0', savedQueryName && 'fill-current')} />
        {/if}
        Save
      </Button>
    </div>
  </div>

  {#if paramsPanelOpen && sqlParams.length > 0}
    <!-- One field per variable: the name leads as its prefix, the value takes
         the width, and how the value goes into the SQL is a small menu at the
         field's end. It was three separate boxes spread across the row, with
         the close button stranded far from them. -->
    <div class="shrink-0 border-b border-border/60 bg-panel px-3 py-2">
      <div class="flex w-full max-w-xl flex-col gap-1.5">
        <div class="flex h-6 items-center gap-2">
          <Variable class="size-3.5 shrink-0 text-muted-foreground" />
          <span class="text-ui-xs font-medium text-foreground">Variables</span>
          <span class="font-mono text-ui-2xs tabular-nums text-muted-foreground">{sqlParams.length}</span>
          <span class="ml-auto text-ui-2xs text-muted-foreground">Enter to run</span>
          <button
            type="button"
            class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            aria-label="Close variables"
            title="Close (Esc)"
            onclick={() => (paramsPanelOpen = false)}
          >
            <X class="size-3.5" />
          </button>
        </div>
        {#each sqlParams as p (p.name)}
          {@const v = paramValues[p.name] ?? { value: '', mode: 'auto' }}
          {@const mode = PARAM_MODES.find((m) => m.value === v.mode) ?? PARAM_MODES[0]}
          <div class="field-surface flex h-8 min-w-0 items-stretch overflow-hidden bg-input/30 focus-within:outline-2 focus-within:outline-offset-1 focus-within:outline-ring">
            <span
              class="flex min-w-[6.5rem] max-w-[14rem] shrink-0 items-center truncate border-r border-border/60 px-2.5 font-mono text-ui-xs text-foreground"
              title="{p.sigil}{p.name}"
            ><span class="text-muted-foreground">{p.sigil}</span>{p.name}</span>
            <input
              type="text"
              value={v.value}
              disabled={v.mode === 'null'}
              placeholder={v.mode === 'null' ? 'NULL' : v.mode === 'raw' ? 'now(), inserted as written' : 'Value'}
              aria-label="Value for {p.name}"
              spellcheck="false"
              autocomplete="off"
              class="no-focus-ring min-w-0 flex-1 bg-transparent px-2.5 font-mono text-ui-xs text-foreground outline-none placeholder:text-muted-foreground disabled:opacity-50"
              oninput={(e) => setParam(p.name, { ...v, value: e.currentTarget.value })}
              onkeydown={(e) => {
                if (e.key === 'Enter') handleRun(pendingRunSql)
                else if (e.key === 'Escape') { e.stopPropagation(); paramsPanelOpen = false }
              }}
            />
            <DropdownMenu.Root>
              <DropdownMenu.Trigger
                class="inline-flex shrink-0 items-center gap-1 border-l border-border/60 pl-2.5 pr-2 text-ui-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground data-[state=open]:bg-accent data-[state=open]:text-foreground"
                aria-label="How {p.name} goes into the SQL: {mode.label}"
                title={mode.hint}
              >
                {mode.label}
                <ChevronDown class="size-3 shrink-0" />
              </DropdownMenu.Trigger>
              <DropdownMenu.Content align="end" class="min-w-40">
                <DropdownMenu.RadioGroup value={v.mode} onValueChange={(m) => setParam(p.name, { ...v, mode: /** @type {any} */ (m) })}>
                  {#each PARAM_MODES as m (m.value)}
                    <DropdownMenu.RadioItem value={m.value} title={m.hint}>
                      <span data-slot="menu-label">{m.label}</span>
                    </DropdownMenu.RadioItem>
                  {/each}
                </DropdownMenu.RadioGroup>
              </DropdownMenu.Content>
            </DropdownMenu.Root>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Editor and results: one above the other, or side by side (Settings →
       SQL editor → Results beside the editor). Collapsed, the results bar
       always sits under the editor. -->
  <div bind:this={splitEl} class={cn('flex min-h-0 flex-1', beside ? 'flex-row' : 'flex-col')}>
  <div
    class={beside
      ? "relative shrink-0 overflow-hidden bg-panel"
      : outputVisible
        ? "relative shrink-0 overflow-hidden bg-panel"
        : "relative min-h-0 flex-1 overflow-hidden bg-panel"}
    style={beside ? `width: ${editorWidth}px` : outputVisible ? `height: ${editorHeight}px` : undefined}
  >
    <SqlEditor
      bind:this={sqlEditorRef}
      bind:value={sql}
      class="absolute inset-0"
      {schemaHints}
      dialect={engine}
      {onmodk}
      onmodenter={() => handleRun(undefined)}
      onrunstatement={(stmt) => handleRun(stmt)}
      onmods={() => void saveQuery()}
      onmodshifts={openSaveDialog}
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
      onmodshiftb={() => { toggleHistory(); onmodshiftb?.() }}
      onlens={onStatementAction}
      onrevert={onrevertrun ? (id) => void revertFromLens(id) : undefined}
      onactionsready={(actions) => {
        formatSql = actions.format;
      }}
    />
  </div>

  {#if beside}
  <ResizeHandle
    axis="x"
    edge="end"
    onresizestart={() => {
      resizeStartWidth = editorWidth;
    }}
    onresize={(dx) => {
      editorWidth = clampEditorWidth(resizeStartWidth + dx);
    }}
    onresizeend={() => {
      resizeStartWidth = editorWidth;
      try { localStorage.setItem(EDITOR_WIDTH_KEY, String(editorWidth)) } catch { /* only the width is lost */ }
    }}
  />
  {:else if outputVisible}
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
  <div class={outputVisible ? "flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden" : "flex shrink-0 flex-col"}>
    <!-- Output tab bar -->
    <div
      class="studio-chrome flex h-8 shrink-0 items-stretch border-b border-border bg-panel"
      data-studio-chrome
    >
      <!-- Result-set selector tabs (only when multi-result) -->
      {#if resultSets.length > 1}
        <!-- One pill per statement: compact, the selected one filled, a failed
             one red with its ✗, each with its row count so they tell apart.
             Scrolls sideways when a script has many. -->
        <div class="flex min-w-0 items-center gap-0.5 overflow-x-auto px-1.5 [scrollbar-width:none]" role="tablist" aria-label="Statement results">
          {#each resultSets as rs, i (i)}
            {@const rsActive = Math.min(activeResultIdx, resultSets.length - 1) === i}
            <button
              type="button"
              role="tab"
              aria-selected={rsActive}
              onclick={() => { activeResultIdx = i; resultSort = null; selected = new Set(); if (listView) outputView = 'table' }}
              title={rs.error ? rs.error.replace(/^Error:\s*/, '') : rs.sql}
              class={cn(
                'flex h-6 shrink-0 items-center gap-1 rounded-md px-2 text-ui-2xs font-medium whitespace-nowrap transition-colors',
                rs.error
                  ? rsActive ? 'bg-destructive/12 text-destructive' : 'text-destructive/80 hover:bg-destructive/10 hover:text-destructive'
                  : rsActive ? 'bg-muted text-foreground' : 'text-muted-foreground hover:bg-muted/50 hover:text-foreground',
              )}
            >
              {#if rs.error}<X class="size-3 shrink-0" aria-label="Failed" />{/if}
              Result {i + 1}
              {#if !rs.error}
                <span class={cn('font-mono text-ui-3xs tabular-nums', rsActive ? 'text-muted-foreground' : 'text-muted-foreground/70')}>
                  {rs.columns.length ? formatCompactCount(rs.rows.length) : '✓'}
                </span>
              {/if}
            </button>
          {/each}
        </div>
        <div class="mx-1 h-4 w-px shrink-0 self-center bg-border"></div>
      {/if}

      <!-- View tabs (icon-only). The Error tab appears with a label only after a
           failed run, tinted destructive so it reads at a glance. -->
      <div class="flex items-center gap-0.5 px-1">
        {#each [
          { id: 'table',   label: 'Table',   Icon: Table2,     pro: false },
          { id: 'chart',   label: 'Chart',   Icon: BarChart2,  pro: true },
          { id: 'json',    label: 'JSON',    Icon: Braces,     pro: true },
          { id: 'explain', label: 'Explain', Icon: ScanSearch, pro: true },
          ...(shownError ? [{ id: 'error', label: stopped ? 'Stopped' : 'Error', Icon: stopped ? Square : CircleAlert, pro: false }] : []),
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

      <!-- The lists, beside the result views. They are the connection's, so
           every editor tab shows the same ones. -->
      <div class="mx-0.5 h-4 w-px shrink-0 self-center bg-border" aria-hidden="true"></div>
      <div class="flex items-center gap-0.5 px-1">
        {#each LIST_VIEWS as lv (lv.id)}
          {@const on = outputVisible && outputView === lv.id}
          <button
            type="button"
            aria-pressed={on}
            onclick={() => toggleList(lv.id)}
            class={cn(
              'flex h-7 items-center rounded-md px-2 text-ui-2xs font-medium whitespace-nowrap transition-colors',
              on ? 'bg-muted/70 text-foreground' : 'text-muted-foreground hover:bg-muted/40 hover:text-foreground',
            )}
            title={lv.id === 'history' ? tipText(lv.label, lv.hint, [mod, '⇧', 'B']) : tipText(lv.label, lv.hint)}
          >{lv.label}</button>
        {/each}
      </div>

      <!-- Right: metadata + toggle -->
      <div class="ml-auto flex shrink-0 items-center gap-3 pr-1.5">
        {#if resultShown && currentDisplay.rows.length > 1 && !resultSort && !hasOrderBy}
          <!-- Without ORDER BY the database returns rows in whatever order it
               reads them (storage order, roughly), which is rarely id or time
               order. Said once, quietly, where the row count is. -->
          <span
            class="font-mono text-ui-2xs text-muted-foreground/70"
            title="The query has no ORDER BY, so rows come back in the order the database read them. Add ORDER BY, or click a column header to sort."
          >unordered</span>
        {/if}
        {#if resultShown && currentDisplay.rows.length > 0}
          <span class="font-mono text-ui-2xs tabular-nums text-muted-foreground">{formatCompactCount(currentDisplay.rows.length)} rows</span>
        {/if}
        {#if resultShown && currentDisplay.queryMs > 0}
          <span class="font-mono text-ui-2xs tabular-nums text-muted-foreground">{currentDisplay.queryMs}ms</span>
        {/if}
        {#if resultShown && currentDisplay.message}
          <span class="max-w-[160px] truncate font-mono text-ui-2xs text-muted-foreground">{currentDisplay.message}</span>
        {/if}

        <!-- Export dropdown, only when there are results -->
        {#if resultShown && currentDisplay.rows.length > 0}
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
          {#if outputView === 'history' || outputView === 'saved' || outputView === 'charts'}
            <QueryHistoryPanel
              view={outputView}
              history={queryHistory}
              saved={savedQueries}
              onselect={(text) => { void onhistoryselect(text); sqlEditorRef?.focus() }}
              onrun={(text) => void runListed(text)}
              onrefresh={onqueryrefresh}
              onescape={closeList}
            />
          {:else if outputView === 'error' || activeSet?.error}
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
            {:else if isNetworkError(shownError)}
              <div class="flex h-full flex-col items-center justify-center gap-2.5 px-6 text-center">
                <WifiOff class="size-6 text-muted-foreground" />
                <p class="font-mono text-ui-sm text-muted-foreground">Cannot reach database, check your connection and try again.</p>
              </div>
            {:else}
              <!-- The failed statement as a console prints it: the message, where
                   it failed, the statement with the failing text underlined. -->
              <SqlErrorConsole
                error={shownError}
                sql={failedStatement.sql}
                position={failedStatement.position}
                queryMs={currentDisplay.queryMs}
                dialect={engine}
                hints={schemaHints}
                copied={errorCopied}
                oncopy={copyError}
                onfixwithai={onfixwithai ? fixWithAi : undefined}
                onsuggest={applySuggestion}
              >
                {#if /statement timeout|canceling statement due to/i.test(shownError)}
                  <p class="text-ui-xs leading-relaxed text-muted-foreground">
                    The query ran past the statement timeout. If the table has large JSON or text columns, select only
                    the columns you need instead of <span class="font-mono text-foreground">*</span>, or add a smaller
                    <span class="font-mono text-foreground">LIMIT</span>.
                  </p>
                {/if}
              </SqlErrorConsole>
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
                loading={loading || sorting}
                bind:selected
                rowSort={resultSort}
                onsortchange={handleResultSort}
                {windowed}
                {dataVersion}
                {windowStatus}
                {onvisiblerange}
                onretrywindows={onretrywindows}
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
</div>

<Dialog.Root bind:open={saveDialogOpen}>
  <Dialog.Content class="max-w-md gap-4">
    <Dialog.Header>
      <Dialog.Title class="text-ui-sm font-semibold">{savedQueryName ? 'Save as a new query' : 'Save query'}</Dialog.Title>
      <Dialog.Description class="text-ui-xs text-muted-foreground">
        {savedQueryName
          ? `A copy of "${savedQueryName}". This tab moves to the copy; ${mod}S then saves into it.`
          : `Saved queries are kept per connection, under Saved in the results pane. After this, ${mod}S saves this tab into it.`}
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
