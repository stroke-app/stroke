<script>
  import { tx } from '$lib/i18n.js'
  import { cachedCommandFilter } from '$lib/command-filter.js'
  import { loadRecentCommands, rememberCommand } from '$lib/stores/recent-commands.js'
  import { openTransfer } from '$lib/stores/transfer-ui.js'
  import { EXPORT_FORMATS } from '$lib/export.js'
  import Kbd from './Kbd.svelte'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'
  import { engineId, engineLabel } from '$lib/connection-provider.js'
  import * as Command from '$lib/components/ui/command/index.js'
  import { formatTableRowCount, formatFullCount } from '$lib/table-list.js'
  import { get } from 'svelte/store'
  import { tick, setContext, untrack } from 'svelte'
  import { createLocalFilter, LOCAL_FILTER } from '$lib/command-local-filter.svelte.js'
  import { chatCompletionStream, buildSystemPrompt, AI_TOOLS, isDestructiveSql, classifyDbError } from '$lib/ai.js'
  import { aiSettings, isAiConfigured } from '$lib/stores/ai-settings.js'
  import { profileAgent } from '$lib/local-agents.js'
  import { pinEnabled, lockNow } from '$lib/stores/app-lock.js'
  import { appCmdkAi, appThemeId, appSidebarStyle, previewTheme, endThemePreview, updateSettings } from '$lib/stores/settings.js'
  import { executeSql, dockerTemplates, scanDockerDatabases } from '$lib/api.js'
  import DbIcon from './DbIcon.svelte'
  import { themesByGroup, getThemeDefinition } from '$lib/themes/registry.js'
  import ThemeSwatchStrip from './ThemeSwatchStrip.svelte'

  let {
    /** Export the open table in a format, or null when no table tab is open.
     *  @type {((format: string) => void) | null} */
    onexporttable = null,
    /** The open table's name, for the export rows' labels. */
    exportTableName = '',
    /** Switch the open table between its rows and its structure, or null when
     *  the tab in front is not a table with a structure view.
     *  @type {(() => void) | null} */
    ontablestructure = null,
    /** Whether that table is showing its structure now. */
    tableStructureShown = false,
    /** Tab commands that only had a shortcut or the tab's right-click menu:
     *  each is null when it does not apply.
     *  @type {{ reopen: (() => void) | null, close: (() => void) | null, closeOthers: (() => void) | null, togglePin: (() => void) | null, pinned: boolean, newTable: (() => void) | null } | null} */
    tabActions = null,
    /** The open table's toolbar, by name; null off a table tab.
     *  @type {{ filter: () => void, sort: () => void, columns: () => void, addRow: (() => void) | null, findReplace: (() => void) | null, refresh: () => void, view: ((v: string) => void) | null, openInSql: () => void } | null} */
    tableCommands = null,
    /** Text to open with: a letter typed on the New tab page starts the search
     *  with it. Read once as the menu opens, then cleared. */
    seed = $bindable(''),
    /** Zoom and full screen, which only the View menu and their keys reached.
     *  @type {{ zoomIn: () => void, zoomOut: () => void, zoomReset: () => void, fullscreen: () => void } | null} */
    viewCommands = null,
    /** Explain the open editor's query, or null when no editor tab is in front.
     *  @type {(() => void) | null} */
    onexplainquery = null,
    /** Open Import data for the open table, or null when there is none or it is read-only.
     *  @type {(() => void) | null} */
    onimporttable = null,
    open = $bindable(false),
    /** @type {'root' | 'docker' | 'connections' | 'tables' | 'pages' | 'ask' | 'theme'} */
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
    /** The docked AI sidebar exists (StudioShell's AI_SIDEBAR_ENABLED). Off, its row would open the same view as Open AI panel. */
    aiSidebarEnabled = false,
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
    /** Engine can show Instance Insights (see db-capabilities.js). */
    hasInsights = true,
    /** Engine can run the Advisor's checks. */
    hasAdvisor = true,
    /** Redis (key-value) connection - hides SQL-only pages, shows the keyspace. */
    isRedis = false,
    /** Any key-value engine (Redis, DynamoDB): hides the SQL-only entries. */
    isKeyValue = isRedis,
    isDynamo = false,
    onopendynamo = () => {},
    isDoc = false,
    onopendocs = () => {},
    /** PostGIS present on this connection - gates the Map entry. */
    geoAvailable = false,
    onopenredis = () => {},
    onopenlogs = () => {},
    /** The Queries page: history, saved queries and charts. */
    onopenqueries = () => {},
    onopenterminal = () => {},
    onnewterminal = () => {},
    /** What's new in this version, any time. */
    onopenwhatsnew = () => {},
    onopeninsights = () => {},
    /** Labs: share the open database as a web studio (shown when the Labs switch is on). */
    onsharestudio = /** @type {(() => void) | null} */ (null),
    /** While sharing: the SQL console everyone on the link edits together. */
    onsharedconsole = /** @type {(() => void) | null} */ (null),
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
    /** A database file in one step: a new SQLite file, or an existing one. @type {(how: 'new' | 'open') => void} */
    onfiledb = () => {},
    onopenshortcuts = () => {},
    /** My shares: share links made on this account. */
    onopenmyshares = () => {},
    /** Settings ▸ Keybindings, where shortcuts are remapped. */
    onopenkeybindings = () => {},
    /** Show or hide the sidebar (Mod+B). @type {(() => void) | null} */
    ontogglesidebar = null,
    /** Back and forward through the tabs visited. @type {(() => void) | null} */
    onback = null,
    /** @type {(() => void) | null} */
    onforward = null,
    /** Settings, on Appearance (where the theme builder is). */
    onopenappearance = () => {},
    onopenabout = () => {},
    onopenreport = () => {},
    oncheckupdate = () => {},
    /** Whether the connection is currently read-only (locks edits/inserts/deletes). */
    readonly = false,
    /** Toggle read-only mode. */
    onreadonlytoggle = () => {},
    /** @param {'postgres'|'mysql'} dbType */
    ondockerlaunch = (dbType) => {},
    /** Open a database already running in Docker. @type {(conn: Record<string, any>) => void} */
    onopendocker = () => {},
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
    openmigrations = () => {},
    openlocalservers = () => {},
    /** Live AI schema/db context for the inline quick-ask. */
    schemaContext = null,
    /** Escalate a quick-ask question into the full AI sidebar/chat. @param {string} q */
    onaskcontinue = (q) => {},
  } = $props()

  /** @param {'docker' | 'connections' | 'tables' | 'ask' | 'pages'} target */
  function navigate(target) {
    page = target
    // A sub-page starts with an empty search: the words that found its row
    // ("all tables", "connections") filtered the page down to nothing.
    paletteSearch = ''
  }

  // ── Theme page: live preview while the highlight moves ──────────────────────
  // The highlighted row is painted with previewTheme(), which writes nothing.
  // Enter/click commits through updateSettings, so theme history and the
  // last-dark/last-light memory see one change from the original theme. Leaving
  // the page any other way (Esc, Back, Backspace, closing) puts the original back.
  /** @typedef {import('$lib/themes/registry.js').ThemeId} ThemeId */

  /** Value of the highlighted command row (bound to the palette). */
  let highlighted = $state('')

  // ── Docker page: the launch catalog and what is already running ──────────
  /** @type {import('$lib/api.js').DockerTemplate[]} */
  let dockerTpls = $state([])
  /** @type {import('$lib/api.js').DockerDatabase[]} */
  let dockerRunning = $state([])
  $effect(() => {
    if (page !== 'docker') return
    if (!dockerTpls.length) dockerTemplates().then((t) => (dockerTpls = t)).catch(() => {})
    scanDockerDatabases().then((d) => (dockerRunning = d.filter((x) => !x.reason))).catch(() => {})
  })
  const dockerGroups = $derived.by(() => {
    /** @type {Map<string, import('$lib/api.js').DockerTemplate[]>} */
    const m = new Map()
    for (const t of dockerTpls) m.set(t.group, [...(m.get(t.group) ?? []), t])
    return [...m]
  })
  /** The theme in effect when the page opened; null when no preview is live. */
  let themeOrigin = $state(/** @type {ThemeId | null} */ (null))
  /** Off until the page has highlighted the current theme, so the first row is never flashed. */
  let themePreviewArmed = false

  // Dark first; visibleThemes() inside keeps the easter egg out until found.
  const themePageGroups = $derived(page === 'theme' ? [...themesByGroup()].reverse() : [])

  /** @param {{ id: ThemeId, name: string, description: string }} t @param {string} groupLabel */
  const themeRowValue = (t, groupLabel) => `theme:${t.id} ${t.name} ${groupLabel} ${t.description}`

  /** @param {string} value @returns {ThemeId | null} */
  function themeIdFromValue(value) {
    const m = /^theme:([\w-]+)/.exec(value ?? '')
    return m ? /** @type {ThemeId} */ (m[1]) : null
  }

  function openThemePage() {
    themePreviewArmed = false
    themeOrigin = get(appThemeId)
    paletteSearch = ''
    page = 'theme'
  }

  /** @param {ThemeId} id */
  function commitTheme(id) {
    themePreviewArmed = false
    themeOrigin = null
    cancelAnimationFrame(previewFrame)
    // Settle the preview first (drops its pending store sync), then choose the
    // theme normally. Same task, so nothing paints in between.
    endThemePreview()
    updateSettings({ theme: id })
    open = false
  }

  // Opened straight onto the theme page (the title-bar menu's Theme…): record
  // the theme to come back to, as openThemePage does from inside.
  $effect(() => {
    if (open && page === 'theme' && themeOrigin === null && !themePreviewArmed) {
      themeOrigin = get(appThemeId)
      paletteSearch = ''
    }
  })

  // Start on the current theme rather than the first row, then arm the preview.
  // Two frames: bits-ui selects the first row a tick after the rows mount.
  $effect(() => {
    if (!open || page !== 'theme') return
    const origin = themeOrigin
    let inner = 0
    const outer = requestAnimationFrame(() => {
      inner = requestAnimationFrame(() => {
        const t = origin ? getThemeDefinition(origin) : null
        const group = themePageGroups.find((g) => g.themes.some((x) => x.id === t?.id))
        const value = t && group ? themeRowValue(t, group.label) : null
        if (value) highlighted = value
        themePreviewArmed = true
        // After the row is marked: queried at once it found the first row,
        // still selected, and the list stayed at the top with the current
        // theme (often in the Light group, further down) out of sight.
        void tick().then(() => {
          const row = value
            ? document.querySelector(`[data-slot="command-item"][data-value="${CSS.escape(value)}"]`)
            : document.querySelector('[data-slot="command-item"][data-selected]')
          row?.scrollIntoView({ block: 'center' })
        })
      })
    })
    return () => {
      cancelAnimationFrame(outer)
      cancelAnimationFrame(inner)
    }
  })

  // Arrow keys (held down included) and hover both move `highlighted`, and so
  // does typing, which re-ranks the rows. Only the first two preview: typing
  // "nord" repainted the whole app in Studio Dark at "n", and each repaint
  // held up the next letter. `navAt` is when an arrow key or the pointer last
  // moved the highlight; a typed key clears it.
  let navAt = 0
  $effect(() => {
    if (!open || page !== 'theme') return
    /** @param {KeyboardEvent} e */
    const onKey = (e) => {
      if (/^(Arrow(Up|Down)|Page(Up|Down)|Home|End)$/.test(e.key)) navAt = performance.now()
      else if (e.key.length === 1 || e.key === 'Backspace' || e.key === 'Delete') navAt = 0
    }
    // Only a pointer that moved: rows re-ranking under a still one send a move too.
    let px = NaN, py = NaN
    /** @param {PointerEvent} e */
    const onMove = (e) => {
      if (e.clientX === px && e.clientY === py) return
      px = e.clientX; py = e.clientY
      navAt = performance.now()
    }
    window.addEventListener('keydown', onKey, true)
    window.addEventListener('pointermove', onMove, { capture: true, passive: true })
    return () => {
      window.removeEventListener('keydown', onKey, true)
      window.removeEventListener('pointermove', onMove, true)
    }
  })
  // At most one repaint per frame: key repeat can outrun the display, and
  // painting a theme the next frame replaces anyway is the lag you would see.
  let previewFrame = 0
  $effect(() => {
    const value = highlighted
    if (page !== 'theme' || !themePreviewArmed) return
    const id = themeIdFromValue(value)
    if (!id) return
    if (performance.now() - navAt > 400) return
    cancelAnimationFrame(previewFrame)
    previewFrame = requestAnimationFrame(() => {
      if (page !== 'theme' || !themePreviewArmed) return
      if (document.documentElement.getAttribute('data-theme') !== id) previewTheme(id)
    })
  })

  // Any way off the page without committing restores the saved theme.
  $effect(() => {
    if (page === 'theme' || !themeOrigin) return
    themePreviewArmed = false
    themeOrigin = null
    cancelAnimationFrame(previewFrame)
    endThemePreview()
  })

  // Single source of truth for page destinations - rendered both in the root
  // "Views" group and in the dedicated "Go to page" navigator (Ctrl/⌘+Shift+P). Add a
  // page once here and it shows up in both. `keys` is an optional shortcut hint.
  const pageItems = $derived([
    { icon: 'key-round',       label: 'Redis keyspace',                action: onopenredis,      show: connected && isRedis, value: 'redis keyspace keys browse key value store console cli' },
    { icon: 'database',        label: 'DynamoDB tables',               action: onopendynamo,     show: connected && isDynamo, value: 'dynamodb tables items scan query partiql aws' },
    { icon: 'database',        label: 'Browse documents',              action: onopendocs,       show: connected && isDoc, value: 'browse collections documents mongodb mongo couchdb elasticsearch opensearch indices cassandra firestore tables rows' },
    { icon: 'table-2',         label: 'Table data',       keys: '⌘⇧D', action: onopentable,      show: connected && !isKeyValue, value: 'open table data browser rows' },
    { icon: 'search',          label: 'Find in database', keys: '⌘⇧G', action: onglobalsearch,   show: connected && !isKeyValue, value: 'find search rows data across all tables global database' },
    { icon: 'terminal',        label: 'SQL editor',       keys: '⌘⇧S', action: onopensql,        show: connected && (!isKeyValue || isDynamo || isDoc), value: 'open sql editor query console' },
    { icon: 'plus',            label: 'New SQL editor',                action: onnewsql,         show: connected && (!isKeyValue || isDynamo || isDoc), value: 'new sql editor tab additional multiple query console open another' },
    { icon: 'square-terminal', label: 'Terminal',         keys: '⌘J',  action: onopenterminal,   show: connected, value: 'terminal shell cli command line psql mysql mariadb sqlite3 sqlcmd redis-cli console repl meta commands' },
    { icon: 'sparkles',        label: "What's new",       keys: '',    action: onopenwhatsnew,   show: true,      value: 'whats new changelog release notes updates features version' },
    { icon: 'square-terminal', label: 'New terminal tab', keys: '⌃⇧`', action: onnewterminal,    show: connected, value: 'new terminal tab another second shell psql mysql session' },
    { icon: 'code-2',          label: 'ORM runner',       keys: '⌘⇧O', action: onopenorm,        show: connected && !isKeyValue, value: 'open orm runner drizzle prisma query builder' },
    { icon: 'network',         label: 'Schema diagram',                action: onopenerd,        show: connected && !isKeyValue, value: 'open schema diagram data model er diagram erd entity relationship relationships relations foreign key foreign keys pk fk graph hierarchy roadmap mermaid tree dictionary ddl schema map' },
    { icon: 'layout-template', label: 'Schema explorer',               action: onopenSchema,     show: connected && hasSchemaExplorer, value: 'open schema explorer indexes enums views materialized' },
    { icon: 'shield-check',    label: 'Security',                      action: onopensecurity,   show: connected && hasSecurity, value: 'open security roles users policies rls row level permissions privileges grants access' },
    { icon: 'shield-check',    label: 'Advisor',                       action: onopenadvisor,    show: connected && hasAdvisor, value: 'open advisor lint audit checks security performance schema rls unused index foreign key bloat vacuum' },
    { icon: 'database',        label: 'Instance insights',             action: onopeninsights,   show: connected && hasInsights, value: 'open instance insights monitoring sessions processes activity locks replication config pg_settings slow queries' },
    { icon: 'users',           label: 'Open shared console',           action: () => onsharedconsole?.(), show: connected && !!onsharedconsole, value: 'shared console collaborate live cursors sql together web studio' },
    { icon: 'globe',           label: 'Share as web studio',           action: () => onsharestudio?.(), show: connected && !!onsharestudio, value: 'share as web studio link remote relay browser device labs collaborate' },
    { icon: 'layout-dashboard',label: 'Dashboard',                     action: onopendashboard,  show: connected && !isKeyValue, value: 'open dashboard saved metrics overview widgets' },
    { icon: 'bar-chart-2',     label: 'Charts',                        action: onopencharts,     show: connected && !isKeyValue, value: 'open charts visualize query results graphs plots' },
    { icon: 'git-branch',      label: 'Diagrams',                      action: onopendiagrams,   show: connected && !isKeyValue, value: 'open diagrams draw erd relationship map' },
    { icon: 'table-2',         label: 'Database objects',              action: onopenobjects,    show: connected && !isKeyValue, value: 'open database objects overview tables views functions routines triggers sizes rows stats' },
    { icon: 'code-2',          label: 'Codegen',                       action: onopenormschema,  show: connected && !isKeyValue, value: 'codegen open schema as code prisma drizzle orm model relations types generate dml psl' },
    { icon: 'history',         label: 'Activity log',                  action: onopenlogs,       show: connected, value: 'open activity log logs events history operations audit' },
    { icon: 'history',         label: 'Query history',                 action: onopenqueries,    show: connected, value: 'queries query history saved queries past runs charts' },
    { icon: 'terminal',        label: 'Query log console', keys: '⌘⇧K', action: ontogglequerylog, show: connected && !isKeyValue, value: 'toggle query log console sql executed statements bottom panel' },
    { icon: 'globe',           label: 'Map',                           action: onopenmap, show: connected && geoAvailable, value: 'open map geo postgis spatial geometry geography geojson world plot points' },
    { icon: 'blocks',          label: 'Extensions',                    action: onopenextensions, show: connected && !isKeyValue, value: 'open extensions plugins formatters generators transforms better time uuid' },
    { icon: 'archive',         label: 'Backup & restore',              action: onopenbackup,     show: connected && !isKeyValue, value: 'backup restore export import dump data schedule scheduled cron' },
    { icon: 'braces',          label: 'JSON viewer',                   action: onopenJsonViewer, show: connected, value: 'open json viewer explorer jsonpath tool format pretty print' },
    { icon: 'plus',            label: 'New SQLite database',           action: () => onfiledb('new'), show: true, value: 'new sqlite database file create db local empty' },
    { icon: 'folder-open',     label: 'Open a database file',          action: () => onfiledb('open'), show: true, value: 'open database file sqlite duckdb db local path browse' },
    { icon: 'download',        label: 'Export connections and settings', action: () => openTransfer('export'), show: true, value: 'export connections settings snippets saved queries file backup transfer move another computer' },
    { icon: 'file-up',         label: 'Import connections and settings', action: () => openTransfer('import'), show: true, value: 'import connections settings snippets saved queries file restore transfer move another computer' },
    { icon: 'git-branch',      label: 'Schema timeline',               action: openschematimeline, show: connected && !isKeyValue, value: 'schema timeline snapshots snapshot drift detection history changes diff version compare' },
    { icon: 'database',        label: 'Local servers',                 action: openlocalservers, show: true, value: 'local servers database postgres mysql mariadb redis mongodb docker native homebrew dbngin start stop version' },
    { icon: 'git-merge',       label: 'Migrations',                    action: openmigrations,   show: connected && !isKeyValue, value: 'migrations prisma drizzle applied pending failed migrate schema history' },
    { icon: 'git-compare',     label: 'Data diff',                     action: opendatadiff,     show: connected && !isKeyValue, value: 'data diff compare tables rows changes' },
    { icon: 'file-text',       label: 'New notebook',                  action: onopennotebook,   show: connected && !isKeyValue, value: 'new sql notebook jupyter cells sql markdown' },
    { icon: 'file-text',       label: 'Open notebook…',                action: onopennotebookfile, show: connected && !isKeyValue, value: 'open notebook file sqlnb' },
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
    const settings = get(aiSettings)
    if (!isAiConfigured(settings)) { onopensettings(); open = false; return }
    // A local agent (Claude Code, Codex...) runs as a session in the AI chat,
    // with Stroke's tools over MCP; it has no HTTP endpoint to stream from
    // here, and calling one anyway failed with reqwest's bare "builder error".
    if (profileAgent(settings)) { open = false; onaskcontinue(q); return }
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
      if (!String(settings.baseUrl ?? '').trim()) throw new Error('This AI profile has no endpoint URL. Set one in Settings → Agent.')
      await runAskLoop(0, ac, settings)
    } catch (e) {
      if (!ac.signal.aborted) {
        const msg = String(/** @type {any} */ (e)?.message ?? e)
        askError = /^builder error/i.test(msg) ? 'The AI endpoint URL or key is malformed. Check it in Settings → Agent.' : msg
      }
      // Nothing is streaming any more: drop the empty "Thinking…" answer.
      askTurns = askTurns.filter((t) => !(t.role === 'assistant' && t.streaming && !t.text)).map((t) => (t.streaming ? { ...t, streaming: false } : t))
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

  const lastAnswer = $derived([...askTurns].reverse().find((t) => t.role === 'assistant' && t.text)?.text ?? '')
  let answerCopied = $state(false)
  async function copyAnswer() {
    if (!lastAnswer) return
    await navigator.clipboard?.writeText(lastAnswer).catch(() => {})
    answerCopied = true
    setTimeout(() => (answerCopied = false), 1400)
  }
  function continueInChat() {
    const q = lastAskQuestion
    open = false
    onaskcontinue(q)
  }
  const askModel = $derived(String($aiSettings.model ?? '').split('/').pop() || 'AI')

  const askBtn = 'inline-flex items-center gap-1.5 rounded-md bg-muted/50 px-2 py-1 text-ui-2xs font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground'
  const askBtnPrimary = 'inline-flex items-center gap-1.5 rounded-md bg-primary px-2 py-1 text-ui-2xs font-medium text-primary-foreground transition-opacity hover:opacity-90'
  const askBar = 'inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md px-2 text-ui-2xs font-medium transition-colors hover:bg-muted/60 hover:text-foreground'

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
    const mod = e.metaKey || e.ctrlKey
    // Ask page actions, as the footer shows them.
    if (page === 'ask' && mod && e.key === 'Enter') {
      e.preventDefault(); e.stopPropagation(); continueInChat(); return
    }
    if (page === 'ask' && mod && e.shiftKey && (e.key === 'c' || e.key === 'C')) {
      e.preventDefault(); e.stopPropagation(); void copyAnswer(); return
    }
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
      askTurns = []
      askApi = []
      askError = ''
      // Clear the search so it never persists between opens, but after the
      // frame that shows the picked page: cleared at once, the list re-ranked
      // and re-scrolled every row while the menu was on its way out, ahead of
      // the page the Enter asked for.
      _resetPending = true
      let raf = requestAnimationFrame(() => {
        raf = 0
        setTimeout(() => { if (!open) resetSearch() }, 0)
      })
      return () => { if (raf) cancelAnimationFrame(raf) }
    }
    // Opened again before the deferred clear ran: clear now, before it shows.
    if (_resetPending) resetSearch()
    if (seed) { paletteSearch = seed; seed = '' }
  })
  let _resetPending = false
  function resetSearch() {
    _resetPending = false
    page = 'root'
    paletteSearch = ''
  }

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
  // The schema list sends `kind`; older callers passed `tableKind`.
  const kindOf = (/** @type {any} */ t) => t.kind ?? t.tableKind ?? 'table'
  const regularTables = $derived(tables.filter((t) => kindOf(t) === 'table' || kindOf(t) === 'foreign_table'))
  const viewTables = $derived(tables.filter((t) => kindOf(t) === 'view'))
  const matViewTables = $derived(tables.filter((t) => kindOf(t) === 'materialized_view'))

  // Search text bound from Command.Input. We filter + cap the Tables page
  // ourselves (instead of mounting every table and letting bits-ui score them
  // all on each keystroke), which keeps the rendered list small and scrolling
  // smooth even with thousands of tables.
  let paletteSearch = $state('')
  const paletteFilter = cachedCommandFilter()
  // The root and theme pages filter their own rows: bits-ui's search stays
  // empty there, so it never rebuilds or re-sorts the list (see
  // command-local-filter.svelte.js). The theme page's first letter rebuilt 40
  // rows of swatches, 160ms in a dev build.
  // Not Connections: its current connection is a disabled row, and the best
  // row picked here does not know to skip it.
  const LOCAL_PAGES = new Set(['root', 'theme', 'pages', 'docker'])
  const localFilter = createLocalFilter(() => (LOCAL_PAGES.has(page) ? paletteSearch.trim() : ''), paletteFilter)
  setContext(LOCAL_FILTER, localFilter)
  const ROOT_GROUPS = ['ask', 'recent', 'views', 'schemas', 'tables', 'ai', 'queries', 'saved', 'recentq', 'export', 'table', 'actions', 'tabs', 'view', 'switchdb', 'database', 'launch', 'appearance']
  /** Root groups in page order, or best match first while searching. */
  const rootOrder = $derived.by(() => {
    if (!localFilter.search) return ROOT_GROUPS
    const best = new Map(ROOT_GROUPS.map((g) => [g, localFilter.best(g)]))
    return [...ROOT_GROUPS].sort((a, b) => (best.get(b) ?? 0) - (best.get(a) ?? 0))
  })
  // Page rows, best match first while searching, as bits-ui sorted them: Go to
  // page is one flat list, and on the root page Views is the group whose rows
  // differ most. Same value and keywords the row scores itself with (pageRow),
  // so the order and the highlight agree.
  const pagesRanked = $derived.by(() => {
    const q = page === 'pages' || page === 'root' ? localFilter.search : ''
    if (!q) return pageItems
    const scoreOf = (/** @type {typeof pageItems[number]} */ it) => {
      const shown = $tx('cmd.' + it.label, it.label)
      return paletteFilter(shown === it.label ? it.value : `${it.value} ${shown}`, q, [shown])
    }
    return pageItems.map((it, i) => ({ it, i, s: scoreOf(it) })).sort((a, b) => b.s - a.s || a.i - b.i).map((x) => x.it)
  })
  // Highlight the best match as the search changes: bits-ui did this while it
  // filtered, and with its search empty it no longer does.
  let _wasSearching = false
  $effect(() => {
    if (!LOCAL_PAGES.has(page)) return
    if (!localFilter.search) {
      // Cleared: back to the first row, as a fresh menu opens.
      if (!_wasSearching) return
      _wasSearching = false
      void tick().then(() => {
        const first = document.querySelector('[data-command-dialog] [data-command-item]:not([aria-disabled="true"])')
        const v = first?.getAttribute('data-value')
        if (v) highlighted = v
      })
      return
    }
    _wasSearching = true
    // Elsewhere groups keep their order, and a tie goes to the row mounted
    // first, which is the first on the page.
    const order = page === 'root' ? rootOrder : null
    const top = localFilter.top(order ? (g) => order.indexOf(g ?? '') : undefined)
    if (top && top !== untrack(() => highlighted)) highlighted = top
  })
  const TABLES_PAGE_CAP = 100
  // Fewer while searching: results are ranked, nobody reads past the first
  // screens, and every row costs layout as the ranking changes per letter.
  const TABLES_SEARCH_CAP = 40

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
    return { items: scored.slice(0, TABLES_SEARCH_CAP).map((x) => x.t), total: scored.length }
  }

  const tablesPageRegular = $derived(filterAndCap(_loweredRegular))
  const tablesPageViews = $derived(filterAndCap(_loweredViews))
  const tablesPageMatViews = $derived(filterAndCap(_loweredMatViews))
  // bits-ui's search stays empty on this page (it would rebuild every row on
  // the first letter: 370ms for 100 rows), so it no longer moves the
  // highlight as the results change. The best match is the first row.
  $effect(() => {
    if (page !== 'tables') return
    void tablesPageRegular; void tablesPageViews; void tablesPageMatViews
    void tick().then(() => {
      const first = document.querySelector('[data-command-dialog] [data-command-item]:not([aria-disabled="true"])')
      const v = first?.getAttribute('data-value')
      if (v && v !== untrack(() => highlighted)) highlighted = v
    })
  })

  // On tables page: shouldFilter=false so bits-ui skips its sort/filter pass entirely
  // (no CSS `order` reordering, no keyboard-nav interference). We pre-filter in JS.
  // On root page: bits-ui filters with a plain substring check against item value strings.
  const shouldFilter = $derived(page !== 'tables' && page !== 'ask')

  /**
   * Rows mounted on the frame the menu opens: the ones that fit on screen.
   * Mounting all ~80 root rows (each a bits-ui item, icon and keycaps) took
   * ~110ms of script in a production build before the menu could paint, past
   * the 100ms a menu has to open within to feel instant. The rest mount on
   * the next frame, before anyone can scroll or press a key that needs them,
   * and typing mounts everything at once so search always sees every row.
   */
  const FIRST_PAINT_ROWS = 12

  /** The last few page rows picked, newest first, shown above Views on an empty search. */
  let recentCommands = $state(loadRecentCommands())
  const recentRows = $derived(recentCommands.map((l) => pageItems.find((p) => p.label === l)).filter((p) => !!p))
  let mountAll = $state(false)
  const listAll = $derived(mountAll || !!paletteSearch.trim())
  $effect(() => {
    if (!open) { mountAll = false; return }
    let raf = requestAnimationFrame(() => { raf = requestAnimationFrame(() => { mountAll = true }) })
    return () => cancelAnimationFrame(raf)
  })

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
    // Engines without a host (Spanner, BigQuery, Snowflake, a DSN, ...): the
    // parts they do have, never a URI (it can hold a password).
    if (!conn.host) {
      const c = /** @type {any} */ (conn)
      const parts = [c.account, c.projectId, c.instanceId, c.databaseId ?? c.dataset, c.dsn && `DSN ${c.dsn}`, c.database]
      return [...new Set(parts.filter(Boolean))].join(' · ') || engineLabel(engineId(conn))
    }
    return `${conn.user ?? ''}@${conn.host ?? ''}:${conn.port ?? ''}/${conn.database ?? ''}`
  }

  const pageLabel = /** @type {Record<string, string>} */ ({
    docker: 'Docker',
    connections: 'Connections',
    tables: 'Tables',
    ask: 'Ask AI',
    pages: 'Go to page',
    theme: 'Theme',
  })
</script>

<Command.Dialog
  bind:open
  bind:value={highlighted}
  filter={paletteFilter}
  shouldFilter={shouldFilter}
  title="Command menu"
  description="Search tables, schemas, and commands"
  class={page === 'ask'
    ? 'w-[min(51.25rem,calc(100vw-3rem))] sm:max-w-none'
    : 'w-[min(36rem,calc(100vw-2rem))] sm:max-w-none'}
>
  {#snippet children()}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- Command.Input is FIRST in DOM so bits-ui's focus trap always lands on it.
         The breadcrumb is rendered after but floated to the top visually via order-first,
         keeping it above the input without disrupting the focus-trap scan order. -->
    <div class="flex flex-col" onkeydown={handleKeydown}>
      <Command.Input
        bind:value={paletteSearch}
        filterValue={LOCAL_PAGES.has(page) || page === 'tables' ? '' : undefined}
        oninput={(/** @type {Event} */ e) => { paletteSearch = /** @type {HTMLInputElement} */ (e.currentTarget).value }}
        placeholder={
          page === 'root' ? $tx('cmd.ph.root', 'Search tables, schemas, commands…')
          : page === 'tables' ? $tx('cmd.ph.tables', 'Search tables and views…')
          : page === 'ask' ? $tx('cmd.ph.ask', 'Ask a follow-up…')
          : page === 'pages' ? $tx('cmd.ph.pages', 'Go to page…')
          : page === 'theme' ? $tx('cmd.ph.theme', 'Select a theme…')
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
            <Icon name="chevron-left" class="size-3 shrink-0" />
            Back
          </button>
          <span class="text-muted-foreground text-ui-2xs">/</span>
          <span class="text-ui-2xs font-medium text-foreground/80">{pageLabel[page]}</span>
        </div>
      {/if}

      <!-- Taller on the Ask AI page: a transcript scrolled inside 440px shows about
           one exchange at a time, so answers and their result tables get clipped. -->
      <!-- The last rows fade into the footer (a mask, so it works on every
           theme's surface) and the thin scrollbar says there is more below. -->
      <Command.List
        class={page === 'ask'
          ? 'h-[min(640px,74vh)] max-h-[min(640px,74vh)]'
          : 'show-scrollbar max-h-[min(420px,56vh)] pb-2 [mask-image:linear-gradient(to_bottom,black_calc(100%-16px),transparent)]'}
      >
        {#if page !== 'ask' && !LOCAL_PAGES.has(page)}
          <Command.Empty class="py-8 text-center text-ui-xs text-muted-foreground">{$tx('cmd.noResults', 'No results')}</Command.Empty>
        {/if}

        <!-- Shared page-destination row (root "Views" + "Go to page" navigator) -->
        {#snippet pageRow(/** @type {typeof pageItems[number]} */ it, /** @type {boolean} */ recent = false)}
          {@const shown = $tx('cmd.' + it.label, it.label)}
          <!-- Translated name shown and searchable; the English words stay in
               the value so either language finds it. A recent row repeats one
               further down, so its value carries a prefix to stay unique. -->
          <Command.Item value={(recent ? 'recent ' : '') + (shown === it.label ? it.value : `${it.value} ${shown}`)} keywords={[shown]} onSelect={() => { recentCommands = rememberCommand(it.label); run(it.action) }}>
            <Icon name={it.icon} class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
            <span data-slot="command-label" class="truncate">{shown}</span>
            {#if it.keys}<Command.Shortcut keys={it.keys} />{/if}
          </Command.Item>
        {/snippet}

        <!-- ── ASK AI (inline quick-ask) ─────────────────────────────── -->
        {#if page === 'ask'}
          <div bind:this={askContentEl} class="select-text px-5 pt-4 pb-6">
            {#each askTurns as turn, ti (turn.id)}
              {#if turn.role === 'user'}
                <!-- The question, as the heading of its exchange. -->
                <h3 class={['text-ui-sm font-semibold leading-snug text-foreground', ti > 0 && 'mt-6 border-t border-border/40 pt-5']}>{turn.text}</h3>
              {:else if turn.role === 'tool'}
                <div class="mt-2.5 flex flex-col gap-1.5">
                  <div class={['inline-flex items-center gap-1.5 text-ui-2xs', turn.status === 'error' ? 'text-destructive' : 'text-muted-foreground']}>
                    {#if turn.status === 'running'}<Icon name="loader-2" class="size-3 shrink-0 animate-spin" />
                    {:else if turn.status === 'error'}<Icon name="x" class="size-3 shrink-0" />
                    {:else}<Icon name="check" class="size-3 shrink-0 text-success" />{/if}
                    <span class="truncate">{turn.label}</span>
                  </div>
                  {#if turn.result && turn.result.columns.length}
                    <div class="overflow-hidden rounded-lg bg-muted/40">
                      <div class="overflow-x-auto">
                        <table class="w-full border-collapse">
                          <thead>
                            <tr>
                              {#each turn.result.columns.slice(0, 5) as c}
                                <th class="whitespace-nowrap px-3 pt-2 pb-1.5 text-left text-ui-3xs font-medium text-muted-foreground">{c}</th>
                              {/each}
                            </tr>
                          </thead>
                          <tbody>
                            {#each turn.result.rows as row}
                              <tr class="border-t border-border/30">
                                {#each row.slice(0, 5) as cell}
                                  <td class={['max-w-[13.75rem] truncate px-3 py-1.5 font-mono text-ui-2xs', cell === null ? 'text-muted-foreground' : 'text-foreground/85']}>{cell === null ? 'NULL' : String(cell)}</td>
                                {/each}
                              </tr>
                            {/each}
                          </tbody>
                        </table>
                      </div>
                      {#if turn.result.total > turn.result.rows.length}
                        <div class="border-t border-border/30 px-3 py-1.5 text-ui-3xs tabular-nums text-muted-foreground">{turn.result.rows.length} of {turn.result.total.toLocaleString()} rows</div>
                      {/if}
                    </div>
                  {/if}
                </div>
              {:else}
                <div class="mt-2.5">
                  {#if turn.streaming && !turn.text}
                    <div class="flex items-center gap-1.5 text-ui-xs text-muted-foreground">
                      <span class="flex gap-0.5" aria-hidden="true">
                        <span class="size-1 animate-pulse rounded-full bg-current"></span>
                        <span class="size-1 animate-pulse rounded-full bg-current [animation-delay:150ms]"></span>
                        <span class="size-1 animate-pulse rounded-full bg-current [animation-delay:300ms]"></span>
                      </span>
                      Thinking
                    </div>
                  {:else}
                    {#await import('./AiMarkdown.svelte') then { default: AiMarkdown }}<AiMarkdown content={turn.text} streaming={turn.streaming} debounceMs={120} class="text-ui-sm leading-relaxed" />{/await}
                  {/if}
                  {#if !turn.streaming && turn.sqls?.length}
                    <div class="mt-2 flex flex-wrap gap-1.5">
                      {#each turn.sqls as s, i}
                        <button type="button" class={i === 0 ? askBtnPrimary : askBtn} onclick={() => void runAskSql(s)}>
                          <Icon name="play" class="size-3 shrink-0" /> Run{turn.sqls.length > 1 ? ` #${i + 1}` : ''}
                        </button>
                        <button type="button" class={askBtn} onclick={() => { const sql = s; open = false; onqueryselect(sql) }}>
                          <Icon name="code-2" class="size-3 shrink-0" /> Open in editor
                        </button>
                      {/each}
                    </div>
                  {/if}
                </div>
              {/if}
            {/each}
            {#if askError}
              <div class="mt-3 flex items-start gap-2 rounded-lg bg-destructive/8 px-3 py-2.5 text-ui-xs text-destructive">
                <Icon name="alert-circle" class="mt-px size-3.5 shrink-0" />
                <span class="min-w-0 flex-1">{askError}</span>
                <button type="button" class="shrink-0 font-medium underline-offset-2 hover:underline" onclick={() => { open = false; onopensettings() }}>Settings</button>
              </div>
            {/if}
          </div>
        {/if}

        <!-- ── ROOT PAGE ─────────────────────────────────────────────── -->
        {#if page === 'root'}
          <!-- Each group is a snippet so a search can put the group with the
               best match first: the keyed each below moves their DOM nodes
               instead of bits-ui rebuilding the list (command-local-filter.svelte.js). -->
          {#snippet group_ask()}
          {#if paletteSearch.trim() && $appCmdkAi}
            <Command.Group value="ask" heading={$tx('group.Ask', 'Ask')}>
              <Command.Item value={"ask ai " + paletteSearch} onSelect={() => startAsk(paletteSearch)}>
                <Icon name="sparkles" class="size-4 shrink-0 text-primary" />
                <span data-slot="command-label" class="truncate">Ask AI: <span class="text-muted-foreground">"{paletteSearch}"</span></span>
                <Command.Shortcut keys="↵" />
              </Command.Item>
            </Command.Group>
          {/if}
          {/snippet}
          {#snippet group_recent()}
            {#if connected}
            {#if recentRows.length && !paletteSearch.trim()}
              <Command.Group value="recent" heading={$tx('group.Recent', 'Recent')}>
                {#each recentRows as it (it.label)}
                  {@render pageRow(it, true)}
                {/each}
              </Command.Group>
            {/if}
            {/if}
          {/snippet}
          {#snippet group_views()}
            {#if connected}
            <Command.Group value="views" heading={$tx('group.Views', 'Views')}>
              {#each listAll ? pagesRanked : pagesRanked.slice(0, FIRST_PAINT_ROWS) as it (it.label)}
                {@render pageRow(it)}
              {/each}
            </Command.Group>
            {/if}
          {/snippet}
          {#snippet group_schemas()}
            {#if connected && listAll}
            {#if schemas.length > 0}
              <Command.Group value="schemas" heading={$tx('group.Schemas', 'Schemas')}>
                {#each schemas as schema (schema)}
                  <Command.Item value="schema {schema}" keywords={[schema]} onSelect={() => run(() => onschemachange(schema))}>
                    <Icon name="database" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                    <span data-slot="command-label" class="truncate font-mono">{schema}</span>
                    {#if schema === activeSchema}
                      <span data-slot="command-trailing" class="ml-auto shrink-0"><Icon name="check" class="size-3.5 shrink-0 text-foreground" /><span class="sr-only">(current)</span></span>
                    {/if}
                  </Command.Item>
                {/each}
              </Command.Group>
            {/if}
            {/if}
          {/snippet}
          {#snippet group_tables()}
            {#if connected && listAll}
            {#if tables.length > 0}
              <Command.Group value="tables" heading={$tx('group.Tables', 'Tables')}>
                {#each regularTables.slice(0, 8) as table (table.name)}
                  <Command.Item value="table {activeSchema} {table.name}" keywords={[table.name]} onSelect={() => run(() => ontableselect(table.name))}>
                    <Icon name="table-2" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                    <span data-slot="command-label" class="truncate font-mono">{table.name}</span>
                    <span data-slot="command-trailing" class="shrink-0 font-mono text-ui-xs tabular-nums text-muted-foreground">{formatTableRowCount(table.rowCount)}</span>
                  </Command.Item>
                {/each}
                {#each viewTables.slice(0, 4) as table (table.name)}
                  <Command.Item value="view {activeSchema} {table.name}" keywords={[table.name]} onSelect={() => run(() => ontableselect(table.name))}>
                    <Icon name="eye" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                    <span data-slot="command-label" class="truncate font-mono">{table.name}</span>
                    <span data-slot="command-trailing" class="shrink-0 text-ui-xs text-muted-foreground">view</span>
                  </Command.Item>
                {/each}
                {#each matViewTables.slice(0, 4) as table (table.name)}
                  <Command.Item value="materialized view {activeSchema} {table.name}" keywords={[table.name]} onSelect={() => run(() => ontableselect(table.name))}>
                    <Icon name="layers" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                    <span data-slot="command-label" class="truncate font-mono">{table.name}</span>
                    <span data-slot="command-trailing" class="shrink-0 text-ui-xs text-muted-foreground">mat. view</span>
                  </Command.Item>
                {/each}
                {#if tables.length > 12}
                  <Command.Item value="browse all tables views search" keywords={[`All ${tables.length} tables & views`]} onSelect={() => navigate('tables')}>
                    <Icon name="table-2" class="size-4 shrink-0 opacity-40" />
                    <span data-slot="command-label" class="truncate text-muted-foreground">All {tables.length} tables & views…</span>
                    <Icon name="chevron-right" class="size-3.5 shrink-0 text-muted-foreground" />
                  </Command.Item>
                {/if}
              </Command.Group>
            {/if}
            {/if}
          {/snippet}
          {#snippet group_ai()}
            {#if connected && listAll}
            <Command.Group value="ai" heading={$tx('group.AI', 'AI')}>
              <Command.Item value="ask ai assistant chat query" onSelect={() => run(onopenai)} keywords={['Ask AI']}>
                <Icon name="bot" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                <span data-slot="command-label" class="truncate">Ask AI</span>
                <Command.Shortcut command="ai.panel" />
              </Command.Item>
              {#if aiSidebarEnabled}
              <Command.Item value="toggle ai sidebar inline assistant context" onSelect={() => run(onopenaisidebar)} keywords={['AI sidebar']}>
                <Icon name="sparkles" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                <span data-slot="command-label" class="truncate">AI sidebar</span>
                <Command.Shortcut command="ai.sidebar" />
              </Command.Item>
              {/if}
              <Command.Item
                value={aiMode ? "close ai panel hide assistant" : "open ai panel show assistant chat"}
                keywords={[aiMode ? 'Close AI panel' : 'Open AI panel']}
                onSelect={() => run(ontoggleaimode)}
              >
                <Icon name="arrow-left-right" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                <span data-slot="command-label" class="truncate">{aiMode ? 'Close AI panel' : 'Open AI panel'}</span>
              </Command.Item>
            </Command.Group>
            {/if}
          {/snippet}
          {#snippet group_queries()}
            {#if connected && listAll}
            <Command.Group value="queries" heading={$tx('group.Queries', 'Queries')}>
              <Command.Item value="open query history sql statements" onSelect={() => run(onopenqueryhistory)} keywords={['Query history']}>
                <Icon name="history" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                <span data-slot="command-label" class="truncate">Query history</span>
              </Command.Item>
            </Command.Group>
            {/if}
          {/snippet}
          {#snippet group_saved()}
            {#if connected && listAll}
            {#if savedQueries.length > 0}
              <Command.Group value="saved" heading={$tx('group.Saved queries', 'Saved queries')}>
                {#each savedQueries as entry (entry.id)}
                  <Command.Item value="saved query {entry.name} {entry.sql}" keywords={[entry.name]} onSelect={() => run(() => onqueryselect(entry.sql))}>
                    <Icon name="bookmark" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                    <span data-slot="command-label" class="min-w-0 truncate text-ui-xs">{entry.name}</span>
                  </Command.Item>
                {/each}
              </Command.Group>
            {/if}
            {/if}
          {/snippet}
          {#snippet group_recentq()}
            {#if connected && listAll}
            {#if queryHistory.length > 0}
              <Command.Group value="recentq" heading={$tx('group.Recent queries', 'Recent queries')}>
                {#each queryHistory.slice(0, 20) as entry (entry.id)}
                  <Command.Item value="recent query {entry.title} {entry.sql}" keywords={[entry.title ?? '']} onSelect={() => run(() => onqueryselect(entry.sql))}>
                    <Icon name="history" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                    <span data-slot="command-label" class="min-w-0 truncate font-mono text-ui-xs">{entry.title}</span>
                  </Command.Item>
                {/each}
              </Command.Group>
            {/if}
            {/if}
          {/snippet}
          {#snippet group_export()}
            {#if connected && listAll}
            {#if (onexporttable || onimporttable) && paletteSearch.trim()}
              <!-- Export was only in the table toolbar's menu: "export" here
                   found Backup & restore and nothing else. Only while searching,
                   so eight format rows don't sit in the root list. -->
              <Command.Group value="export" heading={$tx('group.Export and import', 'Export and import')}>
                {#each onexporttable ? EXPORT_FORMATS : [] as fmt (fmt.id)}
                  <Command.Item value={`export download save table rows ${fmt.label} ${fmt.id} ${fmt.hint}`} keywords={[`Export as ${fmt.label}`]} onSelect={() => run(() => onexporttable?.(fmt.id))}>
                    <Icon name="file-down" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                    <span data-slot="command-label" class="truncate">Export {exportTableName || 'table'} as {fmt.label}</span>
                    <span data-slot="command-trailing" class="shrink-0 text-ui-xs text-muted-foreground">{fmt.hint}</span>
                  </Command.Item>
                {/each}
                {#if onimporttable}
                  <Command.Item value="import load upload rows file csv json excel into table" keywords={['Import data']} onSelect={() => run(onimporttable)}>
                    <Icon name="file-up" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                    <span data-slot="command-label" class="truncate">Import data into {exportTableName || 'table'}…</span>
                  </Command.Item>
                {/if}
              </Command.Group>
            {/if}
            {/if}
          {/snippet}
          {#snippet group_table()}
            <!-- The table toolbar by name, while searching: "filter", "sort" or
                 "add row" found a table called audit_notification_log_filters,
                 or nothing. -->
            {#if tableCommands && paletteSearch.trim()}
            <Command.Group value="table" heading={$tx('group.Table', 'Table')}>
                <Command.Item minScore={0.3} value="filter rows where condition narrow" keywords={['Filter rows']} onSelect={() => run(/** @type {() => void} */ (tableCommands?.filter))}>
                  <Icon name="filter" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Filter rows</span>
                  <Command.Shortcut command="table.filterMenu" />
                </Command.Item>
                <Command.Item minScore={0.3} value="sort rows order by ascending descending" keywords={['Sort rows']} onSelect={() => run(/** @type {() => void} */ (tableCommands?.sort))}>
                  <Icon name="arrow-up-down" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Sort rows</span>
                  <Command.Shortcut command="table.sortMenu" />
                </Command.Item>
                <Command.Item minScore={0.3} value="show hide columns visible column picker" keywords={['Show or hide columns']} onSelect={() => run(/** @type {() => void} */ (tableCommands?.columns))}>
                  <Icon name="eye" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Show or hide columns</span>
                  <Command.Shortcut command="table.columnsMenu" />
                </Command.Item>
              {#if tableCommands.addRow}
                <Command.Item minScore={0.3} value="add row insert new record" keywords={['Add row']} onSelect={() => run(/** @type {() => void} */ (tableCommands?.addRow))}>
                  <Icon name="plus" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Add row</span>
                  <Command.Shortcut command="table.addRow" />
                </Command.Item>
              {/if}
              {#if tableCommands.findReplace}
                <Command.Item minScore={0.3} value="find and replace in column search substitute" keywords={['Find and replace']} onSelect={() => run(/** @type {() => void} */ (tableCommands?.findReplace))}>
                  <Icon name="replace" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Find and replace</span>
                  <Command.Shortcut command="table.findReplace" />
                </Command.Item>
              {/if}
                <Command.Item minScore={0.3} value="refresh rows reload data table" keywords={['Refresh rows']} onSelect={() => run(/** @type {() => void} */ (tableCommands?.refresh))}>
                  <Icon name="refresh-cw" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Refresh rows</span>
                </Command.Item>
                {#if tableCommands.view}
                  <Command.Item minScore={0.3} value="table grid rows view" keywords={['Table view']} onSelect={() => run(() => tableCommands?.view?.('table'))}>
                    <Icon name="table-2" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                    <span data-slot="command-label" class="truncate">Table view</span>
                    <Command.Shortcut command="table.view.1" />
                  </Command.Item>
                {/if}
                {#if tableCommands.view}
                  <Command.Item minScore={0.3} value="json view objects" keywords={['JSON view']} onSelect={() => run(() => tableCommands?.view?.('json'))}>
                    <Icon name="braces" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                    <span data-slot="command-label" class="truncate">JSON view</span>
                    <Command.Shortcut command="table.view.2" />
                  </Command.Item>
                {/if}
                {#if tableCommands.view}
                  <Command.Item minScore={0.3} value="record view one row form card" keywords={['Record view']} onSelect={() => run(() => tableCommands?.view?.('record'))}>
                    <Icon name="file-text" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                    <span data-slot="command-label" class="truncate">Record view</span>
                    <Command.Shortcut command="table.view.3" />
                  </Command.Item>
                {/if}
                {#if tableCommands.view}
                  <Command.Item minScore={0.3} value="text view plain ascii" keywords={['Text view']} onSelect={() => run(() => tableCommands?.view?.('text'))}>
                    <Icon name="wrap-text" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                    <span data-slot="command-label" class="truncate">Text view</span>
                    <Command.Shortcut command="table.view.4" />
                  </Command.Item>
                {/if}
                {#if tableCommands.view}
                  <Command.Item minScore={0.3} value="chart view graph plot visualize" keywords={['Chart view']} onSelect={() => run(() => tableCommands?.view?.('chart'))}>
                    <Icon name="bar-chart-2" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                    <span data-slot="command-label" class="truncate">Chart view</span>
                    <Command.Shortcut command="table.view.5" />
                  </Command.Item>
                {/if}
                <Command.Item minScore={0.3} value="open table in sql editor select query" keywords={['Open in SQL editor']} onSelect={() => run(() => tableCommands?.openInSql())}>
                  <Icon name="terminal" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Open in SQL editor</span>
                </Command.Item>
            </Command.Group>
            {/if}
          {/snippet}
          {#snippet group_view()}
            {#if viewCommands && paletteSearch.trim()}
            <Command.Group value="view" heading={$tx('group.View', 'View')}>
                <Command.Item minScore={0.3} value="zoom in bigger larger text scale" keywords={['Zoom in']} onSelect={() => run(/** @type {() => void} */ (viewCommands?.zoomIn))}>
                  <Icon name="plus" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Zoom in</span>
                </Command.Item>
                <Command.Item minScore={0.3} value="zoom out smaller text scale" keywords={['Zoom out']} onSelect={() => run(/** @type {() => void} */ (viewCommands?.zoomOut))}>
                  <Icon name="minus" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Zoom out</span>
                </Command.Item>
                <Command.Item minScore={0.3} value="actual size reset zoom 100%" keywords={['Actual size']} onSelect={() => run(/** @type {() => void} */ (viewCommands?.zoomReset))}>
                  <Icon name="square" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Actual size</span>
                </Command.Item>
                <Command.Item minScore={0.3} value="full screen fullscreen window maximize" keywords={['Full screen']} onSelect={() => run(/** @type {() => void} */ (viewCommands?.fullscreen))}>
                  <Icon name="maximize-2" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Full screen</span>
                </Command.Item>
            </Command.Group>
            {/if}
          {/snippet}
          {#snippet group_tabs()}
            <!-- Only while searching, as Export: "close", "reopen" or "pin"
                 found nothing here, and four more rows in the root list
                 would push the views down. -->
            {#if tabActions && paletteSearch.trim()}
            <Command.Group value="tabs" heading={$tx('group.Tabs', 'Tabs')}>
              {#if tabActions.reopen}
                <Command.Item minScore={0.3} value="reopen closed tab undo close restore last tab" keywords={['Reopen closed tab']} onSelect={() => run(/** @type {() => void} */ (tabActions?.reopen))}>
                  <Icon name="rotate-ccw" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Reopen closed tab</span>
                  <Command.Shortcut command="tab.reopen" />
                </Command.Item>
              {/if}
              {#if tabActions.close}
                <Command.Item minScore={0.3} value="close tab current this" keywords={['Close tab']} onSelect={() => run(/** @type {() => void} */ (tabActions?.close))}>
                  <Icon name="x" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Close tab</span>
                  <Command.Shortcut command="tab.close" />
                </Command.Item>
              {/if}
              {#if tabActions.closeOthers}
                <Command.Item minScore={0.3} value="close other tabs all but this keep only" keywords={['Close other tabs']} onSelect={() => run(/** @type {() => void} */ (tabActions?.closeOthers))}>
                  <Icon name="layers" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Close other tabs</span>
                </Command.Item>
              {/if}
              {#if tabActions.togglePin}
                <Command.Item minScore={0.3} value="pin unpin tab keep stick" keywords={[tabActions.pinned ? 'Unpin tab' : 'Pin tab']} onSelect={() => run(/** @type {() => void} */ (tabActions?.togglePin))}>
                  <Icon name={tabActions.pinned ? 'pin-off' : 'pin'} class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">{tabActions.pinned ? 'Unpin tab' : 'Pin tab'}</span>
                </Command.Item>
              {/if}
            </Command.Group>
            {/if}
          {/snippet}
          {#snippet group_actions()}
            {#if connected && listAll}
            <Command.Group value="actions" heading={$tx('group.Actions', 'Actions')}>
              {#if onexplainquery}
                <!-- Explain was only a button in the results bar. -->
                <Command.Item value="explain query plan analyze cost why slow performance" keywords={['Explain query']} onSelect={() => run(onexplainquery)}>
                  <Icon name="scan-search" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Explain query</span>
                </Command.Item>
              {/if}
              {#if ontablestructure}
                <!-- The structure view was only in the table toolbar's overflow
                     menu: "structure" here found nothing. -->
                <Command.Item value="table structure columns types indexes foreign keys constraints edit columns alter schema ddl back to data rows" keywords={[tableStructureShown ? 'Back to data' : 'Table structure']} onSelect={() => run(ontablestructure)}>
                  <Icon name={tableStructureShown ? 'table-2' : 'columns-3'} class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">{tableStructureShown ? `Back to the rows of ${exportTableName || 'the table'}` : `Structure of ${exportTableName || 'the table'}`}</span>
                </Command.Item>
              {/if}
              {#if tabActions?.newTable}
                <!-- Only the sidebar's + made a table. -->
                <Command.Item value="new table create table add schema design columns" keywords={['New table']} onSelect={() => run(/** @type {() => void} */ (tabActions?.newTable))}>
                  <Icon name="plus" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">New table…</span>
                </Command.Item>
              {/if}
              <Command.Item value="refresh schema tables" onSelect={() => run(onrefresh)} keywords={['Refresh tables']}>
                <Icon name="refresh-cw" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                <span data-slot="command-label" class="truncate">Refresh tables</span>
              </Command.Item>
              <Command.Item value="read only read-only lock mode protect prevent edits writes inserts deletes writable" keywords={[readonly ? 'Disable read-only mode' : 'Enable read-only mode']} onSelect={() => run(onreadonlytoggle)}>
                <Icon name={readonly ? 'lock-open' : 'lock'} class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                <span data-slot="command-label" class="truncate">{readonly ? 'Disable read-only mode' : 'Enable read-only mode'}</span>
              </Command.Item>
              <Command.Item value="open settings preferences" onSelect={() => run(onopensettings)} keywords={['Settings']}>
                <Icon name="settings" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                <span data-slot="command-label" class="truncate">Settings</span>
                <Command.Shortcut command="settings.open" />
              </Command.Item>
              {#if ontogglesidebar}
                <Command.Item value="toggle sidebar hide show tables list panel" onSelect={() => run(() => ontogglesidebar?.())} keywords={['Toggle sidebar']}>
                  <Icon name="panel-left-close" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Toggle sidebar</span>
                  <Command.Shortcut command="view.sidebar" />
                </Command.Item>
              {/if}
              {#if onback}
                <Command.Item value="go back previous tab history" onSelect={() => run(() => onback?.())} keywords={['Go back']}>
                  <Icon name="chevron-left" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Go back</span>
                  <Command.Shortcut command="tab.back" />
                </Command.Item>
              {/if}
              {#if onforward}
                <Command.Item value="go forward next tab history" onSelect={() => run(() => onforward?.())} keywords={['Go forward']}>
                  <Icon name="chevron-right" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Go forward</span>
                  <Command.Shortcut command="tab.forward" />
                </Command.Item>
              {/if}
              <Command.Item value="inset content panel layout linear rounded flat appearance" onSelect={() => run(() => updateSettings({ insetPanel: document.documentElement.dataset.layout !== 'inset' }))} keywords={['Toggle inset content panel']}>
                <Icon name="layout-template" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                <span data-slot="command-label" class="truncate">Toggle inset content panel</span>
              </Command.Item>
              <Command.Item value="toggle tree sidebar beta explorer classic hierarchy folders" onSelect={() => run(() => updateSettings({ sidebarStyle: $appSidebarStyle === 'tree' ? 'classic' : 'tree' }))} keywords={['Toggle tree sidebar (Labs)']}>
                <Icon name="list-tree" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                <span data-slot="command-label" class="truncate">Toggle tree sidebar (Labs)</span>
                <span data-slot="command-trailing" class="shrink-0 text-ui-xs text-muted-foreground">{$appSidebarStyle === 'tree' ? 'on' : 'off'}</span>
              </Command.Item>
              {#if $pinEnabled}
                <Command.Item value="lock app pin screen secure" onSelect={() => run(lockNow)} keywords={['Lock Stroke']}>
                  <Icon name="lock" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">Lock Stroke</span>
                </Command.Item>
              {/if}
              <Command.Item value="keyboard shortcuts keybindings remap customize change rebind hotkeys" onSelect={() => run(onopenkeybindings)} keywords={['Keyboard shortcuts']}>
                <Icon name="keyboard" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                <span data-slot="command-label" class="truncate">Keyboard shortcuts</span>
              </Command.Item>
              <Command.Item value="shortcut reference cheat sheet keys help" onSelect={() => run(onopenshortcuts)} keywords={['Shortcut reference']}>
                <Icon name="command" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                <span data-slot="command-label" class="truncate">Shortcut reference</span>
                <Command.Shortcut command="help.shortcutsKey" />
              </Command.Item>
              <Command.Item value="my shares share links shared revoke views encrypted link" onSelect={() => run(onopenmyshares)} keywords={['My shares']}>
                <Icon name="share-2" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                <span data-slot="command-label" class="truncate">My shares</span>
              </Command.Item>
              <Command.Item value="about license version info app" onSelect={() => run(onopenabout)} keywords={['About Stroke']}>
                <Icon name="info" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                <span data-slot="command-label" class="truncate">About Stroke</span>
              </Command.Item>
              <Command.Item value="report issue bug crash problem feedback github" onSelect={() => run(onopenreport)} keywords={['Report an issue']}>
                <Icon name="bug" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                <span data-slot="command-label" class="truncate">Report an issue</span>
              </Command.Item>
              <Command.Item value="check for updates upgrade version" onSelect={() => run(oncheckupdate)} keywords={['Check for updates']}>
                <Icon name="arrow-down-to-line" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                <span data-slot="command-label" class="truncate">Check for updates</span>
              </Command.Item>
              <Command.Item value="disconnect database" onSelect={() => run(ondisconnect)} keywords={['Disconnect']}>
                <Icon name="unplug" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                <span data-slot="command-label" class="truncate">Disconnect</span>
                <Command.Shortcut command="connection.disconnect" />
              </Command.Item>
            </Command.Group>
            {/if}
          {/snippet}
          {#snippet group_switchdb()}
            {#if listAll || !connected}

          <!-- ── Switch database, other saved connections, right at root ── -->
          {@const otherConns = savedConnections.filter((c) => c.id !== activeConnectionId)}
          {#if otherConns.length > 0}
            <Command.Group value="switchdb" heading={$tx('group.Switch database', 'Switch database')}>
              {#each otherConns.slice(0, 8) as conn (conn.id)}
                <Command.Item
                  value="switch database connection {conn.name} {connSubtitle(conn)} {conn.type}"
                  keywords={[conn.name]}
                  onSelect={() => run(() => onswitchdatabase(conn))}
                >
                  <Icon name={driverIcon(conn.type ?? 'postgres')} class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <div data-slot="command-label" class="flex min-w-0 flex-1 flex-col">
                    <span class="truncate">{conn.name}</span>
                    <span class="truncate font-mono text-ui-2xs text-muted-foreground">{connSubtitle(conn)}</span>
                  </div>
                  {#if savedConnections.indexOf(conn) < 9}
                    <Command.Shortcut command="connection.switchTo.{savedConnections.indexOf(conn) + 1}" />
                  {/if}
                </Command.Item>
              {/each}
            </Command.Group>
          {/if}
            {/if}
          {/snippet}
          {#snippet group_database()}
            {#if listAll || !connected}

          <!-- ── Drill-in: Connections ───────────────────────────────── -->
          <Command.Group value="database" heading={$tx('group.Database', 'Database')}>
            <Command.Item
              value="connections switch database connect postgres mysql sqlite saved {savedConnections.map(c => c.name).join(' ')}"
              keywords={['Connections']}
              onSelect={() => navigate('connections')}
            >
              <Icon name="database" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
              <span data-slot="command-label" class="truncate">Connections</span>
              {#if savedConnections.length > 0}
                <span class="shrink-0 text-ui-xs tabular-nums text-muted-foreground">{savedConnections.length}</span>
              {/if}
              <Icon name="chevron-right" class="size-3.5 shrink-0 text-muted-foreground" />
            </Command.Item>
            <!-- Also at the top level: "new connection" found nothing here
                 because the item lived only inside Connections. -->
            <Command.Item value="new connection add database server" keywords={['New connection']} onSelect={() => run(onopenconnection)}>
              <Icon name="plus" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
              <span data-slot="command-label" class="truncate">New connection…</span>
            </Command.Item>
          </Command.Group>
            {/if}
          {/snippet}
          {#snippet group_launch()}
            {#if listAll || !connected}

          <!-- ── Drill-in: Docker ───────────────────────────────────── -->
          <Command.Group value="launch" heading={$tx('group.Launch', 'Launch')}>
            <Command.Item value="docker containers launch run postgresql mysql mariadb mongodb redis valkey sql server oracle cockroachdb clickhouse cassandra couchdb elasticsearch opensearch dynamodb libsql pgvector postgis timescale" onSelect={() => navigate('docker')} keywords={['Docker']}>
              <DbIcon id="docker" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
              <span data-slot="command-label" class="truncate">Docker</span>
              <Icon name="chevron-right" class="size-3.5 shrink-0 text-muted-foreground" />
            </Command.Item>
          </Command.Group>
            {/if}
          {/snippet}
          {#snippet group_appearance()}
            {#if listAll || !connected}

          <!-- ── Drill-in: Theme ────────────────────────────────────── -->
          <Command.Group value="appearance" heading={$tx('group.Appearance', 'Appearance')}>
            <Command.Item value="theme appearance colour color scheme palette dark light mode switch change" onSelect={openThemePage} keywords={['Theme…']}>
              <Icon name={getThemeDefinition($appThemeId)?.isDark ? 'moon' : 'sun'} class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
              <span data-slot="command-label" class="truncate">Theme…</span>
              <span data-slot="command-trailing" class="flex shrink-0 items-center gap-2">
                <span class="text-ui-xs text-muted-foreground">{getThemeDefinition($appThemeId)?.name}</span>
                <!-- The picker has its own chord (Mod+Alt+T): say so where it is found. -->
                <Command.Shortcut command="theme.pick" />
                <Icon name="chevron-right" class="size-3.5 shrink-0 text-muted-foreground" />
              </span>
            </Command.Item>
            <!-- The builder lives in Settings; "theme builder" found nothing here. -->
            <Command.Item value="theme builder custom make your own colours colors grain intensity" onSelect={() => run(onopenappearance)} keywords={['Theme builder']}>
              <Icon name="sparkles" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
              <span data-slot="command-label" class="truncate">Theme builder</span>
            </Command.Item>
          </Command.Group>
            {/if}
          {/snippet}
          {#each rootOrder as id (id)}
            {#if id === 'ask'}{@render group_ask()}
            {:else if id === 'recent'}{@render group_recent()}
            {:else if id === 'views'}{@render group_views()}
            {:else if id === 'schemas'}{@render group_schemas()}
            {:else if id === 'tables'}{@render group_tables()}
            {:else if id === 'ai'}{@render group_ai()}
            {:else if id === 'queries'}{@render group_queries()}
            {:else if id === 'saved'}{@render group_saved()}
            {:else if id === 'recentq'}{@render group_recentq()}
            {:else if id === 'export'}{@render group_export()}
            {:else if id === 'actions'}{@render group_actions()}
            {:else if id === 'tabs'}{@render group_tabs()}
            {:else if id === 'table'}{@render group_table()}
            {:else if id === 'view'}{@render group_view()}
            {:else if id === 'switchdb'}{@render group_switchdb()}
            {:else if id === 'database'}{@render group_database()}
            {:else if id === 'launch'}{@render group_launch()}
            {:else if id === 'appearance'}{@render group_appearance()}
            {/if}
          {/each}
        {:else if page === 'tables'}
          {#if tablesPageRegular.total > 0}
            <Command.Group heading={$tx('group.Tables', 'Tables')}>
              {#each tablesPageRegular.items as table (table.name)}
                <Command.Item value="table {activeSchema} {table.name} {table.name}" onSelect={() => run(() => ontableselect(table.name))}>
                  <Icon name="table-2" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate font-mono">{table.name}</span>
                  <span
                    data-slot="command-trailing"
                    class="shrink-0 font-mono text-ui-xs tabular-nums text-muted-foreground"
                    title={table.rowCount != null ? formatFullCount(table.rowCount) : undefined}
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
            <Command.Group heading={$tx('group.Views', 'Views')}>
              {#each tablesPageViews.items as table (table.name)}
                <Command.Item value="view {activeSchema} {table.name} {table.name}" onSelect={() => run(() => ontableselect(table.name))}>
                  <Icon name="eye" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
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
            <Command.Group heading={$tx('group.Materialized views', 'Materialized views')}>
              {#each tablesPageMatViews.items as table (table.name)}
                <Command.Item value="materialized view {activeSchema} {table.name} {table.name}" onSelect={() => run(() => ontableselect(table.name))}>
                  <Icon name="layers" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
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
          {#if dockerRunning.length}
            <Command.Group heading={$tx('group.Running', 'Running')}>
              {#each dockerRunning as d (d.containerId)}
                <Command.Item
                  value="running open {d.name} {d.engine} {d.image} {d.port}"
                  onSelect={() => run(() => onopendocker({ id: `docker-${d.name}`, type: d.engine, name: d.name, host: d.host, port: d.port, user: d.user, password: d.password, database: d.database }))}
                >
                  <DbIcon id={d.engine} class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">{d.name}</span>
                  <span data-slot="command-trailing" class="shrink-0 font-mono text-ui-2xs text-muted-foreground">{d.target}</span>
                </Command.Item>
              {/each}
            </Command.Group>
          {/if}
          {#each dockerGroups as [group, items] (group)}
            <Command.Group heading={group}>
              {#each items as t (t.id)}
                <Command.Item
                  value="launch {t.label} {t.engine} {t.image} {t.note} docker"
                  onSelect={() => run(() => ondockerlaunch(t.id))}
                >
                  <DbIcon id={t.icon} class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
                  <span data-slot="command-label" class="truncate">{t.label}{#if t.note}<span class="ml-2 text-ui-2xs text-muted-foreground">{t.note}</span>{/if}</span>
                  <span data-slot="command-trailing" class="shrink-0 font-mono text-ui-2xs text-muted-foreground">{t.image}</span>
                </Command.Item>
              {/each}
            </Command.Group>
          {/each}
          {#if !dockerTpls.length}
            <div class="px-3 py-6 text-center text-ui-xs text-muted-foreground">Loading images…</div>
          {/if}

        <!-- ── CONNECTIONS PAGE ───────────────────────────────────────── -->
        {:else if page === 'connections'}
          {#if savedConnections.length > 0}
            <Command.Group heading={$tx('group.Saved connections', 'Saved connections')}>
              {#each savedConnections as conn (conn.id)}
                {@const driverIconName = driverIcon(conn.type ?? 'postgres')}
                {@const isActive = conn.id === activeConnectionId}
                <Command.Item
                  value="connection {conn.name} {connSubtitle(conn)} {conn.type}"
                  onSelect={() => run(() => onswitchdatabase(conn))}
                  disabled={isActive}
                >
                  <Icon name={driverIconName} class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
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
          <Command.Group heading={$tx('group.Add', 'Add')}>
            <Command.Item value="new connection add connect database" onSelect={() => run(onopenconnection)} keywords={['New connection…']}>
              <Icon name="database" class="size-4 shrink-0 text-muted-foreground group-data-[selected]/command-item:text-foreground" />
              <span data-slot="command-label" class="truncate">New connection…</span>
            </Command.Item>
          </Command.Group>

        {:else if page === 'pages'}
          <!-- Go to page, VSCode-style focused navigator (Ctrl/⌘+P) -->
          <Command.Group heading={$tx('group.Go to page', 'Go to page')}>
            {#each pagesRanked as it (it.label)}
              {@render pageRow(it)}
            {/each}
          </Command.Group>

        <!-- ── THEME PAGE ─────────────────────────────────────────────── -->
        {:else if page === 'theme'}
          {#each themePageGroups as group (group.id)}
            <Command.Group heading={group.label}>
              {#each group.themes as t (t.id)}
                <Command.Item value={themeRowValue(t, group.label)} onSelect={() => commitTheme(t.id)}>
                  <span data-slot="command-label" class="truncate">{t.name}</span>
                  <span data-slot="command-trailing" class="flex shrink-0 items-center gap-2.5">
                    {#if t.id === themeOrigin}
                      <Icon name="check" class="size-3.5 shrink-0 text-primary" />
                      <span class="sr-only">(current)</span>
                    {/if}
                    <ThemeSwatchStrip themeId={t.id} />
                  </span>
                </Command.Item>
              {/each}
            </Command.Group>
          {/each}
        {/if}
        <!-- A page that filters its own rows draws its own empty state: bits-ui's
             search is empty there, so its Command.Empty never shows. -->
        {#if LOCAL_PAGES.has(page) && localFilter.search && localFilter.count === 0}
          <p class="py-8 text-center text-ui-xs text-muted-foreground">{$tx('cmd.noResults', 'No results')}</p>
        {/if}

      </Command.List>

      {#if page === 'ask'}
        <!-- Action bar, like Raycast's: what's answering on the left, the
             actions and their keys on the right. -->
        <div class="flex h-10 shrink-0 items-center gap-2 border-t border-border/50 bg-muted/20 ps-4 pe-2 text-ui-2xs text-muted-foreground">
          <Icon name="sparkles" class="size-3.5 shrink-0 text-primary" />
          <span class="min-w-0 truncate">{askModel}</span>
          {#if askStreaming}<span class="shrink-0 text-muted-foreground">· answering</span>{/if}
          <span class="flex-1"></span>
          {#if askStreaming}
            <button type="button" class={askBar} onclick={stopAsk}><Icon name="square" class="size-3 shrink-0" />Stop</button>
          {:else}
            {#if lastAnswer}
              <button type="button" class={askBar} onclick={() => void copyAnswer()}>{answerCopied ? 'Copied' : 'Copy answer'} <Kbd combo="Mod+Shift+C" /></button>
            {/if}
            <span class="w-2 shrink-0" aria-hidden="true"></span>
            <button type="button" class={cn(askBar, 'text-foreground')} onclick={continueInChat}>Continue in chat <Kbd combo="Mod+Enter" /></button>
          {/if}
        </div>
      {/if}
      <!-- The keys, said once at the bottom, as the palette's own legend. -->
      {#if page === 'theme'}
        <div class="flex h-10 shrink-0 items-center gap-4 border-t border-border/50 bg-muted/25 px-3.5 text-ui-2xs text-muted-foreground">
          <span class="flex items-center gap-1.5"><Kbd keys={['↑']} /><Kbd keys={['↓']} /> Preview</span>
          <span class="ml-auto flex items-center gap-1.5">Select theme <Kbd keys={['↵']} /></span>
          <span class="flex items-center gap-1.5">Cancel <Kbd keys={['esc']} /></span>
        </div>
      {:else if page !== 'ask'}
        <div class="flex h-10 shrink-0 items-center gap-4 border-t border-border/50 bg-muted/25 px-3.5 text-ui-2xs text-muted-foreground">
          <span class="flex items-center gap-1.5"><Kbd keys={['↑']} /><Kbd keys={['↓']} /> {$tx('cmd.foot.navigate', 'Navigate')}</span>
          <span class="flex items-center gap-1.5"><Kbd keys={['↵']} /> {$tx('cmd.foot.select', 'Select')}</span>
          <span class="flex items-center gap-1.5"><Kbd keys={['esc']} /> {$tx('common.close', 'Close')}</span>
        </div>
      {/if}
    </div>
  {/snippet}
</Command.Dialog>
