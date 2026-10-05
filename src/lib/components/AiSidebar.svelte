<script>
  import { comboTitle } from '$lib/shortcuts.js'
  import { tick, onMount, onDestroy } from "svelte";
  import { getAppScale } from '$lib/app-zoom.js';
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Loader2 from "@lucide/svelte/icons/loader-2";
  import Send from "@lucide/svelte/icons/send";
  import Square from "@lucide/svelte/icons/square";
  import Play from "@lucide/svelte/icons/play";
  import Copy from "@lucide/svelte/icons/copy";
  import CornerDownLeft from "@lucide/svelte/icons/corner-down-left";
  import AlertTriangle from "@lucide/svelte/icons/alert-triangle";
  import Table2 from "@lucide/svelte/icons/table-2";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import X from "@lucide/svelte/icons/x";
  import History from "@lucide/svelte/icons/history";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import At from "@lucide/svelte/icons/at-sign";
  import Slash from "@lucide/svelte/icons/slash";
  import SquarePen from "@lucide/svelte/icons/square-pen";
  import MessageSquareText from "@lucide/svelte/icons/message-square-text";
  import Gauge from "@lucide/svelte/icons/gauge";
  import Network from "@lucide/svelte/icons/network";
  import Rows3 from "@lucide/svelte/icons/rows-3";
  import FileText from "@lucide/svelte/icons/file-text";
  import CodeXml from "@lucide/svelte/icons/code-xml";
  import { cn } from "$lib/utils.js";
  import { executeSql } from "$lib/api.js";
  import { isReadOnly } from '$lib/stores/read-only.js'
  import { isWriteSql } from '$lib/sql-write.js'
  import DataTable from "$lib/components/DataTable.svelte";
  import AiMarkdown from "$lib/components/AiMarkdown.svelte";
  import AiSqlBlock from "$lib/components/AiSqlBlock.svelte";
  import ShikiBlock from "$lib/components/ShikiBlock.svelte";
  import AiChartRenderer from "$lib/components/AiChartRenderer.svelte";
  import AiModelPicker from "$lib/components/AiModelPicker.svelte";
  import ResizeHandle from "$lib/components/ResizeHandle.svelte";
  import {
    chatCompletionStream,
    manageHistory,
    MAX_AI_RETRIES,
    AI_TOOLS,
    isDestructiveSql,
    parseAssistantMessage,
    buildSystemPrompt,
    classifyDbError,
    filterSchemaForQuery,
  } from "$lib/ai.js";
  import {
    aiSettings,
    isAiConfigured,
  } from "$lib/stores/ai-settings.js";
  import { loadSkills } from "$lib/stores/ai-skills.js";
  import {
    clampAiSidebarWidth,
    loadLayout,
    saveLayout,
  } from "$lib/stores/layout.js";
  import { formatCompactCount } from "$lib/table-list.js";
  import {
    listConversations,
    createConversation,
    updateConversation,
    deleteConversation,
    clearConversations,
  } from "$lib/stores/conversations.js";

  /**
   * @typedef {
   *   | { id: string, kind: 'user', text: string, sql?: string }
   *   | { id: string, kind: 'assistant', parts: import('$lib/ai.js').AssistantPart[] }
   *   | { id: string, kind: 'streaming' }
   *   | { id: string, kind: 'result', sql: string, columns: {name:string,dataType?:string}[], rows: unknown[][], total: number, error: string|null, isSchema?: boolean, capped?: boolean }
   *   | { id: string, kind: 'chart', spec: { type: string, title: string, data: object[], x_key: string, y_keys: {key:string,label:string}[] }, error: string|null }
   *   | { id: string, kind: 'confirm', sql: string, resolve: (ok: boolean) => void }
   *   | { id: string, kind: 'thinking' }
   *   | { id: string, kind: 'executing', sql: string }
   * } ChatItem
   */

  let {
    schemaContext = /** @type {any} */ ({
      schemas: [],
      activeSchema: "public",
      tables: [],
      activeTable: null,
      columns: [],
      primaryKey: [],
      foreignKeys: [],
    }),
    connectionId = "",
    isActive = false,
    currentView = /** @type {'table' | 'sql' | 'orm' | 'schema' | 'welcome' | string} */ ("table"),
    currentSql = "",
    currentCode = "",
    ormMode = /** @type {'drizzle' | 'prisma'} */ ("drizzle"),
    onclose = () => {},
    /** @param {{ kind: 'sql' | 'code', lang?: string, content: string }} detail */
    onaccept = (detail) => {},
    onopensettings = () => {},
  } = $props();

  const uid = () => crypto.randomUUID();
  const configured = $derived(isAiConfigured($aiSettings));

  // ── Chat state ───────────────────────────────────────────────────────────
  /** @type {ChatItem[]} */
  let items = $state([]);
  /** @type {import('$lib/ai.js').ApiMessage[]} */
  let apiHistory = $state([]);
  let rawApiHistory = $state([]);
  let loading = $state(false);
  let error = $state("");
  let aiStatusHint = $state("");
  let inputText = $state("");
  let turnSystemPrompt = "";
  let executedCalls = new Set();
  /** @type {Map<string, { count: number, lastError: string }>} */
  let failureTracker = new Map();
  /** @type {Record<string, {name:string, dataType:string, nullable:boolean}[]>} */
  let fetchedSchemas = $state({});
  /** @type {Set<string>} */
  let collapsed = $state(new Set());
  /** @type {string | null} */
  let openResultId = $state(null);

  /** @type {HTMLTextAreaElement | null} */
  let inputRef = $state(null);
  /** @type {HTMLDivElement | null} */
  let scrollEl = $state(null);
  let userScrolledUp = $state(false);

  // ── Thinking cycling (matches AiChat) ────────────────────────────────────
  const THINKING_PHRASES = [
    'Thinking…', 'Analyzing schema…', 'Writing the query…',
    'Reading the data…', 'Checking relationships…', 'Running the numbers…',
    'Exploring tables…', 'Crafting response…', 'Almost there…',
  ]
  let thinkingPhrase = $state(THINKING_PHRASES[0])
  let thinkingVisible = $state(true)

  $effect(() => {
    if (!loading) { thinkingPhrase = THINKING_PHRASES[0]; thinkingVisible = true; return }
    let i = 0
    let fadeId = 0
    const tick2 = () => {
      thinkingVisible = false
      fadeId = setTimeout(() => { fadeId = 0; i = (i + 1) % THINKING_PHRASES.length; thinkingPhrase = THINKING_PHRASES[i]; thinkingVisible = true }, 220)
    }
    const id = setInterval(tick2, 2600)
    return () => { clearInterval(id); clearTimeout(fadeId) }
  })

  // ── @ mention system ──────────────────────────────────────────────────────
  let mentionOpen = $state(false)
  let mentionQuery = $state('')
  let mentionStart = $state(0)
  let mentionIdx = $state(0)
  /** Tables pinned via @ - shown as removable badges above the input and folded
   *  into the next message's context. */
  let contextTables = $state(/** @type {string[]} */ ([]))

  const allTables = $derived(schemaContext.tables ?? [])

  const mentionItems = $derived.by(() => {
    const q = mentionQuery.toLowerCase()
    const tables = q
      ? allTables.filter(t => t.name.toLowerCase().includes(q))
      : allTables
    return tables.slice(0, 8).map(t => ({
      label: t.name,
      sub: schemaContext.activeSchema,
      insert: `${schemaContext.activeSchema}.${t.name}`,
    }))
  })

  $effect(() => { mentionIdx = 0 })

  function handleInputKeydown(/** @type {KeyboardEvent} */ e) {
    if (slashOpen) {
      if (e.key === 'ArrowDown') { e.preventDefault(); slashIdx = (slashIdx + 1) % Math.max(1, slashItems.length); return }
      if (e.key === 'ArrowUp')   { e.preventDefault(); slashIdx = (slashIdx - 1 + Math.max(1, slashItems.length)) % Math.max(1, slashItems.length); return }
      if (e.key === 'Enter' || e.key === 'Tab') { e.preventDefault(); const it = slashItems[slashIdx]; if (it) runSlash(it); else slashOpen = false; return }
      if (e.key === 'Escape') { slashOpen = false; return }
    }
    if (mentionOpen) {
      if (e.key === 'ArrowDown') { e.preventDefault(); mentionIdx = (mentionIdx + 1) % Math.max(1, mentionItems.length); return }
      if (e.key === 'ArrowUp')   { e.preventDefault(); mentionIdx = (mentionIdx - 1 + Math.max(1, mentionItems.length)) % Math.max(1, mentionItems.length); return }
      if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault()
        const item = mentionItems[mentionIdx]
        if (item) insertMention(item.insert)
        else mentionOpen = false
        return
      }
      if (e.key === 'Escape') { mentionOpen = false; return }
    }
    // Backspace in an empty box takes the attached statement off, as with a chip.
    if (e.key === 'Backspace' && attachedSql && !inputText) { e.preventDefault(); attachedSql = ''; return }
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); void send(); return }
  }

  function handleInputChange(/** @type {Event} */ e) {
    const el = /** @type {HTMLTextAreaElement} */ (e.target)
    inputText = el.value
    resizeInput()
    // Slash command menu - only when the input starts with "/" (no space yet).
    if (inputText.startsWith('/') && !/\s/.test(inputText)) {
      slashQuery = inputText.slice(1)
      slashOpen = true
      mentionOpen = false
      return
    }
    slashOpen = false
    // Detect @ trigger
    const cursor = el.selectionStart ?? 0
    const before = inputText.slice(0, cursor)
    const atIdx = before.lastIndexOf('@')
    if (atIdx >= 0) {
      const after = before.slice(atIdx + 1)
      if (!/\s/.test(after)) {
        mentionQuery = after
        mentionStart = atIdx
        mentionOpen = true
        return
      }
    }
    mentionOpen = false
  }

  function insertMention(/** @type {string} */ text) {
    // Drop the "@query" from the textarea and pin the table as a badge instead.
    const before = inputText.slice(0, mentionStart)
    const afterAt = inputText.slice(mentionStart + 1 + mentionQuery.length)
    inputText = (before + afterAt).replace(/^\s+/, '')
    if (!contextTables.includes(text)) contextTables = [...contextTables, text]
    mentionOpen = false
    void tick().then(() => { resizeInput(); inputRef?.focus() })
  }

  // ── Slash commands (quick prompts) ──────────────────────────────────────────
  let slashOpen = $state(false)
  let slashQuery = $state('')
  let slashIdx = $state(0)

  const SLASH_COMMANDS = [
    { cmd: 'explain',  label: 'Explain',    desc: 'Explain the current table or SQL',    prompt: 'Explain the currently open table (or the SQL in the editor if one is open): its purpose, key columns, and relationships. Be concise.', send: true },
    { cmd: 'optimize', label: 'Optimize',   desc: 'Suggest query optimizations',         prompt: 'Review the SQL currently in the editor and suggest concrete optimizations (indexes, rewrites, avoiding scans). Return improved SQL in a ```sql block.', send: true },
    { cmd: 'fix',      label: 'Fix',        desc: 'Find and fix issues in the SQL',       prompt: 'Find problems in the SQL currently in the editor and return corrected SQL in a ```sql block with a brief explanation.', send: true },
    { cmd: 'index',    label: 'Indexes',    desc: 'Suggest indexes for the active table', prompt: "Based on the active table's columns, primary key, and foreign keys, suggest useful indexes and give the CREATE INDEX statements in a ```sql block.", send: true },
    { cmd: 'chart',    label: 'Chart',      desc: 'Visualize data as a chart',           prompt: 'Chart ', send: false },
    { cmd: 'erd',      label: 'Diagram',    desc: 'Draw an ERD of the schema',           prompt: 'Draw an entity-relationship diagram of this schema as a mermaid erDiagram inside a ```mermaid code block, with the main tables, key columns, and relationships.', send: true },
    { cmd: 'summary',  label: 'Summary',    desc: 'Summarize the whole schema',          prompt: 'Give a concise overview of this database: the main tables, how they relate, and what the schema is for.', send: true },
    { cmd: 'count',    label: 'Row counts', desc: 'Count rows across every table',        prompt: 'Show the row count of every table in the current schema, ordered by count descending. Run the query and report the results.', send: true },
  ]

  const slashItems = $derived.by(() => {
    const q = slashQuery.toLowerCase()
    return SLASH_COMMANDS.filter((c) => !q || c.cmd.startsWith(q) || c.label.toLowerCase().includes(q))
  })

  $effect(() => { void slashItems; slashIdx = 0 })

  /** @param {typeof SLASH_COMMANDS[number]} command */
  function runSlash(command) {
    slashOpen = false
    if (command.send) {
      inputText = ''
      resetInputHeight()
      void send([command.prompt])
    } else {
      inputText = command.prompt
      void tick().then(() => { resizeInput(); inputRef?.focus() })
    }
  }

  // ── Scroll helpers ────────────────────────────────────────────────────────
  /** Sentinel pinned to the end of the transcript; see the observer below. */
  let bottomSentinel = $state(/** @type {HTMLElement | null} */ (null))

  // "Am I at the bottom?" comes from an IntersectionObserver on the sentinel
  // rather than reading scrollHeight/scrollTop in a scroll handler - with
  // content-visibility:auto on the message rows, each such read forces layout
  // of the off-screen subtrees it was meant to skip (same fix as AiChat).
  $effect(() => {
    const root = scrollEl
    const target = bottomSentinel
    if (!root || !target || typeof IntersectionObserver !== 'function') return
    const io = new IntersectionObserver(
      (entries) => { userScrolledUp = !entries[entries.length - 1].isIntersecting },
      { root, rootMargin: '0px 0px 80px 0px', threshold: 0 },
    )
    io.observe(target)
    return () => io.disconnect()
  })

  /** Scroll the transcript to its end without measuring it (no scrollHeight read). */
  function pinToBottom() {
    if (bottomSentinel) bottomSentinel.scrollIntoView({ block: 'end' })
    else if (scrollEl) scrollEl.scrollTop = scrollEl.scrollHeight
  }
  function jumpToBottom() {
    userScrolledUp = false
    pinToBottom()
  }

  // ── Conversations ─────────────────────────────────────────────────────────
  /** @type {import('$lib/stores/conversations.js').Conversation[]} */
  let convList = $state([])
  /** @type {string | null} */
  let activeConvId = $state(null)
  let historyOpen = $state(false)

  async function loadConvList() {
    convList = await listConversations(connectionId || undefined, "sidebar")
  }

  async function restoreLatest() {
    await loadConvList()
    if (items.length || !convList.length) return
    const latest = convList[0]
    activeConvId = latest.id
    items = /** @type {ChatItem[]} */ ((latest.items ?? []).filter(
      (i) => ['user','assistant','result','chart'].includes(/** @type {any} */ (i).kind)
    ))
    apiHistory = /** @type {import('$lib/ai.js').ApiMessage[]} */ (latest.apiHistory ?? [])
    rawApiHistory = /** @type {import('$lib/ai.js').ApiMessage[]} */ (latest.apiHistory ?? [])
    await tick()
    pinToBottom()
  }

  async function persistCurrent() {
    const saveable = items.filter(i => ['user','assistant','result','chart'].includes(i.kind))
    if (saveable.length === 0) return
    const firstUser = saveable.find(i => i.kind === 'user')
    const title = firstUser?.kind === 'user' ? firstUser.text.slice(0, 60) + (firstUser.text.length > 60 ? '…' : '') : 'Conversation'
    const plainItems = $state.snapshot(saveable)
    const plainHistory = $state.snapshot(rawApiHistory)
    if (activeConvId) {
      await updateConversation(activeConvId, { title, items: plainItems, apiHistory: plainHistory })
      convList = convList.map(c => c.id === activeConvId ? { ...c, title } : c)
    } else {
      const conv = await createConversation({ title, source: 'sidebar', schema: schemaContext.activeSchema, connectionId, items: plainItems, apiHistory: plainHistory })
      activeConvId = conv.id
      convList = [conv, ...convList]
    }
  }

  async function selectConversation(/** @type {string} */ id) {
    if (id === activeConvId) { historyOpen = false; return }
    abortCurrentRequest(); await persistCurrent()
    const conv = convList.find(c => c.id === id)
    if (!conv) return
    activeConvId = id
    items = /** @type {ChatItem[]} */ ((conv.items ?? []).filter(i => ['user','assistant','result','chart'].includes(/** @type {any} */ (i).kind)))
    apiHistory = /** @type {import('$lib/ai.js').ApiMessage[]} */ (conv.apiHistory ?? [])
    rawApiHistory = /** @type {import('$lib/ai.js').ApiMessage[]} */ (conv.apiHistory ?? [])
    error = ''; historyOpen = false
    await tick()
    pinToBottom()
  }

  async function removeConversation(/** @type {string} */ id) {
    await deleteConversation(id)
    convList = convList.filter(c => c.id !== id)
    if (activeConvId === id) { activeConvId = null; items = []; apiHistory = []; rawApiHistory = []; error = '' }
  }

  async function clearAllConversations() {
    await clearConversations(connectionId || undefined, 'sidebar')
    convList = []; activeConvId = null; items = []; apiHistory = []; rawApiHistory = []; error = ''; historyOpen = false
  }

  // ── Resize ────────────────────────────────────────────────────────────────
  const initialLayout = loadLayout()
  let width = $state(initialLayout.aiSidebarWidth)
  let resizeStartWidth = initialLayout.aiSidebarWidth
  /** App scale sampled at drag start - `dx` is screen px, `width` is px at 100%. */
  let resizeScale = 1

  // ── Streaming ─────────────────────────────────────────────────────────────
  let streamingContent = $state('')
  let _pendingStreamContent = ''
  let _streamTimer = /** @type {ReturnType<typeof setTimeout> | null} */ (null)
  let _lastStreamCommit = 0
  const STREAM_COMMIT_MS = 90
  let streamingId = $state(/** @type {string | null} */ (null))
  let abortController = /** @type {AbortController | null} */ (null)

  function scheduleStreamingUpdate(content) {
    _pendingStreamContent = content
    if (_streamTimer !== null) return
    const elapsed = performance.now() - _lastStreamCommit
    const delay = elapsed >= STREAM_COMMIT_MS ? 0 : STREAM_COMMIT_MS - elapsed
    _streamTimer = setTimeout(() => { _streamTimer = null; _lastStreamCommit = performance.now(); streamingContent = _pendingStreamContent }, delay)
  }
  function flushStreamingContent() {
    if (_streamTimer !== null) { clearTimeout(_streamTimer); _streamTimer = null; _lastStreamCommit = performance.now(); streamingContent = _pendingStreamContent }
  }
  const displayStreamingContent = $derived(
    streamingContent.includes('<think>')
      ? streamingContent.replace(/<think>[\s\S]*?<\/think>/g, '').replace(/<think>[\s\S]*$/, '').trim()
      : streamingContent.trim()
  )

  $effect(() => { if (isActive) void Promise.resolve().then(() => inputRef?.focus()) })

  onMount(() => { void restoreLatest() })
  let rafId = /** @type {number | null} */ (null)
  onDestroy(() => {
    void persistCurrent()
    if (rafId !== null) { cancelAnimationFrame(rafId); rafId = null }
    if (_streamTimer !== null) { clearTimeout(_streamTimer); _streamTimer = null }
    // Decline pending confirms and abort the in-flight stream/tool loop -
    // otherwise they keep running against a destroyed component.
    for (const i of items.filter((i) => i.kind === 'confirm')) i.resolve(false)
    abortController?.abort()
    abortController = null
  })

  function scrollBottomSoon() {
    if (userScrolledUp || rafId !== null) return
    rafId = requestAnimationFrame(() => { rafId = null; pinToBottom() })
  }
  async function scrollBottom() {
    await tick(); userScrolledUp = false
    pinToBottom()
  }

  function toggleCollapse(key) { const n = new Set(collapsed); n.has(key) ? n.delete(key) : n.add(key); collapsed = n }
  function toggleResult(id) { openResultId = openResultId === id ? null : id }
  function autoOpenResult(/** @type {string} */ id, isSchema = false) { if (!isSchema) openResultId = id }

  async function newChat() {
    abortCurrentRequest(); await persistCurrent()
    activeConvId = null; items = []; apiHistory = []; rawApiHistory = []; error = ''; aiStatusHint = ''; inputText = ''; historyOpen = false
    await tick(); inputRef?.focus()
  }
  function abortCurrentRequest() {
    // Decline pending confirms first so their awaiting tool loops resolve and
    // can observe the abort (snapshot: resolve() splices the item out of `items`).
    for (const i of items.filter((i) => i.kind === 'confirm')) i.resolve(false)
    abortController?.abort(); abortController = null; loading = false
  }
  function stop() { flushStreamingContent(); abortController?.abort() }
  async function copyText(text) { await navigator.clipboard.writeText(text).catch(() => {}) }

  /** @param {number} ts */
  function relTime(ts) {
    const diff = (Date.now() - ts) / 1000
    if (diff < 60) return 'just now'
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
    return new Date(ts).toLocaleDateString()
  }

  /** Readable names for the tab kinds the panel can sit beside. */
  const VIEW_NAMES = /** @type {Record<string, string>} */ ({
    table: 'a table', sql: 'the SQL editor', orm: 'the ORM runner', schema: 'the schema explorer',
    erd: 'the schema diagram', welcome: 'the start page', objects: 'the database objects page',
  })

  /**
   * What is on screen, said every turn, including when nothing is open: with
   * no line at all the model answered "I can't see your UI" to "explain the
   * open table" instead of saying no table is open.
   */
  function buildViewContext() {
    const lines = ['', '=== CURRENT WORKSPACE CONTEXT ===']
    lines.push('This block is the live state of the user\'s workspace and you can rely on it. Never say you cannot see the user\'s screen, table or editor; if they refer to something that is not open, say it is not open and offer what is.')
    lines.push(`Connection schema: "${schemaContext.activeSchema}" (${schemaContext.tables?.length ?? 0} tables).`)
    lines.push(`The user is on ${VIEW_NAMES[currentView] ?? `the "${currentView}" page`}.`)
    if (schemaContext.activeTable) lines.push(`Open table: "${schemaContext.activeSchema}.${schemaContext.activeTable}".`)
    else lines.push('No table is open.')
    if (currentView === 'sql') {
      const t = (currentSql ?? '').trim()
      if (t) { lines.push('SQL in the editor:'); lines.push('```sql'); lines.push(t.slice(0, 4000)); lines.push('```') }
      else lines.push('The SQL editor is empty.')
      lines.push('Return runnable SQL in ```sql blocks.')
    } else if (currentView === 'orm') {
      lines.push(`The ORM runner is open in ${ormMode} mode.`)
    } else {
      lines.push('Return SQL in ```sql blocks when relevant.')
    }
    lines.push('Keep answers concise; this is a compact side panel.')
    return lines.join('\n')
  }

  async function ensureFullSchemaCache() {
    if (!schemaContext.tables?.length) return
    const dbType = schemaContext.dbType ?? 'postgres'
    const isMysql = dbType === 'mysql'
    const isSqliteFamily = dbType === 'sqlite' || dbType === 'd1' || dbType === 'libsql'
    const sc = schemaContext.activeSchema
    const combined = { ...schemaContext.allTableColumns, ...fetchedSchemas }
    const missing = schemaContext.tables.filter(t => !combined[`${sc}.${t.name}`])
    if (!missing.length) return
    try {
      /** @type {Record<string, {name:string, dataType:string, nullable:boolean}[]>} */
      const byTable = {}
      if (isSqliteFamily) {
        // SQLite/D1/LibSQL: PRAGMA per table (no information_schema), in parallel.
        await Promise.all(missing.map(async (t) => {
          try {
            const data = await executeSql(`PRAGMA table_info("${t.name.replace(/"/g, '""')}")`)
            const c = data.columns ?? [], r = data.rows ?? []
            const nameI = c.findIndex((x) => x.name === 'name'), typeI = c.findIndex((x) => x.name === 'type')
            const nnI = c.findIndex((x) => x.name === 'notnull')
            byTable[`${sc}.${t.name}`] = r.map((row) => ({
              name: String(row[nameI] ?? row[1] ?? ''),
              dataType: String(row[typeI] ?? row[2] ?? 'text'),
              nullable: !(row[nnI] ?? row[3]),
            }))
          } catch { /* skip this table */ }
        }))
      } else {
        const scSafe = sc.replace(/'/g, "''")
        const sql = isMysql
          ? `SELECT TABLE_NAME, COLUMN_NAME, DATA_TYPE, IS_NULLABLE FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = '${scSafe}' ORDER BY TABLE_NAME, ORDINAL_POSITION`
          : `SELECT table_name, column_name, data_type, is_nullable FROM information_schema.columns WHERE table_schema = '${scSafe}' ORDER BY table_name, ordinal_position`
        const data = await executeSql(sql)
        for (const row of data.rows ?? []) {
          const key = `${sc}.${String(row[0])}`
          if (!byTable[key]) byTable[key] = []
          byTable[key].push({ name: String(row[1]), dataType: String(row[2]), nullable: String(row[3]).toUpperCase() === 'YES' })
        }
      }
      fetchedSchemas = { ...fetchedSchemas, ...byTable }
    } catch {}
  }

  // ── Send ──────────────────────────────────────────────────────────────────
  async function send(/** @type {string[]} */ [overrideText] = []) {
    const sql = attachedSql
    // An attachment alone is a question too: what does this do.
    const text = (overrideText ?? inputText).trim() || (sql ? 'Explain this query.' : '')
    if (!text || loading) return
    if (!configured) { onopensettings(); return }
    error = ''; aiStatusHint = ''
    if (!overrideText) { inputText = ''; resetInputHeight() }
    attachedSql = ''

    const ctxNote = contextTables.length ? `\n\n(Focus on these tables: ${contextTables.join(', ')})` : ''
    const content = sql ? `${text}\n\n\`\`\`sql\n${sql}\n\`\`\`` : text
    items.push(/** @type {ChatItem} */ ({ id: uid(), kind: 'user', text, ...(sql ? { sql } : {}) }))
    apiHistory.push({ role: 'user', content: content + ctxNote })
    rawApiHistory.push({ role: 'user', content: content + ctxNote })
    if (contextTables.length) contextTables = []
    await scrollBottom()

    const thinkingId = uid()
    items.push(/** @type {ChatItem} */ ({ id: thinkingId, kind: 'thinking' }))
    await scrollBottom()

    loading = true; abortController = new AbortController(); executedCalls = new Set(); failureTracker = new Map()

    const looksLikeDataQuery = text.length > 4 || /select|from|show|list|count|table|schema|column|insert|update|delete/i.test(text)
    if (looksLikeDataQuery) await ensureFullSchemaCache()

    const skills = loadSkills()
    const filteredCtx = filterSchemaForQuery({ ...schemaContext, allTableColumns: { ...schemaContext.allTableColumns, ...fetchedSchemas }, userSkills: skills }, text)
    turnSystemPrompt = buildSystemPrompt(filteredCtx) + '\n' + buildViewContext()

    const { history: managedHistory } = await manageHistory($aiSettings, apiHistory, { maxChars: 200_000, keepLastN: 14, summarizeThreshold: 60_000, onStatus: (msg) => { aiStatusHint = msg } })
    const managedLen = managedHistory.length
    apiHistory = managedHistory

    try {
      await runAiTurn(0)
    } catch (e) {
      if (/** @type {any} */ (e)?.name !== 'AbortError') error = String(e)
    } finally {
      flushStreamingContent()
      if (streamingId) {
        const partial = streamingContent.trim(); const sid = streamingId
        items = items.filter(i => i.kind !== 'thinking' && i.kind !== 'executing').map(i => i.id === sid ? /** @type {ChatItem} */ ({ id: sid, kind: 'assistant', parts: parseAssistantMessage(partial || '…') }) : i)
        streamingId = null; streamingContent = ''; _pendingStreamContent = ''
      } else {
        items = items.filter(i => i.kind !== 'thinking' && i.kind !== 'executing')
      }
      abortController = null; loading = false; openResultId = null; aiStatusHint = ''
      rawApiHistory.push(...apiHistory.slice(managedLen))
      void persistCurrent()
      await tick(); inputRef?.focus()
    }
  }

  /**
   * Programmatically send a message - called by the parent (e.g. "Fix with AI").
   * @param {string} text
   */
  export function sendMessage(text) {
    if (!text.trim()) return
    void send([text])
  }

  /**
   * SQL handed over by the editor's Ask AI. It rides with the next message as
   * a card above the box instead of a fenced block typed into it, so the box
   * stays free for the question and the card can be dropped with one click.
   */
  let attachedSql = $state('')

  /**
   * Start a message without sending it (the SQL editor's Ask AI). A fenced SQL
   * block in `text` becomes the attachment; the rest goes in the box, caret at
   * its end.
   * @param {string} text
   */
  export function draftMessage(text) {
    const fence = text.match(/```(?:sql)?[ \t]*\n([\s\S]*?)```/i)
    if (fence) {
      attachedSql = fence[1].trim()
      inputText = text.replace(fence[0], '').trim()
    } else {
      inputText = text
    }
    void tick().then(() => {
      resizeInput()
      inputRef?.focus()
      const end = inputText.length
      inputRef?.setSelectionRange?.(end, end)
    })
  }

  /**
   * A user message split into prose and fenced code, so a bubble shows SQL as
   * code rather than backticks (older chats carry the fence in the text).
   * @param {string} text
   */
  function splitFences(text) {
    /** @type {{ code: boolean, content: string }[]} */
    const out = []
    const re = /```[\w-]*[ \t]*\n([\s\S]*?)```/g
    let at = 0
    for (const m of text.matchAll(re)) {
      const before = text.slice(at, m.index).trim()
      if (before) out.push({ code: false, content: before })
      out.push({ code: true, content: m[1].trim() })
      at = (m.index ?? 0) + m[0].length
    }
    const rest = text.slice(at).trim()
    if (rest) out.push({ code: false, content: rest })
    return out
  }

  const AI_ROW_LIMIT = 500; const AI_DISPLAY_ROWS = 100

  function guardSelectLimit(sql) {
    const cleaned = sql.trimEnd().replace(/;+$/, '')
    const t = cleaned.trimStart()
    if (!/^(with\b|select\b)/i.test(t)) return { sql: cleaned, capped: false }
    if (/\blimit\s+\d/i.test(t)) return { sql: cleaned, capped: false }
    return { sql: `${cleaned}\nLIMIT ${AI_ROW_LIMIT}`, capped: true }
  }

  async function runAiTurn(depth) {
    if (depth > 40) throw new Error('Too many AI iterations')
    // A null controller means the turn was aborted or finalized - the chain can
    // resume here after a declined confirm, so treat it the same as an abort.
    if (!abortController || abortController.signal.aborted) throw Object.assign(new Error('Aborted'), { name: 'AbortError' })
    if (depth > 0) {
      await new Promise(r => setTimeout(r, 300))
      if (!abortController || abortController.signal.aborted) throw Object.assign(new Error('Aborted'), { name: 'AbortError' })
    }
    let fullContent = ''; /** @type {import('$lib/ai.js').ToolCall[]} */ let toolCalls = []; let itemId = /** @type {string | null} */ (null)
    for await (const chunk of chatCompletionStream($aiSettings, [{ role: 'system', content: turnSystemPrompt }, ...apiHistory], AI_TOOLS, abortController?.signal, ({ attempt, waitMs }) => { aiStatusHint = `Rate limited, retrying in ${Math.ceil(waitMs/1000)}s…` })) {
      if (chunk.textDelta) {
        aiStatusHint = ''; fullContent += chunk.textDelta
        if (!itemId) {
          itemId = uid(); streamingId = itemId
          const idx = items.findIndex(i => i.kind === 'thinking'); if (idx >= 0) items.splice(idx, 1)
          items.push(/** @type {ChatItem} */ ({ id: itemId, kind: 'streaming' }))
        }
        scheduleStreamingUpdate(fullContent); scrollBottomSoon()
      }
      if (chunk.toolCalls) toolCalls = chunk.toolCalls
    }
    if (!abortController || abortController.signal.aborted) throw Object.assign(new Error('Aborted'), { name: 'AbortError' })
    flushStreamingContent()
    if (itemId && streamingId) {
      streamingId = null; streamingContent = ''; _pendingStreamContent = ''
      items = items.map(i => i.id === itemId ? /** @type {ChatItem} */ ({ id: itemId, kind: 'assistant', parts: parseAssistantMessage(fullContent) }) : i)
    }
    if (toolCalls.length > 0) {
      const idx = items.findIndex(i => i.kind === 'thinking'); if (idx >= 0) items.splice(idx, 1)
      apiHistory.push({ role: 'assistant', content: fullContent || null, tool_calls: toolCalls })
      for (const call of toolCalls) await runToolCall(call)
      items.push(/** @type {ChatItem} */ ({ id: uid(), kind: 'thinking' })); scrollBottomSoon()
      await runAiTurn(depth + 1)
    } else if (fullContent) {
      apiHistory.push({ role: 'assistant', content: fullContent })
      if (!itemId) { items.push(/** @type {ChatItem} */ ({ id: uid(), kind: 'assistant', parts: parseAssistantMessage(fullContent) })); await scrollBottom() }
    }
  }

  async function runToolCall(call) {
    // The tool loop can resume here after an abort resolves a pending confirm -
    // answer the call as cancelled instead of executing it for a dead turn.
    if (!abortController || abortController.signal.aborted) {
      apiHistory.push({ role: 'tool', tool_call_id: call.id, content: JSON.stringify({ cancelled: true }) })
      return
    }
    const callKey = `${call.function.name}:${call.function.arguments}`
    if (executedCalls.has(callKey)) { apiHistory.push({ role: 'tool', tool_call_id: call.id, content: JSON.stringify({ error: 'Duplicate call.' }) }); return }
    const prior = failureTracker.get(callKey); if (prior && prior.count >= 2) { apiHistory.push({ role: 'tool', tool_call_id: call.id, content: JSON.stringify({ error: `Failed ${prior.count} times. Do not retry.`, last_error: prior.lastError }) }); return }
    executedCalls.add(callKey)
    let toolResult = ''
    try {
      const args = JSON.parse(call.function.arguments || '{}')
      if (call.function.name === 'execute_sql') {
        const sql = String(args.sql ?? '').trim()
        if (!sql) { apiHistory.push({ role: 'tool', tool_call_id: call.id, content: JSON.stringify({ error: 'Empty SQL' }) }); return }
        // Same refusal as the AI tab: answer in the tool channel so the model
        // stops retrying a write it will never be allowed to make.
        if (isReadOnly() && isWriteSql(sql)) {
          apiHistory.push({ role: 'tool', tool_call_id: call.id, content: JSON.stringify({ error: 'This connection is open in read-only mode. Writing statements are blocked - propose the SQL to the user instead of running it.' }) })
          return
        }
        if (isDestructiveSql(sql)) { const ok = await waitForConfirm(sql); if (!ok) { apiHistory.push({ role: 'tool', tool_call_id: call.id, content: JSON.stringify({ cancelled: true }) }); return } }
        const { sql: g, capped } = guardSelectLimit(sql)
        const execId = uid(); items.push(/** @type {ChatItem} */ ({ id: execId, kind: 'executing', sql })); await scrollBottom()
        try {
          const data = await executeSql(g); const cols = data.columns ?? []; const rows = data.rows ?? []; const total = data.rowCount ?? rows.length
          const resultId = uid(); const resultItem = /** @type {ChatItem} */ ({ id: resultId, kind: 'result', sql, columns: cols, rows: rows.slice(0, AI_DISPLAY_ROWS), total, error: null, capped })
          const idx = items.findIndex(i => i.id === execId); if (idx >= 0) items.splice(idx, 1, resultItem); else items.push(resultItem)
          autoOpenResult(resultId); await scrollBottom()
          toolResult = JSON.stringify({ columns: cols.map(c => c.name), rows: rows.slice(0, 30), total_rows: total })
        } catch (sqlErr) {
          const idx = items.findIndex(i => i.id === execId); if (idx >= 0) items.splice(idx, 1)
          const msg = String(sqlErr); const hint = classifyDbError(msg); const ex = failureTracker.get(callKey) ?? { count: 0, lastError: '' }; failureTracker.set(callKey, { count: ex.count + 1, lastError: msg })
          toolResult = JSON.stringify({ error: msg, ...(hint ? { hint } : {}), attempt: ex.count + 1 })
        }
      } else if (call.function.name === 'describe_table') {
        const schema = String(args.schema ?? schemaContext.activeSchema).replace(/'/g, "''"); const table = String(args.table ?? '').replace(/'/g, "''")
        const dbType = schemaContext.dbType ?? 'postgres'
        const isSqliteFamily = dbType === 'sqlite' || dbType === 'd1' || dbType === 'libsql'
        let cols, rows, colObjs
        if (isSqliteFamily) {
          // SQLite/D1/LibSQL: PRAGMA (no information_schema). Columns: cid, name, type, notnull, dflt_value, pk
          const data = await executeSql(`PRAGMA table_info("${table.replace(/"/g, '""')}")`)
          cols = data.columns ?? []; rows = data.rows ?? []
          const nameI = cols.findIndex((c) => c.name === 'name'), typeI = cols.findIndex((c) => c.name === 'type')
          const nnI = cols.findIndex((c) => c.name === 'notnull'), dfltI = cols.findIndex((c) => c.name === 'dflt_value')
          colObjs = rows.map(r => ({ name: r[nameI] ?? r[1], type: r[typeI] ?? r[2] ?? 'text', nullable: !(r[nnI] ?? r[3]), default: r[dfltI] ?? r[4] ?? null }))
        } else {
          const descSql = dbType === 'mysql' ? `SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, COLUMN_DEFAULT FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = '${schema}' AND TABLE_NAME = '${table}' ORDER BY ORDINAL_POSITION` : `SELECT column_name, data_type, is_nullable, column_default FROM information_schema.columns WHERE table_schema = '${schema}' AND table_name = '${table}' ORDER BY ordinal_position`
          const data = await executeSql(descSql); cols = data.columns ?? []; rows = data.rows ?? []
          colObjs = rows.map(r => ({ name: r[0], type: r[1], nullable: r[2] === 'YES', default: r[3] ?? null }))
        }
        const sid = uid(); items.push(/** @type {ChatItem} */ ({ id: sid, kind: 'result', sql: `${schema}.${table} schema`, columns: cols, rows, total: rows.length, error: null, isSchema: true }))
        autoOpenResult(sid, true); await scrollBottom()
        toolResult = JSON.stringify({ table: `${schema}.${table}`, columns: colObjs })
      } else if (call.function.name === 'render_chart') {
        const chartId = uid()
        if (!args.data?.length) { items.push(/** @type {ChatItem} */ ({ id: chartId, kind: 'chart', spec: args, error: 'No data provided.' })); toolResult = JSON.stringify({ error: 'No data.' }) }
        else { items.push(/** @type {ChatItem} */ ({ id: chartId, kind: 'chart', spec: args, error: null })); await scrollBottom(); toolResult = JSON.stringify({ success: true }) }
      } else if (call.function.name === 'list_tables') {
        toolResult = JSON.stringify({ schema: schemaContext.activeSchema, tables: schemaContext.tables.map(t => ({ name: t.name, rowCount: t.rowCount })), total: schemaContext.tables.length })
      } else if (call.function.name === 'get_schema') {
        const targetTable = String(args.table ?? '').trim()
        try {
          const dbType = schemaContext.dbType ?? 'postgres'
          const isMysql = dbType === 'mysql'
          const isSqliteFamily = dbType === 'sqlite' || dbType === 'd1' || dbType === 'libsql'
          const sc = schemaContext.activeSchema.replace(/'/g, "''")
          if (isSqliteFamily && targetTable) {
            const data = await executeSql(`PRAGMA table_info("${targetTable.replace(/"/g, '""')}")`)
            const c = data.columns ?? [], r = data.rows ?? []
            const nameI = c.findIndex((x) => x.name === 'name'), typeI = c.findIndex((x) => x.name === 'type')
            const nnI = c.findIndex((x) => x.name === 'notnull'), dfltI = c.findIndex((x) => x.name === 'dflt_value')
            toolResult = JSON.stringify({ table: `${schemaContext.activeSchema}.${targetTable}`, columns: r.map(row => ({ name: row[nameI] ?? row[1], type: row[typeI] ?? row[2] ?? 'text', nullable: !(row[nnI] ?? row[3]), default: row[dfltI] ?? row[4] ?? null })) })
          } else if (isSqliteFamily) {
            const byTable = /** @type {Record<string, unknown[]>} */ ({})
            await Promise.all((schemaContext.tables ?? []).map(async (/** @type {{name: string}} */ t) => {
              try {
                const data = await executeSql(`PRAGMA table_info("${t.name.replace(/"/g, '""')}")`)
                const c = data.columns ?? [], r = data.rows ?? []
                const nameI = c.findIndex((x) => x.name === 'name'), typeI = c.findIndex((x) => x.name === 'type')
                const nnI = c.findIndex((x) => x.name === 'notnull')
                byTable[t.name] = r.map(row => ({ name: row[nameI] ?? row[1], type: row[typeI] ?? row[2] ?? 'text', nullable: !(row[nnI] ?? row[3]) }))
              } catch { byTable[t.name] = [] }
            }))
            toolResult = JSON.stringify({ schema: schemaContext.activeSchema, tables: byTable })
          } else if (targetTable) {
            const tt = targetTable.replace(/'/g, "''"); const sql = isMysql ? `SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE, COLUMN_DEFAULT FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = '${sc}' AND TABLE_NAME = '${tt}' ORDER BY ORDINAL_POSITION` : `SELECT column_name, data_type, is_nullable, column_default FROM information_schema.columns WHERE table_schema = '${sc}' AND table_name = '${tt}' ORDER BY ordinal_position`
            const data = await executeSql(sql); toolResult = JSON.stringify({ table: `${schemaContext.activeSchema}.${targetTable}`, columns: (data.rows ?? []).map(r => ({ name: r[0], type: r[1], nullable: r[2] === 'YES', default: r[3] ?? null })) })
          } else {
            const sql = isMysql ? `SELECT TABLE_NAME, COLUMN_NAME, DATA_TYPE, IS_NULLABLE FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = '${sc}' ORDER BY TABLE_NAME, ORDINAL_POSITION` : `SELECT table_name, column_name, data_type, is_nullable FROM information_schema.columns WHERE table_schema = '${sc}' ORDER BY table_name, ordinal_position`
            const data = await executeSql(sql); const byTable = /** @type {Record<string, unknown[]>} */ ({})
            for (const row of data.rows ?? []) { const n = String(row[0]); if (!byTable[n]) byTable[n] = []; byTable[n].push({ name: row[1], type: row[2], nullable: row[3] === 'YES' }) }
            toolResult = JSON.stringify({ schema: schemaContext.activeSchema, tables: byTable })
          }
        } catch (e) { toolResult = JSON.stringify({ error: String(e) }) }
      } else { toolResult = JSON.stringify({ error: `Unknown tool: ${call.function.name}` }) }
    } catch (e) {
      items = items.filter(i => i.kind !== 'executing')
      const msg = String(e); const hint = classifyDbError(msg); const ex = failureTracker.get(callKey) ?? { count: 0, lastError: '' }; failureTracker.set(callKey, { count: ex.count + 1, lastError: msg })
      toolResult = JSON.stringify({ error: msg, ...(hint ? { hint } : {}), attempt: ex.count + 1 })
    }
    apiHistory.push({ role: 'tool', tool_call_id: call.id, content: toolResult })
  }

  async function runSqlBlock(/** @type {string} */ sql) {
    if (loading) return; error = ''
    if (isDestructiveSql(sql)) { const ok = await waitForConfirm(sql); if (!ok) return }
    loading = true; const execId = uid(); items.push(/** @type {ChatItem} */ ({ id: execId, kind: 'executing', sql })); await scrollBottom()
    try {
      const data = await executeSql(sql); const cols = data.columns ?? []; const rows = data.rows ?? []
      const resId = uid(); const ri = /** @type {ChatItem} */ ({ id: resId, kind: 'result', sql, columns: cols, rows, total: data.rowCount ?? rows.length, error: null })
      const idx = items.findIndex(i => i.id === execId); if (idx >= 0) items.splice(idx, 1, ri); else items.push(ri)
      autoOpenResult(resId); await scrollBottom()
    } catch (e) {
      const errId = uid(); const ei = /** @type {ChatItem} */ ({ id: errId, kind: 'result', sql, columns: [], rows: [], total: 0, error: String(e) })
      const idx = items.findIndex(i => i.id === execId); if (idx >= 0) items.splice(idx, 1, ei); else items.push(ei)
      autoOpenResult(errId); await scrollBottom()
    } finally { loading = false }
  }

  function waitForConfirm(/** @type {string} */ sql) {
    return new Promise((resolve) => {
      const itemId = uid()
      items.push(/** @type {ChatItem} */ ({ id: itemId, kind: 'confirm', sql, resolve: (ok) => { const idx = items.findIndex(i => i.id === itemId); if (idx >= 0) items.splice(idx, 1); resolve(ok) } }))
      void scrollBottom()
    })
  }

  function acceptSql(sql) { onaccept({ kind: 'sql', content: sql }) }
  function acceptCode(lang, code) { onaccept({ kind: 'code', lang, content: code }) }

  function resizeInput() {
    if (!inputRef) return
    inputRef.style.height = 'auto'
    inputRef.style.height = `${Math.min(inputRef.scrollHeight, 160)}px`
  }
  function resetInputHeight() { if (inputRef) inputRef.style.height = 'auto' }

  /** Starters for an empty chat, each with an icon that says what it does. */
  const suggestions = $derived.by(() => {
    /** @type {{ label: string, icon: typeof Sparkles }[]} */
    const out = []
    if (currentView === 'sql' && (currentSql ?? '').trim()) out.push({ label: 'Explain this query', icon: MessageSquareText }, { label: 'Optimize this query', icon: Gauge })
    else if (currentView === 'orm') out.push({ label: `Write a ${ormMode} query for the active table`, icon: CodeXml })
    if (schemaContext.activeTable) out.push({ label: `Show 10 recent rows from ${schemaContext.activeTable}`, icon: Rows3 }, { label: `Describe ${schemaContext.activeTable}`, icon: FileText })
    else out.push({ label: 'List the tables', icon: Table2 }, { label: 'Draw an ERD of this schema', icon: Network })
    return out.slice(0, 4)
  })

  const showWorking = $derived(loading && !items.some(i => i.kind === 'thinking' || i.kind === 'streaming' || i.kind === 'executing'))
</script>

<div
  class="relative flex h-full min-h-0 min-w-0 shrink-0 flex-col overflow-hidden border-l border-border/50 bg-background"
  style="width: calc({width}px * var(--app-scale, 1)); min-width: calc({width}px * var(--app-scale, 1)); max-width: calc({width}px * var(--app-scale, 1))"
  data-studio-region="ai-sidebar"
>
  <div class="absolute inset-y-0 left-0 z-20">
    <ResizeHandle edge="start" onresizestart={() => { resizeStartWidth = width; resizeScale = getAppScale() }} onresize={(dx) => { width = clampAiSidebarWidth(resizeStartWidth + dx / resizeScale) }} onresizeend={() => saveLayout({ aiSidebarWidth: width })} />
  </div>

  <!-- Header -->
  <div class="studio-chrome flex h-9 shrink-0 items-center gap-1.5 border-b border-border/50 px-3" data-studio-chrome>
    <Sparkles class="size-3.5 shrink-0 text-primary" />
    <span class="min-w-0 flex-1 truncate text-ui-xs font-medium text-foreground">Assistant</span>

    <!-- Model picker -->
    <AiModelPicker onopenSettings={onopensettings} />

    <div class="flex items-center gap-0.5">
      <button type="button"
        class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
        title="New chat"
        aria-label="New chat"
        disabled={items.length === 0 && !loading}
        onclick={() => void newChat()}
      ><SquarePen class="size-3.5" /></button>
      <button type="button"
        class={cn('inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground', historyOpen && 'bg-accent text-foreground')}
        title="Conversation history"
        aria-label="Conversation history"
        aria-pressed={historyOpen}
        onclick={() => { historyOpen = !historyOpen; if (historyOpen) void loadConvList() }}
      ><History class="size-3.5" /></button>
      <button type="button"
        class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        title={`Close (${comboTitle('Mod+I')})`}
        aria-label="Close the assistant"
        onclick={onclose}
      ><X class="size-3.5" /></button>
    </div>
  </div>

  <!-- History dropdown -->
  {#if historyOpen}
    <button type="button" class="absolute inset-0 z-30 cursor-default" aria-label="Close history" onclick={() => (historyOpen = false)}></button>
    <div class="absolute right-2 top-11 z-40 flex max-h-[55%] w-[calc(100%-1rem)] flex-col overflow-hidden rounded-[10px] border border-border/60 bg-popover elevate-2-rim">
      <div class="flex items-center justify-between gap-2 border-b border-border/50 px-3 py-2.5">
        <span class="text-ui-xs font-medium text-foreground">History</span>
        {#if convList.length}
          <button type="button" class="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-ui-2xs text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive" onclick={() => void clearAllConversations()}>
            <Trash2 class="size-3" />Clear all
          </button>
        {/if}
      </div>
      <div class="app-scroll min-h-0 flex-1 overflow-y-auto p-1">
        {#if convList.length === 0}
          <p class="px-3 py-5 text-center text-ui-xs text-muted-foreground">No saved chats yet</p>
        {:else}
          {#each convList as conv (conv.id)}
            <div class={cn('group flex items-center gap-1 rounded-lg px-2 py-1.5 transition-colors hover:bg-accent/40', conv.id === activeConvId && 'bg-accent/60')}>
              <button type="button" class="flex min-w-0 flex-1 flex-col items-start gap-0.5 text-left" onclick={() => void selectConversation(conv.id)}>
                <span class="w-full truncate text-ui-xs text-foreground">{conv.title}</span>
                <span class="text-ui-2xs text-muted-foreground">{relTime(conv.updatedAt)}</span>
              </button>
              <button type="button" class="inline-flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground opacity-0 transition-opacity hover:bg-destructive/10 hover:text-destructive group-hover:opacity-100" onclick={() => void removeConversation(conv.id)}>
                <Trash2 class="size-3" />
              </button>
            </div>
          {/each}
        {/if}
      </div>
    </div>
  {/if}

  <!-- Messages -->
  <div bind:this={scrollEl}
    class="app-scroll relative min-h-0 flex-1 overflow-y-auto [will-change:transform] [overflow-anchor:none]">

    {#if items.length === 0}
      <!-- Empty state. It sits at the foot of the pane, just above the box:
           the starters are where the hands already are, and a tall pane no
           longer leaves them floating in the middle. -->
      <div class="flex min-h-full flex-col justify-end gap-4 px-3 pb-3 pt-6">
        <div class="flex items-start gap-2.5 px-1">
          <span class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
            <Sparkles class="size-4" />
          </span>
          <div class="min-w-0">
            <p class="text-ui-sm font-medium text-foreground">Ask about your data</p>
            {#if schemaContext.activeTable}
              <p class="truncate font-mono text-ui-2xs text-muted-foreground">{schemaContext.activeSchema}.{schemaContext.activeTable}</p>
            {:else if schemaContext.tables?.length}
              <p class="truncate font-mono text-ui-2xs text-muted-foreground">{schemaContext.activeSchema} · {schemaContext.tables.length} tables</p>
            {:else}
              <p class="text-ui-2xs text-muted-foreground">Knows your schema, the open table and the editor</p>
            {/if}
          </div>
        </div>

        {#if !configured}
          <button
            type="button"
            class="inline-flex h-8 items-center justify-center gap-1.5 rounded-lg bg-primary px-3 text-ui-xs font-medium text-primary-foreground transition-[background-color,scale] duration-150 hover:bg-primary/90 active:scale-[0.96]"
            onclick={onopensettings}
          >
            <Sparkles class="size-3.5" />Configure a model
          </button>
        {:else}
          <ul class="flex flex-col gap-0.5" aria-label="Suggestions">
            {#each suggestions as s (s.label)}
              {@const Icon = s.icon}
              <li>
                <button type="button"
                  class="group/sug flex h-8 w-full items-center gap-2.5 rounded-md px-2 text-left text-ui-xs text-foreground transition-colors hover:bg-accent disabled:opacity-40"
                  disabled={loading}
                  onclick={() => void send([s.label])}
                >
                  <Icon class="size-3.5 shrink-0 text-muted-foreground transition-colors group-hover/sug:text-foreground" />
                  <span class="min-w-0 flex-1 truncate">{s.label}</span>
                  <CornerDownLeft class="size-3 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover/sug:opacity-100" />
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>

    {:else}
      <!-- Messages list -->
      <div class="flex flex-col gap-3 px-3 py-3" data-studio-selectable="text">

        {#each items as item (item.id)}
          <!-- content-visibility:auto skips layout/paint for off-screen messages so
               scrolling long conversations stays smooth (transient animated rows are
               excluded so their ping/bounce isn't clipped by paint containment). -->
          <div class={item.kind === 'thinking' || item.kind === 'executing' || item.kind === 'streaming' ? '' : '[content-visibility:auto] [contain-intrinsic-size:auto_100px]'}>
          {#if item.kind === 'user'}
            <!-- Prose stays prose; SQL, attached or fenced in an older chat,
                 shows as code inside the bubble instead of backticks. -->
            <div class="flex justify-end">
              <div class="flex max-w-[88%] min-w-0 flex-col gap-1.5 rounded-xl rounded-tr-md bg-primary px-3 py-2 text-ui-xs leading-relaxed text-primary-foreground">
                {#each splitFences(item.text) as part, pi (pi)}
                  {#if part.code}
                    <pre class="max-h-40 overflow-auto rounded-md bg-primary-foreground/12 px-2 py-1.5 font-mono text-ui-2xs leading-snug whitespace-pre-wrap">{part.content}</pre>
                  {:else}
                    <p class="font-reading whitespace-pre-wrap break-words">{part.content}</p>
                  {/if}
                {/each}
                {#if item.sql}
                  <pre class="max-h-40 overflow-auto rounded-md bg-primary-foreground/12 px-2 py-1.5 font-mono text-ui-2xs leading-snug whitespace-pre-wrap">{item.sql}</pre>
                {/if}
              </div>
            </div>

          {:else if item.kind === 'thinking'}
            <div class="flex items-center gap-2.5">
              <Sparkles class="size-3 shrink-0 animate-pulse text-primary" />
              <span class="agent-think-label text-ui-xs text-muted-foreground transition-opacity duration-200 {thinkingVisible ? 'opacity-100' : 'opacity-0'}">{aiStatusHint || thinkingPhrase}</span>
            </div>

          {:else if item.kind === 'streaming'}
            <AiMarkdown content={displayStreamingContent} streaming class="text-ui-xs" />

          {:else if item.kind === 'assistant'}
            <div class="flex flex-col gap-2">
              {#each item.parts as part, pi}
                {#if part.type === 'text'}
                  <AiMarkdown content={part.content} class="text-ui-xs" />

                {:else if part.type === 'sql'}
                  {@const sqlKey = `${item.id}-${pi}`}
                  {@const sqlOpen = !collapsed.has(sqlKey)}
                  <div class="overflow-hidden rounded-lg border border-border/50 bg-card/30">
                    <div class="group/sqlbar flex items-center gap-2 border-b border-border/30 bg-muted/8 px-2.5 py-1.5">
                      <button type="button" class="flex min-w-0 flex-1 items-center gap-1.5 text-left" onclick={() => toggleCollapse(sqlKey)}>
                        <span class="flex size-4 shrink-0 items-center justify-center text-muted-foreground">
                          {#if sqlOpen}<ChevronDown class="size-3.5" />{:else}<ChevronRight class="size-3.5" />{/if}
                        </span>
                        <span class="shrink-0 rounded border border-border/40 bg-muted/50 px-1 font-mono text-ui-3xs font-semibold uppercase tracking-widest text-muted-foreground">SQL</span>
                        <span class="min-w-0 truncate font-mono text-ui-2xs text-muted-foreground">{part.content.trim().replace(/\s+/g, ' ').slice(0, 60)}</span>
                      </button>
                      <div class="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover/sqlbar:opacity-100">
                        <button type="button" class="inline-flex size-6 items-center justify-center rounded text-muted-foreground hover:bg-muted/60 hover:text-foreground" title="Copy" onclick={() => copyText(part.content)}><Copy class="size-3" /></button>
                        <button type="button" class="inline-flex size-6 items-center justify-center rounded text-muted-foreground hover:bg-muted/60 hover:text-foreground" title="Accept" onclick={() => acceptSql(part.content)}><CornerDownLeft class="size-3" /></button>
                        <button type="button" class="inline-flex h-6 items-center gap-1 rounded bg-primary px-2 text-ui-3xs font-medium text-primary-foreground hover:opacity-90 disabled:opacity-40" disabled={loading} onclick={() => void runSqlBlock(part.content)}><Play class="size-3" />Run</button>
                      </div>
                    </div>
                    <AiSqlBlock sql={part.content} open={sqlOpen} />
                  </div>

                {:else if part.type === 'mermaid'}
                  <div class="overflow-hidden rounded-lg border border-border/50">
                    <div class="flex items-center justify-between gap-1 border-b border-border/30 bg-muted/10 px-2.5 py-1.5">
                      <span class="font-mono text-ui-2xs text-muted-foreground">diagram</span>
                      <button type="button" class="inline-flex size-6 items-center justify-center rounded text-muted-foreground hover:bg-muted/60 hover:text-foreground" title="Copy" aria-label="Copy" onclick={() => copyText(part.content)}><Copy class="size-3" /></button>
                    </div>
                    <ShikiBlock code={part.content} lang="plaintext" embedded />
                  </div>

                {:else if part.type === 'error'}
                  <p class="text-ui-xs text-muted-foreground">{part.content}</p>

                {:else if part.type === 'confirm_prompt'}
                  <div class="flex items-start gap-2 rounded-lg border border-warning/30 bg-warning/6 px-3 py-2 text-ui-xs text-warning">
                    <AlertTriangle class="mt-0.5 size-3.5 shrink-0" /><span>{part.content}</span>
                  </div>

                {:else}
                  {@const codeKey = `${item.id}-${pi}`}
                  {@const codeOpen = !collapsed.has(codeKey)}
                  <div class="overflow-hidden rounded-lg border border-border/50">
                    <div class="flex items-center justify-between gap-1 border-b border-border/30 bg-muted/10 px-2.5 py-1.5">
                      <button type="button" class="flex items-center gap-1 text-ui-2xs text-muted-foreground hover:text-foreground" onclick={() => toggleCollapse(codeKey)}>
                        {#if codeOpen}<ChevronDown class="size-3" />{:else}<ChevronRight class="size-3" />{/if}
                        <span class="font-mono">{part.lang || 'code'}</span>
                      </button>
                      <div class="flex gap-0.5">
                        <button type="button" class="inline-flex size-6 items-center justify-center rounded text-muted-foreground hover:bg-muted/60 hover:text-foreground" title="Copy" aria-label="Copy" onclick={() => copyText(part.content)}><Copy class="size-3" /></button>
                        <button type="button" class="inline-flex size-6 items-center justify-center rounded text-muted-foreground hover:bg-muted/60 hover:text-foreground" title="Accept" onclick={() => acceptCode(part.lang, part.content)}><CornerDownLeft class="size-3" /></button>
                      </div>
                    </div>
                    {#if codeOpen}<ShikiBlock code={part.content} lang={part.lang || 'plaintext'} embedded />{/if}
                  </div>
                {/if}
              {/each}
            </div>

          {:else if item.kind === 'executing'}
            <div class="flex items-center gap-2.5">
              <Loader2 class="size-3 shrink-0 animate-spin text-muted-foreground" />
              <span class="shrink-0 rounded bg-warning/10 px-1.5 py-0.5 font-mono text-ui-3xs font-medium text-warning">SQL</span>
              <span class="min-w-0 truncate text-ui-xs text-muted-foreground">{item.sql}</span>
            </div>

          {:else if item.kind === 'result'}
            {@const resOpen = openResultId === item.id}
            <div class={cn('overflow-hidden rounded-lg border text-ui-2xs', item.error ? 'border-destructive/35 bg-destructive/4' : item.isSchema ? 'border-primary/20 bg-primary/3' : 'border-border/40 bg-muted/8')}>
              <button type="button"
                class={cn('flex w-full items-center gap-1.5 px-2.5 py-1.5 text-left transition-colors hover:bg-muted/20', resOpen && 'border-b border-border/30')}
                onclick={() => toggleResult(item.id)}>
                {#if resOpen}<ChevronDown class="size-3 shrink-0 text-muted-foreground" />{:else}<ChevronRight class="size-3 shrink-0 text-muted-foreground" />{/if}
                <Table2 class={cn('size-3 shrink-0', item.isSchema ? 'text-primary' : 'text-muted-foreground')} />
                <span class="min-w-0 flex-1 truncate text-ui-xs text-muted-foreground">{item.sql || 'Query'}</span>
                {#if !item.error}
                  <span class="shrink-0 rounded bg-muted/50 px-1.5 py-0.5 font-mono text-ui-3xs tabular-nums text-muted-foreground">{formatCompactCount(item.total)} {item.total === 1 ? 'row' : 'rows'}</span>
                {/if}
              </button>
              {#if resOpen}
                {#if item.error}
                  <div class="flex items-start gap-2 px-2.5 py-2">
                    <AlertTriangle class="mt-0.5 size-3.5 shrink-0 text-destructive" />
                    <p class="font-mono text-ui-2xs leading-relaxed text-destructive">{item.error}</p>
                  </div>
                {:else if item.rows.length === 0}
                  <p class="px-2.5 py-2.5 text-center text-ui-2xs italic text-muted-foreground">No rows returned.</p>
                {:else}
                  <div class="overflow-x-auto"><DataTable columns={item.columns} rows={item.rows.slice(0, 15)} embedded showSelection={false} /></div>
                  {#if item.total > 15}<p class="border-t border-border/20 px-2.5 py-1 text-ui-3xs text-muted-foreground">Showing 15 of {formatCompactCount(item.total)} rows</p>{/if}
                {/if}
              {/if}
            </div>

          {:else if item.kind === 'confirm'}
            <div class="overflow-hidden rounded-lg border border-destructive/35 bg-destructive/4">
              <div class="flex items-center gap-2 border-b border-destructive/25 px-2.5 py-2">
                <AlertTriangle class="size-3.5 shrink-0 text-destructive" />
                <span class="text-ui-2xs font-medium text-destructive">Confirm destructive operation</span>
              </div>
              <pre class="whitespace-pre-wrap px-2.5 py-2 font-mono text-ui-2xs text-foreground">{item.sql}</pre>
              <div class="flex items-center justify-end gap-2 border-t border-destructive/15 px-2.5 py-1.5">
                <button type= "field-surface button"class="inline-flex h-7 items-center px-3 text-ui-2xs text-muted-foreground hover:bg-accent"onclick={() => item.resolve(false)}>Cancel</button>
                <button type="button" class="inline-flex h-7 items-center rounded-md bg-destructive px-3 text-ui-2xs font-medium text-destructive-foreground hover:opacity-90" onclick={() => item.resolve(true)}>Execute</button>
              </div>
            </div>

          {:else if item.kind === 'chart'}
            <div class="group/chart">
              {#if item.error}
                <p class="flex items-center gap-1.5 text-ui-xs text-destructive">
                  <AlertTriangle class="size-3 shrink-0" />{item.error}
                </p>
              {:else}
                <div class="mb-0.5 flex items-center gap-1.5">
                  <span class="min-w-0 flex-1 truncate font-mono text-ui-3xs font-medium text-foreground/55">{item.spec.title || ''}</span>
                  <span class="font-mono text-ui-3xs capitalize text-muted-foreground opacity-0 transition-opacity group-hover/chart:opacity-100">{item.spec.type}</span>
                </div>
                <div style="height:{['choropleth','dendrogram','tree','sankey'].includes(item.spec.type) ? 340 : 240}px; width:100%">
                  <AiChartRenderer spec={item.spec} noTitle={true} />
                </div>
              {/if}
            </div>
          {/if}
          </div>
        {/each}

        {#if showWorking}
          <div class="flex items-center gap-2.5">
            <Sparkles class="size-3 shrink-0 animate-pulse text-primary" />
            <span class="agent-think-label text-ui-xs text-muted-foreground transition-opacity duration-200 {thinkingVisible ? 'opacity-100' : 'opacity-0'}">{aiStatusHint || thinkingPhrase}</span>
          </div>
        {/if}

        <!-- Watched by the observer above to decide whether the transcript is
             pinned to the bottom. A zero-height element is enough. -->
        <div bind:this={bottomSentinel} aria-hidden="true" class="h-px w-full shrink-0"></div>
      </div>
    {/if}

    {#if error}
      <div class="mx-3 mb-3 flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/6 px-3 py-2 text-ui-xs text-destructive">
        <AlertTriangle class="mt-0.5 size-3.5 shrink-0" />
        <span class="min-w-0 break-words">{error}</span>
      </div>
    {/if}
  </div>

  <!-- Jump to bottom -->
  {#if userScrolledUp}
    <div class="pointer-events-none absolute inset-x-0 bottom-20 z-10 flex justify-center">
      <button type="button" onclick={jumpToBottom} class="pointer-events-auto flex items-center gap-1.5 rounded-full border border-border/50 bg-background px-3 py-1.5 text-ui-2xs font-medium text-foreground elevate-2-rim transition-all hover:bg-accent">
        <ChevronDown class="size-3" />Jump to bottom
      </button>
    </div>
  {/if}

  <!-- Input area -->
  <div class="relative shrink-0 bg-background px-2 pb-2 pt-1">

    <!-- @ mention popup -->
    {#if mentionOpen && mentionItems.length > 0}
      <div
        class="absolute bottom-full left-2.5 right-2.5 z-50 mb-1.5 overflow-hidden rounded-[10px] border border-border/60 bg-popover elevate-2-rim">
        <div class="app-scroll max-h-56 overflow-y-auto p-1">
          {#each mentionItems as item, idx (item.insert)}
            {@const active = idx === mentionIdx}
            <button type="button"
              class={cn('flex w-full items-center gap-2 rounded-md px-2 py-1 text-left transition-colors', active ? 'bg-accent' : 'hover:bg-accent/40')}
              onmousedown={(e) => { e.preventDefault(); insertMention(item.insert) }}
            >
              <Table2 class={cn('size-3 shrink-0', active ? 'text-primary' : 'text-muted-foreground')} />
              <span class={cn('min-w-0 flex-1 truncate font-mono text-ui-2xs', active ? 'text-foreground' : 'text-foreground/70')}>{item.label}</span>
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <!-- slash command popup -->
    {#if slashOpen && slashItems.length > 0}
      <div class="absolute bottom-full left-2.5 right-2.5 z-50 mb-1.5 overflow-hidden rounded-[10px] border border-border/60 bg-popover elevate-2-rim">
        <div class="app-scroll max-h-56 overflow-y-auto p-1">
          {#each slashItems as item, idx (item.cmd)}
            {@const active = idx === slashIdx}
            <button type="button"
              class={cn('flex w-full items-center gap-2 rounded-md px-2 py-1 text-left transition-colors', active ? 'bg-accent' : 'hover:bg-accent/40')}
              onmousedown={(e) => { e.preventDefault(); runSlash(item) }}
            >
              <span class={cn('w-14 shrink-0 truncate font-mono text-ui-2xs', active ? 'text-primary' : 'text-muted-foreground')}>/{item.cmd}</span>
              <span class="shrink-0 text-ui-2xs font-medium text-foreground/85">{item.label}</span>
              <span class="min-w-0 flex-1 truncate text-right text-ui-3xs text-muted-foreground">{item.desc}</span>
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Input box. One surface: context chips, an attached statement, the
         text and the actions, grouped by space rather than inner rules. The
         box is rounded-xl so its corner runs concentric with the send
         button's rounded-lg at 6px inset. -->
    <div class="flex flex-col gap-1.5 rounded-xl border border-border/60 bg-muted/20 p-1.5 transition-[border-color,background-color] focus-within:border-border focus-within:bg-background">
      {#if contextTables.length || schemaContext.activeTable || (currentView === 'sql' && currentSql.trim() && !attachedSql)}
        <div class="flex flex-wrap items-center gap-1 px-1 pt-0.5">
          <!-- Mentioned tables: the schema is dropped when it is the one in use,
               so two or three fit on a line instead of one chip per line. -->
          {#each contextTables as t (t)}
            {@const short = t.startsWith(`${schemaContext.activeSchema}.`) ? t.slice(schemaContext.activeSchema.length + 1) : t}
            <span class="inline-flex h-6 max-w-full min-w-0 items-center gap-1 rounded-md bg-primary/10 pl-1.5 pr-0.5 text-primary" title={t}>
              <Table2 class="size-3 shrink-0" />
              <span class="min-w-0 truncate font-mono text-ui-2xs">{short}</span>
              <button type="button" class="inline-flex size-5 shrink-0 items-center justify-center rounded text-primary/80 transition-colors hover:bg-primary/15 hover:text-primary" title="Remove {t}" aria-label="Remove {t}" onclick={() => (contextTables = contextTables.filter((x) => x !== t))}>
                <X class="size-3" />
              </button>
            </span>
          {/each}
          {#if schemaContext.activeTable && !contextTables.includes(`${schemaContext.activeSchema}.${schemaContext.activeTable}`)}
            <span class="inline-flex h-6 max-w-full min-w-0 items-center gap-1 rounded-md bg-foreground/[0.06] px-1.5 text-muted-foreground" title="{schemaContext.activeSchema}.{schemaContext.activeTable} is part of every question">
              <Table2 class="size-3 shrink-0" />
              <span class="min-w-0 truncate font-mono text-ui-2xs">{schemaContext.activeTable}</span>
            </span>
          {/if}
          {#if currentView === 'sql' && currentSql.trim() && !attachedSql}
            <span class="inline-flex h-6 items-center gap-1 rounded-md bg-foreground/[0.06] px-1.5 text-ui-2xs text-muted-foreground" title="The SQL in the editor is part of every question">
              <CodeXml class="size-3 shrink-0" />Editor SQL
            </span>
          {/if}
        </div>
      {/if}

      {#if attachedSql}
        <!-- The statement from the editor's Ask AI: goes with the next message. -->
        <div class="group/att flex items-start gap-2 rounded-lg bg-foreground/[0.05] py-1.5 pl-2 pr-1">
          <CodeXml class="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
          <pre class="line-clamp-3 min-w-0 flex-1 font-mono text-ui-2xs leading-snug whitespace-pre-wrap break-all text-foreground" title={attachedSql}>{attachedSql}</pre>
          <button
            type="button"
            class="inline-flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            title="Remove the statement"
            aria-label="Remove the attached statement"
            onclick={() => { attachedSql = ''; inputRef?.focus() }}
          ><X class="size-3" /></button>
        </div>
      {/if}

      <textarea
        bind:this={inputRef}
        value={inputText}
        oninput={handleInputChange}
        onkeydown={handleInputKeydown}
        rows="1"
        aria-label="Message"
        placeholder={!configured ? 'Configure a model first' : attachedSql ? 'Ask about this query, or press Enter to explain it' : 'Ask about your data'}
        disabled={!configured}
        class="no-focus-ring max-h-40 min-h-[2.25rem] w-full resize-none bg-transparent px-1.5 py-1 font-reading text-ui-xs leading-relaxed text-foreground outline-none placeholder:text-muted-foreground disabled:opacity-60"
      ></textarea>

      <!-- Actions -->
      <div class="flex items-center gap-0.5">
        <button type="button"
          class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          title="Mention a table (@)"
          aria-label="Mention a table"
          onclick={() => { inputText += '@'; inputRef?.focus(); void tick().then(resizeInput) }}
        ><At class="size-3.5" /></button>
        <button type="button"
          class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          title="Quick commands (/)"
          aria-label="Quick commands"
          onclick={() => { inputText = '/'; slashQuery = ''; slashIdx = 0; slashOpen = true; inputRef?.focus(); void tick().then(resizeInput) }}
        ><Slash class="size-3.5" /></button>
        <span class="flex-1"></span>
        {#if loading}
          <button type="button" class="inline-flex size-7 shrink-0 items-center justify-center rounded-lg bg-foreground/[0.08] text-foreground transition-[background-color,scale] duration-150 hover:bg-foreground/15 active:scale-[0.96]" onclick={stop} title="Stop" aria-label="Stop">
            <Square class="size-2.5 fill-current" />
          </button>
        {:else}
          {@const ready = (inputText.trim() || attachedSql) && configured}
          <button type="button"
            class={cn('inline-flex size-7 shrink-0 items-center justify-center rounded-lg transition-[background-color,color,scale] duration-150', ready ? 'bg-primary text-primary-foreground hover:bg-primary/90 active:scale-[0.96]' : 'bg-foreground/[0.06] text-muted-foreground')}
            disabled={!ready}
            onclick={() => void send()}
            title="Send (Enter)"
            aria-label="Send"
          ><Send class="size-3.5 -translate-x-px translate-y-px" /></button>
        {/if}
      </div>
    </div>
  </div>
</div>
