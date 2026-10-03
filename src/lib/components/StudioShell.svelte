<script>
  import { onMount, onDestroy, untrack, tick } from 'svelte'
  import { fade } from 'svelte/transition'
  import { revealApp, isRevealed } from '$lib/app-reveal.js'
  import { setReadOnly } from '$lib/stores/read-only.js'
  import { isWriteSql } from '$lib/sql-write.js'
  import Logo from './Logo.svelte'
  import ConnectOverlay from './ConnectOverlay.svelte'
  import Database from '@lucide/svelte/icons/database'
  import Boxes from '@lucide/svelte/icons/boxes'
  import FileCode2 from '@lucide/svelte/icons/file-code-2'
  import Terminal from '@lucide/svelte/icons/terminal'
  import Sparkles from '@lucide/svelte/icons/sparkles'
  import LayoutTemplate from '@lucide/svelte/icons/layout-template'
  import { cn } from '$lib/utils.js'
  import Command from '@lucide/svelte/icons/command'
  import Code2 from '@lucide/svelte/icons/code-2'
  import ShieldCheck from '@lucide/svelte/icons/shield-check'
  import ScrollText from '@lucide/svelte/icons/scroll-text'
  import BarChart2 from '@lucide/svelte/icons/bar-chart-2'
  import LayoutDashboard from '@lucide/svelte/icons/layout-dashboard'
  import GitBranch from '@lucide/svelte/icons/git-branch'
  import Blocks from '@lucide/svelte/icons/blocks'
  import GitCompare from '@lucide/svelte/icons/git-compare'
  import History from '@lucide/svelte/icons/history'
  import Plus from '@lucide/svelte/icons/plus'
  import Search from '@lucide/svelte/icons/search'
  import Gauge from '@lucide/svelte/icons/gauge'
  import Network from '@lucide/svelte/icons/network'
  import { createHotkey } from '@tanstack/svelte-hotkeys'
  import { IS_MAC } from '$lib/shortcuts.js'
  import { findSearchInput, isTypingTarget } from '$lib/focus-search.js'
  import { appFkAutoExpandJson, cycleTheme, restorePreviousTheme, isCurrentThemeDark, loadSettings, appPaginationMode, appVimMode, appAutoSaveQueries, appStreamResults, increaseZoom, decreaseZoom, resetZoom } from '$lib/stores/settings.js'
  import { requireUnlock } from '$lib/stores/app-lock.js'
  import { isTextEntryTarget, setVimSubMode } from '$lib/vim/vim.js'
  import { normalizeColumn, columnType } from '$lib/column.js'
  import {
    loadAiMode, saveAiMode, loadHiddenCols, saveHiddenCols,
    loadQueryHistoryPref, saveQueryHistoryPref, loadInfiniteScroll, saveInfiniteScroll,
  } from '$lib/stores/table-prefs.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { startTelemetry, track } from '$lib/telemetry.js'
  import Sidebar from './Sidebar.svelte'
  import TabBar from './TabBar.svelte'
  import PaneLayout from './PaneLayout.svelte'
  import PaneSnapshot from './PaneSnapshot.svelte'
  import * as PaneTree from '$lib/pane-layout.js'
  import TabLoading from './TabLoading.svelte'
  import TableToolbar from './TableToolbar.svelte'
  import ImportDataDialog from './ImportDataDialog.svelte'
  import DataTable from './DataTable.svelte'
  import RowDetailPanel from './RowDetailPanel.svelte'
  // TableJsonView / TableTextView are NOT imported here: both reach monaco-editor
  // statically, which would drag ~3.7 MB of Monaco (plus its CSS) into the boot
  // chunk even though neither view is on screen until the user picks that data
  // view mode. They load via {#await import()} at their (already guarded) call
  // sites below, exactly like MapPage / EntityRelationPage.
  import TableRecordView from './TableRecordView.svelte'
  import ChartView from './ChartView.svelte'
  import CommandPalette from './CommandPalette.svelte'
  // AiChat / AiSidebar (large, pull in marked + shiki) are loaded lazily the first time
  // the AI panel is opened - see the {#await import()} blocks below.
  import AiSettingsDialog from './AiSettingsDialog.svelte'
  import ConnectionModal from './ConnectionModal.svelte'
  import DbIcon from './DbIcon.svelte'
  import DockerLaunchModal from './DockerLaunchModal.svelte'
  import CreateTableDialog from './CreateTableDialog.svelte'
  import CreateSchemaDialog from './CreateSchemaDialog.svelte'
  import GenerateSqlDialog from './GenerateSqlDialog.svelte'
  import { genSelectStar } from '$lib/sql-generate.js'
  import { qualifiedTable } from '$lib/dml-preview.js'
  import { pluginState, pluginEnabledIn } from '$lib/stores/plugins.js'
  import { loadTableViews, saveTableViews } from '$lib/stores/table-views.js'
  import { loadSqlDraft, saveSqlDraft } from '$lib/stores/sql-draft.js'
  import { buildBatchUpdateSql } from '$lib/sql-batch-update.js'
  import { buildSearchQuery, searchOptionsSupported, supportedSearchOptions } from '$lib/search-options.js'
  import Onboarding from './Onboarding.svelte'
  import SettingsDialog from './SettingsDialog.svelte'
  import KeyboardShortcutsDialog from './KeyboardShortcutsDialog.svelte'
  import InsiderDialog from './InsiderDialog.svelte'
  import AboutDialog from './AboutDialog.svelte'
  import ReportIssueDialog from './ReportIssueDialog.svelte'
  import UpdateDialog from './UpdateDialog.svelte'
  import StatusBar from './StatusBar.svelte'
  import QueryLogConsole from './QueryLogConsole.svelte'
  import DisconnectDialog from './DisconnectDialog.svelte'
  import SwitchDatabaseDialog from './SwitchDatabaseDialog.svelte'
  import CreateDatabaseDialog from './CreateDatabaseDialog.svelte'
  import DatabaseNameDialog from './DatabaseNameDialog.svelte'
  import DropDatabaseDialog from './DropDatabaseDialog.svelte'
  import DatabaseInfoDialog from './DatabaseInfoDialog.svelte'
  import { CatalogCache, catalogKey, connectionPrefix } from '$lib/catalog-cache.js'
  import { refreshExternalPlugins, stopAllExternalPlugins } from '$lib/plugins/external/host.js'
  import {
    dbAdminKind,
    createDatabaseSql,
    dropDatabaseSql,
    terminateSessionsSql,
    databaseInfoSql,
    databaseInfoRows,
  } from '$lib/database-admin.js'
  import McpPanel from './McpPanel.svelte'
  // NotebookEditor (pulls Monaco via SqlCell + marked via MarkdownCell) is lazy-loaded
  // at its render site so notebooks don't drag those into the startup bundle.
  //
  // The keep-alive tab pages - SearchPage, SchemaTimelinePage, SchemaPage,
  // BackupPage, LogsPage, InstanceInsightsPage, ObjectsPage, RedisKeyspacePage -
  // are lazy-loaded at their render sites for the same reason. Each is already
  // behind an `{#if …EverOpened}` guard, so a static import only ever meant
  // "ship this page's code to every user at boot whether or not they open it".
  // They are warmed during idle below, so opening one is still instant.
  // Monaco-backed pages (DataDiffPage, OrmRunner, SecurityPage, JsonViewerPage, SqlConsole)
  // are loaded lazily at their render sites so the Monaco editor stays out of the
  // startup bundle until the user actually opens a SQL / ORM / JSON / diff / security tab.
  // Heavy feature pages (echarts / mermaid / swapy / xyflow+dagre) are loaded lazily
  // the first time their tab is opened - see the {#await import()} blocks below.
  // This keeps those large libraries out of the startup bundle and idle memory.
  import { Button } from '$lib/components/ui/button/index.js'
  import AlertTriangle from '@lucide/svelte/icons/triangle-alert'
  import Lock from '@lucide/svelte/icons/lock'
  import WifiOff from '@lucide/svelte/icons/wifi-off'
  import Unplug from '@lucide/svelte/icons/unplug'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import {
    disconnectPostgres,
    prewarmDns,
    listSchemas,
    listTables,
    getTableRowCounts,
    getTableRows,
    countTableRows,
    liveStart,
    liveStop,
    getTableColumnStructure,
    getSchemaColumnStructure,
    getIncomingForeignKeys,
    executeSql,
    executeSqlMulti,
    executeSqlStream,
    resultSort,
    txBegin,
    txExecute,
    txStatus,
    txCommit,
    txRollback,
    executeDdl,
    updateTableCell,
    fetchCellValue,
    deleteTableRows,
    insertTableRow,
    toggleDevtools,
    mcpStart,
    mcpStop,
    mcpUpdateConnections,
    geoOverview,
  } from '$lib/api.js'
  import { StoredResultView } from '$lib/stored-result-view.js'
  import {
    createTableTab,
    createSqlTab,
    createDdlTab,
    createWelcomeTab,
    createSchemaTab,
    createOrmTab,
    createOrmSchemaTab,
    findOrmSchemaTab,
    createSecurityTab,
    createLogsTab,
    createInsightsTab,
    createAdvisorTab,
    createGolfTab,
    findInsightsTab,
    findAdvisorTab,
    findGolfTab,
    createObjectsTab,
    findObjectsTab,
    createRedisTab,
    findRedisTab,
    createExtensionsTab,
    findExtensionsTab,
    createMapTab,
    findMapTab,
    createExtensionDetailTab,
    findExtensionDetailTab,
    createJsonTab,
    createBackupTab,
    createChartsTab,
    createDashboardTab,
    createErdTab,
    findErdTab,
    createDiagramsTab,
    findDiagramsTab,
    createSearchTab,
    findSearchTab,
    createNotebookTab,
    createSchemaTimelineTab,
    findSchemaTimelineTab,
    createDataDiffTab,
    findDataDiffTab,
    createLicenseTab,
    findLicenseTab,
    findTableTab,
    findSqlTab,
    findSchemaTab,
    findOrmTab,
    findSecurityTab,
    findLogsTab,
    findBackupTab,
    findJsonTab,
    findChartsTab,
    findDashboardTab,
    findLastTableTab,
    tableTabTitle,
    cycleTabIndex,
    cloneTableTabState,
    cloneSqlTabState,
  } from '$lib/studio-tabs.js'
  import {
    pendingChangesCount,
    clearPendingChanges,
    anyPendingChanges,
  } from '$lib/stores/pending-table-edits.js'
  import { createNotebook, deserializeNotebook } from '$lib/notebook.js'
  import { openNotebookFile } from '$lib/api.js'
  import { formatCompactCount, normalizeTableRowCount } from '$lib/table-list.js'
  import { humanizeDbError } from '$lib/ai.js'
  import { focusTrap } from '$lib/actions/focus-trap.js'
  import {
    MAX_PAGE_SIZE,
    fetchLimitFor,
    PAGE_SIZE_ALL,
    DEFAULT_PAGE_SIZE,
    saveDefaultPageSize,
    loadDefaultPageSize,
    activeFilters,
    filtersApiSignature,
    filtersForApi,
    sortForApi,
    buildSelectSql,
    readRowsResponse,
  } from '$lib/table-query.js'
  import {
    WINDOW_PROBE,
    WINDOW_ROWS_DEFAULT,
    WINDOW_THRESHOLD,
    measureRowBytes,
    pickWindowRows,
    seekKeyFor,
    shouldWindow,
    stableWindowOrder,
    windowKeepCount,
    windowsFullyCovered,
  } from '$lib/row-window.js'
  import {
    buildForeignKeyFilters,
    buildReverseForeignKeyFilters,
    findForeignKeyForColumn,
    normalizeForeignKeys,
  } from '$lib/foreign-key-nav.js'
  import { loadLayout, saveLayout, sidebarSideStore, setSidebarSide } from '$lib/stores/layout.js'
  import {
    getLastConnection,
    getLastSchema,
    loadSavedConnections,
    removeConnection,
    setConnectionGroup,
    setLastConnectionId,
    setLastSchema,
    wasDisconnected,
    setWasDisconnected,
    upsertConnection,
    engineFamily,
  } from '$lib/stores/connections.js'
  import { hasPro, FREE_CONNECTION_LIMIT } from '$lib/stores/license.js'
  import { engineSupports } from '$lib/db-capabilities.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import KeyRound from '@lucide/svelte/icons/key-round'
  import {
    connectPostgres,
    connectSqlite,
    connectD1,
    connectLibSql,
    connectMysql,
    connectClickhouse,
    connectPosthog,
    connectDuckdb,
    connectMssql,
    connectRedis,
    listIndexes,
    listEnums,
    listFunctions,
    pingConnection,
    listTriggers,
    listSequences,
    truncateTable,
    dropTable,
    initSampleDb,
    getTableDdl,
  } from '$lib/api.js'
  import {
    remapNullableRowIndex,
    remapRowIndexSet,
  } from '$lib/table-row-indices.js'
  import { rowsToCsv, rowsToJson, rowsToCsvAsync, rowsToJsonAsync, rowsToSql, rowsToTsv, rowsToMarkdown, rowsToJsonl, saveExportFile, buildExportFilename } from '$lib/export.js'
  import {
    recordQueryExecution,
    listQueryHistory,
    listSavedQueries,
    createSavedQuery,
    saveQueryOnce,
  } from '$lib/stores/query-history.js'
  import { recordActivity } from '$lib/stores/activity-log.js'
  import { loadRecentTabs, pushRecentTab, removeRecentTab, clearRecentTabs } from '$lib/stores/recent-tabs.js'
  import { installInputShortcuts } from '$lib/input-shortcuts.js'
  import Kbd from './Kbd.svelte'
  import TitleBar from './TitleBar.svelte'
  import { savedCharts, updateChart, switchChartsConnection } from '$lib/stores/saved-charts.js'
  import { switchDiagramsConnection } from '$lib/stores/saved-diagrams.js'
  import { dashboards, activeDashboardId, switchDashboardsConnection } from '$lib/stores/dashboards.js'
  import { buildOption } from '$lib/chart-utils.js'
  import { isNetworkError, connectionErrorKind } from '$lib/utils.js'
  import { get } from 'svelte/store'
  import { virtualColumnsStore } from '$lib/stores/virtual-columns.js'

  /** @typedef {import('$lib/studio-tabs.js').StudioTab} StudioTab */
  /** @typedef {import('$lib/studio-tabs.js').TableTabState} TableTabState */
  /** @typedef {import('$lib/studio-tabs.js').SqlTabState} SqlTabState */
  /** @typedef {import('$lib/table-query.js').TableSort} TableSort */
  /** @typedef {import('$lib/table-query.js').TableFilter} TableFilter */
  /** @typedef {import('$lib/foreign-key-nav.js').ForeignKeyInfo} ForeignKeyInfo */

  const SEARCH_DEBOUNCE_MS = 150
  // Per-table column metadata is tiny (an array of column defs), so we can cache a
  // lot of it. A high cap means revisiting a table almost never re-fetches columns
  // on-demand - which is what caused a visible flicker on switch-back when the old
  // 60-entry limit evicted earlier tables in a large schema.
  const COLUMNS_CACHE_MAX = 400

  /** @param {Map<string, unknown>} map @param {string} key @param {unknown} value */
  function lruSet(map, key, value) {
    if (map.has(key)) map.delete(key)
    map.set(key, value)
    if (map.size > COLUMNS_CACHE_MAX) map.delete(/** @type {string} */ (map.keys().next().value))
  }

  const ONBOARDING_KEY = 'stroke:onboarded'
  const SAMPLE_SEEDED_KEY = 'stroke:sample-seeded'
  const SAMPLE_DB_ID = 'stroke:sample-sqlite'
  let showOnboarding = $state(false)

  // Dev-only: Alt+Shift+O resets and re-shows the onboarding. Dead code in prod.
  if (import.meta.env.DEV) {
    createHotkey('Alt+Shift+O', () => {
      try { localStorage.removeItem(ONBOARDING_KEY) } catch {}
      showOnboarding = true
    })
  }

  let connection = $state(null)
  /** @type {boolean} - true when the DB went away mid-session */
  let connectionLost = $state(false)
  let autoConnecting = $state(false)
  /** Name of the connection the overlay is waiting on. */
  let autoConnectName = $state('')
  /** Overlay verb: resuming the last session reads "Reconnecting", every other
   *  path (switching database, Docker, sample) is a fresh "Connecting". */
  let autoConnectVerb = $state('Connecting')

  /**
   * Raise the full-screen connect overlay. Always name the connection being
   * dialled - the name is what tells the user which database they are waiting
   * on, and a stale one during a switch says the wrong database entirely.
   * @param {string} name
   * @param {'Connecting' | 'Reconnecting'} [verb]
   */
  /** Longest a startup reconnect keeps the window hidden before showing the overlay. */
  const RECONNECT_REVEAL_MS = 700
  /** Set by the startup reconnect: reveal the page once the tables have loaded. */
  let revealOnTables = $state(false)
  $effect(() => {
    if (revealOnTables && connection && !loadingTables) revealApp()
  })

  function beginConnectOverlay(name, verb = 'Connecting') {
    autoConnectName = name ?? ''
    autoConnectVerb = verb
    autoConnecting = true
  }

  /**
   * Saved connections are often named `db@full.rds.host.name` - too long to read
   * in the connect overlay. Keep the database and the host's first label.
   * @param {string} name
   */
  function shortConnLabel(name) {
    const at = name.lastIndexOf('@')
    const label = at === -1 ? name : `${name.slice(0, at)}@${name.slice(at + 1).split('.')[0]}`
    return label.length > 44 ? `${label.slice(0, 43)}…` : label
  }
  /** Set by the overlay's Cancel so a late-landing connect doesn't yank the user back. */
  let autoConnectCancelled = false

  /** Give up on the startup reconnect and let the user choose a connection instead. */
  async function cancelAutoConnect() {
    autoConnectCancelled = true
    autoConnecting = false
    showConnectionModal = true
    try { await disconnectPostgres() } catch { /* nothing to tear down */ }
  }
  /** Assigned by ObjectsPage so ⌘F can reach its search box. */
  let objectsFocusSearch = $state(/** @type {() => void} */ (() => {}))
  let dbSearchFocusInput = $state(/** @type {() => void} */ (() => {}))
  let showConnectionModal = $state(false)
  /** Engine chosen on the welcome screen - the modal opens straight into its form. */
  let connectionModalEngine = $state('')
  /** @type {HTMLElement | null} */
  let welcomeConnectBtn = $state(null)

  // The welcome screen has exactly one thing to do, so its button starts with
  // focus: Enter opens the dialog, Tab walks the engine chips, and nobody has to
  // reach for the mouse to get in. Skipped while a dialog owns the window.
  $effect(() => {
    if (connection || showConnectionModal || !welcomeConnectBtn) return
    welcomeConnectBtn.focus({ preventScroll: true })
  })

  /**
   * Open a link in the user's browser. In a Tauri window a plain anchor
   * navigates the webview itself - away from the app - so every outbound link
   * goes through the opener plugin, with `window.open` as the dev-server path.
   * @param {string} url
   */
  async function openExternalUrl(url) {
    try {
      const { openUrl } = await import('@tauri-apps/plugin-opener')
      await openUrl(url)
    } catch {
      window.open(url, '_blank', 'noopener,noreferrer')
    }
  }
  let showDockerModal = $state(false)
  let dockerInitialDb = $state(/** @type {string | null} */ (null))
  /** Bottom query-log console visibility. */
  let queryLogOpen = $state(false)
  /** When set, the ERD tab is scoped to this table + its FK-connected neighbors. */
  let erdFocusTable = $state('')
  /** The mounted per-table ERD pane, so the tab bar's Export menu can drive its
   *  diagram exports (PNG / copy PNG / SVG / Mermaid). */
  let erdPane = $state(/** @type {any} */ (null))
  let chartPane = $state(/** @type {any} */ (null))
  let showCreateTableDialog = $state(false)
  let showCreateSchemaDialog = $state(false)
  let savedConnections = $state(loadSavedConnections())
  let showSettingsModal = $state(false)
  let showShortcutsModal = $state(false)
  let showInsiderModal = $state(false)
  let showAboutModal = $state(false)
  let showReportIssueDialog = $state(false)
  let showDisconnectDialog = $state(false)
  let showAiModelSettings = $state(false)
  let showProGate = $state(false)
  let commandOpen = $state(false)
  let commandPage = $state(/** @type {'root'|'docker'|'connections'|'tables'|'pages'} */ ('root'))
  // Reset to the root view whenever the palette closes so generic openers
  // (⌘K, the status-bar button) never reopen onto a stale sub-page.
  $effect(() => { if (!commandOpen) commandPage = 'root' })

  // ── DB-type capability flags ───────────────────────────────────────────────
  // Normalize wire-compatible aliases (mariadb → mysql, cockroachdb → postgres)
  // so every capability/dialect check below keeps working unchanged.
  const dbType = $derived(engineFamily(connection?.type))
  /** Key-value engine (Redis) - swaps the whole relational UI for the keyspace page. */
  const isRedis = $derived(dbType === 'redis')
  /** Schema Explorer is useful for postgres + mysql; sqlite/d1 have no meaningful schema pages. */
  const hasSchemaExplorer = $derived((dbType === 'postgres' || dbType === 'mysql') && !isRedis)
  /** Security (RLS, policies, roles) is PostgreSQL-only. */
  const hasSecurity = $derived(dbType === 'postgres' && !isRedis)
  /**
   * PostGIS present on this connection - gates the Map view, which has nothing
   * to show without it. Asked once per connection: it is a single indexed
   * catalog row, and the alternative (offering Map everywhere and dead-ending on
   * a non-spatial database) is worse than one cheap query.
   */
  let geoAvailable = $state(false)
  $effect(() => {
    const conn = connection
    const pg = dbType === 'postgres'
    untrack(() => (geoAvailable = false))
    if (!conn || !pg) return
    void geoOverview()
      .then((res) => untrack(() => (geoAvailable = Boolean(res?.available))))
      .catch(() => {})
  })
  /** @type {import('./UpdateDialog.svelte').default | null} */
  let updateDialog = $state(null)
  let statusBarHasUpdate = $state(false)
  let sidebarOpen = $state(loadLayout().navSidebarOpen)
  /** The nav sidebar was open when a visual page took the width; put it back on leaving. */
  let _sidebarBeforeErd = false
  /** Tab kinds that are a picture first: they get the sidebar's width. */
  const WIDE_TAB_KINDS = new Set(['erd', 'reltree', 'diagrams'])
  let sidebarEverOpened = $state(loadLayout().navSidebarOpen)
  /** Which switchable sidebar panel is showing: 'tables' | 'connections' | 'extensions'. */
  let navSidebarPanel = $state(loadLayout().navSidebarPanel ?? 'tables')
  /** @param {string} p */
  function setSidebarPanel(p) {
    navSidebarPanel = p
    if (!sidebarOpen) sidebarOpen = true
    saveLayout({ navSidebarPanel: p, navSidebarOpen: true })
  }
  /** Which side the navigation sidebar docks to. @type {'left' | 'right'} */
  // Read through the store, not from the layout blob: the side can now be set
  // from Settings → Appearance as well as the sidebar's own context menu, and
  // both need to land in one place the shell can react to.
  const sidebarSide = $derived($sidebarSideStore)
  /** @param {'left' | 'right'} s */
  function moveSidebar(s) { setSidebarSide(s) }
  let aiSidebarOpen = $state(loadLayout().aiSidebarOpen)
  let aiSidebarEverOpened = $state(loadLayout().aiSidebarOpen)
  let statusBarVisible = $state(loadLayout().statusBarVisible)
  let tabBarVisible = $state(loadLayout().tabBarVisible)
  let tableToolbarVisible = $state(loadLayout().tableToolbarVisible)

  // Publish the app-chrome heights as CSS vars on :root so PORTALED overlays
  // (the connection modal lives in document.body, outside this tree) can inset
  // themselves below the draggable titlebar and above the status bar - leaving
  // the window's drag region and status controls usable while the modal is open.
  $effect(() => {
    const root = document.documentElement
    root.style.setProperty('--app-titlebar-h', '38px') // TitleBar is always shown
    root.style.setProperty('--app-statusbar-h', statusBarVisible ? '32px' : '0px')
  })
  // Width for the loading-fallback shell, so the spinner fills a properly-sized
  // sidebar panel (matching saved width) instead of a zero-width strip while the
  // lazy AiSidebar chunk downloads.
  const aiSidebarFallbackWidth = loadLayout().aiSidebarWidth

  /** @type {StudioTab[]} */
  let tabs = $state([])
  // Recently-closed tabs (most-recent last), for Reopen Closed Tab (Ctrl/⌘+Shift+T).
  // Stores lightweight descriptors, not live tab objects; capped so it can't grow.
  let closedTabStack = $state(/** @type {any[]} */ ([]))
  const CLOSED_TAB_STACK_MAX = 20
  let activeTabId = $state(/** @type {string | null} */ (null))

  // ── Split-pane / editor-group layout ─────────────────────────────────────
  // `paneRoot` is a binary split tree whose leaves ("groups") reference tab ids
  // from the flat `tabs` array above. The FOCUSED group's active tab is kept in
  // sync with the global `activeTabId`, so the existing data pipeline (columns,
  // rows, loadRows, handleSaveCell, …) drives whatever the focused pane shows.
  // Background panes render read-only snapshots from each tab's own `tab.state`.
  /** @type {import('$lib/pane-layout.js').PaneNode | null} */
  let paneRoot = $state(null)
  let activeGroupId = $state(/** @type {string | null} */ (null))
  /** Tab id currently being dragged (drives the split drop targets). */
  let dragTabId = $state(/** @type {string | null} */ (null))
  /** Number of panes (groups) in the layout - drives the focused-pane accent. */
  const paneCount = $derived(paneRoot ? PaneTree.allGroups(paneRoot).length : 0)
  /** Floating drag preview position + label (follows the cursor). */
  let dragGhost = $state(/** @type {{ x: number, y: number, title: string } | null} */ (null))
  /** Current drop target under the cursor: which pane + which edge. */
  let dropTarget = $state(/** @type {{ groupId: string, edge: import('$lib/pane-layout.js').DropEdge } | null} */ (null))

  /** Which edge of a pane the cursor is over (drives the split direction / hint). */
  function edgeFromPoint(/** @type {number} */ x, /** @type {number} */ y, /** @type {DOMRect} */ r) {
    const fx = (x - r.left) / r.width
    const fy = (y - r.top) / r.height
    const E = 0.25
    if (fx < E) return /** @type {const} */ ('left')
    if (fx > 1 - E) return /** @type {const} */ ('right')
    if (fy < E) return /** @type {const} */ ('top')
    if (fy > 1 - E) return /** @type {const} */ ('bottom')
    return /** @type {const} */ ('center')
  }

  /** @param {string} id */
  function beginTabDrag(id) {
    dragTabId = id
    const t = tabs.find((tab) => tab.id === id)
    dragGhost = { x: 0, y: 0, title: t?.title ?? 'Tab' }
    dropTarget = null
  }
  /** @param {number} x @param {number} y */
  function moveTabDrag(x, y) {
    if (!dragTabId) return
    dragGhost = { x, y, title: dragGhost?.title ?? '' }
    const el = /** @type {Element | null} */ (document.elementFromPoint(x, y))
    const group = el?.closest('[data-pane-group]') ?? null
    if (!group) { dropTarget = null; return }
    const groupId = group.getAttribute('data-pane-group')
    if (!groupId) { dropTarget = null; return }
    dropTarget = { groupId, edge: edgeFromPoint(x, y, group.getBoundingClientRect()) }
  }
  function endTabDrag() {
    const target = dropTarget
    // handleDropZone reads dragTabId, so call it before clearing.
    if (target && dragTabId) handleDropZone(target.groupId, target.edge)
    dragTabId = null
    dragGhost = null
    dropTarget = null
  }

  /** @type {import('$lib/stores/recent-tabs.js').RecentTab[]} */
  let recentTabs = $state([])

  let schemas = $state([])
  let activeSchema = $state('public')
  // $state.raw: a large schema is thousands of table objects, and deep $state
  // proxies every one of them plus every field. Nothing mutates a table in
  // place - the list is always replaced wholesale (including the rowCount
  // backfill, which rebuilds it with .map) - so the proxies bought nothing and
  // cost a walk of the whole list on load plus a proxy hop on every read from
  // Sidebar's and CommandPalette's filter/map passes.
  let tables = $state.raw([])
  let indexes = $state([])
  /** @type {{ name: string, values: string[] }[]} */
  let enums = $state([])
  /** @type {{ name: string, tableName: string, timing: string, events: string, functionName: string, enabled: boolean }[]} */
  let triggers = $state([])
  /** @type {{ name: string, dataType: string, startValue: number, minValue: number, maxValue: number, increment: number, cycle: boolean, ownedBy: string|null }[]} */
  let sequences = $state([])
  /** @type {'data' | 'structure'} */
  let tableViewMode = $state('data')
  /** How the data view renders loaded rows - sticky per tab via snapshots. */
  /** @type {'table' | 'json' | 'record' | 'text' | 'chart' | 'erd'} */
  let dataViewMode = $state(/** @type {any} */ (loadSettings().defaultDataView))
  /** @type {import('$lib/api.js').ColumnStructureRow[] | null} - loaded on demand when switching to structure view */
  let structureColumns = $state(/** @type {any[]} */ ([]))
  let loadingStructure = $state(false)
  let _structureSeq = 0
  let structureSearch = $state('')
  let activeTable = $state(/** @type {string | null} */ (null))
  // Pre-filtered to the active table so the DataTable/StructureView props don't
  // recreate a filtered array on every StudioShell re-render (was a per-render
  // cost on each tab switch and keystroke).
  const activeTableIndexes = $derived(activeTable ? indexes.filter((i) => i.tableName === activeTable) : [])
  const activeTableTriggers = $derived(activeTable ? triggers.filter((t) => t.tableName === activeTable) : [])
  let tableFilter = $state('')
  let loadingTables = $state(false)

  // ── Live mode ─────────────────────────────────────────────────────────────
  // Auto-refresh the active table when its data changes. The backend polls a
  // cheap change signal (Postgres pg_stat counters / SQLite data_version). Off by
  // default; per-session, applies to the active table tab.
  let liveEnabled = $state(false)
  const liveSupported = $derived(connection?.type === 'postgres' || connection?.type === 'sqlite')
  // Stable identity of what should be watched - only changes when the target
  // table changes, so row refetches (which replace the tab object) don't churn it.
  const liveTableKey = $derived.by(() => {
    if (!liveEnabled || !liveSupported) return ''
    const t = activeTab
    if (t?.kind !== 'table') return ''
    const s = /** @type {TableTabState} */ (t.state)
    return s?.table ? JSON.stringify([s.schema, s.table]) : ''
  })
  let _liveRefetchTimer = /** @type {ReturnType<typeof setTimeout> | null} */ (null)
  // True while the backend watcher has been stopped *because the window is
  // hidden*. Doubles as the guard that we only ever resume a watcher we paused
  // (never start one that was off), so the visibility handler can't double-start.
  let _livePausedByHide = false

  // Start/stop the backend watcher whenever the watched table (or toggle) changes.
  $effect(() => {
    const key = liveTableKey
    // No liveStop() here: when the key clears, Svelte runs the prior run's
    // cleanup (which stops the watcher) *before* re-executing this effect, so
    // the returned cleanup is the single stop path - calling it again here
    // would be a redundant double-stop.
    if (!key) return
    const [schema, table] = JSON.parse(key)
    void liveStart(schema, table).catch((e) => {
      liveEnabled = false
      toast.error(`Live mode unavailable: ${String(e)}`)
    })
    // Stop the watcher on unmount/HMR (and before the effect re-runs for a new
    // key), so a backend watcher/trigger never leaks past this component.
    return () => { void liveStop().catch(() => {}) }
  })

  // Auto-disable when the connection can't support live mode (switched to MySQL,
  // disconnected, etc.) so the toggle never lingers in an impossible state.
  $effect(() => { if (!liveSupported && liveEnabled) liveEnabled = false })

  /** Refetch the active table (throttled) when the backend reports a change. */
  function onLiveChange(/** @type {{ schema: string, table: string } | undefined} */ payload) {
    if (!liveEnabled || !payload) return
    if (typeof document !== 'undefined' && document.hidden) return
    const t = activeTab
    if (t?.kind !== 'table') return
    const s = /** @type {TableTabState} */ (t.state)
    if (!s || s.schema !== payload.schema || s.table !== payload.table) return
    if (editingCell) return // never clobber an in-progress edit
    if (_liveRefetchTimer) return // coalesce bursts
    _liveRefetchTimer = setTimeout(() => {
      _liveRefetchTimer = null
      // loadRows() (not fetchRowsForTab) so the refresh honours the CURRENT
      // filters/sort/search/page held in global state; keepScroll avoids the
      // grid jumping to the top on every change.
      if (liveEnabled && activeTabId && !editingCell && activeTab?.kind === 'table') {
        void loadRows({ keepScroll: true })
      }
    }, 350)
  }

  onMount(() => {
    // Anonymous, opt-out, and a no-op when the setting is off. See telemetry.js
    // for what it does and does not send.
    startTelemetry()
    let unlisten = () => {}
    void (async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event')
        unlisten = await listen('live-change', (e) => onLiveChange(/** @type {any} */ (e.payload)))
      } catch { /* non-Tauri / web preview - no live-change events */ }
    })()
    return () => { unlisten(); if (_liveRefetchTimer) clearTimeout(_liveRefetchTimer) }
  })

  let aiMode = $state(loadAiMode())
  let aiEverOpened = $state(loadAiMode())
  $effect(() => { if (aiMode) aiEverOpened = true })
  $effect(() => { if (sidebarOpen) sidebarEverOpened = true })
  $effect(() => { if (aiSidebarOpen) aiSidebarEverOpened = true })

  // Keep Monaco-heavy tabs mounted once opened so the editor isn't destroyed on tab switch.
  let sqlEverOpened = $state(false)
  let ormEverOpened = $state(false)
  let securityEverOpened = $state(false)
  let logsEverOpened = $state(false)
  let insightsEverOpened = $state(false)
  let advisorEverOpened = $state(false)
  let golfEverOpened = $state(false)
  let objectsEverOpened = $state(false)
  let redisEverOpened = $state(false)
  let extensionsEverOpened = $state(false)
  let mapEverOpened = $state(false)
  let jsonEverOpened = $state(false)
  let backupEverOpened = $state(false)
  let chartsEverOpened = $state(false)
  let dashboardEverOpened = $state(false)
  let erdEverOpened = $state(false)
  let diagramsEverOpened = $state(false)
  let searchEverOpened = $state(false)
  let schemaTimelineEverOpened = $state(false)
  let dataDiffEverOpened = $state(false)
  let ormSchemaEverOpened = $state(false)
  /** @type {{ focusEditor: () => void, openQuery?: (content: string) => void } | null} */
  let sqlConsoleRef = $state(null)

  // ── Search options (match case / whole word / regex) ──────────────────────
  /** @type {import('$lib/search-options.js').SearchOptions} */
  let searchOptions = $state({ matchCase: false, wholeWord: false, regex: false })
  const searchOptsSupported = $derived(searchOptionsSupported(dbType))
  /** Per-option support for the current engine (gates individual toggles). */
  const searchOptsSupport = $derived(supportedSearchOptions(dbType))

  /** Translate a search term + the active options into API search params. */
  function apiSearch(/** @type {string} */ term) {
    return buildSearchQuery(term, searchOptions, dbType)
  }

  /** @param {import('$lib/search-options.js').SearchOptions} next */
  function handleSearchOptionsChange(next) {
    searchOptions = next
    if (rowSearch.trim()) {
      page = 1
      void loadRows()
    }
  }

  // ── Workflow extensions: saved views + find & replace ─────────────────────
  const savedViewsEnabled = $derived(pluginEnabledIn($pluginState, 'saved-views'))
  const findReplaceEnabled = $derived(pluginEnabledIn($pluginState, 'find-replace'))

  /** @type {import('$lib/stores/table-views.js').SavedTableView[]} */
  let savedTableViews = $state([])
  /** @type {string | null} */
  let activeTableViewId = $state(null)
  /**
   * Show find & replace. It is a sidebar panel now, not a dialog, so "open"
   * means: make sure the sidebar is visible, switch it to that panel, and put
   * the cursor in the Find field. `sidebarOpenFindReplace` is assigned by the
   * Sidebar, which owns the tab state.
   */
  let sidebarOpenFindReplace = $state(/** @type {() => void} */ (() => {}))
  /** Assigned by the Sidebar; puts its strip back on the Tables list. */
  let sidebarShowTables = $state(/** @type {() => void} */ (() => {}))
  function openFindReplacePanel() {
    if (!connection || !activeTable || columns.length === 0 || !findReplaceEnabled) return
    // `setSidebarPanel` also opens the sidebar and persists the choice, which
    // is what every other way of reaching a panel does.
    if (navSidebarPanel !== 'tables' || !sidebarOpen) setSidebarPanel('tables')
    void tick().then(() => sidebarOpenFindReplace())
  }

  $effect(() => {
    void persistConnectionId
    void activeSchema
    const t = activeTable
    savedTableViews = t ? loadTableViews(persistConnectionId, activeSchema, t) : []
    activeTableViewId = null
  })

  /** @param {string} name */
  function saveCurrentTableView(name) {
    if (!activeTable) return
    const view = {
      id: crypto.randomUUID(),
      name,
      search: rowSearch,
      filters: rowFilters.map((f) => ({ ...f })),
      sort: rowSort ? { ...rowSort } : null,
      sortMore: rowSortMore.map((s) => ({ ...s })),
      hiddenColumns: [...hiddenColumns],
      dataViewMode,
      searchOptions: { ...searchOptions },
    }
    savedTableViews = [...savedTableViews, view]
    saveTableViews(persistConnectionId, activeSchema, activeTable, savedTableViews)
    toast.success(`View "${name}" saved`)
  }

  /** @param {import('$lib/stores/table-views.js').SavedTableView} view */
  function applySavedView(view) {
    searchOptions = { matchCase: false, wholeWord: false, regex: false, .../** @type {any} */ (view).searchOptions }
    rowSearch = view.search ?? ''
    rowFilters = (view.filters ?? []).map((f) => ({ ...f }))
    rowSort = view.sort ? { ...view.sort } : null
    rowSortMore = (view.sortMore ?? []).map((s) => ({ ...s }))
    hiddenColumns = new Set(view.hiddenColumns ?? [])
    if (activeTable) saveHiddenCols(persistConnectionId, activeSchema, activeTable, hiddenColumns)
    dataViewMode = view.dataViewMode ?? /** @type {any} */ (loadSettings().defaultDataView)
    activeTableViewId = view.id
    page = 1
    void loadRows()
  }

  /** Toggle back to the unfiltered default (clears the applied view). */
  function resetTableView() {
    searchOptions = { matchCase: false, wholeWord: false, regex: false }
    rowSearch = ''
    rowFilters = []
    rowSort = null
    rowSortMore = []
    hiddenColumns = new Set()
    if (activeTable) saveHiddenCols(persistConnectionId, activeSchema, activeTable, hiddenColumns)
    dataViewMode = /** @type {any} */ (loadSettings().defaultDataView)
    activeTableViewId = null
    page = 1
    void loadRows()
  }

  /** @param {string} id */
  function deleteSavedView(id) {
    savedTableViews = savedTableViews.filter((v) => v.id !== id)
    if (activeTableViewId === id) activeTableViewId = null
    if (activeTable) saveTableViews(persistConnectionId, activeSchema, activeTable, savedTableViews)
  }

  /**
   * Apply find & replace edits. All edits target one column, so they collapse
   * into chunked single-statement CASE updates keyed by primary key - one or
   * two round-trips instead of one per row. ClickHouse (no standard UPDATE)
   * keeps the per-row pipeline.
   * @param {Array<{ rowIdx: number, colIdx: number, value: string }>} edits
   */
  async function handleFindReplaceApply(edits) {
    if (!activeTable || edits.length === 0) return
    if (dbType === 'clickhouse') {
      let done = 0
      try {
        for (const e of edits) {
          await handleSaveCell(e)
          done++
        }
        toast.success(`Replaced ${done.toLocaleString('en-US')} value${done === 1 ? '' : 's'} in ${activeTable}`)
      } catch (err) {
        toast.error(`Stopped after ${done} of ${edits.length} replacements`, { description: String(err) })
      }
      return
    }

    const CHUNK = 400
    let done = 0
    const _start = Date.now()
    try {
      for (let i = 0; i < edits.length; i += CHUNK) {
        const chunk = edits.slice(i, i + CHUNK)
        const sql = buildBatchUpdateSql({
          dialect: dbType,
          schema: activeSchema,
          table: activeTable,
          columns,
          primaryKey,
          rows,
          colIdx: chunk[0].colIdx,
          edits: chunk,
        })
        await executeSql(sql)
        done += chunk.length
      }
      recordActivity({ type: 'row_save', title: `Replaced ${done} values in ${activeTable}`, schema: activeSchema, table: activeTable, durationMs: Date.now() - _start, success: true })
      toast.success(`Replaced ${done.toLocaleString('en-US')} value${done === 1 ? '' : 's'} in ${activeTable}`)
    } catch (err) {
      recordActivity({ type: 'row_save', title: `Find & replace failed in ${activeTable}`, schema: activeSchema, table: activeTable, success: false, error: String(err) })
      toast.error(done ? `Stopped after ${done.toLocaleString('en-US')} of ${edits.length}` : 'Find & replace failed', { description: String(err) })
    }
    await loadRows({ keepScroll: true })
  }

  /**
   * Reset a table tab to its unfiltered default (tab context menu). Background
   * tabs are activated first so the reset goes through the live state and the
   * rows actually refetch.
   * @param {string} id
   */
  async function resetTableTab(id) {
    const tab = tabs.find((t) => t.id === id)
    if (!tab || tab.kind !== 'table') return
    if (id !== activeTabId) await activateTab(id)
    resetTableView()
  }

  // ── Sidebar table actions: console / generate SQL / count / copy columns ──
  /** @type {string | null} */
  let generateSqlTable = $state(null)
  let generateSqlOpen = $state(false)

  /** @param {string} tableName */
  function handleOpenTableInConsole(tableName) {
    const sql = genSelectStar({ dialect: dbType, schema: activeSchema, table: tableName, columns: [], primaryKey: [] })
    if (aiMode) exitAiMode()
    void openQueryInEditor(sql)
  }

  /** @param {string} tableName */
  function handleGenerateSql(tableName) {
    generateSqlTable = tableName
    generateSqlOpen = true
  }

  /** @param {string} tableName */
  async function handleCountRows(tableName) {
    try {
      let n = await countTableRows(activeSchema, tableName)
      if (n < 0) {
        // The count command is Postgres-only (other engines return -1) - fall
        // back to a portable COUNT(*) through the regular SQL path, which every
        // driver implements. Quoting/qualification follows the grid's rules.
        const tbl = qualifiedTable({ dialect: dbType, schema: activeSchema, table: tableName })
        const res = await executeSql(`SELECT COUNT(*) FROM ${tbl}`)
        n = Number(res?.rows?.[0]?.[0])
      }
      if (!Number.isFinite(n) || n < 0) throw new Error('Count not available on this connection')
      toast.success(tableName, { description: `${n.toLocaleString('en-US')} row${n === 1 ? '' : 's'}` })
    } catch (e) {
      toast.error('Count failed', { description: String(e) })
    }
  }

  /** @param {string} tableName */
  async function handleCopyColumns(tableName) {
    try {
      const cols = await getTableColumnStructure(activeSchema, tableName)
      await navigator.clipboard.writeText(cols.map((c) => c.name).join(', '))
      toast.success(`Copied ${cols.length} column name${cols.length === 1 ? '' : 's'}`)
    } catch (e) {
      toast.error('Copy columns failed', { description: String(e) })
    }
  }

  /** "Open in SQL editor" - generate a SELECT reflecting the current table view and open it in the SQL editor. */
  function openTableInSqlEditor() {
    if (!activeTable) return
    const sql = buildSelectSql({
      schema: activeSchema,
      table: activeTable,
      columns,
      filters: rowFilters,
      search: rowSearch,
      sort: rowSort,
      limit: pageSize,
      engine: connection?.type ?? 'postgres',
    })
    if (aiMode) exitAiMode()
    void (async () => {
      await focusSqlView()
      whenRefReady(() => sqlConsoleRef, (r) => r.openQuery?.(sql))
    })()
  }

  // Auto-focus the SQL editor whenever the SQL tab becomes active. Uses whenRefReady
  // so focus still lands on the first open while the lazy SqlConsole chunk mounts.
  $effect(() => {
    if (activeTab?.kind !== 'sql') return
    whenRefReady(() => sqlConsoleRef, (r) => { if (activeTab?.kind === 'sql') r.focusEditor?.() })
  })

  $effect(() => {
    if (activeTab?.kind === 'sql') sqlEverOpened = true
    if (activeTab?.kind === 'orm') ormEverOpened = true
    if (activeTab?.kind === 'security') securityEverOpened = true
    if (activeTab?.kind === 'logs') logsEverOpened = true
    if (activeTab?.kind === 'insights') insightsEverOpened = true
    if (activeTab?.kind === 'advisor') advisorEverOpened = true
    if (activeTab?.kind === 'golf') golfEverOpened = true
    if (activeTab?.kind === 'objects') objectsEverOpened = true
    if (activeTab?.kind === 'redis') redisEverOpened = true
    if (activeTab?.kind === 'extensions') extensionsEverOpened = true
    if (activeTab?.kind === 'map') mapEverOpened = true
    if (activeTab?.kind === 'json') jsonEverOpened = true
    if (activeTab?.kind === 'backup') backupEverOpened = true
    if (activeTab?.kind === 'charts') chartsEverOpened = true
    if (activeTab?.kind === 'dashboard') dashboardEverOpened = true
    if (activeTab?.kind === 'erd') erdEverOpened = true
    // The visual pages want the width: the nav sidebar folds while one of them
    // is the active tab and comes back when another kind is. Untracked so
    // Ctrl+B on the tab itself is not undone by this effect re-running.
    untrack(() => {
      if (activeTab && WIDE_TAB_KINDS.has(activeTab.kind)) {
        if (sidebarOpen) { _sidebarBeforeErd = true; sidebarOpen = false }
      } else if (_sidebarBeforeErd) {
        _sidebarBeforeErd = false
        sidebarOpen = true
      }
    })
    if (activeTab?.kind === 'diagrams') diagramsEverOpened = true
    if (activeTab?.kind === 'search') searchEverOpened = true
    if (activeTab?.kind === 'schema-timeline') schemaTimelineEverOpened = true
    if (activeTab?.kind === 'data-diff') dataDiffEverOpened = true
    if (activeTab?.kind === 'orm-schema') ormSchemaEverOpened = true
  })

  // ── Idle-based teardown of hidden heavy views ─────────────────────────────
  // Keeping every opened view mounted-but-hidden makes switching instant but
  // retains its Monaco/ECharts/canvas/observers forever, so memory grows over a
  // long session. To reclaim it, a teardown-eligible view that has been HIDDEN
  // (not the active pane) continuously past IDLE_TEARDOWN_MS gets its *EverOpened
  // flag reset to false, so the {#if} unmounts it and frees its resources.
  // Reopening re-sets the flag (via the activate effect above) and re-mounts.
  //
  // Only purely data-derived views are eligible - they hold no unpersisted user
  // input, so re-deriving on re-open is lossless. EXCLUDED (kept mounted): sql &
  // table (kept hot), and orm/json/data-diff/logs/search/redis/objects/extensions
  // which hold unpersisted user input, editor/undo state, or streaming buffers.
  const IDLE_TEARDOWN_MS = 3 * 60 * 1000 // 3 min hidden → unmount to reclaim memory
  /** Teardown-eligible views: tab kind ↔ its keep-alive flag (closures over $state). */
  const teardownViews = /** @type {const} */ ([
    { kind: 'security',        get: () => securityEverOpened,       set: (/** @type {boolean} */ v) => (securityEverOpened = v) },
    { kind: 'backup',          get: () => backupEverOpened,         set: (/** @type {boolean} */ v) => (backupEverOpened = v) },
    { kind: 'insights',        get: () => insightsEverOpened,       set: (/** @type {boolean} */ v) => (insightsEverOpened = v) },
    { kind: 'advisor',         get: () => advisorEverOpened,        set: (/** @type {boolean} */ v) => (advisorEverOpened = v) },
    { kind: 'charts',          get: () => chartsEverOpened,         set: (/** @type {boolean} */ v) => (chartsEverOpened = v) },
    { kind: 'dashboard',       get: () => dashboardEverOpened,      set: (/** @type {boolean} */ v) => (dashboardEverOpened = v) },
    { kind: 'erd',             get: () => erdEverOpened,            set: (/** @type {boolean} */ v) => (erdEverOpened = v) },
    { kind: 'diagrams',        get: () => diagramsEverOpened,       set: (/** @type {boolean} */ v) => (diagramsEverOpened = v) },
    { kind: 'schema-timeline', get: () => schemaTimelineEverOpened, set: (/** @type {boolean} */ v) => (schemaTimelineEverOpened = v) },
    { kind: 'orm-schema',      get: () => ormSchemaEverOpened,      set: (/** @type {boolean} */ v) => (ormSchemaEverOpened = v) },
  ])
  /** kind → Date.now() when it last became hidden; absent while active or unmounted. */
  let _hiddenSince = /** @type {Record<string, number>} */ ({})

  // Track when each eligible view enters/leaves the hidden state.
  $effect(() => {
    const active = activeTab?.kind
    const now = Date.now()
    for (const v of teardownViews) {
      if (v.kind === active) delete _hiddenSince[v.kind]        // active → clear idle clock
      else if (v.get() && _hiddenSince[v.kind] == null) _hiddenSince[v.kind] = now // just hidden → stamp
    }
  })

  // Single sweep (~60s) unmounts views hidden past the idle threshold. Runs even
  // when backgrounded - freeing memory while hidden is desirable. Cheap scan.
  $effect(() => {
    const id = setInterval(() => {
      const now = Date.now()
      const active = activeTab?.kind
      for (const v of teardownViews) {
        if (v.kind === active || !v.get()) continue            // never the active view; skip already-unmounted
        const since = _hiddenSince[v.kind]
        if (since != null && now - since >= IDLE_TEARDOWN_MS) {
          v.set(false)                                         // {#if} unmounts → disposes Monaco/ECharts/observers
          delete _hiddenSince[v.kind]
        }
      }
    }, 60 * 1000)
    return () => clearInterval(id)
  })

  let columns = $state([])
  /** @type {Set<string>} */
  let hiddenColumns = $state(new Set())
  /** @type {Map<string, typeof columns>} */
  let tableColumnsCache = $state(new Map())
  /** Bumped on every in-place write to tableColumnsCache: a Map in $state is
   *  not deeply reactive, so the SQL completion hints never saw new entries. */
  let tableColumnsVersion = $state(0)
  let primaryKey = $state([])
  /** Data-import dialog for the open table. */
  let importDataOpen = $state(false)

  // ── Explicit transactions ──────────────────────────────────────────────────
  // One transaction per SQL tab: it holds a connection, so it belongs to the
  // editor the user opened it from rather than to the app as a whole.
  /** @type {Map<string, string>} tab id -> backend session id */
  let sqlTxSessions = $state(new Map())
  /** @type {Map<string, import('$lib/api.js').TxStatus>} tab id -> its status */
  let sqlTxStatuses = $state(new Map())
  let sqlTxBusy = $state(false)

  const activeTxSession = $derived(activeTabId ? sqlTxSessions.get(activeTabId) ?? null : null)
  const activeTxStatus = $derived(activeTabId ? sqlTxStatuses.get(activeTabId) ?? null : null)

  /** @param {string} tabId @param {import('$lib/api.js').TxStatus|null} status */
  function setTxStatus(tabId, status) {
    const next = new Map(sqlTxStatuses)
    if (status) next.set(tabId, status)
    else next.delete(tabId)
    sqlTxStatuses = next
  }

  async function beginSqlTransaction() {
    if (!activeTabId || sqlTxBusy) return
    const session = `tx-${activeTabId}`
    sqlTxBusy = true
    try {
      const status = await txBegin(session)
      sqlTxSessions = new Map(sqlTxSessions).set(activeTabId, session)
      setTxStatus(activeTabId, status)
      toast.info('Transaction open — nothing is saved until you commit')
    } catch (e) {
      toast.error('Could not start a transaction', { description: String(e) })
    } finally {
      sqlTxBusy = false
    }
  }

  /** @param {'commit'|'rollback'} how */
  async function endSqlTransaction(how) {
    const tabId = activeTabId
    if (!tabId || sqlTxBusy) return
    const session = sqlTxSessions.get(tabId)
    if (!session) return
    sqlTxBusy = true
    try {
      const applied = sqlTxStatuses.get(tabId)?.rowsAffected ?? 0
      if (how === 'commit') {
        await txCommit(session)
        toast.success(applied > 0 ? `Committed — ${formatCompactCount(applied)} row(s) written` : 'Committed')
      } else {
        await txRollback(session)
        toast.info('Rolled back — the database is unchanged')
      }
      // Only forget the session once the backend has actually closed it;
      // dropping it on failure would strand the held connection with no way
      // left in the UI to reach it.
      const next = new Map(sqlTxSessions)
      next.delete(tabId)
      sqlTxSessions = next
      setTxStatus(tabId, null)
    } catch (e) {
      toast.error(how === 'commit' ? 'Could not commit' : 'Could not roll back', {
        description: String(e),
      })
    } finally {
      sqlTxBusy = false
    }
  }
  /** @type {ForeignKeyInfo[]} */
  let foreignKeys = $state([])
  /** Cache of incoming (reverse) FKs, keyed by "schema.table". Loaded once per table open. */
  let incomingFkCache = $state(/** @type {Map<string,any[]>} */ (new Map()))
  /** Incoming FKs for the active table, read from cache. */
  const incomingForeignKeys = $derived(
    (activeSchema && activeTable) ? (incomingFkCache.get(`${activeSchema}.${activeTable}`) ?? []) : []
  )
  /** Virtual rel column labels for the toolbar columns dropdown.
   *  Must enumerate ALL incoming FK tables - no cap - so every rel column
   *  the DataTable can ever render is always reachable in the show/hide panel.
   *  (DataTable caps visible columns at MAX_VIRTUAL_COLS=5 but fills that
   *   quota from whatever isn't hidden, so capping here makes it impossible
   *   to hide the "overflow" tables that get promoted into view.) */
  const virtualRelColumnsForToolbar = $derived.by(() => {
    const seen = new Set()
    const result = []
    for (const fk of incomingForeignKeys) {
      const label =
        (fk.fromSchema && fk.fromSchema !== activeSchema)
          ? `${fk.fromSchema}.${fk.fromTable}`
          : fk.fromTable
      if (seen.has(label)) continue
      seen.add(label)
      result.push({ label })
    }
    return result
  })
  
  // $state.raw (not deep-proxied): the browse grid can hold millions of rows and
  // the canvas draw() reads rows[idx][col] for every visible cell every frame, so
  // proxy traps would sit directly on the scroll hot path. In-place window
  // load/evict (fetchWindow / evictFarWindows) and single-cell save already repaint
  // via dataVersion++ / the editingCell repaint effect, not via proxy reactivity.
  let rows = $state.raw([])
  let savingCell = $state(false)
  let deletingRows = $state(false)
  let insertingRow = $state(false)
  let tableReadonly = $state(false)
  // Mirror it into the store the api layer gates on, so every surface (sidebar
  // context menu, structure editor, AI tool calls, backup restore) is covered
  // rather than only the components this shell hands a `readonly` prop to.
  $effect(() => setReadOnly(tableReadonly))
  /** Bound from DataTable - triggers the inline new-row draft. */
  let dtBeginInsertRow = $state(/** @type {() => void} */ (() => {}))
  let showMcpPanel = $state(false)
  let mcpRunning = $state(false)
  /** @type {{ rowIdx: number, colIdx: number, draft: string } | null} */
  let editingCell = $state(null)
  // ── Staged (unsaved) cell edits - surfaced as Apply/Reset in the StatusBar ──
  let pendingEditCount = $state(0)
  /** @type {() => void | Promise<void>} */
  let applyEdits = $state(() => {})
  /** @type {() => void} */
  let resetEdits = $state(() => {})
  /** Copy the grid's staged changes as SQL (bound from DataTable). */
  let copyEditsSql = $state(() => {})
  /** Bound from DataTable - stages the selected rows for deletion (red diff). */
  let stageDeleteSelectedRows = $state(() => {})

  /** Table key ("schema.table") for a tab, or '' for non-table tabs. */
  function tabTableKey(/** @type {any} */ tab) {
    return tab?.kind === 'table' && tab.state?.table
      ? `${tab.state.schema}.${tab.state.table}`
      : ''
  }

  /** Count of unsaved changes staged in a given tab (live for the active one, cached otherwise). */
  function tabPendingCount(/** @type {any} */ tab) {
    if (!tab) return 0
    if (tab.id === activeTabId) return pendingEditCount
    const key = tabTableKey(tab)
    return key ? pendingChangesCount(key) : 0
  }

  /** True when any table (active or backgrounded) has unsaved staged changes. */
  function hasAnyUnsavedChanges() {
    return pendingEditCount > 0 || anyPendingChanges()
  }

  // In-app confirm dialog. `window.confirm` is blocked in the Tauri webview
  // ("dialog.confirm not allowed"), so route all confirmations through this.
  /** @type {{ message: string, confirmLabel: string, resolve: (v: boolean) => void } | null} */
  let confirmDialog = $state(null)
  /** @param {string} message @param {string} [confirmLabel] @returns {Promise<boolean>} */
  function askConfirm(message, confirmLabel = 'Discard') {
    return new Promise((resolve) => { confirmDialog = { message, confirmLabel, resolve } })
  }
  /** @param {boolean} v */
  function resolveConfirm(v) {
    const d = confirmDialog
    confirmDialog = null
    d?.resolve(v)
  }

  // Confirm before quitting the app while table edits are still unsaved.
  // Installed plugins are read once, at startup: scanning a folder and starting a
  // Worker per enabled plugin has no business happening on a repaint. The panel
  // rescans on demand.
  onMount(() => {
    void refreshExternalPlugins()
    return () => stopAllExternalPlugins()
  })

  onMount(() => {
    let unlisten = () => {}
    ;(async () => {
      try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window')
        const win = getCurrentWindow()
        unlisten = await win.onCloseRequested(async (event) => {
          if (!hasAnyUnsavedChanges()) return
          // Stop the close, ask in-app, then force-close if confirmed. Use destroy()
          // (not close()) so we don't re-enter this handler.
          event.preventDefault()
          const ok = await askConfirm('You have unsaved table changes. Discard them and quit?', 'Discard & quit')
          if (ok) await win.destroy()
        })
      } catch { /* non-Tauri / web preview - no window close event */ }
    })()
    return () => unlisten()
  })
  // ── Table scroll controls (StatusBar go-to-top / go-to-bottom / left / right) ──
  /** @type {() => void} */
  let scrollTableTop = $state(() => {})
  /** @type {() => void} */
  let scrollTableBottom = $state(() => {})
  /** @type {() => void} */
  let scrollTableLeft = $state(() => {})
  /** @type {() => void} */
  let scrollTableRight = $state(() => {})
  /** True when the active grid overflows horizontally (from DataTable). */
  let tableCanScrollH = $state(false)
  /** @type {(name: string) => void} */
  let focusTableColumn = $state(() => {})
  // Read/restore the data grid's scroll so horizontal (+ vertical) position is
  // preserved across tab switches.
  /** @type {() => { left: number, top: number }} */
  let tableGetScroll = $state(() => ({ left: 0, top: 0 }))
  /** @type {() => number[]} - live grid's open row-expand panels, persisted per tab */
  let tableGetExpanded = $state(() => /** @type {number[]} */ ([]))
  /** The active tab was opened by following a foreign key (see openTableTab). */
  let expandSingleRow = $state(false)
  /** @type {(pos: { left?: number, top?: number }) => void} */
  let tableApplyScroll = $state(() => {})
  /** Put the grid cursor on a cell and scroll it into view (back/forward restore).
   *  @type {(row: number, col?: number | null) => void} */
  let tableFocusCell = $state(() => {})
  /** @type {{ refresh: () => void } | null} */
  let securityPageRef = $state(null)
  let dataDiffPageRef = $state(null)
  /** @type {{ sendMessage: (text: string) => void } | null} */
  let aiSidebarRef = $state(null)

  /**
   * Run `fn` once a lazily-mounted component ref becomes available. These panels
   * (SQL console, AI sidebar, …) are now loaded via dynamic import, so their
   * `bind:this` ref is null for the first few frames after opening - a single
   * tick/microtask isn't enough. Polls a bounded number of animation frames so
   * the action still fires on first open without ever spinning forever.
   * @template T @param {() => T | null} getRef @param {(ref: T) => void} fn @param {number} [frames]
   */
  function whenRefReady(getRef, fn, frames = 90) {
    const ref = getRef()
    if (ref) { fn(ref); return }
    if (frames <= 0) return
    requestAnimationFrame(() => whenRefReady(getRef, fn, frames - 1))
  }

  let total = $state(0)
  let queryMs = $state(0)
  /** Columns the last page fetched as a preview instead of a value, with the
   *  average size that earned it. Empty on every ordinary table. */
  let previewColumns = $state(/** @type {{ name: string, avgBytes: number }[]} */ ([]))
  let loadingRows = $state(false)
  let loadingMore = $state(false)
  let page = $state(1)
  let pageSize = $state(loadDefaultPageSize())
  let rawOffset = $state(/** @type {number | null} */ (null))
  // When "All rows" sentinel is active, always fetch from offset 0.
  const currentOffset = $derived(
    pageSize === PAGE_SIZE_ALL ? 0 : (rawOffset ?? (page - 1) * pageSize),
  )

  // ── Keyset / cursor / temporal pagination ────────────────────────────────
  // Cursor for the NEXT fetch (null = first page / offset-0). Set by next/prev.
  let _keysetCursor = $state(/** @type {{ value: unknown, after: boolean } | null} */ (null))
  // Boundary key values of the currently-shown page (for building next/prev cursors).
  let _pageFirstKey = null
  let _pageLastKey = null
  /** Column keyset orders by: single-column PK (cursor/keyset), or a timestamp (temporal). */
  const _keysetKeyCol = $derived.by(() => {
    const mode = $appPaginationMode
    if (mode === 'temporal') {
      const ts = columns.find((c) => /(_at$|created|updated|inserted|timestamp|date)/i.test(c.name) && /(time|date)/i.test(c.dataType ?? c.data_type ?? ''))
        ?? columns.find((c) => /(time|date)/i.test(c.dataType ?? c.data_type ?? ''))
      return ts?.name ?? null
    }
    if (mode === 'cursor' || mode === 'keyset') return primaryKey.length === 1 ? primaryKey[0] : null
    return null
  })
  const _keysetKeyType = $derived.by(() => {
    const col = _keysetKeyCol
    if (!col) return null
    const c = columns.find((x) => x.name === col)
    return c?.dataType ?? c?.data_type ?? null
  })
  /** Temporal is newest-first; keyset/cursor honour the sort dir on the key (default asc). */
  const _keysetDesc = $derived($appPaginationMode === 'temporal' ? true : (rowSort?.column === _keysetKeyCol && rowSort?.direction === 'desc'))
  /** Whether keyset can drive this table right now (else offset - the safe fallback). */
  const _keysetActive = $derived.by(() => {
    if ($appPaginationMode === 'offset') return false
    if (!_keysetKeyCol || !_keysetKeyType) return false
    if (pageSize === PAGE_SIZE_ALL) return false
    // A page big enough to be windowed can't be keyset-driven: the windows are
    // fetched by offset, so a keyset-ordered first fetch and offset-ordered
    // windows would be two different orderings spliced into one view. Keyset
    // exists to make *small* deep pages cheap, which a 1M-row page isn't.
    if (Number.isFinite(pageSize) && pageSize > WINDOW_THRESHOLD) return false
    if ((connection?.type ?? '') !== 'postgres') return false // keyset is Postgres-only
    if (rowSortMore.length > 0) return false                   // multi-sort → offset
    if (rowSort && rowSort.column !== _keysetKeyCol) return false // sorted by a non-key col → offset
    return true
  })
  const _keyColIndex = $derived(_keysetKeyCol ? columns.findIndex((c) => c.name === _keysetKeyCol) : -1)
  // Bumped whenever a fresh page of rows is applied (page/filter/sort/search).
  // The DataTable watches it to jump its scroll + virtual window back to the
  // top, so a reload never leaves the view stranded mid-table.
  let reloadToken = $state(0)
  // Monotonic id so an out-of-order / superseded row fetch can't clobber a
  // newer one when the user pages rapidly. Kept PER TAB: a single global token
  // meant starting a load in one tab silently invalidated a load still running
  // in another, so a big table left fetching in the background was abandoned the
  // moment you touched a second tab.
  /** @type {Map<string, number>} */
  const _loadSeqByTab = new Map()
  /** @param {string | null} tabId */
  const loadSeqOf = (tabId) => (tabId ? _loadSeqByTab.get(tabId) ?? 0 : 0)
  /** @param {string | null} tabId */
  function bumpLoadSeq(tabId) {
    if (!tabId) return 0
    const n = loadSeqOf(tabId) + 1
    _loadSeqByTab.set(tabId, n)
    return n
  }
  // Infinite scroll - accumulated rows across all "load more" fetches. Plain
  // (non-reactive) array: handing a deep proxy to the grid would put get-traps
  // on every rows[r][c] read in the per-frame canvas draw (the scroll-lag
  // regression). handleLoadMore appends in place (O(1) amortised) and then
  // assigns `rows = _infiniteRows.slice()` - the fresh identity is what notifies
  // the grid's rows.length deriveds, and a shallow page-sized-growth copy is far
  // cheaper than proxy traps on the draw hot path.
  let _infiniteRows = /** @type {any[]} */ ([])
  // Hard ceiling on accumulated infinite-scroll rows. Without it, scrolling a
  // huge table in infinite mode would keep appending until the entire result set
  // was resident (and deep-proxied) - unbounded memory. At the cap we stop
  // auto-loading; the count/"load more" UI reflects that the view is capped.
  const INFINITE_ROW_CAP = 200_000
  let infiniteScroll = $state(loadInfiniteScroll())

  // ── Windowed loading (huge result sets) ──────────────────────────────────
  // For results past WINDOW_THRESHOLD we never hold every row: `rows` becomes a
  // sparse array of length = total with only the windows near the viewport
  // loaded, so 5M rows cost ~tens of MB instead of ~GBs. Absolute indexing is
  // preserved (rows[i] is still row i), so selection / hit-testing / scroll are
  // untouched - only the *data* is windowed. dataVersion bumps trigger a grid
  // redraw after a window is spliced in (rows identity is unchanged).
  // Window size is measured, not fixed - see $lib/row-window.js for why and how.
  const WINDOW_MAX_INFLIGHT = 3     // concurrent window requests (rest queue)
  const WINDOW_PREFETCH = 4         // windows fetched ahead in the scroll direction
  let windowed = $state(false)
  let dataVersion = $state(0)
  let _windowSeq = 0
  /** Rows per window for the current load (measured - see pickWindowRows). */
  let _windowRows = WINDOW_ROWS_DEFAULT
  /** Absolute row offset this windowed view starts at (page offset; 0 for "All"). */
  let _windowBase = 0
  /** How many rows this windowed view covers (the page, or the whole table). */
  let _windowCount = 0
  /** Measured payload per row, for sizing the export chunk too. */
  let _windowBytesPerRow = 0
  /** The one total order every window of this view slices - see stableWindowOrder.
   *  @type {{ sortColumn: string, sortDirection: string, sorts: Array<{column:string,direction:string}> } | null} */
  let _windowOrder = null
  /** @type {Set<number>} */
  let _windowLoaded = new Set()
  /** @type {Set<number>} */
  let _windowFetching = new Set()
  /** Windows wanted but not yet started, nearest-to-viewport first. @type {number[]} */
  let _windowQueue = []
  let _windowInFlight = 0
  /** Last visible window + travel direction, so prefetch runs ahead of the scroll. */
  let _lastFirstW = 0
  let _lastDir = 1
  // Set when a window fetch still comes back slow despite the sizing above (a slow
  // link, a sort the server can't index). Prefetch depth and concurrency drop to 1:
  // reading ahead then queues more work than the scroll can consume.
  let _windowSlow = false
  const WINDOW_SLOW_MS = 400
  /** Rolling mean window latency (EWMA), which is what _windowSlow now reads.
   *  A single measurement latched it: one deep page on a big table (a 350ms
   *  OFFSET scan, not a slow link) turned prefetch down to one window for the
   *  rest of the view - on exactly the tables that need read-ahead most. */
  let _windowMs = 0
  /** Failed windows → attempts so far, and the ones that have given up.
   *  A failed fetch used to be retried only by the next visible-range emit, so
   *  stopping the scroll left those rows shimmering forever. @type {Map<number, number>} */
  let _windowAttempts = new Map()
  /** @type {Set<number>} */
  let _windowFailed = new Set()
  const WINDOW_RETRIES = 3
  /** Set when a keyset window fetch errors - the view then stays on OFFSET. */
  let _windowSeekOff = false
  /** What the window fetcher is doing, for the grid's loading pill. Plain state
   *  (not the hot-path sets, which stay non-reactive): assigned only when the
   *  answer actually changes. */
  let windowStatus = $state({ slow: false, failed: false })
let rowSearch = $state('')
  let rowSort = $state(/** @type {TableSort | null} */ (null))
  /** Secondary sort keys for multi-column sort (primary is rowSort). @type {TableSort[]} */
  let rowSortMore = $state(/** @type {TableSort[]} */ ([]))
  let rowFilters = $state(/** @type {TableFilter[]} */ ([]))
  let filterBarOpen = $state(false)
  let vcolPanelOpen = $state(false)

  const _vcolTableKey = $derived(`${activeSchema}.${activeTable ?? ''}`)
  const vcolCount = $derived(
    ($virtualColumnsStore[_vcolTableKey] ?? []).filter(c => c.enabled).length
  )
  const virtualExprColsForToolbar = $derived($virtualColumnsStore[_vcolTableKey] ?? [])
  /** @type {{ focusRowSearch?: () => void, clearRowSearch?: () => void } | null} */
  let tableToolbar = $state(null)
  /** @type {{ focusedColumnName: () => string, openCellDock: (r: number, c: number) => void } | null} */
  let dataTable = $state(null)
  /** @type {ReturnType<typeof setTimeout> | null} */
  let filterDebounceTimer = null
  /** @type {ReturnType<typeof setTimeout> | null} */
  let searchDebounceTimer = null
  onDestroy(() => {
    if (filterDebounceTimer) clearTimeout(filterDebounceTimer)
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer)
  })


  /** Tracks which tab IDs have an in-flight background fetch (re-entry guard). */
  const fetchingTabIds = new Set()

  // ── Per-tab auto-refresh ──────────────────────────────────────────────────
  // An interval per tab, not one global setting: the table you're watching should
  // re-poll while the one you're editing stays still. Only the tab on screen
  // actually polls - a background tab re-fetches when you return to it anyway, so
  // ticking there would spend queries on nothing.
  /** @type {Map<string, number>} */
  const _autoRefreshByTab = new Map()
  let _autoRefreshTick = $state(0)
  const activeAutoRefreshMs = $derived.by(() => {
    void _autoRefreshTick
    return activeTabId ? _autoRefreshByTab.get(activeTabId) ?? 0 : 0
  })
  /** @param {number} ms */
  function setActiveAutoRefresh(ms) {
    if (!activeTabId) return
    if (ms > 0) _autoRefreshByTab.set(activeTabId, ms)
    else _autoRefreshByTab.delete(activeTabId)
    _autoRefreshTick += 1
  }
  // The timer is torn down and rebuilt whenever the interval or the active tab
  // changes, so exactly one is ever running and it always belongs to what's on
  // screen. Effects run after init, so the deriveds read here are live by then.
  $effect(() => {
    const ms = activeAutoRefreshMs
    const tabId = activeTabId
    if (!ms || !tabId) return
    if (activeTab?.kind !== 'table') return
    const timer = setInterval(() => {
      // Never stack a refresh on a load still running, and never pull rows out
      // from under an open cell editor or an in-flight save.
      if (!activeTable || isTabBusy(tabId) || editingCell || savingCell) return
      // keepScroll: this is a background update, not navigation - it must not jump
      // the grid to the top or close the row inspector.
      void loadRows({ keepScroll: true })
    }, ms)
    return () => clearInterval(timer)
  })
  /** Cancel handle of the run in flight per tab (`Stop` targets just that query). @type {Map<string, string>} */
  const _sqlQueryIdByTab = new Map()
  let _sqlRunSeq = 0
  /** Cancel handle for the tab on screen, handed to SqlConsole's Stop button. */
  const activeSqlQueryId = $derived.by(() => {
    void _busyTick
    return activeTabId ? _sqlQueryIdByTab.get(activeTabId) ?? null : null
  })
  // ── Per-tab busy accounting ───────────────────────────────────────────────
  // A tab is busy while it owns an UNSETTLED PROMISE - not while a hand-written
  // flag or counter says so. Both of those went wrong here: a flag let an older
  // load clear a mark its successor still needed, and guarding the clear to fix
  // that let a mark outlive its work, leaving the tab strip spinning forever
  // after the rows had landed. A promise settles exactly once, and the runtime
  // runs its continuation on every path - early return, throw, supersede - so
  // the mark cannot leak or clear early.
  /** @type {Map<string, Set<Promise<any>>>} */
  const _busyJobs = new Map()
  // Bumped on every change so consumers (activeSqlQueryId) recompute without the
  // map itself having to be a reactive proxy. The tab-strip spinner deliberately
  // does NOT read this: it draws from each tab's own loadingRows/sqlLoading, which
  // is written in the same patch as the rows and so can't disagree with them.
  let _busyTick = $state(0)
  /**
   * Mark `tabId` busy until `promise` settles. Returns the SAME promise, so
   * callers keep their own error handling; the bookkeeping hangs off a separate
   * continuation whose rejection is already handled here.
   * @template T @param {string | null} tabId @param {Promise<T>} promise @returns {Promise<T>}
   */
  function trackBusy(tabId, promise) {
    if (!tabId) return promise
    let jobs = _busyJobs.get(tabId)
    if (!jobs) { jobs = new Set(); _busyJobs.set(tabId, jobs) }
    jobs.add(promise)
    _busyTick += 1
    const settled = () => {
      jobs.delete(promise)
      if (jobs.size === 0) _busyJobs.delete(tabId)
      _busyTick += 1
    }
    promise.then(settled, settled)
    return promise
  }
  /** Forget a tab's work entirely - the tab itself is gone. @param {string | null} tabId */
  function clearBusy(tabId) {
    if (tabId && _busyJobs.delete(tabId)) _busyTick += 1
  }
  /**
   * Whether `tabId` really has work in flight *right now*. Snapshots store the
   * loading flag so a tab you leave mid-query still reads as running, but a
   * snapshot can outlive its query (duplicated tab, reconnect, restored session)
   * - restoring it blindly would strand a spinner nothing ever clears.
   * @param {string | null} tabId
   */
  function isTabBusy(tabId) {
    return !!tabId && (_busyJobs.get(tabId)?.size ?? 0) > 0
  }
  let error = $state('')
  /** Whether the error banner is showing the raw driver text. */
  let showRawError = $state(false)
  // A new failure starts collapsed; leaving it expanded from a previous
  // error would dump raw driver text on someone who never asked for it.
  $effect(() => { void error; showRawError = false })
  let selected = $state(new Set())
  /** @type {number | null} */
  let focusedRow = $state(null)
  /** Visible-column index of the focused cell. Held here (not just inside the
   *  grid) so the full cursor round-trips through a tab snapshot and through the
   *  back/forward history. @type {number | null} */
  let focusedCol = $state(null)
  /** @type {number | null} */
  let inspectorRow = $state(null)

  let sqlText = $state('SELECT 1;')
  let sqlColumns = $state([])
  let sqlRows = $state.raw([]) // raw: always assigned wholesale, read per-cell in draw()
  let sqlQueryMs = $state(0)
  let sqlMessage = $state('')
  let sqlLoading = $state(false)
  let sqlError = $state('')
  /** @type {any[]} */
  let sqlMultiResults = $state([])
  /** The active SQL tab's result lives in the backend's result store and
   *  `sqlRows` is its sparse view (Settings → Database → Stream query results). */
  let sqlWindowed = $state(false)
  /** Bumped when stored rows land in `sqlRows` in place. */
  let sqlDataVersion = $state(0)
  /** @type {{ slow: boolean, failed: boolean } | null} */
  let sqlWindowStatus = $state(null)
  let sqlSorting = $state(false)
  /** Each statement of the active tab's last run, for the editor's ✓/✗ marks.
   *  @type {Array<{ sql: string, error: string | null, position: number | null }>} */
  let sqlRunOutcomes = $state([])
  // SQL results by tab, kept out of the reactive `tabs` tree. Stored inside it,
  // a result came back from a tab switch as a deep proxy the grid then read per
  // cell (the scroll-lag regression `_liveRowsByTab` fixes for table tabs), and
  // a 5M-row array in there made every switch and close crawl.
  /** @type {Map<string, any[]>} */
  const _sqlRowsByTab = new Map()
  /** @type {Map<string, StoredResultView>} */
  const _sqlViewsByTab = new Map()

  let ormCode = $state('')
  let ormMode = $state(/** @type {'drizzle' | 'prisma'} */ ('drizzle'))
  let ormColumns = $state([])
  let ormRows = $state.raw([]) // raw: always assigned wholesale, read per-cell in draw()
  let ormQueryMs = $state(0)
  let ormLoading = $state(false)
  let ormError = $state('')

  const activeTab = $derived(tabs.find((t) => t.id === activeTabId) ?? null)
  /** Fast id → tab lookup for per-group tab-strip rendering. */
  const tabsById = $derived(new Map(tabs.map((t) => [t.id, t])))
  /** 'table' | 'view' | 'materialized_view' | 'foreign_table' - for the active table tab */
  const activeTableKind = $derived(
    activeTab?.kind === 'table'
      ? (/** @type {any} */ (activeTab.state))?.tableKind ?? 'table'
      : 'table'
  )
  /** Structure view only makes sense for real tables, not views/materialized views */
  const canShowStructure = $derived(activeTableKind === 'table' || activeTableKind === 'foreign_table')

  const activeView = $derived(activeTab?.kind === 'sql' ? 'sql' : 'table')

  // Stable name arrays derived separately so sqlSchemaHints doesn't rebuild
  // on every row fetch - only rebuilds when the column set actually changes.
  const _activeColsTyped = $derived(columns.map((c) => ({ name: c.name, type: c.dataType ?? c.data_type ?? '' })))
  const _sqlColNames = $derived(sqlColumns.map((c) => c.name))
  const _tableNames = $derived(tables.map((t) => t.name))

  // ── Async SQL-completion hints (enums + user functions) ──────────────────────
  // Loaded lazily when the SQL tab becomes active. Stored as reactive state so
  // sqlSchemaHints (a $derived) picks them up automatically once they arrive.
  let _sqlEnumValues = $state(/** @type {Record<string, string[]>} */ ({}))
  let _sqlUserFunctions = $state(/** @type {Array<{name:string,signature:string,returnType:string,kind:string}>} */ ([]))
  // Which connection+schema the hints reflect - enums/functions only change with
  // those, so don't re-run the catalog round-trips on every switch into SQL.
  let _sqlHintsLoadedFor = ''

  $effect(() => {
    if (activeView !== 'sql' || !connection || !activeSchema) return
    const schema = activeSchema
    const key = `${persistConnectionId}:${schema}`
    // Columns first, before the early return: they also go stale when the
    // table list changes, which loadSchemaColumns tracks on its own.
    void loadSchemaColumns()
    if (key === _sqlHintsLoadedFor) return
    _sqlHintsLoadedFor = key
    // Enum/function completion hints are PostgreSQL-only - skip the round-trips
    // (which would just return empty) on every other engine.
    if (engineSupports('enums', connection?.type)) {
      listEnums(schema).then((enums) => {
        /** @type {Record<string, string[]>} */
        const ev = {}
        for (const e of enums) ev[e.name] = e.values
        _sqlEnumValues = ev
      }).catch(() => {})
    } else {
      _sqlEnumValues = {}
    }
    if (engineSupports('functions', connection?.type)) {
      listFunctions(schema).then((fns) => {
        _sqlUserFunctions = fns
      }).catch(() => {})
    } else {
      _sqlUserFunctions = []
    }
  })

  // ── Connection health monitor ─────────────────────────────────────────────────
  // Pings the active DB periodically. On failure it flags the connection as lost
  // (a subtle red dot in the StatusBar) and silently reconnects in place - NO
  // popup. Pings every 30s when healthy; every 6s while lost so recovery is
  // detected fast. Clears itself the moment the connection is back.
  $effect(() => {
    if (!connection) {
      connectionLost = false
      return
    }
    let stopped = false
    /** @type {ReturnType<typeof setTimeout> | undefined} */
    let timer
    const schedule = () => {
      if (stopped) return
      timer = setTimeout(run, connectionLost ? 6_000 : 30_000)
    }
    const run = async () => {
      if (stopped) return
      // Skip the round-trip while the window is hidden (backgrounded/minimized):
      // there's no UI to update, and idle pings only risk waking a sleeping
      // remote connection. The next tick after the window is shown will ping.
      if (typeof document !== 'undefined' && document.hidden) { schedule(); return }
      try {
        await pingConnection()
        if (connectionLost) { connectionLost = false; error = '' }
      } catch {
        if (!connectionLost) connectionLost = true
        // Heal in place instead of showing a Reconnect popup.
        void silentReconnect()
      }
      schedule()
    }
    schedule()
    return () => { stopped = true; if (timer) clearTimeout(timer) }
  })

  // Reconnect the instant the OS reports the network is back, or when the user
  // returns to a backgrounded window - the two moments a dropped remote DB most
  // likely became reachable again. Both are no-ops unless the connection is lost.
  $effect(() => {
    if (typeof window === 'undefined') return
    const kick = () => { if (connection && connectionLost) void silentReconnect() }
    const onVis = () => {
      if (typeof document === 'undefined') return
      if (document.hidden) {
        // Backgrounded/minimized: stop the backend live watcher so it stops
        // polling the DB (and doing remote round-trips) for a window nobody can
        // see. liveTableKey is non-empty only when a watcher is actually running
        // (live enabled + supported + a table tab active), which is exactly the
        // "live is active" guard; the flag prevents redundant stops.
        if (liveTableKey && !_livePausedByHide) {
          _livePausedByHide = true
          void liveStop().catch(() => {})
        }
      } else {
        kick()
        // Back in view: resume only a watcher we paused on hide, retargeting
        // whatever table is active now. Re-checking liveTableKey means we never
        // start when live got disabled or the table tab went away while hidden,
        // and clearing the flag first prevents a double-start.
        if (_livePausedByHide) {
          _livePausedByHide = false
          if (liveTableKey) {
            const [schema, table] = JSON.parse(liveTableKey)
            void liveStart(schema, table).catch(() => {})
          }
        }
      }
    }
    window.addEventListener('online', kick)
    window.addEventListener('focus', kick)
    document.addEventListener('visibilitychange', onVis)
    return () => {
      window.removeEventListener('online', kick)
      window.removeEventListener('focus', kick)
      document.removeEventListener('visibilitychange', onVis)
    }
  })

  const sqlSchemaHints = $derived.by(() => {
    // The SQL editor is the always-on consumer, and building columnsByTable
    // iterates the whole table-column cache (dozens of tables). Skip that work
    // unless the SQL view is active - otherwise every table-tab switch paid for
    // hints nothing was showing. When the user opens SQL, activeView flips and
    // this rebuilds fresh from the current caches.
    if (activeView !== 'sql') {
      return { schemas, activeSchema, tables: _tableNames, columnsByTable: /** @type {Record<string, string[]>} */ ({}) }
    }
    return buildSqlHints()
  })

  // ── Every column of the schema, for completion ─────────────────────────────
  // The column cache above only holds tables whose rows were opened, so a query
  // naming any other table got no column suggestions at all. One catalog call
  // (the one the ER diagram uses) brings the whole schema's columns, typed.
  // Loaded the first time completion needs it, or when the SQL tab opens.

  /** @type {Record<string, Array<{ name: string, type: string }>>} */
  let _schemaColumns = $state({})
  /** Connection + schema the columns above belong to. */
  let _schemaColumnsFor = ''
  /** That, plus the table list generation: a new table (or a dropped one) reloads. */
  let _schemaColumnsKey = ''
  let _schemaColumnsGen = 0
  /** @type {Promise<void>} */
  let _schemaColumnsReady = Promise.resolve()
  $effect(() => {
    void _tableNames
    _schemaColumnsGen++
  })

  /** Load the active schema's columns once per schema and table list. */
  function loadSchemaColumns() {
    if (!connection || !activeSchema) return Promise.resolve()
    const owner = `${persistConnectionId}:${activeSchema}`
    const key = `${owner}:${_schemaColumnsGen}`
    if (key === _schemaColumnsKey) return _schemaColumnsReady
    _schemaColumnsKey = key
    // Another schema's columns would be wrong; this schema's older ones are
    // only incomplete, so they stay until the new set lands.
    if (owner !== _schemaColumnsFor) {
      _schemaColumnsFor = owner
      _schemaColumns = {}
    }
    _schemaColumnsReady = getSchemaColumnStructure(activeSchema)
      .then((rows) => {
        if (_schemaColumnsKey !== key) return
        /** @type {Record<string, Array<{ name: string, type: string }>>} */
        const next = {}
        for (const { table, columns: cols } of rows ?? []) {
          next[table] = cols.map((c) => ({ name: c.name, type: c.dataType ?? '' }))
        }
        _schemaColumns = next
      })
      .catch(() => {
        // Let the next request try again rather than caching the failure.
        if (_schemaColumnsKey === key) _schemaColumnsKey = ''
      })
    return _schemaColumnsReady
  }

  /** @param {any[]} cols */
  const typedColumns = (cols) => cols.map((c) => ({ name: c.name, type: c.dataType ?? c.data_type ?? '' }))

  /**
   * The full completion hints, on demand. The table view's review dock calls
   * this only while it is open, so the cost above is paid only then.
   */
  function buildSqlHints() {
    /** @type {Record<string, Array<string | { name: string, type: string }>>} */
    const columnsByTable = { ..._schemaColumns }
    void tableColumnsVersion
    for (const [key, cols] of tableColumnsCache) {
      columnsByTable[key] = typedColumns(cols)
    }
    if (activeTable && _activeColsTyped.length) {
      columnsByTable[activeTable] = _activeColsTyped
      columnsByTable[`${activeSchema}.${activeTable}`] = _activeColsTyped
    }
    if (_sqlColNames.length) {
      columnsByTable.__result__ = _sqlColNames
    }
    return {
      schemas,
      activeSchema,
      tables: _tableNames,
      columnsByTable,
      enumValues: _sqlEnumValues,
      userFunctions: _sqlUserFunctions,
      loadColumns: loadSchemaColumns,
    }
  }

  const connectionId = $derived(
    connection
      ? `${connection.host}:${connection.port}/${connection.database}@${connection.user}`
      : '',
  )

  // ID of the currently-active saved connection (for highlighting in the palette)
  const activeConnectionId = $derived(
    connection
      ? (savedConnections.find((c) => {
          if (c.type === 'sqlite') return c.filePath === connection.filePath
          if (c.type === 'd1') return c.databaseId === connection.databaseId && c.accountId === connection.accountId
          return c.host === connection.host && c.database === connection.database && c.user === connection.user
        })?.id ?? '')
      : '',
  )

  const persistConnectionId = $derived(activeConnectionId || connectionId)

  /**
   * Title-bar label for the current connection. A file-backed connection would
   * otherwise show its whole path - and a D1 local database's miniflare path
   * (`…/.wrangler/state/v3/d1/miniflare-D1DatabaseObject/<hash>.sqlite`) both
   * overflows the bar and says nothing. Prefer the SQL database name, then the
   * connection's own label, and only fall back to the file's basename.
   */
  const windowTitle = $derived.by(() => {
    const c = connection
    if (!c) return 'Stroke'
    // A provider connection is named after where it lives ("Railway ·
    // luminous-flexibility / MySQL"). Its database name is usually the
    // template's generic one - Railway's `railway`, Supabase's `postgres`,
    // Neon's `neondb` - so titling by database showed "railway" for every
    // Railway service. Title by the name, minus the provider prefix.
    if (c.provider && c.name) return c.name.replace(/^[^·]+·\s*/, '') || c.name
    if (c.database) return c.database
    if (c.name) return c.name
    if (c.filePath) return c.filePath.split(/[\\/]/).pop() || c.filePath
    return 'Stroke'
  })

  // Keep MCP layer in sync with saved connections + active connection (no passwords sent).
  $effect(() => {
    void mcpUpdateConnections(savedConnections, activeConnectionId || null)
  })

  // Persist the Query Editor buffer per connection so reopening the tab (or
  // restarting the app) restores where the user left off. Debounced so fast
  // typing doesn't hammer localStorage; gated on `sqlEverOpened` so the initial
  // "SELECT 1;" default can't clobber a real saved draft before the editor is used.
  /** @type {ReturnType<typeof setTimeout> | null} */
  let _sqlDraftTimer = null
  $effect(() => {
    const text = sqlText
    const cid = persistConnectionId
    if (!sqlEverOpened) return
    // A DDL viewer tab is a scratch buffer, not the user's query draft.
    if (/** @type {any} */ (activeTab)?.draft === false) return
    if (_sqlDraftTimer) clearTimeout(_sqlDraftTimer)
    _sqlDraftTimer = setTimeout(() => saveSqlDraft(cid, text), 400)
  })


  /** @type {import('$lib/stores/query-history.js').QueryHistoryEntry[]} */
  let queryHistory = $state([])
  /** @type {import('$lib/stores/query-history.js').SavedQuery[]} */
  let savedQueries = $state([])
  let queryHistoryVisible = $state(loadQueryHistoryPref())

  function refreshRecentTabs() {
    recentTabs = persistConnectionId ? loadRecentTabs(persistConnectionId) : []
  }

  async function refreshQueryStores() {
    if (!persistConnectionId) {
      queryHistory = []
      savedQueries = []
      return
    }
    const [history, saved] = await Promise.all([
      listQueryHistory(persistConnectionId),
      listSavedQueries(persistConnectionId),
    ])
    queryHistory = history
    savedQueries = saved
  }

  $effect(() => {
    saveQueryHistoryPref(queryHistoryVisible)
  })

  $effect(() => {
    if (commandOpen && persistConnectionId) void refreshQueryStores()
  })

  $effect(() => {
    // Reload recents, charts, and dashboards whenever the active connection changes
    void persistConnectionId
    refreshRecentTabs()
    switchChartsConnection(persistConnectionId)
    switchDashboardsConnection(persistConnectionId)
    switchDiagramsConnection(persistConnectionId)
  })

  $effect(() => {
    if (!commandOpen) return
    showConnectionModal = false
    showSettingsModal = false
    showShortcutsModal = false
  })

  const _aiActive = $derived(aiMode || aiSidebarOpen)

  // The all-tables column map only changes when the table-column cache changes
  // (a new table is fetched), NOT when the user switches to an already-cached
  // table. Splitting it into its own derived means a table click no longer
  // rebuilds this whole object - that rebuild (Object.fromEntries over every
  // cached table) was a main-thread stall on each table switch while AI was open.
  const _allTableColumns = $derived.by(() => {
    if (!_aiActive) return /** @type {Record<string, { name: string, dataType: string, nullable: boolean, enumValues?: string[] }[]>} */ ({})
    return Object.fromEntries(
      [...tableColumnsCache.entries()].map(([key, cols]) => [
        key,
        cols.map(normalizeColumn),
      ]),
    )
  })

  const aiSchemaContext = $derived.by(() => {
    // Only rebuild the expensive schema context when AI is actually visible.
    // When AI is hidden, return a cheap stable object - components using it
    // are either hidden or inert, so stale data is fine.
    if (!_aiActive) {
      return {
        schemas, activeSchema, tables: _tableNames,
        activeTable, columns: [], primaryKey: [], foreignKeys: [],
        allTableColumns: {}, dbType: engineFamily(connection?.type),
        environment: connection?.environment ?? null,
      }
    }
    return {
      schemas,
      activeSchema,
      tables: tables.map((t) => ({ name: t.name, rowCount: t.rowCount })),
      activeTable,
      columns: columns.map(normalizeColumn),
      primaryKey,
      foreignKeys,
      allTableColumns: _allTableColumns,
      /** @type {import('$lib/stores/connections.js').DbType} */
      dbType: engineFamily(connection?.type),
      environment: connection?.environment ?? null,
    }
  })

  const inspectorTarget = $derived.by(() => {
    if (activeTab?.kind !== 'table') return null
    if (inspectorRow !== null) {
      return { kind: 'row', rowIdx: inspectorRow }
    }
    return null
  })

  // Columns/rows narrowed to visible columns for the JSON/text views (the
  // record view filters internally so it can keep original column indices).
  const dataViewColumns = $derived(columns.filter((c) => !hiddenColumns.has(c.name)))
  const dataViewRows = $derived.by(() => {
    if (dataViewMode !== 'json' && dataViewMode !== 'text' && dataViewMode !== 'chart') return []
    if (dataViewColumns.length === columns.length) return rows
    const idxs = columns.map((_, i) => i).filter((i) => !hiddenColumns.has(columns[i].name))
    return rows.map((r) => idxs.map((i) => r[i]))
  })

  const activeTabIndex = $derived(
    activeTabId ? tabs.findIndex((t) => t.id === activeTabId) : -1,
  )

  /** @returns {TableTabState} */
  function captureTableSnapshot() {
    // A windowed result set holds a sparse array as long as the whole table, so it
    // never goes into `tabs` (a $state proxy would then trap every rows[r][c] read
    // in the draw loop). It's parked in _liveRowsByTab instead and handed back on
    // re-activation together with the window bookkeeping below - so leaving a
    // million-row table and coming back no longer re-runs the fetch and the count.
    return {
      schema: activeSchema,
      table: activeTable,
      page,
      pageSize,
      rowSearch,
      rowSort: rowSort ? { ...rowSort } : null,
      rowSortMore: rowSortMore.map((s) => ({ ...s })),
      rowFilters: rowFilters.map((f) => ({ ...f })),
      columns,
      primaryKey,
      foreignKeys,
      rows: windowed ? [] : rows,
      windowedHead: windowed,
      windowedLoaded: windowed ? [..._windowLoaded] : [],
      windowRows: windowed ? _windowRows : 0,
      windowBase: windowed ? _windowBase : 0,
      windowCount: windowed ? _windowCount : 0,
      windowBytesPerRow: windowed ? _windowBytesPerRow : 0,
      windowOrder: windowed ? _windowOrder : null,
      total,
      queryMs,
      // The real flag, not a hardcoded false: a table left mid-fetch has to read
      // as still loading, or leaving the tab makes a 1M-row load look cancelled.
      // applyTableSnapshot only trusts it while that tab is genuinely busy.
      loadingRows,
      error,
      selected: new Set(selected),
      focusedRow,
      focusedCol,
      inspectorRow,
      editingCell: editingCell ? { ...editingCell } : null,
      savingCell: false,
      hiddenColumns: new Set(hiddenColumns),
      filterBarOpen,
      dataViewMode,
      // Structure vs data is per tab, like every other thing on this list. It
      // used to be one module-level `$state` shared by every open table, so
      // opening a second tab reset the first one back to the grid - and the
      // structure you were reading belonged to whichever table was last active.
      tableViewMode,
      structureColumns,
      structureSearch,
      ...(() => { const s = tableGetScroll(); return { scrollLeft: s.left, scrollTop: s.top } })(),
      expandedRows: tableGetExpanded(),
      expandSingleRow,
    }
  }

  /** @param {TableTabState} s @param {string | null} [tabId] - owner, for the live-loading check */
  function applyTableSnapshot(s, tabId = null) {
    page = s.page
    pageSize = s.pageSize ?? loadDefaultPageSize()
    rowSearch = s.rowSearch ?? ''
    rowSort = s.rowSort ? { ...s.rowSort } : null
    rowSortMore = (s.rowSortMore ?? []).map((x) => ({ ...x }))
    rowFilters = (s.rowFilters ?? []).map((f) => ({ ...f }))
    columns = s.columns
    primaryKey = s.primaryKey
    foreignKeys = s.foreignKeys ?? []
    rows = s.rows
    total = s.total
    // The snapshot's count and its filters were taken together, so the sig for
    // the state just restored above is the one this total belongs to.
    _totalSig = rowPredicateSig
    queryMs = s.queryMs
    previewColumns = s.previewColumns ?? []
    loadingRows = !!s.loadingRows && isTabBusy(tabId)
    error = s.error
    selected = new Set(s.selected)
    focusedRow = s.focusedRow
    focusedCol = s.focusedCol ?? null
    inspectorRow = s.inspectorRow ?? null
    editingCell = s.editingCell ? { ...s.editingCell } : null
    savingCell = false
    activeTable = s.table
    hiddenColumns = new Set(s.hiddenColumns)
    filterBarOpen = s.filterBarOpen ?? false
    // Fresh tabs have no stored view mode - honor Settings → Database → Default view.
    dataViewMode = s.dataViewMode ?? /** @type {any} */ (loadSettings().defaultDataView)
    // Tabs opened before this was per-tab have no stored value; the grid is the
    // right default for them. The structure rows travel with the mode so a tab
    // restored into structure paints its OWN columns rather than flashing the
    // previous tab's while the auto-load effect refetches.
    tableViewMode = s.tableViewMode ?? 'data'
    structureColumns = /** @type {any} */ (s.structureColumns ?? [])
    structureSearch = s.structureSearch ?? ''
    expandSingleRow = !!s.expandSingleRow
    // Restore the grid scroll position for this tab. Defer one tick so that
    // if DataTable just remounted (switching from a non-table tab), the new
    // applyScroll binding is in place before we call it - otherwise the old
    // closure reads a null tableContainer and the restore is silently dropped.
    const _restoreLeft = s.scrollLeft ?? 0
    const _restoreTop = s.scrollTop ?? 0
    tick().then(() => tableApplyScroll({ left: _restoreLeft, top: _restoreTop }))
  }

  /** @returns {SqlTabState} */
  function captureSqlSnapshot() {
    return {
      sqlText,
      sqlColumns,
      // The rows themselves stay in _sqlRowsByTab / _sqlViewsByTab.
      sqlRows: [],
      sqlQueryMs,
      sqlMessage,
      // The real flag, not a hardcoded false. Saving a running tab as "idle"
      // is what made a long query look cancelled the moment you left it.
      sqlLoading,
      sqlError,
    }
  }

  /** @param {SqlTabState} s @param {string | null} [tabId] - owner, for the live-running check */
  function applySqlSnapshot(s, tabId = null) {
    sqlText = s.sqlText
    sqlColumns = s.sqlColumns
    const view = tabId ? _sqlViewsByTab.get(tabId) : undefined
    sqlWindowed = !!view
    sqlWindowStatus = null
    sqlRows = view ? view.rows : (tabId ? _sqlRowsByTab.get(tabId) : undefined) ?? s.sqlRows ?? []
    sqlQueryMs = s.sqlQueryMs
    sqlMessage = s.sqlMessage
    sqlLoading = !!s.sqlLoading && isTabBusy(tabId)
    sqlError = s.sqlError
  }

  /**
   * Write a finished run into the tab that started it.
   *
   * A run outlives the tab being in front of it: the editor is one shared
   * component driven by shell-level state, so switching tabs used to swap that
   * state out from under an in-flight query - the spinner stopped, the results
   * pane showed the other tab's snapshot, and whatever came back landed in
   * whichever tab happened to be active by then. The query itself never
   * stopped; only the UI lost track of it.
   *
   * @param {string} tabId
   * @param {Partial<SqlTabState>} patch
   */
  function patchSqlTab(tabId, patch) {
    if (patch.sqlRows) {
      // Rows go beside the tab, never into it (see _sqlRowsByTab).
      if (patch.sqlRows.length) _sqlRowsByTab.set(tabId, patch.sqlRows)
      else _sqlRowsByTab.delete(tabId)
      patch = { ...patch, sqlRows: [] }
    }
    const idx = tabs.findIndex((t) => t.id === tabId && t.kind === 'sql')
    if (idx === -1) return
    const t = tabs[idx]
    const next = [...tabs]
    next[idx] = { ...t, state: cloneSqlTabState({ .../** @type {SqlTabState} */ (t.state), ...patch }) }
    tabs = next
  }

  /**
   * Write a partial update into a table tab's own state, whichever tab is in
   * front. Same contract as patchSqlTab: a row fetch outlives the tab being
   * visible, so its results (and its loading flag) belong to the tab that
   * started it, never to whatever tab happens to be active when it lands.
   * @param {string} tabId
   * @param {Partial<TableTabState>} patch
   */
  function patchTableTab(tabId, patch) {
    const i = tabs.findIndex((t) => t.id === tabId && t.kind === 'table')
    if (i === -1) return
    const next = [...tabs]
    next[i] = { ...next[i], state: { .../** @type {TableTabState} */ (next[i].state), ...patch } }
    tabs = next
  }

  function clearTableEditor() {
    activeTable = null
    page = 1
    pageSize = loadDefaultPageSize()
    rowSearch = ''
    rowSort = null
    rowSortMore = []
    rowFilters = []
    columns = []
    primaryKey = []
    foreignKeys = []
    rows = []
    total = 0
    queryMs = 0
    previewColumns = []
    loadingRows = false
    error = ''
    selected = new Set()
    focusedRow = null
    focusedCol = null
    inspectorRow = null
    editingCell = null
    savingCell = false
  }

  // Raw (non-proxied) row arrays keyed by tab id. `tabs` is a deep-proxied
  // $state, so a rows array stored inside it and read back on re-activation
  // returns a proxy - which the canvas draw() would then index per cell per
  // frame (the scroll-lag regression). We keep the raw reference here and hand it
  // straight back on restore so the live grid always holds a genuine raw array.
  const _liveRowsByTab = new Map()

  function saveActiveTabState() {
    if (!activeTabId) return
    const idx = tabs.findIndex((t) => t.id === activeTabId)
    if (idx === -1) return
    const t = tabs[idx]
    /** @type {StudioTab | null} */
    let updated = null
    if (t.kind === 'table') {
      const state = cloneTableTabState(captureTableSnapshot())
      updated = { ...t, state, title: tableTabTitle(state) }
      // Windowed sets are cached the same way - the sparse array stays raw here
      // and out of the reactive tree. evictColdTabRows drops it once the tab
      // falls out of the recently-viewed window.
      _liveRowsByTab.set(activeTabId, rows)
    } else if (t.kind === 'sql') {
      if (!_sqlViewsByTab.has(activeTabId)) _sqlRowsByTab.set(activeTabId, sqlRows)
      updated = { ...t, state: cloneSqlTabState(captureSqlSnapshot()) }
    }
    if (updated) {
      const next = [...tabs]
      next[idx] = updated
      tabs = next
    }
  }

  /** @param {StudioTab} tab */
  async function applyTabToEditor(tab) {
    // Moving to a tab that isn't a SQL editor parks the editor's run state. A run
    // that finishes while its own tab is in the background leaves the shared
    // `sqlLoading` true (only the owning tab's state is patched), and the next
    // snapshot of ANY sql tab would then capture that stale true as its own.
    if (tab.kind !== 'sql') sqlLoading = false
    if (tab.kind === 'welcome' || tab.kind === 'ai' || tab.kind === 'schema' || tab.kind === 'orm' || tab.kind === 'ddl') {
      clearTableEditor()
      return
    }
    if (tab.kind === 'sql' && tab.state) {
      clearTableEditor()
      applySqlSnapshot(cloneSqlTabState(/** @type {SqlTabState} */ (tab.state)), tab.id)
      return
    }
    if (tab.kind === 'table' && tab.state) {
      const raw = /** @type {TableTabState} */ (tab.state)
      if (raw.schema && raw.schema !== activeSchema) {
        activeSchema = raw.schema
        await loadTables()
      }
      if (raw.columns.length === 0) {
        // No cached data - apply lightweight snapshot (no need to clone rows)
        if (windowed) resetWindowing()
        applyTableSnapshot(raw, tab.id)
        // A fetch this tab already has in flight (including a windowed loadRows
        // it started before you left) keeps running and lands here on its own -
        // starting a second one would duplicate the work and fight over the grid.
        if (raw.table && !isTabBusy(tab.id)) void startTabFetch(tab.id)
      } else {
        // Has cached data - clone Sets so mutations don't bleed between tabs, and
        // restore the RAW rows reference (not the proxied tab.state.rows) so the
        // grid doesn't index a proxy on the scroll hot path.
        const snap = cloneTableTabState(raw)
        const rawRows = _liveRowsByTab.get(tab.id)
        if (rawRows) snap.rows = rawRows
        // A windowed tab comes back with its sparse array and its resident window
        // set intact, so the rows already fetched are still on screen and only new
        // windows are requested. The length check is the consistency guard: without
        // the cached array (evicted), fall through to a clean refetch.
        if (raw.windowedHead) {
          const want = raw.windowCount ?? raw.total
          if (Array.isArray(rawRows) && want > 0 && rawRows.length === want) {
            restoreWindowing(raw)
          } else {
            resetWindowing()
            snap.rows = []
            snap.columns = []
            applyTableSnapshot(snap, tab.id)
            if (raw.table && !isTabBusy(tab.id)) void startTabFetch(tab.id)
            return
          }
        } else if (windowed) {
          // Coming from a windowed tab into a normal one.
          resetWindowing()
        }
        applyTableSnapshot(snap, tab.id)
      }
    }
  }

  // F12 or Ctrl/Cmd+Shift+I → toggle DevTools (no-op in release builds)
  createHotkey('F12', (e) => { e.preventDefault(); void toggleDevtools() })
  createHotkey('Mod+Shift+I', (e) => { e.preventDefault(); void toggleDevtools() })

  createHotkey('Mod+K', (e) => {
    e.preventDefault()
    commandPage = 'root'
    commandOpen = true
  })

  // Ctrl/⌘+P - table search. Registered as a capture-phase window listener (not
  // createHotkey) so it beats the webview's native Print accelerator on
  // WebKitGTK/WebView2 - otherwise the print dialog opens first and the handler
  // never runs. ⌘⇧P is the page navigator, one modifier away.
  $effect(() => {
    /** @param {KeyboardEvent} e */
    function onKeyP(e) {
      if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && (e.key === 'p' || e.key === 'P')) {
        e.preventDefault()
        e.stopPropagation()
        // Only open+set page mode from a closed state - never yank the page mode
        // if the palette is already open mid-interaction.
        if (commandOpen) return
        commandPage = 'tables'
        commandOpen = true
      }
    }
    window.addEventListener('keydown', onKeyP, { capture: true })
    return () => window.removeEventListener('keydown', onKeyP, { capture: true })
  })

  createHotkey('Mod+Shift+E', (e) => {
    if (!connection) return
    e.preventDefault()
    if (!$hasPro) { showProGate = true; return }
    if (aiMode) exitAiMode()
    else enterAiMode()
  })

  createHotkey('Mod+F', (e) => {
    if (commandOpen || showConnectionModal || showSettingsModal) return
    // Inside a code editor (the cell panel's CodeMirror, its find bar
    // included) Mod+F is the editor's own find and replace. Taking it here
    // pulled focus up to the table's row search instead.
    if (e.target instanceof Element && e.target.closest('.cm-editor')) return
    // Find means "search what this page is showing", and on the objects page
    // that is its own box. It used to mean nothing there at all.
    if (activeTab?.kind === 'objects') { e.preventDefault(); objectsFocusSearch?.(); return }
    if (activeTab?.kind === 'search') { e.preventDefault(); dbSearchFocusInput?.(); return }
    if (activeTab?.kind !== 'table' || !activeTable) return
    e.preventDefault()
    tableToolbar?.focusRowSearch?.()
  })

  createHotkey('Mod+Shift+G', (e) => {
    if (!connection) return
    e.preventDefault()
    commandOpen = false
    openSearchTab()
  })

  createHotkey('Mod+Shift+X', (e) => {
    if (!connection) return
    e.preventDefault()
    commandOpen = false
    if (aiMode) exitAiMode()
    openExtensionsTab()
  })

  createHotkey('Mod+Enter', (e) => {
    if (activeTab?.kind !== 'sql' || !connection) return
    e.preventDefault()
    runSql()
  })

  createHotkey('Mod+W', (e) => {
    if (!connection) return
    e.preventDefault()
    if (aiMode) { exitAiMode(); return }
    closeActiveTab()
  })

  // Close all tabs. This was the sequence `Mod+K` then `W`, which could not
  // work: `Mod+K` is bound to the command palette, so the first step opened a
  // dialog and moved focus into its search field - the `W` then went in as a
  // character and never reached the document, and on the occasions it did, the
  // palette had already flashed open and shut. A chord whose first key is
  // already a command cannot be a prefix.
  createHotkey('Mod+Shift+W', (e) => {
    if (!connection) return
    e.preventDefault()
    void closeAllTabs()
  })

  createHotkey('Mod+N', (e) => {
    if (!connection) return
    e.preventDefault()
    openWelcomeTab()
  })

  createHotkey('Mod+T', (e) => {
    if (!connection) return
    e.preventDefault()
    commandPage = 'tables'
    commandOpen = true
  })

  // Tab cycling lives in onWindowKeydownCapture below, not here. Two reasons it
  // cannot go through the hotkey layer:
  //
  //  - `Mod` is the wrong modifier for it. On macOS Mod is ⌘, and ⌘Tab is the
  //    system app switcher: the keydown never reaches the webview, so the
  //    binding was dead on that platform in both directions. Ctrl+Tab is what
  //    every browser uses on all three platforms, and nothing above the app
  //    claims it.
  //  - Tab is a focus key. Anything between the focused element and `document`
  //    that handles focus movement - the grid canvas, the Monaco editors, the
  //    bits-ui overlays - can stop the chord before the document-level bubble
  //    listener sees it. Capture on window is the only phase that is reliably
  //    ahead of all of them.

  // ⌘B / Ctrl+B is bound in the CAPTURE phase on window, not through
  // createHotkey. The hotkey layer listens on `document` in the BUBBLE phase, so
  // anything between the focused element and the document that calls
  // stopPropagation eats the chord first - and the places you most want to hide
  // the sidebar from are exactly those: the Monaco editors, the grid canvas, and
  // the bits-ui overlays. That is why SqlEditor and OrmRunner had to be handed a
  // manual `onmodb` callback to forward the key back out; capture makes the one
  // global binding reach every one of them, so the forwarding is gone.
  //
  // Matching the modifier per platform rather than accepting either one matters
  // on macOS: Ctrl+B is the emacs "move backward" binding that text fields there
  // still honour, and swallowing it would break caret movement in every input.
  function onWindowKeydownCapture(/** @type {KeyboardEvent} */ e) {
    // Ctrl+Tab / Ctrl+Shift+Tab cycle tabs on every platform - see the note
    // above the ⌘B block for why this is Ctrl rather than Mod, and why it is
    // captured on window rather than registered as a hotkey. Shift only picks
    // the direction, so both directions are one binding and cannot drift apart.
    if (e.key === 'Tab' && e.ctrlKey && !e.metaKey && !e.altKey) {
      if (!connection || tabs.length < 2) return
      e.preventDefault()
      e.stopPropagation()
      cycleTab(e.shiftKey ? -1 : 1)
      return
    }

    const modOnly = IS_MAC ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey
    if (!modOnly || e.altKey || e.shiftKey) return
    if (e.key.toLowerCase() !== 'b') return
    // The connection dialog owns the screen while it's open, and it binds ⌘B to
    // its own connections rail - toggling the workspace sidebar behind it would
    // be an invisible edit the user only discovers after closing the dialog.
    // Left un-stopped so the dialog's own handler still receives it.
    if (showConnectionModal) return
    e.preventDefault()
    e.stopPropagation()
    toggleSidebar()
  }

  createHotkey('Mod+Shift+B', (e) => {
    e.preventDefault()
    toggleStatusBar()
  })

  // Reopen the most recently closed tab (browser-style).
  createHotkey('Mod+Shift+T', (e) => {
    if (!connection) return
    e.preventDefault()
    reopenLastClosedTab()
  })

  // Tab-bar visibility toggle moved here so Mod+Shift+T can reopen closed tabs.
  createHotkey('Alt+Shift+T', (e) => {
    e.preventDefault()
    toggleTabBar()
  })

  // ── Data views / find & replace / database switching ─────────────────────

  const DATA_VIEW_CYCLE = /** @type {const} */ (['table', 'json', 'record', 'text', 'chart'])

  // Cycle through the table data views (Table → JSON → Record → Text → Chart).
  createHotkey('Mod+Shift+V', (e) => {
    if (!connection || !activeTable || columns.length === 0) return
    e.preventDefault()
    const i = DATA_VIEW_CYCLE.indexOf(/** @type {any} */ (dataViewMode))
    dataViewMode = /** @type {any} */ (DATA_VIEW_CYCLE[(i + 1) % DATA_VIEW_CYCLE.length])
  })

  // Jump straight to a data view: Alt+1 Table, Alt+2 JSON, Alt+3 Record,
  // Alt+4 Text, Alt+5 Chart.
  for (const [i, view] of DATA_VIEW_CYCLE.entries()) {
    createHotkey(`Alt+${i + 1}`, (e) => {
      if (!connection || !activeTable || columns.length === 0) return
      e.preventDefault()
      dataViewMode = /** @type {any} */ (view)
    })
  }

  /** Flip the window in or out of full screen. Shared by F11 and the View menu. */
  async function toggleFullscreen() {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      const win = getCurrentWindow()
      await win.setFullscreen(!(await win.isFullscreen()))
    } catch { /* not Tauri, or the window refused - nothing to fall back to */ }
  }

  /** Open another Stroke window (⌘⇧N and File ▸ New window). */
  function openNewWindow() {
    void import('@tauri-apps/api/core')
      .then(({ invoke }) => invoke('open_new_window'))
      .catch((err) => toast.error('Could not open a new window', { description: String(err) }))
  }

  /**
   * What the menu bar can do, by name. Every entry is an action that already
   * exists elsewhere in this component - the bar is a second door to them, not a
   * second implementation.
   */
  const menuActions = $derived({
    newTab: () => openWelcomeTab(),
    newSql: () => openNewSqlTab(),
    newWindow: openNewWindow,
    openConnection: () => (showConnectionModal = true),
    disconnect: () => { if (connection) showDisconnectDialog = true },
    openSettings: () => (showSettingsModal = true),
    closeTab: () => { if (activeTabId) void closeTab(activeTabId) },
    closeAllTabs: () => void closeAllTabs(),

    search: () => tableToolbar?.focusRowSearch?.(),
    findReplace: () => openFindReplacePanel(),
    findInDatabase: () => openSearchTab(),
    applyEdits: () => void applyEdits(),
    copyEditsSql: () => void copyEditsSql(),
    resetEdits: () => resetEdits(),

    toggleSidebar,
    toggleChat: () => { if (aiMode) exitAiMode(); toggleAiSidebar() },
    zoomIn: () => increaseZoom(),
    zoomOut: () => decreaseZoom(),
    zoomReset: () => resetZoom(),
    fullscreen: () => void toggleFullscreen(),
    commandPalette: () => { commandPage = 'root'; commandOpen = true },

    objects: () => openObjectsTab(),
    insights: () => openInsightsTab(),
    schema: () => openSchemaTab(),
    erd: () => openErdTab(),
    dataDiff: () => openDataDiffTab(),
    dashboard: () => openDashboardTab(),
    logs: () => openLogsTab(),
    extensions: () => openExtensionsTab(),

    shortcuts: () => (showShortcutsModal = true),
    changelog: () => void openExternalUrl('https://stroke.click/changelog?utm_source=stroke-app&utm_medium=menu&utm_campaign=changelog'),
    reportIssue: () => (showReportIssueDialog = true),
    checkUpdates: () => void updateDialog?.checkNow?.(),
    about: () => (showAboutModal = true),
  })

  // A second Stroke window. Same process and the same connection - the backend
  // holds one pool - so this is another view of the session, which is what makes
  // "the table on one screen, the editor on the other" possible.
  createHotkey('Mod+Shift+N', (e) => {
    e.preventDefault()
    openNewWindow()
  })

  // Staged grid changes: ⌘S writes them, ⌘⌥S copies the SQL. Both are guarded on
  // there being staged changes AND a table tab being active, so the SQL editor's
  // own ⌘S (save query) is untouched - the two never both apply.
  createHotkey('Mod+S', (e) => {
    if (activeTab?.kind !== 'table' || pendingEditCount === 0) return
    e.preventDefault()
    void applyEdits()
  })
  // Not ⌘⇧S: that already opens the SQL editor everywhere in the app, and a
  // chord that means two things depending on whether a cell was edited is worse
  // than one extra modifier.
  createHotkey('Mod+Alt+S', (e) => {
    if (activeTab?.kind !== 'table' || pendingEditCount === 0) return
    e.preventDefault()
    void copyEditsSql()
  })
  // Discard them: ⌘⌥⌫. Guarded the same way, and deliberately NOT a bare ⌘⌫ -
  // that deletes the selected rows, and the two are the last pair in the app
  // that should share a chord. Deletion of work in progress also earns a
  // modifier more than applying it does.
  // Discard every staged change. Alt+⌫ is what the Reset button advertises and
  // what the hand reaches for; ⌘⌥⌫ stays bound because it shipped and someone's
  // fingers know it.
  for (const combo of ['Alt+Backspace', 'Mod+Alt+Backspace']) {
    createHotkey(combo, (e) => {
      if (activeTab?.kind !== 'table' || pendingEditCount === 0) return
      // `input-shortcuts.js` shields the editing chords from this layer, but it
      // resolves these to no editing mode and so lets them through. Alt+⌫ is
      // also word-delete in a text field, and discarding every staged change
      // because someone reached for that is not a trade worth making.
      if (isTypingTarget(document.activeElement)) return
      e.preventDefault()
      resetEdits()
    })
  }

  // Find & replace in the current table - editor-style Ctrl/⌘+H.
  createHotkey('Mod+H', (e) => {
    // In a code editor this is the editor's own replace, not the table's.
    if (e.target instanceof Element && e.target.closest('.cm-editor')) return
    if (!connection || !activeTable || columns.length === 0 || !findReplaceEnabled) return
    e.preventDefault()
    openFindReplacePanel()
  })

  // Also bind Cmd/Ctrl+Alt+F (VS Code's macOS "replace" shortcut). On macOS the OS
  // swallows Cmd+H to hide the app, so Mod+H never reaches us there - this is the
  // reliable cross-platform binding.
  createHotkey('Mod+Alt+F', (e) => {
    // In a code editor this is the editor's own replace, not the table's.
    if (e.target instanceof Element && e.target.closest('.cm-editor')) return
    if (!connection || !activeTable || columns.length === 0 || !findReplaceEnabled) return
    e.preventDefault()
    openFindReplacePanel()
  })

  // Switch to saved database connection N (Mod+Alt+1..9) - plain digits, not
  // shifted ones, so the binding survives non-US keyboard layouts.
  for (let n = 1; n <= 9; n++) {
    createHotkey(`Mod+Alt+${n}`, (e) => {
      const conn = savedConnections[n - 1]
      if (!conn) return
      e.preventDefault()
      if (conn.id && conn.id === activeConnectionId) return
      void handleSwitchDatabase(conn)
    })
  }

  // Page navigator - the VS Code chord, one modifier off ⌘P's table search.
  createHotkey('Mod+Shift+P', (e) => {
    e.preventDefault()
    commandPage = 'pages'
    commandOpen = true
  })

  // Data view (⌘⌥D). It used to be ⌘⇧D; that key now disconnects, which is the
  // more consequential of the two and the one worth having under the easier
  // chord.
  createHotkey('Mod+Alt+D', (e) => {
    if (!connection) return
    e.preventDefault()
    void focusDataView()
  })

  // Open the keyboard-shortcuts reference (Ctrl/⌘+/ - same key as Mod+? without shift).
  createHotkey('Mod+/', (e) => {
    if (commandOpen || showConnectionModal || showSettingsModal || showShortcutsModal) return
    e.preventDefault()
    showShortcutsModal = true
  })

  // Jump straight to tab N (Ctrl/⌘+1..8); 9 always jumps to the last tab -
  // the same convention as browsers and editors.
  for (let n = 1; n <= 9; n++) {
    createHotkey(`Mod+${n}`, (e) => {
      if (!connection || tabs.length === 0) return
      e.preventDefault()
      const idx = n === 9 ? tabs.length - 1 : Math.min(n - 1, tabs.length - 1)
      void activateTab(tabs[idx].id)
    })
  }

  createHotkey('Mod+Shift+L', (e) => {
    e.preventDefault()
    openLogsTab()
  })

  // Toggle the bottom query-log console.
  createHotkey('Mod+Shift+K', (e) => {
    e.preventDefault()
    queryLogOpen = !queryLogOpen
  })

  createHotkey('Mod+M', (e) => {
    e.preventDefault()
    cycleTheme()
  })

  createHotkey('Mod+Shift+M', (e) => {
    e.preventDefault()
    restorePreviousTheme()
  })

  // Refresh everything - schemas, table list, and the active table's rows.
  createHotkey('Mod+Shift+R', (e) => {
    if (!connection) return
    e.preventDefault()
    void (async () => {
      await handleRefresh()
      toast.success('Refreshed', { description: 'Schemas, tables, and rows reloaded' })
    })()
  })

  // Disconnect (⌘⇧D) - opens the confirm dialog, where Enter confirms.
  createHotkey('Mod+Shift+D', (e) => {
    if (!connection) return
    e.preventDefault()
    showDisconnectDialog = true
  })

  createHotkey('Mod+Shift+S', (e) => {
    if (!connection) return
    e.preventDefault()
    void focusSqlView()
  })

  createHotkey('Mod+Shift+O', (e) => {
    if (!connection) return
    e.preventDefault()
    openOrmTab()
  })

  createHotkey('Mod+Shift+F', (e) => {
    if (commandOpen || showConnectionModal || showSettingsModal) return
    e.preventDefault()
    const el = document.querySelector('[data-sidebar-filter]')
    if (el instanceof HTMLElement) {
      if (!sidebarOpen) toggleSidebar()
      el.focus()
    }
  })

  // Table toolbar menus - open the Filter / Sort / Columns menu for the active
  // table tab (data view only). Guarded so they don't fire while typing.
  /** @param {KeyboardEvent} e */
  function tableMenuHotkeyGuard(e) {
    if (activeTab?.kind !== 'table' || tableViewMode !== 'data') return false
    if (commandOpen || showConnectionModal || showSettingsModal) return false
    const el = document.activeElement
    if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || (el instanceof HTMLElement && el.isContentEditable)) return false
    return true
  }

  createHotkey('Alt+A', (e) => {
    if (!tableMenuHotkeyGuard(e)) return
    e.preventDefault()
    // Seeded with the column the cell cursor is on: the filter you want is
    // nearly always about the cell you are looking at.
    tableToolbar?.openFilterMenu?.(dataTable?.focusedColumnName?.() ?? '')
  })

  createHotkey('Alt+S', (e) => {
    if (!tableMenuHotkeyGuard(e)) return
    e.preventDefault()
    tableToolbar?.openSortMenu?.()
  })

  createHotkey('Alt+C', (e) => {
    if (!tableMenuHotkeyGuard(e)) return
    e.preventDefault()
    tableToolbar?.openColumnsMenu?.()
  })

  // Switch connection. The shortcuts dialog has listed ⌘⇧C under Navigation for
  // as long as it has existed, and nothing was bound to it - the quick-access
  // tile now prints the chord, which is the wrong place to find out it does
  // nothing.
  createHotkey('Mod+Shift+C', (e) => {
    e.preventDefault()
    commandOpen = false
    if (aiMode) exitAiMode()
    showConnectionModal = true
  })

  // Reset the active table tab to its unfiltered default (clears search, filters,
  // sort, hidden columns, custom view, and resets the data view + page). Works in
  // any table view mode, but not while typing in an input.
  createHotkey('Alt+R', (e) => {
    if (activeTab?.kind !== 'table' || !activeTable) return
    if (commandOpen || showConnectionModal || showSettingsModal) return
    const el = document.activeElement
    if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || (el instanceof HTMLElement && el.isContentEditable)) return
    e.preventDefault()
    resetTableView()
  })

  createHotkey('Escape', (e) => {
    // Close dialogs in reverse z-index order (topmost first).
    // Each branch prevents lower-priority handlers from firing.
    if (commandOpen)           { e.preventDefault(); commandOpen = false;           return }
    if (showShortcutsModal)    { e.preventDefault(); showShortcutsModal = false;    return }
    if (showAiModelSettings)   { e.preventDefault(); showAiModelSettings = false;   return }
    if (showAboutModal)        { e.preventDefault(); showAboutModal = false;        return }
    if (showInsiderModal)      { e.preventDefault(); showInsiderModal = false;      return }
    if (showDisconnectDialog)  { e.preventDefault(); showDisconnectDialog = false;  return }
    if (showCreateTableDialog) { e.preventDefault(); showCreateTableDialog = false; return }
    if (showDockerModal)       { e.preventDefault(); showDockerModal = false;       return }
    if (showMcpPanel)          { e.preventDefault(); showMcpPanel = false;          return }
    if (showConnectionModal)   { e.preventDefault(); showConnectionModal = false;   return }
    if (showSettingsModal)     { e.preventDefault(); showSettingsModal = false;     return }
    if (vcolPanelOpen)         { e.preventDefault(); vcolPanelOpen = false;         return }
    if (editingCell) {
      e.preventDefault()
      editingCell = null
      return
    }
    if (filterBarOpen) { e.preventDefault(); filterBarOpen = false; return }
    if (!inspectorTarget) return
    e.preventDefault()
    closeInspector()
  })

  createHotkey('Mod+Backspace', (e) => {
    if (commandOpen || showConnectionModal || showSettingsModal) return
    // Ctrl+Backspace is the word-delete shortcut in inputs/textareas - don't steal it
    const el = document.activeElement
    if (
      el instanceof HTMLInputElement ||
      el instanceof HTMLTextAreaElement ||
      (el instanceof HTMLElement && el.isContentEditable)
    ) return
    if (activeTab?.kind !== 'table' || !activeTable) return
    // No checkbox selection is not "nothing to delete": the grid falls back to
    // the focused row, which is the row the menu offers this chord on.
    e.preventDefault()
    stageDeleteSelectedRows()
  })

  createHotkey('Mod+R', (e) => {
    if (!connection) return
    if (commandOpen || showConnectionModal || showSettingsModal) return
    // Inside a SQL editor Mod+R is the editor's own (run the statement at the
    // cursor). Hotkeys with a modifier fire in editable fields too, so this ran
    // right after it and started the WHOLE buffer, replacing that run.
    if (e.defaultPrevented || (e.target instanceof Element && e.target.closest('.sql-editor-host'))) return
    e.preventDefault()
    void handleModRefresh({ statementOnly: true })
  })

  // Alt+X empties the table search from anywhere in the tab - the ✕ and Escape
  // both want the caret already in the box, and the point of a search you are
  // done with is that you have moved on to the rows.
  createHotkey('Alt+X', (e) => {
    if (!connection) return
    if (commandOpen || showConnectionModal || showSettingsModal) return
    if (activeTab?.kind !== 'table' || !activeTable) return
    if (!rowSearch.trim()) return
    e.preventDefault()
    tableToolbar?.clearRowSearch?.()
  })

  // Alt+N stages a new row, the same thing the toolbar's Add does. It joins the
  // Alt family the grid already uses for what is in front of you - Alt+F filter
  // by this value, Alt+E exclude it, Alt+D duplicate this row - and it appends,
  // so holding it out for three rows is three rows. Mod+Escape clears the band.
  createHotkey('Alt+N', (e) => {
    if (!connection) return
    if (commandOpen || showConnectionModal || showSettingsModal) return
    if (activeTab?.kind !== 'table' || !activeTable) return
    // The typing guard keeps Alt+N out of the search box and every other field,
    // but the staged band is the one place you are typing AND want another row -
    // "again for another" is the whole point of it, and it is not a chord any
    // text field claims.
    const typingIn = document.activeElement
    if (isTypingTarget(typingIn) && !(typingIn instanceof HTMLElement && typingIn.closest('[data-new-row]'))) return
    e.preventDefault()
    dtBeginInsertRow?.()
  })

  // F5 reloads what you are looking at, not the app.
  //
  // It was bound to nothing, so the webview took it and reloaded the document -
  // which tears down the session, redials the connection, re-reads the catalog
  // and refetches every list, for what anyone pressing F5 over a table means:
  // fetch these rows again. It is `handleModRefresh` now, the same contextual
  // refresh ⌘R runs, and it is always prevented so the reload cannot happen by
  // accident with unsaved edits staged.
  createHotkey('F5', (e) => {
    e.preventDefault()
    if (!connection) return
    if (commandOpen || showConnectionModal || showSettingsModal) return
    void handleModRefresh()
  })

  // Ctrl+Arrow (Windows/Linux) or Cmd+Arrow (Mac) for pagination and scroll.
  // Uses a raw listener instead of createHotkey because:
  //   1. macOS intercepts Ctrl+Arrow at the OS level for Mission Control.
  //   2. Raw listeners read current reactive signal values at call time.
  $effect(() => {
    /** @param {KeyboardEvent} e */
    function onArrowKey(e) {
      const mod = e.ctrlKey || e.metaKey
      if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(e.key)) return
      // Bail before the guards below for plain arrows - they belong to the grid,
      // and this listener sits on their hot path (key repeat while scrolling).
      if (!mod && !e.altKey) return
      if (commandOpen || showConnectionModal || showSettingsModal) return
      const el = document.activeElement
      if (
        el instanceof HTMLInputElement ||
        el instanceof HTMLTextAreaElement ||
        (el instanceof HTMLElement && el.isContentEditable)
      ) return

      // Ctrl/Cmd+Alt+Left/Right → scroll grid to the first / last column.
      if (e.altKey) {
        if (activeTab?.kind !== 'table' || !activeTable) return
        if (e.key === 'ArrowLeft') { e.preventDefault(); scrollTableLeft(); return }
        if (e.key === 'ArrowRight') { e.preventDefault(); scrollTableRight(); return }
        return
      }

      // Ctrl/Cmd+Up → scroll table to top; Ctrl/Cmd+Down → scroll table to bottom
      if (e.key === 'ArrowUp' && !e.shiftKey) {
        e.preventDefault()
        scrollTableTop()
        return
      }
      if (e.key === 'ArrowDown' && !e.shiftKey) {
        e.preventDefault()
        scrollTableBottom()
        return
      }

      // Left/Right: pagination (table tabs only)
      if (activeTab?.kind !== 'table' || !activeTable) return
      if (e.shiftKey) {
        // Ctrl/Cmd+Shift+Left → first page, Ctrl/Cmd+Shift+Right → last page
        if (e.key === 'ArrowLeft') {
          if (page <= 1) return
          e.preventDefault()
          void handlePageChange(1)
        } else {
          const lastPage = Math.max(1, Math.ceil(total / effectivePageSize))
          if (page >= lastPage) return
          e.preventDefault()
          void handlePageChange(lastPage)
        }
      } else {
        if (e.key === 'ArrowLeft') {
          if (page <= 1) return
          e.preventDefault()
          void handlePageChange(page - 1)
        } else {
          if (total >= 0 && page * effectivePageSize >= total) return
          e.preventDefault()
          void handlePageChange(page + 1)
        }
      }
    }
    document.addEventListener('keydown', onArrowKey)
    return () => document.removeEventListener('keydown', onArrowKey)
  })

  // The mouse's dedicated back/forward buttons are swallowed, and do nothing
  // else. There is no in-app history to walk any more, but the default still has
  // to be suppressed: left alone, the webview acts on them itself and walks its
  // *document* history, which in a Tauri window means navigating away from the
  // app's own page.
  //
  // Registered in the capture phase so a handler that stops propagation on its own
  // subtree (the grid canvas has several) can't swallow the button first.
  $effect(() => {
    /** @param {MouseEvent} e */
    function onMouseNav(e) {
      if (e.button !== 3 && e.button !== 4) return
      e.preventDefault()
    }
    /** Swallow the paired auxclick/mouseup so the webview can't act on them either. */
    function swallowAux(/** @type {MouseEvent} */ e) {
      if (e.button === 3 || e.button === 4) e.preventDefault()
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

  // F11 (all platforms) and Cmd+Ctrl+F (macOS standard) for fullscreen toggle.
  $effect(() => {
    const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
    if (!isTauri) return
    /** @param {KeyboardEvent} e */
    async function onFullscreenKey(e) {
      const isF11 = e.key === 'F11' && !e.metaKey && !e.ctrlKey && !e.shiftKey && !e.altKey
      const isMacFullscreen = e.key === 'f' && e.metaKey && e.ctrlKey && !e.shiftKey && !e.altKey
      if (!isF11 && !isMacFullscreen) return
      e.preventDefault()
      await toggleFullscreen()
    }
    document.addEventListener('keydown', onFullscreenKey)
    return () => document.removeEventListener('keydown', onFullscreenKey)
  })

  // "/" focuses the search box for whatever is on screen - the sidebar filter,
  // the settings search, a dialog's own field. It resolves its target from the
  // live DOM rather than being bound per field, so a search box added later is
  // reachable without anyone registering it. See $lib/focus-search.js.
  createHotkey('/', (e) => {
    if (isTypingTarget(document.activeElement)) return
    const el = findSearchInput()
    if (!el) return
    e.preventDefault()
    el.focus()
    // Select what is there: "/" then typing should replace a stale query, which
    // is what every other search-on-slash does. The caret lands at the end for
    // anyone who meant to append.
    el.select?.()
  })

  createHotkey('?', (e) => {
    if (commandOpen || showConnectionModal || showSettingsModal || showShortcutsModal) return
    // Was a tag check, which missed contenteditable and anything Monaco routes
    // through a non-textarea - so "?" in those opened the shortcuts panel
    // mid-sentence.
    if (isTypingTarget(document.activeElement)) return
    e.preventDefault()
    showShortcutsModal = true
  })

  // Same panel via a modifier chord (Ctrl/⌘+?), which works even while a text
  // field is focused, unlike the bare "?".
  createHotkey('Mod+?', (e) => {
    if (commandOpen || showConnectionModal || showSettingsModal || showShortcutsModal) return
    e.preventDefault()
    showShortcutsModal = true
  })

  createHotkey('Mod+,', (e) => {
    if (commandOpen || showConnectionModal || showShortcutsModal) return
    e.preventDefault()
    showSettingsModal = !showSettingsModal
  })

  createHotkey('Mod+I', (e) => {
    if (!connection) return
    if (commandOpen || showConnectionModal || showSettingsModal) return
    e.preventDefault()
    toggleAiSidebar()
  })

  createHotkey('Alt+I', (e) => {
    if (commandOpen || showConnectionModal || showSettingsModal) return
    e.preventDefault()
    showInsiderModal = !showInsiderModal
  })

  /** @returns {boolean} */
  function isFocusInRegion(region) {
    const el = document.activeElement
    if (!el || !(el instanceof HTMLElement)) return false
    return !!el.closest(`[data-studio-region="${region}"]`)
  }

  /** @param {{ statementOnly?: boolean }} [opts] statementOnly: Mod+R, which in a
   *  SQL tab runs the statement at the cursor; F5 re-runs the whole query. */
  async function handleModRefresh(opts = {}) {
    if (isFocusInRegion('sidebar')) {
      await loadTables({ force: true })
      return
    }
    if (activeTab?.kind === 'sql') {
      // Mod+R in a SQL tab is "run the statement at the cursor" (Run ▾, the
      // shortcuts list), from wherever focus is in the tab.
      if (opts.statementOnly) sqlConsoleRef?.runStatementAtCursor?.()
      else await runSql()
      return
    }
    if (activeTab?.kind === 'table' && activeTable) {
      await loadRows()
      return
    }
    if (activeTab?.kind === 'schema') {
      await loadSchemas()
      await loadTables({ force: true })
      return
    }
    if (activeTab?.kind === 'security') {
      securityPageRef?.refresh()
      return
    }
    if (activeTab?.kind === 'data-diff') {
      // Re-lists the source/target pickers. Mod+R means "reload what this page is
      // showing" everywhere else, and on this page what it shows IS those lists.
      await dataDiffPageRef?.refresh()
      return
    }
    if (activeTab?.kind === 'dashboard') {
      await refreshDashboardCharts()
      return
    }
    await loadTables({ force: true })
  }

  async function refreshDashboardCharts() {
    const dash = get(dashboards).find((d) => d.id === get(activeDashboardId))
    if (!dash) return
    const charts = get(savedCharts)
    const isDark = get(isCurrentThemeDark)
    await Promise.all(
      dash.items.map(async (item) => {
        const chart = charts.find((c) => c.id === item.chartId)
        if (!chart?.sql) return
        try {
          const result = await executeSql(chart.sql)
          const cols = result.columns ?? []
          const rows = result.rows ?? []
          const option = buildOption({
            type: chart.config.type,
            columns: cols,
            rows,
            xCol: chart.config.xCol,
            yCol: chart.config.yCol,
            zCol: chart.config.zCol,
            groupCol: chart.config.groupCol,
            isDark,
            title: chart.config.title,
          })
          updateChart(chart.id, { previewOption: option })
        } catch {
          // silently skip failed charts
        }
      })
    )
  }

  function closeInspector() {
    focusedRow = null
    focusedCol = null
    inspectorRow = null
    selected = new Set()
    editingCell = null
  }

  function toggleSidebar() {
    if (!connection) return
    sidebarOpen = !sidebarOpen
    saveLayout({ navSidebarOpen: sidebarOpen })
  }


  function toggleAiSidebar() {
    if (!connection) return
    aiSidebarOpen = !aiSidebarOpen
    saveLayout({ aiSidebarOpen })
  }

  function toggleStatusBar() {
    statusBarVisible = !statusBarVisible
    saveLayout({ statusBarVisible })
  }

  function toggleTabBar() {
    tabBarVisible = !tabBarVisible
    saveLayout({ tabBarVisible })
  }

  function toggleTableToolbar() {
    tableToolbarVisible = !tableToolbarVisible
    saveLayout({ tableToolbarVisible })
  }

  /**
   * Route "Fix with AI" from SqlConsole into the AI sidebar.
   * Opens the sidebar if hidden, then sends the composed message.
   * @param {{ error: string, sql: string }} detail
   */
  function handleFixWithAi({ error, sql }) {
    if (!connection) return
    // Ensure sidebar is visible
    if (!aiSidebarOpen) {
      aiSidebarOpen = true
      aiSidebarEverOpened = true
      saveLayout({ aiSidebarOpen: true })
    }
    const msg =
      `Fix this SQL error.\n\nError:\n${error}\n\nSQL:\n\`\`\`sql\n${sql}\n\`\`\`\n\n` +
      `Return the corrected SQL in a \`\`\`sql block and a brief explanation.`
    // Wait for the (lazily-imported) sidebar to mount before sending, so the very
    // first "Fix with AI" use doesn't drop the message while the chunk loads.
    whenRefReady(() => aiSidebarRef, (r) => r.sendMessage(msg))
  }

  /** Escalate a command-palette quick-ask into the full sidebar chat. */
  /** @param {string} q */
  function handleAskContinue(q) {
    if (!connection || !q) return
    if (!aiSidebarOpen) {
      aiSidebarOpen = true
      aiSidebarEverOpened = true
      saveLayout({ aiSidebarOpen: true })
    }
    whenRefReady(() => aiSidebarRef, (r) => r.sendMessage(q))
  }

  /** Context-aware Accept from the AI sidebar - routes into the right editor. */
  /** @param {{ kind: 'sql' | 'code', lang?: string, content: string }} detail */
  async function handleAiSidebarAccept(detail) {
    if (detail.kind === 'code' && activeTab?.kind === 'orm') {
      const lang = (detail.lang ?? '').toLowerCase()
      if (lang === 'prisma' || /\bprisma\./.test(detail.content)) ormMode = 'prisma'
      else if (lang === 'drizzle' || /\bdb\.(select|insert|update|delete)\b/.test(detail.content)) ormMode = 'drizzle'
      ormCode = detail.content
      openOrmTab()
      toast.success(`Inserted into ${ormMode} editor`)
      return
    }
    await openQueryInEditor(detail.content)
    toast.success('Inserted into SQL editor')
  }

  /** @param {1 | -1} direction */
  function cycleTab(direction) {
    const idx = cycleTabIndex(tabs, activeTabIndex, direction)
    if (idx < 0) return
    void activateTab(tabs[idx].id)
  }

  // ── Experimental Vim mode - global layer ────────────────────────────────────
  // `:` opens the command palette (command mode); `gt` / `gT` cycle tabs. Only
  // fires on "neutral" surfaces - inputs, Monaco editors, and the data grid own
  // their own Vim handling. A focusin listener keeps the status indicator honest.
  let _vimGPending = false
  let _vimGTimer = /** @type {ReturnType<typeof setTimeout> | 0} */ (0)
  /** @param {KeyboardEvent} e */
  function handleGlobalVimKey(e) {
    if (!$appVimMode || e.metaKey || e.ctrlKey || e.altKey) return
    const el = document.activeElement
    if (isTextEntryTarget(el) || el?.closest?.('[data-canvas-table]')) return
    const k = e.key
    if (_vimGPending) {
      _vimGPending = false
      if (_vimGTimer) { clearTimeout(_vimGTimer); _vimGTimer = 0 }
      if (k === 't') { e.preventDefault(); if (tabs.length > 1) cycleTab(1); return }
      if (k === 'T') { e.preventDefault(); if (tabs.length > 1) cycleTab(-1); return }
    }
    if (k === ':') { e.preventDefault(); commandOpen = true; setVimSubMode('command'); return }
    if (k === 'g') { _vimGPending = true; e.preventDefault(); _vimGTimer = setTimeout(() => { _vimGPending = false }, 700); return }
  }
  function handleVimFocusIn() {
    if (!$appVimMode) return
    const el = document.activeElement
    if (el?.closest?.('.monaco-editor, .sql-editor-host') || el?.closest?.('[data-canvas-table]')) return // owned by their own layers
    const isInput = el instanceof HTMLElement &&
      (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT' || el.isContentEditable)
    setVimSubMode(isInput ? 'insert' : 'normal')
  }
  onMount(() => {
    document.addEventListener('keydown', handleGlobalVimKey, true)
    document.addEventListener('focusin', handleVimFocusIn)
    return () => {
      document.removeEventListener('keydown', handleGlobalVimKey, true)
      document.removeEventListener('focusin', handleVimFocusIn)
    }
  })

  function resetTabs() {
    tabs = []
    activeTabId = null
    // Every tab id in the map/MRU/stack just died with the tab list - drop them
    // so old connections' row arrays and reopen descriptors can't be retained.
    _liveRowsByTab.clear()
    _tabRowsMru = []
    closedTabStack = []
    _loadSeqByTab.clear()
    fetchingTabIds.clear()
    _sqlQueryIdByTab.clear()
    for (const id of [..._sqlViewsByTab.keys()]) forgetSqlResult(id)
    _sqlRowsByTab.clear()
    _autoRefreshByTab.clear()
    _autoRefreshTick += 1
    _busyJobs.clear()
    _busyTick += 1
    // Every tab id in the history just died with the tab list.
    clearTableEditor()
    sqlText = 'SELECT 1;'
    sqlColumns = []
    sqlRows = []
    sqlQueryMs = 0
    sqlMessage = ''
    sqlLoading = false
    sqlError = ''
    sqlMultiResults = []
  }

  function openWelcomeTab() {
    const existing = tabs.find((t) => t.kind === 'welcome')
    if (existing) {
      activeTabId = existing.id
      clearTableEditor()
      return
    }
    saveActiveTabState()
    const tab = createWelcomeTab()
    tabs = [...tabs, tab]
    activeTabId = tab.id
    clearTableEditor()
  }

  function dropWelcomeTabs() {
    tabs = tabs.filter((t) => t.kind !== 'welcome')
  }

  function openSqlTab() {
    saveActiveTabState()
    dropWelcomeTabs()
    const existing = findSqlTab(tabs)
    if (existing) {
      void activateTab(existing.id)
      return
    }
    // Seed a fresh Query Editor tab. If a SQL tab was already opened this session
    // (keep-alive), reuse the live buffer so we don't clobber Q2/Q3/etc.;
    // otherwise restore the last saved draft for this connection (survives tab
    // close and app restart). Falls back to the default when there's no draft.
    const tab = createSqlTab(
      sqlEverOpened ? sqlText : (loadSqlDraft(persistConnectionId) ?? undefined),
    )
    tabs = [...tabs, tab]
    activeTabId = tab.id
    clearTableEditor()
    applySqlSnapshot(cloneSqlTabState(/** @type {SqlTabState} */ (tab.state)))
  }

  // Always open a brand-new, empty SQL editor tab (multiple allowed), unlike
  // openSqlTab which focuses the single existing one. Wired to the Cmd-K
  // "New SQL Editor" command so several query editors can be open at once;
  // the existing per-tab snapshot swap keeps each tab's buffer/results intact.
  function openNewSqlTab() {
    const count = tabs.filter((t) => t.kind === 'sql').length
    openSqlTabWith(undefined, count === 0 ? 'Query Editor' : `Query Editor ${count + 1}`)
  }

  /**
   * Open a brand-new SQL editor tab seeded with `sql` - the editor is where long
   * SQL belongs (search, folding, selection, run), rather than a scrollable dialog.
   * @param {string|undefined} sql @param {string} title
   * @param {{ draft?: boolean }} [opts] `draft: false` keeps this buffer out of the
   *   per-connection Query Editor draft (a DDL dump must not clobber it).
   */
  function openSqlTabWith(sql, title, { draft = true } = {}) {
    saveActiveTabState()
    dropWelcomeTabs()
    const tab = { ...createSqlTab(sql, title), draft }
    tabs = [...tabs, tab]
    activeTabId = tab.id
    clearTableEditor()
    applySqlSnapshot(cloneSqlTabState(/** @type {SqlTabState} */ (tab.state)))
  }

  /**
   * Open a read-only DDL tab, or re-focus the one already showing this object.
   * @param {string} ddlText @param {string} title
   */
  function openDdlTab(ddlText, title) {
    const existing = tabs.find((t) => t.kind === 'ddl' && t.title === title)
    if (existing) {
      // Refresh in place - the object may have been altered since it was opened.
      tabs = tabs.map((t) => (t.id === existing.id ? { ...t, state: { ddlText } } : t))
      void activateTab(existing.id)
      return
    }
    saveActiveTabState()
    dropWelcomeTabs()
    const tab = createDdlTab(ddlText, title)
    tabs = [...tabs, tab]
    activeTabId = tab.id
    clearTableEditor()
  }

  function openAiTab() {
    if (!$hasPro) { showProGate = true; return }
    enterAiMode()
  }

  // Open a pro-gated singleton tab: focus it if already open, otherwise create,
  // append, and activate it. Replaces ~13 near-identical open*Tab bodies.
  // `capability` gates availability; `capabilityFirst` runs that gate before the
  // pro check (so an unavailable feature returns silently rather than showing the
  // pro upsell) - matching the original per-tab ordering.
  /**
   * @param {{ find: (tabs: any[]) => any, create: () => any, capability?: () => boolean, capabilityFirst?: boolean }} cfg
   */
  function openSingletonTab({ find, create, capability, capabilityFirst = false }) {
    if (capabilityFirst && capability && !capability()) return
    if (!$hasPro) { showProGate = true; return }
    if (!capabilityFirst && capability && !capability()) return
    const existing = find(tabs)
    if (existing) { void activateTab(existing.id); return }
    saveActiveTabState()
    dropWelcomeTabs()
    const tab = create()
    tabs = [...tabs, tab]
    activeTabId = tab.id
    clearTableEditor()
  }

  function openSchemaTab() {
    openSingletonTab({ find: findSchemaTab, create: createSchemaTab, capability: () => hasSchemaExplorer })
  }

  function openOrmTab() {
    openSingletonTab({ find: findOrmTab, create: createOrmTab })
  }

  /** The live schema written out as Prisma / Drizzle source. */
  function openOrmSchemaTab() {
    openSingletonTab({ find: findOrmSchemaTab, create: createOrmSchemaTab, capability: () => !isRedis })
  }

  function openSecurityTab() {
    openSingletonTab({ find: findSecurityTab, create: createSecurityTab, capability: () => hasSecurity, capabilityFirst: true })
  }

  function openBackupTab() {
    openSingletonTab({ find: findBackupTab, create: createBackupTab })
  }

  function openLogsTab() {
    openSingletonTab({ find: findLogsTab, create: createLogsTab })
  }

  function openInsightsTab() {
    openSingletonTab({ find: findInsightsTab, create: createInsightsTab })
  }

  function openAdvisorTab() {
    openSingletonTab({ find: findAdvisorTab, create: createAdvisorTab })
  }

  /** The hidden game. Reached only by typing its word into a table's search. */
  function openGolfTab() {
    openSingletonTab({ find: findGolfTab, create: createGolfTab })
  }

  function openObjectsTab() {
    openSingletonTab({ find: findObjectsTab, create: createObjectsTab })
  }

  /** The Redis keyspace workspace. NOT pro-gated - it's the primary (and only)
   *  interface for a Redis connection, so it must open on connect for everyone. */
  function openRedisTab() {
    const existing = findRedisTab(tabs)
    if (existing) { void activateTab(existing.id); return }
    saveActiveTabState()
    dropWelcomeTabs()
    const tab = createRedisTab()
    tabs = [...tabs, tab]
    activeTabId = tab.id
    clearTableEditor()
  }

  function openExtensionsTab() {
    openSingletonTab({ find: findExtensionsTab, create: createExtensionsTab })
  }

  function openMapTab() {
    openSingletonTab({ find: findMapTab, create: createMapTab })
  }

  /** Open (or focus) a per-extension detail tab. Non-singleton: one tab per id. */
  function openExtensionDetailTab(ext) {
    const existing = findExtensionDetailTab(tabs, ext.id)
    if (existing) { void activateTab(existing.id); return }
    saveActiveTabState()
    dropWelcomeTabs()
    const tab = createExtensionDetailTab(ext.id, ext.name)
    tabs = [...tabs, tab]
    activeTabId = tab.id
    clearTableEditor()
  }

  /** License activation page. Deliberately NOT via openSingletonTab - that
   *  helper is pro-gated, and this tab exists precisely for non-Pro users. */
  function openLicenseTab() {
    const existing = findLicenseTab(tabs)
    if (existing) { void activateTab(existing.id); return }
    saveActiveTabState()
    dropWelcomeTabs()
    const tab = createLicenseTab()
    tabs = [...tabs, tab]
    activeTabId = tab.id
    clearTableEditor()
  }

  /** Close the license tab after a successful activation. */
  function closeLicenseTab() {
    const existing = findLicenseTab(tabs)
    if (existing) void closeTab(existing.id)
  }

  function openJsonTab() {
    openSingletonTab({ find: findJsonTab, create: createJsonTab })
  }

  function openChartsTab() {
    openSingletonTab({ find: findChartsTab, create: createChartsTab })
  }

  function openDashboardTab() {
    openSingletonTab({ find: findDashboardTab, create: createDashboardTab })
  }

  /** @param {string} [focusTable] Scope the ERD to one table + its FK neighbors. */
  function openErdTab(focusTable = '') {
    erdFocusTable = typeof focusTable === 'string' ? focusTable : ''
    openSingletonTab({ find: findErdTab, create: createErdTab })
  }

  function openDiagramsTab() {
    if (!$hasPro) { showProGate = true; return }
    const existing = findDiagramsTab(tabs)
    if (existing) { activateTab(existing.id); return }
    const tab = createDiagramsTab()
    tabs = [...tabs, tab]
    activeTabId = tab.id
  }

  function openSearchTab() {
    openSingletonTab({ find: findSearchTab, create: createSearchTab })
  }

  function openNewNotebookTab() {
    if (!$hasPro) { showProGate = true; return }
    saveActiveTabState()
    dropWelcomeTabs()
    const nb = createNotebook()
    const tab = createNotebookTab(nb, null)
    tabs = [...tabs, tab]
    activeTabId = tab.id
    clearTableEditor()
  }

  async function openNotebookFromFile() {
    if (!$hasPro) { showProGate = true; return }
    try {
      const result = await openNotebookFile()
      if (!result) return
      const nb = deserializeNotebook(result.content)
      saveActiveTabState()
      dropWelcomeTabs()
      const tab = createNotebookTab(nb, result.path)
      tabs = [...tabs, tab]
      activeTabId = tab.id
      clearTableEditor()
    } catch (err) {
      toast.error(`Failed to open notebook: ${/** @type {Error} */ (err).message}`)
    }
  }

  function openSchemaTimelineTab() {
    openSingletonTab({ find: findSchemaTimelineTab, create: createSchemaTimelineTab })
  }

  function openDataDiffTab() {
    openSingletonTab({ find: findDataDiffTab, create: createDataDiffTab })
  }

  /**
   * Update the state of a notebook tab (called by NotebookEditor via onupdate).
   * @param {string} tabId
   * @param {{ notebook?: import('$lib/notebook.js').Notebook, filePath?: string | null, dirty?: boolean, title?: string }} updates
   */
  function updateNotebookTab(tabId, updates) {
    tabs = tabs.map((t) => {
      if (t.id !== tabId || t.kind !== 'notebook') return t
      const next = { ...t }
      if (updates.title) next.title = updates.title
      next.state = { .../** @type {any} */ (t.state) }
      if (updates.notebook !== undefined) next.state.notebook = updates.notebook
      if (updates.filePath !== undefined) next.state.filePath = updates.filePath
      if (updates.dirty !== undefined) next.state.dirty = updates.dirty
      return next
    })
  }

  /** @param {{ sql: string, mode: string }} detail */
  async function runOrm(detail) {
    if (!connection || !detail.sql.trim()) return
    ormLoading = true
    ormError = ''
    ormColumns = []
    ormRows = []
    try {
      const data = await executeSql(detail.sql)
      ormColumns = data.columns ?? []
      ormRows = data.rows ?? []
      ormQueryMs = data.queryMs ?? data.query_ms ?? 0
    } catch (e) {
      ormError = String(e)
    } finally {
      ormLoading = false
    }
  }

  /** @param {string} id */
  // Background table tabs each pin their full result set. Keep the few most
  // recently viewed ones warm (instant switch) but evict the row arrays of
  // colder, LARGE tabs so N open million-row tables don't retain N× the memory.
  // Eviction just clears rows/columns; switching back refetches (applyTabToEditor
  // treats columns.length === 0 as "no cached data → fetch"). Prefetch only runs
  // on initial open, so this never fights it. Small tabs are left untouched.
  const TAB_ROWS_MRU_MAX = 3
  const TAB_EVICT_ROW_THRESHOLD = 5_000
  // Row count alone is the wrong unit. A table with a pgvector column decodes to
  // ~17KB per row, so 500 rows of embeddings cost more than 5,000 rows of
  // integers and were never evicted however cold the tab got - three of those
  // parked in the background is a quarter of a gigabyte held for tabs nobody is
  // looking at. Size is what matters, so size is what gets measured.
  const TAB_EVICT_BYTE_THRESHOLD = 8 * 1024 * 1024
  const PAYLOAD_SAMPLE_ROWS = 8

  /**
   * Rough byte cost of a result set, from a sample rather than the whole thing:
   * stringifying a million rows to decide whether to drop them would cost more
   * than keeping them. Rows are uniform enough that a handful extrapolates well,
   * and the decision only needs the right order of magnitude.
   * @param {unknown[][] | undefined} rowsArr
   */
  function estimateRowsBytes(rowsArr) {
    if (!Array.isArray(rowsArr) || rowsArr.length === 0) return 0
    const step = Math.max(1, Math.floor(rowsArr.length / PAYLOAD_SAMPLE_ROWS))
    let sampled = 0
    let bytes = 0
    for (let i = 0; i < rowsArr.length && sampled < PAYLOAD_SAMPLE_ROWS; i += step) {
      const row = rowsArr[i]
      if (!row) continue // windowed sets are sparse
      try {
        bytes += JSON.stringify(row)?.length ?? 0
      } catch {
        bytes += 256 // circular or otherwise unserialisable: assume small
      }
      sampled += 1
    }
    if (sampled === 0) return 0
    return Math.round((bytes / sampled) * rowsArr.length)
  }
  let _tabRowsMru = /** @type {string[]} */ ([])
  function evictColdTabRows(activeId) {
    // Trim to the window we actually read - entries past it are never consulted,
    // so without the slice the MRU grows by one id per distinct tab ever opened.
    _tabRowsMru = [..._tabRowsMru.filter((x) => x !== activeId), activeId].slice(-TAB_ROWS_MRU_MAX)
    const keep = new Set(_tabRowsMru)
    // Never evict a tab that is the active tab of a visible split pane - its rows
    // are on screen in that pane's snapshot, so blanking them would flip the pane
    // to the empty "Focus this pane to load" placeholder.
    if (paneRoot) for (const g of PaneTree.allGroups(paneRoot)) if (g.activeTabId) keep.add(g.activeTabId)
    /** @param {StudioTab} t */
    const evictable = (t) => {
      if (t.kind !== 'table' || t.id === activeId || keep.has(t.id)) return false
      const st = /** @type {TableTabState} */ (t.state)
      if (!st) return false
      // Windowed tabs keep their (huge, sparse) array outside the reactive tree,
      // so `st.rows` is empty and has to be measured from the cache instead, or
      // an open million-row table would be retained for the whole session
      // however cold it got.
      const cached = _liveRowsByTab.get(t.id)
      const rowsArr = Array.isArray(st.rows) && st.rows.length > 0 ? st.rows : cached
      if (!Array.isArray(rowsArr) || rowsArr.length === 0) return false
      if (rowsArr.length > TAB_EVICT_ROW_THRESHOLD) return true
      // Cheap enough to run on every switch: a sample of eight rows, and only
      // for tabs that survived the row-count test.
      return estimateRowsBytes(rowsArr) > TAB_EVICT_BYTE_THRESHOLD
    }
    // Most switches evict nothing. Decided in ONE pass so the common path does
    // not rebuild the tabs array - that write invalidates every consumer of
    // `tabs` (tab strip, panes, tabsById) for no change at all.
    //
    // The verdicts are collected rather than recomputed: this used to run
    // `tabs.some(evictable)` and then `evictable(t)` again for every tab inside
    // the map, so the row sampling and its `JSON.stringify` were paid twice on
    // any switch that evicted anything.
    /** @type {Set<string>} */
    const doomed = new Set()
    for (const t of tabs) if (evictable(t)) doomed.add(t.id)
    if (doomed.size === 0) return
    tabs = tabs.map((t) => {
      if (!doomed.has(t.id)) return t
      _liveRowsByTab.delete(t.id)
      const st = /** @type {TableTabState} */ (t.state)
      // windowedHead cleared too: with the array gone the tab has to refetch, and
      // the restore path must not look for a cache that no longer exists.
      return { ...t, state: { ...st, rows: [], columns: [], selected: new Set(), windowedHead: false, windowedLoaded: [], windowCount: 0, loadingRows: false } }
    })
  }

  async function activateTab(id) {
    if (id === activeTabId) return
    saveActiveTabState()
    activeTabId = id
    evictColdTabRows(id)
    const tab = tabs.find((t) => t.id === id)
    if (tab) await applyTabToEditor(tab)
  }

  /**
   * Guard: closing a tab discards its unsaved edits/deletes - ask first when it
   * has staged changes. Returns false when the close should be aborted.
   * @param {StudioTab} closing
   */
  async function confirmDiscardTabChanges(closing) {
    const count = tabPendingCount(closing)
    if (count === 0) return true
    const ok = await askConfirm(
      `This table has ${count} unsaved change${count === 1 ? '' : 's'}. Close the tab and discard them?`,
      'Close & discard',
    )
    if (!ok) return false
    if (closing.id === activeTabId) resetEdits()
    const key = tabTableKey(closing)
    if (key) clearPendingChanges(key)
    return true
  }

  /** @param {string} id */
  async function closeTab(id) {
    const idx = tabs.findIndex((t) => t.id === id)
    if (idx < 0) return
    const closing = tabs[idx]
    if (!(await confirmDiscardTabChanges(closing))) return
    rememberClosedTab(closing)
    const nextTabs = tabs.filter((t) => t.id !== id)
    if (nextTabs.length === 0) {
      tabs = [createWelcomeTab()]
      activeTabId = tabs[0].id
      clearTableEditor()
      return
    }
    tabs = nextTabs
    if (activeTabId === id) {
      const nextIdx = Math.min(idx, nextTabs.length - 1)
      await activateTab(nextTabs[nextIdx].id)
    }
  }

  /** Push a closed tab onto the reopen stack (welcome tabs aren't worth restoring). */
  function rememberClosedTab(tab) {
    if (!tab || tab.kind === 'welcome') return
    _liveRowsByTab.delete(tab.id)
    forgetSqlResult(tab.id)
    // Anything still in flight for this tab is now orphaned: dropping its load
    // token makes the in-flight result fail its own liveness check instead of
    // being applied to whatever tab took its place.
    _loadSeqByTab.delete(tab.id)
    fetchingTabIds.delete(tab.id)
    _sqlQueryIdByTab.delete(tab.id)
    if (_autoRefreshByTab.delete(tab.id)) _autoRefreshTick += 1
    clearBusy(tab.id)
    // Snapshot with a shallow state clone so later edits to the live tree can't
    // mutate what we'll restore. `id`/`pinned` are dropped - reopen mints fresh.
    const { id: _id, pinned: _pinned, ...rest } = tab
    const state = tab.state ? { ...tab.state } : tab.state
    // Drop the bulky payloads - a closed 1M-row tab must not pin its whole result
    // set in memory (20 stacked closed tabs could otherwise retain gigabytes).
    // Reopen restores the query/filters/sort and refetches: applyTabToEditor
    // already treats columns.length === 0 as "no cached data → fetch".
    if (state && tab.kind === 'table') {
      state.rows = []
      state.columns = []
      state.selected = new Set()
      state.loadingRows = false
      state.windowedHead = false
      state.windowedLoaded = []
      state.windowCount = 0
    } else if (state && tab.kind === 'sql') {
      state.sqlRows = []
      state.sqlColumns = []
      state.sqlLoading = false
    }
    const snapshot = { ...rest, state }
    closedTabStack = [...closedTabStack, snapshot].slice(-CLOSED_TAB_STACK_MAX)
  }

  /** Ctrl/⌘+Shift+T - reopen the most recently closed tab. */
  function reopenLastClosedTab() {
    const entry = closedTabStack[closedTabStack.length - 1]
    if (!entry) return
    closedTabStack = closedTabStack.slice(0, -1)
    saveActiveTabState()
    dropWelcomeTabs()
    const tab = { ...entry, id: crypto.randomUUID(), pinned: false, state: entry.state ? { ...entry.state } : entry.state }
    tabs = [...tabs, tab]
    void activateTab(tab.id)
  }

  /** @param {string} id - keep this tab (and pinned tabs), close everything else */
  async function closeOtherTabs(id) {
    const keep = tabs.find((t) => t.id === id)
    if (!keep) return
    for (const t of tabs) if (t.id !== id && !t.pinned) rememberClosedTab(t)
    tabs = tabs.filter((t) => t.id === id || t.pinned)
    await activateTab(keep.id)
  }

  async function closeAllTabs() {
    const pinned = tabs.filter((t) => t.pinned)
    for (const t of tabs) if (!t.pinned) rememberClosedTab(t)
    if (pinned.length === 0) {
      tabs = [createWelcomeTab()]
      activeTabId = tabs[0].id
      clearTableEditor()
      return
    }
    tabs = pinned
    if (!pinned.some((t) => t.id === activeTabId)) await activateTab(pinned[0].id)
  }

  /**
   * Close a batch of tabs at once ("Close Tabs to Left/Right"). Pinned tabs are
   * skipped; a single confirm covers all unsaved changes in the batch.
   * @param {string[]} ids @param {string} anchorId - the tab whose menu was used
   */
  async function closeManyTabs(ids, anchorId) {
    saveActiveTabState()
    const idSet = new Set(ids)
    const toClose = tabs.filter((t) => idSet.has(t.id) && !t.pinned)
    if (toClose.length === 0) return
    const pending = toClose.reduce((n, t) => n + tabPendingCount(t), 0)
    if (pending > 0) {
      const ok = await askConfirm(
        `Closing these tabs discards ${pending} unsaved change${pending === 1 ? '' : 's'}. Close and discard?`,
        'Close & discard',
      )
      if (!ok) return
      for (const t of toClose) {
        const key = tabTableKey(t)
        if (key) clearPendingChanges(key)
      }
    }
    const closeSet = new Set(toClose.map((t) => t.id))
    for (const t of toClose) rememberClosedTab(t)
    const wasActiveClosed = activeTabId !== null && closeSet.has(activeTabId)
    tabs = tabs.filter((t) => !closeSet.has(t.id))
    if (wasActiveClosed) await activateTab(anchorId)
  }

  /**
   * Duplicate a table/SQL tab - a fresh tab with a deep-enough copy of the
   * source state, inserted right after the original.
   * @param {string} id
   */
  async function duplicateTab(id) {
    const src = tabs.find((t) => t.id === id)
    if (!src || (src.kind !== 'table' && src.kind !== 'sql')) return
    // Freshen the snapshot first when duplicating the live tab.
    if (id === activeTabId) saveActiveTabState()
    const fresh = tabs.find((t) => t.id === id)
    if (!fresh?.state) return
    const state =
      fresh.kind === 'table'
        ? cloneTableTabState(/** @type {TableTabState} */ (fresh.state))
        : cloneSqlTabState(/** @type {SqlTabState} */ (fresh.state))
    // The copy owns no in-flight work, so it must not inherit the original's
    // loading flags - nothing would ever clear them on the duplicate.
    if (state) {
      if (fresh.kind === 'table') /** @type {any} */ (state).loadingRows = false
      else /** @type {any} */ (state).sqlLoading = false
    }
    const copy = { id: crypto.randomUUID(), kind: fresh.kind, title: fresh.title, state }
    const idx = tabs.findIndex((t) => t.id === id)
    tabs = [...tabs.slice(0, idx + 1), copy, ...tabs.slice(idx + 1)]
    await activateTab(copy.id)
  }

  // ── Pane-layout reconciliation & group-aware actions ─────────────────────
  // AI tabs live in a full-window overlay, not the tab flow, so they never
  // belong to a group.
  /** @param {StudioTab} t */
  function isGroupable(t) {
    return t.kind !== 'ai'
  }

  /**
   * Keep `paneRoot` / `activeGroupId` consistent with `tabs` + `activeTabId`.
   * Runs from an effect on every tab/active change. Idempotent: it only assigns
   * new references when something actually changed, so it can't loop.
   */
  function reconcilePanes() {
    const ids = tabs.filter(isGroupable).map((t) => t.id)
    const idSet = new Set(ids)
    const root = untrack(() => paneRoot)
    const curGroup = untrack(() => activeGroupId)

    if (ids.length === 0) {
      if (root !== null) paneRoot = null
      if (curGroup !== null) activeGroupId = null
      return
    }

    let next = root
    if (!next) {
      next = PaneTree.makeGroup(ids, activeTabId)
    } else {
      // Drop tab ids that no longer exist and repair each group's active tab.
      next = PaneTree.mapGroups(next, (g) => {
        const kept = g.tabIds.filter((id) => idSet.has(id))
        let active = g.activeTabId
        if (!active || !idSet.has(active)) active = kept[kept.length - 1] ?? null
        if (kept.length === g.tabIds.length && active === g.activeTabId) return g
        return { ...g, tabIds: kept, activeTabId: active }
      })
      next = PaneTree.prune(next)
    }

    // Place brand-new tabs (not yet in any group) into the active group.
    const placed = new Set()
    for (const g of PaneTree.allGroups(next)) for (const id of g.tabIds) placed.add(id)
    const missing = ids.filter((id) => !placed.has(id))
    if (missing.length) {
      let targetId =
        curGroup && PaneTree.findGroup(next, curGroup) ? curGroup : PaneTree.firstGroup(next)?.id ?? null
      if (!targetId || !next) {
        next = PaneTree.makeGroup(missing, activeTabId)
      } else {
        for (const id of missing) next = PaneTree.addTabToGroup(next, targetId, id)
      }
    }

    // Focus follows the global active tab: its holding group becomes active and
    // mirrors `activeTabId`.
    let nextGroup = curGroup
    if (activeTabId && idSet.has(activeTabId)) {
      const holder = PaneTree.groupOfTab(next, activeTabId)
      if (holder) {
        nextGroup = holder.id
        if (holder.activeTabId !== activeTabId) {
          next = PaneTree.updateGroup(next, holder.id, (g) => ({ ...g, activeTabId }))
        }
      }
    }
    if (!nextGroup || !PaneTree.findGroup(next, nextGroup)) {
      nextGroup = PaneTree.firstGroup(next)?.id ?? null
    }

    if (next !== root) paneRoot = next
    if (nextGroup !== curGroup) activeGroupId = nextGroup
  }

  $effect(() => {
    // Depend on tab identity/order + the active tab.
    tabs.map((t) => t.id).join('|')
    void activeTabId
    reconcilePanes()
  })

  /** Focus a pane (its active tab drives the live pipeline). @param {string} groupId */
  async function focusGroup(groupId) {
    if (groupId === activeGroupId) return
    const g = PaneTree.findGroup(paneRoot, groupId)
    if (!g) return
    activeGroupId = groupId
    if (g.activeTabId && g.activeTabId !== activeTabId) await activateTab(g.activeTabId)
  }

  /** Activate a tab within a specific pane (pane tab-strip click). */
  async function focusTabInGroup(groupId, tabId) {
    activeGroupId = groupId
    paneRoot = PaneTree.updateGroup(paneRoot, groupId, (g) => ({ ...g, activeTabId: tabId }))
    await activateTab(tabId)
  }

  /** Close a tab from a specific pane, keeping focus within that pane. */
  async function closeTabInGroup(groupId, id) {
    const g = PaneTree.findGroup(paneRoot, groupId)
    const idx = tabs.findIndex((t) => t.id === id)
    if (idx < 0) return
    const closing = tabs[idx]
    if (!(await confirmDiscardTabChanges(closing))) return
    // Remember it so Reopen Closed Tab (⌘⇧T) can restore it - mirrors closeTab().
    rememberClosedTab(closing)
    const nextTabs = tabs.filter((t) => t.id !== id)
    if (nextTabs.length === 0) {
      tabs = [createWelcomeTab()]
      activeTabId = tabs[0].id
      clearTableEditor()
      return
    }
    // Pick the next tab to activate *within this group* when closing its active.
    let nextActiveId = /** @type {string | null} */ (null)
    if (id === activeTabId && g) {
      const gi = g.tabIds.indexOf(id)
      const rest = g.tabIds.filter((t) => t !== id)
      if (rest.length) {
        nextActiveId = rest[Math.min(gi, rest.length - 1)]
      } else {
        const other = PaneTree.allGroups(paneRoot).find(
          (gr) => gr.id !== groupId && gr.activeTabId && gr.activeTabId !== id,
        )
        if (other) {
          activeGroupId = other.id
          nextActiveId = other.activeTabId
        }
      }
    }
    tabs = nextTabs
    if (id === activeTabId) {
      if (nextActiveId) await activateTab(nextActiveId)
      else {
        const fallback = nextTabs[Math.min(idx, nextTabs.length - 1)]
        if (fallback) await activateTab(fallback.id)
      }
    }
  }

  /** Close the focused pane's active tab (Mod+W / editor shortcuts). */
  function closeActiveTab() {
    if (!activeTabId) return
    if (activeGroupId) void closeTabInGroup(activeGroupId, activeTabId)
    else void closeTab(activeTabId)
  }

  /** Resize a split node (splitter drag). */
  function handlePaneResize(splitId, sizes) {
    paneRoot = PaneTree.setSizes(paneRoot, splitId, sizes)
  }

  /**
   * Handle a tab dropped on a pane drop-zone: center = move into that group,
   * an edge = split that group and place the tab in the new pane.
   * @param {string} targetGroupId
   * @param {import('$lib/pane-layout.js').DropEdge} edge
   */
  function handleDropZone(targetGroupId, edge) {
    const tabId = dragTabId
    dragTabId = null
    if (!tabId || !paneRoot) return
    const source = PaneTree.groupOfTab(paneRoot, tabId)
    if (!source) return

    if (edge === 'center') {
      if (source.id === targetGroupId) {
        void focusTabInGroup(targetGroupId, tabId)
        return
      }
      let next = PaneTree.removeTab(paneRoot, tabId)
      next = PaneTree.addTabToGroup(next, targetGroupId, tabId)
      paneRoot = PaneTree.prune(next) ?? next
      void focusTabInGroup(targetGroupId, tabId)
      return
    }

    // Splitting the source into itself with only one tab is a no-op.
    if (source.id === targetGroupId && source.tabIds.length <= 1) return
    const newGroup = PaneTree.makeGroup([tabId], tabId)
    let next = PaneTree.removeTab(paneRoot, tabId)
    next = PaneTree.splitGroup(next, targetGroupId, edge, newGroup)
    paneRoot = PaneTree.prune(next) ?? next
    activeGroupId = newGroup.id
    void activateTab(tabId)
  }

  /**
   * "View data structure" from the sidebar - open the table's tab and switch
   * straight to the structure view (the structure auto-load effect fetches it).
   * @param {string} table
   */
  async function openTableStructure(table) {
    if (aiMode) exitAiMode()
    await openTableTab(activeSchema, table)
    tableViewMode = 'structure'
  }

  /** @param {string} id - pin/unpin a tab; pinned tabs group at the front */
  function toggleTabPin(id) {
    const tab = tabs.find((t) => t.id === id)
    if (!tab) return
    tab.pinned = !tab.pinned
    tabs = [...tabs.filter((t) => t.pinned), ...tabs.filter((t) => !t.pinned)]
  }

  /**
   * @param {string} schema
   * @param {string} table
   * @param {{ filters?: TableFilter[], resetQuery?: boolean, search?: string|null,
   *   duplicate?: boolean, viewMode?: string }} [options]
   */
  async function openTableTab(schema, table, options = {}) {
    track('table_open')
    const { filters = null, resetQuery = false, search = null, duplicate = false, viewMode = null, expandSingleRow: openSingleRow = false } = options
    // `duplicate` forces a second tab for a table that is already open - used when
    // the request comes from inside that table's own tab (e.g. "Open table" in the
    // ERD inspector), where re-activating the existing tab would be a no-op.
    const existing = duplicate ? null : findTableTab(tabs, schema, table)
    if (existing) {
      // No view reset here. These two lines used to force the grid on, because
      // `tableViewMode` was shared by every tab and re-activating one would
      // otherwise inherit the last tab's mode. It is part of the tab's snapshot
      // now, so clicking a table that is already open in the structure editor
      // returns you to the structure editor - which is what "the tab kept my
      // place" means everywhere else in this app.
      await activateTab(existing.id)
      if (filters || search !== null) {
        if (resetQuery) {
          rowSearch = ''
          rowSort = null; rowSortMore = []
        }
        if (search !== null) rowSearch = search
        if (filters) {
          rowFilters = filters.map((f) => ({ ...f }))
          filterBarOpen = filters.length > 0
        }
        page = 1
        expandSingleRow = openSingleRow
        // This query supersedes whatever `activateTab` may have started for the
        // tab: the filters just changed, so that in-flight page is the wrong
        // one. Waiting for it first keeps the two from writing out of order.
        await _tabFetches.get(existing.id)
        await loadRows()
      } else if (
        activeTable === table &&
        columns.length === 0 &&
        // …and nothing is already fetching this tab's rows. `activateTab` above
        // ends in `applyTabToEditor`, which starts that fetch for a tab whose
        // rows were evicted (or never cached) and deliberately does not await
        // it. `columns` is therefore still empty here, which this branch read as
        // "nothing loaded" - so clicking an already-open table fired the same
        // full-page query twice, decoded both and kept whichever landed last.
        !fetchingTabIds.has(existing.id) &&
        !_tabFetches.has(existing.id)
      ) {
        await loadRows()
      }
      return
    }
    saveActiveTabState()
    dropWelcomeTabs()
    const tableKind = tables.find((t) => t.name === table)?.kind ?? 'table'
    if (persistConnectionId) {
      pushRecentTab(persistConnectionId, { schema, table, tableKind: /** @type {any} */ (tableKind) })
      refreshRecentTabs()
    }
    const tab = createTableTab(schema, table, /** @type {any} */ (tableKind))
    // Pre-bake any filters/search into the tab state before fetching
    if (tab.state) {
      if (filters) /** @type {TableTabState} */ (tab.state).rowFilters = filters.map((f) => ({ ...f }))
      if (search !== null) /** @type {TableTabState} */ (tab.state).rowSearch = search
      if (openSingleRow) /** @type {TableTabState} */ (tab.state).expandSingleRow = true
    }
    tabs = [...tabs, tab]
    activeTabId = tab.id
    activeTable = table
    tableViewMode = 'data'
    structureColumns = []
    structureSearch = ''
    page = 1
    pageSize = loadDefaultPageSize()
    rowSearch = search ?? ''
    rowSort = null
    rowSortMore = []
    rowFilters = filters ? filters.map((f) => ({ ...f })) : []
    filterBarOpen = filters ? filters.length > 0 : false
    columns = []
    primaryKey = []
    foreignKeys = []
    rows = []
    total = 0
    queryMs = 0
    error = ''
    selected = new Set()
    focusedRow = null
    focusedCol = null
    inspectorRow = null
    editingCell = null
    expandSingleRow = openSingleRow
    // A fresh tab opens in the configured default view, never in whatever view the
    // tab you came from happened to be on - the three-dot picker is per tab. Read
    // the setting here (not at startup) so changing it takes effect immediately.
    dataViewMode = /** @type {any} */ (viewMode ?? loadSettings().defaultDataView)
    hiddenColumns = loadHiddenCols(persistConnectionId, schema, table)
    if (schema !== activeSchema) {
      activeSchema = schema
      setLastSchema(persistConnectionId, schema)
      await loadTables()
    }
    // Fire the fetch in background - caller can open more tabs without waiting
    void startTabFetch(tab.id)
    // Load reverse FKs once per table (cached - not re-fetched on page/sort changes)
    void loadIncomingForeignKeys(schema, table)
  }

  /** Cap on the incoming-FK cache so long sessions across many tables don't grow it unbounded. */
  const INCOMING_FK_CACHE_MAX = 120
  /** @param {string} key @param {any[]} value */
  function setIncomingFkCache(key, value) {
    const next = new Map(incomingFkCache)
    next.set(key, value)
    // FIFO eviction - Map preserves insertion order, so drop the oldest entries.
    while (next.size > INCOMING_FK_CACHE_MAX) {
      const oldest = next.keys().next().value
      if (oldest === undefined) break
      next.delete(oldest)
    }
    incomingFkCache = next
  }

  /**
   * Requests already in flight, keyed the same way as the cache.
   *
   * The cache is only populated once the request comes back, so the `has(key)`
   * guard below could not see a request that had been fired but not yet
   * answered. Restoring a session therefore fired one reverse-FK query per
   * restored tab at the same instant - measured at nine concurrent calls
   * totalling 37s of backend time against a D1 database, where each call is
   * several HTTP round trips to Cloudflare. Everything the user was actually
   * waiting for queued behind them.
   * @type {Map<string, Promise<void>>}
   */
  const _incomingFkInFlight = new Map()
  /** Bumped whenever the cache is dropped, so a reply from before the drop is discarded. */
  let _incomingFkGen = 0

  /** Load incoming FKs for a table into the cache (no-op if cached or in flight). */
  async function loadIncomingForeignKeys(schema, table) {
    const key = `${schema}.${table}`
    if (incomingFkCache.has(key)) return
    const pending = _incomingFkInFlight.get(key)
    if (pending) return pending
    const gen = _incomingFkGen
    const p = (async () => {
      /** @type {any[]} */
      let result = []
      try {
        result = (await getIncomingForeignKeys(schema, table)) ?? []
      } catch {
        result = []
      }
      // The connection or schema may have changed while this was in flight; the
      // cache is keyed by "schema.table" alone, so writing now would serve one
      // database's reverse FKs for a same-named table in another.
      if (gen !== _incomingFkGen) return
      _incomingFkInFlight.delete(key)
      setIncomingFkCache(key, result)
    })()
    _incomingFkInFlight.set(key, p)
    return p
  }

  /** Drop the reverse-FK cache and abandon anything still in flight for it. */
  function resetIncomingFkCache() {
    incomingFkCache = new Map()
    _incomingFkInFlight.clear()
    _incomingFkGen++
  }

  /**
   * Fetch related rows for an FK sub-view panel.
   * kind='forward': follow this table's FK to the referenced table.
   * kind='reverse': query a referencing table filtered to this row.
   */
  async function handleFetchRelatedRows(detail) {
    try {
      if (detail.kind === 'forward') {
        const filters = buildForeignKeyFilters(detail.fk, columns, detail.row)
        if (!filters) return { columns: [], rows: [], error: 'FK value is NULL' }
        const refSchema = detail.fk.referencedSchema || detail.fk.referenced_schema || activeSchema
        const refTable = detail.fk.referencedTable  || detail.fk.referenced_table  || ''
        if (!refTable) return { columns: [], rows: [], error: 'No referenced table' }
        const data = await getTableRows(refSchema, refTable, 50, 0, { filters: filtersForApi(filters) })
        return { columns: data.columns ?? [], rows: data.rows ?? [] }
      } else {
        const revFilters = buildReverseForeignKeyFilters(detail, columns, detail.row)
        if (!revFilters) return { columns: [], rows: [], error: 'Cannot build filter, value may be NULL' }
        const fromSchema = detail.fromSchema || activeSchema
        const data = await getTableRows(fromSchema, detail.fromTable, 50, 0, { filters: filtersForApi(revFilters) })
        return { columns: data.columns ?? [], rows: data.rows ?? [] }
      }
    } catch (e) {
      return { columns: [], rows: [], error: String(e) }
    }
  }

  /**
   * @param {{ rowIdx: number, colIdx: number, reverseRel?: any, row?: unknown[], newTab?: boolean }} detail
   *   `newTab` forces a second tab even when the target table is already open,
   *   which is what Shift+Enter in the grid asks for.
   */
  async function handleFollowForeignKey({ rowIdx, colIdx, reverseRel, row: detailRow, newTab = false }) {
    // Reverse FK: navigate to the referencing table with the correct filter
    if (reverseRel?.fromTable) {
      const row = detailRow ?? rows[rowIdx]
      if (!row) return
      const fromSchema = reverseRel.fromSchema || activeSchema
      const revFilters = buildReverseForeignKeyFilters(reverseRel, columns, row)
      if (!revFilters) {
        toast.error('Cannot open reference', { description: 'Could not build filter.' })
        return
      }
      await openTableTab(fromSchema, reverseRel.fromTable, { filters: filtersForApi(revFilters), resetQuery: true, duplicate: newTab })
      return
    }
    // Forward FK: standard navigation
    const col = columns[colIdx]
    if (!col) return
    const fk = findForeignKeyForColumn(foreignKeys, col.name)
    if (!fk) return
    const row = rows[rowIdx]
    if (!row) return
    const filters = buildForeignKeyFilters(fk, columns, row)
    if (!filters) {
      toast.error('Cannot open reference', {
        description: 'Foreign key value is NULL or incomplete.',
      })
      return
    }
    const refSchema = fk.referencedSchema || activeSchema
    await openTableTab(refSchema, fk.referencedTable, { filters, resetQuery: true, duplicate: newTab, expandSingleRow: true })
  }

  /**
   * True when the reply to a request issued while `conn` was connected belongs
   * to a database that is no longer the one on screen.
   *
   * Every catalog loader needs this, and the `activeSchema === schemaAtCall`
   * check some of them already do is NOT a substitute: clearConnectionState()
   * resets activeSchema to 'public', so switching between two PostgreSQL
   * databases leaves that comparison true and a late reply from the old database
   * is applied to the new one - wrong table list, wrong catalog, and a cache
   * entry written under whichever connection id happened to be current. The
   * window is invisible over a local socket and wide open on a slow link or a
   * slower machine, which is exactly where it gets reported from.
   *
   * Capture `persistConnectionId` before the first await, compare after.
   * @param {string} conn
   */
  function connectionMoved(conn) {
    return persistConnectionId !== conn
  }

  async function loadSchemas() {
    const connAtCall = persistConnectionId
    const list = await listSchemas()
    if (connectionMoved(connAtCall)) return
    schemas = list
    if (list.length === 0) {
      activeSchema = ''
      return
    }
    if (!list.includes(activeSchema)) {
      activeSchema = list.includes('public') ? 'public' : list[0]
    }
  }

  /** @param {string} schema */
  async function fetchIndexes(schema) {
    if (!schema) return []
    try {
      const list = await listIndexes(schema)
      return list
        .map((i) => ({
          name: i.name ?? '',
          tableName: i.tableName ?? i.table_name ?? '',
          columns: i.columns ?? '',
          indexType: i.indexType ?? i.index_type ?? 'btree',
          isUnique: i.isUnique ?? i.is_unique ?? false,
          isPrimary: i.isPrimary ?? i.is_primary ?? false,
          condition: i.condition ?? null,
          comment: i.comment ?? null,
        }))
        .filter((i) => i.name)
    } catch {
      return []
    }
  }

  async function loadStructure() {
    if (!activeSchema || !activeTable) { structureColumns = []; return }
    loadingStructure = true
    const mySeq = ++_structureSeq
    const targetSchema = activeSchema
    const targetTable = activeTable
    const connAtCall = persistConnectionId
    const driver = dbType  // 'postgres' | 'mysql' | 'sqlite' | 'd1'
    try {
      const s = targetSchema.replace(/'/g, "''")
      const t = targetTable.replace(/'/g, "''")
      let rows = /** @type {unknown[][]} */ ([])

      // ── PostgreSQL / Supabase ─────────────────────────────────────────
      // pg_catalog is 10-100× faster than information_schema on hosted PG.
      if (driver === 'postgres') {
        const r = await executeSql(`
          SELECT
            a.attnum::int,
            a.attname,
            CASE
              WHEN t.typtype = 'b' AND t.typelem <> 0 AND t.typname LIKE '\\_%'
                THEN (SELECT bt.typname FROM pg_catalog.pg_type bt WHERE bt.oid = t.typelem) || '[]'
              WHEN a.atttypmod > 0 AND t.typname IN ('varchar','bpchar')
                THEN t.typname || '(' || (a.atttypmod - 4)::text || ')'
              WHEN a.atttypmod > 0 AND t.typname = 'numeric' AND a.atttypmod <> -1
                THEN 'numeric(' || (((a.atttypmod - 4) >> 16) & 65535)::text
                  || ',' || ((a.atttypmod - 4) & 65535)::text || ')'
              WHEN a.atttypmod > 0 AND t.typname IN ('bit','varbit')
                THEN t.typname || '(' || a.atttypmod::text || ')'
              ELSE t.typname
            END,
            NOT a.attnotnull,
            pg_get_expr(ad.adbin, ad.adrelid),
            (
              SELECT rn.nspname || '.' || rc.relname || '.' || ra.attname
              FROM pg_catalog.pg_constraint  pc
              JOIN pg_catalog.pg_class        rc ON rc.oid = pc.confrelid
              JOIN pg_catalog.pg_namespace    rn ON rn.oid = rc.relnamespace
              JOIN pg_catalog.pg_attribute    ra ON ra.attrelid = rc.oid AND ra.attnum = pc.confkey[1]
              WHERE pc.contype = 'f' AND pc.conrelid = a.attrelid AND pc.conkey[1] = a.attnum
              LIMIT 1
            ),
            (
              SELECT pc.conname FROM pg_catalog.pg_constraint pc
              WHERE pc.contype = 'f' AND pc.conrelid = a.attrelid AND pc.conkey[1] = a.attnum
              LIMIT 1
            ),
            col_description(a.attrelid, a.attnum)
          FROM pg_catalog.pg_attribute  a
          JOIN pg_catalog.pg_class      c  ON c.oid = a.attrelid
          JOIN pg_catalog.pg_namespace  n  ON n.oid = c.relnamespace
          JOIN pg_catalog.pg_type       t  ON t.oid = a.atttypid
          LEFT JOIN pg_catalog.pg_attrdef ad ON ad.adrelid = a.attrelid AND ad.adnum = a.attnum
          WHERE n.nspname = '${s}' AND c.relname = '${t}'
            AND a.attnum > 0 AND NOT a.attisdropped
          ORDER BY a.attnum
        `)
        rows = r?.rows ?? []

      // ── MySQL ─────────────────────────────────────────────────────────
      } else if (driver === 'mysql') {
        const r = await executeSql(`
          SELECT
            c.ORDINAL_POSITION,
            c.COLUMN_NAME,
            c.COLUMN_TYPE,
            c.IS_NULLABLE = 'YES',
            c.COLUMN_DEFAULT,
            CASE WHEN kcu.REFERENCED_TABLE_NAME IS NOT NULL
              THEN CONCAT(kcu.REFERENCED_TABLE_SCHEMA,'.',kcu.REFERENCED_TABLE_NAME,'.',kcu.REFERENCED_COLUMN_NAME)
              ELSE NULL END,
            kcu.CONSTRAINT_NAME,
            c.COLUMN_COMMENT
          FROM information_schema.COLUMNS c
          LEFT JOIN information_schema.KEY_COLUMN_USAGE kcu
            ON kcu.TABLE_SCHEMA = c.TABLE_SCHEMA AND kcu.TABLE_NAME = c.TABLE_NAME
           AND kcu.COLUMN_NAME = c.COLUMN_NAME AND kcu.REFERENCED_TABLE_NAME IS NOT NULL
          WHERE c.TABLE_SCHEMA = '${s}' AND c.TABLE_NAME = '${t}'
          ORDER BY c.ORDINAL_POSITION
        `)
        rows = r?.rows ?? []

      // ── SQLite / D1 ───────────────────────────────────────────────────
      } else {
        const [colR, fkR] = await Promise.all([
          executeSql(`PRAGMA table_info('${t}')`),
          executeSql(`PRAGMA foreign_key_list('${t}')`),
        ])
        /** @type {Map<string, string>} */
        const fkMap = new Map()
        for (const fkRow of fkR?.rows ?? []) {
          const fromCol = String(fkRow[3] ?? '')
          const toTable = String(fkRow[2] ?? '')
          const toCol = String(fkRow[4] ?? '')
          if (fromCol && !fkMap.has(fromCol)) fkMap.set(fromCol, `${toTable}.${toCol}`)
        }
        rows = (colR?.rows ?? []).map((row) => [
          Number(row[0]) + 1,  // cid → 1-based ordinal
          row[1],              // name
          row[2],              // type
          !(row[3] === 1 || row[3] === '1' || row[3] === true),  // notnull→nullable
          row[4],              // dflt_value
          fkMap.get(String(row[1] ?? '')) ?? null,
          null,
          null,
        ])
      }

      // Table and schema names both survive a connection switch - two databases
      // routinely hold a `public.users` - so the connection has to be checked on
      // its own or one database's columns get shown for the other's table.
      if (activeTable === targetTable && activeSchema === targetSchema && !connectionMoved(connAtCall)) {
        structureColumns = rows.map((row) => ({
          ordinalPosition:  Number(row[0]) || 0,
          name:             String(row[1] ?? ''),
          dataType:         String(row[2] ?? ''),
          isNullable:       row[3] === true || row[3] === 't' || String(row[3]).toLowerCase() === 'true',
          columnDefault:    row[4] != null ? String(row[4]) : null,
          foreignKey:       row[5] != null ? String(row[5]) : null,
          fkConstraintName: row[6] != null ? String(row[6]) : null,
          comment:          row[7] != null ? String(row[7]) : null,
        }))
      }
    } catch (e) {
      if (String(e).includes('Query cancelled')) return
      if (connectionMoved(connAtCall)) return
      toast.error('Could not load table structure', { description: String(e) })
      if (activeTable === targetTable) structureColumns = []
    } finally {
      if (mySeq === _structureSeq) loadingStructure = false
    }
  }

  // Auto-reset structure mode when navigating to a view/materialized_view.
  $effect(() => {
    if (!canShowStructure && tableViewMode === 'structure') {
      tableViewMode = 'data'
      structureColumns = []
    }
  })

  // Auto-load structure when the view is in structure mode, a table is active,
  // and there is an active connection.
  $effect(() => {
    if (connection && tableViewMode === 'structure' && activeTable && canShowStructure) {
      void loadStructure()
    }
  })

  /** @param {string} schema */
  async function fetchEnums(schema) {
    if (!schema || !engineSupports('enums', connection?.type)) return []
    try {
      const list = await listEnums(schema)
      // Dedupe values defensively: enum labels are a set, but a bad introspection
      // join could return repeats, and the schema pages key their {#each} on the
      // value - a duplicate would crash the view (each_key_duplicate).
      return list.map((e) => ({ name: e.name ?? '', values: [...new Set(e.values ?? [])] }))
    } catch {
      return []
    }
  }

  /** @param {string} schema */
  async function fetchTriggers(schema) {
    if (!schema || !engineSupports('triggers', connection?.type)) return []
    try {
      const list = await listTriggers(schema)
      return list.map((t) => ({
        name: t.name ?? '',
        tableName: t.tableName ?? t.table_name ?? '',
        timing: t.timing ?? 'AFTER',
        events: t.events ?? '',
        functionName: t.functionName ?? t.function_name ?? '',
        enabled: t.enabled ?? true,
      })).filter((t) => t.name)
    } catch {
      return []
    }
  }

  /** @param {string} schema */
  async function fetchSequences(schema) {
    if (!schema || !engineSupports('sequences', connection?.type)) return []
    try {
      const list = await listSequences(schema)
      return list.map((s) => ({
        name: s.name ?? '',
        dataType: s.dataType ?? s.data_type ?? 'bigint',
        startValue: s.startValue ?? s.start_value ?? 1,
        minValue: s.minValue ?? s.min_value ?? 1,
        maxValue: s.maxValue ?? s.max_value ?? 9007199254740991,
        increment: s.increment ?? 1,
        cycle: s.cycle ?? false,
        ownedBy: s.ownedBy ?? s.owned_by ?? null,
      })).filter((s) => s.name)
    } catch {
      return []
    }
  }

  // ── Schema-level catalog ──────────────────────────────────────────────────
  // Indexes, enums, triggers and sequences are four round trips that describe a
  // whole schema, so they are fetched, cached and invalidated as one unit. The
  // TTL is long compared with the table list's: none of these change without a
  // DDL statement, and every path that runs DDL invalidates the connection.
  const CATALOG_TTL_MS = 60_000

  /** @typedef {{ indexes: any[], enums: any[], triggers: any[], sequences: any[] }} SchemaCatalog */

  /** Push a catalog payload into the reactive state the pages read. @param {SchemaCatalog} c */
  function applySchemaCatalog(c) {
    indexes = c.indexes
    enums = c.enums
    triggers = c.triggers
    sequences = c.sequences
  }

  /** @param {{ force?: boolean }} [opts] */
  async function loadSchemaCatalog({ force = false } = {}) {
    const schema = activeSchema
    if (!schema) { applySchemaCatalog({ indexes: [], enums: [], triggers: [], sequences: [] }); return }
    const key = catalogKey(persistConnectionId, 'catalog', schema)
    if (force) _catalog.invalidate(key)
    const hit = /** @type {SchemaCatalog | undefined} */ (_catalog.get(key, CATALOG_TTL_MS))
    if (hit) { applySchemaCatalog(hit); return }
    const connAtCall = persistConnectionId
    const [idx, enm, trg, seq] = await Promise.all([
      fetchIndexes(schema), fetchEnums(schema), fetchTriggers(schema), fetchSequences(schema),
    ])
    // The schema can change while four requests are in flight; a late payload
    // must not overwrite the catalog of wherever the user has since gone. The
    // connection can change too, and that check has to be separate - the schema
    // is 'public' on both sides of a Postgres-to-Postgres switch.
    if (connectionMoved(connAtCall)) return
    const payload = { indexes: idx, enums: enm, triggers: trg, sequences: seq }
    _catalog.set(key, payload)
    if (activeSchema === schema) applySchemaCatalog(payload)
  }

  /** Refresh one kind on its own (a page's own refresh button) and keep the
   *  cached payload in step, so the next schema revisit does not undo it.
   *  @param {'indexes' | 'enums' | 'triggers' | 'sequences'} kind */
  async function reloadCatalogKind(kind) {
    const schema = activeSchema
    if (!schema) return
    const connAtCall = persistConnectionId
    const fetched = kind === 'indexes' ? await fetchIndexes(schema)
      : kind === 'enums' ? await fetchEnums(schema)
      : kind === 'triggers' ? await fetchTriggers(schema)
      : await fetchSequences(schema)
    if (activeSchema !== schema || connectionMoved(connAtCall)) return
    if (kind === 'indexes') indexes = fetched
    else if (kind === 'enums') enums = fetched
    else if (kind === 'triggers') triggers = fetched
    else sequences = fetched
    const key = catalogKey(persistConnectionId, 'catalog', schema)
    const cached = /** @type {SchemaCatalog | undefined} */ (_catalog.get(key, CATALOG_TTL_MS))
    if (cached) _catalog.set(key, { ...cached, [kind]: fetched })
  }

  // Table-list cache keyed by connection+schema. Rapid navigation (switching
  // between tabs in different schemas, reopening the sidebar) used to re-run the
  // full listTables round-trip - which includes a per-table row count - on every
  // move. A short TTL collapses those repeats while keeping counts near-live;
  // data-changing paths (connect, refresh, DDL) pass { force: true } to bypass it.
  const TABLE_LIST_TTL_MS = 3000
  /** Every catalog read in one cache: table lists, schema catalogs, incoming FKs.
   *  Keyed connection-first so one connection's DDL can drop only its own entries. */
  const _catalog = new CatalogCache({ max: 128 })

  /** Drop every cached catalog read for the live connection. Call after DDL, and
   *  on any refresh the user asked for - they are asking because they expect the
   *  catalog to have changed. */
  function invalidateCatalog() {
    _catalog.invalidate(connectionPrefix(persistConnectionId))
    resetIncomingFkCache()
  }

  /** @param {{ force?: boolean }} [opts] */
  async function loadTables({ force = false } = {}) {
    if (!activeSchema) {
      tables = []
      loadingTables = false
      return
    }
    // Captured once: activeSchema can change while the fetch is in flight, and
    // the background count pass must target the schema this list came from.
    const schemaAtCall = activeSchema
    const connAtCall = persistConnectionId
    const key = catalogKey(persistConnectionId, 'tables', schemaAtCall)
    if (force) invalidateCatalog()
    const cached = force ? undefined : /** @type {any[] | undefined} */ (_catalog.get(key, TABLE_LIST_TTL_MS))
    if (cached) {
      tables = cached
      loadingTables = false
      if (activeTable && !tables.find((t) => t.name === activeTable)) {
        activeTable = tables[0]?.name ?? null
      }
    } else {
      loadingTables = true
      error = ''
      try {
        const list = await listTables(schemaAtCall)
        // Dropped, not applied: writing `tables` now would put the previous
        // database's tables in the sidebar of the one just connected, and the
        // _catalog.set below would file them under whichever connection id is
        // current - poisoning that connection's cache for the whole TTL.
        if (connectionMoved(connAtCall)) return
        tables = list
          .map((t) => ({
            name: t.name ?? t.table_name ?? '',
            rowCount: normalizeTableRowCount(t.rowCount ?? t.row_count),
            kind: t.kind ?? 'table',
            rlsEnabled: t.rlsEnabled ?? null,
          }))
          .filter((t) => t.name)
        _catalog.set(key, tables)
        if (activeTable && !tables.find((t) => t.name === activeTable)) {
          activeTable = tables[0]?.name ?? null
        }
      } catch (e) {
        if (connectionMoved(connAtCall)) return
        error = String(e)
        tables = []
      } finally {
        // A stale reply must not clear the spinner the *new* connection's load
        // just put up, so this is guarded too - `return` above still runs it.
        if (!connectionMoved(connAtCall)) loadingTables = false
      }
    }
    // The list arrives with unknown (null) counts so it renders immediately;
    // resolve them with exact COUNT(*)s in the background and patch them in.
    // Covers the cached path too - a list cached mid-resolve may still hold nulls.
    const unresolved = tables.filter((t) => t.rowCount === null).map((t) => t.name)
    if (unresolved.length > 0) void resolveRowCounts(key, schemaAtCall, unresolved)
    // The schema-level catalog rides the same cache, so returning to a schema
    // costs nothing. A forced reload has already invalidated it above.
    void loadSchemaCatalog()
  }

  /**
   * Background pass: exact counts for tables whose row count came back unknown
   * from listTables. Patches the sidebar (and the list cache) as results land.
   * Failures are ignored - counts are cosmetic and must never block the catalog.
   * @param {string} key connection:schema cache key at the time of the request
   * @param {string} schema
   * @param {string[]} names
   */
  async function resolveRowCounts(key, schema, names) {
    // In chunks, in list order, so counts land in waves down the sidebar instead
    // of all at the end. One request for the whole schema is all-or-nothing: on a
    // 135-table production schema every COUNT(*) had to finish before a single
    // number appeared, and a connection swapped or dropped part-way through threw
    // the completed counts away with the rest - which is why the sidebar sat on a
    // column of blanks. A chunk that fails now costs only its own tables.
    const CHUNK = 12
    for (let i = 0; i < names.length; i += CHUNK) {
      // Stale guard: the user may have switched connection/schema meanwhile.
      if (catalogKey(persistConnectionId, 'tables', activeSchema) !== key) return
      try {
        const counts = await getTableRowCounts(schema, names.slice(i, i + CHUNK))
        if (!counts?.length) continue
        if (catalogKey(persistConnectionId, 'tables', activeSchema) !== key) return
        const byName = new Map(counts.map((c) => [c.name, normalizeTableRowCount(c.rowCount ?? c.row_count)]))
        tables = tables.map((t) => (byName.has(t.name) ? { ...t, rowCount: byName.get(t.name) ?? null } : t))
        // Patch the cached list in place. Re-setting stamps a new timestamp, which
        // would extend the list's freshness window on every count that lands, so
        // the entry is only patched when it is still there to patch.
        if (_catalog.has(key)) _catalog.set(key, tables)
      } catch {
        /* ignore this chunk - its counts fill in on the next refresh instead */
      }
    }
  }

  async function reloadTableFromQuery(resetPage = true) {
    if (resetPage) { page = 1; rawOffset = null; _keysetCursor = null }
    await loadRows()
  }

  /** @param {string} value */
  function handleRowSearchChange(value) {
    const cleared = !String(value ?? '').trim()
    rowSearch = value
    page = 1
    rawOffset = null
    _keysetCursor = null
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer)
    searchDebounceTimer = null
    // Clearing is not a keystroke. The toolbar debounces typing for 250ms and
    // then this debounced it again, so emptying the box sat for another 150ms
    // while the search that was already in flight painted its rows - which is
    // the search carrying on after you cancelled it. An empty box means "show
    // me the table", and there is nothing left to coalesce, so it runs now.
    if (cleared) {
      void loadRows()
      return
    }
    searchDebounceTimer = setTimeout(() => {
      searchDebounceTimer = null
      void loadRows()
    }, SEARCH_DEBOUNCE_MS)
  }

  /** @param {TableFilter[]} filters */
  function handleRowFiltersChange(filters) {
    const prevSig = filtersApiSignature(rowFilters)
    rowFilters = filters
    const nextSig = filtersApiSignature(filters)
    if (prevSig === nextSig) return

    page = 1
    _keysetCursor = null
    if (filterDebounceTimer) clearTimeout(filterDebounceTimer)
    filterDebounceTimer = setTimeout(() => {
      filterDebounceTimer = null
      void loadRows()
    }, SEARCH_DEBOUNCE_MS)
  }

  /** @param {TableSort[]} sorts full ordered sort-key list ([] clears; [primary, …secondary]) */
  async function handleRowSortChange(sorts) {
    const list = Array.isArray(sorts) ? sorts.filter((s) => s?.column) : (sorts ? [sorts] : [])
    rowSort = list[0] ?? null
    rowSortMore = list.slice(1)
    await reloadTableFromQuery(true)
  }

  /** @param {number} size */
  async function handlePageSizeChange(size) {
    if (!Number.isFinite(size)) return
    if (size === PAGE_SIZE_ALL) {
      pageSize = PAGE_SIZE_ALL
    } else {
      if (size <= 0) return
      pageSize = Math.min(size, MAX_PAGE_SIZE)
    }
    // Persist as the session-wide default so new tabs open at this size too.
    saveDefaultPageSize(pageSize)
    await reloadTableFromQuery(true)
  }

  /**
   * Identity of the WHERE a row count was taken under. "All" has no page size of
   * its own, so it borrows `total` as the fetch limit - and that is only sound
   * while `total` describes the view about to be fetched. Filter a table down to
   * one row and then clear the filter: the count is still 1, so "All" asked for
   * LIMIT 1 and drew a one-row grid under a footer reading "1-752 of 752".
   * @param {Record<string, unknown>} search @param {TableFilter[]} filters
   */
  function predicateSig(search, filters) {
    return JSON.stringify([search, filtersApiSignature(filters)])
  }
  /** The predicate `total` belongs to; '' until a count lands. */
  let _totalSig = $state('')
  /**
   * The view a known `total` was actually counted for: connection, schema,
   * table and predicate together. The predicate alone is not enough - two
   * tables routinely carry the same filter, and reusing one's count for the
   * other would show the wrong number of rows.
   * @param {string} schema @param {string} table @param {string} sig
   */
  function rowCountKey(schema, table, sig) {
    return `${persistConnectionId}\u0000${schema}\u0000${table}\u0000${sig}`
  }
  /** Set only when a count lands; cleared by anything that can change how many
   *  rows match. While it matches the view being loaded, the count on screen is
   *  the answer and the background COUNT(*) is skipped. */
  let _countedView = ''
  /** Row data may have changed under the current predicate - count again. */
  function invalidateRowCount() {
    _countedView = ''
  }
  const rowPredicateSig = $derived(predicateSig(apiSearch(rowSearch), rowFilters))
  /** `total` was counted under the search + filters now on screen. */
  const totalIsForThisView = $derived(total > 0 && _totalSig === rowPredicateSig)

  /** Resolve the effective fetch limit for the current pageSize. */
  const effectivePageSize = $derived(fetchLimitFor(pageSize, total, totalIsForThisView))

  /** @param {number} nextPage */
  async function handlePageChange(nextPage) {
    // Keyset/cursor/temporal: navigation is next/prev via the page's boundary
    // keys, not an offset jump. page 1 ⇔ no cursor (offset-0 first page).
    if (_keysetActive) {
      if (nextPage <= 1) {
        _keysetCursor = null
        page = 1
      } else if (nextPage > page) {
        _keysetCursor = { value: _pageLastKey, after: true }
        page = nextPage
      } else {
        _keysetCursor = { value: _pageFirstKey, after: false }
        page = nextPage
      }
      await loadRows()
      return
    }
    rawOffset = null
    page = nextPage
    await loadRows()
  }

  /** @param {number} limit @param {number} offset */
  async function handleLimitOffsetChange(limit, offset) {
    const clampedLimit = Math.min(Math.max(1, limit), MAX_PAGE_SIZE)
    pageSize = clampedLimit
    // A custom limit (e.g. typing 100) also becomes the persisted default.
    saveDefaultPageSize(clampedLimit)
    rawOffset = offset
    page = Math.max(1, Math.floor(offset / clampedLimit) + 1)
    await loadRows()
  }

  /**
   * Background row fetches in flight, keyed by tab. Only history travel needs to
   * wait on one, so this stays a plain Map - nothing renders off it.
   * @type {Map<string, Promise<void>>}
   */
  const _tabFetches = new Map()

  /**
   * Kick off a background fetch for `tabId`, or hand back the one already
   * running. Mirrors fetchRowsForTab's own re-entry guard: without this, a second
   * call would replace the map entry with a promise that resolves instantly and
   * anyone waiting would be told the rows had landed while they were still in
   * flight.
   * @param {string} tabId
   */
  function startTabFetch(tabId) {
    const existing = _tabFetches.get(tabId)
    if (existing) return existing
    const p = fetchRowsForTab(tabId).finally(() => {
      if (_tabFetches.get(tabId) === p) _tabFetches.delete(tabId)
    })
    _tabFetches.set(tabId, p)
    // The spinner lives exactly as long as this promise.
    return trackBusy(tabId, p)
  }

  /**
   * Fetch rows for any tab in the background.
   * Writes results into that tab's state; if the tab is still active when the
   * fetch resolves, also syncs to the global editor state so the UI updates.
   * Callers should go through startTabFetch, which owns the tab-strip spinner.
   * @param {string} tabId
   */
  async function fetchRowsForTab(tabId) {
    if (fetchingTabIds.has(tabId)) return
    fetchingTabIds.add(tabId)
    // Re-entry guard only - the tab-strip spinner is tied to this function's
    // promise by startTabFetch, so `done` never has to be reached for it to stop.
    let _settled = false
    const done = () => {
      if (_settled) return
      _settled = true
      fetchingTabIds.delete(tabId)
    }

    const getTab = () => tabs.find((t) => t.id === tabId)
    const tab = getTab()
    if (!tab || tab.kind !== 'table' || !tab.state) {
      done()
      return
    }
    const s = /** @type {TableTabState} */ (tab.state)
    if (!s.table) {
      done()
      return
    }
    // A fresh fetch replaces the row set, so any row-index-keyed staged changes
    // cached for this table no longer line up - drop them.
    clearPendingChanges(`${s.schema}.${s.table}`)

    // Patch this tab's own state (shared helper - one tabs write per call).
    /** @param {Partial<TableTabState>} patch */
    const patchTab = (patch) => patchTableTab(tabId, patch)

    // Mark loading - one tabs write
    patchTab({ loadingRows: true, error: '' })
    if (tabId === activeTabId) { loadingRows = true; error = '' }

    // A big fetch limit must never be pulled whole into a tab - and especially not
    // into a background one. Opening three tables with the page size on 1M used to
    // fire three million-row requests at once and lock the app up for the duration.
    // The active tab goes through loadRows (which probes, measures and windows);
    // background tabs are left for activation.
    const hugeLimit =
      s.pageSize === PAGE_SIZE_ALL
        ? (s.total > WINDOW_THRESHOLD || s.total <= 0)
        : (Number.isFinite(s.pageSize) && s.pageSize > WINDOW_THRESHOLD)
    if (hugeLimit) {
      if (tabId === activeTabId) {
        // loadRows takes over the loading flag (and clears it in its finally).
        const p = loadRows()
        done()
        await p
      } else {
        // Nothing will fetch this tab until it's activated, so don't leave it
        // marked as loading - that spinner would never stop.
        patchTab({ loadingRows: false })
        done()
      }
      return
    }

    // Keyset/cursor/temporal: route the active tab's first page through loadRows
    // so it's ordered by the key column (page 1 and cursor pages share one order).
    if (tabId === activeTabId && _keysetActive) {
      _keysetCursor = null
      const p = loadRows()
      done()
      await p
      return
    }

    try {
      // Resolve the "All" sentinel - and guard a corrupt/unset value - into a
      // real fetch limit; the backend rejects limit < 1. Mirrors effectivePageSize.
      // A snapshot's count and its filters were taken together, so that count
      // does describe the view being refetched here.
      const limit =
        s.pageSize === PAGE_SIZE_ALL
          ? fetchLimitFor(PAGE_SIZE_ALL, s.total, true)
          : (Number.isFinite(s.pageSize) && s.pageSize > 0 ? s.pageSize : DEFAULT_PAGE_SIZE)
      const offset = s.pageSize === PAGE_SIZE_ALL ? 0 : (s.page - 1) * limit
      const { sortColumn, sortDirection, sorts } = sortForApi(s.rowSort, s.rowSortMore)
      const data = await getTableRows(s.schema, s.table, limit, offset, {
        ...apiSearch(s.rowSearch),
        sortColumn,
        sortDirection,
        sorts,
        filters: filtersForApi(s.rowFilters),
        // Paint rows now; the total (readRowsResponse -> -1 on Postgres) fills
        // in via the background count below.
        includeCount: false,
      })

      const result = {
        ...readRowsResponse(data),
        loadingRows: false,
        error: '',
        selected: new Set(),
        focusedRow: null,
        focusedCol: null,
        inspectorRow: null,
        editingCell: null,
      }

      // An empty table must still show its columns, or the grid reads "No
      // columns visible" and there is nothing to add a row into. Every engine
      // returns columns from its catalog when there are no rows, but a catalog
      // decode that fails (MySQL 8+ typing information_schema text VARBINARY
      // did exactly this) yields none. Fall back to the table's structure.
      if (!result.columns?.length) {
        try {
          const structure = await getTableColumnStructure(s.schema, s.table)
          if (structure?.length) {
            result.columns = structure.map((c) => ({ name: c.name, dataType: c.dataType, nullable: c.isNullable }))
          }
        } catch { /* keep the empty result; the grid shows its empty state */ }
      }

      // Persist result to tab - one tabs write
      patchTab(result)

      // Keep the raw array too. Everything inside `tabs` comes back through the
      // $state proxy, and the grid indexes rows[r][c] per visible cell per frame -
      // so handing it a proxied array puts a trap on every one of those reads.
      // saveActiveTabState only ever caches the tab you *leave*, which left every
      // prefetched background tab proxied on its first activation: open a handful
      // of tables and each one felt sticky the first time you clicked it.
      _liveRowsByTab.set(tabId, result.rows)

      // Update AI schema cache (LRU, capped)
      lruSet(tableColumnsCache, `${s.schema}.${s.table}`, result.columns)
      tableColumnsVersion++

      // Sync to global state only if this tab is still active
      if (tabId === activeTabId) {
        columns = result.columns
        primaryKey = result.primaryKey
        foreignKeys = result.foreignKeys
        rows = result.rows
        total = result.total
        queryMs = result.queryMs
        loadingRows = false
        error = ''
      }

      // Background count - keeps the tab's row fetch non-blocking. Patches the
      // tab (and global total if still active) when it resolves; -1 (non-PG /
      // failure) leaves the total readRowsResponse already set.
      void (async () => {
        try {
          const n = await countTableRows(s.schema, s.table, {
            ...apiSearch(s.rowSearch),
            filters: filtersForApi(s.rowFilters),
          })
          if (typeof n === 'number' && n >= 0) {
            patchTab({ total: n })
            if (tabId === activeTabId) {
              total = n
              _totalSig = predicateSig(apiSearch(s.rowSearch), s.rowFilters ?? [])
            }
          }
        } catch { /* best-effort */ }
      })()
    } catch (e) {
      const errStr = String(e)
      if (isNetworkError(errStr)) { connectionLost = true; void silentReconnect() }
      patchTab({ loadingRows: false, error: errStr, columns: [], rows: [], total: 0 })
      if (tabId === activeTabId) {
        loadingRows = false
        error = errStr
        columns = []
        primaryKey = []
        foreignKeys = []
        rows = []
        total = 0
      }
    } finally {
      done()
    }
  }

  /** Tear down windowing (switching to a normal / small load). */
  function resetWindowing() {
    windowed = false
    _windowSeq++
    _windowLoaded = new Set()
    _windowFetching = new Set()
    _windowQueue = []
    _windowInFlight = 0
    _windowSlow = false
    _windowBase = 0
    _windowCount = 0
    _windowRows = WINDOW_ROWS_DEFAULT
    _windowBytesPerRow = 0
    _windowOrder = null
    forgetWindowFetchState()
  }

  /** Per-view fetch bookkeeping (latency, retries, seek fallback) - reset with
   *  the view it describes, so a new table doesn't inherit the last one's. */
  function forgetWindowFetchState() {
    _windowMs = 0
    _windowSeekOff = false
    _windowAttempts.clear()
    _windowFailed.clear()
    syncWindowStatus()
  }

  /**
   * Re-establish windowing for a tab being restored from its snapshot: the sparse
   * `rows` array is handed back by applyTabToEditor, and this restores the
   * bookkeeping that says which windows it already holds, at the same window size
   * the rows were fetched with. Bumping _windowSeq abandons any window still in
   * flight for the tab we just left.
   * @param {TableTabState} s
   */
  function restoreWindowing(s) {
    _windowSeq++
    _windowLoaded = new Set(s.windowedLoaded ?? [])
    _windowFetching = new Set()
    _windowQueue = []
    _windowInFlight = 0
    _windowSlow = false
    _windowRows = s.windowRows && s.windowRows > 0 ? s.windowRows : WINDOW_ROWS_DEFAULT
    _windowBase = s.windowBase ?? 0
    _windowCount = s.windowCount ?? (Array.isArray(s.rows) ? s.rows.length : 0)
    _windowBytesPerRow = s.windowBytesPerRow ?? 0
    // Same total order as before, so windows fetched after the switch back still
    // line up with the rows already resident.
    _windowOrder = s.windowOrder ? { ...s.windowOrder, sorts: (s.windowOrder.sorts ?? []).map((k) => ({ ...k })) } : null
    forgetWindowFetchState()
    windowed = true
    dataVersion++
  }

  /** Install a fresh windowed view over `count` rows starting at absolute `base`. */
  function beginWindowing(base, count, rowsPerWindow, bytesPerRow = 0, order = null) {
    _windowSeq++
    _windowLoaded = new Set()
    _windowFetching = new Set()
    _windowQueue = []
    _windowInFlight = 0
    _windowSlow = false
    _windowRows = rowsPerWindow
    _windowBase = base
    _windowCount = count
    _windowBytesPerRow = bytesPerRow
    _windowOrder = order
    _lastFirstW = 0
    _lastDir = 1
    forgetWindowFetchState()
    windowed = true
  }

  /** Build the shared row-query options for the current view (search/sort/filter).
   *  A windowed view substitutes its stable order, so every window - and an export
   *  reading the same view - slices one and the same total ordering. */
  function currentRowQuery(includeCount = false) {
    const order = (windowed && _windowOrder) ? _windowOrder : sortForApi(rowSort, rowSortMore)
    return { ...apiSearch(rowSearch), ...order, filters: filtersForApi(rowFilters, columns), includeMeta: false, includeCount }
  }

  /** The key this view can seek by instead of paging with OFFSET (see
   *  seekKeyFor), or null while the view has none / after one was refused. */
  function windowSeek() {
    if (!windowed || _windowSeekOff) return null
    return seekKeyFor({
      order: _windowOrder,
      columns,
      primaryKey,
      dialect: connection?.type ?? '',
    })
  }

  /** The row value window `w` can seek from (last row of the window before it),
   *  or null when that row isn't resident and the window must use OFFSET. */
  function windowAnchor(w, seek = windowSeek()) {
    if (!seek || w <= 0) return null
    const prev = rows[w * _windowRows - 1]
    const v = prev ? prev[seek.index] : undefined
    return v === undefined || v === null ? null : v
  }

  /** Fetch one window and splice it into the sparse `rows` array in place.
   *  Window `w` covers view rows [w*_windowRows, …), which live at absolute table
   *  offset _windowBase + that - the view can be a page, not just the whole table. */
  async function fetchWindow(w) {
    if (!windowed || w < 0 || _windowLoaded.has(w) || _windowFetching.has(w)) return
    const start = w * _windowRows
    if (start >= _windowCount) return
    const limit = Math.min(_windowRows, _windowCount - start)
    const seq = _windowSeq
    const seek = windowSeek()
    const anchor = windowAnchor(w, seek)
    const keyset = anchor === null || !seek
      ? null
      : { column: seek.column, value: String(anchor), sqlType: seek.sqlType, after: true, desc: seek.desc }
    _windowFetching.add(w)
    _windowInFlight++
    // Re-queued after giving up (a scroll, or the pill's Retry) - it's loading
    // again, so the grid should say so rather than keep showing the error.
    if (_windowFailed.delete(w)) syncWindowStatus()
    const t0 = performance.now()
    try {
      const data = await getTableRows(activeSchema, activeTable, limit, _windowBase + start, { ...currentRowQuery(false), keyset })
      // Rolling, so the measure tracks the connection rather than latching on the
      // first deep page. Recorded even for a superseded fetch - the round-trip
      // was just as slow whether or not its rows are still wanted.
      const ms = performance.now() - t0
      _windowMs = _windowMs ? _windowMs * 0.65 + ms * 0.35 : ms
      _windowSlow = _windowMs > WINDOW_SLOW_MS
      if (seq !== _windowSeq) return // table / query changed while in flight
      const fetched = data.rows ?? []
      for (let i = 0; i < fetched.length; i++) rows[start + i] = fetched[i]
      _windowLoaded.add(w)
      _windowAttempts.delete(w)
      _windowFailed.delete(w)
      dataVersion++
    } catch {
      if (seq !== _windowSeq) return
      if (keyset) {
        // The cursor cast can be rejected for an exotic key type. Fall back for
        // the whole view and retry this window on the offset path immediately -
        // that path is always correct, just slower.
        _windowSeekOff = true
        _windowQueue.unshift(w)
        return
      }
      // Retry with backoff. Without this a dropped connection left the rows
      // shimmering until the user happened to scroll, since only a *change* of
      // visible range re-queues anything.
      const tries = (_windowAttempts.get(w) ?? 0) + 1
      _windowAttempts.set(w, tries)
      if (tries > WINDOW_RETRIES) {
        _windowFailed.add(w)
      } else {
        setTimeout(() => {
          if (!windowed || seq !== _windowSeq || _windowLoaded.has(w)) return
          _windowQueue.unshift(w)
          pumpWindowQueue()
        }, 300 * 2 ** (tries - 1))
      }
    } finally {
      _windowFetching.delete(w)
      // Clamped: a reset (new query) zeroes the counter while requests are still
      // in flight, and their finallys must not drive it negative.
      _windowInFlight = Math.max(0, _windowInFlight - 1)
      syncWindowStatus()
      pumpWindowQueue()
    }
  }

  /**
   * Start queued window fetches up to the concurrency cap. Bounded on purpose: a
   * fast flick crosses many windows, and firing a request for each one at once
   * both floods the pool and puts the rows the user is actually looking at behind
   * a queue of windows already scrolled past.
   */
  function pumpWindowQueue() {
    const cap = _windowSlow ? 1 : WINDOW_MAX_INFLIGHT
    while (windowed && _windowInFlight < cap && _windowQueue.length > 0) {
      const i = nextStartableWindow()
      if (i < 0) break
      const w = _windowQueue.splice(i, 1)[0]
      void fetchWindow(w)
    }
  }

  /**
   * Which queued window to start next, or -1 to wait for one in flight.
   *
   * The queue is built viewport-first, so its head is what the user is looking
   * at and always starts. The rest is read-ahead, and on a seekable view (see
   * windowSeek) read-ahead is worth *waiting* for: a window whose predecessor
   * has landed is an index seek, while firing four deep-OFFSET queries in
   * parallel is four full index walks for the same rows. Nothing can wait
   * forever - with nothing in flight to produce an anchor, the head starts on
   * the offset path.
   */
  function nextStartableWindow() {
    const seek = windowSeek()
    let head = -1
    for (let i = 0; i < _windowQueue.length; i++) {
      const w = _windowQueue[i]
      if (_windowLoaded.has(w) || _windowFetching.has(w)) continue
      if (head < 0) {
        if (!seek || _windowInFlight === 0) return i
        head = i
        continue
      }
      if (windowAnchor(w, seek) !== null) return i
    }
    // head >= 0 here only while something is in flight, so waiting costs at most
    // one round-trip - after which that fetch's anchor may make read-ahead cheap.
    return seek ? -1 : head
  }

  /** Publish fetcher state for the grid's loading pill, only on a real change. */
  function syncWindowStatus() {
    const failed = _windowFailed.size > 0
    if (windowStatus.slow !== _windowSlow || windowStatus.failed !== failed) {
      windowStatus = { slow: _windowSlow, failed }
    }
  }

  /** "Retry" in the grid's loading pill: re-queue every window that gave up. */
  function retryFailedWindows() {
    if (!windowed || _windowFailed.size === 0) return
    const again = [..._windowFailed]
    _windowFailed.clear()
    for (const w of again) _windowAttempts.delete(w)
    _windowQueue = [...again, ..._windowQueue]
    syncWindowStatus()
    pumpWindowQueue()
  }

  /** Evict resident windows far from the viewport so memory stays bounded. */
  function evictFarWindows(firstW, lastW) {
    if (!windowed) return
    let evicted = false
    const keep = windowKeepCount(_windowRows)
    for (const w of _windowLoaded) {
      if (w < firstW - keep || w > lastW + keep) {
        const start = w * _windowRows
        const endI = Math.min(start + _windowRows, _windowCount)
        for (let i = start; i < endI; i++) rows[i] = undefined
        _windowLoaded.delete(w)
        evicted = true
      }
    }
    if (evicted) dataVersion++
  }

  let _visRangeTimer = /** @type {ReturnType<typeof setTimeout> | null} */ (null)
  /**
   * DataTable reports its visible row range → load nearby windows, evict far ones.
   *
   * The windows under the viewport are requested immediately: those are the rows
   * being drawn as skeletons right now, and the old 100ms debounce added that
   * delay to every one of them. Only eviction waits for the scroll to settle -
   * dropping windows mid-flick just re-fetches them a moment later.
   */
  function handleVisibleRange(start, end) {
    if (!windowed) return
    const firstW = Math.floor(start / _windowRows)
    const lastW = Math.floor(end / _windowRows)
    if (firstW !== _lastFirstW) {
      _lastDir = firstW > _lastFirstW ? 1 : -1
      _lastFirstW = firstW
    }
    // Priority order: what's on screen, then ahead in the direction of travel,
    // then the window just behind (so a small scroll-back is already resident).
    /** @type {number[]} */
    const want = []
    const ahead = _windowSlow ? 1 : WINDOW_PREFETCH
    for (let w = firstW; w <= lastW; w++) want.push(w)
    for (let i = 1; i <= ahead; i++) want.push(_lastDir >= 0 ? lastW + i : firstW - i)
    want.push(_lastDir >= 0 ? firstW - 1 : lastW + 1)
    // Rebuilt (not appended) every emit, so windows already scrolled past drop out
    // of the queue instead of being fetched after the user has left them.
    _windowQueue = want.filter(
      (w) => w >= 0 && w * _windowRows < _windowCount && !_windowLoaded.has(w) && !_windowFetching.has(w),
    )
    pumpWindowQueue()
    if (_visRangeTimer) clearTimeout(_visRangeTimer)
    _visRangeTimer = setTimeout(() => {
      _visRangeTimer = null
      if (windowed) evictFarWindows(firstW, lastW)
    }, 300)
  }

  /** Fetch the full ordered result set in chunks (windowed export/copy path).
   *  Chunked coarser than the scroll windows - nothing is being rendered meanwhile,
   *  so throughput matters more than latency - but still by BYTES, since 50k rows
   *  of a table with an embedding column is most of a gigabyte in one response. */
  const EXPORT_CHUNK_MAX = 50_000
  const EXPORT_TARGET_BYTES = 20_000_000
  async function fetchAllRows(onProgress) {
    const chunk = _windowBytesPerRow > 0
      ? Math.max(500, Math.min(EXPORT_CHUNK_MAX, Math.round(EXPORT_TARGET_BYTES / _windowBytesPerRow)))
      : EXPORT_CHUNK_MAX
    /** @type {any[]} */
    const out = []
    for (let off = 0; off < total; off += chunk) {
      const data = await getTableRows(activeSchema, activeTable, chunk, off, currentRowQuery(false))
      const r = data.rows ?? []
      for (let i = 0; i < r.length; i++) out.push(r[i])
      onProgress?.(out.length)
      if (r.length < chunk) break
    }
    return out
  }

  /**
   * @param {{ keepScroll?: boolean }} [opts]
   *   keepScroll - used by live refresh: re-run the *current* query (filters,
   *   sort, search, page from global state) but update rows in place without
   *   jumping the grid to the top or closing the row inspector.
   */
  function loadRows(opts = {}) {
    // The tab that owns this load, captured before anything can await. Every
    // write inside is routed through it, so switching or closing tabs mid-fetch
    // neither cancels the query nor drops its rows into whichever tab happens to
    // be in front when they arrive - and the tab strip spins until it settles.
    return trackBusy(activeTabId, runLoadRows(opts, activeTabId))
  }

  /** @param {{ keepScroll?: boolean }} opts @param {string | null} ownerTabId */
  async function runLoadRows({ keepScroll = false } = {}, ownerTabId = null) {
    if (!activeTable) {
      columns = []
      rows = []
      _infiniteRows = []
      total = 0
      return
    }
    const ownerSchema = activeSchema
    const ownerTable = activeTable
    const seq = bumpLoadSeq(ownerTabId)
    /** Still the newest load for its tab, and that tab still exists. */
    const live = () => seq === loadSeqOf(ownerTabId) && tabs.some((t) => t.id === ownerTabId)
    /** Owner is on screen - only then may this load touch the shared editor state. */
    const isActive = () => ownerTabId === activeTabId
    patchTableTab(ownerTabId ?? '', { loadingRows: true, error: '' })
    _windowSeq++ // discard any window fetches in flight from a prior query
    loadingRows = true
    _infiniteRows = []
    if (!keepScroll) {
      selected = new Set()
      focusedRow = null
      focusedCol = null
      inspectorRow = null
      editingCell = null
    }
    error = ''
    try {
      const offset = currentOffset
      let { sortColumn, sortDirection, sorts } = sortForApi(rowSort, rowSortMore)
      // Keyset/cursor/temporal: force ordering by the key column (so page 1 and
      // subsequent cursor pages share one total order) and send the cursor. When
      // inactive this is all skipped and it's plain OFFSET.
      const ksActive = _keysetActive
      let keysetArg = null
      if (ksActive) {
        sortColumn = _keysetKeyCol
        sortDirection = _keysetDesc ? 'desc' : 'asc'
        sorts = []
        if (_keysetCursor && _keysetCursor.value != null) {
          keysetArg = {
            column: _keysetKeyCol,
            value: String(_keysetCursor.value),
            sqlType: _keysetKeyType,
            after: _keysetCursor.after,
            desc: _keysetDesc,
          }
        }
      }
      // Catalog metadata (pk/fk/enums/nullable) only changes when the table's
      // structure does, so request it only on the first load of a table; repeat
      // fetches (pagination, sort, filter, live) reuse what we already hold,
      // skipping several round-trips per fetch.
      const includeMeta = columns.length === 0
      // A large fetch limit gets probed instead of pulled: 200 rows plus the count,
      // then the rows are measured and either the rest is loaded normally (small,
      // light result) or a windowed view is installed.
      //
      // Note this is NOT gated on the "All" sentinel. Page size 1M is an ordinary
      // option in the toolbar, and taking it literally meant one request for a
      // million rows - ~130MB of JSON decoded on the main thread, per tab, which
      // froze the whole app. Any limit past the threshold is treated the same way
      // now, whichever control produced it.
      const wantsWindow = effectivePageSize > WINDOW_THRESHOLD
      // A windowed view is assembled from many separate LIMIT/OFFSET queries, and
      // those only line up if they all slice ONE total order - so the probe is
      // ordered by the primary key too, whenever we already know it. On the very
      // first load of a table the key arrives with this response, so the probe
      // can't be ordered yet; its rows are then treated as provisional below.
      const probeOrder = wantsWindow ? stableWindowOrder(rowSort, rowSortMore, primaryKey) : null
      // Query shape frozen at call time. The remainder fetch below must not read
      // it back off the globals - by then they may describe another tab.
      const ownerQuery = {
        ...apiSearch(rowSearch),
        ...(probeOrder ?? { sortColumn, sortDirection, sorts }),
        filters: filtersForApi(rowFilters, columns),
      }
      // Frozen with the query: the count this load produces belongs to THIS
      // predicate, even if the user edits the filters while it is in flight.
      const ownerSig = predicateSig(apiSearch(rowSearch), rowFilters)
      // Identity of the view this load is for, for reusing a count across pages.
      const viewKey = rowCountKey(ownerSchema, ownerTable, ownerSig)
      const data = await getTableRows(
        ownerSchema, ownerTable,
        wantsWindow ? WINDOW_PROBE : effectivePageSize,
        offset,
        { ...ownerQuery,
          keyset: keysetArg,
          includeMeta,
          // Don't wait on COUNT(*) - paint rows now, count streams in below.
          // Windowed loads need the total immediately to size the sparse array.
          includeCount: wantsWindow,
        })
      if (!live()) return
      const nextColumns = data.columns ?? []
      const fetched = data.rows ?? []
      const windowTotal = wantsWindow ? Number(data.total ?? 0) : 0
      const ranMs = Number(data.queryMs ?? data.query_ms ?? 0)
      // How long this view is, and how heavy: both decide whether to window.
      // `articles` at 100k rows is small enough to load whole; `openai_docs` at
      // 10k rows is not, because each row carries a ~17KB embedding.
      const viewBase = wantsWindow ? offset : 0
      // A windowed view has to know exactly how long it is - a sparse array sized
      // from a guess would leave skeleton rows past the end of the table forever.
      // Engines that can't count (total ≤ 0) therefore keep the old behaviour: load
      // the requested limit in full.
      const countKnown = windowTotal > 0
      const viewCount = wantsWindow
        ? (countKnown ? Math.max(0, Math.min(effectivePageSize, windowTotal - viewBase)) : effectivePageSize)
        : fetched.length
      const bytesPerRow = wantsWindow ? measureRowBytes(fetched) : 0
      const probeRows = wantsWindow ? pickWindowRows(bytesPerRow) : WINDOW_ROWS_DEFAULT
      // The order every window will slice. Recomputed here because the primary key
      // may only just have arrived with this response.
      const pkNow = includeMeta ? (data.primaryKey ?? data.primary_key ?? []) : primaryKey
      const windowOrder = wantsWindow ? stableWindowOrder(rowSort, rowSortMore, pkNow) : null
      // No stable order (a table with no primary key) means no windowing: a single
      // query is internally consistent, many unordered ones are not. Such a table
      // loads whole, as it did before.
      const useWindow =
        wantsWindow && !!windowOrder && shouldWindow({ rowCount: viewCount, bytesPerRow, countKnown })
      // Probe rows belong in the sparse array only if they came from that same
      // order. Otherwise they're a different slice of the table, and keeping them
      // is exactly the "row 108 shows a different id each time" problem - so they
      // are dropped and window 0 is fetched properly.
      const probeUsable = !!probeOrder
      if (includeMeta && nextColumns.length) {
        lruSet(tableColumnsCache, `${ownerSchema}.${ownerTable}`, nextColumns)
        tableColumnsVersion++
      }

      // ── The owner moved to the background while this ran ────────────────────
      // Commit into its own tab state and leave the visible editor untouched.
      // (Before this branch existed, a 1M-row load landing after a tab switch
      // overwrote whatever table was now on screen.)
      if (!isActive()) {
        const meta = includeMeta
          ? {
              primaryKey: data.primaryKey ?? data.primary_key ?? [],
              foreignKeys: normalizeForeignKeys(data.foreignKeys ?? data.foreign_keys),
            }
          : {}
        // Keep the columns the tab already holds when the shape is unchanged -
        // they carry enum/nullable metadata a includeMeta:false fetch doesn't.
        const prevCols = /** @type {TableTabState | null} */ (tabs.find((t) => t.id === ownerTabId)?.state ?? null)?.columns ?? []
        const cols = nextColumns.length && !sameColumnShape(prevCols, nextColumns)
          ? nextColumns
          : (prevCols.length ? prevCols : nextColumns)
        if (useWindow) {
          // Finish the windowed load into the tab that started it: the sparse array
          // (raw, outside the reactive tree) plus the window bookkeeping. Switching
          // back then shows the rows immediately instead of re-running the fetch and
          // the count - which is the whole point of leaving a huge table loading.
          const arr = new Array(viewCount)
          if (probeUsable) {
            for (let i = 0; i < fetched.length && i < viewCount; i++) arr[i] = fetched[i]
          }
          _liveRowsByTab.set(ownerTabId, arr)
          patchTableTab(ownerTabId ?? '', {
            columns: cols,
            ...meta,
            rows: [],
            windowedHead: true,
            // The probe covers only whole windows it filled completely; a partly
            // covered one is refetched so no row is left silently missing.
            windowedLoaded: probeUsable ? windowsFullyCovered(fetched.length, probeRows) : [],
            windowRows: probeRows,
            windowBase: viewBase,
            windowCount: viewCount,
            windowBytesPerRow: bytesPerRow,
            windowOrder,
            total: windowTotal,
            queryMs: ranMs,
            loadingRows: false,
            error: '',
          })
          return
        }
        let landed = fetched
        if (wantsWindow && viewCount > fetched.length) {
          const restData = await getTableRows(ownerSchema, ownerTable, Math.min(viewCount - fetched.length, MAX_PAGE_SIZE), viewBase + fetched.length, { ...ownerQuery, includeMeta: false, includeCount: false })
          if (!live()) return
          landed = [...fetched, ...(restData.rows ?? [])]
        }
        _liveRowsByTab.set(ownerTabId, landed)
        patchTableTab(ownerTabId ?? '', {
          columns: cols,
          ...meta,
          rows: landed,
          windowedHead: false,
          windowedLoaded: [],
          total: wantsWindow ? windowTotal : Number(data.total ?? 0),
          queryMs: ranMs,
          loadingRows: false,
          error: '',
        })
        return
      }

      // Update column shape whenever it actually changes (e.g. a column was
      // added/dropped) even on a metadata-skipping fetch; otherwise keep the
      // richer existing columns (which carry enum/nullable info).
      if (nextColumns.length && !sameColumnShape(columns, nextColumns)) columns = nextColumns
      if (includeMeta) {
        primaryKey = data.primaryKey ?? data.primary_key ?? []
        foreignKeys = normalizeForeignKeys(data.foreignKeys ?? data.foreign_keys)
      }
      if (useWindow) {
        // Sparse windowed array: one slot per row of this view, holding only the
        // probe's rows so far. Window size came from measuring those rows.
        beginWindowing(viewBase, viewCount, probeRows, bytesPerRow, windowOrder)
        const arr = new Array(viewCount)
        // Only rows that came from the view's own ordering may stay - see probeUsable.
        if (probeUsable) {
          for (let i = 0; i < fetched.length && i < viewCount; i++) arr[i] = fetched[i]
          for (const w of windowsFullyCovered(fetched.length, probeRows)) _windowLoaded.add(w)
        }
        rows = arr
        _infiniteRows = []
        total = windowTotal
        _totalSig = ownerSig
        dataVersion++
        // Warm the windows the probe didn't cover, before the first scroll asks.
        // Only for a load that starts at the top: on a refresh that keeps the
        // scroll position the viewport is somewhere else entirely, and queuing
        // the head would put five windows the user cannot see in front of the
        // one they are looking at. The grid re-emits its visible range as soon
        // as the new array lands, which asks for the right ones.
        if (!keepScroll) {
          const firstGap = _windowLoaded.size
          for (let w = firstGap; w <= firstGap + WINDOW_PREFETCH; w++) _windowQueue.push(w)
          pumpWindowQueue()
        }
      } else if (wantsWindow) {
        // Past the limit bar but small and light after measuring - load the rest.
        resetWindowing()
        total = windowTotal
        _totalSig = ownerSig
        // Paint the probe NOW, before going back for the rest. The remainder is a
        // second round-trip over tens of thousands of rows, and holding `rows`
        // empty until it lands leaves the grid blank for all of it - with the
        // columns already drawn, which reads as "this table is empty" rather than
        // "this is still loading".
        rows = fetched
        _infiniteRows = fetched
        if (viewCount > fetched.length) {
          const restData = await getTableRows(ownerSchema, ownerTable, Math.min(viewCount - fetched.length, MAX_PAGE_SIZE), viewBase + fetched.length, { ...ownerQuery, includeMeta: false, includeCount: false })
          if (!live()) return
          const all = [...fetched, ...(restData.rows ?? [])]
          // The tab may have gone to the background during this second trip -
          // then the rows belong to its own state, not to the visible grid.
          if (!isActive()) {
            _liveRowsByTab.set(ownerTabId, all)
            patchTableTab(ownerTabId ?? '', { columns: nextColumns, rows: all, total: windowTotal, queryMs: ranMs, loadingRows: false, error: '', windowedHead: false, windowedLoaded: [] })
            return
          }
          rows = all
          _infiniteRows = rows
        }
      } else {
        resetWindowing()
        rows = fetched
        _infiniteRows = fetched
        // total = -1 means "counting" (Postgres, non-blocking). Other engines
        // return a real total here; refreshRowCount() then no-ops for them.
        // When the count for this exact view is already known, keep it rather
        // than dropping back to the sentinel: without this, every page change
        // would blank the total and force the COUNT(*) the skip below exists to
        // avoid, and the footer would flicker through "counting" on each page.
        const incoming = Number(data.total ?? 0)
        total = incoming >= 0 ? incoming : (_countedView === viewKey ? total : incoming)
        _totalSig = ownerSig
      }
      queryMs = ranMs
      // Record this page's boundary key values so next/prev can build cursors.
      if (ksActive && _keyColIndex >= 0) {
        _pageFirstKey = rows.length ? rows[0]?.[_keyColIndex] : null
        _pageLastKey = rows.length ? rows[rows.length - 1]?.[_keyColIndex] : null
      }
      // Live refresh updates in place - only reset scroll for user-driven loads
      // (page/filter/sort/search), where jumping to the top is expected.
      if (!keepScroll) reloadToken++
      if (total >= 0) {
        const maxPage = Math.max(1, Math.ceil(total / effectivePageSize) || 1)
        if (page > maxPage) page = maxPage
      }
      // Fire-and-forget: fill the total in the background so the count never
      // delays the rows. Windowed loads already have a real total from the fetch.
      // A page change cannot change how many rows match the predicate, so the
      // count already on screen is the answer. Re-asking is a full COUNT(*) per
      // page: measured on the 1M-row `events` fixture with a `kind LIKE` filter
      // at 182ms cold / 32ms warm, against 0.79ms for the page of rows it
      // accompanies - and on a remote database it is another round trip on top.
      // The count is background work, so this never showed up as a blocked
      // paint, only as load the server was carrying for an answer we had.
      // `keepScroll` is the live refresh: rows may have changed underneath, so
      // that always recounts.
      if (!windowed && (keepScroll || _countedView !== viewKey)) {
        void refreshRowCount(ownerTabId, seq, ownerSchema, ownerTable, ownerQuery, ownerSig, viewKey)
      }
      // Mirror the landed result into the owning tab so the spinner in the tab
      // strip stops and a later switch away/back doesn't refetch it.
      patchTableTab(ownerTabId ?? '', { loadingRows: false, error: '' })
    } catch (e) {
      if (!live()) return
      const errStr = String(e)
      if (isNetworkError(errStr)) { connectionLost = true; void silentReconnect() }
      patchTableTab(ownerTabId ?? '', { loadingRows: false, error: errStr, columns: [], rows: [], total: 0 })
      if (isActive()) {
        error = errStr
        resetWindowing()
        columns = []
        primaryKey = []
        foreignKeys = []
        rows = []
        _infiniteRows = []
        total = 0
      }
      recordActivity({ type: 'row_fetch', title: `Failed to load ${ownerTable}`, schema: ownerSchema, table: ownerTable ?? undefined, success: false, error: errStr })
    } finally {
      // The newest load for a tab owns its loading flag - in the tab's own state
      // (which is what the tab strip draws) and, while that tab is on screen, in
      // the grid too. A superseded load clears neither: the load that replaced it
      // will, when it reaches this same block.
      if (seq === loadSeqOf(ownerTabId)) {
        patchTableTab(ownerTabId ?? '', { loadingRows: false })
        if (isActive()) loadingRows = false
      }
    }
  }

  /**
   * Background row-count pass for the main grid. Runs after loadRows() has
   * already painted the rows, so COUNT(*) never blocks the initial view. The
   * per-tab load token drops results from a superseded load (fast tab/filter
   * switches). Returns -1 on non-Postgres engines / failure - in which case the
   * total set by loadRows() is kept untouched. Like the load itself, the count
   * belongs to the tab that asked for it: it lands in that tab's state and only
   * touches the visible total while that tab is still in front.
   * @param {string | null} tabId @param {number} seq @param {string} schema
   * @param {string} table
   * @param {{ search?: string, searchIsRegex?: boolean, searchCaseSensitive?: boolean, filters?: any[] }} query
   *   The SAME object the rows query used - a count taken under a different
   *   predicate is a total that disagrees with the page it labels.
   */
  async function refreshRowCount(tabId, seq, schema, table, query, sig = '', viewKey = '') {
    if (!table) return
    try {
      const n = await countTableRows(schema, table, query)
      if (seq !== loadSeqOf(tabId)) return
      if (typeof n === 'number' && n >= 0) {
        patchTableTab(tabId ?? '', { total: n })
        if (tabId !== activeTabId) return
        total = n
        _totalSig = sig
        _countedView = viewKey
        const maxPage = Math.max(1, Math.ceil(total / effectivePageSize) || 1)
        if (page > maxPage) page = maxPage
      }
    } catch { /* count is best-effort - leave the current total as-is */ }
  }

  async function handleLoadMore() {
    if (windowed) return // windowed mode loads via handleVisibleRange, not append
    if (!infiniteScroll || !activeTable || loadingRows || loadingMore) return
    if (total >= 0 && _infiniteRows.length >= total) return
    if (_infiniteRows.length >= INFINITE_ROW_CAP) return
    loadingMore = true
    const ownerTabId = activeTabId
    const seq = loadSeqOf(ownerTabId)
    try {
      const offset = _infiniteRows.length
      const { sortColumn, sortDirection, sorts } = sortForApi(rowSort, rowSortMore)
      const data = await getTableRows(activeSchema, activeTable, effectivePageSize, offset, {
        ...apiSearch(rowSearch),
        sortColumn,
        sortDirection,
        sorts,
        filters: filtersForApi(rowFilters, columns),
      })
      // A superseded load, or a tab switch, means these rows no longer belong to
      // what's on screen - appending them would splice one table into another.
      if (seq !== loadSeqOf(ownerTabId) || ownerTabId !== activeTabId) return
      const fetched = data.rows ?? []
      if (!fetched.length) return
      // Append in place - spreading the whole accumulated array on every page was
      // O(n) per load (O(n²) over a scroll session). Pages are page-size (small),
      // so push() is cheap; the slice() below gives `rows` (which is $state.raw)
      // a fresh raw identity so the grid's rows.length deriveds fire.
      for (let i = 0; i < fetched.length; i++) _infiniteRows.push(fetched[i])
      rows = _infiniteRows.slice()
      total = Number(data.total ?? total)
      _totalSig = rowPredicateSig
    } catch (e) {
      error = String(e)
    } finally {
      loadingMore = false
    }
  }

  function toggleInfiniteScroll() {
    infiniteScroll = !infiniteScroll
    saveInfiniteScroll(infiniteScroll)
    if (infiniteScroll) {
      // reset to page 1 with current pageSize so infinite starts from the top
      page = 1; rawOffset = null
      void loadRows()
    } else {
      // switching off: go back to normal page view (already on page 1)
      void loadRows()
    }
  }

/**
   * Whether two column lists describe the same shape (name + type, in order).
   * Used to preserve the array reference across page fetches of the same table.
   * @param {any[]} a @param {any[]} b
   */
  function sameColumnShape(a, b) {
    if (a === b) return true
    if (!a || !b || a.length !== b.length) return false
    for (let i = 0; i < a.length; i++) {
      if (a[i]?.name !== b[i]?.name) return false
      if (columnType(a[i]) !== columnType(b[i])) return false
    }
    return true
  }

  /** @param {'csv' | 'json'} format */
  let exportingData = $state(false)

  async function handleExport(format) {
    if (exportingData) return
    exportingData = true
    // Windowed results keep only a few resident windows, so read the full
    // ordered set from the server for export instead of the sparse client array.
    let sourceRows = rows
    if (windowed) {
      const fetchId = toast.info('Fetching all rows for export…', { duration: 60 * 60 * 1000 })
      try {
        sourceRows = await fetchAllRows()
      } catch (e) {
        toast.dismiss(fetchId)
        exportingData = false
        toast.error('Export failed', { description: String(e) })
        return
      }
      toast.dismiss(fetchId)
    }
    const exportRows = (selected.size > 0 && selected.size < sourceRows.length)
      ? [...selected].sort((a, b) => a - b).map((i) => sourceRows[i]).filter(Boolean)
      : sourceRows
    // Export only the columns the user has left visible. Rows are positional
    // arrays, so filter the column list AND project each row's cells by the
    // original column index (mirrors dataViewRows for the json/text views).
    const visibleIdxs = columns.map((_, i) => i).filter((i) => !hiddenColumns.has(columns[i].name))
    const exportColumns = visibleIdxs.map((i) => columns[i])
    const exportCells = visibleIdxs.length === columns.length
      ? exportRows
      : exportRows.map((r) => visibleIdxs.map((i) => r[i]))
    const filename = buildExportFilename(activeTable, format)
    const n = exportRows.length
    // Small exports build instantly; only large ones need the async/progress path.
    const LARGE = 20000
    // A persistent toast that stays up for the whole build/save, dismissed on completion.
    const toastId = toast.info(`Exporting ${formatCompactCount(n)} rows…`, {
      description: `Preparing ${format.toUpperCase()}, please wait`,
      duration: 60 * 60 * 1000,
    })
    try {
      // Yield once so the toast paints before any heavy synchronous work.
      await new Promise((res) => setTimeout(res, 16))
      /** @type {string} */
      let content
      // Only CSV/JSON have chunked async builders; the other formats (added for
      // parity with the SQL console) build synchronously even for large sets.
      if (n > LARGE && (format === 'csv' || format === 'json')) {
        content = format === 'csv'
          ? await rowsToCsvAsync(exportColumns, exportCells)
          : await rowsToJsonAsync(exportColumns, exportCells)
      } else {
        content =
          format === 'csv' ? rowsToCsv(exportColumns, exportCells)
            : format === 'json' ? rowsToJson(exportColumns, exportCells)
            : format === 'sql' ? rowsToSql(exportColumns, exportCells, activeTable || 'exported_table')
            : format === 'tsv' ? rowsToTsv(exportColumns, exportCells)
            : format === 'md' ? rowsToMarkdown(exportColumns, exportCells)
            : rowsToJsonl(exportColumns, exportCells)
      }
      const saved = await saveExportFile(content, filename, format)
      toast.dismiss(toastId)
      if (saved) {
        toast.success(`Exported ${formatCompactCount(n)} rows`, { description: filename })
        recordActivity({ type: 'export', title: `Exported ${activeTable} as ${format.toUpperCase()}`, schema: activeSchema, table: activeTable ?? undefined, rowCount: n, success: true, detail: filename })
      }
    } catch (e) {
      toast.dismiss(toastId)
      toast.error('Export failed', { description: String(e) })
      recordActivity({ type: 'export', title: `Failed to export ${activeTable} as ${format.toUpperCase()}`, schema: activeSchema, table: activeTable ?? undefined, success: false, error: String(e) })
    } finally {
      exportingData = false
    }
  }

  /**
   * Execute SQL in the editor. With `overrideSql` (run-statement-at-cursor,
   * ⌘R), only that statement runs; otherwise the whole editor buffer runs.
   * @param {string} [overrideSql]
   */
  function runSql(overrideSql) {
    // The owning tab spins until the run settles, wherever the user has navigated
    // to in the meantime.
    return trackBusy(activeTabId, runSqlOnTab(overrideSql))
  }

  /** @param {string} [overrideSql] */
  /**
   * The view of a result streaming into the store for `tabId`. It paints only
   * while its tab is the one in front; in the background it just keeps count.
   * @param {string | null} tabId @param {string} id
   */
  function storedView(tabId, id) {
    const inFront = () => activeTabId === tabId && sqlViewOf(tabId) === view
    const view = new StoredResultView({
      id,
      onrows: (r) => { if (inFront()) sqlRows = r },
      onchange: () => { if (inFront()) sqlDataVersion++ },
      onstatus: (st) => { if (inFront()) sqlWindowStatus = st },
    })
    if (tabId) _sqlViewsByTab.set(tabId, view)
    return view
  }

  /** @param {string | null} tabId */
  function sqlViewOf(tabId) {
    return tabId ? _sqlViewsByTab.get(tabId) ?? null : null
  }

  /** Drop a SQL tab's result: its rows here and its file in the store. @param {string} tabId */
  function forgetSqlResult(tabId) {
    _sqlViewsByTab.get(tabId)?.dispose()
    _sqlViewsByTab.delete(tabId)
    _sqlRowsByTab.delete(tabId)
  }

  /**
   * Sort the active tab's stored result in the backend (typed by the column's
   * database type) and refetch what is on screen, or back to stream order.
   * @param {{ column: string, direction: 'asc' | 'desc' } | null} sort
   */
  async function sortStoredSqlResult(sort) {
    const view = sqlViewOf(activeTabId)
    if (!view) return
    const column = sort ? sqlColumns.findIndex((c) => (c.name ?? c) === sort.column) : -1
    sqlSorting = true
    try {
      await resultSort(view.id, column >= 0 ? column : null, sort?.direction === 'desc')
      view.reorder()
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ''))
    } finally {
      sqlSorting = false
    }
  }

  async function runSqlOnTab(overrideSql) {
    track('sql_run')
    const sqlRan = typeof overrideSql === 'string' && overrideSql.trim() ? overrideSql : sqlText
    if (!connection || !sqlRan.trim()) return
    if (tableReadonly && isWriteSql(sqlRan)) {
      sqlError = 'This connection is open in read-only mode, so write statements are blocked.'
      return
    }
    // The tab that owns this run. Everything below writes results back through
    // it, so leaving for another tab mid-query neither stops the spinner here
    // nor drops the answer into whatever tab is in front when it arrives.
    const runTabId = activeTabId
    const stillHere = () => activeTabId === runTabId
    // Cancel handle for this run, keyed to its tab: several tabs can be running
    // at once, and Stop has to reach the query belonging to the tab you're on.
    const queryId = `sql-${runTabId ?? 'none'}-${++_sqlRunSeq}`
    if (runTabId) _sqlQueryIdByTab.set(runTabId, queryId)
    sqlLoading = true
    sqlError = ''
    sqlMessage = ''
    sqlColumns = []
    sqlRows = []
    sqlMultiResults = []
    // The tab's last result goes, in the store too.
    if (runTabId) forgetSqlResult(runTabId)
    if (stillHere()) { sqlWindowed = false; sqlWindowStatus = null; sqlRunOutcomes = [] }
    // Mark the owning tab as running too, so its snapshot says so if the user
    // leaves before this finishes.
    patchSqlTab(runTabId, { sqlLoading: true, sqlError: '', sqlMessage: '', sqlColumns: [], sqlRows: [] })
    let ranMs = 0
    let ranError = ''
    let ranRowCount = 0
    try {
      // A tab with an open transaction runs on that transaction's connection,
      // so its statements stay invisible until the user commits. Everything
      // below is identical - only the executor differs.
      const txSession = runTabId ? sqlTxSessions.get(runTabId) : null
      let results
      if (txSession) {
        results = [await txExecute(txSession, sqlRan)]
        if (runTabId) setTxStatus(runTabId, await txStatus(txSession))
      } else {
        // Rows stream in (executeSqlStream), into the backend's result store
        // when that setting is on, otherwise into this window.
        /** @type {any[][] | null} */
        let streamed = null
        /** @type {any[]} */
        let streamedCols = []
        /** @type {StoredResultView | null} */
        let view = null
        let lastPaint = 0
        let paintedLen = 0
        results = await executeSqlStream(sqlRan, queryId, (chunk) => {
          if (chunk.stored) {
            // Store mode: the first message paints, later ones move the count.
            if (chunk.columns) {
              view ??= storedView(runTabId, queryId)
              streamedCols = chunk.columns
              view.begin(chunk.rows, chunk.count)
              if (stillHere()) { sqlColumns = streamedCols; sqlWindowed = true }
              lastPaint = performance.now()
              return
            }
            if (!view) return
            const now = performance.now()
            if (chunk.done || now - lastPaint >= 500) { lastPaint = now; view.grow(chunk.count) }
            return
          }
          // Rows mode: every row comes here. The grid gets a new array (its
          // length is what the grid reads) only when the rows have doubled or
          // three seconds passed, so a 5M-row result is copied ~20 times in
          // all, not once a second.
          if (chunk.offset === 0 || !streamed) { streamed = []; paintedLen = 0 }
          if (chunk.columns) streamedCols = chunk.columns
          for (const row of chunk.rows) streamed.push(row)
          const now = performance.now()
          const due = paintedLen === 0 || streamed.length >= paintedLen * 2 || now - lastPaint >= 3000
          if (!stillHere() || !due) return
          lastPaint = now
          paintedLen = streamed.length
          sqlColumns = streamedCols
          sqlRows = streamed.slice()
        }, { keepInStore: get(appStreamResults) })
        // Each statement's outcome, for the editor marks. A single failed
        // statement is the run failing; in a script the others still ran.
        if (stillHere()) {
          sqlRunOutcomes = results.map((r) => ({ sql: r.sql ?? '', error: r.error ?? null, position: r.errorPosition ?? null }))
        }
        if (results.length === 1 && results[0].error) throw new Error(results[0].error)
        const last = results.at(-1)
        if (view && results.length === 1 && last && !last.rows?.length) {
          // Store mode: the rows stay in the store; the view is the result.
          view.grow(last.rowCount ?? view.count)
          results = [{ ...last, columns: streamedCols.length ? streamedCols : last.columns, rows: view.rows }]
        } else if (streamed && results.length === 1 && last && !last.rows?.length && streamed.length === last.rowCount) {
          // A streamed result's reply has no rows; they are all in `streamed`.
          results = [{ ...last, rows: streamed }]
        } else if (view && runTabId) {
          // The run answered in the reply after all (a script, another engine).
          forgetSqlResult(runTabId)
          view = null
        }
      }
      const data = results.length > 0 ? results[results.length - 1] : {}
      const cols = data.columns ?? []
      const rws = data.rows ?? []
      ranMs = data.query_ms ?? data.queryMs ?? 0
      ranRowCount = rws.length
      let msg = data.message ?? ''
      if (!msg && data.row_count != null && cols.length === 0) {
        msg = `${formatCompactCount(data.row_count)} row(s) affected`
      }
      const stored = runTabId ? _sqlViewsByTab.has(runTabId) : false
      patchSqlTab(runTabId, { sqlColumns: cols, sqlRows: stored ? [] : rws, sqlQueryMs: ranMs, sqlMessage: msg, sqlError: '' })
      if (stillHere()) {
        sqlMultiResults = results.length > 1 ? results : []
        sqlColumns = cols
        sqlRows = rws
        sqlQueryMs = ranMs
        sqlMessage = msg
      }
    } catch (e) {
      ranError = String(e)
      if (runTabId) forgetSqlResult(runTabId)
      patchSqlTab(runTabId, { sqlError: ranError })
      if (stillHere()) {
        sqlError = ranError
        sqlMultiResults = []
        sqlWindowed = false
        sqlRows = []
      }
      if (isNetworkError(ranError)) { connectionLost = true; void silentReconnect() }
    } finally {
      if (runTabId && _sqlQueryIdByTab.get(runTabId) === queryId) _sqlQueryIdByTab.delete(runTabId)
      patchSqlTab(runTabId, { sqlLoading: false })
      if (stillHere()) sqlLoading = false
      recordActivity({ type: 'sql_exec', title: sqlRan.trim().slice(0, 80) + (sqlRan.trim().length > 80 ? '…' : ''), detail: sqlRan, durationMs: ranMs, rowCount: ranRowCount || undefined, success: !ranError, error: ranError || undefined })
      if (persistConnectionId && !ranError) {
        await recordQueryExecution(persistConnectionId, sqlRan, {
          success: true,
          queryMs: ranMs,
        })
        // Settings → Database → Auto-save executed queries. Only successful runs,
        // and deduplicated by SQL, so re-running the statement you're iterating on
        // doesn't push out the ones you saved deliberately.
        if (get(appAutoSaveQueries)) {
          await saveQueryOnce(persistConnectionId, sqlRan).catch(() => {})
        }
        await refreshQueryStores()
      }
    }
  }

  async function onConnected(conn, savedId) {
    recordActivity({ type: 'connect', title: `Connected to ${conn.name ?? conn.database ?? conn.filePath ?? 'database'}`, success: true })
    setWasDisconnected(false)
    connection = conn
    savedConnections = loadSavedConnections()
    tableReadonly = savedConnections.find(c => c.id === savedId)?.readOnly ?? conn.readOnly ?? false
    // Persist last-used ID and bump timestamp
    if (savedId) {
      setLastConnectionId(savedId)
      upsertConnection({ ...conn, id: savedId, lastConnectedAt: Date.now() })
    }
    tableFilter = ''
    error = ''
    activeTable = null
    tables = []
    schemas = []
    // Land on the schema the user was on last time for this connection.
    // loadSchemas() falls back to public/first if it no longer exists.
    activeSchema = (savedId && getLastSchema(savedId)) || 'public'
    // Schema caches are keyed by "schema.table" only, so entries from the old
    // database would be served for same-named tables on the new one (and stay
    // resident forever). The ConnectionModal connect path lands here without
    // going through clearConnectionState, so reset them in both places.
    tableColumnsCache = new Map()
    resetIncomingFkCache()
    _catalog.clear()
    // The connection is live: render the shell NOW (setting `connection` above
    // dropped the reconnect overlay) with the welcome tab open and the sidebar
    // in its skeleton state, and let the catalog stream in below. This is what
    // makes reconnect feel instant - the overlay no longer waits on the
    // schema/table/row-count round trips.
    tabs = []
    _liveRowsByTab.clear()
    _tabRowsMru = []
    _loadSeqByTab.clear()
    fetchingTabIds.clear()
    _sqlQueryIdByTab.clear()
    for (const id of [..._sqlViewsByTab.keys()]) forgetSqlResult(id)
    _sqlRowsByTab.clear()
    _autoRefreshByTab.clear()
    _autoRefreshTick += 1
    _busyJobs.clear()
    _busyTick += 1
    // Redis has no relational catalog: skip schema/table loading entirely and
    // open the keyspace workspace instead of the welcome tab + tables sidebar.
    const connIsRedis = engineFamily(conn.type) === 'redis'
    if (connIsRedis) {
      openRedisTab()
    } else {
      openWelcomeTab()
    }
    loadingTables = true
    // Query history + saved queries only depend on the connection id, not on the
    // catalog, so kick them off concurrently with the schema/table load instead
    // of waiting behind it. Errors are non-fatal (history is best-effort).
    const storesReady = refreshQueryStores().catch(() => {})
    if (!connIsRedis) {
      // Schema → tables is a genuine dependency (loadTables needs activeSchema), so
      // this pair stays serial. The retry loop only sleeps when the backend isn't
      // ready yet (schemas come back empty); the happy path succeeds on attempt 0.
      for (let attempt = 0; attempt < 3; attempt++) {
        if (attempt > 0) await new Promise(r => setTimeout(r, attempt * 700))
        try {
          await loadSchemas()
        } catch (e) {
          // Schema list failed - surface it but keep going so loadTables() still
          // runs and clears the loading flag (sidebar must never hang forever).
          toast.error('Could not load schemas', { description: String(e) })
        }
        if (schemas.length > 0) break
      }
      await loadTables({ force: true })
      // Retry only when the fetch actually failed - an empty database is a valid
      // result and must not pay a 1 s penalty on every connect.
      if (tables.length === 0 && schemas.length > 0 && error) {
        await new Promise(r => setTimeout(r, 1000))
        await loadTables({ force: true })
      }
    } else {
      loadingTables = false
    }
    // MCP autostart is independent of the catalog - don't block first render on it.
    void (async () => {
      try {
        if (loadSettings().mcpAutoStart) {
          const s = await mcpStart()
          mcpRunning = s.running
        }
      } catch { /* ignore */ }
    })()
    await storesReady
  }

  onMount(() => installInputShortcuts())

  // Warm the lazy page/panel chunks during browser idle time so the first
  // navigation to a tab is instant instead of paying a cold chunk fetch+parse.
  // We warm ONE per idle slot - never blocking interaction. Ordered by how
  // commonly each is opened; the monaco-backed editors come first since they
  // dominate latency. If the user opens a page sooner, import() dedups to the
  // same promise and resolves immediately. Fire-and-forget; failures are harmless.
  //
  // Measured on a release build against the manifest's static import graph -
  // warming a chunk pulls its static imports, its own dynamic imports stay lazy.
  // The eager entry graph is 2.71MB/25 chunks; the full warm set adds 6.01MB/49.
  // But 5.35MB of that is two entries: SqlConsole drags in monaco (3.78MB) and
  // AiChat the markdown/highlight stack (1.57MB). The other 22 pages cost 0.66MB
  // between them, 0.01-0.11MB each - so trimming that tail buys nothing and only
  // costs first-open latency, which is why it is all still here.
  //
  // What is worth skipping is whatever this engine cannot open at all. A warmed
  // chunk is never freed again (which already sits badly beside the idle-teardown
  // above), and on a Redis connection every relational page is unreachable UI -
  // monaco included, so ~4.3MB of the 6.01MB was being pinned for tabs that do
  // not exist. Hence the gate per entry, and hence waiting for a connection:
  // before one exists the engine is unknown and no tab can be opened anyway.
  //
  // Keep these specifiers identical to the {#await import('./X.svelte')} blocks
  // below so Vite resolves them to the same chunk.
  $effect(() => {
    if (!connection) return
    // [reachable on this engine, loader]. Gates mirror the tab affordances in
    // the empty-state grid and the palette; keep them in sync when a page moves.
    /** @type {Array<[boolean, () => Promise<unknown>]>} */
    const candidates = [
      [isRedis,          () => import('./RedisKeyspacePage.svelte')], // the only page Redis has
      [!isRedis,         () => import('./SqlConsole.svelte')],        // monaco
      [true,             () => import('./AiSidebar.svelte')],         // marked + shiki
      [true,             () => import('./AiChat.svelte')],            // marked + shiki
      [!isRedis,         () => import('./OrmRunner.svelte')],         // monaco
      [!isRedis,         () => import('./TableJsonView.svelte')],     // monaco - data view mode
      [!isRedis,         () => import('./TableTextView.svelte')],     // monaco - data view mode
      [!isRedis,         () => import('./StructureView.svelte')],
      [hasSchemaExplorer, () => import('./SchemaPage.svelte')],
      [!isRedis,         () => import('./ChartsPage.svelte')],        // echarts
      [!isRedis,         () => import('./DashboardPage.svelte')],
      [hasSecurity,      () => import('./SecurityPage.svelte')],
      [!isRedis,         () => import('./SearchPage.svelte')],
      [!isRedis,         () => import('./InstanceInsightsPage.svelte')],
      [!isRedis,         () => import('./ObjectsPage.svelte')],
      [!isRedis,         () => import('./DiagramsPage.svelte')],      // echarts
      [!isRedis,         () => import('./EntityRelationPage.svelte')],
      [!isRedis,         () => import('./DataDiffPage.svelte')],      // monaco
      [!isRedis,         () => import('./NotebookEditor.svelte')],
      [!isRedis,         () => import('./JsonViewerPage.svelte')],
      [!isRedis,         () => import('./ExtensionsPage.svelte')],
      [!isRedis,         () => import('./BackupPage.svelte')],
      [true,             () => import('./LogsPage.svelte')],
      [!isRedis,         () => import('./SchemaTimelinePage.svelte')],
    ]
    const warmers = candidates.filter(([reachable]) => reachable).map(([, load]) => load)
    const ric = window.requestIdleCallback ?? ((/** @type {Function} */ fn) => setTimeout(() => fn(), 200))
    const cancel = window.cancelIdleCallback ?? clearTimeout
    let i = 0
    let handle = 0
    const pump = () => {
      if (i >= warmers.length) return
      warmers[i++]().catch(() => {})
      handle = ric(pump, { timeout: 3000 })
    }
    handle = ric(pump, { timeout: 3000 })
    return () => cancel(handle)
  })

  onMount(async () => {
    // Resolve every saved host before the user can pick one. A cold DNS lookup
    // measured 4147ms here against 58ms warm, and the connect waits on it, so
    // this is the difference between a 4s connect and a 300ms one. Fire and
    // forget - nothing downstream waits on it.
    // Repeat on an interval, not just once: resolver cache entries expire, and a
    // cold lookup on this machine has been measured stalling the full 5s DNS
    // budget - for unrelated hosts at the same instant, so it is the resolver,
    // not the database. Re-resolving every 60s keeps the entry hot so the connect
    // lands on the ~260ms path instead of the ~6.9s one. Fire and forget.
    const warmHosts = () => {
      try {
        // A PostHog host is a base URL, not a hostname: resolve the name inside it.
        const hostname = (/** @type {string} */ h) => { try { return h.includes('://') ? new URL(h).hostname : h } catch { return '' } }
        const hosts = [...new Set(loadSavedConnections().map((c) => hostname(c.host ?? '')).filter(Boolean))]
        if (hosts.length) void prewarmDns(hosts)
      } catch { /* best effort */ }
    }
    warmHosts()
    const dnsWarmTimer = setInterval(warmHosts, 60_000)
    onDestroy(() => clearInterval(dnsWarmTimer))

    // Seed the sample SQLite database once on first launch (any install, any user).
    // Uses a sentinel key so re-seeding is skipped if the user later deletes the connection.
    try {
      if (!localStorage.getItem(SAMPLE_SEEDED_KEY)) {
        const filePath = await initSampleDb()
        upsertConnection({ id: SAMPLE_DB_ID, type: 'sqlite', name: 'Sample Database', filePath })
        savedConnections = loadSavedConnections()
        localStorage.setItem(SAMPLE_SEEDED_KEY, '1')
      }
    } catch {
      // Non-critical - don't block app start if Tauri backend unavailable (browser dev)
    }

    // Every branch below reveals the page on its own first screen (onboarding,
    // the connection modal, the welcome screen), except the startup reconnect,
    // which holds the window back until the shell has its tables - see there.

    // First-time user - show onboarding instead of bare connection modal
    try {
      if (!localStorage.getItem(ONBOARDING_KEY)) {
        showOnboarding = true
        revealApp()
        return
      }
    } catch {}

    const last = getLastConnection()
    if (!last) { showConnectionModal = true; revealApp(); return }

    // Disconnect is a decision, and it survives a restart. Coming back connected
    // to the database someone deliberately stepped away from - on a reload, or
    // the next morning - is the one outcome that command exists to prevent, so
    // this launch stays on the welcome screen. The connection is still saved and
    // still the highlighted one; reconnecting is a click.
    if (wasDisconnected()) { revealApp(); return }

    // Respect the "auto reconnect on startup" setting - if disabled, go straight
    // to the connection modal instead of re-connecting silently.
    if (!loadSettings().autoReconnectOnStartup) { showConnectionModal = true; revealApp(); return }

    beginConnectOverlay(last.name ?? '', 'Reconnecting')
    // A warm reconnect and its table list land in a few hundred ms. Revealing
    // on the overlay meant three screens in that time - "Reconnecting", an empty
    // shell with a skeleton sidebar, then the tables - which is the startup
    // flicker. Stay hidden until the tables are in (the effect watching
    // `revealOnTables`) and only fall back to the overlay when it runs long.
    revealOnTables = true
    setTimeout(revealApp, RECONNECT_REVEAL_MS)
    // The backend already enforces its own per-engine deadlines (DNS + TCP preflight,
    // a 20s connect deadline for Postgres, HTTP timeouts for the REST engines) and
    // fails with a message the user can act on. This race is only a last-resort guard
    // for a command that never returns AT ALL, so it has to sit above those deadlines.
    // At 5s it fired first and turned every slow-but-healthy wake-up into "not
    // connected" - serverless Postgres (Neon, and Prisma/Supabase pooler cold starts)
    // autosuspends and takes 3-15s to come back, which is why resuming one meant
    // reconnecting by hand on nearly every launch.
    /** @param {Promise<unknown>} p */
    const withTimeout = (p) => Promise.race([
      p,
      new Promise((_, reject) => setTimeout(() => reject(new Error('Connection timed out')), 45_000)),
    ])
    try {
      if (last.type === 'sqlite') await withTimeout(connectSqlite(last))
      else if (last.type === 'd1') await withTimeout(connectD1(last))
      else if (last.type === 'libsql') await withTimeout(connectLibSql(last))
      else if (last.type === 'mysql' || last.type === 'mariadb') await withTimeout(connectMysql(last))
      else if (last.type === 'clickhouse') await withTimeout(connectClickhouse(last))
      else if (last.type === 'posthog') await withTimeout(connectPosthog(last))
      else if (last.type === 'duckdb') await withTimeout(connectDuckdb(last))
      else if (last.type === 'mssql') await withTimeout(connectMssql(last))
      else if (last.type === 'redis') await withTimeout(connectRedis(last))
      else await withTimeout(connectPostgres(last))
      // onConnected already refreshes the query stores (concurrently with the
      // catalog load) - no second fetch needed here.
      if (autoConnectCancelled) { try { await disconnectPostgres() } catch { /* ignore */ } return }
      await onConnected(last, last.id)
    } catch {
      if (autoConnectCancelled) return
      // Racing the timeout abandons the promise but not the backend: a connect that
      // lands after we gave up would leave a live pool behind a UI that says
      // "Not connected", and the next attempt would stack a second one on top.
      try { await disconnectPostgres() } catch { /* nothing to tear down */ }
      showConnectionModal = true
    } finally {
      autoConnecting = false
      // Failed, cancelled or timed out: whatever screen is up now is final.
      revealApp()
    }
  })

  async function handleSchemaChange(schema) {
    if (!schema || schema === activeSchema) return
    if (connectionLost) await reconnectPool()
    activeSchema = schema
    setLastSchema(persistConnectionId, schema)
    activeTable = null
    page = 1
    tableFilter = ''
    columns = []
    rows = []
    await loadTables()
    if (activeTab?.kind === 'table') {
      if (tables.length) {
        activeTable = tables[0].name
        await loadRows()
      } else {
        saveActiveTabState()
      }
    }
  }

  async function handleTableSelect(name) {
    // Switching tables should transparently heal a dropped connection.
    if (connectionLost) await reconnectPool()
    recordActivity({ type: 'table_open', title: `Opened ${name}`, schema: activeSchema, table: name, success: true })
    await openTableTab(activeSchema, name)
  }

  function enterAiMode() {
    if (!connection) return
    aiMode = true
    saveAiMode(true)
  }

  function exitAiMode() {
    aiMode = false
    saveAiMode(false)
  }

  /** @param {{ provider: string, dbRef: string, name: string }} args */
  async function switchProviderDb({ provider, dbRef, name }) {
    if (!connection) return
    try {
      const { providerBuildConnection } = await import('$lib/providers.js')
      const built = await providerBuildConnection(provider, dbRef)
      if (built.needs_password) {
        // Supabase needs a per-project password - can't switch silently, so
        // send the user to the connect dialog to finish it.
        toast.message(`${name} needs its database password, opening the connection dialog.`)
        showConnectionModal = true
        return
      }
      void handleSwitchDatabase({
        ...connection,
        type: built.db_type === 'mysql' ? 'mysql' : 'postgres',
        host: built.host,
        port: built.port,
        user: built.username,
        password: built.password,
        database: built.database,
        ssl: built.ssl,
        name: built.name,
        provider,
      })
    } catch (e) {
      toast.error('Could not switch database', { description: String(e) })
    }
  }

  /** @param {{ databaseId: string, name: string }} args */
  function switchD1Database({ databaseId, name }) {
    if (!connection) return
    void handleSwitchDatabase({ ...connection, databaseId, database: name, name })
  }

  /** @param {string} dbName */
  function switchToDb(dbName) {
    if (!connection) return
    void handleSwitchDatabase({ ...connection, database: dbName, name: `${connection.host ?? connection.name}/${dbName}` })
  }

  function requestDisconnect() {
    showDisconnectDialog = true
  }

  // ── Switch database (sidebar) ──────────────────────────────────────────────
  let showSwitchDbDialog = $state(false)
  /** @type {{ key: string, label: string } | null} */
  let pendingDbSwitch = $state(null)

  /** Ask first: switching drops the pool, the catalog and every open tab. */
  function requestDatabaseSwitch(/** @type {{ key: string, label: string }} */ entry) {
    pendingDbSwitch = entry
    showSwitchDbDialog = true
  }

  /** Double-click a database: switch straight there, no confirm step. The dialog
   *  exists because switching drops the open tabs, but once you have done it a
   *  few times the confirm is just a second click - so the second click IS the
   *  confirmation. Single click still asks. */
  function switchDatabaseNow(/** @type {{ key: string, label: string }} */ entry) {
    pendingDbSwitch = entry
    showSwitchDbDialog = false
    commitDatabaseSwitch()
  }

  /** Same three dispatch paths the status-bar switcher uses, by engine. */
  function commitDatabaseSwitch() {
    const entry = pendingDbSwitch
    pendingDbSwitch = null
    if (!entry || !connection) return
    if (connection.provider) return void switchProviderDb({ provider: connection.provider, dbRef: entry.key, name: entry.label })
    if (connection.type === 'd1') return void switchD1Database({ databaseId: entry.key, name: entry.label })
    switchToDb(entry.label)
  }

  // ── Manage databases (sidebar) ────────────────────────────────────────────
  // Create, rename, duplicate, drop, and the read-only info panel. Every
  // statement runs on the current pool, which is why none of them can target the
  // database this session is attached to - `dbActionBlocker` in database-admin.js
  // is what greys those items out, and the handlers below only ever see targets
  // that passed it.
  let showCreateDbDialog = $state(false)
  /** Bumped after any change, so the sidebar refetches its list. */
  let databasesRefreshKey = $state(0)
  /** @type {{ mode: 'rename' | 'duplicate', source: string, existing: string[] } | null} */
  let dbNameDialog = $state(null)
  let dbNameDialogOpen = $state(false)
  let dropDbName = $state('')
  let dropDbSessions = $state('')
  let showDropDbDialog = $state(false)
  let dbInfoName = $state('')
  let showDbInfoDialog = $state(false)
  let dbInfoLoading = $state(false)
  let dbInfoError = $state('')
  /** @type {import('$lib/database-admin.js').DbInfoRow[]} */
  let dbInfoRows = $state([])

  const dbAdmin = $derived(dbAdminKind(connection))

  /** @param {import('./CreateDatabaseDialog.svelte').CreateDbOptions} opts */
  async function createDatabase(opts) {
    const kind = dbAdmin ?? 'postgres'
    await executeDdl(createDatabaseSql(kind, opts))
    toast.success(`Database "${opts.name}" created`)
    databasesRefreshKey++
  }

  /** @param {{ mode: 'rename' | 'duplicate', name: string, existing: string[] }} args */
  function openDbNameDialog({ mode, name, existing }) {
    dbNameDialog = { mode, source: name, existing }
    dbNameDialogOpen = true
  }

  /** Shared by rename and duplicate - the dialog has already built the SQL.
   *  Throwing keeps the dialog open with the server's message.
   *  @param {{ sql: string, name: string }} args */
  async function runDbNameStatement({ sql, name }) {
    const mode = dbNameDialog?.mode
    const source = dbNameDialog?.source ?? ''
    await executeDdl(sql)
    toast.success(mode === 'rename' ? `Renamed "${source}" to "${name}"` : `Copied "${source}" to "${name}"`)
    databasesRefreshKey++
  }

  /** Look up how many sessions are on a database, for the drop dialog's warning.
   *  Best effort: a failed count must not block the dialog. @param {string} name */
  async function countDbSessions(name) {
    if (dbAdmin !== 'postgres') return ''
    try {
      const r = await executeSql(`SELECT count(*) FROM pg_stat_activity WHERE datname = '${name.replace(/'/g, "''")}'`)
      const n = Number(r?.rows?.[0]?.[0] ?? 0)
      return n > 0 ? String(n) : ''
    } catch {
      return ''
    }
  }

  /** @param {{ name: string }} args */
  async function requestDropDatabase({ name }) {
    dropDbName = name
    dropDbSessions = ''
    showDropDbDialog = true
    dropDbSessions = await countDbSessions(name)
  }

  /** @param {{ sql: string, force: boolean }} args */
  async function commitDropDatabase({ sql }) {
    const name = dropDbName
    try {
      await executeDdl(sql)
      toast.success(`Database "${name}" dropped`)
      databasesRefreshKey++
    } catch (e) {
      toast.error(`Could not drop "${name}"`, { description: String(e) })
    }
  }

  /** @param {{ name: string }} args */
  async function terminateDbSessions({ name }) {
    if (!dbAdmin) return
    try {
      const r = await executeSql(terminateSessionsSql(dbAdmin, name))
      const closed = r?.rows?.length ?? 0
      toast.success(closed === 0 ? `No other sessions on "${name}"` : `Closed ${closed} session(s) on "${name}"`)
    } catch (e) {
      toast.error(`Could not close sessions on "${name}"`, { description: String(e) })
    }
  }

  /** @param {{ name: string }} args */
  async function openDatabaseInfo({ name }) {
    if (!dbAdmin) return
    dbInfoName = name
    dbInfoRows = []
    dbInfoError = ''
    dbInfoLoading = true
    showDbInfoDialog = true
    try {
      const result = await executeSql(databaseInfoSql(dbAdmin, name))
      dbInfoRows = databaseInfoRows(dbAdmin, result)
    } catch (e) {
      dbInfoError = String(e)
    } finally {
      dbInfoLoading = false
    }
  }

  /** Reset all connection-scoped UI state to blank. */
  function clearConnectionState() {
    schemas = []
    tables = []
    indexes = []
    enums = []
    triggers = []
    sequences = []
    tableColumnsCache = new Map()
    resetIncomingFkCache()
    _catalog.clear()
    _sqlHintsLoadedFor = ''
    activeSchema = 'public'
    activeTable = null
    tableFilter = ''
    tableViewMode = 'data'
    structureColumns = []
    structureSearch = ''
    loadingStructure = false
    recentTabs = []
    resetTabs()
  }

  /** @param {{ forget?: boolean }} [opts] `forget` skips remembering the schema,
   *  for a connection that was just deleted. */
  async function handleDisconnect({ forget = false } = {}) {
    // Remember where the user was so reconnecting restores this schema.
    if (!forget && persistConnectionId && activeSchema) setLastSchema(persistConnectionId, activeSchema)
    recordActivity({ type: 'disconnect', title: `Disconnected from ${connection?.name ?? 'database'}`, success: true })
    setWasDisconnected(true)
    try { await disconnectPostgres() } catch { /* ignore */ }
    try { await mcpStop() } catch { /* ignore */ }
    mcpRunning = false
    connection = null
    clearConnectionState()
    showConnectionModal = true
  }

  /**
   * A saved connection was deleted (sidebar or connection modal). Deleting the
   * one that is open also ends its session: staying connected to a connection
   * that no longer exists left nothing to reconnect or switch back to.
   * @param {string} id
   */
  function handleConnectionRemoved(id) {
    savedConnections = loadSavedConnections()
    if (connection && id && (id === persistConnectionId || id === connection.id)) {
      void handleDisconnect({ forget: true })
    }
  }

  /** @param {{ db_type: string, host: string, port: number, user: string, password: string, database: string, name: string }} info */
  async function handleDockerConnect(info) {
    const conn = /** @type {import('$lib/stores/connections.js').SavedConnection} */ ({
      id: crypto.randomUUID(),
      type: info.db_type === 'mysql' ? 'mysql' : 'postgres',
      name: info.name,
      host: info.host,
      port: info.port,
      user: info.user,
      password: info.password,
      database: info.database,
      ssl: false,
    })
    upsertConnection(conn)
    await disconnectPostgres().catch(() => {})
    connection = null
    clearConnectionState()
    beginConnectOverlay(conn.name ?? '')
    try {
      if (conn.type === 'mysql' || conn.type === 'mariadb') await connectMysql(conn)
      else await connectPostgres(conn)
      await onConnected(conn, conn.id)
      showDockerModal = false
    } catch (e) {
      error = String(e)
      showDockerModal = false
      showConnectionModal = true
    } finally {
      autoConnecting = false
    }
  }

  async function handleSampleConnect() {
    await disconnectPostgres().catch(() => {})
    connection = null
    clearConnectionState()
    beginConnectOverlay('Sample Database')
    try {
      const filePath = await initSampleDb()
      const sample = /** @type {import('$lib/stores/connections.js').SavedConnection} */ ({
        id: SAMPLE_DB_ID, type: 'sqlite', name: 'Sample Database', filePath,
      })
      // Clicking Sample Database is an explicit re-add, so it may bring back a
      // sample connection the user deleted earlier.
      upsertConnection(sample, { revive: true })
      savedConnections = loadSavedConnections()
      localStorage.setItem(SAMPLE_SEEDED_KEY, '1')
      await connectSqlite(sample)
      await onConnected(sample, sample.id)
    } catch (e) {
      error = String(e)
      showConnectionModal = true
    } finally {
      autoConnecting = false
    }
  }

  /**
   * Re-establish the backend pool for a saved connection, dispatched by engine.
   * Shared by the explicit switch flow and the silent auto-reconnect path.
   * @param {import('$lib/stores/connections.js').SavedConnection} conn
   */
  async function connectByType(conn) {
    // Which engines get used, never which servers. The event name is the whole
    // payload - there is no field here that could carry a host or a database.
    track(`connect_${conn.type === 'cockroachdb' ? 'cockroachdb' : conn.type}`)
    if (conn.type === 'sqlite') await connectSqlite(conn)
    else if (conn.type === 'd1') await connectD1(conn)
    else if (conn.type === 'libsql') await connectLibSql(conn)
    else if (conn.type === 'mysql' || conn.type === 'mariadb') await connectMysql(conn)
    else if (conn.type === 'clickhouse') await connectClickhouse(conn)
    else if (conn.type === 'posthog') await connectPosthog(conn)
    else if (conn.type === 'duckdb') await connectDuckdb(conn)
    else if (conn.type === 'mssql') await connectMssql(conn)
    else if (conn.type === 'redis') await connectRedis(conn)
    else await connectPostgres(conn)
  }

  /** @param {import('$lib/stores/connections.js').SavedConnection} conn */
  async function handleSwitchDatabase(conn) {
    // A deliberate switch is the other half of "PIN to open or reconnect".
    // Resolves true immediately unless a PIN is set with the connect prompt on.
    if (!(await requireUnlock('Unlock to connect to a database'))) return
    // Disconnect current before connecting to the new one to avoid the race
    // where set_conn(None) could fire after connect_* sets the new connection.
    await disconnectPostgres().catch(() => {})
    connection = null
    clearConnectionState()
    // Connect to the chosen saved connection
    beginConnectOverlay(conn.name ?? conn.database ?? conn.host ?? '')
    try {
      await connectByType(conn)
      await onConnected(conn, conn.id)
      // The switch landed, so the sidebar goes back to the tables of the
      // database you are now in. Every route into a switch - the Databases tab,
      // the status bar, the command palette, a context menu, another saved
      // connection - comes through here, so this is the one place that has to
      // say it. The list left on screen belongs to a database nobody is looking
      // at any more, and the Databases tab is a switcher, not a destination.
      sidebarShowTables()
    } catch (e) {
      error = String(e)
      showConnectionModal = true
    } finally {
      autoConnecting = false
    }
  }

  // ── Silent auto-reconnect ──────────────────────────────────────────────────
  // Heals a dropped connection IN PLACE - rebuilds the backend pool for the
  // CURRENT connection config, with no UI teardown, no full-screen overlay, and
  // no toast. The health monitor, an `online` event, a table switch, a refresh,
  // or a failed query can all trigger this, so the app recovers by itself instead
  // of nagging the user with a "Connection lost / Reconnect" popup.
  let _reconnecting = false
  let _lastReconnectAt = 0
  /** Rebuild the pool for the active connection. Returns true on success. */
  async function reconnectPool() {
    const conn = connection
    if (!conn) return false
    try {
      await connectByType(conn)
      connectionLost = false
      return true
    } catch {
      return false
    }
  }
  /** Drives the spinner on the error state's Retry / Reconnect button. */
  let retryingLoad = $state(false)
  /**
   * Retry whatever the table view failed at. A dropped pool needs rebuilding
   * first - retrying the same query against a closed pool just reprints the
   * same error.
   */
  async function retryTableLoad() {
    if (retryingLoad) return
    const dropped = connectionErrorKind(error) === 'dropped' || connectionLost
    retryingLoad = true
    try {
      error = ''
      showRawError = false
      if (dropped) await reconnectPool()
      // No table selected means the failure came from the catalog, not a row
      // fetch, so retry what actually broke.
      if (activeTable) await loadRows()
      else await loadTables({ force: true })
    } finally {
      retryingLoad = false
    }
  }

  /** Rate-limited silent reconnect + quiet refetch, for background triggers. */
  async function silentReconnect() {
    if (_reconnecting || !connection) return
    // Coalesce bursts of triggers (online + visibilitychange + ping fire together)
    // and stop a still-unreachable host from hammering connect_* in a tight loop.
    const now = Date.now()
    if (now - _lastReconnectAt < 3000) return
    _lastReconnectAt = now
    _reconnecting = true
    try {
      if (await reconnectPool()) {
        error = ''
        if (activeTab?.kind === 'table' && activeTable) await loadRows().catch(() => {})
        else await loadTables().catch(() => {})
      }
    } finally {
      _reconnecting = false
    }
  }

  async function handleRefresh() {
    // A manual refresh should also recover a dropped connection.
    if (connectionLost) await reconnectPool()
    _sqlHintsLoadedFor = '' // re-fetch enum/function hints on next SQL view
    // Where the user was reading, captured before anything can move it. A
    // refresh re-runs the SAME query in the same order on the same page, so the
    // position is still meaningful - unlike a page / sort / filter change, which
    // is a different view and rightly starts at the top.
    const onTable = activeTab?.kind === 'table' && !!activeTable
    const at = onTable ? (tableGetScroll?.() ?? { left: 0, top: 0 }) : null
    await loadSchemas()
    await loadTables({ force: true })
    if (onTable) {
      await loadRows({ keepScroll: true })
      // Reasserted rather than merely left alone: reloading the schema and table
      // lists on the way through re-applies the active tab's snapshot, and that
      // restore lands whenever its tick fires - after this point, with whatever
      // scroll the snapshot happened to hold. Setting it last is what makes the
      // position survive the whole refresh, both axes.
      await tick()
      tableApplyScroll?.({ left: at?.left ?? 0, top: at?.top ?? 0 })
    }
  }

  /** @param {string} tableName */
  async function handleTruncateTable(tableName) {
    try {
      await truncateTable(activeSchema, tableName)
      toast.success(`Truncated "${tableName}"`)
      if (activeTable === tableName) await loadRows()
    } catch (err) {
      toast.error('Could not truncate table', { description: String(err) })
    }
  }

  /**
   * @param {string} tableName
   * @param {boolean} [cascade]
   */
  async function handleDropTable(tableName, cascade = false) {
    try {
      await dropTable(activeSchema, tableName, cascade)
      toast.success(`Dropped table "${tableName}"`)
      await loadTables({ force: true })
      if (activeTable === tableName) {
        activeTable = null
      }
    } catch (err) {
      toast.error('Could not drop', { description: String(err) })
    }
  }

  /** @param {string} tableName */
  async function handleViewDdl(tableName) {
    try {
      const ddl = await getTableDdl(activeSchema, tableName)
      if (aiMode) exitAiMode()
      openDdlTab(ddl.endsWith('\n') ? ddl : `${ddl}\n`, `DDL · ${tableName}`)
    } catch (e) {
      toast.error('Could not load DDL', { description: String(e) })
    }
  }

  /** @param {string} tableName */
  async function handleExportSql(tableName) {
    try {
      toast.info('Preparing SQL export…')
      const [ddl, result] = await Promise.all([
        getTableDdl(activeSchema, tableName),
        getTableRows(activeSchema, tableName, 100000, 0),
      ])
      const cols = result.columns ?? []
      const rows = result.rows ?? []

      // Gate on rows.length: rowsToSql returns a "-- no rows" comment for empty
      // input, but an empty table should export as DDL only.
      const inserts = rows.length ? rowsToSql(cols, rows, tableName) : ''
      const sql = inserts ? `${ddl}\n\n${inserts}` : ddl
      const filename = `${tableName}_${new Date().toISOString().slice(0, 10)}.sql`
      await saveExportFile(sql, filename, 'sql')
    } catch (e) {
      toast.error('Could not export', { description: String(e) })
    }
  }

  /** @param {string} tableName */
  async function handleExportData(tableName) {
    try {
      toast.info('Preparing CSV export…')
      const result = await getTableRows(activeSchema, tableName, 100000, 0)
      const cols = result.columns ?? []
      const rows = result.rows ?? []
      const csv = rowsToCsv(cols, rows)
      const filename = buildExportFilename(tableName, 'csv')
      await saveExportFile(csv, filename, 'csv')
    } catch (e) {
      toast.error('Could not export', { description: String(e) })
    }
  }

  /** @param {string} tableName */
  async function handleGenerateTestData(tableName) {
    try {
      const cols = await getTableColumnStructure(activeSchema, tableName)
      const colNames = cols.map((c) => `"${c.name}"`).join(',\n  ')
      const colPlaceholders = cols.map((c) => {
        const t = columnType(c)
        if (t.includes('int') || t.includes('serial')) return '0'
        if (t.includes('bool')) return 'true'
        if (t.includes('date') || t.includes('timestamp')) return `'${new Date().toISOString().slice(0, 10)}'`
        if (t.includes('numeric') || t.includes('float') || t.includes('double') || t.includes('decimal')) return '0.0'
        return `'sample_${c.name}'`
      }).join(',\n  ')
      const sql = `-- Generated test data template for ${tableName}\nINSERT INTO "${tableName}" (\n  ${colNames}\n) VALUES (\n  ${colPlaceholders}\n);`
      await openQueryInEditor(sql)
    } catch (e) {
      toast.error('Could not generate test data', { description: String(e) })
    }
  }

  /** @param {number} rowIdx */
  function primaryKeyForRow(rowIdx) {
    const row = rows[rowIdx]
    if (!row) return null

    /** @type {Record<string, unknown>} */
    const pk = {}
    for (const key of primaryKey) {
      const keyIdx = columns.findIndex((c) => c.name === key)
      if (keyIdx < 0) throw new Error(`Primary key column not found: ${key}`)
      pk[key] = row[keyIdx]
    }
    return pk
  }

  /** @param {number[]} rowIndices */
  async function handleDeleteRows(rowIndices) {
    if (!activeTable || !primaryKey.length || rowIndices.length === 0) return

    const removed = [...new Set(rowIndices)].sort((a, b) => a - b)
    const primaryKeys = removed
      .map((idx) => primaryKeyForRow(idx))
      .filter((pk) => pk !== null)

    if (primaryKeys.length === 0) return

    deletingRows = true
    const _deleteStart = Date.now()
    try {
      const deleted = await deleteTableRows(activeSchema, activeTable, primaryKeys)
      if (deleted === 0) {
        throw new Error('No rows deleted (they may have changed)')
      }
      recordActivity({ type: 'row_delete', title: `Deleted ${deleted} row${deleted === 1 ? '' : 's'} from ${activeTable}`, schema: activeSchema, table: activeTable, rowCount: deleted, durationMs: Date.now() - _deleteStart, success: true })

      const removedSet = new Set(removed)
      rows = rows.filter((_, i) => !removedSet.has(i))
      total = Math.max(0, total - deleted)

      selected = remapRowIndexSet(selected, removed)
      focusedRow = remapNullableRowIndex(focusedRow, removed)
      inspectorRow = remapNullableRowIndex(inspectorRow, removed)

      if (editingCell && removedSet.has(editingCell.rowIdx)) {
        editingCell = null
      } else if (editingCell) {
        editingCell = {
          ...editingCell,
          rowIdx: remapNullableRowIndex(editingCell.rowIdx, removed) ?? editingCell.rowIdx,
        }
      }

      // The splice above only removes rows already resident on the client. On a
      // paginated/windowed table that leaves the view empty (or showing a stale
      // total) while thousands of rows remain server-side, so refetch the current
      // page - this also refreshes the true COUNT(*).
      if (pageSize !== PAGE_SIZE_ALL && total > 0 && (page - 1) * pageSize >= total) {
        // The current page fell past the end after the deletion - step back.
        page = Math.max(1, Math.ceil(total / pageSize))
        rawOffset = null
      }
      saveActiveTabState()
      await loadRows({ keepScroll: true })
    } catch (err) {
      toast.error('Could not delete rows', { description: String(err) })
      throw err
    } finally {
      deletingRows = false
    }
  }

  /** @param {{ rowIndices: number[] }} detail */
  async function handleDeleteRow(detail) {
    await handleDeleteRows(detail.rowIndices)
  }

  /** @param {Record<string, unknown>} values */
  async function handleInsertRow(values) {
    if (!activeTable) return

    insertingRow = true
    const _insertStart = Date.now()
    try {
      const { row } = await insertTableRow(activeSchema, activeTable, values)
      recordActivity({ type: 'row_insert', title: `Inserted row into ${activeTable}`, schema: activeSchema, table: activeTable, durationMs: Date.now() - _insertStart, success: true })

      const hasActiveFilters =
        rowSearch.trim() !== '' || activeFilters(rowFilters).length > 0
      // Only splice the row in when the result is trustworthy. Two cases made a
      // successful insert look like nothing happened until a manual refresh:
      // - "All" (windowed) mode: the grid draws `_windowCount` rows from a
      //   sparse array, so prepending grew the total but not what was drawn.
      // - An incomplete row back from the driver (MySQL can't RETURNING; when it
      //   can't re-read the row the id comes back NULL), which is not the row
      //   the table holds.
      // A sort that isn't the default can also place the row elsewhere. In each
      // of these the page is reloaded instead: one query, always correct.
      const pkIdx = (primaryKey ?? []).map((k) => columns.findIndex((c) => c.name === k))
      const rowComplete =
        Array.isArray(row) &&
        row.length === columns.length &&
        pkIdx.every((i) => i >= 0 && row[i] !== null && row[i] !== undefined)
      const canSplice = !windowed && rowComplete && !rowSort

      if (!hasActiveFilters && page === 1 && canSplice) {
        rows = [row, ...rows]
        if (rows.length > effectivePageSize) {
          rows = rows.slice(0, effectivePageSize)
        }
        total += 1
        saveActiveTabState()
        toast.success('Row inserted')
      } else {
        // This branch does not adjust `total` (whether the new row matches the
        // active filter is the database's call), so the count has to be redone.
        invalidateRowCount()
        await loadRows()
        toast.success('Row inserted', {
          description: hasActiveFilters
            ? 'Refresh filters or go to page 1 if the row is not visible'
            : undefined,
        })
      }
    } catch (err) {
      toast.error('Could not insert row', { description: String(err) })
      throw err
    } finally {
      insertingRow = false
    }
  }

  /**
   * Run raw DML that the user hand-edited in the "Review changes" preview, then
   * refetch the current page so the grid reflects it.
   * @param {string} sql
   */
  async function handleExecuteRawDml(sql) {
    if (!sql.trim()) return
    const _start = Date.now()
    try {
      await executeSqlMulti(sql)
      // Hand-written DML: rows may have been inserted or deleted, or edited so
      // they no longer match the active filter. Nothing here can be assumed.
      invalidateRowCount()
      recordActivity({ type: 'row_save', title: `Applied edited SQL${activeTable ? ` on ${activeTable}` : ''}`, schema: activeSchema, table: activeTable ?? '', durationMs: Date.now() - _start, success: true })
      await loadRows()
      toast.success('Changes applied')
    } catch (err) {
      recordActivity({ type: 'row_save', title: `Failed edited SQL${activeTable ? ` on ${activeTable}` : ''}`, schema: activeSchema, table: activeTable ?? '', success: false, error: String(err) })
      toast.error('Could not apply changes', { description: String(err) })
      throw err
    }
  }

  /**
   * Load one capped cell in full, for the dock.
   *
   * A browse page fetches wide columns as a preview - the whole point of that
   * is not moving half a megabyte per row for a grid that draws forty
   * characters - so reading one is an explicit, per-row request.
   * @param {{ rowIdx: number, colIdx: number }} detail
   */
  async function handleFetchCellValue(detail, maxBytes = DOCK_VALUE_MAX) {
    if (!activeTable) throw new Error('No table is open')
    const col = columns[detail.colIdx]
    if (!col) throw new Error('That column is gone')
    const pk = primaryKeyForRow(detail.rowIdx)
    if (!pk) throw new Error('This table has no primary key, so a single row cannot be addressed')
    return await fetchCellValue(activeSchema, activeTable, pk, col.name, maxBytes)
  }

  /**
   * What a load will hold, in the dock and in the cell alike. The fetch command
   * can return far more, but a `<textarea>` in a webview cannot lay out tens of
   * megabytes - 8MB is already past anything anyone reads and is the point where
   * the panel stays usable.
   *
   * The cell used to stop at 1MB on the theory that its value is what the canvas
   * formats and the search walks. It is, but neither reads more than the forty
   * characters a cell is wide: the drawn text is cut to 400 chars and cached per
   * row, the highlighter matches that same cut string, and the row search runs
   * in SQL. A 1.2MB resume refusing to load into the cell it belongs to was that
   * theory charging for a cost it was not paying.
   */
  const DOCK_VALUE_MAX = 8 * 1024 * 1024
  const CELL_VALUE_MAX = DOCK_VALUE_MAX
  /**
   * Above this, a loaded JSON value stays the text it arrived as rather than
   * being parsed into an object. Drawing an object means stringifying it, so a
   * parsed 6MB payload is a 6MB parse on the way in and a 6MB serialize on the
   * way out, both blocking, to show forty characters - and it retains both
   * copies. As text it is a slice. The dock reads JSON out of text perfectly
   * well, so nothing downstream loses anything.
   */
  const CELL_PARSE_MAX = 1024 * 1024

  /**
   * Load one capped cell and put it in the row, for the grid's in-cell button.
   * Only that cell: the rest of the column keeps its size stand-in, because one
   * value being read is not a reason to pull a half-megabyte column back into
   * the page.
   * @param {{ rowIdx: number, colIdx: number }} detail
   */
  async function handleLoadCellValue(detail) {
    const col = columns[detail.colIdx]
    if (!col) return
    const res = await handleFetchCellValue(detail, CELL_VALUE_MAX)
    if (res.truncated) {
      // A cut value in a cell is worse than the size it replaces: it reads as
      // the value and is not one. Past this size nothing loads whole anywhere,
      // so the dock is the honest answer - it pages through what it has.
      //
      // It opens it, rather than printing a message telling you to press a key.
      // Load is a request to see the value, and the answer to a request you can
      // satisfy is not a notification.
      dataTable?.openCellDock?.(detail.rowIdx, detail.colIdx)
      return
    }
    // A JSON column renders from a parsed value, the way an under-cap row in the
    // same column already arrives; anything else is text. Past CELL_PARSE_MAX it
    // stays text too - see the constant.
    const type = String(col.dataType ?? col.data_type ?? '').toLowerCase()
    /** @type {unknown} */
    let next = res.text
    if ((type === 'json' || type === 'jsonb') && res.text.length <= CELL_PARSE_MAX) {
      try { next = JSON.parse(res.text) } catch { /* leave it as text */ }
    }
    rows[detail.rowIdx] = rows[detail.rowIdx].map((cell, j) => (j === detail.colIdx ? next : cell))
    // rows is $state.raw, so the assignment above does not notify the canvas.
    dataVersion++
  }

  /** @param {{ rowIdx: number, colIdx: number, value: unknown }} detail */
  async function handleSaveCell(detail) {
    if (!activeTable || !primaryKey.length) return

    const col = columns[detail.colIdx]
    if (!col) return

    const pk = primaryKeyForRow(detail.rowIdx)
    if (!pk) return

    savingCell = true
    const _saveStart = Date.now()
    try {
      await updateTableCell(activeSchema, activeTable, pk, col.name, detail.value)
      rows[detail.rowIdx] = rows[detail.rowIdx].map(
        (cell, j) => (j === detail.colIdx ? detail.value : cell),
      )
      // rows is $state.raw, so the in-place assignment above doesn't notify the
      // canvas; bump dataVersion to force the grid to repaint the edited cell.
      dataVersion++
      // With a filter or search active, the edited value may have moved this row
      // in or out of the result - so the total is no longer known to be right.
      // Unfiltered, editing a cell cannot change how many rows there are.
      if (rowSearch.trim() !== '' || activeFilters(rowFilters).length > 0) invalidateRowCount()
      saveActiveTabState()
      recordActivity({ type: 'row_save', title: `Updated ${col.name} in ${activeTable}`, schema: activeSchema, table: activeTable, durationMs: Date.now() - _saveStart, success: true })
    } catch (e) {
      recordActivity({ type: 'row_save', title: `Failed to update ${col.name} in ${activeTable}`, schema: activeSchema, table: activeTable, success: false, error: String(e) })
      throw e
    } finally {
      savingCell = false
    }
  }

  /** Write SQL into the SQL editor and focus it. */
  /** @param {string} sql */
  async function openQueryInEditor(sql) {
    await focusSqlView()
    sqlText = sql
  }

  async function openQueryHistory() {
    await focusSqlView()
    queryHistoryVisible = true
  }

  /** @param {string} name @param {string} sql */
  async function handleSaveQuery(name, sql) {
    if (!persistConnectionId) return
    await createSavedQuery(persistConnectionId, name, sql)
    await refreshQueryStores()
    toast.success('Query saved')
  }

  async function handleAiWriteSql(sql) {
    await openQueryInEditor(sql)
  }

  async function focusSqlView() {
    const existing = findSqlTab(tabs)
    if (existing) {
      await activateTab(existing.id)
      return
    }
    openSqlTab()
  }

  async function focusDataView() {
    if (activeTab?.kind === 'table') return
    if (activeTable) {
      const existing = findTableTab(tabs, activeSchema, activeTable)
      if (existing) {
        await activateTab(existing.id)
        return
      }
      await openTableTab(activeSchema, activeTable)
      return
    }
    const tableTab = findLastTableTab(tabs)
    if (tableTab) {
      await activateTab(tableTab.id)
      return
    }
    if (activeTab?.kind === 'sql') {
      openWelcomeTab()
    }
  }

</script>

<Onboarding bind:open={showOnboarding} onconnect={() => (showConnectionModal = true)} onsample={handleSampleConnect} />
<ConnectionModal
  bind:open={showConnectionModal}
  bind:initialEngine={connectionModalEngine}
  onconnected={(conn, id) => onConnected(conn, id)}
  maxConnections={$hasPro ? Infinity : FREE_CONNECTION_LIMIT}
  activeConnectionName={connection ? (connection.name || connection.database || connection.host || connection.filePath || 'Connected') : ''}
  ondisconnect={requestDisconnect}
  onremoved={handleConnectionRemoved}
/>
<SwitchDatabaseDialog
  bind:open={showSwitchDbDialog}
  databaseName={pendingDbSwitch?.label ?? ''}
  currentName={connection?.database ?? connection?.name ?? ''}
  onconfirm={commitDatabaseSwitch}
/>
<CreateDatabaseDialog
  bind:open={showCreateDbDialog}
  connType={dbAdmin ?? connection?.type ?? 'postgres'}
  oncreate={createDatabase}
/>
{#if dbNameDialog}
  <DatabaseNameDialog
    bind:open={dbNameDialogOpen}
    mode={dbNameDialog.mode}
    kind={dbAdmin ?? 'postgres'}
    source={dbNameDialog.source}
    existing={dbNameDialog.existing}
    onsubmit={runDbNameStatement}
  />
{/if}
<DropDatabaseDialog
  bind:open={showDropDbDialog}
  kind={dbAdmin ?? 'postgres'}
  name={dropDbName}
  sessions={dropDbSessions}
  onconfirm={(args) => void commitDropDatabase(args)}
/>
<DatabaseInfoDialog
  bind:open={showDbInfoDialog}
  name={dbInfoName}
  rows={dbInfoRows}
  loading={dbInfoLoading}
  error={dbInfoError}
/>
<DisconnectDialog bind:open={showDisconnectDialog} connectionName={connection ? (connection.name || connection.database || connection.host || connection.filePath || 'Connected') : ''} ondisconnect={() => handleDisconnect()} />
<CreateTableDialog
  bind:open={showCreateTableDialog}
  {activeSchema}
  dbType={dbType}
  onexecute={async (sql) => { await executeSql(sql) }}
  oncreated={async () => { await loadTables({ force: true }) }}
/>
<CreateSchemaDialog
  bind:open={showCreateSchemaDialog}
  existingSchemas={schemas}
  onexecute={async (sql) => {
    try { await executeDdl(sql) }
    catch (e) { toast.error('Could not create schema', { description: String(e) }); throw e }
  }}
  oncreated={async (schemaName) => {
    toast.success(`Schema "${schemaName}" created`)
    await loadSchemas()
    await handleSchemaChange(schemaName)
  }}
/>
<GenerateSqlDialog
  bind:open={generateSqlOpen}
  schema={activeSchema}
  table={generateSqlTable}
  dialect={dbType}
  onopeninsql={(sql) => { if (aiMode) exitAiMode(); void openQueryInEditor(sql) }}
/>
<DockerLaunchModal
  bind:open={showDockerModal}
  initialDbType={dockerInitialDb}
  onconnect={handleDockerConnect}
/>

<McpPanel bind:open={showMcpPanel} connected={!!connection} />

<SettingsDialog
  bind:open={showSettingsModal}
  onopenmcp={() => (showMcpPanel = true)}
  onopenmodelconfiguration={() => (showAiModelSettings = true)}
  onopenabout={() => (showAboutModal = true)}
  onopenextensions={() => openExtensionsTab()}
  onopenlicense={() => openLicenseTab()}
/>

<ImportDataDialog
  bind:open={importDataOpen}
  schema={activeSchema}
  table={activeTable ?? ''}
  {columns}
  {primaryKey}
  onimported={() => reloadTableFromQuery(false)}
/>

<AiSettingsDialog bind:open={showAiModelSettings} />

<KeyboardShortcutsDialog bind:open={showShortcutsModal} />

<InsiderDialog bind:open={showInsiderModal} />

<AboutDialog bind:open={showAboutModal} onopenreport={() => (showReportIssueDialog = true)} />
<ReportIssueDialog bind:open={showReportIssueDialog} />

<Dialog.Root bind:open={showProGate} closeOnEscape={true} closeOnOutsideClick={true}>
  <Dialog.Portal>
    <Dialog.Overlay class="fixed inset-0 z-50 bg-black/65" />
    <Dialog.Content class="fixed left-1/2 top-1/2 z-50 w-full max-w-sm -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-border/60 bg-background p-5 elevate-3-rim outline-none">
      <div class="mb-5 flex size-10 items-center justify-center rounded-lg border border-warning/20 bg-warning/10">
        <Lock class="size-5 text-warning" />
      </div>
      <h2 class="mb-1.5 text-ui-sm font-semibold text-foreground">Stroke Pro required</h2>
      <p class="mb-5 text-ui-xs leading-relaxed text-muted-foreground">This feature is not available on the free plan. Upgrade to Stroke Pro to unlock AI, dashboards, ORM runner, schema explorer, and more.</p>
      <div class="flex items-center gap-2">
        <button
          onclick={() => (showProGate = false)}
          class= "field-surface flex h-8 flex-1 items-center justify-center bg-muted/50 px-4 text-ui-xs font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        >
          Back
        </button>
        <button
          onclick={() => {
            showProGate = false
            openLicenseTab()
          }}
          class="flex h-8 flex-1 items-center justify-center gap-1.5 rounded-lg bg-foreground px-4 text-ui-xs font-medium text-background transition-colors hover:bg-foreground/90"
        >
          <KeyRound class="size-3" />
          Activate Pro
        </button>
      </div>
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>

<UpdateDialog bind:this={updateDialog} onupdatefound={() => (statusBarHasUpdate = true)} />


<CommandPalette
  bind:open={commandOpen}
  bind:page={commandPage}
  connected={!!connection}
  {schemas}
  {tables}
  {activeSchema}
  {savedConnections}
  {activeConnectionId}
  ontableselect={(name) => { if (aiMode) exitAiMode(); void handleTableSelect(name) }}
  onschemachange={(schema) => { if (aiMode) exitAiMode(); handleSchemaChange(schema) }}
  onopensql={() => { if (aiMode) exitAiMode(); void focusSqlView() }}
  onnewsql={() => { if (aiMode) exitAiMode(); openNewSqlTab() }}
  onopentable={() => { if (aiMode) exitAiMode(); void focusDataView() }}
  onopensettings={() => (showSettingsModal = true)}
  onopenconnection={() => (showConnectionModal = true)}
  ondisconnect={requestDisconnect}
  onrefresh={handleRefresh}
  readonly={tableReadonly}
  onreadonlytoggle={() => { tableReadonly = !tableReadonly }}
  onopenai={() => openAiTab()}
  onopenaisidebar={() => { if (aiMode) exitAiMode(); toggleAiSidebar() }}
  {aiMode}
  ontoggleaimode={() => aiMode ? exitAiMode() : enterAiMode()}
  onopenorm={() => { if (aiMode) exitAiMode(); openOrmTab() }}
  onopenerd={() => { if (aiMode) exitAiMode(); openErdTab() }}
  onopenbackup={() => { if (aiMode) exitAiMode(); openBackupTab() }}
  onopendashboard={() => { if (aiMode) exitAiMode(); openDashboardTab() }}
  onopencharts={() => { if (aiMode) exitAiMode(); openChartsTab() }}
  onopendiagrams={() => { if (aiMode) exitAiMode(); openDiagramsTab() }}
  onopenSchema={() => { if (aiMode) exitAiMode(); openSchemaTab() }}
  onopensecurity={() => { if (aiMode) exitAiMode(); openSecurityTab() }}
  onopenlogs={() => { if (aiMode) exitAiMode(); openLogsTab() }}
  onopeninsights={() => { if (aiMode) exitAiMode(); openInsightsTab() }}
  onopenadvisor={() => { if (aiMode) exitAiMode(); openAdvisorTab() }}
  onopenobjects={() => { if (aiMode) exitAiMode(); openObjectsTab() }}
  onopenormschema={() => { if (aiMode) exitAiMode(); openOrmSchemaTab() }}
  ontogglequerylog={() => { commandOpen = false; queryLogOpen = !queryLogOpen }}
  onopenextensions={() => { if (aiMode) exitAiMode(); openExtensionsTab() }}
  onopenmap={() => { if (aiMode) exitAiMode(); openMapTab() }}
  onopenredis={() => { if (aiMode) exitAiMode(); openRedisTab() }}
  {isRedis}
  {geoAvailable}
  {hasSchemaExplorer}
  {hasSecurity}
  onopenJsonViewer={() => { if (aiMode) exitAiMode(); openJsonTab() }}
  onopenshortcuts={() => (showShortcutsModal = true)}
  onopenabout={() => (showAboutModal = true)}
  onopenreport={() => (showReportIssueDialog = true)}
  oncheckupdate={() => void updateDialog?.checkNow()}
  ondockerlaunch={(dbType) => { commandOpen = false; dockerInitialDb = dbType; showDockerModal = true }}
  onswitchdatabase={handleSwitchDatabase}
  {queryHistory}
  {savedQueries}
  onqueryselect={(sql) => { if (aiMode) exitAiMode(); void openQueryInEditor(sql) }}
  onopenqueryhistory={() => { if (aiMode) exitAiMode(); void openQueryHistory() }}
  onglobalsearch={() => { commandOpen = false; openSearchTab() }}
  onopennotebook={() => { commandOpen = false; openNewNotebookTab() }}
  onopennotebookfile={() => { commandOpen = false; void openNotebookFromFile() }}
  openschematimeline={() => { commandOpen = false; openSchemaTimelineTab() }}
  opendatadiff={() => { commandOpen = false; openDataDiffTab() }}
  schemaContext={aiSchemaContext}
  onaskcontinue={handleAskContinue}
/>


<!-- Capture phase, deliberately - see onWindowKeydownCapture. -->
<svelte:window onkeydowncapture={onWindowKeydownCapture} />

{#if autoConnecting && !connection}
  <!-- Only covers the actual connect handshake: onConnected sets `connection`
       the moment the backend confirms, which drops this overlay and lets the
       catalog (schemas/tables/counts) stream into the sidebar skeletons. -->
  <div
    class="fixed inset-0 z-50 flex flex-col items-center justify-center gap-7 bg-background"
    out:fade={{ duration: isRevealed() ? 120 : 0 }}
  >
    <!-- The ring and the label wait before they appear (ConnectOverlay's
         `delay`). A warm reconnect lands in a few hundred ms, and showing them
         at once flashed "Reconnecting" over the shell on a cold start. -->
    <ConnectOverlay
      verb={autoConnectVerb}
      name={autoConnectName ? shortConnLabel(autoConnectName) : ''}
      oncancel={() => void cancelAutoConnect()}
    />
  </div>
{/if}


<div class="flex h-full min-h-0 w-full flex-col overflow-hidden bg-background">
<TitleBar
  title={windowTitle}
  connected={!!connection}
  {aiMode}
  {aiSidebarOpen}
  ontoggleaimode={() => { if (aiMode) exitAiMode(); else openAiTab() }}
  ontoggleaisidebar={() => { if (aiMode) exitAiMode(); toggleAiSidebar() }}
  {menuActions}
/>
<div class="flex min-h-0 flex-1 overflow-hidden">
  {#if connection}
    <!-- Sidebar dock. Navigation between pages/views now lives in the ⌘P
         "Go to page" palette, so there's no activity rail, the sidebar just
         hosts the tables list. Hidden on Ctrl+B and in AI mode. -->
    <div
      class="flex min-h-0 shrink-0 flex-col"
      class:order-last={sidebarSide === 'right'}
      style={sidebarOpen && !aiMode && !isRedis ? '' : 'display:none'}
    >
    {#if sidebarEverOpened}
    <div
      class="flex min-h-0 flex-1"
      style={sidebarOpen && !aiMode && !isRedis ? '' : 'display:none'}
      inert={!sidebarOpen || aiMode || isRedis || undefined}
    >
      <svelte:boundary>
        {#snippet failed(err, reset)}
          <div class="flex h-full w-[220px] shrink-0 flex-col items-center justify-center gap-3 border-r border-border/50 bg-sidebar p-4 text-center">
            <AlertTriangle class="size-5 text-destructive" />
            <p class="text-ui-xs font-medium text-muted-foreground">Sidebar error</p>
            <button
              type="button"
              class="rounded-md border border-border bg-muted/40 px-2.5 py-1 text-ui-xs font-medium transition-colors hover:bg-accent"
              onclick={reset}
            >Reload</button>
          </div>
        {/snippet}
      <Sidebar
        bind:openFindReplace={sidebarOpenFindReplace}
        bind:showTablesTab={sidebarShowTables}
        frColumns={columns}
        frRows={rows}
        frPrimaryKey={primaryKey}
        frForeignKeys={foreignKeys}
        frTableName={activeTable}
        frEnabled={findReplaceEnabled && !!activeTable && columns.length > 0}
        onfindreplaceapply={handleFindReplaceApply}
        onrevealcell={(rowIdx, colIdx) => {
          // A match is a cell, and the point of the panel living beside the grid
          // is that clicking one takes you to it.
          if (dataViewMode !== 'table') dataViewMode = 'table'
          void tick().then(() => tableFocusCell(rowIdx, colIdx))
        }}
        connectionName={connection ? (connection.name || connection.database || connection.host || connection.filePath || 'Connected') : ''}
        {navSidebarPanel}
        connections={savedConnections}
        activeConnectionId={connection?.id ?? ''}
        onswitchconnection={(c) => { if (aiMode) exitAiMode(); void handleSwitchDatabase(c) }}
        onaddconnection={() => { showConnectionModal = true }}
        onremoveconnection={(id) => { removeConnection(id); handleConnectionRemoved(id) }}
        onsetconnectiongroup={(id, group) => { savedConnections = setConnectionGroup(id, group) }}
        ondisconnectconnection={() => handleDisconnect()}
        onopenextensiondetail={(ext) => openExtensionDetailTab(ext)}
        side={sidebarSide}
        onmoveside={moveSidebar}
        {schemas}
        {tables}
        bind:activeSchema
        {activeTable}
        {activeView}
        {tableFilter}
        {loadingTables}
        onschemachange={handleSchemaChange}
        ontableselect={handleTableSelect}
        ontablefilter={(v) => (tableFilter = v)}
        onrefresh={handleRefresh}
        {connection}
        onswitchdatabase={requestDatabaseSwitch}
        onswitchdatabasenow={switchDatabaseNow}
        onnewdatabase={() => (showCreateDbDialog = true)}
        onrenamedatabase={({ name, existing }) => openDbNameDialog({ mode: 'rename', name, existing })}
        onduplicatedatabase={({ name, existing }) => openDbNameDialog({ mode: 'duplicate', name, existing })}
        ondropdatabase={(args) => void requestDropDatabase(args)}
        ondatabaseinfo={(args) => void openDatabaseInfo(args)}
        onterminatedbsessions={(args) => void terminateDbSessions(args)}
        {databasesRefreshKey}
        onnewtable={() => (showCreateTableDialog = true)}
        onnewschema={() => (showCreateSchemaDialog = true)}
        ontruncatetable={handleTruncateTable}
        ondroptable={(t, c) => void handleDropTable(t, c)}
        onviewddl={(t) => void handleViewDdl(t)}
        onviewstructure={(t) => void openTableStructure(t)}
        onexportsql={(t) => void handleExportSql(t)}
        onexportdata={(t) => void handleExportData(t)}
        onopeninconsole={handleOpenTableInConsole}
        ongeneratesql={handleGenerateSql}
        onopentableerd={(t) => { if (aiMode) exitAiMode(); openErdTab(t) }}
        oncountrows={(t) => void handleCountRows(t)}
        oncopycolumns={(t) => void handleCopyColumns(t)}
        openTables={tabs.filter((t) => t.kind === 'table' && t.state && /** @type {any} */ (t.state).schema === activeSchema).map((t) => /** @type {any} */ (t.state).table)}
        onclosetable={(name) => {
          const tab = findTableTab(tabs, activeSchema, name)
          if (tab) void closeTab(tab.id)
        }}
        {recentTabs}
        onrecentselect={(schema, table) => { if (aiMode) exitAiMode(); void openTableTab(schema, table) }}
        onrecentremove={(schema, table) => {
          if (persistConnectionId) {
            removeRecentTab(persistConnectionId, schema, table)
            refreshRecentTabs()
          }
        }}
        onrecentclear={() => {
          if (persistConnectionId) {
            clearRecentTabs(persistConnectionId)
            refreshRecentTabs()
          }
        }}
      />
      </svelte:boundary>
    </div>
    {/if}
    </div>
  {/if}

  <main class="flex min-h-0 min-w-0 flex-1 flex-col bg-panel" data-studio-region="main">
    {#snippet tabError(/** @type {unknown} */ error, /** @type {() => void} */ reset)}
      <div class="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 p-8 text-center">
        <AlertTriangle class="size-8 text-destructive" />
        <div class="flex flex-col gap-1">
          <p class="text-ui-sm font-medium text-foreground">This view hit an error</p>
          <p class="max-w-md break-words font-mono text-ui-xs text-muted-foreground">
            {error instanceof Error ? error.message : String(error)}
          </p>
        </div>
        <button
          type="button"
          class="rounded-md border border-border bg-muted/40 px-3 py-1.5 text-ui-xs font-medium transition-colors hover:bg-accent hover:text-foreground"
          onclick={reset}
        >
          Reload this view
        </button>
      </div>
    {/snippet}

    {#if !connection}
      <div class="relative flex flex-1 flex-col items-center justify-center gap-8 p-8 text-center">
        <!-- Subtle vignette, kept faint for a flat, crisp surface -->
        <div
          class="pointer-events-none absolute inset-0"
          style="background: radial-gradient(58% 48% at 50% 36%, color-mix(in oklch, var(--primary) 7%, transparent), transparent 72%);"
        ></div>

        <!-- Brand mark -->
        <div class="relative flex size-[72px] items-center justify-center rounded-[20px] border border-border/60 bg-card ring-1 ring-inset ring-white/[0.04] shadow-[0_10px_30px_-14px_rgba(0,0,0,0.7)]">
          <Logo class="size-9" />
        </div>

        <div class="relative flex max-w-md flex-col items-center gap-2.5">
          <h1 class="text-ui-3xl font-semibold leading-tight tracking-tight text-foreground">Connect a database</h1>
          <p class="max-w-[21rem] text-ui-sm leading-relaxed text-muted-foreground">
            Browse schemas, edit rows, and run SQL, all in one fast, native window.
          </p>
        </div>

        <!-- Supported engines, real brand marks. Each one is the shortcut into
             its own form: naming the engine IS the first step of the wizard, so
             a chip that only sat there was asking to be clicked and doing
             nothing. -->
        <div class="relative flex flex-wrap items-center justify-center gap-2">
          {#each [['postgres','PostgreSQL'],['mysql','MySQL'],['sqlite','SQLite'],['clickhouse','ClickHouse'],['d1','Cloudflare D1']] as [id, label]}
            <button
              type="button"
              title="New {label} connection"
              aria-label="New {label} connection"
              class="inline-flex items-center gap-2 rounded-full border border-border/50 bg-muted/20 py-1.5 pl-2.5 pr-3.5 text-ui-xs font-medium text-muted-foreground transition-colors hover:border-border hover:bg-muted/40 hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
              onclick={() => { connectionModalEngine = id; showConnectionModal = true }}
            >
              <DbIcon {id} class="size-4 shrink-0 text-muted-foreground" />
              {label}
            </button>
          {/each}
        </div>

        <div class="relative flex flex-col items-center gap-4 pt-1">
          <Button
            type="button"
            class="h-9 rounded-lg px-5 text-ui-sm font-semibold"
            bind:ref={welcomeConnectBtn}
            onclick={() => (showConnectionModal = true)}
          >
            <Plus class="size-4" />
            Add connection
          </Button>
          <p class="flex items-center gap-1.5 text-ui-xs text-muted-foreground">
            or press
            <Kbd combo="Mod+K" size="md" />
            for the command menu
          </p>
        </div>

        <p class="relative mt-2 text-ui-xs text-muted-foreground">
          made with care by
          <a
            href="https://nischal-dahal.com.np"
            target="_blank"
            rel="noreferrer"
            class="text-foreground/80 underline-offset-4 transition-colors hover:text-foreground hover:underline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
            onclick={(e) => { e.preventDefault(); void openExternalUrl('https://nischal-dahal.com.np') }}
          >@broisnees</a>
        </p>
      </div>
    {:else}
      <!-- Full-window AI chat, kept mounted after first open so state is preserved -->
      {#if aiEverOpened}
        <div
          class={aiMode ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={!aiMode}
        >
          {#await import('./AiChat.svelte')}<TabLoading />{:then { default: AiChat }}
            <svelte:boundary failed={tabError}>
            <AiChat
              schemaContext={{ ...aiSchemaContext, activeTable: null, columns: [], primaryKey: [], foreignKeys: [] }}
              {connectionId}
              isActive={aiMode}
              mode="full"
              onexit={exitAiMode}
              onwritesql={(sql) => void handleAiWriteSql(sql)}
              onopenmodelsettings={() => (showAiModelSettings = true)}
              onopendiagramspage={() => { exitAiMode(); openDiagramsTab() }}
            />
            </svelte:boundary>
          {/await}
        </div>
      {/if}

      {#if aiMode}
        <!-- AI mode: tabs + content hidden above via always-mounted block -->
      {:else}

      <!-- Split-pane workspace. A single group renders exactly like the classic
           single-tab layout (the sole focused leaf renders `sharedContent`);
           splitting adds sibling panes. -->
      {#if paneRoot}
        <PaneLayout
          node={paneRoot}
          focusedGroupId={activeGroupId}
          multiPane={paneCount > 1}
          {dropTarget}
          renderGroup={groupPane}
          onresize={handlePaneResize}
          onfocusgroup={(gid) => void focusGroup(gid)}
        />
      {:else}
        {#if tabBarVisible}
          <TabBar
            tabs={tabs.filter((t) => t.kind !== 'ai')}
            {activeTabId}
            onselect={(id) => activateTab(id)}
            onclose={closeTab}
            oncloseothers={closeOtherTabs}
            oncloseall={closeAllTabs}
            onclosemany={(ids, anchorId) => void closeManyTabs(ids, anchorId)}
            onduplicate={(id) => void duplicateTab(id)}
            onresettable={(id) => void resetTableTab(id)}
            onreopenclosed={reopenLastClosedTab}
            canreopenclosed={closedTabStack.length > 0}
            onpintoggle={toggleTabPin}
            onnewsql={() => { if (aiMode) exitAiMode(); openNewSqlTab() }}
          />
        {/if}
        {@render sharedContent()}
      {/if}

      {#snippet groupPane(/** @type {import('$lib/pane-layout.js').GroupNode} */ group, /** @type {boolean} */ isFocused)}
        {#if tabBarVisible}
          <TabBar
            tabs={group.tabIds.map((id) => tabsById.get(id)).filter(Boolean)}
            activeTabId={group.activeTabId}
            onselect={(id) => void focusTabInGroup(group.id, id)}
            onclose={(id) => void closeTabInGroup(group.id, id)}
            oncloseothers={closeOtherTabs}
            oncloseall={closeAllTabs}
            onclosemany={(ids, anchorId) => void closeManyTabs(ids, anchorId)}
            onduplicate={(id) => void duplicateTab(id)}
            onresettable={(id) => void resetTableTab(id)}
            onreopenclosed={reopenLastClosedTab}
            canreopenclosed={closedTabStack.length > 0}
            onpintoggle={toggleTabPin}
            onnewsql={() => { if (aiMode) exitAiMode(); openNewSqlTab() }}
            ondragtabstart={(id) => beginTabDrag(id)}
            ondragtabmove={(x, y) => moveTabDrag(x, y)}
            ondragtabend={() => endTabDrag()}
          />
        {/if}
        {#if isFocused}
          {@render sharedContent()}
        {:else}
          <PaneSnapshot tab={tabsById.get(group.activeTabId ?? '') ?? null} toolbarSpacer={tableToolbarVisible} connectionId={persistConnectionId} {schemas} />
        {/if}
      {/snippet}

      {#snippet sharedContent()}
      {#if activeTab?.kind === 'ai'}
        <!-- AI is handled via AI mode toggle -->
      {:else if activeTab?.kind === 'schema'}
        <svelte:boundary failed={tabError}>
          {#await import('./SchemaPage.svelte')}<TabLoading />{:then { default: SchemaPage }}
            <SchemaPage
              schema={activeSchema}
              connectionType={connection?.type ?? null}
              {indexes}
              {enums}
              {triggers}
              {sequences}
              {tables}
              loading={loadingTables}
              active={activeTab?.kind === 'schema'}
              onrefresh={async () => { await loadSchemas(); await loadTables({ force: true }) }}
            />
          {/await}
        </svelte:boundary>
      {/if}

      <!-- Schema as ORM code - mount once, keep alive -->
      {#if ormSchemaEverOpened}
        <div
          class={activeTab?.kind === 'orm-schema' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'orm-schema' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./OrmSchemaPage.svelte')}<TabLoading />{:then { default: OrmSchemaPage }}
              <OrmSchemaPage schema={activeSchema} dbType={connection?.type ?? 'postgres'} connectionId={persistConnectionId} />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- Security tab - mount once, keep alive -->
      {#if securityEverOpened}
        <div
          class={activeTab?.kind === 'security' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'security' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./SecurityPage.svelte')}<TabLoading />{:then { default: SecurityPage }}
              <SecurityPage bind:this={securityPageRef} active={activeTab?.kind === 'security'} connectionType={connection?.type ?? null} />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- Backup tab -->
      {#if backupEverOpened}
        <div
          class={activeTab?.kind === 'backup' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'backup' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./BackupPage.svelte')}<TabLoading />{:then { default: BackupPage }}
              <BackupPage dbType={dbType} activeSchema={activeSchema} {schemas} tables={tables.map((t) => ({ name: t.name, rowCount: t.rowCount }))} />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- Logs tab - mount once, keep alive -->
      {#if logsEverOpened}
        <div
          class={activeTab?.kind === 'logs' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'logs' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./LogsPage.svelte')}<TabLoading />{:then { default: LogsPage }}
              <LogsPage active={activeTab?.kind === 'logs'} />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- Instance Insights tab - mount once, keep alive -->
      {#if insightsEverOpened}
        <div
          class={activeTab?.kind === 'insights' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'insights' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./InstanceInsightsPage.svelte')}<TabLoading />{:then { default: InstanceInsightsPage }}
              <InstanceInsightsPage active={activeTab?.kind === 'insights'} connectionName={connection?.name ?? connection?.database ?? ''} {dbType} />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- VACUUM. Lazy like every other page, so an easter egg nobody has found
           costs nothing in the bundle they downloaded. -->
      {#if golfEverOpened}
        <div
          class={activeTab?.kind === 'golf' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'golf' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./VacuumGame.svelte')}<TabLoading />{:then { default: VacuumGame }}
              <VacuumGame />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- Advisor tab - mount once, keep alive. Teardown-eligible: it re-scans on
           reopen, so nothing the user typed is lost by unmounting it. -->
      {#if advisorEverOpened}
        <div
          class={activeTab?.kind === 'advisor' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'advisor' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./AdvisorPage.svelte')}<TabLoading />{:then { default: AdvisorPage }}
              <AdvisorPage connectionId={persistConnectionId} />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- Database Objects tab - mount once, keep alive -->
      {#if objectsEverOpened}
        <div
          class={activeTab?.kind === 'objects' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'objects' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./ObjectsPage.svelte')}<TabLoading />{:then { default: ObjectsPage }}
              <ObjectsPage
                bind:focusSearch={objectsFocusSearch}
                active={activeTab?.kind === 'objects'}
                connectionType={connection?.type ?? null}
                onopen={({ schema, name }) => void openTableTab(schema || activeSchema, name)}
              />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- Redis keyspace tab - mount once, keep alive -->
      {#if redisEverOpened}
        <div
          class={activeTab?.kind === 'redis' ? 'flex min-h-0 flex-1' : 'hidden'}
          inert={activeTab?.kind !== 'redis' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./RedisKeyspacePage.svelte')}<TabLoading />{:then { default: RedisKeyspacePage }}
              <RedisKeyspacePage active={activeTab?.kind === 'redis'} {connection} />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- Map tab - mount once, keep alive (holds viewport + fetched features) -->
      {#if mapEverOpened}
        <div
          class={activeTab?.kind === 'map' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'map' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./MapPage.svelte')}<TabLoading />{:then { default: MapPage }}
              <MapPage />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- Extensions tab - mount once, keep alive -->
      {#if extensionsEverOpened}
        <div
          class={activeTab?.kind === 'extensions' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'extensions' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./ExtensionsPage.svelte')}<TabLoading />{:then { default: ExtensionsPage }}
              <ExtensionsPage />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- Extension detail tab, one per extension, rendered on demand -->
      {#if activeTab?.kind === 'extension-detail'}
        {#key activeTab.id}
          <div class="flex min-h-0 flex-1 flex-col">
            <svelte:boundary failed={tabError}>
              {#await import('./ExtensionsPage.svelte')}<TabLoading />{:then { default: ExtensionsPage }}
                <ExtensionsPage initialExtensionId={activeTab.state?.extensionId ?? ''} />
              {/await}
            </svelte:boundary>
          </div>
        {/key}
      {/if}

      <!-- License tab, rarely opened, no keep-alive needed -->
      {#if activeTab?.kind === 'license'}
        <div class="flex min-h-0 flex-1 flex-col">
          <svelte:boundary failed={tabError}>
            {#await import('./LicensePage.svelte')}<TabLoading />{:then { default: LicensePage }}
              <LicensePage onactivated={closeLicenseTab} />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- JSON Viewer tab - mount once, keep alive -->
      {#if jsonEverOpened}
        <div
          class={activeTab?.kind === 'json' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'json' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./JsonViewerPage.svelte')}<TabLoading />{:then { default: JsonViewerPage }}
              <JsonViewerPage active={activeTab?.kind === 'json'} />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- Charts tab - mount once, keep alive -->
      {#if chartsEverOpened}
        <div
          class={activeTab?.kind === 'charts' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'charts' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./ChartsPage.svelte')}<TabLoading />{:then { default: ChartsPage }}
              <ChartsPage
                {connection}
                onrunsql={(sql) => { if (aiMode) exitAiMode(); void openQueryInEditor(sql) }}
              />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- Diagrams tab - mount once, keep alive -->
      {#if diagramsEverOpened}
        <div
          class={activeTab?.kind === 'diagrams' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'diagrams' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./DiagramsPage.svelte')}<TabLoading />{:then { default: DiagramsPage }}
              <DiagramsPage {connection} />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- Dashboard tab - mount once, keep alive -->
      {#if dashboardEverOpened}
        <div
          class={activeTab?.kind === 'dashboard' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'dashboard' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./DashboardPage.svelte')}<TabLoading />{:then { default: DashboardPage }}
              <DashboardPage {connection} />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- Data model tab -->
      {#if erdEverOpened}
        <div
          class={activeTab?.kind === 'erd' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'erd' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./EntityRelationPage.svelte')}<TabLoading />{:then { default: EntityRelationPage }}
              <EntityRelationPage
                onopendiagrams={() => openDiagramsTab()}
                schema={activeSchema}
                {schemas}
                focusTable={erdFocusTable}
                onclearfocus={() => (erdFocusTable = '')}
                onopentable={(s, t) => void openTableTab(s, t)}
              />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- Search tab - mount once, keep alive -->
      {#if searchEverOpened}
        <div
          class={activeTab?.kind === 'search' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'search' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./SearchPage.svelte')}<TabLoading />{:then { default: SearchPage }}
              <SearchPage
                bind:focusSearch={dbSearchFocusInput}
                {tables}
                schema={activeSchema}
                dialect={dbType}
                active={activeTab?.kind === 'search'}
                onopentable={(tableName, searchTerm) => {
                  if (aiMode) exitAiMode()
                  void openTableTab(activeSchema, tableName, { search: searchTerm })
                }}
              />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- Notebook tabs, one instance per open notebook, kept alive -->
      {#each tabs.filter((t) => t.kind === 'notebook') as nbTab (nbTab.id)}
        {@const nbState = /** @type {import('$lib/studio-tabs.js').NotebookTabState} */ (nbTab.state)}
        <div
          class={activeTab?.id === nbTab.id ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.id !== nbTab.id || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./NotebookEditor.svelte')}<TabLoading />{:then { default: NotebookEditor }}
              <NotebookEditor
                active={activeTab?.id === nbTab.id}
                notebook={nbState.notebook}
                filePath={nbState.filePath}
                dirty={nbState.dirty}
                onupdate={(updates) => updateNotebookTab(nbTab.id, updates)}
              />
            {/await}
          </svelte:boundary>
        </div>
      {/each}

      <!-- DDL tabs, one read-only editor per open object, kept alive -->
      {#each tabs.filter((t) => t.kind === 'ddl') as ddlTab (ddlTab.id)}
        {@const ddlState = /** @type {{ ddlText: string }} */ (ddlTab.state)}
        <div
          class={activeTab?.id === ddlTab.id ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.id !== ddlTab.id || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./DdlView.svelte')}<TabLoading />{:then { default: DdlView }}
              <DdlView ddl={ddlState.ddlText} objectName={ddlTab.title.replace(/^DDL · /, '')} />
            {/await}
          </svelte:boundary>
        </div>
      {/each}

      <!-- Schema Timeline tab - mount once, keep alive -->
      {#if schemaTimelineEverOpened}
        <div
          class={activeTab?.kind === 'schema-timeline' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'schema-timeline' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./SchemaTimelinePage.svelte')}<TabLoading />{:then { default: SchemaTimelinePage }}
              <SchemaTimelinePage
                connectionId={persistConnectionId}
                connectionLabel={connection?.name ?? connection?.database ?? connection?.filePath ?? ''}
                dbType={dbType}
                connections={savedConnections}
              />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- Data Diff tab - mount once, keep alive -->
      {#if dataDiffEverOpened}
        <div
          class={activeTab?.kind === 'data-diff' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'data-diff' || undefined}
        >
          <svelte:boundary failed={tabError}>
            {#await import('./DataDiffPage.svelte')}<TabLoading />{:then { default: DataDiffPage }}
              <DataDiffPage
                bind:this={dataDiffPageRef}
                {schemas}
                {tables}
                activeSchema={activeSchema}
                connections={savedConnections}
                currentConnectionId={persistConnectionId}
              />
            {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- ORM tab: mount once, keep alive so Monaco is not destroyed on tab switch -->
      {#if ormEverOpened}
        <div
          class={activeTab?.kind === 'orm' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'orm' || undefined}
        >
          <svelte:boundary failed={tabError}>
          {#await import('./OrmRunner.svelte')}<TabLoading />{:then { default: OrmRunner }}
          <OrmRunner
            bind:code={ormCode}
            bind:mode={ormMode}
            columns={ormColumns}
            rows={ormRows}
            loading={ormLoading}
            error={ormError}
            queryMs={ormQueryMs}
            schemaHints={sqlSchemaHints}
            onrun={(d) => void runOrm(d)}
            onmodi={() => { if (connection) toggleAiSidebar() }}
            onmodw={() => closeActiveTab()}
            onmodn={() => { if (connection) openWelcomeTab() }}
            onmodm={() => cycleTheme()}
            onmodt={() => { if (connection) { commandPage = 'tables'; commandOpen = true } }}
            onmodshifte={() => { if (connection) aiMode ? exitAiMode() : enterAiMode() }}
            onmodshiftd={() => { if (connection) showDisconnectDialog = true }}
            onmodaltd={() => { if (connection) void focusDataView() }}
          />
          {/await}
          </svelte:boundary>
        </div>
      {/if}

      <!-- SQL tab: mount once, keep alive so Monaco is not destroyed on tab switch -->
      {#if sqlEverOpened}
        <div
          class={activeTab?.kind === 'sql' ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}
          inert={activeTab?.kind !== 'sql' || undefined}
        >
          <svelte:boundary failed={tabError}>
          {#await import('./SqlConsole.svelte')}<TabLoading />{:then { default: SqlConsole }}
          <SqlConsole
            bind:this={sqlConsoleRef}
            active={activeTab?.kind === 'sql'}
            engine={dbType}
            txStatus={activeTxSession ? activeTxStatus : null}
            txBusy={sqlTxBusy}
            onbegintransaction={() => void beginSqlTransaction()}
            oncommittransaction={() => void endSqlTransaction('commit')}
            onrollbacktransaction={() => void endSqlTransaction('rollback')}
            bind:sql={sqlText}
            bind:queryHistoryVisible
            {queryHistory}
            {savedQueries}
            columns={sqlColumns}
            rows={sqlRows}
            runOutcomes={sqlRunOutcomes}
            windowed={sqlWindowed}
            dataVersion={sqlDataVersion}
            windowStatus={sqlWindowStatus}
            sorting={sqlSorting}
            onvisiblerange={(start, end) => sqlViewOf(activeTabId)?.visible(start, end)}
            onretrywindows={() => sqlViewOf(activeTabId)?.retry()}
            onstoredsort={(sort) => void sortStoredSqlResult(sort)}
            onloadallrows={(onprogress) => sqlViewOf(activeTabId)?.readAll(onprogress) ?? Promise.resolve(sqlRows)}
            queryMs={sqlQueryMs}
            message={sqlMessage}
            loading={sqlLoading}
            runningQueryId={activeSqlQueryId}
            error={sqlError}
            multiResults={sqlMultiResults}
            schemaHints={sqlSchemaHints}
            schemaContext={aiSchemaContext}
            onrun={runSql}
            onmodk={() => { commandOpen = true }}
            onmods={() => saveActiveTabState()}
            onmodi={() => { if (connection) toggleAiSidebar() }}
            onmodw={() => closeActiveTab()}
            onmodn={() => { if (connection) openWelcomeTab() }}
            onmodm={() => cycleTheme()}
            onmodt={() => { if (connection) { commandPage = 'tables'; commandOpen = true } }}
            onmodshifte={() => { if (connection) aiMode ? exitAiMode() : enterAiMode() }}
            onmodshiftd={() => { if (connection) showDisconnectDialog = true }}
            onmodaltd={() => { if (connection) void focusDataView() }}
            onmodshifto={() => { if (connection) openOrmTab() }}
            onqueryrefresh={refreshQueryStores}
            onhistoryselect={(sql) => void openQueryInEditor(sql)}
            onsavequery={handleSaveQuery}
            onfixwithai={handleFixWithAi}
            onprorequired={() => (showProGate = true)}
          />
          {/await}
          </svelte:boundary>
        </div>
      {/if}

      {#if activeTab?.kind === 'table'}
        <!-- The failure outranks the empty state: an error raised while the tab
             has no table selected still has to be readable, and it carries the
             only control that clears it. -->
        {#if error}
          <!-- ── Load failure, stated where the grid would be ────────────────
               A banner pinned to the top over an empty grid made the reader
               look in two places and left them with "dismiss to continue" as
               the only way forward. One centred state instead: what broke, why,
               and the action that fixes it - reconnect for a dropped pool,
               retry for everything else. -->
          {@const kind = connectionErrorKind(error)}
          {@const humanized = humanizeDbError(error)}
          <div
            class="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 p-8 text-center"
            role="alert"
          >
            {#if kind === 'unreachable'}
              <WifiOff class="size-8 shrink-0 text-muted-foreground" />
            {:else if kind === 'dropped'}
              <Unplug class="size-8 shrink-0 text-muted-foreground" />
            {:else}
              <AlertTriangle class="size-8 shrink-0 text-destructive" />
            {/if}

            <div class="flex max-w-md flex-col gap-1">
              <p class="text-ui-sm font-medium text-foreground">
                {#if kind === 'unreachable'}Cannot reach database
                {:else if kind === 'dropped'}Connection dropped
                {:else}This table failed to load{/if}
              </p>
              <!-- Drivers wrap the cause in transport noise - D1 returns its whole
                   HTTP envelope around a five-word message. Show the cause; the
                   raw text stays one click away and in the query log. -->
              <p class="break-words font-mono text-ui-xs leading-relaxed {kind ? 'text-muted-foreground' : 'text-destructive'}">
                {#if kind === 'unreachable'}Check your internet connection, or whether the server is still accepting connections.
                {:else if kind === 'dropped'}The pool for this connection closed. Reconnecting rebuilds it and reloads the table.
                {:else}{humanized}{/if}
              </p>
              {#if showRawError}
                <p class="mt-1 max-h-40 overflow-auto break-all text-left font-mono text-ui-3xs text-muted-foreground">{error}</p>
              {/if}
            </div>

            <div class="flex items-center gap-2">
              <button
                type="button"
                data-autofocus
                disabled={retryingLoad}
                class="inline-flex h-8 items-center gap-1.5 rounded-md border border-border bg-muted/40 px-3 text-ui-xs font-medium text-foreground transition-colors hover:bg-accent disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-offset-0 focus-visible:outline-ring"
                onclick={() => void retryTableLoad()}
              >
                <RefreshCw class="size-3.5 shrink-0 {retryingLoad ? 'animate-spin' : ''}" />
                {kind === 'dropped' || connectionLost ? 'Reconnect' : 'Retry'}
              </button>
              {#if !kind}
                <button
                  type="button"
                  class="inline-flex h-8 items-center rounded-md px-3 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-0 focus-visible:outline-ring"
                  onclick={() => { error = ''; showRawError = false }}
                >Dismiss</button>
              {/if}
            </div>

            {#if humanized !== error.replace(/^Error:\s*/, '').trim()}
              <button
                type="button"
                class="text-ui-3xs text-muted-foreground underline-offset-2 transition-colors hover:text-foreground hover:underline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
                onclick={() => (showRawError = !showRawError)}
              >{showRawError ? 'Hide raw error' : 'Show raw error'}</button>
            {/if}
          </div>
        {:else if !activeTable}
          <div class="flex flex-1 flex-col items-center justify-center gap-2 p-8 text-center">
            <p class="font-mono text-ui text-muted-foreground">
              Select a table from the sidebar or press
              <Kbd combo="Mod+K" size="md" />
            </p>
          </div>
        {:else}
          {#if tableViewMode === 'structure' && canShowStructure}
            {#if tableToolbarVisible}
            <TableToolbar
              bind:this={tableToolbar}
              bind:filterBarOpen
              bind:tableViewMode
              structureAllowed={canShowStructure}
              ontogglestructure={() => { tableViewMode = 'data'; structureColumns = [] }}
              {sidebarOpen}
              queryMs={0}
              page={1}
              pageSize={0}
              offset={0}
              total={0}
              columns={[]}
              rowSearch=""
              rowSort={null}
              rowFilters={[]}
              loading={loadingStructure}
              selectedCount={0}
              hasPrimaryKey={false}
              deleting={false}
                            onrefresh={() => void loadStructure()}
              onprev={() => {}}
              onnext={() => {}}
              {structureSearch}
              onstructuresearchchange={(v) => (structureSearch = v)}
            />
            {/if}
            {#await import('./StructureView.svelte')}<TabLoading />{:then { default: StructureView }}
              <StructureView
                schema={activeSchema}
                table={activeTable ?? ''}
                connectionType={connection?.type ?? null}
                {primaryKey}
                columns={structureColumns}
                indexes={activeTableIndexes}
                triggers={activeTableTriggers}
                {tables}
                {enums}
                columnSearch={structureSearch}
                loading={loadingStructure}
                onrefresh={() => { void loadStructure(); void reloadCatalogKind('triggers') }}
              />
            {/await}
          {:else}
          {#if tableToolbarVisible}
          <TableToolbar
            bind:this={tableToolbar}
            bind:filterBarOpen
            bind:tableViewMode
            bind:dataViewMode
            structureAllowed={canShowStructure}
            ontogglestructure={() => { tableViewMode = 'structure'; if (!structureColumns.length) void loadStructure() }}

            {sidebarOpen}
            {queryMs}
            {previewColumns}
            {page}
            {pageSize}
            offset={currentOffset}
            {total}
            {columns}
            {rowSearch}
            {rowSort}
            {rowSortMore}
            {rowFilters}
            loading={loadingRows}
            selectedCount={selected.size}
            hasPrimaryKey={primaryKey.length > 0}
            deleting={deletingRows}
                        onrefresh={loadRows}
            autoRefreshMs={activeAutoRefreshMs}
            onautorefreshchange={setActiveAutoRefresh}
            onsearchchange={handleRowSearchChange}
            onfilterschange={(f) => void handleRowFiltersChange(f)}
            onsortchange={(s) => void handleRowSortChange(s)}
            onpagesizechange={(s) => void handlePageSizeChange(s)}
            onpagechange={(p) => void handlePageChange(p)}
            keysetMode={_keysetActive}
            keysetHasMore={rows.length >= effectivePageSize}
            onlimitoffsetchange={(l, o) => void handleLimitOffsetChange(l, o)}
            {infiniteScroll}
            oninfinitescrolltoggle={toggleInfiniteScroll}
            live={liveEnabled}
            {searchOptions}
            onsearchoptionschange={handleSearchOptionsChange}
            searchOptionsSupported={searchOptsSupported}
            searchOptionsSupport={searchOptsSupport}
            savedViews={savedTableViews}
            viewsEnabled={savedViewsEnabled}
            activeViewId={activeTableViewId}
            onapplyview={applySavedView}
            onresetview={resetTableView}
            onsaveview={saveCurrentTableView}
            ondeleteview={deleteSavedView}
            {findReplaceEnabled}
            onfindreplace={() => openFindReplacePanel()}
            ondeleteselected={() => stageDeleteSelectedRows()}
            onexport={handleExport}
            onimport={() => (importDataOpen = true)}
            onexportdiagram={(kind) => erdPane?.exportDiagram?.(kind)}
            onexportchart={(kind) => chartPane?.exportChart?.(kind)}
            onaddrow={() => {
              // Row insertion happens on the canvas grid - jump back to it first.
              if (dataViewMode !== 'table') dataViewMode = 'table'
              void tick().then(() => dtBeginInsertRow?.())
            }}
            onopeninsql={openTableInSqlEditor}
            onmagicword={openGolfTab}
            readonly={tableReadonly}
            {hiddenColumns}
            virtualColCount={vcolCount}
            onopenvirtualcols={() => { vcolPanelOpen = !vcolPanelOpen }}
            virtualRelColumns={virtualRelColumnsForToolbar}
            virtualExprCols={virtualExprColsForToolbar}
            ontogglevexpr={(id) => {
              const all = $virtualColumnsStore[_vcolTableKey] ?? []
              const col = all.find(c => c.id === id)
              if (col) virtualColumnsStore.patch(_vcolTableKey, id, { enabled: !col.enabled })
            }}
            onhiddencolumnschange={(next) => {
              hiddenColumns = next
              if (activeTable) saveHiddenCols(persistConnectionId, activeSchema, activeTable, next)
            }}
            onfocuscolumn={(name) => focusTableColumn?.(name)}
            onprev={async () => {
              if (page <= 1) return
              await handlePageChange(page - 1)
            }}
            onnext={async () => {
              if (!_keysetActive && total >= 0 && page * effectivePageSize >= total) return
              await handlePageChange(page + 1)
            }}
          />
          {/if}

          <div class="flex min-h-0 min-w-0 flex-1">
            <svelte:boundary failed={tabError}>
              <!-- The grid stays mounted (hidden) in other view modes so staged
                   edits, selection and scroll position survive mode switches. -->
              <div class={dataViewMode === 'table' ? 'flex min-h-0 min-w-0 flex-1' : 'hidden'}>
              <DataTable
                bind:this={dataTable}
                {columns}
                {rows}
                getsqlhints={buildSqlHints}
                {primaryKey}
                {foreignKeys}
                rowNumberOffset={infiniteScroll ? 0 : currentOffset}
                {incomingForeignKeys}
                onfetchrelatedrows={handleFetchRelatedRows}
                onfetchcellvalue={dbType === 'postgres' ? handleFetchCellValue : null}
                onloadcellvalue={dbType === 'postgres' ? handleLoadCellValue : null}
                schema={activeSchema}
                tableName={activeTable ?? ''}
                connectionId={persistConnectionId}
                onrequestsearch={() => tableToolbar?.focusRowSearch?.()}
                dialect={dbType}
                indexes={activeTableIndexes}
                {hiddenColumns}
                {reloadToken}
                {dataVersion}
                expandSingleRow={expandSingleRow && $appFkAutoExpandJson}
                {windowed}
                {windowStatus}
                onvisiblerange={handleVisibleRange}
                onretrywindows={retryFailedWindows}
                columnWidthsKey={activeTable ? `${persistConnectionId}\x00${activeSchema}.${activeTable}` : undefined}
                loading={loadingRows}
                {loadingMore}
                {infiniteScroll}
                endOfResults={infiniteScroll && !windowed && total > 0 && rows.length >= total}
                onloadmore={handleLoadMore}
                saving={savingCell || deletingRows || insertingRow}
                bind:selected
                bind:focusedRow
                bind:focusedCol
                bind:inspectorRow
                bind:editingCell
                bind:pendingEditCount
                bind:applyEdits
                bind:copyEditsSql
                bind:resetEdits
                bind:scrollToTop={scrollTableTop}
                bind:scrollToBottom={scrollTableBottom}
                bind:scrollToLeft={scrollTableLeft}
                bind:scrollToRight={scrollTableRight}
                bind:canScrollHorizontally={tableCanScrollH}
                bind:focusColumn={focusTableColumn}
                bind:focusCell={tableFocusCell}
                bind:getScroll={tableGetScroll}
                bind:getExpanded={tableGetExpanded}
                bind:applyScroll={tableApplyScroll}
                bind:vcolPanelOpen
                {rowSort}
                {rowSortMore}
                searchQuery={rowSearch}
                {searchOptions}
                onsortchange={(s) => void handleRowSortChange(s)}
                onhidecolumn={(colName) => {
                  const next = new Set(hiddenColumns)
                  next.add(colName)
                  hiddenColumns = next
                  if (activeTable) saveHiddenCols(persistConnectionId, activeSchema, activeTable, next)
                }}
                onfiltercolumn={(colName) => {
                  const newFilter = { id: crypto.randomUUID(), column: colName, op: /** @type {any} */ ('contains'), value: '', conjunct: /** @type {any} */ ('and') }
                  void handleRowFiltersChange([...rowFilters, newFilter])
                  filterBarOpen = true
                  tableToolbar?.focusLastFilter?.()
                }}
                onfilterbyvalue={(colName, value, exclude) => {
                  /** @type {string} */ let op
                  let filterValue = ''
                  if (value === null || value === undefined) {
                    op = exclude ? 'is_not_null' : 'is_null'
                  } else {
                    op = exclude ? 'neq' : 'eq'
                    filterValue = String(value)
                  }
                  const newFilter = { id: crypto.randomUUID(), column: colName, op: /** @type {any} */ (op), value: filterValue, conjunct: /** @type {any} */ ('and') }
                  void handleRowFiltersChange([...rowFilters, newFilter])
                  filterBarOpen = true
                }}
                onquickfilter={(colName, op, value) => {
                  const newFilter = { id: crypto.randomUUID(), column: colName, op: /** @type {any} */ (op), value: value ?? '', conjunct: /** @type {any} */ ('and') }
                  void handleRowFiltersChange([...rowFilters, newFilter])
                  filterBarOpen = true
                }}
                onsave={handleSaveCell}
                ondelete={handleDeleteRow}
                onfollowforeignkey={(d) => void handleFollowForeignKey(d)}
                oninsertrow={handleInsertRow}
                onexecutesql={handleExecuteRawDml}
                insertSaving={insertingRow}
                bind:beginInsertRow={dtBeginInsertRow}
                bind:stageDeleteSelected={stageDeleteSelectedRows}
                readonly={tableReadonly}
              />
              </div>
              {#if dataViewMode === 'json'}
                {#await import('./TableJsonView.svelte')}<TabLoading />{:then { default: TableJsonView }}
                  <TableJsonView
                    columns={dataViewColumns}
                    rows={dataViewRows}
                    tableKey={`${activeSchema}.${activeTable}`}
                  />
                {/await}
              {:else if dataViewMode === 'record'}
                <TableRecordView
                  {columns}
                  {rows}
                  {primaryKey}
                  {hiddenColumns}
                  offset={currentOffset}
                  {total}
                  readonly={tableReadonly}
                  initialIndex={focusedRow ?? 0}
                  onindexchange={(i) => (focusedRow = i)}
                  hasPrevPage={!infiniteScroll && page > 1}
                  hasNextPage={!infiniteScroll && (total < 0 || page * effectivePageSize < total)}
                  onprevpage={() => void handlePageChange(page - 1)}
                  onnextpage={() => void handlePageChange(page + 1)}
                  onsave={handleSaveCell}
                />
              {:else if dataViewMode === 'text'}
                {#await import('./TableTextView.svelte')}<TabLoading />{:then { default: TableTextView }}
                  <TableTextView columns={dataViewColumns} rows={dataViewRows} tableName={activeTable} />
                {/await}
              {:else if dataViewMode === 'chart'}
                <ChartView bind:this={chartPane} columns={dataViewColumns} rows={dataViewRows} connectionId={persistConnectionId} />
              {:else if dataViewMode === 'map'}
                <div class="flex min-h-0 min-w-0 flex-1">
                  {#await import('./MapPage.svelte')}<TabLoading />{:then { default: MapPage }}
                    <MapPage scopeSchema={activeSchema} scopeTable={activeTable ?? ''} />
                  {/await}
                </div>
              {:else if dataViewMode === 'erd'}
                <div class="flex min-h-0 min-w-0 flex-1">
                  {#await import('./EntityRelationPage.svelte')}<TabLoading />{:then { default: EntityRelationPage }}
                    <EntityRelationPage
                onopendiagrams={() => openDiagramsTab()}
                      bind:this={erdPane}
                      hostExports
                      insideTableTab
                      schema={activeSchema}
                      {schemas}
                      focusTable={activeTable}
                      onclearfocus={() => openErdTab('')}
                      onopentable={(s, t, opts) => void openTableTab(s, t, opts)}
                    />
                  {/await}
                </div>
              {/if}
            </svelte:boundary>
            {#if dataViewMode === 'table'}
              <RowDetailPanel
                {columns}
                {rows}
                {primaryKey}
                target={inspectorTarget}
                {dataVersion}
                onclose={closeInspector}
                onsave={handleSaveCell}
              />
            {/if}
          </div>
          {/if}
        {/if}
      {/if}

      {#if !activeTab || activeTab.kind === 'welcome'}
        <!-- One platform test for the whole app: `IS_MAC` comes from `detectOs()`,
             which reads the Tauri platform. `navigator.platform` is deprecated,
             and a second source of truth is a second answer waiting to happen. -->
        {@const isMac = IS_MAC}
        {@const mod = isMac ? '⌘' : 'Ctrl'}
        <!-- Tile chrome. Every tile is the same fixed height with the icon row
             pinned to the top and the label to the bottom, so labels stay on a
             shared baseline whether or not a tile carries a chord or wraps to
             two lines. Colors use solid tokens (not fractional alpha) - thinned
             strokes read as fuzzy against the dark panel. -->
        <!-- min-h, not h: a label that wraps to two lines in a narrow pane grows
             the tile instead of spilling out of it. min-w-0 + overflow-hidden
             keep a long word inside the border when the column is tight. -->
        {@const cell = 'group relative flex min-h-[5.25rem] min-w-0 flex-col justify-between overflow-hidden rounded-lg border border-border/60 bg-card/50 p-2.5 text-left transition-[background-color,border-color,box-shadow,transform] duration-150 ease-[var(--ease-out)] hover:border-border hover:bg-accent/40 hover:shadow-sm active:scale-[0.98]'}
        {@const proCell = 'group relative flex min-h-[5.25rem] min-w-0 cursor-not-allowed flex-col justify-between overflow-hidden rounded-lg border border-border/40 bg-card/30 p-2.5 text-left transition-[background-color,border-color] duration-150 hover:border-warning/30 hover:bg-warning/[0.04]'}
        {@const iconCls = 'size-4 shrink-0 text-muted-foreground transition-colors group-hover:text-foreground'}
        {@const proIconCls = 'size-4 shrink-0 text-muted-foreground'}
        {@const labelCls = 'text-ui-2xs font-medium leading-[1.25] text-foreground/85 transition-colors group-hover:text-foreground [overflow-wrap:anywhere]'}
        {@const proLabelCls = 'text-ui-2xs font-medium leading-[1.25] text-muted-foreground [overflow-wrap:anywhere]'}

        <!-- Shift is spelled out off macOS: the bundled UI/mono webfonts have no
             U+21E7, so "Ctrl⇧E" fell back mid-word and rendered as garbage.
             Keys are separate spans - butted-together chords are unreadable at
             this size. -->
        {@const shiftKey = isMac ? '⇧' : 'Shift'}
        {#snippet chord(/** @type {string[]} */ keys)}
          <Kbd {keys} wrap />
        {/snippet}

        <!-- The tile grid, back to the shape it had: icon pinned top, label and
             chord anchored bottom, every tile the same size. What changed is the
             count - five actions instead of sixteen - so the grid is a shortlist
             you take in at a glance rather than a wall you have to read. -->
        {#snippet row(/** @type {any} */ Icon, /** @type {string} */ label, /** @type {string} */ _desc, /** @type {() => void} */ onclick, /** @type {{ pro?: boolean, keys?: string[] }} */ opts = {})}
          {@const locked = !!opts.pro && !$hasPro}
          <button
            type="button"
            {onclick}
            title={locked ? `${label} - ${_desc} - Pro` : `${label} - ${_desc}`}
            class={cn(
              // min-h, not h: a label that wraps to two lines in a narrow pane grows
              // the tile instead of spilling out of it.
              // The focus ring is the app's own: `outline-2 outline-ring`, the
              // same one every Button draws. These tiles are the first thing Tab
              // reaches on a fresh tab and they drew nothing at all.
              "group flex min-h-[5.25rem] min-w-0 flex-col justify-between gap-2 overflow-hidden rounded-xl border p-2.5 text-left transition-[background-color,border-color,transform] duration-150 ease-[var(--ease-out)] outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
              locked
                ? "cursor-not-allowed border-border/40 bg-card/30 hover:border-warning/30 hover:bg-warning/[0.04]"
                : "border-border/60 bg-card/50 hover:border-border hover:bg-accent/40 active:scale-[0.98]",
            )}
          >
            <span class="flex w-full items-start justify-between gap-1.5">
              <Icon class="size-4 shrink-0 text-muted-foreground transition-colors group-hover:text-foreground" />
              {#if locked}<Lock class="size-3 shrink-0 text-warning" aria-label="Pro feature" />{/if}
            </span>
            <span class="flex w-full min-w-0 flex-col gap-1">
              <span class="truncate text-ui-2xs font-medium leading-[1.25] text-foreground">{label}</span>
              <!-- The chord row is always present, empty or not, so every label in a
                   row lands on the same baseline whether or not it wrapped. -->
              <span class="flex min-h-[1em] min-w-0 flex-wrap items-center">
                {#if opts.keys && !locked}{@render chord(opts.keys)}{/if}
              </span>
            </span>
          </button>
        {/snippet}

        <!-- A second-tier destination: one line, no border, no chord. It has to
             read as subordinate to the tiles at a glance, which is what keeps
             the page a shortlist rather than a menu. -->
        {#snippet jump(/** @type {any} */ Icon, /** @type {string} */ label, /** @type {() => void} */ onclick)}
          <button
            type="button"
            {onclick}
            class="group flex h-8 min-w-0 items-center gap-2 rounded-md px-2 text-left text-muted-foreground outline-none transition-colors duration-150 hover:bg-accent/40 hover:text-foreground focus-visible:bg-accent/40 focus-visible:text-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
          >
            <Icon class="size-3.5 shrink-0" />
            <span class="truncate text-ui-2xs font-medium">{label}</span>
          </button>
        {/snippet}

        <!-- Scroll container keeps top/bottom padding reachable when the content
             outgrows the viewport (e.g. at high zoom); inner wrapper centers when it fits. -->
        <div class="min-h-0 flex-1 overflow-auto">
          <!-- Everything on this page hangs off ONE leading edge, and the block
               itself is centred in the pane. It used to centre each band on its
               own axis: a centred logo over a centred label over a centred meta
               line over a left-aligned grid, so nothing lined up with anything
               and the tiles read as off-centre against the text above them.
               `max-w-xl` is the grid's width, so the header, the list and the
               footer now all start where the first tile starts. -->
          <div class="mx-auto flex min-h-full w-full max-w-xl flex-col justify-center gap-7 px-6 py-10 sm:gap-9 sm:py-12">

          <!-- Header -->
          <div class="flex flex-col items-start gap-3">
            <div class="flex size-11 items-center justify-center rounded-lg border border-border bg-muted">
              <Logo class="size-6" />
            </div>
          </div>
          <!-- Five tiles, not sixteen. Sixteen equal-weight tiles asked you to read
               the whole grid to find the one you wanted; these five are what a tab
               opens for. The rest are not gone - they sit in the quieter "Jump to"
               list below, and in ⌘K. -->
          <!-- Column count comes from the space available, not a fixed number: the
                 pane narrows whenever the sidebar is dragged wider. At full width
                 the five tiles resolve to one clean row. -->
            <!-- Four columns, not five. At five the tiles came out ~96px wide and
                 "Extensions" truncated to "Extensio…" - a launcher whose labels do
                 not fit is not a launcher. The fifth tile was Shortcuts, which the
                 footer below already offers, so dropping it cost nothing and left
                 an exact row. -->
          <!-- The label sits in the same gap-2 column as the tiles, exactly as
               "Jump to" does with its list. In the header it was a full section
               gap away from the tiles, so it read as a caption for the logo. -->
          <div class="flex w-full flex-col gap-2">
            <p class="text-ui-3xs font-medium uppercase tracking-[0.1em] text-muted-foreground">Quick access</p>
            <div class="grid w-full grid-cols-2 gap-2 sm:grid-cols-4">
            {#if isRedis}
              {@render row(KeyRound, "Keyspace", "Browse keys and values", openRedisTab, {})}
            {:else}
              <!-- ⌘T opens the command palette on its tables page; the SQL view
                   is ⌘⇧S. The tile printed a chord that went somewhere else. -->
              {@render row(Terminal, "SQL", "Write and run a query", openSqlTab, { keys: [mod, shiftKey, "S"] })}
              {@render row(Sparkles, "AI", "Ask about this database", openAiTab, { pro: true, keys: [mod, shiftKey, "E"] })}
            {/if}
            {@render row(Blocks, "Extensions", "Add and manage extensions", openExtensionsTab, { pro: true, keys: [mod, shiftKey, "X"] })}
            {@render row(Database, "Connect", "Switch or add a connection", () => (showConnectionModal = true), { keys: [mod, shiftKey, "C"] })}
            </div>
          </div>

          {#if connection && !isRedis}
            <!-- Second tier, and deliberately quieter than the tiles above.
                 The pages worth reaching from a fresh tab outnumber the five a
                 shortlist can hold, but promoting them all to equal-weight tiles
                 is what turned this page into a wall the last time. A plain
                 two-column list of icon + label reads in one sweep, costs a
                 third of the height per item, and leaves the tiles carrying the
                 weight. Same max width as the grid above, so both blocks sit on
                 one alignment edge. -->
            <div class="flex w-full flex-col gap-2">
              <p class="text-ui-3xs font-medium uppercase tracking-[0.1em] text-muted-foreground">Jump to</p>
              <!-- Two columns of four. Three columns left an orphan row of two
                   hanging under a full one, which is the shape that reads as
                   "unfinished" no matter how the items are ordered. -->
              <div class="grid grid-cols-1 gap-x-3 gap-y-0.5 sm:grid-cols-2">
                {@render jump(Plus, "New query editor", openNewSqlTab)}
                {@render jump(Search, "Find in database", openSearchTab)}
                {@render jump(Boxes, "Database objects", openObjectsTab)}
                {@render jump(GitBranch, "Schema explorer", openSchemaTab)}
                {@render jump(Gauge, "Instance insights", openInsightsTab)}
                {@render jump(GitCompare, "Data diff", openDataDiffTab)}
                {@render jump(Network, "Data model", () => openErdTab())}
                {@render jump(LayoutDashboard, "Dashboard", openDashboardTab)}
              </div>
            </div>
          {/if}


          <!-- Footer -->
          <div class="flex w-full flex-wrap items-center gap-x-3 gap-y-1.5 text-ui-3xs text-muted-foreground">
            <button
              type="button"
              onclick={() => showShortcutsModal = true}
              class="flex items-center gap-1.5 rounded-md px-1 py-0.5 transition-colors hover:text-foreground"
            >
              <Command class="size-3 shrink-0" />
              <span>Shortcuts</span>
            </button>
            <span class="text-muted-foreground">·</span>
            <span class="flex items-center gap-1.5">
              {@render chord([mod, 'B'])}
              <span>sidebar</span>
            </span>
            <span class="text-muted-foreground">·</span>
            <span class="flex items-center gap-1.5">
              {@render chord([mod, 'W'])}
              <span>close tab</span>
            </span>
          </div>
          </div>
        </div>
      {/if}
      {/snippet}
      {/if}
    {/if}
  </main>


  {#if aiSidebarEverOpened && connection}
    <div
      style={aiSidebarOpen && !aiMode ? '' : 'display:none'}
      inert={!aiSidebarOpen || aiMode || undefined}
    >
      {#await import('./AiSidebar.svelte')}
        <div
          class="flex h-full min-h-0 shrink-0 flex-col border-l border-border/50 bg-background"
          style="width: {aiSidebarFallbackWidth}px; min-width: {aiSidebarFallbackWidth}px; max-width: {aiSidebarFallbackWidth}px"
          data-studio-region="ai-sidebar"
        >
          <TabLoading />
        </div>
      {:then { default: AiSidebar }}
        <svelte:boundary>
          {#snippet failed(err, reset)}
            <div class="flex h-full min-h-0 shrink-0 flex-col items-center justify-center gap-3 border-l border-border/50 bg-background p-4 text-center"
              style="width: {aiSidebarFallbackWidth}px; min-width: {aiSidebarFallbackWidth}px; max-width: {aiSidebarFallbackWidth}px">
              <AlertTriangle class="size-5 text-destructive" />
              <div class="space-y-1">
                <p class="text-ui-xs font-medium text-foreground">AI sidebar error</p>
                <p class="font-mono text-ui-3xs text-muted-foreground break-words">{err instanceof Error ? err.message : String(err)}</p>
              </div>
              <button
                type="button"
                class="rounded-md border border-border bg-muted/40 px-2.5 py-1 text-ui-xs font-medium transition-colors hover:bg-accent"
                onclick={reset}
              >Reload</button>
            </div>
          {/snippet}
          <AiSidebar
            bind:this={aiSidebarRef}
            schemaContext={aiSchemaContext}
            {connectionId}
            isActive={aiSidebarOpen && !aiMode}
            currentView={activeTab?.kind ?? 'welcome'}
            currentSql={sqlText}
            currentCode={ormCode}
            {ormMode}
            onclose={toggleAiSidebar}
            onaccept={(d) => void handleAiSidebarAccept(d)}
            onopensettings={() => (showAiModelSettings = true)}
          />
        </svelte:boundary>
      {/await}
    </div>
  {/if}
</div>

{#if queryLogOpen && connection}
  <QueryLogConsole {activeSchema} activeTable={activeTable ?? ''} onclose={() => { queryLogOpen = false }} />
{/if}

{#if statusBarVisible}
<StatusBar
  {connection}
  {connectionLost}
  {savedConnections}
  {activeConnectionId}
  {queryMs}
  selectedCount={selected.size}
  {pendingEditCount}
  applying={savingCell || deletingRows || insertingRow}
  onapplyedits={() => void applyEdits()}
  onresetedits={() => resetEdits()}
  oncopyeditssql={() => void copyEditsSql()}
  showTableNav={activeTab?.kind === 'table'}
  onscrolltabletop={() => scrollTableTop()}
  onscrolltablebottom={() => scrollTableBottom()}
  canScrollTableHorizontally={tableCanScrollH}
  onscrolltableleft={() => scrollTableLeft()}
  onscrolltableright={() => scrollTableRight()}
  live={liveEnabled}
  {liveSupported}
  ontogglelive={() => { liveEnabled = !liveEnabled }}
  onswitchconnection={handleSwitchDatabase}
  {mcpRunning}
  hasUpdate={statusBarHasUpdate}
  onopenmcp={() => (showMcpPanel = true)}
  onconnect={() => (showConnectionModal = true)}
  onswitchtodb={switchToDb}
  onswitchd1database={switchD1Database}
  onswitchproviderdb={switchProviderDb}
  oncheckupdate={() => updateDialog?.checkNow()}
  onopenmodelsettings={() => (showAiModelSettings = true)}
  sidebarVisible={sidebarOpen}
  {statusBarVisible}
  {tabBarVisible}
  {tableToolbarVisible}
  ontoggleSidebar={toggleSidebar}
  ontoggletabbar={toggleTabBar}
  ontoggletabletoolbar={toggleTableToolbar}
  ontogglestatusbar={toggleStatusBar}
  {aiMode}
  onopenaimode={() => (aiMode ? exitAiMode() : openAiTab())}
  onopensettings={() => (showSettingsModal = true)}
  onopencommand={() => (commandOpen = true)}
  onopenpages={() => { commandPage = 'pages'; commandOpen = true }}
  bind:readonly={tableReadonly}
  ondisconnect={requestDisconnect}
  oncreatedatabase={createDatabase}
/>
{/if}

<!-- Floating tab drag preview (follows the cursor during a split-pane drag) -->
{#if dragGhost}
  <div
    class="pointer-events-none fixed z-[200] -translate-x-1/2 -translate-y-1/2 rounded-md border border-border/60 bg-panel px-3 py-1.5 text-ui-xs font-medium text-foreground opacity-90 elevate-2-rim"
    style="left:{dragGhost.x}px; top:{dragGhost.y}px"
  >
    {dragGhost.title}
  </div>
{/if}

<!-- In-app confirm (window.confirm is blocked in the Tauri webview) -->
{#if confirmDialog}
  <div
    class="fixed inset-0 z-[100] flex items-center justify-center bg-black/65 p-4"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="confirm-dialog-message"
    onclick={(e) => { if (e.target === e.currentTarget) resolveConfirm(false) }}
    onkeydown={(e) => {
      if (e.key === 'Escape') { e.preventDefault(); resolveConfirm(false); return }
      // Enter on a focused button is the button's own job - this only covers the
      // case where focus is parked on the shell itself.
      if (e.key === 'Enter' && e.target === e.currentTarget) { e.preventDefault(); resolveConfirm(true) }
    }}
    tabindex="-1"
    use:focusTrap
  >
    <div class="w-full max-w-sm rounded-2xl border border-border/60 bg-background p-5 elevate-3-rim">
      <p id="confirm-dialog-message" class="text-ui-sm text-foreground">{confirmDialog.message}</p>
      <div class="mt-4 flex justify-end gap-2">
        <button
          type="button"
          onclick={() => resolveConfirm(false)}
          class="inline-flex h-8 items-center rounded-md px-3 text-ui-xs text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-0 focus-visible:outline-ring"
        >Cancel</button>
        <button
          type="button"
          data-autofocus
          onclick={() => resolveConfirm(true)}
          class="inline-flex h-8 items-center rounded-md bg-destructive px-3 text-ui-xs font-medium text-destructive-foreground hover:opacity-90 focus-visible:outline-2 focus-visible:outline-offset-0 focus-visible:outline-ring"
        >{confirmDialog.confirmLabel}</button>
      </div>
    </div>
  </div>
{/if}
</div>
