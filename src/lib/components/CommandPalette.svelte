<script>
  import Icon from './Icon.svelte'
  import * as Command from '$lib/components/ui/command/index.js'
  import { formatTableRowCount } from '$lib/table-list.js'
  import { get } from 'svelte/store'
  import AiMarkdown from './AiMarkdown.svelte'
  import { chatCompletionStream, buildSystemPrompt, AI_TOOLS, isDestructiveSql, classifyDbError } from '$lib/ai.js'
  import { aiSettings, isAiConfigured } from '$lib/stores/ai-settings.js'
  import { pinEnabled, lockNow } from '$lib/stores/app-lock.js'
  import { appCmdkAi } from '$lib/stores/settings.js'
  import { executeSql } from '$lib/api.js'

  let {
    open = $bindable(false),
    /** @type {'root' | 'docker' | 'connections' | 'tables' | 'pages' | 'ask'} */
    page = $bindable('root'),
    connected = false,
    schemas = [],
    tables = [],
    activeSchema = 'public',
    /** @type {import('$lib/stores/connections.js').SavedConnection[]} */
    savedConnections = [],
    /** Currently connected connection id (to mark it) */
    activeConnectionId = '',
    ontableselect = () => {},
    onschemachange = () => {},
    onopensql = () => {},
    onnewsql = () => {},
    onopentable = () => {},
    onopensettings = () => {},
    onopenconnection = () => {},
    ondisconnect = () => {},
    onrefresh = () => {},
    onopenai = () => {},
    onopenaisidebar = () => {},
    /** Whether the app is currently in AI mode (fullscreen chat) */
    aiMode = false,
    /** Toggle between AI mode and dev mode */
    ontoggleaimode = () => {},
    onopenorm = () => {},
    onopenerd = () => {},
    onopenSchema = () => {},
    onopensecurity = () => {},
    hasSchemaExplorer = true,
    hasSecurity = true,
    /** Redis (key-value) connection - hides SQL-only pages, shows the keyspace. */
    isRedis = false,
    /** PostGIS present on this connection - gates the Map entry. */
    geoAvailable = false,
    onopenredis = () => {},
    onopenlogs = () => {},
    onopeninsights = () => {},
    onopenadvisor = () => {},
    onopenobjects = () => {},
    onopenormschema = () => {},
    onopendashboard = () => {},
    onopencharts = () => {},
    onopendiagrams = () => {},
    onopenbackup = () => {},
    ontogglequerylog = () => {},
    onopenextensions = () => {},
    onopenmap = () => {},
    onopenJsonViewer = () => {},
    onopenshortcuts = () => {},
    onopenabout = () => {},
    onopenreport = () => {},
    oncheckupdate = () => {},
    /** Whether the connection is currently read-only (locks edits/inserts/deletes). */
    readonly = false,
    /** Toggle read-only mode. */
    onreadonlytoggle = () => {},
    /** @param {'postgres'|'mysql'} dbType */
    ondockerlaunch = (dbType) => {},
    /** @param {import('$lib/stores/connections.js').SavedConnection} conn */
    onswitchdatabase = (conn) => {},
    /** @type {import('$lib/stores/query-history.js').QueryHistoryEntry[]} */
    queryHistory = [],
    /** @type {import('$lib/stores/query-history.js').SavedQuery[]} */
    savedQueries = [],
    /** @param {string} sql */
    onqueryselect = (sql) => {},
    onopenqueryhistory = () => {},
    onglobalsearch = () => {},
    onopennotebook = () => {},
    onopennotebookfile = () => {},
    openschematimeline = () => {},
    opendatadiff = () => {},
    /** Live AI schema/db context for the inline quick-ask. */
    schemaContext = null,
    /** Escalate a quick-ask question into the full AI sidebar/chat. @param {string} q */
    onaskcontinue = (q) => {},
  } = $props()

  /** @param {'docker' | 'connections' | 'tables' | 'ask' | 'pages'} target */
  function navigate(target) {
    page = target
  }

  // Single source of truth for page destinations - rendered both in the root
  // "Views" group and in the dedicated "Go to page" navigator (Ctrl/⌘+Shift+P). Add a
  // page once here and it shows up in both. `keys` is an optional shortcut hint.
  const pageItems = $derived([
    { icon: 'key-round',       label: 'Redis Keyspace',                action: onopenredis,      show: connected && isRedis, value: 'redis keyspace keys browse key value store console cli' },
    { icon: 'table-2',         label: 'Table data',       keys: '⌘⇧D', action: onopentable,      show: connected && !isRedis, value: 'open table data browser rows' },
    { icon: 'search',          label: 'Find in database', keys: '⌘⇧G', action: onglobalsearch,   show: connected && !isRedis, value: 'find search rows data across all tables global database' },
    { icon: 'terminal',        label: 'SQL editor',       keys: '⌘⇧S', action: onopensql,        show: connected && !isRedis, value: 'open sql editor query console' },
    { icon: 'plus',            label: 'New SQL Editor',                action: onnewsql,         show: connected && !isRedis, value: 'new sql editor tab additional multiple query console open another' },
    { icon: 'code-2',          label: 'ORM Runner',       keys: '⌘⇧O', action: onopenorm,        show: connected && !isRedis, value: 'open orm runner drizzle prisma query builder' },
    { icon: 'network',         label: 'Data Model',                    action: onopenerd,        show: connected && !isRedis, value: 'open data model er diagram erd entity relationship foreign key pk fk graph mermaid tree dictionary ddl schema map' },
    { icon: 'layout-template', label: 'Schema Explorer',               action: onopenSchema,     show: connected && hasSchemaExplorer, value: 'open schema explorer indexes enums views materialized' },
    { icon: 'shield-check',    label: 'Security',                      action: onopensecurity,   show: connected && hasSecurity, value: 'open security roles users policies rls row level' },
    { icon: 'shield-check',    label: 'Advisor',                       action: onopenadvisor,    show: connected && !isRedis, value: 'open advisor lint audit checks security performance schema rls unused index foreign key bloat' },
    { icon: 'database',        label: 'Instance Insights',             action: onopeninsights,   show: connected && !isRedis, value: 'open instance insights monitoring sessions locks replication config pg_settings' },
    { icon: 'layout-dashboard',label: 'Dashboard',                     action: onopendashboard,  show: connected && !isRedis, value: 'open dashboard saved metrics overview widgets' },
    { icon: 'bar-chart-2',     label: 'Charts',                        action: onopencharts,     show: connected && !isRedis, value: 'open charts visualize query results graphs plots' },
    { icon: 'git-branch',      label: 'Diagrams',                      action: onopendiagrams,   show: connected && !isRedis, value: 'open diagrams draw erd relationship map' },
    { icon: 'table-2',         label: 'Database Objects',              action: onopenobjects,    show: connected && !isRedis, value: 'open database objects overview tables views functions routines triggers sizes rows stats' },
    { icon: 'code-2',          label: 'Codegen',                       action: onopenormschema,  show: connected && !isRedis, value: 'codegen open schema as code prisma drizzle orm model relations types generate dml psl' },
    { icon: 'history',         label: 'Activity Log',                  action: onopenlogs,       show: connected, value: 'open activity log events history operations' },
    { icon: 'terminal',        label: 'Query Log console', keys: '⌘⇧K', action: ontogglequerylog, show: connected && !isRedis, value: 'toggle query log console sql executed statements bottom panel' },
    { icon: 'globe',           label: 'Map',                           action: onopenmap, show: connected && geoAvailable, value: 'open map geo postgis spatial geometry geography world plot points' },
    { icon: 'blocks',          label: 'Extensions',                    action: onopenextensions, show: connected && !isRedis, value: 'open extensions plugins formatters generators transforms better time uuid' },
    { icon: 'archive',         label: 'Backup & Restore',              action: onopenbackup,     show: connected && !isRedis, value: 'backup restore export import dump data' },
    { icon: 'braces',          label: 'JSON Viewer',                   action: onopenJsonViewer, show: connected, value: 'open json viewer explorer jsonpath tool' },
    { icon: 'git-branch',      label: 'Schema Timeline',               action: openschematimeline, show: connected && !isRedis, value: 'schema timeline drift detection history changes diff' },
    { icon: 'git-compare',     label: 'Data Diff',                     action: opendatadiff,     show: connected && !isRedis, value: 'data diff compare tables rows changes' },
    { icon: 'file-text',       label: 'New Notebook',                  action: onopennotebook,   show: connected && !isRedis, value: 'new sql notebook jupyter cells sql markdown' },
    { icon: 'file-text',       label: 'Open Notebook…',                action: onopennotebookfile, show: connected && !isRedis, value: 'open notebook file sqlnb' },
  ].filter((i) => i.show))

  function goBack() {
    if (page === 'ask') stopAsk()
    page = 'root'
  }

  // ── Quick-ask: an inline, tool-using mini-chat right inside the palette ──────
  // Uses the same AI_TOOLS + agentic loop as the sidebar so it can actually run
  // read-only queries and answer, and follow-ups continue the conversation.
  let _uidN = 0
  const uid = () => 'ask_' + (++_uidN)
  /** @typedef {{ id:string, role:'user'|'assistant'|'tool', text?:string, streaming?:boolean, sqls?:string[], status?:'running'|'done'|'error', label?:string, result?:{columns:string[],rows:any[][],total:number}|null }} AskTurn */
  let askTurns = $state(/** @type {AskTurn[]} */ ([]))
  let askStreaming = $state(false)
  let askError = $state('')
  /** @type {any[]} - API messages for the conversation (system prompt added per turn). */
  let askApi = []
  /** @type {AbortController | null} */
  let askController = null
  let askExecuted = new Set()
  const lastAskQuestion = $derived([...askTurns].reverse().find((t) => t.role === 'user')?.text ?? '')

  const QUICK_ASK_SYS =
    '\n\nYou are answering inside a command-palette quick-ask. Be concise and direct. ' +
    'Use the execute_sql tool to actually run read-only queries and answer with real data, do NOT just say you will query. ' +
    'Destructive statements (INSERT/UPDATE/DELETE/DROP/ALTER/TRUNCATE) are blocked here; if one is needed, put it in a ```sql block and tell the user to run it in the editor. ' +
    'Keep answers short and put any ready-to-run query in a ```sql block.'

  /** SELECT-guard: cap unbounded reads so quick-ask never streams a giant result. */
  function guardSql(/** @type {string} */ sql) {
    const t = sql.trimStart()
    if (/^(with\b|select\b)/i.test(t) && !/\blimit\s+\d/i.test(t)) return `${sql.replace(/;+\s*$/, '')}\nLIMIT 500`
    return sql
  }

  function updateAskTurn(/** @type {string} */ id, /** @type {Partial<AskTurn>} */ patch) {
    askTurns = askTurns.map((t) => (t.id === id ? { ...t, ...patch } : t))
  }

  /** Start a fresh quick-ask conversation. @param {string} question */
  async function startAsk(question) {
    if (!get(appCmdkAi)) return  // experimental ⌘K AI is off
    const q = (question ?? '').trim()
    if (!q) return
    if (!isAiConfigured(get(aiSettings))) { onopensettings(); open = false; return }
    askApi = []; askTurns = []; askError = ''; askStick = true
    page = 'ask'; paletteSearch = ''
    await askTurn(q)
  }

  /** Continue the conversation with a follow-up. @param {string} question */
  async function askFollowUp(question) {
    const q = (question ?? '').trim()
    if (!q || askStreaming) return
    paletteSearch = ''
    await askTurn(q)
  }

  /** @param {string} q */
  async function askTurn(q) {
    const settings = get(aiSettings)
    askError = ''
    askApi.push({ role: 'user', content: q })
    askStick = true
    askTurns = [...askTurns, { id: uid(), role: 'user', text: q }]
    askExecuted = new Set()
    askStreaming = true
    askController?.abort()
    const ac = new AbortController(); askController = ac
    try {
      await runAskLoop(0, ac, settings)
    } catch (e) {
      if (!ac.signal.aborted) askError = /** @type {any} */ (e)?.message ?? String(e)
    } finally {
      if (askController === ac) { askStreaming = false; askController = null }
    }
  }

  /** @param {number} depth @param {AbortController} ac @param {any} settings */
  async function runAskLoop(depth, ac, settings) {
    if (depth > 12 || ac.signal.aborted) return
    const sys = buildSystemPrompt(schemaContext ?? {}) + QUICK_ASK_SYS
    const asstId = uid()
    askTurns = [...askTurns, { id: asstId, role: 'assistant', text: '', streaming: true }]
    let text = ''
    /** @type {any[] | null} */
    let toolCalls = null
    for await (const chunk of chatCompletionStream(settings, [{ role: 'system', content: sys }, ...askApi], AI_TOOLS, ac.signal)) {
      if (ac.signal.aborted) return
      if (chunk.textDelta) { text += chunk.textDelta; updateAskTurn(asstId, { text }) }
      if (chunk.toolCalls) toolCalls = chunk.toolCalls
    }
    askApi.push({ role: 'assistant', content: text || '', ...(toolCalls ? { tool_calls: toolCalls } : {}) })
    const sqls = [...text.matchAll(/```sql\s*([\s\S]*?)```/gi)].map((m) => m[1].trim()).filter(Boolean)
    updateAskTurn(asstId, { text, streaming: false, sqls })
    if (toolCalls?.length && !ac.signal.aborted) {
      for (const call of toolCalls) {
        const result = await runAskTool(call, ac)
        askApi.push({ role: 'tool', tool_call_id: call.id, content: result })
      }
      await runAskLoop(depth + 1, ac, settings)
    }
  }

  /** Compact, read-only tool dispatch for quick-ask. @returns {Promise<string>} */
  async function runAskTool(call, ac) {
    const name = call.function?.name
    const key = `${name}:${call.function?.arguments}`
    if (askExecuted.has(key)) return JSON.stringify({ error: 'Duplicate call.' })
    askExecuted.add(key)
    let args = {}
    try { args = JSON.parse(call.function?.arguments || '{}') } catch {}
    const toolId = uid()
    askTurns = [...askTurns, { id: toolId, role: 'tool', status: 'running', label: name === 'execute_sql' ? 'Running query' : String(name ?? 'tool').replace(/_/g, ' ') }]
    try {
      if (name === 'execute_sql') {
        const sql = String(args.sql ?? '').trim()
        if (!sql) { updateAskTurn(toolId, { status: 'error', label: 'Empty query' }); return JSON.stringify({ error: 'Empty SQL' }) }
        if (isDestructiveSql(sql)) { updateAskTurn(toolId, { status: 'error', label: 'Write blocked here' }); return JSON.stringify({ error: 'Destructive statements are blocked in quick-ask. Put the statement in a ```sql block and tell the user to run it in the editor.' }) }
        const data = await executeSql(guardSql(sql))
        const cols = (data.columns ?? []).map((/** @type {any} */ c) => c.name ?? c)
        const rows = data.rows ?? []
        const total = data.rowCount ?? rows.length
        updateAskTurn(toolId, { status: 'done', label: `Ran query · ${total} row${total === 1 ? '' : 's'}`, result: { columns: cols, rows: rows.slice(0, 5), total } })
        return JSON.stringify({ columns: cols, rows: rows.slice(0, 30), total_rows: total })
      }
      if (name === 'list_tables') {
        updateAskTurn(toolId, { status: 'done', label: 'Listed tables' })
        return JSON.stringify({ schema: schemaContext?.activeSchema, tables: (schemaContext?.tables ?? []).map((/** @type {any} */ t) => ({ name: t.name, rowCount: t.rowCount })) })
      }
      if (name === 'describe_table' || name === 'get_schema') {
        const cols = schemaContext?.allTableColumns?.[String(args.table ?? '')] ?? null
        updateAskTurn(toolId, { status: 'done', label: 'Read schema' })
        return JSON.stringify(cols ? { table: args.table, columns: cols } : { note: 'The full schema is already provided in the system context above.' })
      }
      updateAskTurn(toolId, { status: 'done', label: 'Skipped' })
      return JSON.stringify({ error: `${name} is only available in the full chat. Answer in text and suggest continuing there.` })
    } catch (e) {
      const msg = String(e); const hint = classifyDbError(msg)
      updateAskTurn(toolId, { status: 'error', label: 'Query failed' })
      return JSON.stringify({ error: msg, ...(hint ? { hint } : {}) })
    }
  }

  /** Manually run a ```sql block from an answer, inline, showing a compact result. */
  async function runAskSql(/** @type {string} */ sql) {
    if (!sql || askStreaming) return
    if (isDestructiveSql(sql)) { open = false; onqueryselect(sql); return } // route writes to the editor
    const toolId = uid()
    askTurns = [...askTurns, { id: toolId, role: 'tool', status: 'running', label: 'Running query' }]
    try {
      const data = await executeSql(guardSql(sql))
      const cols = (data.columns ?? []).map((/** @type {any} */ c) => c.name ?? c)
      const rows = data.rows ?? []
      const total = data.rowCount ?? rows.length
      updateAskTurn(toolId, { status: 'done', label: `Ran query · ${total} row${total === 1 ? '' : 's'}`, result: { columns: cols, rows: rows.slice(0, 5), total } })
    } catch (e) {
      updateAskTurn(toolId, { status: 'error', label: 'Query failed: ' + String(e).slice(0, 80) })
    }
  }

  function stopAsk() {
    askController?.abort()
    askStreaming = false
  }

  const askBtn = 'inline-flex items-center gap-1.5 rounded-md border border-border/60 px-2 py-1 text-ui-2xs font-medium text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground'
  const askBtnPrimary = 'inline-flex items-center gap-1.5 rounded-md bg-primary px-2 py-1 text-ui-2xs font-medium text-primary-foreground transition-opacity hover:opacity-90'

  // Keep the ask thread pinned to the newest message as it streams / grows.
  // Coalesced to one adjustment per frame and gated on "near the bottom" so it
  // never fights manual scroll and doesn't jitter the view on every token.
  /** The transcript container, watched for growth. */
  let askContentEl = $state(/** @type {HTMLElement | null} */ (null))
  /** Whether to keep pinning to the bottom. False once the user scrolls away. */
  let askStick = true

  /**
   * The scrolling ancestor (the command list).
   *
   * Deliberately does NOT require the element to be overflowing yet: at the
   * moment the transcript mounts it is shorter than the list, so a check on
   * scrollHeight found nothing and the effect below bailed for good.
   * @param {HTMLElement | null} el
   */
  function _nearestScroller(el) {
    let n = el?.parentElement
    while (n) {
      const oy = getComputedStyle(n).overflowY
      if (oy === 'auto' || oy === 'scroll') return n
      n = n.parentElement
    }
    return null
  }

  // Follow the answer as it streams.
  //
  // Driven by a ResizeObserver on the transcript rather than by askTurns:
  // AiMarkdown renders on a 120ms debounce, so a measurement taken when the
  // state changed is always behind what is actually on screen. The old effect
  // scrolled to a bottom that did not exist yet, fell a little further behind on
  // every token, and once the gap passed its 120px "near the bottom" gate it
  // stopped following for the rest of the answer - which is exactly when there
  // was most left to read. Watching the box means the scroll happens after the
  // content it is scrolling to.
  $effect(() => {
    const content = askContentEl
    if (page !== 'ask' || !content) return
    const scroller = _nearestScroller(content)
    if (!scroller) return

    // A small tolerance, not a large one: this decides "is the user reading the
    // live end", and sub-pixel layout means exact equality never holds.
    const atBottom = () => scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 24
    const onScroll = () => { askStick = atBottom() }
    scroller.addEventListener('scroll', onScroll, { passive: true })

    const ro = new ResizeObserver(() => {
      if (askStick) scroller.scrollTop = scroller.scrollHeight
    })
    ro.observe(content)

    return () => {
      scroller.removeEventListener('scroll', onScroll)
      ro.disconnect()
    }
  })

  /** @param {KeyboardEvent} e */
  function handleKeydown(e) {
    // On the ask page, Enter continues the conversation with a follow-up.
    if (page === 'ask' && e.key === 'Enter' && !e.isComposing) {
      const q = paletteSearch.trim()
      if (q) { e.preventDefault(); e.stopPropagation(); void askFollowUp(q) }
      return
    }
    if (page !== 'root' && e.key === 'Backspace') {
      const input = /** @type {HTMLInputElement | null} */ (
        e.currentTarget instanceof Element
          ? e.currentTarget.querySelector('[data-slot="command-input"]')
          : null
      )
      if (!input || input.value === '') {
        e.preventDefault()
        goBack()
      }
    }
  }

  // The mouse's back button inside the palette: a sub-page (tables, pages, ask)
  // steps back to root, root closes. Same reasoning as the connection dialog -
  // the global handler bails while an overlay is open, but "back" still means
  // something in here. Capture-phase and stopped so the global one never sees
  // it, and the default is suppressed either way: unhandled, the webview walks
  // its own document history and navigates out of the app.
  $effect(() => {
    if (!open) return
    /** @param {MouseEvent} e */
    function onMouseNav(e) {
      if (e.button !== 3 && e.button !== 4) return
      e.preventDefault()
      e.stopPropagation()
      if (e.button !== 3) return
      if (page !== 'root') page = 'root'
      else open = false
    }
    /** Swallow the paired auxclick/mouseup so the webview cannot act on them. */
    function swallowAux(/** @type {MouseEvent} */ e) {
      if (e.button === 3 || e.button === 4) { e.preventDefault(); e.stopPropagation() }
    }
    document.addEventListener('mousedown', onMouseNav, { capture: true })
    document.addEventListener('auxclick', swallowAux, { capture: true })
    document.addEventListener('mouseup', swallowAux, { capture: true })
    return () => {
      document.removeEventListener('mousedown', onMouseNav, { capture: true })
      document.removeEventListener('auxclick', swallowAux, { capture: true })
      document.removeEventListener('mouseup', swallowAux, { capture: true })
    }
  })

  $effect(() => {
    if (!open) {
      // Abort any in-flight quick-ask: closing mid-stream must stop the tool
      // loop (which runs real queries), and the transcript is never shown
      // again - reopening always starts back at root.
      stopAsk()
      page = 'root'
      paletteSearch = ''  // clear search so it never persists between opens
      askTurns = []
      askApi = []
      askError = ''
    }
  })

  // Focus the search input when the dialog opens or navigates to a sub-page.
  // Command.Input is now first in DOM order so bits-ui's focus trap lands on it
  // directly. A single rAF is still used as a safety net for async rendering.
  $effect(() => {
    open  // dependency
    page  // dependency
    if (!open) return
    const id = requestAnimationFrame(() => {
      /** @type {HTMLInputElement | null} */
      const input = document.querySelector('[data-slot="command-input"] input')
      input?.focus()
    })
    return () => cancelAnimationFrame(id)
  })

  // Derived table groups - used in both the root page and the dedicated tables page
  const regularTables = $derived(tables.filter((t) => !t.tableKind || t.tableKind === 'table' || t.tableKind === 'foreign_table'))
  const viewTables = $derived(tables.filter((t) => t.tableKind === 'view'))
  const matViewTables = $derived(tables.filter((t) => t.tableKind === 'materialized_view'))

  // Search text bound from Command.Input. We filter + cap the Tables page
  // ourselves (instead of mounting every table and letting bits-ui score them
  // all on each keystroke), which keeps the rendered list small and scrolling
  // smooth even with thousands of tables.
  let paletteSearch = $state('')
  const TABLES_PAGE_CAP = 100

  // Strip word separators so "-", "_", "." and spaces are all interchangeable:
  // "product likes", "product-likes", "productlikes" all match "product_likes".
  const stripSep = (/** @type {string} */ s) => s.replace(/[\s_.\-]+/g, '')

  // Precompute per table ONCE per table-list change (not per keystroke): lowercased
  // name, separator-stripped form, word segments, and word initials (for acronym
  // matches like "pv" → product_visitor). The keystroke path then only runs the
  // integer scorer, never re-lowercasing/splitting N names - so it stays instant
  // (sub-millisecond) for thousands of tables WITHOUT a debounce.
  /** @param {{ name: string }} t */
  const prep = (t) => {
    const l = t.name.toLowerCase()
    const words = l.split(/[\s_.\-]+/).filter(Boolean)
    return { t, l, loose: stripSep(l), words, initials: words.map((w) => w[0]).join('') }
  }
  const _loweredRegular = $derived(regularTables.map(prep))
  const _loweredViews = $derived(viewTables.map(prep))
  const _loweredMatViews = $derived(matViewTables.map(prep))

  /**
   * Subsequence check with a tightness bonus: returns -1 if `q`'s chars don't all
   * appear in order in `text`, otherwise a score that's higher when the matched
   * chars are closer together (fewer gaps) and the first match is earlier.
   * @param {string} text @param {string} q (both lowercased, separator-stripped)
   */
  function subseqScore(text, q) {
    let ti = 0, qi = 0, gaps = 0, first = -1, last = -1
    while (ti < text.length && qi < q.length) {
      if (text[ti] === q[qi]) {
        if (first === -1) first = ti
        if (last >= 0) gaps += ti - last - 1
        last = ti
        qi++
      }
      ti++
    }
    if (qi < q.length) return -1
    return Math.max(0, 80 - gaps - first)
  }

  /**
   * Intuitive relevance score, separator-insensitive. Tiers (high→low):
   * exact, loose-exact, prefix, loose-prefix, word-prefix, acronym, substring,
   * loose-substring, fuzzy subsequence. 0 = no match.
   * @param {{ l: string, loose: string, words: string[], initials: string }} x
   * @param {string} q lowercased query @param {string} qLoose separator-stripped query
   */
  function scoreName(x, q, qLoose) {
    if (x.l === q) return 1000
    if (x.loose === qLoose) return 980
    if (x.l.startsWith(q)) return 940 - Math.min(x.l.length - q.length, 60)
    if (x.loose.startsWith(qLoose)) return 900 - Math.min(x.loose.length - qLoose.length, 60)
    for (const w of x.words) if (w.startsWith(q)) return 860
    if (qLoose.length >= 2) {
      if (x.initials.startsWith(qLoose)) return 840
      if (x.initials.includes(qLoose)) return 800
    }
    const sub = x.l.indexOf(q)
    if (sub !== -1) return 760 - Math.min(sub, 60)
    const lsub = x.loose.indexOf(qLoose)
    if (lsub !== -1) return 700 - Math.min(lsub, 60)
    const ss = subseqScore(x.loose, qLoose)
    return ss >= 0 ? 400 + ss : 0
  }

  /** @param {ReturnType<typeof prep>[]} prepped */
  function filterAndCap(prepped) {
    const q = paletteSearch.trim().toLowerCase()
    if (!q) return { items: prepped.slice(0, TABLES_PAGE_CAP).map((x) => x.t), total: prepped.length }
    const qLoose = stripSep(q)
    /** @type {{ t: { name: string }, s: number, len: number }[]} */
    const scored = []
    for (const x of prepped) {
      const s = scoreName(x, q, qLoose)
      if (s > 0) scored.push({ t: x.t, s, len: x.l.length })
    }
    // Higher score first; ties broken by shorter name (more relevant) then A-Z.
    scored.sort((a, b) => b.s - a.s || a.len - b.len || (a.t.name < b.t.name ? -1 : 1))
    return { items: scored.slice(0, TABLES_PAGE_CAP).map((x) => x.t), total: scored.length }
  }

  const tablesPageRegular = $derived(filterAndCap(_loweredRegular))
  const tablesPageViews = $derived(filterAndCap(_loweredViews))
  const tablesPageMatViews = $derived(filterAndCap(_loweredMatViews))

  // On tables page: shouldFilter=false so bits-ui skips its sort/filter pass entirely
  // (no CSS `order` reordering, no keyboard-nav interference). We pre-filter in JS.
  // On root page: bits-ui filters with a plain substring check against item value strings.
  const shouldFilter = $derived(page !== 'tables' && page !== 'ask')

  /** @type {(value: string, search: string) => number} */
  const commandFilter = (value, search) => {
    if (!search) return 1
    const v = value.toLowerCase(), s = search.toLowerCase()
    if (v.includes(s)) return 1
    // Separator-insensitive fallback so "newtable"/"new-table" match "new table".
    return stripSep(v).includes(stripSep(s)) ? 1 : 0
  }

  /** @param {() => void} action */
  function run(action) {
    open = false
    action()
  }

  /** @param {'postgres'|'sqlite'|'d1'} type */
  function driverIcon(type) {
    if (type === 'sqlite') return 'hard-drive'
    if (type === 'd1')     return 'cloud'
    return 'database'
  }

  /** @param {import('$lib/stores/connections.js').SavedConnection} conn */
  function connSubtitle(conn) {
    if (conn.type === 'sqlite') return conn.filePath ?? ''
    if (conn.type === 'd1')     return `${conn.accountId?.slice(0, 8) ?? ''}… / ${conn.databaseId?.slice(0, 8) ?? ''}…`
    return `${conn.user ?? ''}@${conn.host ?? ''}:${conn.port ?? ''}/${conn.database ?? ''}`
  }

  const pageLabel = /** @type {Record<string, string>} */ ({
    docker: 'Docker',
    connections: 'Connections',
    tables: 'Tables',
    ask: 'Ask AI',
    pages: 'Go to page',
  })
</script>

<Command.Dialog
  bind:open
  filter={commandFilter}
  shouldFilter={shouldFilter}
  title="Command menu"
  description="Search tables, schemas, and commands"
  class={page === 'ask'
    ? 'w-[min(51.25rem,calc(100vw-3rem))] sm:max-w-none'
    : 'w-[min(33.75rem,calc(100vw-2rem))] sm:max-w-none'}
>
  {#snippet children()}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- Command.Input is FIRST in DOM so bits-ui's focus trap always lands on it.
         The breadcrumb is rendered after but floated to the top visually via order-first,
         keeping it above the input without disrupting the focus-trap scan order. -->
    <div class="flex flex-col" onkeydown={handleKeydown}>
      <Command.Input
        bind:value={paletteSearch}
        oninput={(/** @type {Event} */ e) => { paletteSearch = /** @type {HTMLInputElement} */ (e.currentTarget).value }}
        placeholder={
          page === 'root' ? 'Search tables, schemas, commands…'
          : page === 'tables' ? 'Search tables and views…'
          : page === 'ask' ? 'Ask a follow-up…'
          : page === 'pages' ? 'Go to page…'
          : `Search ${pageLabel[page]}…`
        }
      />

      {#if page !== 'root'}
        <div class="order-first flex items-center gap-1.5 border-b border-border/50 px-4 py-2">
          <button
            type="button"
            class="inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-ui-2xs text-muted-foreground transition-colors hover:bg-muted/40 hover:text-foreground focus-visible:outline-none"
            onclick={goBack}
          >
            <Icon name="chevron-left" class="size-3" />
            Back
          </button>
          <span class="text-muted-foreground text-ui-2xs">/</span>
          <span class="text-ui-2xs font-medium text-foreground/80">{pageLabel[page]}</span>
        </div>
      {/if}

      <!-- Taller on the Ask AI page: a transcript scrolled inside 440px shows about
           one exchange at a time, so answers and their result tables get clipped. -->
      <Command.List class={page === 'ask' ? 'h-[min(640px,74vh)] max-h-[min(640px,74vh)]' : 'max-h-[min(440px,58vh)]'}>
        {#if page !== 'ask'}
          <Command.Empty class="py-8 text-center text-ui-xs text-muted-foreground">No results.</Command.Empty>
        {/if}

        <!-- Shared page-destination row (root "Views" + "Go to page" navigator) -->
        {#snippet pageRow(/** @type {typeof pageItems[number]} */ it)}
          <Command.Item value={it.value} onSelect={() => run(it.action)}>
            <Icon name={it.icon} class="size-4 shrink-0 opacity-60" />
            <span data-slot="command-label" class="truncate">{it.label}</span>
            {#if it.keys}<Command.Shortcut keys={it.keys} />{/if}
          </Command.Item>
        {/snippet}

        <!-- ── ASK AI (inline quick-ask) ─────────────────────────────── -->
        {#if page === 'ask'}
          <div bind:this={askContentEl} class="select-text px-3 py-2.5">
            {#each askTurns as turn (turn.id)}
              {#if turn.role === 'user'}
                <div class="mb-1.5 flex items-start gap-2">
                  <Icon name="sparkles" class="mt-0.5 size-3.5 shrink-0 text-primary" />
                  <div class="min-w-0 flex-1 text-ui-sm font-medium text-foreground">{turn.text}</div>
                </div>
              {:else if turn.role === 'tool'}
                <div class="mb-1.5 ml-[22px] flex flex-col gap-1">
                  <div class="flex items-center gap-1.5 text-ui-2xs {turn.status === 'error' ? 'text-destructive' : 'text-muted-foreground'}">
                    {#if turn.status === 'running'}<Icon name="sparkles" class="size-3 animate-pulse" />
                    {:else if turn.status === 'error'}<Icon name="x" class="size-3" />
                    {:else}<Icon name="check" class="size-3 text-success" />{/if}
                    <span class="truncate">{turn.label}</span>
                  </div>
                  {#if turn.result && turn.result.columns.length}
                    <div class="overflow-hidden rounded-lg border border-border/50 bg-card/40">
                      <div class="overflow-x-auto">
                        <table class="w-full border-collapse">
                          <thead>
                            <tr class="bg-muted/25">
                              {#each turn.result.columns.slice(0, 5) as c}
                                <th class="whitespace-nowrap border-b border-border/40 px-2.5 py-1 text-left text-ui-3xs font-medium uppercase tracking-wide text-muted-foreground">{c}</th>
                              {/each}
                            </tr>
                          </thead>
                          <tbody>
                            {#each turn.result.rows as row, ri}
                              <tr class="transition-colors hover:bg-muted/15">
                                {#each row.slice(0, 5) as cell}
                                  <td class="max-w-[220px] truncate {ri < turn.result.rows.length - 1 ? 'border-b border-border/15' : ''} px-2.5 py-1 font-mono text-ui-2xs {cell === null ? 'italic text-muted-foreground' : 'text-foreground/80'}">{cell === null ? 'NULL' : String(cell)}</td>
                                {/each}
                              </tr>
                            {/each}
                          </tbody>
                        </table>
                      </div>
                      {#if turn.result.total > turn.result.rows.length}
                        <div class="border-t border-border/30 bg-muted/10 px-2.5 py-1 text-ui-3xs text-muted-foreground">showing {turn.result.rows.length} of {turn.result.total} rows</div>
                      {/if}
                    </div>
                  {/if}
                </div>
              {:else}
                <div class="mb-2 ml-[22px]">
                  {#if turn.streaming && !turn.text}
                    <div class="flex items-center gap-2 py-1 text-ui-xs text-muted-foreground"><Icon name="sparkles" class="size-3.5 animate-pulse" /> Thinking…</div>
                  {:else}
                    <AiMarkdown content={turn.text} streaming={turn.streaming} debounceMs={120} class="text-ui-xs" />
                  {/if}
                  {#if !turn.streaming && turn.sqls?.length}
                    <div class="mt-1.5 flex flex-wrap gap-1.5">
                      {#each turn.sqls as s, i}
                        <button type="button" class={askBtnPrimary} onclick={() => void runAskSql(s)}>
                          <Icon name="terminal" class="size-3" /> Run{turn.sqls.length > 1 ? ` #${i + 1}` : ''}
                        </button>
                        <button type="button" class={askBtn} onclick={() => { const sql = s; open = false; onqueryselect(sql) }}>
                          Insert
                        </button>
                      {/each}
                    </div>
                  {/if}
                </div>
              {/if}
            {/each}
            {#if askError}
              <div class="ml-[22px] rounded-md border border-destructive/30 bg-destructive/5 px-2.5 py-2 text-ui-xs text-destructive">{askError}</div>
            {/if}
            <div class="ml-[22px] mt-2 flex flex-wrap items-center gap-1.5">
              {#if askStreaming}
                <button type="button" class={askBtn} onclick={stopAsk}><Icon name="square" class="size-3" /> Stop</button>
              {:else if askTurns.length}
                <button type="button" class={askBtn} onclick={() => navigator.clipboard?.writeText([...askTurns].reverse().find((t) => t.role === 'assistant')?.text ?? '')}><Icon name="copy" class="size-3" /> Copy</button>
                <button type="button" class={askBtn} onclick={() => { const q = lastAskQuestion; open = false; onaskcontinue(q) }}><Icon name="bot" class="size-3" /> Continue in chat</button>
              {/if}
            </div>
            <div class="ml-[22px] mt-1.5 text-ui-3xs text-muted-foreground">Type below and press ↵ to follow up</div>
          </div>
        {/if}

        <!-- ── ROOT PAGE ─────────────────────────────────────────────── -->
        {#if page === 'root'}
          {#if paletteSearch.trim() && $appCmdkAi}
            <Command.Group heading="Ask">
              <Command.Item value={"ask ai " + paletteSearch} onSelect={() => startAsk(paletteSearch)}>
                <Icon name="sparkles" class="size-4 shrink-0 text-primary" />
                <span data-slot="command-label" class="truncate">Ask AI: <span class="text-muted-foreground">"{paletteSearch}"</span></span>
                <Command.Shortcut keys="↵" />
              </Command.Item>
            </Command.Group>
          {/if}

          {#if connected}
            <Command.Group heading="Views">
              {#each pageItems as it (it.label)}
                {@render pageRow(it)}
              {/each}
            </Command.Group>

            {#if schemas.length > 0}
              <Command.Group heading="Schemas">
                {#each schemas as schema (schema)}
                  <Command.Item value="schema {schema}" onSelect={() => run(() => onschemachange(schema))}>
                    <Icon name="database" class="size-4 shrink-0 opacity-60" />
                    <span data-slot="command-label" class="truncate font-mono">{schema}</span>
                    {#if schema === activeSchema}
                      <span data-slot="command-trailing" class="shrink-0 text-ui-xs text-muted-foreground">current</span>
                    {/if}
                  </Command.Item>
                {/each}
              </Command.Group>
            {/if}

            {#if tables.length > 0}
              <Command.Group heading="Tables">
                {#each regularTables.slice(0, 8) as table (table.name)}
                  <Command.Item value="table {activeSchema} {table.name}" onSelect={() => run(() => ontableselect(table.name))}>
                    <Icon name="table-2" class="size-4 shrink-0 opacity-60" />
                    <span data-slot="command-label" class="truncate font-mono">{table.name}</span>
                    <span data-slot="command-trailing" class="shrink-0 font-mono text-ui-xs tabular-nums text-muted-foreground">{formatTableRowCount(table.rowCount)}</span>
                  </Command.Item>
                {/each}
                {#each viewTables.slice(0, 4) as table (table.name)}
                  <Command.Item value="view {activeSchema} {table.name}" onSelect={() => run(() => ontableselect(table.name))}>
                    <Icon name="eye" class="size-4 shrink-0 opacity-60" />
                    <span data-slot="command-label" class="truncate font-mono">{table.name}</span>
                    <span data-slot="command-trailing" class="shrink-0 text-ui-xs text-muted-foreground">view</span>
                  </Command.Item>
                {/each}
                {#if tables.length > 12}
                  <Command.Item value="browse all tables views search" onSelect={() => navigate('tables')}>
                    <Icon name="table-2" class="size-4 shrink-0 opacity-40" />
                    <span data-slot="command-label" class="truncate text-muted-foreground">All {tables.length} tables & views…</span>
                    <Icon name="chevron-right" class="size-3.5 shrink-0 text-muted-foreground" />
                  </Command.Item>
                {/if}
              </Command.Group>
            {/if}

            <Command.Group heading="AI">
              <Command.Item value="ask ai assistant chat query" onSelect={() => run(onopenai)}>
                <Icon name="bot" class="size-4 shrink-0 opacity-60" />
                <span data-slot="command-label" class="truncate">Ask AI</span>
                <Command.Shortcut keys="⌘⇧E" />
              </Command.Item>
              <Command.Item value="toggle ai sidebar inline assistant context" onSelect={() => run(onopenaisidebar)}>
                <Icon name="sparkles" class="size-4 shrink-0 opacity-60" />
                <span data-slot="command-label" class="truncate">AI sidebar</span>
                <Command.Shortcut keys="⌘I" />
              </Command.Item>
              <Command.Item
                value={aiMode ? "close ai panel hide assistant" : "open ai panel show assistant chat"}
                onSelect={() => run(ontoggleaimode)}
              >
                <Icon name="arrow-left-right" class="size-4 shrink-0 opacity-60" />
                <span data-slot="command-label" class="truncate">{aiMode ? 'Close AI panel' : 'Open AI panel'}</span>
              </Command.Item>
            </Command.Group>

            <Command.Group heading="Queries">
              <Command.Item value="open query history sql statements" onSelect={() => run(onopenqueryhistory)}>
                <Icon name="history" class="size-4 shrink-0 opacity-60" />
                <span data-slot="command-label" class="truncate">Query history</span>
              </Command.Item>
            </Command.Group>

            {#if savedQueries.length > 0}
              <Command.Group heading="Saved queries">
                {#each savedQueries as entry (entry.id)}
                  <Command.Item value="saved query {entry.name} {entry.sql}" onSelect={() => run(() => onqueryselect(entry.sql))}>
                    <Icon name="bookmark" class="size-4 shrink-0 opacity-60" />
                    <span data-slot="command-label" class="min-w-0 truncate font-mono text-ui-xs">{entry.name}</span>
                  </Command.Item>
                {/each}
              </Command.Group>
            {/if}

            {#if queryHistory.length > 0}
              <Command.Group heading="Recent queries">
                {#each queryHistory.slice(0, 20) as entry (entry.id)}
                  <Command.Item value="recent query {entry.title} {entry.sql}" onSelect={() => run(() => onqueryselect(entry.sql))}>
                    <Icon name="history" class="size-4 shrink-0 opacity-60" />
                    <span data-slot="command-label" class="min-w-0 truncate font-mono text-ui-xs">{entry.title}</span>
                  </Command.Item>
                {/each}
              </Command.Group>
            {/if}

            <Command.Group heading="Actions">
              <Command.Item value="refresh schema tables" onSelect={() => run(onrefresh)}>
                <Icon name="refresh-cw" class="size-4 shrink-0 opacity-60" />
                <span data-slot="command-label" class="truncate">Refresh tables</span>
              </Command.Item>
              <Command.Item value="read only read-only lock mode protect prevent edits writes inserts deletes writable" onSelect={() => run(onreadonlytoggle)}>
                <Icon name={readonly ? 'lock-open' : 'lock'} class="size-4 shrink-0 opacity-60" />
                <span data-slot="command-label" class="truncate">{readonly ? 'Disable read-only mode' : 'Enable read-only mode'}</span>
              </Command.Item>
              <Command.Item value="open settings preferences" onSelect={() => run(onopensettings)}>
                <Icon name="settings" class="size-4 shrink-0 opacity-60" />
                <span data-slot="command-label" class="truncate">Settings</span>
              </Command.Item>
              {#if $pinEnabled}
                <Command.Item value="lock app pin screen secure" onSelect={() => run(lockNow)}>
                  <Icon name="lock" class="size-4 shrink-0 opacity-60" />
                  <span data-slot="command-label" class="truncate">Lock Stroke</span>
                </Command.Item>
              {/if}
              <Command.Item value="keyboard shortcuts keybindings hotkeys help" onSelect={() => run(onopenshortcuts)}>
                <Icon name="keyboard" class="size-4 shrink-0 opacity-60" />
                <span data-slot="command-label" class="truncate">Keyboard shortcuts</span>
                <Command.Shortcut keys="?" />
              </Command.Item>
              <Command.Item value="about license version info app" onSelect={() => run(onopenabout)}>
                <Icon name="info" class="size-4 shrink-0 opacity-60" />
                <span data-slot="command-label" class="truncate">About Stroke</span>
              </Command.Item>
              <Command.Item value="report issue bug crash problem feedback github" onSelect={() => run(onopenreport)}>
                <Icon name="bug" class="size-4 shrink-0 opacity-60" />
                <span data-slot="command-label" class="truncate">Report an issue</span>
              </Command.Item>
              <Command.Item value="check for updates upgrade version" onSelect={() => run(oncheckupdate)}>
                <Icon name="arrow-down-to-line" class="size-4 shrink-0 opacity-60" />
                <span data-slot="command-label" class="truncate">Check for updates</span>
              </Command.Item>
              <Command.Item value="disconnect database" onSelect={() => run(ondisconnect)}>
                <Icon name="unplug" class="size-4 shrink-0 opacity-60" />
                <span data-slot="command-label" class="truncate">Disconnect</span>
              </Command.Item>
            </Command.Group>
          {/if}

          <!-- ── Switch database, other saved connections, right at root ── -->
          {@const otherConns = savedConnections.filter((c) => c.id !== activeConnectionId)}
          {#if otherConns.length > 0}
            <Command.Group heading="Switch database">
              {#each otherConns.slice(0, 8) as conn (conn.id)}
                <Command.Item
                  value="switch database connection {conn.name} {connSubtitle(conn)} {conn.type}"
                  onSelect={() => run(() => onswitchdatabase(conn))}
                >
                  <Icon name={driverIcon(conn.type ?? 'postgres')} class="size-4 shrink-0 opacity-60" />
                  <div data-slot="command-label" class="flex min-w-0 flex-1 flex-col">
                    <span class="truncate">{conn.name}</span>
                    <span class="truncate font-mono text-ui-2xs text-muted-foreground">{connSubtitle(conn)}</span>
                  </div>
                  {#if savedConnections.indexOf(conn) < 9}
                    <Command.Shortcut keys="⌘⌥{savedConnections.indexOf(conn) + 1}" />
                  {/if}
                </Command.Item>
              {/each}
            </Command.Group>
          {/if}

          <!-- ── Drill-in: Connections ───────────────────────────────── -->
          <Command.Group heading="Database">
            <Command.Item
              value="connections switch database connect postgres mysql sqlite saved {savedConnections.map(c => c.name).join(' ')}"
              onSelect={() => navigate('connections')}
            >
              <Icon name="database" class="size-4 shrink-0 opacity-60" />
              <span data-slot="command-label" class="truncate">Connections</span>
              {#if savedConnections.length > 0}
                <span class="shrink-0 font-mono text-ui-xs text-muted-foreground">{savedConnections.length}</span>
              {/if}
              <Icon name="chevron-right" class="size-3.5 shrink-0 text-muted-foreground" />
            </Command.Item>
          </Command.Group>

          <!-- ── Drill-in: Docker ───────────────────────────────────── -->
          <Command.Group heading="Launch">
            <Command.Item value="docker containers postgresql mysql launch run" onSelect={() => navigate('docker')}>
              <Icon name="package" class="size-4 shrink-0 opacity-60" />
              <span data-slot="command-label" class="truncate">Docker</span>
              <Icon name="chevron-right" class="size-3.5 shrink-0 text-muted-foreground" />
            </Command.Item>
          </Command.Group>

        <!-- ── TABLES PAGE ───────────────────────────────────────────── -->
        {:else if page === 'tables'}
          {#if tablesPageRegular.total > 0}
            <Command.Group heading="Tables">
              {#each tablesPageRegular.items as table (table.name)}
                <Command.Item value="table {activeSchema} {table.name} {table.name}" onSelect={() => run(() => ontableselect(table.name))}>
                  <Icon name="table-2" class="size-4 shrink-0 opacity-60" />
                  <span data-slot="command-label" class="truncate font-mono">{table.name}</span>
                  <span
                    data-slot="command-trailing"
                    class="shrink-0 font-mono text-ui-xs tabular-nums text-muted-foreground"
                    title={table.rowCount != null ? Number(table.rowCount).toLocaleString('en-US') : undefined}
                  >{formatTableRowCount(table.rowCount)}</span>
                </Command.Item>
              {/each}
              {#if tablesPageRegular.total > tablesPageRegular.items.length}
                <div class="px-2.5 py-1.5 text-ui-2xs text-muted-foreground">
                  Showing {tablesPageRegular.items.length} of {tablesPageRegular.total}, keep typing to narrow.
                </div>
              {/if}
            </Command.Group>
          {/if}
          {#if tablesPageViews.total > 0}
            <Command.Group heading="Views">
              {#each tablesPageViews.items as table (table.name)}
                <Command.Item value="view {activeSchema} {table.name} {table.name}" onSelect={() => run(() => ontableselect(table.name))}>
                  <Icon name="eye" class="size-4 shrink-0 opacity-60" />
                  <span data-slot="command-label" class="truncate font-mono">{table.name}</span>
                </Command.Item>
              {/each}
              {#if tablesPageViews.total > tablesPageViews.items.length}
                <div class="px-2.5 py-1.5 text-ui-2xs text-muted-foreground">
                  Showing {tablesPageViews.items.length} of {tablesPageViews.total}, keep typing to narrow.
                </div>
              {/if}
            </Command.Group>
          {/if}
          {#if tablesPageMatViews.total > 0}
            <Command.Group heading="Materialized views">
              {#each tablesPageMatViews.items as table (table.name)}
                <Command.Item value="materialized view {activeSchema} {table.name} {table.name}" onSelect={() => run(() => ontableselect(table.name))}>
                  <Icon name="eye" class="size-4 shrink-0 opacity-60" />
                  <span data-slot="command-label" class="truncate font-mono">{table.name}</span>
                  <span
                    data-slot="command-trailing"
                    class="shrink-0 font-mono text-ui-xs tabular-nums text-muted-foreground"
                  >{formatTableRowCount(table.rowCount)}</span>
                </Command.Item>
              {/each}
              {#if tablesPageMatViews.total > tablesPageMatViews.items.length}
                <div class="px-2.5 py-1.5 text-ui-2xs text-muted-foreground">
                  Showing {tablesPageMatViews.items.length} of {tablesPageMatViews.total}, keep typing to narrow.
                </div>
              {/if}
            </Command.Group>
          {/if}

        <!-- ── DOCKER PAGE ────────────────────────────────────────────── -->
        {:else if page === 'docker'}
          <Command.Group heading="Docker containers">
            <Command.Item
              value="launch postgresql postgres container pull run 5433"
              onSelect={() => run(() => ondockerlaunch('postgres'))}
            >
              <Icon name="package" class="size-4 shrink-0 opacity-60" />
              <span data-slot="command-label" class="truncate">PostgreSQL container</span>
              <span data-slot="command-trailing" class="shrink-0 font-mono text-ui-2xs text-muted-foreground">:5433</span>
            </Command.Item>
            <Command.Item
              value="launch mysql container pull run 3307"
              onSelect={() => run(() => ondockerlaunch('mysql'))}
            >
              <Icon name="package" class="size-4 shrink-0 opacity-60" />
              <span data-slot="command-label" class="truncate">MySQL container</span>
              <span data-slot="command-trailing" class="shrink-0 font-mono text-ui-2xs text-muted-foreground">:3307</span>
            </Command.Item>
          </Command.Group>

        <!-- ── CONNECTIONS PAGE ───────────────────────────────────────── -->
        {:else if page === 'connections'}
          {#if savedConnections.length > 0}
            <Command.Group heading="Saved connections">
              {#each savedConnections as conn (conn.id)}
                {@const driverIconName = driverIcon(conn.type ?? 'postgres')}
                {@const isActive = conn.id === activeConnectionId}
                <Command.Item
                  value="connection {conn.name} {connSubtitle(conn)} {conn.type}"
                  onSelect={() => run(() => onswitchdatabase(conn))}
                  disabled={isActive}
                >
                  <Icon name={driverIconName} class="size-4 shrink-0 opacity-60" />
                  <div data-slot="command-label" class="flex min-w-0 flex-1 flex-col">
                    <span class="truncate">{conn.name}</span>
                    <span class="truncate font-mono text-ui-2xs text-muted-foreground">{connSubtitle(conn)}</span>
                  </div>
                  {#if isActive}
                    <span data-slot="command-trailing" class="shrink-0 text-ui-xs text-muted-foreground">connected</span>
                  {/if}
                </Command.Item>
              {/each}
            </Command.Group>
          {/if}
          <Command.Group heading="Add">
            <Command.Item value="new connection add connect database" onSelect={() => run(onopenconnection)}>
              <Icon name="database" class="size-4 shrink-0 opacity-60" />
              <span data-slot="command-label" class="truncate">New connection…</span>
            </Command.Item>
          </Command.Group>

        {:else if page === 'pages'}
          <!-- Go to page, VSCode-style focused navigator (Ctrl/⌘+P) -->
          <Command.Group heading="Go to page">
            {#each pageItems as it (it.label)}
              {@render pageRow(it)}
            {/each}
          </Command.Group>
        {/if}

      </Command.List>
    </div>
  {/snippet}
</Command.Dialog>
