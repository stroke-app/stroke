<script>
  import { comboTitle } from '$lib/shortcuts.js'
  import { untrack } from "svelte";
  import { getAppScale } from '$lib/app-zoom.js';
  import { createHotkey } from "@tanstack/svelte-hotkeys";
  import Icon from "./Icon.svelte";
  import SearchableMenu from "./SearchableMenu.svelte";
  import FindReplacePanel from "./FindReplacePanel.svelte";
  import { listDatabases, canSwitchDatabase, currentDatabaseKey } from "$lib/databases.js";
  import { dbAdminKind, dbActionBlocker } from "$lib/database-admin.js";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import DangerousActionDialog from "./DangerousActionDialog.svelte";
  import { tableDialect } from "$lib/table-admin.js";
  import { readOnlyMode, guardWrite, READ_ONLY_HINT } from "$lib/stores/read-only.js";
  import { appNativeScroll, appSidebarComments } from "$lib/stores/settings.js";
  import SidebarObjects from "./SidebarObjects.svelte";
  import { objectsVersion } from "$lib/stores/sidebar-objects.svelte.js";
  import { listObjectComments } from "$lib/api.js";
  import { smoothScroll } from "$lib/smooth-scroll.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import PanelRight from "@lucide/svelte/icons/panel-right";
  import PanelLeft from "@lucide/svelte/icons/panel-left";
  import ResizeHandle from "./ResizeHandle.svelte";
  import ConnectionsSidebarPanel from "./ConnectionsSidebarPanel.svelte";
  import ExtensionsSidebarPanel from "./ExtensionsSidebarPanel.svelte";
  import { Button } from '$lib/components/ui/button/index.js'
  import { cn } from "$lib/utils.js";
  import { CRASH_WORD, isMagic, armCrash } from '$lib/games/easter-eggs.js'
  import { flushSync, tick } from "svelte";
  import { t } from "$lib/i18n.js";
  import { visibleRowCount, soleMatch } from "$lib/sidebar-filter.js";
  import { splitSchemas, isSystemSchema } from "$lib/system-schemas.js";
  import { formatTableRowCount, compareCreated } from "$lib/table-list.js";
  import {
    clampNavSidebarWidth,
    loadLayout,
    saveLayout,
  } from "$lib/stores/layout.js";

  const initialLayout = loadLayout();
  let width = $state(initialLayout.navSidebarWidth);
  let resizeStartWidth = initialLayout.navSidebarWidth;
  /** App scale sampled at drag start - `dx` is screen px, `width` is px at 100%. */
  let resizeScale = 1;

  let {
    connectionName = "",
    /** A table tab is open: Mod+Alt+Left/Right there jumps to its first / last
     *  column, and cycles these sections only while focus is in the sidebar. */
    tableNav = false,
    /** Which sidebar panel is showing: 'tables' | 'connections' | 'extensions'. */
    navSidebarPanel = "tables",
    /** Saved connections list (Connections panel). @type {import('$lib/stores/connections.js').SavedConnection[]} */
    connections = [],
    /** id of the currently live connection (Connections panel highlight). */
    activeConnectionId = "",
    /** @type {(c: import('$lib/stores/connections.js').SavedConnection) => void} */
    onswitchconnection = () => {},
    onaddconnection = () => {},
    /** @type {(id: string) => void} */
    onremoveconnection = () => {},
    /** @type {(id: string, group: string | null) => void} */
    onsetconnectiongroup = () => {},
    ondisconnectconnection = () => {},
    /** Open an extension's detail tab (Extensions panel). @type {(ext: any) => void} */
    onopenextensiondetail = () => {},
    /** Which side the sidebar docks to. @type {'left' | 'right'} */
    side = "left",
    /** Ask the shell to dock the sidebar to the given side. @type {(side: 'left' | 'right') => void} */
    onmoveside = () => {},
    schemas = [],
    tables = [],
    activeSchema = $bindable("public"),
    activeTable = null,
    activeView = "table",
    tableFilter = "",
    loadingTables = false,
    onschemachange = () => {},
    ontableselect = () => {},
    ontablefilter = () => {},
    onrefresh = () => {},
    onnewtable = () => {},
    onnewschema = () => {},
    /** @type {import('$lib/stores/connections.js').SavedConnection | null} */
    connection = null,
    ontruncatetable = /** @type {(table: string) => void} */ (() => {}),
    ondroptable = /** @type {(table: string, cascade: boolean, kind: import('$lib/table-admin.js').ObjectKind) => void} */ (() => {}),
    /** @type {import('$lib/stores/recent-tabs.js').RecentTab[]} */
    recentTabs = [],
    onrecentselect = /** @type {(schema: string, table: string) => void} */ (() => {}),
    onrecentremove = /** @type {(schema: string, table: string) => void} */ (() => {}),
    onrecentclear = () => {},
    /** Switch the live connection to another database on the same server.
     *  @type {(entry: { key: string, label: string }) => void} */
    onswitchdatabase = () => {},
    onswitchdatabasenow = /** @type {(db: { key: string, label: string }) => void} */ (() => {}),
    /** Open the Create database dialog. */
    onnewdatabase = () => {},
    /** Server-level database actions. Each takes the row's name, plus the full
     *  list so the dialogs can check the new name against it.
     *  @type {(args: { name: string, existing: string[] }) => void} */
    onrenamedatabase = () => {},
    /** @type {(args: { name: string, existing: string[] }) => void} */
    onduplicatedatabase = () => {},
    /** @type {(args: { name: string }) => void} */
    ondropdatabase = () => {},
    /** @type {(args: { name: string }) => void} */
    ondatabaseinfo = () => {},
    /** Close every other session on a database (Postgres). @type {(args: { name: string }) => void} */
    onterminatedbsessions = () => {},
    /** Bumped by the shell to force a refetch of the database list. */
    databasesRefreshKey = 0,
    onviewddl = /** @type {(table: string) => void} */ (() => {}),
    onexportsql = /** @type {(table: string) => void} */ (() => {}),
    onexportdata = /** @type {(table: string) => void} */ (() => {}),
    /** Names of tables that currently have an open tab (current schema). @type {string[]} */
    openTables = [],
    onclosetable = /** @type {(table: string) => void} */ (() => {}),
    /** Open the table with its structure view active. */
    onviewstructure = /** @type {(table: string) => void} */ (() => {}),
    /** Open a SELECT for the table in a SQL console tab. */
    onopeninconsole = /** @type {(table: string) => void} */ (() => {}),
    /** Open SQL for a schema object in a new editor tab: its definition, or a
     *  CREATE template with snippet fields. @type {(sql: { text: string, title: string, snippet?: string }) => void} */
    onopenobjectsql = () => {},
    /** Open the Generate SQL dialog (statement skeletons) for the table. */
    ongeneratesql = /** @type {(table: string) => void} */ (() => {}),
    /** Open the ERD scoped to a table + its FK-connected neighbors. */
    onopentableerd = /** @type {(table: string) => void} */ (() => {}),
    /** Count the table's rows and toast the result. */
    oncountrows = /** @type {(table: string) => void} */ (() => {}),
    /** Copy the table's column names as a comma-separated list. */
    oncopycolumns = /** @type {(table: string) => void} */ (() => {}),
    // ── Find & replace panel ────────────────────────────────────────────────
    // The panel works on the rows the grid has loaded, so the data comes from
    // the shell rather than being fetched again here.
    /** @type {Array<{ name: string, dataType?: string }>} */
    frColumns = [],
    /** @type {unknown[][]} */
    frRows = [],
    /** @type {string | null} */
    frTableName = null,
    frEnabled = false,
    // The key columns, so the panel can refuse to rewrite them. Replacing
    // inside a foreign key is what produced "FOREIGN KEY constraint failed"
    // from D1 - the new value referenced a parent row that does not exist.
    /** @type {string[]} */
    frPrimaryKey = [],
    /** @type {Array<{ columns: string[] }>} */
    frForeignKeys = [],
    /** @type {(edits: Array<{ rowIdx: number, colIdx: number, value: string }>) => Promise<void>} */
    onfindreplaceapply = async () => {},
    /** Put the grid's cell cursor on a match. */
    onrevealcell = /** @type {(rowIdx: number, colIdx: number) => void} */ (() => {}),
    /** Assigned here; the shell calls it to show the panel and focus its field. */
    openFindReplace = $bindable(/** @type {() => void} */ (() => {})),
    /**
     * Assigned here; the shell calls it once a database switch has landed.
     *
     * This used to be inferred in the sidebar, by bookmarking `activeDbKey` and
     * watching it change. The shell is the only place that actually knows a
     * switch happened - it is the thing that performs one - and an inference
     * that has to survive a disconnect, a reconnect and a rebuilt connection
     * object is a guess with three ways to be wrong.
     */
    showTablesTab = $bindable(/** @type {() => void} */ (() => {})),
    /** The Queries tab's panel (saved queries in folders), drawn by the shell
     *  that owns the editor tabs. @type {import('svelte').Snippet | undefined} */
    queriesPanel = undefined,
    /** How many saved queries the connection has, for the tab's label. */
    queriesCount = 0,
  } = $props();

  const openTableSet = $derived(new Set(openTables))

  // PostgreSQL is the only driver where "schema" is a CREATE SCHEMA namespace.
  // (MySQL schemas are databases; SQLite/D1 have none.) Gate the "New schema" action on it.
  const supportsSchemas = $derived(connection?.type === "postgres")

  let localFilter = $state(untrack(() => tableFilter));
  // Debounced mirror of localFilter that the expensive list filtering derives
  // from. localFilter drives the input (instant typing feedback); the O(n) filter
  // + sort only re-runs once typing settles, not on every keystroke.
  let debouncedFilter = $state(untrack(() => tableFilter));
  let filterEl = $state(/** @type {HTMLInputElement | null} */ (null));
  let filterDebounce = /** @type {ReturnType<typeof setTimeout> | null} */ (
    null
  );

  // Section open/collapsed state - persisted across sidebar toggles
  const SIDEBAR_EXPAND_KEY = 'stroke:sidebar-sections'
  function loadSidebarSections() {
    try {
      const raw = localStorage.getItem(SIDEBAR_EXPAND_KEY)
      if (raw) return JSON.parse(raw)
    } catch {}
    return { tables: true, views: false, matViews: false }
  }
  function saveSidebarSection(key, value) {
    try {
      const current = loadSidebarSections()
      localStorage.setItem(SIDEBAR_EXPAND_KEY, JSON.stringify({ ...current, [key]: value }))
    } catch {}
  }

  const databasesOpen = $derived(sidebarTab === 'databases')
  /** @type {import('$lib/databases.js').DatabaseEntry[]} */
  let dbEntries = $state([]);
  let dbEntriesLoading = $state(false);
  let dbEntriesLoaded = $state(false);
  let dbEntriesError = $state('');

  // Listing databases costs a round trip (a catalog query, or a Cloudflare /
  // provider API call), so it waits for the section to be expanded rather than
  // firing on every sidebar mount.
  async function loadDatabases() {
    if (dbEntriesLoading) return
    dbEntriesLoading = true
    dbEntriesError = ''
    try {
      dbEntries = await listDatabases(connection)
      dbEntriesLoaded = true
    } catch (e) {
      dbEntriesError = String(e)
    } finally {
      dbEntriesLoading = false
    }
  }

  // Load whenever the section is open and holds nothing for this connection.
  // Hanging the fetch off the toggle alone missed both cases that matter: the
  // section restoring already-expanded from the persisted prefs, and a
  // connection switch invalidating the list while it stayed open - each showed
  // an "empty" list that had never been fetched.
  $effect(() => {
    const conn = connection
    const isOpen = databasesOpen
    if (!isOpen || !conn) return
    untrack(() => {
      if (!dbEntriesLoaded && !dbEntriesLoading) void loadDatabases()
    })
  })

  /**
   * Which server a loaded database list belongs to. Not the database - switching
   * database rebuilds the connection object, and the list of databases ON that
   * server is the same list either way.
   * @param {any} c
   */
  function dbServerKey(c) {
    if (!c) return ''
    return [c.type ?? '', c.host ?? '', c.port ?? '', c.filePath ?? '', c.accountId ?? '', c.url ?? ''].join('|')
  }
  let dbEntriesServer = untrack(() => dbServerKey(connection))

  // A new connection invalidates the list, so the next expand refetches it. The
  // rows only get dropped when the SERVER changes though: every database switch
  // rebuilds the connection, and blanking the list on each one is what made
  // switching flash - the panel emptied, drew its loading rows, then refilled
  // with the same names and the tick on a different row. Now it refetches
  // underneath the rows it already has.
  $effect(() => {
    const server = dbServerKey(connection)
    untrack(() => {
      if (server !== dbEntriesServer) {
        dbEntries = []
        dbEntriesServer = server
      }
      dbEntriesLoaded = false
      dbEntriesError = ''
    })
  })

  // The shell bumps this after a create/rename/duplicate/drop, since the list it
  // invalidated lives here. Refetch rather than patch: the statement may have
  // failed halfway, and the server is the only honest source.
  $effect(() => {
    if (databasesRefreshKey === 0) return
    databasesRefreshKey
    untrack(() => { void loadDatabases() })
  })

  const canSwitchDb = $derived(canSwitchDatabase(connection))
  const activeDbKey = $derived(currentDatabaseKey(connection))
  const dbAdmin = $derived(dbAdminKind(connection))
  /** Names of every listed database, for the dialogs' collision checks. */
  const dbNames = $derived(dbEntries.map((d) => d.label))

  /** Menu item state for one database row: enabled, or disabled with a reason.
   *  @param {import('$lib/database-admin.js').AdminAction} action @param {boolean} isCurrent */
  function dbItem(action, isCurrent) {
    if ($readOnlyMode) return { disabled: true, title: READ_ONLY_HINT }
    const blocker = dbActionBlocker(action, connection, { isCurrent })
    return { disabled: !!blocker, title: blocker || undefined }
  }

  // ── Pinned tables ─────────────────────────────────────────────────────────
  const PINNED_KEY = 'stroke:pinned-tables'

  function loadPinnedAll() {
    try {
      const raw = localStorage.getItem(PINNED_KEY)
      return raw ? JSON.parse(raw) : {}
    } catch { return {} }
  }

  function savePinnedAll(data) {
    try { localStorage.setItem(PINNED_KEY, JSON.stringify(data)) } catch {}
  }

  let _allPinned = $state(loadPinnedAll())
  const _connKey = $derived(connection?.id ?? '')
  const pinnedTables = $derived(_allPinned[_connKey] ?? [])

  // Only show pinned tables that still exist in the current table list
  const _tableNameSet = $derived(new Set(tables.map((t) => t.name)))
  const _rowCountByName = $derived(new Map(tables.map((t) => [t.name, t.rowCount])))
  const visiblePinnedTables = $derived(pinnedTables.filter((n) => _tableNameSet.has(n)))

  /**
   * The reduced-motion rule from app.css, in JS.
   *
   * Needed because `scrollTo({ behavior: 'smooth' })` states the behaviour
   * explicitly, and an explicit behaviour is NOT overridden by the
   * `scroll-behavior: auto !important` that the stylesheet applies under the
   * media query - that one only decides what `behavior: 'auto'` means. A
   * scripted smooth scroll has to ask the question itself.
   */
  function prefersReducedMotion() {
    const mode = document.documentElement.getAttribute('data-motion')
    if (mode === 'full') return false      // Settings → Appearance overrides the OS
    if (mode === 'reduced') return true
    return typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches
  }

  /**
   * Pin or unpin, and put the scroll offset where the row went.
   *
   * The two directions want opposite things, because the row moves in opposite
   * directions:
   *
   *   PINNING moves the row UP, out of the table list and into the Pinned
   *   section at the top. Scroll there and follow it - the point of pinning is
   *   to put a table somewhere you can find it, and a pin that silently files
   *   the row off-screen never shows you where that somewhere is. The scroll is
   *   also the only confirmation the action worked, since the row vanishes from
   *   where you clicked it.
   *
   *   UNPINNING moves the row DOWN, back into the table list at its sorted
   *   position - which is somewhere you did not ask to go. So hold the list
   *   still instead. Without that, the Pinned section shrinking by one row drags
   *   everything below it up and the list appears to scroll on its own.
   *
   * Holding it still is scroll anchoring, which browsers do natively with
   * `overflow-anchor` and WebKit - the engine this app ships on - does not
   * implement. Doing it by hand is the whole of it: keep one element where it
   * was and move the offset by however far it travelled.
   */
  function togglePin(tableName) {
    const root = scrollContainerEl
    const current = _allPinned[_connKey] ?? []
    const pinning = !current.includes(tableName)

    // Only unpinning needs an anchor; pinning is going to the top regardless.
    // The toggled row is the one that moves, so it can never BE the anchor -
    // take the first other row at or below the viewport's top edge, since rows
    // above it may sit in the section that is about to change size.
    /** @type {HTMLElement | null} */
    let anchor = null
    let beforeY = 0
    if (!pinning && root) {
      const rootTop = root.getBoundingClientRect().top
      for (const el of root.querySelectorAll('li[data-table], li[data-pin]')) {
        const li = /** @type {HTMLElement} */ (el)
        if (li.dataset.table === tableName || li.dataset.pin === tableName) continue
        const y = li.getBoundingClientRect().top - rootTop
        if (y >= 0) { anchor = li; beforeY = y; break }
      }
    }

    const next = pinning
      ? [...current, tableName]
      : current.filter((n) => n !== tableName)
    _allPinned = { ..._allPinned, [_connKey]: next }
    savePinnedAll(_allPinned)

    if (!root) return
    // `flushSync` rather than `tick()`: both paths measure or move the scroll
    // offset against the NEW list, and a frame later is not a fix - it is the
    // jump, followed by a correction you can see.
    flushSync()

    if (pinning) {
      // Safe against the wheel-ease controller on this container: it adopts any
      // offset it did not set (`onScroll` → `sync`), and the pointerdown that
      // delivered this click already stopped it.
      root.scrollTo({ top: 0, behavior: prefersReducedMotion() ? 'auto' : 'smooth' })
      return
    }
    // Both lists are keyed by table name, so the anchor's node survives the
    // re-render. Checked anyway - a filter settling in the same flush could drop it.
    if (!anchor || !anchor.isConnected) return
    const afterY = anchor.getBoundingClientRect().top - root.getBoundingClientRect().top
    root.scrollTop += afterY - beforeY
  }

  function clearAllPins() {
    _allPinned = { ..._allPinned, [_connKey]: [] }
    savePinnedAll(_allPinned)
  }

  // ── Display preferences ───────────────────────────────────────────────────
  const DISPLAY_PREFS_KEY = 'stroke:sidebar-display'
  function loadDisplayPrefs() {
    try {
      const raw = localStorage.getItem(DISPLAY_PREFS_KEY)
      if (raw) return JSON.parse(raw)
    } catch {}
    return { showTables: true, showViews: true, showMatViews: true, showRecent: true, showDatabases: true, sortBy: 'name', showPins: true, showRowCount: true, sortDir: 'asc', hideEmpty: false, hideSystem: false }
  }
  function saveDisplayPrefs(prefs) {
    try { localStorage.setItem(DISPLAY_PREFS_KEY, JSON.stringify(prefs)) } catch {}
  }

  const _dp = loadDisplayPrefs()

  /**
   * One list at a time, chosen from the strip at the top of the sidebar.
   *
   * This replaces six independently-collapsible sections stacked in one scroll
   * container. That layout had two problems no amount of styling fixes: the
   * height of everything below a section moved every time one was opened, so
   * nothing in the panel held still; and with several open at once the list you
   * were actually looking for was usually below the fold, which is what the
   * accordion was supposed to prevent. A tab strip costs one fixed row and the
   * list underneath always starts at the same place.
   *
   * Views, materialized views and the schema's other objects (functions,
   * procedures, triggers, sequences, types, events) share the Objects tab, one
   * collapsible group per kind the engine has. It was the Views tab; a stored
   * 'views' reads as 'objects'.
   * @typedef {'tables' | 'objects' | 'recent' | 'databases' | 'queries' | 'search'} SidebarTab
   */
  const SIDEBAR_TAB_KEY = 'stroke:sidebar-tab'
  /** @type {{ id: SidebarTab, label: string, icon: string }[]} */
  const SIDEBAR_TABS = [
    { id: 'tables',    label: 'Tables',    icon: 'table-2' },
    // Second, not last: switching database is a navigation move you make as
    // often as switching schema, and it was sitting behind three lists you visit
    // far less.
    { id: 'databases', label: 'Databases', icon: 'database' },
    // Saved queries, in folders: the SQL you keep is what you open next.
    { id: 'queries',   label: 'Queries',   icon: 'file-code' },
    { id: 'objects',   label: 'Objects',   icon: 'blocks' },
    { id: 'recent',    label: 'Recent',    icon: 'clock' },
    // Last, and not a list: find & replace is a tool that works on the table
    // you already have open, so it belongs where the other panels live rather
    // than in a modal over the rows it is about to rewrite.
    { id: 'search',    label: 'Find & replace', icon: 'replace' },
  ]
  function loadSidebarTab() {
    try {
      const stored = localStorage.getItem(SIDEBAR_TAB_KEY)
      const raw = stored === 'views' ? 'objects' : stored
      if (SIDEBAR_TABS.some((t) => t.id === raw)) return /** @type {SidebarTab} */ (raw)
    } catch {}
    return /** @type {SidebarTab} */ ('tables')
  }
  let sidebarTab = $state(loadSidebarTab())

  $effect(() => {
    openFindReplace = () => {
      sidebarTab = 'search'
      // After the tab renders, or the field is not in the DOM yet.
      tick().then(() => focusFindField())
    }
    showTablesTab = () => { sidebarTab = 'tables' }
  })
  /** Assigned by the panel. */
  let focusFindField = $state(/** @type {() => void} */ (() => {}))

  /**
   * Keyboard access to the strip. Registered here rather than in StudioShell
   * because the state and the tab list both live here - a hotkey that has to
   * reach across a component boundary to set one field is how that field ends up
   * lifted for no other reason.
   *
   *   ⌘⇧1-5      jump straight to a tab
   *   ⌘⌥← / ⌘⌥→  cycle, wrapping at both ends
   *
   * ⌘1-9 is already "go to editor tab" and ⌘⌥1-9 is "switch saved connection",
   * so neither of those ranges was free.
   */
  /** @param {number} delta */
  function cycleSidebarTab(delta) {
    const i = SIDEBAR_TABS.findIndex((t) => t.id === sidebarTab)
    const next = (i + delta + SIDEBAR_TABS.length) % SIDEBAR_TABS.length
    sidebarTab = SIDEBAR_TABS[next].id
  }
  SIDEBAR_TABS.forEach((tab, i) => {
    createHotkey(`Mod+Shift+${i + 1}`, (e) => {
      if (!connectionName) return
      e.preventDefault()
      sidebarTab = tab.id
    })
  })
  /** Mod+Alt+Left/Right is the table's (first / last column) unless focus is here. */
  const yieldsArrowNav = () =>
    tableNav && !(document.activeElement instanceof Element && document.activeElement.closest('[data-studio-region="sidebar"]'))
  createHotkey('Mod+Alt+ArrowRight', (e) => {
    if (!connectionName || yieldsArrowNav()) return
    e.preventDefault()
    cycleSidebarTab(1)
  })
  createHotkey('Mod+Alt+ArrowLeft', (e) => {
    if (!connectionName || yieldsArrowNav()) return
    e.preventDefault()
    cycleSidebarTab(-1)
  })
  /** Pending single-click database switch, held so a second click can cancel it. */
  let dbClickTimer = /** @type {ReturnType<typeof setTimeout> | null} */ (null)
  /** @param {{ key: string, label: string }} db */
  function onDbClick(db) {
    if (dbClickTimer) clearTimeout(dbClickTimer)
    dbClickTimer = setTimeout(() => { dbClickTimer = null; focusListAfterSwitch(); onswitchdatabase(db) }, 220)
  }
  /** @param {{ key: string, label: string }} db */
  function onDbDblClick(db) {
    if (dbClickTimer) { clearTimeout(dbClickTimer); dbClickTimer = null }
    focusListAfterSwitch()
    onswitchdatabasenow(db)
  }

  /**
   * What an empty tab says. `filtered` fires when the list has rows but the
   * filter matched none - a different problem from having nothing at all, and
   * one the user can fix by clearing the box rather than by creating anything.
   * @type {Record<SidebarTab, { icon: string, title: string, hint: string }>}
   */
  const TAB_EMPTY = {
    tables:    { icon: 'table-2',    title: 'No tables',    hint: 'Nothing in this schema yet.' },
    objects:   { icon: 'blocks',     title: 'No objects',   hint: 'Views, functions, triggers and the like show up here.' },
    recent:    { icon: 'clock',      title: 'No recents',   hint: 'Tables you open appear here.' },
    databases: { icon: 'database',   title: 'No databases', hint: 'Nothing else on this server.' },
  }
  const tabIsEmpty = $derived(
    // Objects always draws its group headers: an empty group still has its +.
    sidebarTab !== 'search' && sidebarTab !== 'queries' && sidebarTab !== 'objects' && !loadingTables && !!connectionName && tabCounts[sidebarTab] === 0,
  )
  /** True when the tab has rows but the filter hid all of them. */
  const tabEmptyFromFilter = $derived(
    tabIsEmpty &&
      !!debouncedFilter &&
      (sidebarTab === 'tables'
        ? regularTablesUnpinned.length + visiblePinnedTables.length > 0
        : sidebarTab === 'objects'
          ? views.length + matViews.length > 0
          : sidebarTab === 'databases'
            ? dbEntries.length > 0
            : recentTables.length > 0),
  )

  /** How many rows each tab holds BEFORE the filter. @type {Record<SidebarTab, number>} */
  const tabTotals = $derived({
    // Pinned rows render at the top of this tab, so they count towards it. Only
    // pins whose table still exists are counted, because only those draw a row.
    tables: regularTablesUnpinned.length + visiblePinnedTables.length,
    objects: objectsTotal,
    // The recents list is capped at 5 rows, so that is the denominator too.
    recent: Math.min(recentTabs.length, 5),
    databases: dbEntries.length,
    queries: queriesCount,
    search: 0,
  })

  /** How many rows each tab holds, after the filter. @type {Record<SidebarTab, number>} */
  const tabCounts = $derived({
    tables: filteredRegularTables.length + filteredPinnedTables.length,
    objects: objectsShown,
    recent: Math.min(filteredRecent.length, 5),
    databases: filteredDbEntries.length,
    queries: queriesCount,
    search: 0,
  })
  $effect(() => { try { localStorage.setItem(SIDEBAR_TAB_KEY, sidebarTab) } catch {} })


  // The old per-section visibility flags are now just "is this the open tab".
  // Keeping the names means the ~900 lines of list markup below did not have to
  // be rewritten to ask a different question.
  const showTables    = $derived(sidebarTab === 'tables')
  const showRecent    = $derived(sidebarTab === 'recent')
  // Pinned is a section of Tables, not a tab of its own. It was a fifth icon in
  // the strip that held, for most connections, nothing at all - and it split
  // "the tables in this schema" across two places you had to switch between to
  // see. It renders above the Tables header, which is where the pins already
  // sorted in `selectableOrder`.
  const showPins      = $derived(sidebarTab === 'tables')
  const showDatabases = $derived(sidebarTab === 'databases')
  // Nothing collapses any more, so every list in the open tab is open.
  const recentOpen = true, tablesOpen = true
  // The engine's own schemas (`pg_catalog`, `information_schema`, `sys`…) are
  // loaded like any other - they are browsable, and sometimes the thing you
  // actually need - but they are not where anyone keeps data, so the picker
  // holds them behind one entry rather than burying `public` among them.
  let showSystemSchemas = $state(_dp.showSystemSchemas ?? false)
  const splitSchemaList = $derived(splitSchemas(schemas))
  const SYSTEM_TOGGLE = '\u0000system-schemas'
  const schemaMenuItems = $derived([
    ...splitSchemaList.user.map((sc) => ({ value: sc, label: sc })),
    ...(showSystemSchemas ? splitSchemaList.system.map((sc) => ({ value: sc, label: sc })) : []),
    ...(splitSchemaList.system.length
      ? [{
          value: SYSTEM_TOGGLE,
          label: showSystemSchemas
            ? 'Hide system schemas'
            : `Show system schemas (${splitSchemaList.system.length})`,
        }]
      : []),
  ])

  let showRowCount = $state(_dp.showRowCount ?? true)
  let hideEmpty = $state(_dp.hideEmpty ?? false)
  let hideSystem = $state(_dp.hideSystem ?? false)
  /** @type {'name' | 'rowCount' | 'created'} */
  let sortBy = $state(_dp.sortBy ?? 'name')
  /** @type {'asc' | 'desc'} */
  let sortDir = $state(_dp.sortDir ?? 'asc')

  $effect(() => { saveDisplayPrefs({ sortBy, showRowCount, sortDir, hideEmpty, hideSystem, showSystemSchemas }) })

  /** System / migration tables that are usually noise: `_prisma_migrations`, `pg_*`, `sqlite_*`, leading-underscore. */
  function isSystemTable(/** @type {string} */ name) {
    return /^(_|pg_|sql_|sqlite_)/i.test(name)
  }

  // ── Shared context-menu target ────────────────────────────────────────────
  // Each long list owns ONE ContextMenu.Root; the row that was right-clicked is
  // recorded here from the event, the way DataTable does it for the grid. Keeps
  // the per-row cost to a <button>, which is what makes an unwindowed list of a
  // few thousand tables affordable.
  let menuTable = $state('')

  // ── Selection state ───────────────────────────────────────────────────────
  /** @type {Set<string>} */
  let selectedItems = $state(new Set())
  const menuTableSelected = $derived(selectedItems.has(menuTable))
  /** Anchor for shift range-select. @type {string | null} */
  let lastSelectedName = $state(null)

  /** @param {string} name */
  function toggleSelect(name) {
    const next = new Set(selectedItems)
    if (next.has(name)) next.delete(name)
    else next.add(name)
    selectedItems = next
  }

  // ── Bulk actions on the current multi-selection ───────────────────────────
  /** True when every selected table is already pinned. */
  const allSelectedPinned = $derived(
    selectedItems.size > 0 && [...selectedItems].every((n) => pinnedTables.includes(n)),
  )

  /** Pin every selected table (no-op for ones already pinned). */
  function pinSelected() {
    const set = new Set(_allPinned[_connKey] ?? [])
    for (const n of selectedItems) set.add(n)
    _allPinned = { ..._allPinned, [_connKey]: [...set] }
    savePinnedAll(_allPinned)
    clearSelection()
  }

  /** Unpin every selected table. */
  function unpinSelected() {
    const next = (_allPinned[_connKey] ?? []).filter((n) => !selectedItems.has(n))
    _allPinned = { ..._allPinned, [_connKey]: next }
    savePinnedAll(_allPinned)
    clearSelection()
  }

  /**
   * Select, or deselect, every pinned table on screen.
   *
   * A toggle rather than a one-way "select all": having selected seven rows to
   * act on them, the way back was clicking each one again. Only the rows the
   * filter is showing are touched, so it always matches what you can see.
   */
  function toggleSelectAllPinned() {
    const names = filteredPinnedTables
    if (!names.length) return
    const allOn = names.every((n) => selectedItems.has(n))
    const next = new Set(selectedItems)
    for (const n of names) { if (allOn) next.delete(n); else next.add(n) }
    selectedItems = next
    lastSelectedName = allOn ? null : names[names.length - 1]
  }

  /** Open every pinned table on screen in its own tab, in the order shown. */
  function openAllPinned() {
    for (const n of filteredPinnedTables) ontableselect(n)
  }


  function copySelectedNames() {
    navigator.clipboard.writeText([...selectedItems].join('\n'))
    clearSelection()
  }

  function clearSelection() {
    selectedItems = new Set()
    lastSelectedName = null
  }

  // ── Dangerous action dialog ───────────────────────────────────────────────
  /** @type {'drop' | 'truncate'} */
  let dangerAction = $state('drop')
  let dangerTable = $state('')
  /** @type {import('$lib/table-admin.js').ObjectKind} */
  let dangerObjectKind = $state('table')
  let dangerCascade = $state(false)
  let dangerOpen = $state(false)
  /** How this engine spells DROP and TRUNCATE; null where it has neither
   *  (Redis, PostHog), which hides the items instead of offering a failure. */
  const tableDdl = $derived(tableDialect(connection?.type))

  /** @param {'drop' | 'truncate'} kind @param {string} tableName
   *  @param {import('$lib/table-admin.js').ObjectKind} [objectKind] */
  function openDangerDialog(kind, tableName, objectKind = 'table') {
    // The menu items are disabled in read-only mode, but the guard stays: a
    // keyboard-driven select on a disabled item is one bits-ui version away.
    if (!guardWrite(kind === 'drop' ? `drop this ${objectKind === 'table' ? 'table' : 'view'}` : 'truncate this table')) return
    dangerAction = kind
    dangerTable = tableName
    dangerObjectKind = objectKind
    dangerCascade = false
    dangerOpen = true
  }

  function confirmDanger(cascade) {
    if (dangerAction === 'drop') ondroptable(dangerTable, cascade, dangerObjectKind)
    else ontruncatetable(dangerTable)
  }

  // Alt+Shift+1-5 to focus pinned tables (only existing ones)
  createHotkey('Alt+Shift+1', (e) => { e.preventDefault(); const t = visiblePinnedTables[0]; if (t) ontableselect(t) })
  createHotkey('Alt+Shift+2', (e) => { e.preventDefault(); const t = visiblePinnedTables[1]; if (t) ontableselect(t) })
  createHotkey('Alt+Shift+3', (e) => { e.preventDefault(); const t = visiblePinnedTables[2]; if (t) ontableselect(t) })
  createHotkey('Alt+Shift+4', (e) => { e.preventDefault(); const t = visiblePinnedTables[3]; if (t) ontableselect(t) })
  createHotkey('Alt+Shift+5', (e) => { e.preventDefault(); const t = visiblePinnedTables[4]; if (t) ontableselect(t) })

  // Sync from parent when it resets externally (e.g. connection change)
  $effect(() => {
    localFilter = tableFilter;
    debouncedFilter = tableFilter;
  });

  /** @param {string} value */
  function handleFilterInput(value) {
    // The fake crash. Exact whole-value match, so filtering for a `crash_logs`
    // table still filters - only a bare "crash" is the joke. Handled before the
    // debounce, so the filter never actually runs with it.
    if (isMagic(value, CRASH_WORD)) {
      localFilter = "";
      if (filterDebounce) clearTimeout(filterDebounce);
      filterDebounce = null;
      debouncedFilter = "";
      ontablefilter("");
      armCrash();
      return;
    }
    localFilter = value;
    if (filterDebounce) clearTimeout(filterDebounce);
    filterDebounce = setTimeout(() => {
      filterDebounce = null;
      debouncedFilter = value;
      ontablefilter(value);
    }, 200);
  }

  // Release the pending filter timer when the sidebar unmounts.
  $effect(() => () => {
    if (filterDebounce) clearTimeout(filterDebounce);
  });

  const lf = $derived(debouncedFilter.toLowerCase());
  // The filter box sits above every section, so it filters every section -
  // databases and recents included. Scoping it to tables meant typing a database
  // name emptied the table list and left the database sitting there unmatched.
  const filteredDbEntries = $derived(
    lf ? dbEntries.filter((d) => d.label.toLowerCase().includes(lf)) : dbEntries,
  );
  const filteredRecent = $derived(
    lf ? recentTabs.filter((r) => r.table.toLowerCase().includes(lf)) : recentTabs,
  );
  const pinnedSet = $derived(new Set(pinnedTables));

  const regularTables = $derived(
    tables.filter(
      (t) => !t.kind || t.kind === "table" || t.kind === "foreign_table",
    ),
  );
  const views = $derived(tables.filter((t) => t.kind === "view"));
  const matViews = $derived(
    tables.filter((t) => t.kind === "materialized_view"),
  );

  /** @param {any[]} list */
  function applySortBy(list) {
    let result = list
    if (hideEmpty) result = result.filter((t) => (t.rowCount ?? 0) > 0)
    if (hideSystem) result = result.filter((t) => !isSystemTable(t.name))
    if (sortBy === 'rowCount') {
      result = [...result].sort((a, b) => (b.rowCount ?? 0) - (a.rowCount ?? 0))
    } else if (sortBy === 'created') {
      // Oldest first; `desc` (the default for this sort) puts the newest on top.
      result = [...result].sort(compareCreated)
      if (sortDir === 'desc') result.reverse()
      return result
    }
    if (sortDir === 'desc' && sortBy === 'name') {
      result = [...result].reverse()
    } else if (sortDir === 'asc' && sortBy === 'rowCount') {
      result = [...result].reverse()
    }
    return result
  }

  // Sorted, hide-filtered, un-pinned base - recomputes only when the data, sort,
  // hide toggles or pins change (NOT on every keystroke). The search term then
  // just filters this base, so typing avoids the sort + array clones.
  const sortedRegularBase = $derived(
    applySortBy(regularTables.filter((t) => !pinnedSet.has(t.name))),
  );
  const filteredRegularTables = $derived(
    lf ? sortedRegularBase.filter((t) => t.name.toLowerCase().includes(lf)) : sortedRegularBase,
  );

  // Pinned rows were never filtered: in their own tab the filter box was the
  // only thing on screen that could act on them, and it did not. Sharing a tab
  // with the table list makes that a visible bug - type a name and the pins
  // would sit above the results untouched - so they take the same predicate.
  const filteredPinnedTables = $derived(
    lf ? visiblePinnedTables.filter((n) => n.toLowerCase().includes(lf)) : visiblePinnedTables,
  );

  /** Drives the pinned header's select-all toggle: its state and its label. */
  const allPinnedSelected = $derived(
    filteredPinnedTables.length > 0 &&
      filteredPinnedTables.every((n) => selectedItems.has(n)),
  );

  // Selectable rows in display order (pinned first, then regular) - drives shift range-select.
  const selectableOrder = $derived([
    ...filteredPinnedTables,
    ...filteredRegularTables.map((t) => t.name),
  ]);

  /**
   * Toggle a row's selection. With Shift held, extend a contiguous range from
   * the last-clicked anchor across the combined pinned + regular ordering.
   * @param {string} name @param {boolean} [shiftKey]
   */
  function selectItem(name, shiftKey = false) {
    // Range-select only extends an existing selection (needs an anchor already selected).
    if (shiftKey && lastSelectedName && lastSelectedName !== name && selectedItems.size > 0) {
      const a = selectableOrder.indexOf(lastSelectedName)
      const b = selectableOrder.indexOf(name)
      if (a !== -1 && b !== -1) {
        const [lo, hi] = a < b ? [a, b] : [b, a]
        const next = new Set(selectedItems)
        for (let i = lo; i <= hi; i++) next.add(selectableOrder[i])
        selectedItems = next
        lastSelectedName = name
        return
      }
    }
    toggleSelect(name)
    lastSelectedName = name
  }

  /** Open every selected table in its own tab. */
  function openSelected() {
    for (const n of selectableOrder) if (selectedItems.has(n)) ontableselect(n)
    clearSelection()
  }

  /** Close the tabs of every selected table that's currently open. */
  function closeSelectedTabs() {
    for (const n of selectedItems) if (openTableSet.has(n)) onclosetable(n)
    clearSelection()
  }
  const sortedViewsBase = $derived(applySortBy(views));
  const sortedMatViewsBase = $derived(applySortBy(matViews));
  const filteredViews = $derived(
    lf ? sortedViewsBase.filter((t) => t.name.toLowerCase().includes(lf)) : sortedViewsBase,
  );
  const filteredMatViews = $derived(
    lf ? sortedMatViewsBase.filter((t) => t.name.toLowerCase().includes(lf)) : sortedMatViewsBase,
  );
  // Views, materialized views and databases are windowed on the same maths as
  // the tables list (see the virtual-list block below). They used to render a
  // capped 500 rows instead, which both instantiated 500 context menus and hid
  // whatever came after row 500 without saying so.

  // ── Counts for section badges ──────────────────────────────────────────────
  // The TABLES list draws from regular tables minus pins; use that as the "total"
  // so pinning (which just relocates a row) doesn't read as a hidden/filtered row.
  const regularTablesUnpinned = $derived(regularTables.filter((t) => !pinnedSet.has(t.name)));
  /** How many rows the active filters (search / hide-empty / hide-system) are hiding right now. */
  const hiddenCount = $derived(
    Math.max(0, regularTablesUnpinned.length - filteredRegularTables.length) +
    Math.max(0, views.length - filteredViews.length) +
    Math.max(0, matViews.length - filteredMatViews.length),
  );
  /** Whether any non-default filter/sort is active (drives the Reset action). */
  const filtersActive = $derived(
    lf !== '' || hideEmpty || hideSystem || sortBy !== 'name' || sortDir !== 'asc',
  );

  /** Name of the open list, for the filter's accessible name and its live region. */
  const activeTabLabel = $derived(SIDEBAR_TABS.find((t) => t.id === sidebarTab)?.label ?? 'items')

  /** Everything the filter rule counts, in the shape `sidebar-filter.js` wants. */
  const filterLists = $derived({
    tables: filteredRegularTables, tablesTotal: regularTablesUnpinned.length,
    views: filteredViews, matViews: filteredMatViews, viewsTotal: views.length + matViews.length,
    recent: filteredRecent, recentTotal: recentTabs.length,
    pins: filteredPinnedTables, pinsTotal: visiblePinnedTables.length,
    databases: filteredDbEntries, databasesTotal: dbEntries.length,
    activeDbKey,
  })

  /** Rows the open tab is showing right now, against what it would show unfiltered. */
  const visibleRows = $derived(visibleRowCount(sidebarTab, filterLists))

  /**
   * The one row left when the filter has narrowed the open tab to exactly one -
   * Enter in the filter box opens it. The picking rule lives in
   * `sidebar-filter.js` under test; this only binds the result to the handler
   * that acts on it.
   * @returns {{ label: string, open: () => void } | null}
   */
  const soleResult = $derived.by(() => {
    if (!connectionName) return null
    const m = soleMatch(sidebarTab, filterLists)
    if (!m) return null
    switch (m.kind) {
      case 'table': return { label: m.name, open: () => ontableselect(m.name) }
      case 'recent': return { label: m.name, open: () => onrecentselect(m.schema, m.name) }
      case 'database': return { label: m.name, open: () => onswitchdatabase(m.entry) }
      default: return null
    }
  })

  /**
   * What the filter's live region says. Reads off `lf`, which is already
   * debounced, so it announces once the typing settles rather than per keystroke.
   */
  const filterStatus = $derived.by(() => {
    if (!connectionName) return ''
    const { shown, total } = visibleRows
    const list = activeTabLabel.toLowerCase()
    if (!lf) return `${total} ${list}`
    if (shown === 0) return `No ${list} match ${lf}`
    if (renderedRowCount === 1) return `1 of ${total} ${list}. Press Enter to open ${soleResult?.label ?? 'it'}.`
    return `${shown} of ${total} ${list}`
  })

  /**
   * How many rows the open list is drawing, kept in sync with the DOM.
   *
   * The model said one row and the chip did not appear, while the section
   * header - reading the very same expression - said 1/14. Rather than keep
   * hunting that, both the chip and Enter now read the rows themselves. There is
   * one source of truth for "is there exactly one thing here", and it is the
   * thing the user is looking at.
   *
   * The effect re-runs whenever any list or the open tab changes; `$effect` runs
   * after the DOM is updated, so the count it takes is the list as rendered.
   */
  let renderedRowCount = $state(0)
  $effect(() => {
    // Touch every list so this re-runs when the rendering could have changed.
    void sidebarTab
    void filteredRegularTables.length
    void filteredViews.length
    void filteredMatViews.length
    void filteredRecent.length
    void filteredPinnedTables.length
    void filteredDbEntries.length
    // …and on the key too: the stop moves with focus, not just with the data.
    void rovingRowKey
    void activeTable
    const rows = listRowButtons()
    renderedRowCount = rows.length
    const stop = rovingRow(rows)
    for (const row of rows) {
      // Compared before writing: this runs on every filter keystroke, and a
      // schema can hold thousands of rows.
      const want = row === stop ? 0 : -1
      if (row.tabIndex !== want) row.tabIndex = want
    }
  })

  /**
   * One activation button per row the open list is actually drawing, in order.
   *
   * Counted off the DOM rather than off `soleResult`, because the model and the
   * rendering can disagree and only one of them is what the user is looking at:
   * a tab that draws two sections, an empty-state row, a list still holding the
   * previous tab's rows. If there is exactly one row on screen, Enter opens that
   * row - and it opens it by clicking the row's own button, so Enter and a click
   * cannot drift apart no matter what a row grows into later.
   *
   * Rows are marked with `data-sidebar-row` rather than being found as "the
   * first button in the `li`". That older rule read the wrong element on the
   * Recent tab, where the row is a `role="button"` div and the first real
   * `<button>` inside it is the remove-from-recent control - so a filter that
   * left one recent row and an Enter in the box deleted it instead of opening
   * it. The attribute also spans every section in the open tab, which is what
   * both the row count and the arrow-key walk below need.
   * @returns {HTMLElement[]}
   */
  function listRowButtons() {
    const root = scrollContainerEl
    if (!root) return []
    return /** @type {HTMLElement[]} */ ([...root.querySelectorAll('[data-sidebar-row]')])
      .filter((el) => !(el instanceof HTMLButtonElement && el.disabled))
  }

  // ── One tab stop for the list (ARIA APG roving tabindex) ──────────────────
  // Every row was its own tab stop, so crossing the sidebar with Tab took one
  // press per table - 23 of them on the schema in the screenshot - and the focus
  // ring crawled the list a row at a time instead of moving between the controls
  // around it. A list is one widget: Tab reaches it once, arrows move inside it,
  // Tab leaves it. Every row renders `tabindex="-1"` and exactly one is promoted
  // to 0 below.
  /** The row holding the tab stop, as its `data-sidebar-row` key. */
  let rovingRowKey = $state(/** @type {string | null} */ (null))

  /**
   * The row that should hold the stop: the one last focused, else the row for
   * the open table, else the first. The fallbacks are what make Tab land
   * somewhere useful after the list is replaced - a schema switch leaves
   * `rovingRowKey` pointing at a row that no longer exists.
   * @param {HTMLElement[]} [rows]
   */
  function rovingRow(rows = listRowButtons()) {
    return (
      rows.find((r) => r.dataset.sidebarRow === rovingRowKey) ??
      rows.find((r) => r.dataset.sidebarCurrent !== undefined) ??
      rows[0] ??
      null
    )
  }

  /**
   * True while the keyboard is the thing moving focus around the sidebar.
   *
   * Every move this component makes is a scripted `.focus()` - the arrow walk,
   * the handoff out of the filter, the landing after a schema switch - and
   * WebKit, the engine this ships on, does not promise `:focus-visible` for
   * scripted focus. The rows would take focus with nothing drawn on them, which
   * is exactly what "I don't see the focus" looks like. The flag puts a plain
   * `:focus` ring on roving rows (see `[data-kbd-nav]` in app.css) and a
   * pointerdown takes it away again, so a click still leaves no ring.
   */
  let kbdNav = $state(false)

  /** Focusable things, in the order the browser would tab through them. */
  const TABBABLE =
    'a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])'

  /**
   * Move focus out of the sidebar and into the content region.
   *
   * Tab off a row landed on the panel splitter, which sits between the two in
   * the DOM: a 1px line most people do not know is a control, and the only sign
   * anything had happened was that line changing colour. The grid is what comes
   * next in reading order, so Tab goes there. The splitter keeps its keyboard
   * resize and is still reachable with Shift+Tab from the content.
   * @returns {boolean} whether focus actually moved
   */
  function focusMainRegion() {
    const main = document.querySelector('[data-studio-region="main"]')
    if (!main) return false
    // The grid is `tabindex="-1"` - it is focused, never tabbed to - so it has
    // to be named rather than found among the tabbables.
    const grid = main.querySelector('[data-canvas-table]')
    const target =
      grid instanceof HTMLElement
        ? grid
        : /** @type {HTMLElement | undefined} */ (
            [...main.querySelectorAll(TABBABLE)].find(
              (el) => el instanceof HTMLElement && el.offsetParent !== null,
            )
          )
    if (!target) return false
    target.focus()
    return true
  }

  /**
   * Schema + database the list was showing when a switch was asked for, or null
   * when none is pending. Picking a schema or another database leaves focus on
   * the control that was picked from, which is the one thing on screen that has
   * nothing left to say - what the user wants next is the new schema's tables.
   *
   * Armed rather than acted on immediately, for two reasons: the outgoing
   * schema's rows are still mounted for a frame or two after the click, and a
   * switch that opens a confirm dialog must not move focus behind it. It fires
   * on the first render that actually shows a different schema or database, so a
   * cancelled switch never fires at all.
   */
  let listFocusFrom = /** @type {string | null} */ (null)
  const listIdentity = $derived(`${activeSchema}\u0000${activeDbKey}`)
  function focusListAfterSwitch() { listFocusFrom = listIdentity }
  $effect(() => {
    if (listFocusFrom === null || loadingTables || listIdentity === listFocusFrom) return
    listFocusFrom = null
    // Only while focus is still where the switch left it. The load takes a
    // moment, and someone who clicked into the editor meanwhile keeps it.
    const from = document.activeElement
    const sidebar = scrollContainerEl?.closest('[data-studio-region="sidebar"]')
    if (from && from !== document.body && !sidebar?.contains(from)) return
    // The filter box is the fallback: a schema with no tables has no row to
    // land on, and leaving focus on the schema button would strand it there.
    kbdNav = true
    ;(rovingRow() ?? filterEl)?.focus()
  })

  /** Commit a pending debounce now, so Enter acts on what is actually typed. */
  function flushFilter() {
    if (!filterDebounce) return
    clearTimeout(filterDebounce)
    filterDebounce = null
    debouncedFilter = localFilter
    ontablefilter(localFilter)
  }

  function resetFilters() {
    localFilter = '';
    debouncedFilter = '';
    if (filterDebounce) { clearTimeout(filterDebounce); filterDebounce = null; }
    ontablefilter('');
    hideEmpty = false;
    hideSystem = false;
    sortBy = 'name';
    sortDir = 'asc';
  }
  // ── Lists (tables, views, materialized views, databases) ─────────────────
  /** @type {HTMLElement | null} */
  let scrollContainerEl = $state(null)

  // No windowing, and no `content-visibility` either. The sidebar renders every
  // row, plainly.
  //
  // It used to virtualize all four lists, and the window maths was the source of
  // a run of scroll bugs - a blank list behind a full-height spacer, and jitter
  // from a fractional row stride that `offsetTop` could only report as an integer.
  // `content-visibility: auto` on each row replaced it, and traded those bugs for
  // a worse one: WebKit - which is the engine this app actually ships on, via
  // WKWebView - renders skipped subtrees lazily enough that a fast scroll outruns
  // it, and rows arrive as blank dark gaps that fill in a frame or two later. A
  // list that disappears while you scroll it is worse than a list that costs more
  // to build.
  //
  // What makes rendering every row affordable is that a row is now just a <button>:
  // the per-row ContextMenu.Root + Trigger (two component instances each) were
  // hoisted to one shared menu per list. See the tables list markup below.
  const dbEntriesToRender = $derived(filteredDbEntries)

  // ── Objects tab ─────────────────────────────────────────────────────────
  /** True while the Objects tab loads, for the bar's refresh spinner. */
  let objectsLoading = $state(false)
  /** @type {SidebarObjects | null} */
  let objectsPanel = $state(null)
  /** Rows in the Objects tree before and after the filter, for the tab strip. */
  let objectsTotal = $state(0)
  let objectsShown = $state(0)

  /**
   * Table and view comments for the schema on screen, read once per schema
   * when the setting is on and again after anything bumps the objects.
   * @type {Map<string, string>}
   */
  let comments = $state(new Map())
  let commentsFor = ''
  $effect(() => {
    const key = $appSidebarComments && connectionName ? `${connection?.id ?? connectionName}\u0000${activeSchema}\u0000${$objectsVersion}` : ''
    untrack(() => void loadComments(key))
  })
  /** @param {string} key */
  async function loadComments(key) {
    if (key === commentsFor) return
    commentsFor = key
    if (!key) { comments = new Map(); return }
    try {
      const list = await listObjectComments(activeSchema)
      if (key === commentsFor) comments = new Map(list.map((c) => [c.name, c.comment]))
    } catch { /* comments are extra: a failure leaves the rows as they were */ }
  }

  /** Shared field chrome for schema select + table filter (aligned in sidebar grid) */
  const sidebarFieldClass =
"field-surface h-7 w-full min-w-0 bg-background/40 text-ui-sm text-foreground shadow-none transition-colors hover:bg-background/55";
</script>

<svelte:window onkeydown={(e) => {
  // Cheap key checks first: reading offsetParent can force layout, and this
  // handler runs on every keystroke app-wide (Monaco, cell editors included).
  const isFilterKey = (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key === 'f'
  const isEscClear = e.key === 'Escape' && selectedItems.size > 0
  if (!isFilterKey && !isEscClear) return
  // A code editor handles its own Mod-F (find and replace) and marks the key
  // handled; stealing it here sent focus to the sidebar filter instead.
  if (e.defaultPrevented || (e.target instanceof Element && e.target.closest('.cm-editor'))) return
  // Guard: filterEl.offsetParent is null when sidebar is hidden via display:none
  if (!filterEl || !filterEl.offsetParent) return
  if (isFilterKey) {
    e.preventDefault(); filterEl.focus(); filterEl.select()
  }
  if (isEscClear) clearSelection()
}} />

<!-- The sidebar's one loading state. The tables list had the dots; every other
     list wrote its own bare "Loading…" line, so the Databases tab looked like it
     had failed and printed a label rather than like it was working. -->
{#snippet listLoading(/** @type {string} */ label)}
  <div class="flex items-center justify-center py-6" role="status" aria-label={label}>
    <span class="inline-flex gap-1.5" aria-hidden="true">
      <span class="size-1.5 animate-bounce rounded-full bg-muted-foreground/50" style="animation-delay: 0ms"></span>
      <span class="size-1.5 animate-bounce rounded-full bg-muted-foreground/50" style="animation-delay: 150ms"></span>
      <span class="size-1.5 animate-bounce rounded-full bg-muted-foreground/50" style="animation-delay: 300ms"></span>
    </span>
  </div>
{/snippet}

<!-- Section count badge: shows "visible/total" when filters hide rows, else just the total. -->
<!-- `tight` keeps the count beside the section name instead of pushing it to
     the far edge. A section with actions in its header needs that edge for
     them, and a count floating between the two reads as part of the buttons. -->
{#snippet countBadge(visible, total, tight = false)}
  {@const cls = cn(tight ? "" : "ml-auto", "shrink-0 font-mono text-ui-2xs text-muted-foreground")}
  {#if visible !== total}
    <span class={cls} title="{visible} shown · {total - visible} hidden of {total}"
      >{visible}<span class="text-muted-foreground">/{total}</span></span>
  {:else}
    <span class={cls}>{total}</span>
  {/if}
{/snippet}

<div
  class={cn("flex h-full shrink-0", side === "right" && "flex-row-reverse")}
  style:width="calc({width}px * var(--app-scale, 1))"
  data-studio-region="sidebar"
>
  <ContextMenu.Root>
  <ContextMenu.Trigger class="flex h-full min-w-0 flex-1">
  <!-- Deliberately NOT a size container (`@container/sb`). Declaring one made the
       whole sidebar subtree re-resolve its container queries on every pixel of a
       resize drag, which measured 31ms of layout per frame against 5.8ms without
       it in the same A/B - the drag ran at ~20fps. The one query it fed is gone:
       the section headers below name their own list and carry their own count,
       which needs no query at all. -->
  <aside
    class="studio-chrome flex h-full min-w-0 flex-1 flex-col bg-sidebar text-sidebar-foreground"
    data-studio-chrome
    data-kbd-nav={kbdNav ? "" : undefined}
    onpointerdown={() => (kbdNav = false)}
  >
    {#if navSidebarPanel === "tables"}
    <div class="flex min-h-0 flex-1 flex-col">

      <div class="flex shrink-0 flex-col">
        <!-- Top row. One list at a time, chosen from the strip: the old sidebar
             stacked six independently-collapsible sections in one scroller, so
             opening any of them moved everything below it and the list you
             wanted was usually past the fold.

             Top row: the tabs on the left, the list actions pushed to the right.
             They act on whichever list the tab picked, so they belong on the line
             with the tabs rather than wedged against a filter they have nothing to
             do with. `ml-auto` is the gap - a fixed one would drift as the sidebar
             is dragged wider. The tablist stays its own element: a tablist holding
             three non-tab buttons is a lie to every screen reader. -->
        <div class="flex h-9 shrink-0 items-center gap-1 border-b border-sidebar-border px-2">
          <div
            role="tablist"
            aria-label="Sidebar sections"
            class="app-scroll-x flex min-w-0 flex-1 items-center gap-1 overflow-x-auto"
          >
            {#each SIDEBAR_TABS as tab (tab.id)}
              {@const active = sidebarTab === tab.id}
              {@const count = tabCounts[tab.id]}
              <button
                type="button"
                role="tab"
                data-roving
                aria-selected={active}
                aria-label={count > 0 ? `${tab.label}, ${count}` : tab.label}
                title={`${tab.label} · ${comboTitle(`Mod+Shift+${SIDEBAR_TABS.indexOf(tab) + 1}`)}`}
                tabindex={active ? 0 : -1}
                disabled={!connectionName}
                class={cn(
                  // Full row height, 32px wide: the whole strip is the target, which
                  // clears 24x24 with room over and lets the accent rail sit on the
                  // row's own bottom edge. It was a 28px pill with the rail pushed
                  // 7px below it, so the rail floated in the gap between the pill and
                  // the border, attached to neither.
                  "group/tab relative inline-flex h-9 w-8 shrink-0 items-center justify-center transition-colors disabled:pointer-events-none disabled:opacity-40",
                  // Two cues, not one. Colour alone does not separate four line icons
                  // at this size, so the selected tab also carries a filled surface and
                  // an accent rail - the same "which panel am I in" signal the VS Code
                  // activity bar uses.
                  active ? "text-foreground" : "text-muted-foreground hover:text-foreground",
                )}
                onclick={() => (sidebarTab = tab.id)}
                onkeydown={(e) => {
                  // Arrow keys move between tabs (ARIA APG tablist); the roving
                  // tabindex above is what keeps the strip to one tab stop.
                  if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return
                  e.preventDefault()
                  kbdNav = true
                  const i = SIDEBAR_TABS.findIndex((t) => t.id === sidebarTab)
                  const next = (i + (e.key === "ArrowRight" ? 1 : -1) + SIDEBAR_TABS.length) % SIDEBAR_TABS.length
                  sidebarTab = SIDEBAR_TABS[next].id
                  /** @type {HTMLElement | null} */ (
                    e.currentTarget.parentElement?.children[next] ?? null
                  )?.focus()
                }}
              >
                <!-- The pill is a child, not the button's own background: the
                     button spans the full row so its target is generous, while
                     the shape you see stays 28px and centred. -->
                <span
                  class={cn(
                    "pointer-events-none absolute inset-x-0.5 inset-y-1 rounded-md transition-colors",
                    active ? "bg-sidebar-accent" : "group-hover/tab:bg-sidebar-accent/50",
                  )}
                  aria-hidden="true"
                ></span>
                <Icon name={tab.icon} class="relative size-4 shrink-0" />
                {#if active}
                  <span
                    class="pointer-events-none absolute inset-x-1 bottom-0 h-0.5 rounded-t-full bg-primary"
                    aria-hidden="true"
                  ></span>
                {/if}
              </button>
            {/each}
          </div>
          <div class="ml-auto flex shrink-0 items-center gap-0.5">
          {#if sidebarTab === 'objects'}
            <!-- On Objects the bar's refresh and + act on objects, so the list
                 needs no header row of its own. -->
            <button
              type="button"
              class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-40"
              title="Refresh objects"
              aria-label="Refresh objects"
              disabled={!connectionName}
              onclick={() => objectsPanel?.refreshAll()}
            >
              <Icon name="refresh-cw" class={cn("size-3.5", objectsLoading && "animate-spin")} />
            </button>
            <DropdownMenu.Root>
              <DropdownMenu.Trigger
                class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground data-[state=open]:bg-accent data-[state=open]:text-foreground disabled:pointer-events-none disabled:opacity-40"
                title={$readOnlyMode ? READ_ONLY_HINT : 'New view, function, trigger…'}
                aria-label="New object"
                disabled={!connectionName || $readOnlyMode}
              >
                <Icon name="plus" class="size-3.5" />
              </DropdownMenu.Trigger>
              <DropdownMenu.Content align="end" class="min-w-44">
                {#each objectsPanel?.newKinds() ?? [] as k (k.kind)}
                  <DropdownMenu.Item onSelect={() => objectsPanel?.createKind(k.kind)}>
                    <Icon name={k.icon} class="size-3.5 shrink-0 text-muted-foreground" />
                    {k.label}
                  </DropdownMenu.Item>
                {/each}
              </DropdownMenu.Content>
            </DropdownMenu.Root>
          {:else}
          <DropdownMenu.Root>
            <DropdownMenu.Trigger
              class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground data-[state=open]:bg-accent data-[state=open]:text-foreground disabled:pointer-events-none disabled:opacity-40"
              title="Display options"
              disabled={!connectionName}
            >
              <Icon name="list-filter" class="size-3.5" />
            </DropdownMenu.Trigger>
            <DropdownMenu.Content align="start" class="min-w-52">
              <div class="px-2 pt-1 pb-1.5 text-ui-2xs text-muted-foreground leading-relaxed">
                <span class="font-mono text-foreground/80">{regularTables.length}</span> tables{#if views.length} · <span class="font-mono text-foreground/80">{views.length}</span> views{/if}{#if matViews.length} · <span class="font-mono text-foreground/80">{matViews.length}</span> mat.{/if}
                {#if hiddenCount > 0}<br /><span class="text-warning">{hiddenCount} hidden by filters</span>{/if}
              </div>
              <DropdownMenu.Separator />
              <DropdownMenu.CheckboxItem
                checked={showRowCount}
                onCheckedChange={(v) => (showRowCount = v)}
              ><Icon name="hash" class="text-muted-foreground" />Row counts</DropdownMenu.CheckboxItem>
              <DropdownMenu.CheckboxItem
                checked={hideEmpty}
                onCheckedChange={(v) => (hideEmpty = v)}
              ><Icon name="circle-slash" class="text-muted-foreground" />Hide empty tables</DropdownMenu.CheckboxItem>
              <DropdownMenu.CheckboxItem
                checked={hideSystem}
                onCheckedChange={(v) => (hideSystem = v)}
              ><Icon name="cog" class="text-muted-foreground" />Hide system tables</DropdownMenu.CheckboxItem>
              <DropdownMenu.Separator />
              <DropdownMenu.Label class="px-2 py-0.5 text-ui-2xs font-medium uppercase tracking-wide text-muted-foreground">Sort by</DropdownMenu.Label>
              <!-- Field + direction merged: pick a field, click it again to flip. -->
              <DropdownMenu.Item
                closeOnSelect={false}
                title="Click to flip direction"
                onSelect={() => { if (sortBy === 'name') sortDir = sortDir === 'asc' ? 'desc' : 'asc'; else sortBy = 'name' }}
              >
                <Icon name={sortBy === 'name' && sortDir === 'desc' ? 'arrow-up-a-z' : 'arrow-down-a-z'} class="text-muted-foreground" />
                Name
                {#if sortBy === 'name'}
                  <span class="ml-auto font-mono text-ui-2xs text-muted-foreground">{sortDir === 'asc' ? 'A→Z' : 'Z→A'}</span>
                {/if}
              </DropdownMenu.Item>
              <DropdownMenu.Item
                closeOnSelect={false}
                title="Click to flip direction"
                onSelect={() => { if (sortBy === 'rowCount') sortDir = sortDir === 'asc' ? 'desc' : 'asc'; else sortBy = 'rowCount' }}
              >
                <Icon name={sortBy === 'rowCount' && sortDir === 'asc' ? 'arrow-up-0-1' : 'arrow-down-0-1'} class="text-muted-foreground" />
                Row count
                {#if sortBy === 'rowCount'}
                  <span class="ml-auto font-mono text-ui-2xs text-muted-foreground">{sortDir === 'desc' ? '9→0' : '0→9'}</span>
                {/if}
              </DropdownMenu.Item>
              <!-- Newest first on the first pick: "what did I just make" is the
                   usual question. Engines that keep no creation time (Postgres,
                   SQLite, DuckDB) sort by creation order instead. -->
              <DropdownMenu.Item
                closeOnSelect={false}
                title="Click to flip direction. MySQL and SQL Server sort by creation time; other engines by the order tables were created in."
                onSelect={() => { if (sortBy === 'created') sortDir = sortDir === 'asc' ? 'desc' : 'asc'; else { sortBy = 'created'; sortDir = 'desc' } }}
              >
                <Icon name="clock" class="text-muted-foreground" />
                Created
                {#if sortBy === 'created'}
                  <span class="ml-auto font-mono text-ui-2xs text-muted-foreground">{sortDir === 'desc' ? 'New→Old' : 'Old→New'}</span>
                {/if}
              </DropdownMenu.Item>
              <DropdownMenu.Separator />
              <DropdownMenu.Item onSelect={resetFilters} disabled={!filtersActive}>
                <Icon name="rotate-ccw" class="text-muted-foreground" />
                Reset filters &amp; sort
              </DropdownMenu.Item>
            </DropdownMenu.Content>
          </DropdownMenu.Root>
          <button
            type="button"
            class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-40"
            title={`Refresh tables (${comboTitle('Mod+R')})`}
            disabled={loadingTables || !connectionName}
            onclick={onrefresh}
          >
            <Icon name="refresh-cw"
              class={cn("size-3.5", loadingTables && "animate-spin")}
            />
          </button>
          {#if supportsSchemas}
            <DropdownMenu.Root>
              <DropdownMenu.Trigger
                class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground data-[state=open]:bg-accent data-[state=open]:text-foreground disabled:pointer-events-none disabled:opacity-40"
                title="Create new…"
                disabled={!connectionName}
              >
                <Icon name="plus" class="size-3.5" />
              </DropdownMenu.Trigger>
              <DropdownMenu.Content align="end" class="w-44 p-1 text-ui-sm">
                <DropdownMenu.Item
                  onSelect={onnewtable}
                  disabled={$readOnlyMode}
                  title={$readOnlyMode ? READ_ONLY_HINT : undefined}
                  class="gap-2"
                >
                  <Icon name="table-2" class="size-3.5 shrink-0 text-muted-foreground" />
                  {$t('sidebar.newTable')}
                </DropdownMenu.Item>
                <DropdownMenu.Item
                  onSelect={onnewschema}
                  disabled={$readOnlyMode}
                  title={$readOnlyMode ? READ_ONLY_HINT : undefined}
                  class="gap-2"
                >
                  <Icon name="box" class="size-3.5 shrink-0 text-muted-foreground" />
                  {$t('sidebar.newSchema')}
                </DropdownMenu.Item>
              </DropdownMenu.Content>
            </DropdownMenu.Root>
          {:else}
            <button
              type="button"
              class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-40"
              title={$readOnlyMode ? READ_ONLY_HINT : 'New table'}
              disabled={!connectionName || $readOnlyMode}
              onclick={onnewtable}
            >
              <Icon name="plus" class="size-3.5" />
            </button>
          {/if}
          {/if}
          </div>
        </div>

        <!-- The filter row belongs to the lists. Find & replace and Queries
             bring their own fields, so leaving this here would be a second
             search box with nothing to search. -->
        {#if sidebarTab !== 'search' && sidebarTab !== 'queries'}
        <!-- Filter row: the schema the list belongs to, and the filter itself. -->
        <div class="flex h-9 shrink-0 items-center gap-1.5 border-b border-sidebar-border px-2">
          <!-- Shown when the engine actually has schemas to pick between, which
               is not the same question as `supportsSchemas` - that flag is
               postgres-only because Postgres is the only driver with a CREATE
               SCHEMA namespace, and it gates the "New schema" action below.
               MySQL and SQL Server both LIST schemas without supporting that,
               so gating the picker on it hid theirs. SQLite, D1 and Redis have
               no schemas at all and correctly show nothing. -->
          {#if schemas.length > 0}
            <div class="max-w-[8rem] shrink-0">
                    <SearchableMenu
                      contentClass="min-w-60"
                      placeholder="Search schemas…"
                      empty="No schema"
                      items={schemaMenuItems}
                      onselect={(it) => {
                        if (!it.value) return
                        // The last entry is the toggle, not a schema.
                        if (it.value === SYSTEM_TOGGLE) { showSystemSchemas = !showSystemSchemas; return }
                        // Armed BEFORE the switch: `activeSchema` is bound, so the
                        // shell writes the new one back synchronously and arming
                        // afterwards would record the schema we are moving TO.
                        focusListAfterSwitch()
                        onschemachange(it.value)
                      }}
                    >
                      {#snippet trigger(props)}
                        <button
                          {...props}
                          id="sidebar-schema"
                          type="button"
                          class={cn(sidebarFieldClass, "flex h-7 w-full items-center gap-1 px-2 font-normal")}
                        >
                          <Icon name="box" class="size-3.5 shrink-0 text-muted-foreground" />
                          <span class="min-w-0 truncate">{activeSchema}</span>
                          <Icon name="chevron-down" class="size-3 shrink-0 text-muted-foreground" />
                        </button>
                      {/snippet}
                      {#snippet item(it)}
                        {#if it.value === SYSTEM_TOGGLE}
                          <Icon name={showSystemSchemas ? 'eye-off' : 'eye'} class="size-3.5 shrink-0 text-muted-foreground" />
                          <span class="min-w-0 flex-1 truncate text-muted-foreground">{it.label}</span>
                        {:else}
                          <Icon name="box" class={cn('size-3.5 shrink-0', isSystemSchema(it.value) ? 'text-muted-foreground/60' : 'text-muted-foreground')} />
                          <span class="min-w-0 flex-1 truncate">{it.label}</span>
                          {#if isSystemSchema(it.value)}
                            <!-- A padlock, not the word "system". The word was as
                                 long as the names it sat beside, so it pushed
                                 `information_schema` into an ellipsis to label the
                                 thing it had just made unreadable. -->
                            <span class="flex shrink-0 items-center" title="System schema" aria-label="System schema">
                              <Icon name="lock" class="size-3 text-muted-foreground/70" />
                            </span>
                          {/if}
                          {#if it.value === activeSchema}<Icon name="check" class="size-3.5 shrink-0 text-primary" />{/if}
                        {/if}
                      {/snippet}
                    </SearchableMenu>
            </div>
          {/if}
          <div class="relative min-w-0 flex-1">
          <Icon name="search"
            class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
          />
          <input
            type="search"
            bind:this={filterEl}
            placeholder={connectionName ? "Filter…" : "Not connected"}
            value={localFilter}
            disabled={!connectionName}
            oninput={(e) => handleFilterInput(e.currentTarget.value)}
            onkeydown={(e) => {
              // Enter opens the match when the filter has left exactly one. The
              // debounce is committed first, or a fast typist who narrows to one
              // row and hits Enter within 200ms is judged against the previous
              // term. Inert with nothing or several to open: a key that guesses
              // which of six rows was meant is worse than a key that does nothing.
              if (e.key === 'Enter' && sidebarTab === 'objects') {
                flushFilter()
                flushSync()
                if (objectsPanel?.openSole()) e.preventDefault()
                return
              }
              if (e.key === 'Enter') {
                flushFilter()
                // Commit the render too: `flushFilter` only sets state, and the
                // rows are counted off the DOM, which is a frame behind until
                // this runs.
                flushSync()
                const rows = listRowButtons()
                if (rows.length === 1) {
                  e.preventDefault()
                  rows[0].click()
                  return
                }
                // Nothing on screen to act on, or several. Fall back to the model
                // only where the list is not rendered at all (a collapsed section).
                const sole = rows.length === 0 ? soleResult : null
                if (!sole) return
                e.preventDefault()
                sole.open()
                return
              }
              // Tab / ArrowDown from the filter → jump focus into the result list
              // so the user can keyboard-navigate the matched tables directly.
              if (((e.key === 'Tab' && !e.shiftKey) || e.key === 'ArrowDown') && sidebarTab === 'objects') {
                if (objectsPanel?.focusTree()) e.preventDefault()
                return
              }
              if ((e.key === 'Tab' && !e.shiftKey) || e.key === 'ArrowDown') {
                // The FIRST result, not the row holding the tab stop: this is a
                // search box, and the answer to what was typed starts at the top
                // of the list. (It was also not the first `<button>` under the
                // scroller - that is whichever section-header action comes first,
                // so Tab out of the filter landed on an icon in a heading.)
                const row = listRowButtons()[0]
                if (row) { e.preventDefault(); kbdNav = true; row.focus() }
              }
            }}
            class={cn(sidebarFieldClass, "w-full pl-8 pr-2.5 outline-none disabled:opacity-40 disabled:cursor-not-allowed")}
            aria-label="Filter {activeTabLabel.toLowerCase()}"
            aria-describedby="sidebar-filter-hint"
            data-sidebar-filter
          />
          </div>
          <!-- Only while a search has narrowed the list to one row. A schema
               that simply has one table showed it with an empty box, where it
               read as a stray key rather than "open your match". -->
          {#if renderedRowCount === 1 && debouncedFilter.trim()}
            <!-- A real target, not a legend. When the filter has left one row,
                 the fastest thing to do with it is open it, and a hint that only
                 tells you which key to press makes the pointer take the long way
                 round to a row it can already see.
                 `tabindex="-1"` keeps it out of the tab order on purpose: Tab
                 from the filter goes to the list, which with one match is this
                 same row, so a stop here would be a second stop on one thing.
                 The keyboard path is Enter in the field, which the field's own
                 description offers. -->
            <button
              type="button"
              tabindex="-1"
              class="hit-area shrink-0 rounded border border-border/60 bg-muted/40 px-1 py-px font-mono text-ui-3xs text-muted-foreground transition-colors hover:border-primary/60 hover:bg-accent hover:text-foreground"
              title={soleResult ? `Open ${soleResult.label} (Enter)` : 'Open the only match (Enter)'}
              aria-label={soleResult ? `Open ${soleResult.label}` : 'Open the only match'}
              onclick={() => { const rows = listRowButtons(); if (rows.length === 1) rows[0].click(); else soleResult?.open() }}
            >↵</button>
          {/if}
          <!-- Held apart: the description is static and read on focus, the status
               is rewritten as the list narrows. Merging them would re-announce
               the instruction on every keystroke. Both are rendered whether or
               not they have anything to say, because a polite region inserted at
               the moment its text appears is announced unreliably. -->
          <span id="sidebar-filter-hint" class="sr-only"
            >Filters the {activeTabLabel.toLowerCase()} list. Tab or press the down arrow to move into the results, then Tab or the arrow keys to move through them. Press Enter to open a row, or Shift and Enter to open it and move into the data grid.</span>
          <span class="sr-only" role="status" aria-live="polite" aria-atomic="true">{filterStatus}</span>
        </div>
        {/if}
      </div>

      {#if sidebarTab === 'queries'}
        {@render queriesPanel?.()}
      {:else if sidebarTab === 'objects'}
        {#if connectionName}
          <SidebarObjects
            bind:this={objectsPanel}
            bind:busyLoading={objectsLoading}
            {connection}
            schema={activeSchema}
            {views}
            {matViews}
            {activeTable}
            filter={debouncedFilter}
            {comments}
            showComments={$appSidebarComments}
            onopenview={(name) => ontableselect(name)}
            onrefreshtables={onrefresh}
            ondropview={(name, cascade, kind) => ondroptable(name, cascade, kind)}
            onopensql={onopenobjectsql}
            onexittop={() => { filterEl?.focus(); filterEl?.select() }}
            bind:total={objectsTotal}
            bind:shown={objectsShown}
          />
        {/if}
      {:else if sidebarTab === 'search'}
        <FindReplacePanel
          bind:focusFind={focusFindField}
          columns={frColumns}
          rows={frRows}
          primaryKey={frPrimaryKey}
          foreignKeys={frForeignKeys}
          tableName={frEnabled ? frTableName : null}
          onapply={onfindreplaceapply}
          onreveal={onrevealcell}
        />
      {:else}
      <div class="flex min-h-0 flex-1 flex-col">
        <div
          bind:this={scrollContainerEl}
          class="app-scroll min-h-0 w-full flex-1 overflow-y-auto overscroll-y-contain [will-change:scroll-position]"
          role="none"
          use:smoothScroll={{ enabled: !$appNativeScroll }}
          onclick={(e) => {
            // Clicking the empty space around the rows drops the selection.
            // A button is not empty space: the section headers carry actions
            // that operate ON the selection, and this handler was undoing them
            // on the same click that made them.
            const el = /** @type {Element} */ (e.target)
            if (selectedItems.size > 0 && !el.closest?.('li') && !el.closest?.('button')) {
              clearSelection()
            }
          }}
          onfocusin={(e) => {
            // The stop follows focus, so Tab comes back to the row it left.
            const row = e.target instanceof Element ? e.target.closest('[data-sidebar-row]') : null
            if (row instanceof HTMLElement) rovingRowKey = row.dataset.sidebarRow ?? null
          }}
          onkeydown={(e) => {
            if (e.key === 'Escape' && selectedItems.size > 0) { clearSelection(); return }
            // Shift+Enter opens the row AND hands focus to the grid, which is
            // the one thing Enter deliberately does not do: Enter keeps you in
            // the list so you can keep looking, and there was no way to say
            // "this one, and let me work in it" without reaching for the mouse.
            if (e.key === 'Enter' && e.shiftKey) {
              const row = e.target instanceof Element ? e.target.closest('[data-sidebar-row]') : null
              if (!(row instanceof HTMLElement)) return
              e.preventDefault()
              row.click()
              // After the click: opening a table re-renders the content region,
              // and the grid has to exist before it can take focus.
              tick().then(() => focusMainRegion())
              return
            }
            // Tab walks the list, one row per press, because that is what the
            // key does everywhere else in this panel and pressing it twice from
            // the filter box otherwise skipped the whole list to land in the
            // grid. Tab off the LAST row still leaves for the content region -
            // a list you cannot tab out of is a focus trap - and Shift+Tab off
            // the first returns to the filter the list was narrowed from.
            // Shift+Enter (above) is the deliberate way into the grid.
            // `e.code` as well as `e.key`: on WebKitGTK, Shift+Tab reaches the
            // page as GTK's ISO_Left_Tab keysym rather than "Tab", so a key-name
            // check skipped this branch and the browser's own Shift+Tab moved
            // focus to the previous tab stop - the filter - instead of the
            // previous row. The physical key is Tab on every engine.
            if (e.key === 'Tab' || e.code === 'Tab') {
              const onRow = e.target instanceof Element && e.target.closest('[data-sidebar-row]')
              if (!onRow) return
              const rows = listRowButtons()
              // The ROW, not document.activeElement: the row's checkbox span is
              // tabindex="-1", so a click near the icon focuses the span, which
              // is in no row list - every Shift+Tab from there fell through to
              // the filter instead of going to the previous table.
              const i = rows.indexOf(/** @type {HTMLElement} */ (onRow))
              const next = i === -1 ? null : rows[i + (e.shiftKey ? -1 : 1)]
              if (next) {
                e.preventDefault(); kbdNav = true; next.focus()
                return
              }
              if (e.shiftKey) {
                if (!filterEl) return
                e.preventDefault(); kbdNav = true; filterEl.focus(); filterEl.select()
              } else if (focusMainRegion()) {
                e.preventDefault()
              }
              return
            }
            // Arrows walk the rows of every section in the open tab, Home/End
            // jump to its ends, and ArrowUp off the top row returns to the filter
            // box - the field the list was narrowed from.
            if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp' && e.key !== 'Home' && e.key !== 'End') return
            const rows = listRowButtons()
            const focusedRow = e.target instanceof Element ? e.target.closest('[data-sidebar-row]') : null
            const i = rows.indexOf(/** @type {HTMLElement} */ (focusedRow))
            if (i === -1) return
            e.preventDefault()
            kbdNav = true
            if (e.key === 'Home') rows[0]?.focus()
            else if (e.key === 'End') rows[rows.length - 1]?.focus()
            else if (e.key === 'ArrowDown') rows[i + 1]?.focus()
            else if (i === 0) filterEl?.focus()
            else rows[i - 1]?.focus()
          }}
        >
          {#if loadingTables}
            {@render listLoading('Loading tables')}
          {:else}
            <!-- ── Databases ──────────────────────────────────────
                 Other databases on the same server. Collapsed by default and
                 only fetched once expanded - see loadDatabases(). Engines that
                 cannot switch in place (SQLite, Redis) never render it. -->
            <!-- `!tabIsEmpty`: with nothing to list, the tab-level empty state
                 below already says so in the middle of the panel. Rendering the
                 section as well put a second, quieter "No other databases" at the
                 top-left of the same empty panel - two answers to one question,
                 in two different places and two different type sizes. -->
            {#if showDatabases && canSwitchDb && connectionName && !tabIsEmpty}
              <div class="flex w-full items-center gap-1 px-2.5 pt-2 pb-1">
                <span class="text-ui-2xs font-medium tracking-wider text-muted-foreground uppercase">Databases</span>
                {#if dbEntries.length > 0}
                  {@render countBadge(filteredDbEntries.length, dbEntries.length)}
                {/if}
                <!-- `countBadge` carries the `ml-auto` that pushes this group right;
                     the buttons must not carry one too, or the free space splits
                     between them and the count drifts into the middle of the row. -->
                <div class={cn("flex shrink-0 items-center gap-1", dbEntries.length === 0 && "ml-auto")}>
                  {#if dbAdmin}
                    <button
                      type="button"
                      class="hit-area inline-flex size-4 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:text-foreground disabled:opacity-40"
                      onclick={onnewdatabase}
                      title={$readOnlyMode ? READ_ONLY_HINT : "New database"}
                      disabled={$readOnlyMode}
                    >
                      <Icon name="plus" class="size-3" />
                    </button>
                  {/if}
                  <button
                    type="button"
                    class="hit-area inline-flex size-4 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:text-foreground"
                    onclick={() => void loadDatabases()}
                    title="Refresh databases"
                    disabled={dbEntriesLoading}
                  >
                    <Icon name="refresh-cw" class={cn("size-3", dbEntriesLoading && "animate-spin")} />
                  </button>
                </div>
              </div>
              {#if databasesOpen}
                {#if dbEntriesLoading && dbEntries.length === 0}
                  {@render listLoading('Loading databases')}
                {:else if dbEntriesError && dbEntries.length === 0}
                  <p class="px-4 pb-1.5 text-ui-2xs text-destructive">{dbEntriesError}</p>
                {:else if !dbEntriesLoaded && dbEntries.length === 0}
                  <!-- Opened but the first fetch has not started or landed yet.
                       This branch used to print "Loading…" as body text, which is
                       the state the list spends longest in on a remote server.
                       Only when there is nothing to show: a refetch over an
                       existing list leaves the list up. -->
                  {@render listLoading('Loading databases')}
                {:else if filteredDbEntries.length === 0}
                  <p class="px-4 pb-1.5 text-ui-2xs text-muted-foreground">
                    {lf ? 'No matching databases' : 'No other databases'}
                  </p>
                {:else}
                  <ul class="flex w-full min-w-full flex-col px-1.5 pb-1 [&>li]:pb-0.5">
                    {#each dbEntriesToRender as db (db.key)}
                      {@const isCurrent = db.key === activeDbKey}
                      <li>
                        <ContextMenu.Root>
                          <ContextMenu.Trigger class="w-full">
                            <button
                              type="button"
                              disabled={isCurrent}
                              data-sidebar-row="db:{db.key}"
                              data-roving
                              tabindex="-1"
                              class={cn(
                                "grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-2 rounded-md px-2 py-1.5 text-left transition-colors",
                                isCurrent
                                  ? "bg-sidebar-accent text-sidebar-accent-foreground"
                                  : "text-foreground/70 hover:bg-sidebar-accent/50 hover:text-foreground",
                              )}
                              onclick={() => !isCurrent && onDbClick(db)}
                              ondblclick={() => !isCurrent && onDbDblClick(db)}
                              title={isCurrent ? `${db.label} (current)` : `Switch to ${db.label} · double-click to switch without confirming`}
                              aria-label={isCurrent ? `${db.label}, current database` : `Switch to ${db.label}`}
                            >
                              <Icon name="database" class="size-3 shrink-0 opacity-50" />
                              <span class="min-w-0 truncate font-mono text-ui-sm leading-4">{db.label}</span>
                              {#if isCurrent}
                                <Icon name="check" class="size-3 shrink-0 text-success" />
                              {/if}
                            </button>
                          </ContextMenu.Trigger>
                          <ContextMenu.Content class="min-w-52">
                            {#if !isCurrent}
                              <ContextMenu.Item onSelect={() => { focusListAfterSwitch(); onswitchdatabase(db) }}>
                                <Icon name="arrow-right" />
                                Switch to this database
                              </ContextMenu.Item>
                            {/if}
                            <ContextMenu.Item onSelect={() => { navigator.clipboard.writeText(db.label) }}>
                              <Icon name="copy" />
                              Copy name
                            </ContextMenu.Item>
                            {#if dbAdmin}
                              <ContextMenu.Item onSelect={() => ondatabaseinfo({ name: db.label })}>
                                <Icon name="info" />
                                Database info
                              </ContextMenu.Item>
                              <ContextMenu.Separator />
                              {@const ren = dbItem('rename', isCurrent)}
                              <ContextMenu.Item disabled={ren.disabled} title={ren.title} onSelect={() => onrenamedatabase({ name: db.label, existing: dbNames })}>
                                <Icon name="pencil" />
                                Rename…
                              </ContextMenu.Item>
                              {@const dup = dbItem('duplicate', isCurrent)}
                              <ContextMenu.Item disabled={dup.disabled} title={dup.title} onSelect={() => onduplicatedatabase({ name: db.label, existing: dbNames })}>
                                <Icon name="copy-plus" />
                                Duplicate…
                              </ContextMenu.Item>
                              {@const term = dbItem('terminate', isCurrent)}
                              {#if !term.disabled}
                                <ContextMenu.Item onSelect={() => onterminatedbsessions({ name: db.label })}>
                                  <Icon name="unplug" />
                                  Close other sessions
                                </ContextMenu.Item>
                              {/if}
                              <ContextMenu.Separator />
                              {@const drp = dbItem('drop', isCurrent)}
                              <ContextMenu.Item variant="destructive" disabled={drp.disabled} title={drp.title} onSelect={() => ondropdatabase({ name: db.label })}>
                                <Icon name="trash-2" />
                                Drop database…
                              </ContextMenu.Item>
                            {/if}
                          </ContextMenu.Content>
                        </ContextMenu.Root>
                      </li>
                    {/each}
                  </ul>
                {/if}
              {/if}
            {/if}

            <!-- ── Recent ─────────────────────────────────────────── -->
            {#if showRecent && filteredRecent.length > 0 && connectionName}
              <div class="flex w-full items-center gap-1 px-2.5 pt-2 pb-1">
                <span class="text-ui-2xs font-medium tracking-wider text-muted-foreground uppercase">Recent</span>
                {@render countBadge(Math.min(filteredRecent.length, 5), Math.min(recentTabs.length, 5))}
                <button
                  type="button"
                  class="shrink-0 font-mono text-ui-2xs text-muted-foreground transition-colors hover:text-destructive"
                  onclick={onrecentclear}
                  title="Clear recent"
                >Clear</button>
              </div>
              {#if recentOpen}
                <ul class="flex w-full min-w-full flex-col px-1.5 pb-1 [&>li]:pb-0.5">
                  {#each filteredRecent.slice(0, 5) as item (item.schema + '.' + item.table)}
                    <li class="group/recent">
                      <div
                        class={cn(
                          "grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-2 rounded-md px-2 py-1.5 transition-colors cursor-pointer",
                          activeTable === item.table
                            ? "bg-sidebar-accent text-sidebar-accent-foreground"
                            : "text-foreground/70 hover:bg-sidebar-accent/50 hover:text-foreground",
                        )}
                        role="button"
                        tabindex="-1"
                        data-sidebar-row="recent:{item.schema}.{item.table}"
                        data-roving
                        data-sidebar-current={activeTable === item.table ? '' : undefined}
                        onclick={() => onrecentselect(item.schema, item.table)}
                        onkeydown={(e) => {
                          // role="button" has to answer Space as well as Enter (ARIA APG).
                          // Shift+Enter belongs to the list's own handler, which
                          // opens the row AND moves focus into the grid - taking
                          // it here too would open it twice.
                          if ((e.key === 'Enter' && !e.shiftKey) || e.key === ' ') {
                            e.preventDefault()
                            onrecentselect(item.schema, item.table)
                            return
                          }
                          // The remove control is hover-revealed and out of the
                          // tab order, so the keyboard reaches it here instead.
                          if (e.key === 'Delete' || e.key === 'Backspace') {
                            e.preventDefault()
                            // The row being removed is the one holding focus, so
                            // the next one has to take it or focus falls to the
                            // body and the list has to be tabbed into again.
                            const rows = listRowButtons()
                            const i = rows.indexOf(/** @type {HTMLElement} */ (e.currentTarget))
                            const next = rows[i + 1] ?? rows[i - 1] ?? null
                            onrecentremove(item.schema, item.table)
                            kbdNav = true
                            void tick().then(() => (next ?? filterEl)?.focus())
                          }
                        }}
                      >
                        {#if item.tableKind === 'view'}
                          <Icon name="table-view" class="size-3 shrink-0 opacity-50" />
                        {:else if item.tableKind === 'materialized_view'}
                          <Icon name="layers" class="size-3 shrink-0 opacity-50" />
                        {:else}
                          <Icon name="table-2" class="size-3 shrink-0 opacity-50" />
                        {/if}
                        <span class="min-w-0 truncate font-mono text-ui-sm leading-4">{item.table}</span>
                        <button
                          type="button"
                          tabindex="-1"
                          aria-label="Remove {item.table} from recent (Delete)"
                          class="hit-area inline-flex size-4 shrink-0 items-center justify-center rounded text-muted-foreground opacity-0 transition-opacity duration-150 group-hover/recent:opacity-100 hover:text-foreground focus-visible:opacity-100"
                          onclick={(e) => { e.stopPropagation(); onrecentremove(item.schema, item.table) }}
                        >
                          <Icon name="x" class="size-3" />
                        </button>
                      </div>
                    </li>
                  {/each}
                </ul>
              {/if}
            {/if}

            <!-- ── Pinned ─────────────────────────────────────────── -->
            {#if showPins && filteredPinnedTables.length > 0 && connectionName}
              <!-- `pb-1.5`, because the action row is 20px tall now and 4px of
                   air put it on top of the first pin. -->
              <div class="flex w-full items-center gap-1.5 px-2.5 pt-2 pb-1.5">
                <Icon name="pin" class="size-3 shrink-0 text-muted-foreground" />
                <span class="text-ui-2xs font-medium tracking-wider text-muted-foreground uppercase">Pinned</span>
                {@render countBadge(filteredPinnedTables.length, visiblePinnedTables.length, true)}
                <!-- The actions own the trailing edge, the count stays with the
                     name. `stopPropagation` on every one of them: the list's
                     own click handler clears the selection for any click that
                     did not land on a row, and the header is not a row - so
                     "select all" selected seven tables and the same click
                     deselected them again before the frame was out. -->
                <!-- `size-5` with `gap-1.5`, not `size-4` with `gap-1`:
                     `hit-area` gives each of these a 24px target whatever their
                     visual size, and 16px buttons 4px apart put those targets
                     20px apart - overlapping by 4px, so a click near the edge
                     of one landed on the other. 20px buttons 6px apart clear
                     each other, and a hover surface makes them read as buttons
                     rather than as two glyphs in the heading. -->
                <div class="ml-auto flex shrink-0 items-center gap-1.5">
                  <button
                    type="button"
                    aria-pressed={allPinnedSelected}
                    class={cn(
                      'hit-area inline-flex size-5 items-center justify-center rounded-md transition-colors hover:bg-muted/50 hover:text-foreground',
                      allPinnedSelected ? 'text-primary' : 'text-muted-foreground',
                    )}
                    onclick={(e) => { e.stopPropagation(); toggleSelectAllPinned() }}
                    title={allPinnedSelected
                      ? `Deselect all ${filteredPinnedTables.length} pinned tables`
                      : `Select all ${filteredPinnedTables.length} pinned tables`}
                  >
                    <Icon name={allPinnedSelected ? 'check-circle-2' : 'check'} class="size-3.5" />
                  </button>
                  <button
                    type="button"
                    class="hit-area inline-flex size-5 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
                    onclick={(e) => { e.stopPropagation(); openAllPinned() }}
                    title="Open all {filteredPinnedTables.length} pinned tables in tabs"
                  >
                    <Icon name="external-link" class="size-3.5" />
                  </button>
                  {#if pinnedTables.length > 5}
                    <!-- A hairline before the destructive one. It is a word
                         among glyphs and the only action here that throws
                         something away, so it gets its own side of a rule
                         rather than sitting flush against "open all". -->
                    <span class="h-3 w-px shrink-0 bg-border/50" aria-hidden="true"></span>
                    <button
                      type="button"
                      class="hit-area inline-flex h-5 items-center rounded-md px-1 font-mono text-ui-2xs text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                      onclick={(e) => { e.stopPropagation(); clearAllPins() }}
                      title="Clear all pinned tables"
                    >Clear all</button>
                  {/if}
                </div>
              </div>
              <ul class="flex w-full min-w-full flex-col px-1.5 pb-1 [&>li]:pb-0.5">
                {#each filteredPinnedTables as tableName, idx (tableName)}
                  {@const isSelected = selectedItems.has(tableName)}
                  <li data-pin={tableName}>
                    <ContextMenu.Root>
                      <ContextMenu.Trigger class="w-full">
                        <button
                          type="button"
                          tabindex="-1"
                          data-sidebar-row="pin:{tableName}"
                          data-roving
                          data-sidebar-current={activeTable === tableName ? '' : undefined}
                          class={cn(
                            "group grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-2 rounded-md px-2 py-1.5 text-left transition-colors",
                            isSelected
                              ? "bg-primary/10 text-foreground"
                              : activeTable === tableName
                                ? "bg-sidebar-accent text-sidebar-accent-foreground"
                                : "text-foreground/70 hover:bg-sidebar-accent/50 hover:text-foreground",
                          )}
                          onclick={(e) => {
                            if (e.shiftKey) { e.preventDefault(); selectItem(tableName, true) }
                            else if (e.metaKey || e.ctrlKey) { e.preventDefault(); selectItem(tableName, false) }
                            else ontableselect(tableName)
                          }}
                        >
                          <span
                            class="relative size-3 shrink-0"
                            onclick={(e) => { e.stopPropagation(); selectItem(tableName, e.shiftKey) }}
                            onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); e.stopPropagation(); selectItem(tableName, e.shiftKey); } }}
                            role="checkbox"
                            aria-checked={isSelected}
                            tabindex="-1"
                          >
                            {#if isSelected}
                              <Icon name="square-check" class="size-3 text-primary" />
                            {:else}
                              <Icon name="pin" class="absolute inset-0 size-3 text-muted-foreground group-hover:opacity-0" />
                              <Icon name="square" class="absolute inset-0 size-3 opacity-0 group-hover:opacity-40" />
                            {/if}
                          </span>
                          <span class="min-w-0 truncate font-mono text-ui-sm leading-4">{tableName}</span>
                          {#if showRowCount}
                          <span class="shrink-0 text-right font-mono text-ui-xs leading-4 tabular-nums text-muted-foreground">
                            {formatTableRowCount(_rowCountByName.get(tableName))}
                          </span>
                          {/if}
                        </button>
                      </ContextMenu.Trigger>
                      <ContextMenu.Content class="min-w-48">
                        {#if isSelected && selectedItems.size > 1}
                          <!-- Multi-select: actions apply to all selected tables -->
                          <ContextMenu.Item onSelect={openSelected}>
                            <Icon name="external-link" />
                            Open {selectedItems.size} tables
                          </ContextMenu.Item>
                          {#if [...selectedItems].some((n) => openTableSet.has(n))}
                            <ContextMenu.Item onSelect={closeSelectedTabs}>
                              <Icon name="x" />
                              Close open tabs
                            </ContextMenu.Item>
                          {/if}
                          <ContextMenu.Separator />
                          <ContextMenu.Item onSelect={copySelectedNames}>
                            <Icon name="clipboard-copy" />
                            Copy {selectedItems.size} names
                          </ContextMenu.Item>
                          <ContextMenu.Item onSelect={() => (allSelectedPinned ? unpinSelected() : pinSelected())}>
                            {#if allSelectedPinned}
                              <Icon name="pin-off" />
                              Unpin {selectedItems.size} tables
                            {:else}
                              <Icon name="pin" />
                              Pin {selectedItems.size} tables
                            {/if}
                          </ContextMenu.Item>
                          <ContextMenu.Separator />
                          <ContextMenu.Item onSelect={clearSelection}>
                            <Icon name="square" />
                            Deselect all
                          </ContextMenu.Item>
                        {:else}
                        <ContextMenu.Item onSelect={() => { navigator.clipboard.writeText(tableName) }}>
                          <Icon name="clipboard-copy" />
                          Copy name
                        </ContextMenu.Item>
                        <ContextMenu.Item onSelect={() => oncopycolumns(tableName)}>
                          <Icon name="copy" />
                          Copy columns
                        </ContextMenu.Item>
                        {#if openTableSet.has(tableName)}
                          <ContextMenu.Item onSelect={() => onclosetable(tableName)}>
                            <Icon name="x" />
                            Close tab
                          </ContextMenu.Item>
                        {/if}
                        <ContextMenu.Item onSelect={() => togglePin(tableName)}>
                          <Icon name="pin-off" />
                          Unpin table
                        </ContextMenu.Item>
                        <ContextMenu.Separator />
                        <ContextMenu.Item onSelect={() => onopeninconsole(tableName)}>
                          <Icon name="terminal" />
                          Open Console
                        </ContextMenu.Item>
                        <ContextMenu.Item onSelect={() => ongeneratesql(tableName)}>
                          <Icon name="zap" />
                          Generate SQL…
                        </ContextMenu.Item>
                        <ContextMenu.Item onSelect={() => oncountrows(tableName)}>
                          <Icon name="hash" />
                          Count rows
                        </ContextMenu.Item>
                        <ContextMenu.Separator />
                        <ContextMenu.Item onSelect={() => onviewstructure(tableName)}>
                          <Icon name="layout-list" />
                          View structure
                        </ContextMenu.Item>
                        <ContextMenu.Item onSelect={() => onopentableerd(tableName)}>
                          <Icon name="git-branch" />
                          Open in schema diagram
                        </ContextMenu.Item>
                        <ContextMenu.Item onSelect={() => onviewddl(tableName)}>
                          <Icon name="code-2" />
                          View DDL
                        </ContextMenu.Item>
                        <ContextMenu.Item onSelect={() => onexportsql(tableName)}>
                          <Icon name="file-down" />
                          Export as SQL
                        </ContextMenu.Item>
                        <ContextMenu.Item onSelect={() => onexportdata(tableName)}>
                          <Icon name="download" />
                          Export data
                        </ContextMenu.Item>
                        {/if}
                      </ContextMenu.Content>
                    </ContextMenu.Root>
                  </li>
                {/each}
              </ul>
            {/if}

            <!-- ── Tables ─────────────────────────────────────────── -->
            <!-- `!tabIsEmpty` here and on Views / Materialized views, for the
                 reason given on Databases above: an empty tab gets exactly one
                 empty state, the tab-level one below. -->
            {#if showTables && !tabIsEmpty}
              <!-- Outside `tablesOpen`, like every other section header: a
                   collapsed list still has to say what it is and how much it is
                   hiding. `regularTablesUnpinned` is the denominator because
                   pinning relocates a row into the Pinned list rather than
                   filtering it out - counting against `regularTables` would read
                   as "one table went missing" every time one is pinned. -->
              <div class="flex w-full items-center gap-1 px-2.5 pt-2 pb-1">
                <span class="text-ui-2xs font-medium tracking-wider text-muted-foreground uppercase">{$t('sidebar.tables')}</span>
                {#if regularTablesUnpinned.length > 0}
                  {@render countBadge(filteredRegularTables.length, regularTablesUnpinned.length)}
                {/if}
              </div>

            {#if tablesOpen}
              <!-- The arrow walk lives on the scroll container, which reaches
                   every section in the tab rather than this one list. -->
              <div>
              <!-- ONE context menu for the whole list, not one per row.
                   A ContextMenu.Root + Trigger per <li> is two component instances
                   per table, and this list is no longer windowed - a 5,000-table
                   schema instantiated 10,000 menu components that exist only to be
                   right-clicked, and that cost is paid in full on every schema
                   switch and every filter keystroke. Which row was clicked is read
                   off the event instead. -->
              <ContextMenu.Root>
                <ContextMenu.Trigger>
                  {#snippet child({ props })}
                    {@const openMenu = props.oncontextmenu}
                    <ul
                      {...props}
                      oncontextmenu={(e) => {
                        const li = e.target instanceof Element ? e.target.closest("li[data-table]") : null
                        if (!(li instanceof HTMLElement)) return
                        menuTable = li.dataset.table ?? ""
                        openMenu?.(e)
                      }}
                      class="flex w-full min-w-full flex-col px-1.5 pb-1 [&>li]:pb-0.5"
                    >
                {#if regularTables.length === 0 && tables.length > 0}
                  <!-- The tab-level empty state covers this. -->
                {:else}
                  {#each filteredRegularTables as table (table.name)}
                    {@const isSelected = selectedItems.has(table.name)}
                    <li data-table={table.name}>
                    <button
                      type="button"
                      tabindex="-1"
                      data-sidebar-row="table:{table.name}"
                      data-roving
                      data-sidebar-current={activeTable === table.name ? '' : undefined}
                      class={cn(
                        "group grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-2 rounded-md px-2 py-1.5 text-left transition-colors",
                        isSelected
                          ? "bg-primary/10 text-foreground"
                          : activeTable === table.name
                            ? "bg-sidebar-accent text-sidebar-accent-foreground"
                            : "text-foreground/70 hover:bg-sidebar-accent/50 hover:text-foreground",
                      )}
                      onclick={(e) => {
                        if (e.shiftKey) { e.preventDefault(); selectItem(table.name, true) }
                        else if (e.metaKey || e.ctrlKey) { e.preventDefault(); selectItem(table.name, false) }
                        else ontableselect(table.name)
                      }}
                      title={comments.get(table.name) || undefined}
                    >
                      <span
                        class="relative size-3.5 shrink-0"
                        onclick={(e) => { e.stopPropagation(); selectItem(table.name, e.shiftKey) }}
                        onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); e.stopPropagation(); selectItem(table.name, e.shiftKey); } }}
                        role="checkbox"
                        aria-checked={isSelected}
                        tabindex="-1"
                      >
                        {#if isSelected}
                          <Icon name="square-check" class="size-3.5 text-primary" />
                        {:else}
                          <Icon name="table-2" class="absolute inset-0 size-3.5 opacity-70 group-hover:opacity-0" />
                          <Icon name="square" class="absolute inset-0 size-3.5 opacity-0 group-hover:opacity-70" />
                        {/if}
                      </span>
                      <span class="flex min-w-0 items-center gap-1.5">
                        <span class="min-w-0 truncate font-mono text-ui-sm leading-4">{table.name}</span>
                        {#if table.rlsEnabled}
                          <Icon
                            name="lock"
                            class="size-3 shrink-0 text-muted-foreground"
                            role="img"
                            aria-label="Row-level security enabled"
                          />
                        {/if}
                      </span>
                      {#if showRowCount}
                      <!-- Fixed min-width so the column doesn't shift every
                           row sideways as lazy counts land. A count that has
                           not arrived draws nothing at all: a placeholder mark
                           on every row made a long list read as a column of
                           dashes, which says "empty" far louder than "counting". -->
                      <span
                        class="flex min-w-[4ch] shrink-0 items-center justify-end font-mono text-ui-2xs leading-4 tabular-nums text-muted-foreground"
                        title={table.rowCount != null ? Number(table.rowCount).toLocaleString("en-US") : "Counting rows…"}
                      >
                        {#if table.rowCount != null}{formatTableRowCount(table.rowCount)}{/if}
                      </span>
                      {/if}
                      {#if comments.get(table.name)}
                        <span class="col-span-2 col-start-2 truncate text-ui-2xs leading-4 text-muted-foreground">{comments.get(table.name)}</span>
                      {/if}
                    </button>
                    </li>
                  {/each}
                {/if}
                    </ul>
                  {/snippet}
                </ContextMenu.Trigger>
            <ContextMenu.Content class="min-w-48">
              {#if menuTableSelected && selectedItems.size > 1}
                <!-- Multi-select: actions apply to all selected tables -->
                <ContextMenu.Item onSelect={openSelected}>
                  <Icon name="external-link" />
                  Open {selectedItems.size} tables
                </ContextMenu.Item>
                {#if [...selectedItems].some((n) => openTableSet.has(n))}
                  <ContextMenu.Item onSelect={closeSelectedTabs}>
                    <Icon name="x" />
                    Close open tabs
                  </ContextMenu.Item>
                {/if}
                <ContextMenu.Separator />
                <ContextMenu.Item onSelect={copySelectedNames}>
                  <Icon name="clipboard-copy" />
                  Copy {selectedItems.size} names
                </ContextMenu.Item>
                <ContextMenu.Item onSelect={() => (allSelectedPinned ? unpinSelected() : pinSelected())}>
                  {#if allSelectedPinned}
                    <Icon name="pin-off" />
                    Unpin {selectedItems.size} tables
                  {:else}
                    <Icon name="pin" />
                    Pin {selectedItems.size} tables
                  {/if}
                </ContextMenu.Item>
                <ContextMenu.Separator />
                <ContextMenu.Item onSelect={clearSelection}>
                  <Icon name="square" />
                  Deselect all
                </ContextMenu.Item>
              {:else}
              <ContextMenu.Item onSelect={() => { navigator.clipboard.writeText(menuTable) }}>
                <Icon name="clipboard-copy" />
                Copy name
              </ContextMenu.Item>
              <ContextMenu.Item onSelect={() => oncopycolumns(menuTable)}>
                <Icon name="copy" />
                Copy columns
              </ContextMenu.Item>
              {#if openTableSet.has(menuTable)}
                <ContextMenu.Item onSelect={() => onclosetable(menuTable)}>
                  <Icon name="x" />
                  Close tab
                </ContextMenu.Item>
              {/if}
              <ContextMenu.Item onSelect={() => togglePin(menuTable)}>
                {#if pinnedTables.includes(menuTable)}
                  <Icon name="pin-off" />
                  Unpin table
                {:else}
                  <Icon name="pin" />
                  Pin table
                {/if}
              </ContextMenu.Item>
              <ContextMenu.Item onSelect={() => toggleSelect(menuTable)}>
                {#if menuTableSelected}
                  <Icon name="square" />
                  Deselect
                {:else}
                  <Icon name="square-check" />
                  Select
                {/if}
              </ContextMenu.Item>
              <ContextMenu.Separator />
              <ContextMenu.Item onSelect={() => onopeninconsole(menuTable)}>
                <Icon name="terminal" />
                Open in SQL console
              </ContextMenu.Item>
              <ContextMenu.Item onSelect={() => ongeneratesql(menuTable)}>
                <Icon name="zap" />
                Generate SQL…
              </ContextMenu.Item>
              <ContextMenu.Item onSelect={() => oncountrows(menuTable)}>
                <Icon name="hash" />
                Count rows
              </ContextMenu.Item>
              <ContextMenu.Separator />
              <ContextMenu.Item onSelect={() => onviewstructure(menuTable)}>
                <Icon name="layout-list" />
                View structure
              </ContextMenu.Item>
              <ContextMenu.Item onSelect={() => onviewddl(menuTable)}>
                <Icon name="code-2" />
                View DDL
              </ContextMenu.Item>
              <ContextMenu.Item onSelect={() => onexportsql(menuTable)}>
                <Icon name="file-down" />
                Export as SQL
              </ContextMenu.Item>
              <ContextMenu.Item onSelect={() => onexportdata(menuTable)}>
                <Icon name="download" />
                Export data
              </ContextMenu.Item>
              {#if tableDdl}
              <ContextMenu.Separator />
              <ContextMenu.Item
                disabled={$readOnlyMode}
                title={$readOnlyMode ? READ_ONLY_HINT : undefined}
                onSelect={() => openDangerDialog('truncate', menuTable)}
              >
                <Icon name="eraser" />
                Truncate table
              </ContextMenu.Item>
              <ContextMenu.Item
                variant="destructive"
                disabled={$readOnlyMode}
                title={$readOnlyMode ? READ_ONLY_HINT : undefined}
                onSelect={() => openDangerDialog('drop', menuTable)}
              >
                <Icon name="trash-2" />
                Drop table
              </ContextMenu.Item>
              {/if}
              {/if}
            </ContextMenu.Content>
              </ContextMenu.Root>
              </div>
            {/if}
            {/if}

            <!-- ── Empty state, one per tab ──────────────────────── -->
            {#if tabIsEmpty}
              {@const empty = TAB_EMPTY[sidebarTab]}
              <div class="flex flex-1 flex-col items-center justify-center gap-3 px-6 py-16 text-center">
                <div class="flex size-10 items-center justify-center rounded-lg border border-border bg-muted/30">
                  <Icon name={tabEmptyFromFilter ? 'search' : empty.icon} class="size-5 text-muted-foreground" />
                </div>
                <div class="max-w-[16rem]">
                  <p class="text-ui-xs font-medium text-foreground">
                    {tabEmptyFromFilter ? 'No matches' : empty.title}
                  </p>
                  <p class="mt-1 text-ui-2xs leading-relaxed text-muted-foreground">
                    {#if tabEmptyFromFilter}
                      Nothing here matches “{debouncedFilter}”.
                    {:else if sidebarTab === 'tables' && activeSchema}
                      Nothing in “{activeSchema}” yet.
                    {:else}
                      {empty.hint}
                    {/if}
                  </p>
                </div>
                {#if tabEmptyFromFilter}
                  <Button variant="outline" size="sm" onclick={() => handleFilterInput('')}>
                    <Icon name="x" class="size-3.5" />
                    Clear filter
                  </Button>
                {:else if sidebarTab === 'databases'}
                  <!-- The section header is not rendered in this state, so its
                       two actions live here instead of being unreachable. -->
                  <div class="flex items-center gap-2">
                    <Button variant="outline" size="sm" onclick={() => void loadDatabases()} disabled={dbEntriesLoading}>
                      <Icon name="refresh-cw" class={cn('size-3.5', dbEntriesLoading && 'animate-spin')} />
                      Refresh
                    </Button>
                    {#if dbAdmin}
                      <Button
                        variant="outline"
                        size="sm"
                        onclick={onnewdatabase}
                        disabled={$readOnlyMode}
                        title={$readOnlyMode ? READ_ONLY_HINT : undefined}
                      >
                        <Icon name="plus" class="size-3.5" />
                        New database
                      </Button>
                    {/if}
                  </div>
                {:else if sidebarTab === 'tables'}
                  <Button variant="outline" size="sm" onclick={onrefresh}>
                    <Icon name="refresh-cw" class="size-3.5" />
                    Refresh
                  </Button>
                {/if}
              </div>
            {/if}


          {/if}

        </div>
      </div>
      {/if}
    </div>
    {:else if navSidebarPanel === "connections"}
      <ConnectionsSidebarPanel
        {connections}
        activeId={activeConnectionId}
        onswitch={onswitchconnection}
        onadd={onaddconnection}
        onremove={onremoveconnection}
        onsetgroup={onsetconnectiongroup}
        ondisconnect={ondisconnectconnection}
      />
    {:else if navSidebarPanel === "extensions"}
      <ExtensionsSidebarPanel onopendetail={onopenextensiondetail} />
    {/if}

  </aside>
  </ContextMenu.Trigger>
  <ContextMenu.Content class="min-w-52">
    <ContextMenu.Item onSelect={() => onmoveside(side === "right" ? "left" : "right")}>
      {#if side === "right"}
        <PanelLeft /> Move sidebar to the left
      {:else}
        <PanelRight /> Move sidebar to the right
      {/if}
    </ContextMenu.Item>
  </ContextMenu.Content>
  </ContextMenu.Root>
  <ResizeHandle
    edge={side === "right" ? "start" : "end"}
    onresizestart={() => {
      resizeStartWidth = width;
      resizeScale = getAppScale();
    }}
    onresize={(dx) => {
      width = clampNavSidebarWidth(resizeStartWidth + dx / resizeScale);
    }}
    onresizeend={() => {
      resizeStartWidth = width;
      saveLayout({ navSidebarWidth: width });
    }}
  />
</div>

<DangerousActionDialog
  bind:open={dangerOpen}
  action={dangerAction}
  objectKind={dangerObjectKind}
  dialect={tableDdl ?? 'postgres'}
  schema={activeSchema}
  table={dangerTable}
  bind:cascade={dangerCascade}
  onconfirm={(c) => confirmDanger(c)}
/>

