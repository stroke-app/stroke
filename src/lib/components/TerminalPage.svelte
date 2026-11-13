<script>
  /**
   * The terminal tab: the connection's own command-line client (psql, mysql,
   * sqlite3, sqlcmd, redis-cli) in a real terminal, so every meta-command,
   * completion and setting works the way it does in the user's own shell.
   *
   * The client runs in a pseudo-terminal in the backend (db/terminal.rs) with
   * the saved credentials, and its bytes travel over a WebSocket on 127.0.0.1
   * (not Tauri IPC: on Linux that made every keystroke wait on the UI thread
   * twice). This draws it with xterm.js in the SQL editor's font on the theme's
   * colours, colours the error lines the clients print plain
   * (terminal-highlight.js), and suggests as you type (terminal-complete.js).
   */
  import { onDestroy, onMount, untrack } from 'svelte'
  import { Terminal } from '@xterm/xterm'
  import { FitAddon } from '@xterm/addon-fit'
  import { WebLinksAddon } from '@xterm/addon-web-links'
  import { SearchAddon } from '@xterm/addon-search'
  import Search from '@lucide/svelte/icons/search'
  import ChevronUp from '@lucide/svelte/icons/chevron-up'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import CaseSensitive from '@lucide/svelte/icons/case-sensitive'
  import '@xterm/xterm/css/xterm.css'
  import Command from '@lucide/svelte/icons/command'
  import X from '@lucide/svelte/icons/x'
  import RotateCw from '@lucide/svelte/icons/rotate-cw'
  import Eraser from '@lucide/svelte/icons/eraser'
  import Copy from '@lucide/svelte/icons/copy'
  import Check from '@lucide/svelte/icons/check'
  import PackageSearch from '@lucide/svelte/icons/package-search'
  import Ban from '@lucide/svelte/icons/ban'
  import CircleAlert from '@lucide/svelte/icons/circle-alert'
  import SquareSlash from '@lucide/svelte/icons/square-slash'
  import Hash from '@lucide/svelte/icons/hash'
  import Table2 from '@lucide/svelte/icons/table-2'
  import Columns3 from '@lucide/svelte/icons/columns-3'
  import SquareFunction from '@lucide/svelte/icons/square-function'
  import ArrowRightLeft from '@lucide/svelte/icons/arrow-right-left'
  import Kbd from './Kbd.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { appSqlEditor } from '$lib/stores/settings.js'
  import { sqlEditorFontSize } from '$lib/sql-editor-options.js'
  import { IS_MAC } from '$lib/shortcuts.js'
  import { detectOs } from '$lib/platform.js'
  import Icon from './Icon.svelte'
  import { terminalClient, terminalInstallClient, terminalOpen, terminalClose, executeSqlMulti, dynamoRequest } from '$lib/api.js'
  import { createSqlRepl } from '$lib/sql-repl.js'
  import { terminalTheme, terminalIsTranslucent, terminalFont } from '$lib/terminal-theme.js'
  import { createHighlighter } from '$lib/terminal-highlight.js'
  import { suggest, keystrokesFor } from '$lib/terminal-complete.js'
  import { lineEditorFor, clearLineKeys, consoleKeystrokes, consoleInput, consoleClearCommand } from '$lib/terminal-keys.js'
  import { wantsTerminator } from '$lib/sql-terminator.js'
  import ArrowDownToLine from '@lucide/svelte/icons/arrow-down-to-line'

  let {
    active = false,
    /** The connected saved connection, credentials included. @type {Record<string, any> | null} */
    connection = null,
    /** Its engine family (`postgres`, `mysql`, ...), which picks the client. */
    dbType = '',
    /** The schema's tables and columns, for suggestions (StudioShell's SQL hints). @type {(() => any) | undefined} */
    getHints = undefined,
    /** Docked under another page: a slimmer bar with a close button instead of the tab header. */
    docked = false,
    /** Start the session while still hidden, so it is logged in before it is shown. */
    prewarm = false,
    /** @type {(() => void) | undefined} */
    onclose = undefined,
  } = $props()

  /** @typedef {'idle' | 'checking' | 'unsupported' | 'missing' | 'failed' | 'starting' | 'running' | 'exited'} Phase */

  let phase = $state(/** @type {Phase} */ ('idle'))
  /** The CLI that is missing while the built-in prompt stands in for it. */
  let missingClient = $state(/** @type {{ name: string, hint?: string } | null} */ (null))
  let installing = $state(false)
  let installError = $state('')
  let client = $state(/** @type {import('$lib/api.js').TerminalClient | null} */ (null))
  /** Engines with no interactive CLI get the app's own SQL prompt instead. */
  const BUILTIN_REPL = new Set(['d1', 'dynamodb'])
  /** Engines whose CLI is not SQL: no SQL prompt to fall back to. */
  const NON_SQL = new Set(['redis', 'valkey', 'keydb', 'dragonfly', 'mongodb', 'couchdb', 'cassandra', 'firestore'])

  /** A result set from executeSqlMulti as the prompt's table shape. @param {string} sql */
  async function sqlRows(sql) {
    const r = (await executeSqlMulti(sql))?.[0] ?? {}
    return { columns: r.columns ?? [], rows: r.rows ?? [] }
  }
  /** `'x'` with quotes doubled. @param {string} v */
  const lit = (v) => `'${v.replace(/'/g, "''")}'`

  /** `.tables` / `.schema` for engines that are not SQLite-shaped. */
  function catalogFor(/** @type {string} */ type) {
    if (type === 'dynamodb') return { language: 'PartiQL', listTables: dynamoTables, describe: dynamoDescribe }
    if (type === 'mysql' || type === 'mariadb') return {
      listTables: () => sqlRows('SHOW TABLES'),
      describe: (/** @type {string} */ t) => sqlRows(`DESCRIBE \`${t.replace(/`/g, '``')}\``),
    }
    if (type === 'mssql') return {
      listTables: () => sqlRows("SELECT s.name + '.' + t.name AS [table] FROM sys.tables t JOIN sys.schemas s ON s.schema_id = t.schema_id ORDER BY 1"),
      describe: (/** @type {string} */ t) => sqlRows(`SELECT column_name, data_type, is_nullable, column_default FROM information_schema.columns WHERE table_name = ${lit(t)} ORDER BY ordinal_position`),
    }
    if (type === 'sqlite' || type === 'd1' || type === 'libsql') return {}
    // Postgres and its wire-compatible engines.
    return {
      listTables: () => sqlRows("SELECT CASE WHEN table_schema = current_schema() THEN table_name ELSE table_schema || '.' || table_name END AS table FROM information_schema.tables WHERE table_schema NOT IN ('pg_catalog', 'information_schema') ORDER BY table_schema <> current_schema(), 1"),
      describe: (/** @type {string} */ t) => sqlRows(`SELECT column_name, data_type, is_nullable, column_default FROM information_schema.columns WHERE table_name = ${lit(t.split('.').pop() ?? t)} ORDER BY ordinal_position`),
    }
  }

  /**
   * The app's own SQL prompt in this panel: for engines with no CLI, or when
   * the CLI is not installed. Nothing is spawned; statements run over the
   * connection the app already holds.
   * @param {string} [note] a dim first line, e.g. why the real client is not used
   */
  function startBuiltinPrompt(note) {
    if (!term || !connection) return
    void endSession()
    term.reset()
    client = { name: 'SQL prompt', path: null, version: null }
    repl = createSqlRepl({
      write: (s) => term?.write(s),
      cols: () => term?.cols ?? 120,
      run: (sql) => executeSqlMulti(sql),
      name: dbType === 'dynamodb' ? (connection.name || connection.region || 'dynamodb') : (connection.database || connection.name || dbType),
      historyKey: `stroke:sql-repl:${connectionKey}`,
      ...catalogFor(dbType),
    })
    phase = 'running'
    fit()
    if (note) term.write(`\x1b[2m${note}\x1b[0m\r\n`)
    repl.start()
    if (active) term.focus()
  }

  /** Install the missing CLI, then restart on it. */
  async function installClient() {
    if (!connection || installing) return
    installing = true
    installError = ''
    try {
      await terminalInstallClient({ ...connection, type: dbType })
      missingClient = null
      await start()
    } catch (e) {
      installError = String(e)
    } finally {
      installing = false
    }
  }

  /** DynamoDB: every table name, paging through ListTables. */
  async function dynamoTables() {
    /** @type {string[]} */
    const names = []
    /** @type {string | undefined} */
    let start
    do {
      const out = await dynamoRequest('ListTables', start ? { ExclusiveStartTableName: start } : {})
      names.push(...(out?.TableNames ?? []))
      start = out?.LastEvaluatedTableName
    } while (start)
    return { columns: [{ name: 'table' }], rows: names.map((n) => [n]) }
  }

  /** DynamoDB: a table's keys, indexes and size, one row per key or index. @param {string} table */
  async function dynamoDescribe(table) {
    const t = (await dynamoRequest('DescribeTable', { TableName: table }))?.Table ?? {}
    const types = Object.fromEntries((t.AttributeDefinitions ?? []).map((/** @type {any} */ a) => [a.AttributeName, a.AttributeType]))
    /** @param {any[]} keys */
    const keyText = (keys) => (keys ?? []).map((k) => `${k.AttributeName} (${k.KeyType === 'HASH' ? 'partition' : 'sort'}, ${types[k.AttributeName] ?? '?'})`).join(', ')
    const rows = [['table', table, keyText(t.KeySchema), `${t.ItemCount ?? '?'} items · ${t.TableStatus ?? ''}`]]
    for (const g of t.GlobalSecondaryIndexes ?? []) rows.push(['GSI', g.IndexName, keyText(g.KeySchema), g.Projection?.ProjectionType ?? ''])
    for (const l of t.LocalSecondaryIndexes ?? []) rows.push(['LSI', l.IndexName, keyText(l.KeySchema), l.Projection?.ProjectionType ?? ''])
    return { columns: [{ name: 'kind' }, { name: 'name' }, { name: 'key' }, { name: 'detail' }], rows }
  }
  /** @type {ReturnType<typeof createSqlRepl> | null} */
  let repl = null
  let hintCopied = $state(false)
  /** Why the client could not start, for the failure card. */
  let failure = $state('')

  /** @type {HTMLDivElement | undefined} */
  let termEl = $state()
  /** @type {Terminal | null} */
  let term = null
  /** @type {FitAddon | null} */
  let fitAddon = null
  /** @type {string | null} */
  let sessionId = null
  /** @type {WebSocket | null} */
  let socket = null
  /** @type {HTMLDivElement | undefined} */
  let bodyEl = $state()
  /** The connection the running (or last) session was started for. */
  let startedKey = ''
  /** Bumped by every start and stop; a callback from an older session drops out. */
  let generation = 0
  /** @type {ReturnType<typeof createHighlighter> | null} */
  let highlighter = null
  let themeKey = ''

  // Identity of the connection, minus secrets: a change means a new session.
  const connectionKey = $derived(
    connection
      ? JSON.stringify([dbType, connection.host, connection.port, connection.database, connection.user, connection.filePath, connection.db])
      : '',
  )

  /** Where the client points, for the header: `user@host:port/db`, a file name, ... */
  const target = $derived.by(() => {
    if (!connection) return ''
    if (connection.filePath) return String(connection.filePath).split(/[\\/]/).pop() ?? ''
    const host = connection.host ? `${connection.host}${connection.port ? `:${connection.port}` : ''}` : ''
    const user = connection.user ? `${connection.user}@` : ''
    const db = connection.database ? `/${connection.database}` : connection.db ? `/${connection.db}` : ''
    return `${user}${host}${db}`
  })

  /**
   * The header's visible label: `host:port/db`, without the user. psql's own
   * prompt already says who you are, and some hosts make the user a key - a
   * Prisma Postgres user is 64 hex characters - which filled the header with a
   * secret-looking string and pushed the database name off the end. The full
   * `user@host/db` stays in the tooltip.
   */
  const targetShort = $derived.by(() => {
    if (!connection || connection.filePath) return target
    const user = connection.user ? `${connection.user}@` : ''
    return user && target.startsWith(user) ? target.slice(user.length) : target
  })

  // The common-commands strip is opt-in: a terminal is the screen and a
  // prompt. Remembered across sessions.
  const QUICK_BAR_KEY = 'stroke:terminal-quick-bar'
  let showQuickBar = $state((() => { try { return localStorage.getItem(QUICK_BAR_KEY) === '1' } catch { return false } })())
  function toggleQuickBar() {
    showQuickBar = !showQuickBar
    try { localStorage.setItem(QUICK_BAR_KEY, showQuickBar ? '1' : '0') } catch { /* only the preference is lost */ }
    requestAnimationFrame(() => term?.focus())
  }

  const IS_WINDOWS = detectOs() === 'windows'
  /** How the client edits its line, which decides how the page's edits are spelled (terminal-keys.js). */
  const lineEditor = $derived(lineEditorFor(client, detectOs()))

  /**
   * One-click commands for the footer, per client: each types its command at
   * the prompt and runs it, after emptying whatever is half-typed there.
   * @type {Record<string, Array<{ cmd: string, label: string }>>}
   */
  const QUICK = {
    psql: [
      { cmd: '\\l', label: 'databases' },
      { cmd: '\\dn', label: 'schemas' },
      { cmd: '\\dt', label: 'tables' },
      { cmd: '\\dv', label: 'views' },
      { cmd: '\\df', label: 'functions' },
      { cmd: '\\du', label: 'roles' },
      { cmd: '\\x', label: 'expanded' },
      { cmd: '\\timing', label: 'timing' },
      { cmd: '\\?', label: 'help' },
    ],
    mysql: [
      { cmd: 'SHOW DATABASES;', label: 'databases' },
      { cmd: 'SHOW TABLES;', label: 'tables' },
      { cmd: 'SHOW PROCESSLIST;', label: 'processes' },
      { cmd: 'STATUS', label: 'status' },
      { cmd: 'HELP', label: 'help' },
    ],
    sqlite3: [
      { cmd: '.tables', label: 'tables' },
      { cmd: '.schema', label: 'schema' },
      { cmd: '.indexes', label: 'indexes' },
      { cmd: '.mode box', label: 'box output' },
      { cmd: '.timer on', label: 'timing' },
      { cmd: '.help', label: 'help' },
    ],
    'redis-cli': [
      { cmd: 'INFO keyspace', label: 'keyspace' },
      { cmd: 'DBSIZE', label: 'keys' },
      { cmd: 'SCAN 0 COUNT 20', label: 'scan' },
      { cmd: 'CLIENT LIST', label: 'clients' },
      { cmd: 'HELP', label: 'help' },
    ],
  }
  QUICK.mariadb = QUICK.mysql
  QUICK['valkey-cli'] = QUICK['redis-cli']
  const quickCommands = $derived(QUICK[client?.name ?? ''] ?? [])

  /** @param {string} cmd */
  function runQuick(cmd) {
    if (!sessionId) return
    send(`${clearLineKeys(lineEditor)}${cmd}\r`)
    term?.focus()
  }

  /** Just the number from the `--version` line: `psql (PostgreSQL) 17.2` is `17.2`. */
  const shortVersion = $derived(client?.version?.match(/\d+\.\d+(?:\.\d+)?(?:-[A-Za-z]+)?/)?.[0] ?? '')

  /** The target split for the header: everything muted except the database. */
  const targetParts = $derived.by(() => {
    if (!connection) return { lead: '', db: '' }
    if (connection.filePath) return { lead: '', db: targetShort }
    const db = connection.database ? String(connection.database) : connection.db ? String(connection.db) : ''
    return { lead: db ? targetShort.slice(0, targetShort.length - db.length) : targetShort, db }
  })

  const installLooksLikeCommand = $derived(/^(brew|winget|sudo|apt|pacman|dnf) /.test(client?.hint ?? ''))

  // ── Session ────────────────────────────────────────────────────────────────

  async function start() {
    if (!term || !connection) return
    const gen = ++generation
    // Set before the first await, or the effects that call sync() in the
    // meantime see no session for this connection and start a second one.
    const sameConnection = startedKey === connectionKey
    startedKey = connectionKey
    phase = 'checking'
    await endSession()
    if (gen !== generation || !term) return
    const config = { ...connection, type: dbType }
    // A restart on the same connection keeps the scrollback; a new connection
    // starts on a clean screen.
    if (sameConnection && !docked) term.write('\r\n')
    else term.reset()
    if (BUILTIN_REPL.has(dbType)) { startBuiltinPrompt(); return }
    let info
    try {
      info = await terminalClient(config)
    } catch (e) {
      if (gen !== generation) return
      showFailure(String(e))
      return
    }
    if (gen !== generation) return
    client = info
    if (!info.name) { phase = 'unsupported'; return }
    if (!info.path) {
      // A SQL engine works without its CLI: drop straight into the built-in
      // prompt rather than stopping on a "not installed" page, and offer the
      // install alongside. Engines with no built-in prompt keep the page.
      if (!NON_SQL.has(dbType)) {
        missingClient = { name: info.name, hint: info.hint }
        startBuiltinPrompt(`${info.name} is not installed, so this is the built-in prompt. Install ${info.name} (corner button) for its own commands.`)
        return
      }
      phase = 'missing'
      return
    }
    missingClient = null

    phase = 'starting'
    fit()
    // Docked, the prompt owns the first row: the shell's own output is the
    // only thing in the panel, with the actions floating over the corner.
    if (!docked) term.write(`\x1b[2mConnecting to ${target} with ${info.name}\x1b[0m\r\n`)
    const highlight = createHighlighter((bytes) => term?.write(bytes), { client: info.name })
    highlighter = highlight
    resetPromptState()
    // Every column of the schema, for suggestions; the SQL editor loads them the same way.
    void getHints?.()?.loadColumns?.()
    let session
    try {
      session = await terminalOpen(config, term.cols, term.rows)
    } catch (e) {
      if (gen === generation) showFailure(String(e))
      return
    }
    if (gen !== generation) { void terminalClose(session.id).catch(() => {}); return }
    sessionId = session.id
    const ws = new WebSocket(session.url)
    ws.binaryType = 'arraybuffer'
    socket = ws
    let opened = false
    ws.onopen = () => {
      if (gen !== generation) return
      opened = true
      phase = 'running'
      flushInput()
      sendResize()
      if (active) term?.focus()
    }
    ws.onmessage = (e) => {
      if (gen !== generation) return
      if (typeof e.data === 'string') {
        try {
          const msg = JSON.parse(e.data)
          if (msg && 'exit' in msg) clientExited(info.name, msg.exit)
        } catch { /* not a control message */ }
        return
      }
      noteEcho()
      highlight.push(new Uint8Array(e.data))
    }
    ws.onclose = () => {
      if (gen !== generation || phase === 'exited') return
      // Closed with no exit message: the session was killed, or the app is
      // restarting under the page.
      if (opened) clientExited(info.name, null)
      else showFailure(`Could not connect to ${info.name}'s terminal session.`)
    }
  }

  /** @param {string} name @param {number | null} code */
  function clientExited(name, code) {
    highlighter?.flush()
    sessionId = null
    socket = null
    phase = 'exited'
    hideSuggest()
    continuing = false
    const how = code ? ` with code ${code}` : ''
    term?.write(`\r\n\x1b[2m${name} exited${how}. Press Enter to start it again.\x1b[0m\r\n`)
  }

  /** @param {string} message */
  function showFailure(message) {
    failure = message.replace(/^Error:\s*/, '')
    phase = 'failed'
  }

  async function endSession() {
    highlighter?.flush()
    highlighter = null
    repl = null
    hideSuggest()
    const id = sessionId
    sessionId = null
    const ws = socket
    socket = null
    // Closing the socket ends the client too; the close below makes sure.
    if (ws) { ws.onclose = null; ws.close() }
    if (id) await terminalClose(id).catch(() => {})
  }

  function stop() {
    generation++
    startedKey = ''
    phase = 'idle'
    void endSession()
  }

  // ── Input ──────────────────────────────────────────────────────────────────

  const encoder = new TextEncoder()
  /** Typed before the socket opened. */
  let queuedInput = ''

  /** Keystrokes to the client. A WebSocket keeps them in order by itself. @param {string} data */
  function send(data) {
    if (socket?.readyState === WebSocket.OPEN) socket.send(encoder.encode(data))
    else if (socket?.readyState === WebSocket.CONNECTING) queuedInput += data
  }

  function flushInput() {
    if (!queuedInput) return
    const data = queuedInput
    queuedInput = ''
    send(data)
  }

  function sendResize() {
    if (socket?.readyState === WebSocket.OPEN && term) {
      socket.send(JSON.stringify({ resize: [term.cols, term.rows] }))
      lastSent = `${term.cols}x${term.rows}`
    }
  }
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let resizeSendTimer
  let lastSent = ''
  /** The PTY hears about a new size once, after it stops changing. */
  function sendResizeSoon() {
    clearTimeout(resizeSendTimer)
    resizeSendTimer = setTimeout(() => {
      if (!term) return
      const key = `${term.cols}x${term.rows}`
      if (key === lastSent) return
      lastSent = key
      sendResize()
    }, 140)
  }

  // Keystroke-to-echo latency, written to the app log every 40 keys: the number
  // to look at when typing feels slow.
  let echoSentAt = 0
  /** @type {number[]} */
  let echoSamples = []
  function noteKeystroke() {
    if (!echoSentAt) echoSentAt = performance.now()
  }
  /** @type {number[]} */
  let paintSamples = []
  function noteEcho() {
    if (!echoSentAt) return
    const sentAt = echoSentAt
    echoSamples.push(performance.now() - sentAt)
    echoSentAt = 0
    // And to the screen: two frames later the echo has been drawn.
    requestAnimationFrame(() => requestAnimationFrame(() => {
      paintSamples.push(performance.now() - sentAt)
      if (paintSamples.length < 40) return
      /** @param {number[]} xs */
      const stats = (xs) => {
        const sorted = [...xs].sort((x, y) => x - y)
        const at = (/** @type {number} */ p) => sorted[Math.min(sorted.length - 1, Math.floor(p * sorted.length))].toFixed(1)
        return `p50 ${at(0.5)}ms, p95 ${at(0.95)}ms, max ${at(1)}ms`
      }
      const renderer = 'dom'
      socket?.send(JSON.stringify({ trace: `keystroke to echo: ${stats(echoSamples)}; to screen: ${stats(paintSamples)} (${renderer})` }))
      echoSamples = []
      paintSamples = []
    }))
  }

  /** @param {string} data what xterm says was typed */
  function onTyped(data) {
    if (phase === 'exited') {
      if (data.includes('\r')) void start()
      return
    }
    if (repl) { repl.input(data); return }
    if (!sessionId) return
    if (lineEditor === 'console') {
      data = consoleInput(data)
      if (!data) return
    }
    if (inputSelected) {
      // Ctrl+A selected the typed line: typing replaces it, Backspace or Delete clears it.
      inputSelected = false
      term?.clearSelection()
      const clear = clearLineKeys(lineEditor)
      if (data === '\x7f' || data === '\x1b[3~') { send(clear); return }
      if (/^[^\x00-\x1f\x7f]+$/.test(data)) data = clear + data
    }
    if (data === '\r') {
      hideSuggest()
      lastTypedAt = 0
    } else if (data === '\x7f' || /^[^\x00-\x1f\x7f]+$/.test(data)) {
      lastTypedAt = performance.now()
      if (data.length === 1) noteKeystroke()
    } else {
      // Arrows, history, Ctrl+keys: the line is somewhere else now.
      hideSuggest()
      lastTypedAt = 0
    }
    send(data)
  }

  // ── Prompt and suggestions ─────────────────────────────────────────────────

  /** Where typing starts at psql's prompt (its OSC 133;B mark): absolute buffer row and column. */
  let inputStart = /** @type {{ row: number, col: number } | null} */ (null)
  /** Earlier lines of a statement psql is still collecting (its `-#` prompt). */
  let statementLines = /** @type {string[]} */ ([])
  /** psql is at its continuation prompt: the statement has no `;` yet. */
  let continuing = $state(false)

  function resetPromptState() {
    inputStart = null
    statementLines = []
    continuing = false
  }

  /** Prompts of the clients that cannot be given marks, to find the input after them. */
  const PROMPT_RE = /** @type {Record<string, RegExp>} */ ({
    // psql where its marks are not read (Windows): the prompts db/terminal.rs sets, `db=> ` and `db-> `.
    psql: /^[^\s=^!]+?[=^!\-'"($*][*!?]?[#>] /,
    // MariaDB's client names the server it is on: `MySQL [db]> ` against MySQL.
    mysql: /^(?:mysql|(?:MariaDB|MySQL) \[[^\]]*\])> |^\s+-> /,
    mariadb: /^(?:mysql|(?:MariaDB|MySQL) \[[^\]]*\])> |^\s+-> /,
    sqlite3: /^sqlite> |^\s*\.\.\.> /,
    'redis-cli': /^[^\s>]+> /,
    'valkey-cli': /^[^\s>]+> /,
    sqlcmd: /^\d+> /,
    // `test> `, `rs0 [direct: primary] app> `, `Atlas atlas-x [primary] app> `.
    mongosh: /^[^>\n]*?> /,
    mongo: /^[^>\n]*?> /,
  })

  /** The same clients' continuation prompts: a statement is still open. */
  const CONTINUATION_RE = /** @type {Record<string, RegExp>} */ ({
    psql: /^[^\s=^!]+?[-'"($*][*!?]?[#>] /,
    mysql: /^\s*(?:->|'>|">|`>|\/\*>) /,
    mariadb: /^\s*(?:->|'>|">|`>|\/\*>) /,
    sqlite3: /^\s*\.\.\.> /,
  })

  /** Clients whose statements end with `;`, and so get Enter that runs a finished one. */
  const SEMICOLON_CLIENTS = new Set(['psql', 'mysql', 'mariadb', 'sqlite3'])

  /**
   * Enter at a SQL prompt. A statement that looks finished runs: the `;` is
   * typed for it, the way the SQL editor offers it (sql-terminator.js). Enter on
   * the empty line of a statement left open runs what is there, so a forgotten
   * `;` never leaves the prompt stuck. Anything unfinished (after FROM, a comma,
   * an open bracket or string), and Shift+Enter always, is a new line.
   * @param {boolean} newLine Shift+Enter
   * @returns {boolean} false: not at a prompt, let the client have the key
   */
  function pressEnter(newLine) {
    if (!client || !SEMICOLON_CLIENTS.has(client.name)) return false
    const line = currentInput()
    if (line === null) return false
    const trimmed = line.trim()
    const statement = [...statementLines, line].join('\n')
    const isCommand = !statementLines.length && (trimmed.startsWith('\\') || (client.name === 'sqlite3' && trimmed.startsWith('.')))
    const balanced = !/\$\$/.test(statement) && (statement.match(/'/g)?.length ?? 0) % 2 === 0
    const run = !newLine && !isCommand && (
      wantsTerminator(statement)
      || (!trimmed && statementLines.length > 0 && balanced && !statement.trim().endsWith(';'))
    )
    if (!run && !isCommand && trimmed) statementLines.push(line)
    hideSuggest()
    lastTypedAt = 0
    send(run ? ';\r' : '\r')
    return true
  }

  /** Keep `continuing` true while mysql or sqlite3 shows its continuation prompt (psql says so in its marks). */
  function trackPrompt() {
    if (!term || !client || inputStart) return
    const cont = CONTINUATION_RE[client.name]
    const primary = PROMPT_RE[client.name]
    if (!cont || !primary) return
    const buf = term.buffer.active
    if (buf.type !== 'normal') return
    const text = buf.getLine(buf.baseY + buf.cursorY)?.translateToString(true) ?? ''
    if (cont.test(text)) {
      if (!continuing) continuing = true
    } else if (primary.test(text)) {
      if (continuing) continuing = false
      statementLines = []
    }
  }

  /** What has been typed at the prompt, up to the cursor; null when the cursor is not at one. */
  function currentInput() {
    if (!term) return null
    const buf = term.buffer.active
    // The alternate screen is a pager or an editor, not the prompt.
    if (buf.type !== 'normal') return null
    const row = buf.baseY + buf.cursorY
    if (inputStart) {
      if (row < inputStart.row) return null
      let text = ''
      for (let r = inputStart.row; r <= row; r++) {
        const line = buf.getLine(r)
        // A row that is not a wrap of the one above: output has moved past the prompt.
        if (!line || (r > inputStart.row && !line.isWrapped)) return null
        text += line.translateToString(false, r === inputStart.row ? inputStart.col : 0, r === row ? buf.cursorX : undefined)
      }
      return text
    }
    const re = PROMPT_RE[client?.name ?? '']
    if (!re) return null
    const line = buf.getLine(row)?.translateToString(false, 0, buf.cursorX) ?? ''
    const m = line.match(re)
    return m ? line.slice(m[0].length) : null
  }

  let suggestItems = $state(/** @type {import('$lib/terminal-complete.js').Suggestion[]} */ ([]))
  let suggestIndex = $state(0)
  let suggestPos = $state({ left: 0, top: 0, bottom: 0, above: false })
  let suggestToken = ''
  /** Suggestions follow typing only; output on its own never opens them. */
  let lastTypedAt = 0
  let suggestFrame = 0
  /** @type {any} */
  let hintsCache = null
  let hintsAt = 0

  function schemaHints() {
    const now = performance.now()
    if (!hintsCache || now - hintsAt > 3000) {
      hintsCache = getHints?.() ?? {}
      hintsAt = now
    }
    return hintsCache
  }

  function hideSuggest() {
    if (suggestItems.length) suggestItems = []
  }

  /** @param {boolean} [force] Ctrl+Space: open even without fresh typing */
  function updateSuggest(force = false) {
    if (!term || phase !== 'running' || !client) return hideSuggest()
    if (!force && performance.now() - lastTypedAt > 800) return hideSuggest()
    const line = currentInput()
    if (line === null) return hideSuggest()
    const { items, token } = suggest({ client: client.name, line, statement: statementLines.join('\n'), hints: schemaHints(), editor: lineEditor })
    if (!items.length) return hideSuggest()
    suggestToken = token
    suggestItems = items
    suggestIndex = 0
    placeSuggest(token)
  }

  /** Under the word being typed, or above it when the cursor is near the bottom. @param {string} token */
  function placeSuggest(token) {
    const screen = /** @type {HTMLElement | null | undefined} */ (term?.element?.querySelector('.xterm-screen'))
    if (!term || !screen || !bodyEl) return
    const s = screen.getBoundingClientRect()
    const h = bodyEl.getBoundingClientRect()
    const cellW = s.width / term.cols
    const cellH = s.height / term.rows
    const buf = term.buffer.active
    // The labels line up with the word being typed: the row's padding, icon and
    // gap (4 + 8 + 12 + 8 px) sit to its left.
    const left = s.left - h.left + Math.max(0, buf.cursorX - token.length) * cellW - 32
    const lineTop = s.top - h.top + buf.cursorY * cellH
    const listHeight = suggestItems.length * 24 + 10
    const above = lineTop + cellH + listHeight > h.height && lineTop > h.height / 2
    suggestPos = {
      left: Math.max(4, Math.min(left, h.width - 300)),
      top: lineTop + cellH + 2,
      bottom: h.height - lineTop + 2,
      above,
    }
  }

  /** @param {number} [i] */
  function acceptSuggestion(i = suggestIndex) {
    const item = suggestItems[i]
    if (!item) return
    hideSuggest()
    lastTypedAt = 0
    send(keystrokesFor(item, suggestToken, lineEditor))
    term?.focus()
  }

  /**
   * Keys the open list takes before the client sees them. Enter and Tab take
   * the highlighted item; Enter on an exact match (`\\dt` typed in full) is
   * not taken, so it runs the line.
   * @param {KeyboardEvent} e
   */
  function suggestKey(e) {
    const n = suggestItems.length
    switch (e.key) {
      case 'ArrowDown': suggestIndex = (suggestIndex + 1) % n; return true
      case 'ArrowUp': suggestIndex = (suggestIndex - 1 + n) % n; return true
      case 'Tab': acceptSuggestion(); return true
      case 'Escape': hideSuggest(); return true
      case 'Enter':
        if (e.shiftKey || suggestItems[suggestIndex]?.exact) { hideSuggest(); return false }
        acceptSuggestion()
        return true
    }
    return false
  }

  const KIND_ICON = {
    meta: SquareSlash, command: SquareSlash, keyword: Hash, table: Table2,
    column: Columns3, function: SquareFunction, translate: ArrowRightLeft,
  }
  /** Each kind its own colour, as in the sidebar tree. */
  const KIND_COLOR = {
    meta: 'text-primary', command: 'text-primary', keyword: 'text-muted-foreground', table: 'text-info',
    column: 'text-success', function: 'text-warning', translate: 'text-primary',
  }

  // ── Scrolling ──────────────────────────────────────────────────────────────

  /** The view is scrolled up from the latest output: offer the way back down. */
  let scrolledUp = $state(false)

  // ── Find in scrollback (⌘F / Ctrl+Shift+F) ─────────────────────────────────
  /** @type {SearchAddon | null} */
  let searchAddon = null
  let findOpen = $state(false)
  let findQuery = $state('')
  let findCase = $state(false)
  /** Current match (1-based) and total; -1 total means "more than xterm counts". */
  let findIndex = $state(0)
  let findTotal = $state(0)
  /** @type {HTMLInputElement | null} */
  let findInput = $state(null)

  /**
   * Match highlights. xterm only takes opaque `#RRGGBB` here (anything else is
   * silently dropped, which is why matches were counted but never drawn), so
   * each colour is the terminal's own blue or yellow blended into its
   * background: a tint the text stays readable on, in every theme.
   */
  function findDecorations() {
    const t = termEl ? terminalTheme(termEl) : null
    const bg = /^#[0-9a-f]{6}/i.test(t?.background ?? '') && !/^#0{6}00$/i.test(t?.background ?? '') ? t.background.slice(0, 7) : (t?.cursorAccent ?? '#0a0a0a').slice(0, 7)
    /** @param {string} a @param {string} b @param {number} k */
    const mix = (a, b, k) => {
      const p = (/** @type {string} */ h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16))
      const [x, y] = [p(a), p(b)]
      return '#' + x.map((v, i) => Math.round(v + (y[i] - v) * k).toString(16).padStart(2, '0')).join('')
    }
    const blue = (t?.blue ?? '#3b82f6').slice(0, 7)
    const yellow = (t?.yellow ?? '#eab308').slice(0, 7)
    return {
      matchBackground: mix(bg, blue, 0.35),
      matchBorder: mix(bg, blue, 0.6),
      matchOverviewRuler: blue,
      activeMatchBackground: mix(bg, yellow, 0.55),
      activeMatchBorder: yellow,
      activeMatchColorOverviewRuler: yellow,
    }
  }

  /** @param {'next' | 'prev'} dir @param {boolean} [incremental] */
  function findStep(dir, incremental = false) {
    if (!searchAddon) return
    if (!findQuery) { searchAddon.clearDecorations(); findIndex = 0; findTotal = 0; return }
    const opts = { caseSensitive: findCase, incremental, decorations: findDecorations() }
    if (dir === 'next') searchAddon.findNext(findQuery, opts)
    else searchAddon.findPrevious(findQuery, opts)
  }

  function openFind() {
    findOpen = true
    // Seed with the selection, as editors do.
    const sel = term?.getSelection()?.trim()
    if (sel && !sel.includes('\n') && sel.length <= 200) findQuery = sel
    queueMicrotask(() => { findInput?.focus(); findInput?.select(); if (findQuery) findStep('next', true) })
  }

  function closeFind() {
    findOpen = false
    searchAddon?.clearDecorations()
    term?.clearSelection()
    findIndex = 0
    findTotal = 0
    term?.focus()
  }

  /** @param {KeyboardEvent} e */
  function onFindKey(e) {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); closeFind() }
    else if (e.key === 'Enter') { e.preventDefault(); findStep(e.shiftKey ? 'prev' : 'next') }
  }

  /** ⌘F on macOS; Ctrl+Shift+F elsewhere, since plain Ctrl+F is readline's forward-char. @param {KeyboardEvent} e */
  function isFindChord(e) {
    if (e.type !== 'keydown' || e.altKey || e.key.toLowerCase() !== 'f') return false
    return IS_MAC ? e.metaKey && !e.ctrlKey && !e.shiftKey : e.ctrlKey && e.shiftKey && !e.metaKey
  }
  function trackScroll() {
    if (!term) return
    const buf = term.buffer.active
    const up = buf.viewportY < buf.baseY
    if (up !== scrolledUp) scrolledUp = up
  }
  function scrollToLatest() {
    term?.scrollToBottom()
    scrolledUp = false
    term?.focus()
  }

  // ── Terminal ───────────────────────────────────────────────────────────────

  function fit() {
    if (!fitAddon || !termEl || termEl.clientWidth === 0) return
    try { fitAddon.fit() } catch { /* not laid out yet */ }
  }

  /**
   * Make xterm measure its cells again. It caches the character size from the
   * first measure; if the editor web font was still loading then, it measured
   * the narrower fallback and every later fit thought more columns and rows
   * fit than do: lines ran off the right edge, the client wrapped its output
   * at the wrong width, and the last row hid under the bottom edge.
   */
  async function remeasure() {
    if (!term || !termEl) return
    const { fontFamily, fontSize } = terminalFont(termEl)
    try { await document.fonts?.load(`${fontSize}px ${fontFamily}`) } catch { /* no FontFaceSet */ }
    if (!term) return
    // Changing the option is what triggers xterm's re-measure; set it away and back.
    term.options.fontFamily = 'monospace'
    term.options.fontFamily = fontFamily
    fit()
  }

  /** Font and colours from the editor setting and the app theme. */
  function applyAppearance() {
    if (!term || !termEl) return
    const { fontFamily, fontSize } = terminalFont(termEl)
    const fontChanged = term.options.fontFamily !== fontFamily || term.options.fontSize !== fontSize
    if (term.options.fontFamily !== fontFamily) term.options.fontFamily = fontFamily
    if (term.options.fontSize !== fontSize) term.options.fontSize = fontSize
    if (fontChanged) void remeasure()
    const theme = terminalTheme(termEl)
    const key = JSON.stringify(theme)
    // <html> attributes change for more than the theme; repaint only when it did.
    if (key !== themeKey) {
      themeKey = key
      const clear = terminalIsTranslucent(termEl)
      if (term.options.allowTransparency !== clear) term.options.allowTransparency = clear
      term.options.theme = theme
    }
    fit()
  }

  /**
   * Copy and paste where a terminal user expects them, without taking Ctrl+C,
   * and the app's own shortcuts kept reachable from inside the terminal.
   * Returning false hands the key to the browser and the app instead of xterm.
   */
  /**
   * Clients whose line editor knows the Meta word keys (ESC DEL, ESC b, ESC f,
   * ESC d): readline and editline (psql, mysql, sqlite3), Node's readline
   * (mongosh) and the Go readline in the Turso shell.
   */
  const READLINE_CLIENTS = new Set(['psql', 'mysql', 'mariadb', 'sqlite3', 'mongosh', 'mongo', 'turso'])
  /**
   * redis-cli's linenoise: Ctrl+W deletes the word before the cursor, Ctrl+U the
   * line, Ctrl+A / Ctrl+E jump to its ends, and it has no word-jump keys at all.
   */
  const LINENOISE_CLIENTS = new Set(['redis-cli', 'valkey-cli', 'memurai-cli'])

  /** Cmd+Left, Cmd+Right and Cmd+Backspace, as in a macOS text field: start and end of the line, delete back to its start. */
  const MAC_COMMAND_KEYS = /** @type {Record<string, string>} */ ({ ArrowLeft: '\x01', ArrowRight: '\x05', Backspace: '\x15' })

  /**
   * The editing keys people bring from an editor, spelled the way the client's
   * line editor understands them. xterm sends Ctrl+Backspace as ^H (one
   * character) and Ctrl+Left as a sequence readline may not be bound to, so
   * they did nothing useful; these are the same keystrokes VS Code's terminal
   * sends. Ctrl+Z is undo here, not suspend: a suspended psql would hang the tab.
   * The Windows console takes its own keys (terminal-keys.js).
   * @param {KeyboardEvent} e
   * @returns {string | null} null leaves the key to xterm; '' drops it
   */
  function editorKeystrokes(e) {
    if (e.shiftKey) return null
    if (e.metaKey) return IS_MAC && !e.ctrlKey && !e.altKey ? MAC_COMMAND_KEYS[e.key] ?? null : null
    if (lineEditor === 'console') return consoleKeystrokes(e)
    // The built-in prompt (sql-repl.js) has its own line editor that speaks
    // the same word keys as readline.
    const word = !!repl || READLINE_CLIENTS.has(client?.name ?? '')
    const mod = e.ctrlKey || e.altKey
    if (LINENOISE_CLIENTS.has(client?.name ?? '')) {
      switch (e.key) {
        case 'Backspace': return mod ? '\x17' : null
        // No word motion in linenoise: drop the key rather than print escape junk.
        case 'ArrowLeft': case 'ArrowRight': case 'Delete': return mod ? '' : null
        case 'Home': return mod ? null : '\x01'
        case 'End': return mod ? null : '\x05'
      }
      return null
    }
    switch (e.key) {
      // On macOS these clients usually link editline, where ^W cuts back to
      // the start of the line; ESC DEL deletes a word there and in readline alike.
      case 'Backspace': return e.ctrlKey ? (IS_MAC && word ? '\x1b\x7f' : '\x17') : e.altKey && word ? '\x1b\x7f' : null
      case 'Delete': return mod && word ? '\x1bd' : null
      case 'ArrowLeft': return mod && word ? '\x1bb' : null
      case 'ArrowRight': return mod && word ? '\x1bf' : null
      case 'Home': return mod ? null : '\x01'
      case 'End': return mod ? null : '\x05'
      case 'z': return e.ctrlKey && !e.altKey && word ? '\x1f' : null
    }
    return null
  }

  /**
   * Ctrl combos the app keeps even inside the terminal (Linux and Windows):
   * the command palette (Ctrl+K), close tab (Ctrl+W), new tab, the tables
   * list, save, the AI panel, settings, tabs by number, zoom. The client loses
   * nothing it needs: each of these letters is readline's duplicate of an arrow
   * or of Tab and Enter, or something nobody wants in a terminal (Ctrl+S
   * freezes the output until Ctrl+Q). The terminal keeps the keys that make it
   * one: C (cancel, or copy a selection), D (quit), R (search history), L
   * (clear), U, E and A (the line), Z (undo), V (paste), X and G (readline's
   * prefix and cancel), and F and H, whose app versions act on the table tabs.
   */
  const APP_CTRL_KEYS = new Set([
    'b', 'i', 'k', 'm', 'n', 'p', 's', 't', 'w', ',', '/', '?', '=', '-', '+',
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'enter', 'tab', 'pageup', 'pagedown',
  ])

  /** Ctrl+A selected the typed line; the next key replaces it. */
  let inputSelected = false

  /**
   * Ctrl+A, as in an editor: select what is typed at the prompt (Ctrl+C then
   * copies it, typing replaces it). With nothing typed, select all the output.
   * Home still goes to the start of the line.
   */
  function selectAll() {
    if (!term) return
    const typed = typedLine()
    if (!typed?.text) { term.selectAll(); return }
    term.select(typed.start.col, typed.start.row, typed.text.length)
    inputSelected = true
  }

  /**
   * The whole typed line, not just up to the cursor: from the prompt to the
   * end of the last row it wraps onto. null when the cursor is not at a prompt.
   */
  function typedLine() {
    const typed = currentInput()
    if (typed === null || !term) return null
    const buf = term.buffer.active
    const start = inputStart ?? { row: buf.baseY + buf.cursorY, col: buf.cursorX - typed.length }
    let text = ''
    for (let r = start.row; ; r++) {
      const line = buf.getLine(r)
      if (!line || (r > start.row && !line.isWrapped)) break
      text += line.translateToString(false, r === start.row ? start.col : 0)
    }
    return { start, text: text.trimEnd() }
  }

  /**
   * Ctrl+L at the Windows console, which has no clear-screen key: the client
   * runs `cls` instead, when no typed line would be lost to it.
   */
  function clearConsole() {
    if (repl) { repl.input('\x0c'); return }
    const cls = consoleClearCommand(client?.name ?? '')
    if (cls && !continuing && typedLine()?.text === '') send(`${cls}\r`)
  }

  /** Tab at the Windows console, which completes nothing: the suggestions open, and a lone one is typed at once. */
  function completeAtCursor() {
    updateSuggest(true)
    if (suggestItems.length === 1 && !suggestItems[0].run) acceptSuggestion(0)
  }

  function handleKey(/** @type {KeyboardEvent} */ e) {
    if (isFindChord(e)) {
      e.preventDefault()
      e.stopPropagation()
      openFind()
      return false
    }
    if (e.type === 'keydown' && consumeKey(e)) {
      e.preventDefault()
      // Handled here: the app's own shortcuts on the same key (Escape, Alt+Backspace) must not run as well.
      e.stopPropagation()
      return false
    }
    // The app's shortcuts: hand the key back so it bubbles to them, unsent.
    if (e.type === 'keydown' && !IS_MAC && e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey && APP_CTRL_KEYS.has(e.key.toLowerCase())) {
      return false
    }
    return handleClipboardKey(e)
  }

  /** Keys the page handles itself; true when it did. @param {KeyboardEvent} e */
  function consumeKey(e) {
    // The built-in prompt edits its own line: Tab completes there, and the
    // PTY clients' suggestion keys, Enter handling and line shortcuts stay out.
    if (repl) {
      if (e.key === 'Tab' && !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey) { repl.input('\t'); return true }
      return false
    }
    if (phase === 'running') {
      const keys = editorKeystrokes(e)
      if (keys !== null) {
        if (inputSelected) { inputSelected = false; term?.clearSelection() }
        hideSuggest()
        if (keys) send(keys)
        return true
      }
      if (!IS_MAC && e.ctrlKey && !e.altKey && !e.shiftKey && e.key.toLowerCase() === 'a') {
        selectAll()
        return true
      }
      if (lineEditor === 'console' && e.ctrlKey && !e.altKey && !e.shiftKey && e.key.toLowerCase() === 'l') {
        clearConsole()
        return true
      }
    }
    if (suggestItems.length && !e.ctrlKey && !e.altKey && !e.metaKey && suggestKey(e)) return true
    if (e.key === 'Tab' && !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey && phase === 'running' && lineEditor === 'console') {
      completeAtCursor()
      return true
    }
    if (e.key === 'Enter' && !e.ctrlKey && !e.altKey && !e.metaKey && phase === 'running' && pressEnter(e.shiftKey)) return true
    // Ctrl+Space asks for suggestions right where the cursor is.
    if (e.ctrlKey && e.code === 'Space') {
      updateSuggest(true)
      return true
    }
    return false
  }

  /** Copy, paste and the app's Ctrl+` toggle. @param {KeyboardEvent} e */
  function handleClipboardKey(e) {
    if (e.type !== 'keydown' || !e.ctrlKey || e.altKey) return true
    const key = e.key.toLowerCase()
    // Ctrl+` is the app's: it toggles between this tab and the last one.
    if (key === '`') return false
    if (IS_MAC) return true
    // Ctrl+Shift+C always copies; plain Ctrl+C copies only over a selection,
    // and otherwise stays the interrupt that cancels a running query.
    if (key === 'c' && (e.shiftKey || term?.hasSelection())) {
      const text = term?.getSelection() ?? ''
      if (text) void navigator.clipboard.writeText(text).catch(() => {})
      if (!e.shiftKey) term?.clearSelection()
      e.preventDefault()
      e.stopPropagation()
      return false
    }
    // Ctrl+V and Ctrl+Shift+V: let the browser fire its paste event, which
    // xterm turns into a (bracketed) paste, instead of sending ^V. Not the
    // app's Ctrl+Shift+V as well.
    if (key === 'v') {
      e.stopPropagation()
      return false
    }
    // Ctrl+Shift+letter reaches a terminal as plain Ctrl+letter, so no client
    // loses anything by these going to the app (go to page, SQL editor...).
    if (e.shiftKey && /^[a-z]$/.test(key)) return false
    return true
  }

  /** @param {string} url */
  async function openLink(url) {
    try {
      const { openUrl } = await import('@tauri-apps/plugin-opener')
      await openUrl(url)
    } catch {
      window.open(url, '_blank', 'noopener,noreferrer')
    }
  }

  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let fitTimer
  /** @type {ResizeObserver | null} */
  let resizeObserver = null
  /** @type {MutationObserver | null} */
  let themeObserver = null
  let fitFrame = 0

  onMount(() => {
    if (!termEl) return
    const { fontFamily, fontSize } = terminalFont(termEl)
    term = new Terminal({
      fontFamily,
      fontSize,
      // The font's own line height: box-drawing characters span it, so psql's
      // table borders join from row to row. Any taller and the DOM renderer
      // leaves a gap in every vertical line.
      lineHeight: 1,
      // A blinking block: what Terminal.app, iTerm and every shell default to.
      cursorBlink: true,
      cursorStyle: 'block',
      cursorInactiveStyle: 'outline',
      scrollback: 25_000,
      // Alt+wheel scrolls a page at a time through long results.
      fastScrollSensitivity: 10,
      // The search add-on's match highlights use the decorations API.
      allowProposedApi: true,
      // Only under a Glass theme: transparency costs xterm its subpixel text.
      allowTransparency: terminalIsTranslucent(termEl),
      theme: terminalTheme(termEl),
    })
    fitAddon = new FitAddon()
    term.loadAddon(fitAddon)
    term.loadAddon(new WebLinksAddon((_event, uri) => void openLink(uri)))
    searchAddon = new SearchAddon({ highlightLimit: 2000 })
    term.loadAddon(searchAddon)
    searchAddon.onDidChangeResults(({ resultIndex, resultCount }) => {
      findIndex = resultIndex + 1
      findTotal = resultCount
    })
    term.open(termEl)
    // DOM renderer everywhere. The WebGL canvas was stretched and blanked by the
    // browser on every size change before xterm repainted it: while the dock was
    // dragged that read as flicker, overlapping lines and blank bands. The DOM
    // renderer reflows with the layout in the same frame, and a database shell
    // never prints fast enough to need the GPU path.
    term.attachCustomKeyEventHandler(handleKey)
    // psql's prompt marks (OSC 133, set in db/terminal.rs): A starts a prompt,
    // `A;k=s` a continuation one, B is where typing starts. Not on Windows:
    // ConPTY redraws the client's output from a screen buffer of its own, and
    // whether the marks survive that, and where among the text, depends on the
    // Windows build. There the prompt is found by its pattern (PROMPT_RE).
    term.parser.registerOscHandler(133, (data) => {
      if (!term || IS_WINDOWS) return true
      const [mark, ...params] = data.split(';')
      const buf = term.buffer.active
      if (mark === 'A') {
        const more = params.includes('k=s')
        if (!more) statementLines = []
        continuing = more
        inputStart = null
        hideSuggest()
      } else if (mark === 'B') {
        inputStart = { row: buf.baseY + buf.cursorY, col: buf.cursorX }
      }
      return true
    })
    term.onData(onTyped)
    // After output lands (once per frame at most): the prompt state, the
    // suggestions for what was just typed, and whether the view is at the end.
    term.onWriteParsed(() => {
      cancelAnimationFrame(suggestFrame)
      suggestFrame = requestAnimationFrame(() => {
        trackPrompt()
        trackScroll()
        if (lastTypedAt) updateSuggest()
      })
    })
    term.onScroll(() => {
      hideSuggest()
      trackScroll()
    })
    // A click starts a selection of its own; the typed line is no longer what is selected.
    termEl.addEventListener('mousedown', () => { inputSelected = false })
    term.textarea?.addEventListener('blur', hideSuggest)
    term.onResize(() => sendResizeSoon())
    fit()

    // Native behaviour: the grid reflows with the panel on every frame, so the
    // text never stretches or gets cut while dragging. What used to flicker was
    // the client redrawing its prompt on each PTY resize; that is now sent
    // once the size settles (sendResizeSoon), not per frame.
    // While a panel is dragged, refit a few times a second (and once at the
    // end) instead of every frame: each fit re-lays every row of the grid, and
    // doing that per frame is what made the drag feel heavy.
    let lastFitAt = 0
    resizeObserver = new ResizeObserver(() => {
      cancelAnimationFrame(fitFrame)
      clearTimeout(fitTimer)
      if (document.documentElement.hasAttribute('data-resizing')) {
        const since = performance.now() - lastFitAt
        if (since > 50) { lastFitAt = performance.now(); fitFrame = requestAnimationFrame(fit) }
        fitTimer = setTimeout(() => { lastFitAt = performance.now(); fitFrame = requestAnimationFrame(fit) }, 50)
        return
      }
      fitFrame = requestAnimationFrame(fit)
    })
    resizeObserver.observe(termEl)
    // Theme, font preset and zoom all land as attributes on <html>.
    themeObserver = new MutationObserver(() => applyAppearance())
    themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'class', 'style'] })
    // The editor font is a web font: measure the cells again once it is in.
    void remeasure()
    void document.fonts?.ready.then(() => void remeasure())

    sync()
  })

  /** Start, restart or stop the session to match the tab and the connection. */
  function sync() {
    if (!term) return
    if (!connectionKey) { if (phase !== 'idle') stop(); return }
    if ((active || prewarm) && connectionKey !== startedKey) void start()
  }

  $effect(() => {
    void connectionKey
    void active
    void prewarm
    untrack(sync)
  })

  // Coming back to the tab: the terminal was display:none, so size and focus it.
  $effect(() => {
    if (!active) return
    untrack(() => requestAnimationFrame(() => {
      fit()
      if (phase === 'running' || phase === 'exited') term?.focus()
    }))
  })

  // The SQL editor's text size setting.
  $effect(() => {
    void $appSqlEditor.textSize
    untrack(() => requestAnimationFrame(applyAppearance))
  })

  onDestroy(() => {
    generation++
    cancelAnimationFrame(fitFrame)
    cancelAnimationFrame(suggestFrame)
    resizeObserver?.disconnect()
    themeObserver?.disconnect()
    void endSession()
    term?.dispose()
    term = null
  })

  async function copyHint() {
    if (!client?.hint) return
    await navigator.clipboard.writeText(client.hint).catch(() => {})
    hintCopied = true
    setTimeout(() => (hintCopied = false), 1500)
  }

  const statusDot = $derived(
    phase === 'running' ? 'bg-success'
      : phase === 'checking' || phase === 'starting' ? 'bg-warning animate-pulse'
      : 'bg-muted-foreground/60',
  )
  const statusLabel = $derived(
    phase === 'running' ? 'Running'
      : phase === 'checking' || phase === 'starting' ? 'Starting'
      : phase === 'exited' ? 'Exited'
      : '',
  )
</script>

<div class="group/term relative flex min-h-0 flex-1 flex-col overflow-hidden bg-panel">
  {#if docked}
    <!-- Docked under a page (⌘J): no bar at all. The output starts at the top
         left like a real terminal; the four actions float in the corner. -->
    <div class="absolute top-1 right-2 z-20 flex items-center gap-0.5 rounded-full bg-panel/95 p-0.5 shadow-sm opacity-0 transition-opacity duration-150 group-hover/term:opacity-100 focus-within:opacity-100">
      <button type="button" class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-40" title="Clear" aria-label="Clear scrollback" disabled={phase !== 'running' && phase !== 'exited'} onclick={() => { term?.clear(); term?.focus() }}><Eraser class="size-3.5 shrink-0" /></button>
      <button type="button" class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-40" title="Restart {client?.name || 'the client'}" aria-label="Restart" disabled={!connection || phase === 'checking' || phase === 'starting'} onclick={() => void start()}><RotateCw class="size-3.5 shrink-0" /></button>
      {#if onclose}
        <button type="button" class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" title="Hide (⌘J)" aria-label="Hide the terminal" onclick={onclose}><X class="size-3.5 shrink-0" /></button>
      {/if}
    </div>
  {:else}
  <!-- One slim strip, like a terminal window's title bar: which client
       against what, a status dot, and the few actions. It was a 36px header
       with a "Running" label and a 32px command bar under the screen, so a
       third of a short pane was chrome before any output. -->
  <div class="studio-chrome flex h-7 shrink-0 items-center gap-2 border-b border-border/50 bg-panel ps-2.5 pe-1">
    <span
      class={['size-1.5 shrink-0 rounded-full', statusDot]}
      title={statusLabel || undefined}
      aria-label={statusLabel || undefined}
      role={statusLabel ? 'status' : undefined}
    ></span>
    <span class="shrink-0 font-mono text-ui-2xs font-medium text-foreground">{client?.name || 'Terminal'}</span>
    {#if shortVersion}
      <span class="shrink-0 font-mono text-ui-3xs text-muted-foreground tabular-nums" title={client?.version ?? ''}>{shortVersion}</span>
    {/if}
    {#if target}
      <span class="min-w-0 truncate font-mono text-ui-2xs" title={target}>
        <span class="text-muted-foreground/70">{targetParts.lead}</span><span class="text-muted-foreground">{targetParts.db}</span>
      </span>
    {/if}
    <div class="flex-1"></div>
    {#if continuing && phase === 'running'}
      <!-- An open statement, said in the strip (fixed height) rather than by a
           bar that appears below the screen and moves it. -->
      <span class="hidden shrink-0 items-center gap-2 text-ui-3xs text-muted-foreground sm:flex">
        <span class="size-1.5 shrink-0 rounded-full bg-warning" aria-hidden="true"></span>
        <span class="text-foreground">Statement open</span>
        <span class="flex items-center gap-1"><Kbd combo="Enter" /> run</span>
        <span class="flex items-center gap-1"><Kbd combo="Ctrl+C" /> discard</span>
      </span>
    {/if}
    <div class="flex shrink-0 items-center gap-0.5">
      {#if quickCommands.length}
        <button
          type="button"
          class={['inline-flex size-6 items-center justify-center rounded-md transition-colors hover:bg-accent hover:text-foreground', showQuickBar ? 'text-foreground' : 'text-muted-foreground']}
          title={showQuickBar ? 'Hide common commands' : 'Show common commands'}
          aria-label="Common commands"
          aria-pressed={showQuickBar}
          onclick={toggleQuickBar}
        >
          <Command class="size-3.5 shrink-0" />
        </button>
      {/if}
      <button
        type="button"
        class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
        title="Clear scrollback"
        aria-label="Clear scrollback"
        disabled={phase !== 'running' && phase !== 'exited'}
        onclick={() => { term?.clear(); term?.focus() }}
      >
        <Eraser class="size-3.5 shrink-0" />
      </button>
      <button
        type="button"
        class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
        title="Restart {client?.name || 'the client'}"
        aria-label="Restart {client?.name || 'the client'}"
        disabled={!connection || phase === 'checking' || phase === 'starting'}
        onclick={() => void start()}
      >
        <RotateCw class="size-3.5 shrink-0" />
      </button>
      {#if onclose}
        <button
          type="button"
          class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          title="Hide (⌘`)"
          aria-label="Hide {client?.name || 'terminal'}"
          onclick={onclose}
        >
          <X class="size-3.5 shrink-0" />
        </button>
      {/if}
    </div>
  </div>
  {/if}

  <div bind:this={bodyEl} class="relative min-h-0 flex-1">
    <!-- The padding lives on this wrapper: FitAddon sizes the grid from the
         terminal's parent, padding included. -->
    <!-- Tight inset and text a touch under the SQL editor's: a terminal is
         dense - Terminal.app and iTerm sit a few px from the window edge - and
         at the editor's size psql's tables read like a document. -->
    {#if findOpen}
      <div
        class="absolute top-2 right-3 z-30 flex h-8 items-center gap-0.5 rounded-lg border border-border/60 bg-popover ps-2.5 pe-1 shadow-md"
        role="search"
      >
        <Search class="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
        <input
          bind:this={findInput}
          bind:value={findQuery}
          oninput={() => findStep('next', true)}
          onkeydown={onFindKey}
          placeholder="Find"
          aria-label="Find in terminal"
          spellcheck="false"
          autocomplete="off"
          class="no-field-frame no-focus-ring h-full w-44 min-w-0 border-0 bg-transparent px-1.5 font-mono text-ui-xs text-foreground shadow-none outline-none placeholder:font-sans placeholder:text-muted-foreground"
        />
        <span class="min-w-[4.5rem] shrink-0 pe-1 text-right text-ui-2xs tabular-nums text-muted-foreground" aria-live="polite">
          {#if !findQuery}{''}{:else if findTotal === 0}No results{:else if findTotal < 0}{findIndex} of many{:else}{findIndex} of {findTotal}{/if}
        </span>
        <button
          type="button"
          class={['inline-flex size-6 items-center justify-center rounded-md transition-colors hover:bg-accent hover:text-foreground', findCase ? 'bg-accent text-foreground' : 'text-muted-foreground']}
          title="Match case"
          aria-label="Match case"
          aria-pressed={findCase}
          onclick={() => { findCase = !findCase; findStep('next', true); findInput?.focus() }}
        ><CaseSensitive class="size-3.5 shrink-0" /></button>
        <button type="button" class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-40" title="Previous match (Shift+Enter)" aria-label="Previous match" disabled={!findQuery} onclick={() => findStep('prev')}><ChevronUp class="size-3.5 shrink-0" /></button>
        <button type="button" class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-40" title="Next match (Enter)" aria-label="Next match" disabled={!findQuery} onclick={() => findStep('next')}><ChevronDown class="size-3.5 shrink-0" /></button>
        <button type="button" class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" title="Close (Esc)" aria-label="Close find" onclick={closeFind}><X class="size-3.5 shrink-0" /></button>
      </div>
    {/if}
    <div class="absolute inset-0 pt-2 pr-1 pb-1 pl-3">
      <div
        bind:this={termEl}
        class="size-full"
        style="font-family: var(--editor-font-family, var(--font-mono)); font-size: calc({sqlEditorFontSize($appSqlEditor.textSize)} * 0.9);"
      ></div>
    </div>

    {#if suggestItems.length}
      <div
        class="absolute z-20 w-max max-w-[min(26rem,calc(100%-1rem))] min-w-44 overflow-hidden rounded-lg border border-border/60 bg-popover p-1 elevate-2-rim"
        style="left: {suggestPos.left}px; {suggestPos.above ? `bottom: ${suggestPos.bottom}px` : `top: ${suggestPos.top}px`}; font-family: var(--editor-font-family, var(--font-mono));"
        role="listbox"
        aria-label="Suggestions"
      >
        {#each suggestItems as item, i (item.kind + item.label)}
          {@const KindIcon = KIND_ICON[item.kind]}
          {@const selected = i === suggestIndex}
          <div
            role="option"
            tabindex="-1"
            aria-selected={selected}
            class={['flex h-6 cursor-default items-center gap-2 rounded-md px-2 text-ui-xs', selected ? 'bg-accent text-foreground' : 'text-muted-foreground']}
            onmousedown={(e) => { e.preventDefault(); acceptSuggestion(i) }}
            onmousemove={() => (suggestIndex = i)}
          >
            <KindIcon class={['size-3 shrink-0', KIND_COLOR[item.kind]]} />
            <span class="whitespace-pre text-foreground">{item.label}</span>
            {#if item.detail}
              <span class="ml-auto min-w-0 truncate pl-3 text-ui-2xs text-muted-foreground">{item.detail}</span>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    {#if scrolledUp && (phase === 'running' || phase === 'exited')}
      <button
        type="button"
        class="absolute right-4 bottom-3 z-10 flex h-7 items-center gap-1.5 rounded-full border border-border/60 bg-popover px-3 text-ui-2xs text-muted-foreground elevate-2-rim transition-colors hover:text-foreground"
        onclick={scrollToLatest}
      >
        <ArrowDownToLine class="size-3.5 shrink-0" />
        Jump to latest
      </button>
    {/if}

    {#if missingClient && phase === 'running'}
      <div class="absolute right-3 bottom-3 z-10 flex max-w-sm flex-col items-end gap-1.5">
        {#if installError}
          <p class="rounded-md border border-destructive/30 bg-popover px-2.5 py-1.5 text-left font-mono text-ui-2xs whitespace-pre-wrap text-destructive elevate-2-rim">{installError}</p>
        {/if}
        <button
          type="button"
          disabled={installing}
          class="inline-flex h-7 items-center gap-1.5 rounded-md border border-border/60 bg-popover px-2.5 text-ui-xs text-foreground elevate-2-rim transition-colors hover:bg-accent disabled:opacity-60"
          title={missingClient.hint ?? ''}
          onclick={() => void installClient()}
        >
          {#if installing}
            <Icon name="loader-2" class="size-3.5 shrink-0 animate-spin" /> Installing {missingClient.name}…
          {:else}
            <Icon name="download" class="size-3.5 shrink-0" /> Install {missingClient.name}
          {/if}
        </button>
      </div>
    {/if}

    {#if phase === 'unsupported' || phase === 'missing' || phase === 'failed'}
      <div class="absolute inset-0 flex items-center justify-center bg-panel p-6">
        <div class="flex w-full max-w-md flex-col items-center gap-3 text-center">
          {#if phase === 'missing'}
            <div class="flex size-10 items-center justify-center rounded-lg border border-border bg-muted/30">
              <PackageSearch class="size-5 shrink-0 text-muted-foreground" />
            </div>
            <p class="text-ui-lg font-semibold text-foreground">{client?.name} is not installed</p>
            <p class="text-ui-sm text-muted-foreground">
              This tab runs the real {client?.name}, so every command works the way it does in a terminal. Install it, then check again, or use the built-in prompt for now.
            </p>
            {#if !NON_SQL.has(dbType)}
            <Button size="sm" variant="outline" onclick={() => startBuiltinPrompt(`${client?.name ?? 'The client'} is not installed: using the built-in prompt. Install it for ${client?.name ?? 'its'} own commands.`)}>
              Use the built-in SQL prompt
            </Button>
            {/if}
            {#if client?.hint}
              <div class="flex w-full items-center gap-2 rounded-lg border border-border bg-muted/30 py-1 pr-1 pl-3 text-left">
                <span class={['min-w-0 flex-1 text-ui-xs', installLooksLikeCommand ? 'font-mono text-foreground' : 'text-muted-foreground']}>{client.hint}</span>
                {#if installLooksLikeCommand}
                  <button
                    type="button"
                    class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                    title="Copy"
                    aria-label="Copy install command"
                    onclick={copyHint}
                  >
                    {#if hintCopied}<Check class="size-3.5 shrink-0 text-success" />{:else}<Copy class="size-3.5 shrink-0" />{/if}
                  </button>
                {/if}
              </div>
            {/if}
            <Button size="sm" disabled={installing} onclick={() => void installClient()}>
              {installing ? `Installing ${client?.name ?? ''}…` : `Install ${client?.name ?? 'it'}`}
            </Button>
            {#if installError}<p class="w-full whitespace-pre-wrap text-left font-mono text-ui-2xs text-destructive">{installError}</p>{/if}
            <Button variant="outline" size="sm" onclick={() => void start()}>
              <RotateCw class="size-3.5 shrink-0" />
              Check again
            </Button>
          {:else if phase === 'failed'}
            <div class="flex size-10 items-center justify-center rounded-lg border border-destructive/30 bg-destructive/10">
              <CircleAlert class="size-5 shrink-0 text-destructive" />
            </div>
            <p class="text-ui-lg font-semibold text-foreground">Could not start {client?.name || 'the terminal'}</p>
            <p class="w-full rounded-lg border border-border bg-muted/30 px-3 py-2 text-left font-mono text-ui-xs break-words text-muted-foreground select-text">{failure}</p>
            <Button variant="outline" size="sm" onclick={() => void start()}>
              <RotateCw class="size-3.5 shrink-0" />
              Try again
            </Button>
          {:else}
            <div class="flex size-10 items-center justify-center rounded-lg border border-border bg-muted/30">
              <Ban class="size-5 shrink-0 text-muted-foreground" />
            </div>
            <p class="text-ui-lg font-semibold text-foreground">No shell for this connection</p>
            <p class="text-ui-sm text-muted-foreground">{client?.hint}</p>
          {/if}
        </div>
      </div>
    {/if}
  </div>

  <!-- Footer: the commands people reach for most, one click each. -->
  <!-- Shown only when turned on, and then whatever the session is doing: a bar
       that came and went with an open statement or a phase change resized the
       terminal under the cursor - a refit and a redraw on the keystroke that
       opened or closed a statement, which read as flicker. -->
  {#if quickCommands.length && showQuickBar}
    <div class="studio-chrome app-scroll-x flex h-8 shrink-0 items-center gap-1 overflow-x-auto border-t border-border bg-panel px-2">
      {#if continuing && phase === 'running'}
        <!-- An open statement: the commands below would only join it, so the
             bar says how to finish it instead. -->
        <span class="flex shrink-0 items-center gap-2 px-2 text-ui-2xs text-muted-foreground">
          <span class="size-1.5 shrink-0 rounded-full bg-warning"></span>
          <span class="text-foreground">Statement open</span>
          <span class="flex items-center gap-1"><Kbd combo="Enter" /> run</span>
          <span class="flex items-center gap-1"><Kbd combo="Shift+Enter" /> new line</span>
          <span class="flex items-center gap-1"><Kbd combo="Ctrl+C" /> discard</span>
        </span>
      {:else}
        {#each quickCommands as q (q.cmd)}
          <button
            type="button"
            class="flex h-6 shrink-0 items-center gap-1.5 rounded-md px-2 text-ui-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
            title="Run {q.cmd}"
            disabled={phase !== 'running'}
            onclick={() => runQuick(q.cmd)}
          >
            <span class="font-mono text-foreground">{q.cmd}</span>
            <span>{q.label}</span>
          </button>
        {/each}
      {/if}
      <div class="flex-1"></div>
      <span class="hidden shrink-0 items-center gap-1 pr-1 text-ui-2xs text-muted-foreground lg:flex">
        <Kbd combo="Ctrl+Space" /> suggest
        <span class="mx-1"></span>
        <Kbd combo="Ctrl+`" /> back
      </span>
    </div>
  {/if}
</div>
