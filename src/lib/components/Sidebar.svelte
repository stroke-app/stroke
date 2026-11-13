<script>
  import { toggleLightDark, isCurrentThemeDark } from '$lib/stores/settings.js'
  import DbIcon from './DbIcon.svelte'
  import { workspaces, activeWorkspaceId, inWorkspace } from '$lib/stores/workspaces.js'

  import { comboTitle } from '$lib/shortcuts.js'
  import { untrack } from "svelte";
  import { getAppScale } from '$lib/app-zoom.js';
  import { createCommandHotkey } from "$lib/command-hotkey.js";
  import { bindingFor, withShortcut } from "$lib/stores/keybindings.js";
  import Icon from "./Icon.svelte";
  import SearchableMenu from "./SearchableMenu.svelte";
  import FindReplacePanel from "./FindReplacePanel.svelte";
  import { listDatabases, canSwitchDatabase, currentDatabaseKey, isCurrentDatabase } from "$lib/databases.js";
  import { dbAdminKind, dbActionBlocker } from "$lib/database-admin.js";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu/index.js";
  import DangerousActionDialog from "./DangerousActionDialog.svelte";
  import { tableDialect } from "$lib/table-admin.js";
  import { readOnlyMode, guardWrite, READ_ONLY_HINT } from "$lib/stores/read-only.js";
  import { appNativeScroll, appSidebarComments, appSidebarStyle, appZoom } from "$lib/stores/settings.js";
  import { rootPxFor } from "$lib/type-scale.js";
  import SidebarObjects from "./SidebarObjects.svelte";
  import SidebarTree from "./SidebarTree.svelte";
  import { objectsVersion } from "$lib/stores/sidebar-objects.svelte.js";
  import { listObjectComments } from "$lib/api.js";
  import { smoothScroll } from "$lib/smooth-scroll.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import * as ContextMenu from "$lib/components/ui/context-menu/index.js";
  import PanelRight from "@lucide/svelte/icons/panel-right";
  import PanelLeft from "@lucide/svelte/icons/panel-left";
  import ResizeHandle from "./ResizeHandle.svelte";
  import ConnectionsSidebarPanel from "./ConnectionsSidebarPanel.svelte";
  import RedisSidebarNav from "./RedisSidebarNav.svelte";
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
    NAV_SIDEBAR_MAX_VW,
    loadLayout,
    saveLayout,
    setNavSidebarWidth,
    NAV_WIDTH_EVENT,
  } from "$lib/stores/layout.js";

  const initialLayout = loadLayout();
  let width = $state(initialLayout.navSidebarWidth);
  // The AI chat list shares this width (layout.js setNavSidebarWidth): follow
  // a resize made over there.
  $effect(() => {
    const on = (/** @type {Event} */ e) => {
      const w = /** @type {CustomEvent<number>} */ (e).detail;
      if (Number.isFinite(w) && w !== width) { width = w; resizeStartWidth = w; }
    };
    window.addEventListener(NAV_WIDTH_EVENT, on);
    return () => window.removeEventListener(NAV_WIDTH_EVENT, on);
  });
  let resizeStartWidth = initialLayout.navSidebarWidth;
  /** App scale sampled at drag start - `dx` is screen px, `width` is px at 100%. */
  let resizeScale = 1;
  /** @type {HTMLDivElement | undefined} */
  let navRootEl = $state();

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
    /** Sidebar footer (T3-style). */
    onopensettings = () => {},
    onopenshortcuts = () => {},
    oncheckupdates = () => {},
    /** @type {(id: string) => void} */
    onremoveconnection = () => {},
    /** @type {(id: string, group: string | null) => void} */
    onsetconnectiongroup = () => {},
    /** A connection's colour or workspace changed; carries the saved list. @type {(list: any[]) => void} */
    onconnectionmeta = () => {},
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
    /** False for NoSQL engines: hides the SQL-only table actions (DDL, structure,
     *  export as SQL, generate SQL, new table/schema, ...). */
    sqlTables = true,
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
  /**
   * The schema picker earns its row when there is a choice to make. A file
   * database (SQLite, D1, libSQL, DuckDB) reports one `main` and nothing else
   * unless something is attached, so there it shows only past one schema.
   */
  const SINGLE_FILE_ENGINES = new Set(["sqlite", "d1", "libsql", "duckdb"])
  const showSchemaPicker = $derived(
    schemas.length > 1 || (schemas.length === 1 && !SINGLE_FILE_ENGINES.has(connection?.type ?? "")),
  )

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
   * @typedef {'tables' | 'objects' | 'recent' | 'databases' | 'queries' | 'search' | 'tree'} SidebarTab
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
    { id: 'objects',   label: 'Objects',   icon: 'shapes' },
    { id: 'recent',    label: 'Recent',    icon: 'clock' },
    // Last, and not a list: find & replace is a tool that works on the table
    // you already have open, so it belongs where the other panels live rather
    // than in a modal over the rows it is about to rewrite.
    { id: 'search',    label: 'Find & replace', icon: 'replace' },
    // Tree sidebar (beta): one explorer in place of Tables, Objects and
    // Databases. Last in this list so the Mod+Shift+N numbers of the others
    // never move; it is drawn first in the strip (see visibleTabs).
    { id: 'tree',      label: 'Explorer', icon: 'list-tree' },
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

  // Panels mount on first visit and then stay, hidden while another tab is up.
  // Rebuilding one on every switch threw its DOM and scroll away, and the
  // rebuild was the flash between tabs. Find & replace still mounts per visit:
  // it previews matches against the open table's rows, which a hidden copy
  // would keep recomputing (its typed text lives here, so it survives).
  const activePanel = $derived(sidebarTab)
  let seenPanels = $state(new Set([untrack(() => sidebarTab)]))
  $effect(() => {
    const p = activePanel
    if (!untrack(() => seenPanels.has(p))) seenPanels = new Set([...untrack(() => seenPanels), p])
  })
  /** @param {string} p */
  const panelMounted = (p) => activePanel === p || seenPanels.has(p)
  // Find & replace's inputs, held here because the panel itself remounts.
  let frFind = $state('')
  let frReplace = $state('')
  let frRegex = $state(false)
  let frWhole = $state(false)
  let frCase = $state(false)

  $effect(() => {
    openFindReplace = () => {
      sidebarTab = 'search'
      // After the tab renders, or the field is not in the DOM yet.
      tick().then(() => focusFindField())
    }
    showTablesTab = () => { sidebarTab = $appSidebarStyle === 'tree' ? 'tree' : 'tables' }
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
  /**
   * Tabs this connection has something to show in. Databases is dropped where
   * there is no other database to switch to (SQLite, DuckDB, libSQL, a D1
   * without its account): the tab only ever opened onto an empty list.
   */
  /** Tree sidebar (beta): the explorer replaces Tables, Objects and Databases. */
  const treeMode = $derived($appSidebarStyle === 'tree')
  /** The tabs the explorer stands in for. */
  const TREE_REPLACES = new Set(['tables', 'objects', 'databases'])
  const visibleTabs = $derived(
    treeMode
      ? [SIDEBAR_TABS[SIDEBAR_TABS.length - 1], ...SIDEBAR_TABS.filter((t) => t.id !== 'tree' && !TREE_REPLACES.has(t.id))]
      : SIDEBAR_TABS.filter((t) => t.id !== 'tree' && (t.id !== 'databases' || canSwitchDb)),
  )
  // The strip shows as many tabs as fit beside the actions and folds the rest,
  // last first, into a "more" menu - it used to scroll sideways under them and
  // cut an icon in half at high zoom. Tabs are w-8 with gap-0.5 (rem), so the fit
  // follows the zoom even when the sidebar's width is pinned by the window cap.
  let tabStripW = $state(0)
  const stripTabs = $derived.by(() => {
    const rem = rootPxFor($appZoom)
    const slot = 2.125 * rem // w-8 + gap-0.5
    // +1px: the strip's width is whole px, the slots are not.
    const fits = Math.floor((tabStripW + 0.125 * rem + 1) / slot)
    if (!tabStripW || fits >= visibleTabs.length) return { shown: visibleTabs, more: [] }
    // One slot goes to the menu button.
    const keep = Math.max(1, fits - 1)
    let shown = visibleTabs.slice(0, keep)
    // The active tab is never folded away: it takes the last visible slot.
    if (!shown.some((t) => t.id === sidebarTab)) {
      const active = visibleTabs.find((t) => t.id === sidebarTab)
      if (active) shown = [...shown.slice(0, keep - 1), active]
    }
    return { shown, more: visibleTabs.filter((t) => !shown.includes(t)) }
  })
  /** @param {string} id */
  function tabTitle(id) {
    const tab = SIDEBAR_TABS.find((t) => t.id === id)
    const combo = bindingFor(`sidebar.tab.${SIDEBAR_TABS.findIndex((t) => t.id === id) + 1}`)
    return combo ? `${tab?.label} · ${comboTitle(combo)}` : (tab?.label ?? '')
  }

  // A remembered Databases tab on a connection without one falls back to Tables;
  // in tree mode the lists it replaces (and their shortcuts) land on the tree.
  $effect(() => {
    if (!visibleTabs.some((t) => t.id === sidebarTab)) sidebarTab = treeMode ? 'tree' : 'tables'
  })

  /** @param {number} delta */
  function cycleSidebarTab(delta) {
    const tabs = visibleTabs
    const i = tabs.findIndex((t) => t.id === sidebarTab)
    const next = (i + delta + tabs.length) % tabs.length
    sidebarTab = tabs[next].id
  }
  // Numbers stay fixed per tab (Mod+Shift+2 is always Databases), so a hidden
  // tab's shortcut does nothing rather than shifting every key after it.
  // Only the six numbered tabs have a shortcut; Explorer (tree mode) takes over
  // the keys of the lists it replaces instead.
  SIDEBAR_TABS.slice(0, 6).forEach((tab, i) => {
    createCommandHotkey(`sidebar.tab.${i + 1}`, (e) => {
      if (!connectionName) return
      // In tree mode, the shortcut of a list the tree replaced opens the tree.
      const id = treeMode && TREE_REPLACES.has(tab.id) ? 'tree' : tab.id
      if (!visibleTabs.some((t) => t.id === id)) return
      e.preventDefault()
      sidebarTab = id
    })
  })
  /** Mod+Alt+Left/Right is the table's (first / last column) unless focus is here. */
  const yieldsArrowNav = () =>
    tableNav && !(document.activeElement instanceof Element && document.activeElement.closest('[data-studio-region="sidebar"]'))
  createCommandHotkey('sidebar.nextSection', (e) => {
    if (!connectionName || yieldsArrowNav()) return
    e.preventDefault()
    cycleSidebarTab(1)
  })
  createCommandHotkey('sidebar.prevSection', (e) => {
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
    objects:   { icon: 'shapes',     title: 'No objects',   hint: 'Views, functions, triggers and the like show up here.' },
    recent:    { icon: 'clock',      title: 'No recents',   hint: 'Tables you open appear here.' },
    databases: { icon: 'database',   title: 'No databases', hint: 'Nothing else on this server.' },
    tree:      { icon: 'list-tree',  title: 'Nothing here', hint: 'Connections and their schemas show up here.' },
  }
  /** No databases listed yet, but a fetch is owed, running or failed: the
   *  section shows its dots or its error, never "No databases". */
  const dbHeld = $derived(
    canSwitchDb && dbEntries.length === 0 && (dbEntriesLoading || !dbEntriesLoaded || !!dbEntriesError),
  )
  /** @param {SidebarTab} t */
  const isTabEmpty = (t) =>
    // Objects always draws its group headers: an empty group still has its +.
    t !== 'search' && t !== 'queries' && t !== 'objects' && t !== 'tree' && !loadingTables && !!connectionName &&
    tabCounts[t] === 0 && !(t === 'databases' && dbHeld)
  /** True when the tab has rows but the filter hid all of them. @param {SidebarTab} t */
  const isTabEmptyFromFilter = (t) =>
    isTabEmpty(t) &&
    !!debouncedFilter &&
    (t === 'tables'
      ? regularTablesUnpinned.length + visiblePinnedTables.length > 0
      : t === 'objects'
        ? views.length + matViews.length > 0
        : t === 'databases'
          ? dbEntries.length > 0
          : recentTabs.length > 0)

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
    tree: 0,
  })

  /** How many rows each tab holds, after the filter. @type {Record<SidebarTab, number>} */
  const tabCounts = $derived({
    tables: filteredRegularTables.length + filteredPinnedTables.length,
    objects: objectsShown,
    recent: Math.min(filteredRecent.length, 5),
    databases: filteredDbEntries.length,
    queries: queriesCount,
    search: 0,
    tree: 0,
  })
  $effect(() => { try { localStorage.setItem(SIDEBAR_TAB_KEY, sidebarTab) } catch {} })


  // Each list tab renders its own panel (see `listPanel` in the markup), which
  // asks `t === 'tables'` where the shared panel asked "is this the open tab".
  // Pinned is a section of Tables, not a tab of its own. It was a fifth icon in
  // the strip that held, for most connections, nothing at all - and it split
  // "the tables in this schema" across two places you had to switch between to
  // see. It renders above the Tables header, which is where the pins already
  // sorted in `selectableOrder`.
  /** @typedef {'tables' | 'databases' | 'recent'} ListTab */
  /** @type {ListTab[]} */
  const LIST_TABS = ['tables', 'databases', 'recent']
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
  createCommandHotkey('sidebar.pinned.1', (e) => { e.preventDefault(); const t = visiblePinnedTables[0]; if (t) ontableselect(t) })
  createCommandHotkey('sidebar.pinned.2', (e) => { e.preventDefault(); const t = visiblePinnedTables[1]; if (t) ontableselect(t) })
  createCommandHotkey('sidebar.pinned.3', (e) => { e.preventDefault(); const t = visiblePinnedTables[2]; if (t) ontableselect(t) })
  createCommandHotkey('sidebar.pinned.4', (e) => { e.preventDefault(); const t = visiblePinnedTables[3]; if (t) ontableselect(t) })
  createCommandHotkey('sidebar.pinned.5', (e) => { e.preventDefault(); const t = visiblePinnedTables[4]; if (t) ontableselect(t) })

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
    // Shift-click selects the range from the anchor - the last row clicked, or
    // the open table - to this one, as in Finder. It used to need a row
    // already checkbox-selected, so the usual "click one, shift-click another"
    // selected only the second.
    const anchor = lastSelectedName ?? activeTable ?? null;
    if (shiftKey && anchor && anchor !== name) {
      const a = selectableOrder.indexOf(anchor)
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
   * Every row the open list draws, in order, as `data-sidebar-row` keys - from
   * the data, not the DOM. The Tables list is windowed (see "Tables window"
   * below), so most of its rows are not in the DOM at any moment; the row
   * count, the ↵ chip, Enter in the filter, the arrow / Tab walk and the
   * roving tab stop all read this instead. Each branch mirrors the markup's
   * own conditions for drawing that section. Disabled rows (the current
   * database) are left out, as the DOM query used to leave them out.
   * @type {string[]}
   */
  const navKeys = $derived.by(() => {
    if (loadingTables) return []
    switch (activePanel) {
      case 'tables': {
        const pins = connectionName ? filteredPinnedTables.map((n) => `pin:${n}`) : []
        const rows = !isTabEmpty('tables') && !(regularTables.length === 0 && tables.length > 0)
          ? filteredRegularTables.map((t) => `table:${t.name}`)
          : []
        return [...pins, ...rows]
      }
      case 'databases':
        return canSwitchDb && connectionName && filteredDbEntries.length > 0
          ? filteredDbEntries.filter((d) => !isCurrentDatabase(d, connection)).map((d) => `db:${d.key}`)
          : []
      case 'recent':
        return connectionName ? filteredRecent.slice(0, 5).map((r) => `recent:${r.schema}.${r.table}`) : []
      default:
        return []
    }
  })
  /** How many rows the open list is drawing. Drives the ↵ chip and Enter. */
  const renderedRowCount = $derived(navKeys.length)

  // ── One tab stop for the list (ARIA APG roving tabindex) ──────────────────
  // Every row was its own tab stop, so crossing the sidebar with Tab took one
  // press per table - 23 of them on the schema in the screenshot - and the focus
  // ring crawled the list a row at a time instead of moving between the controls
  // around it. A list is one widget: Tab reaches it once, arrows move inside it,
  // Tab leaves it. Every row renders `tabindex="-1"` except `stopKey`'s.
  /** The row holding the tab stop, as its `data-sidebar-row` key. */
  let rovingRowKey = $state(/** @type {string | null} */ (null))

  /**
   * The row that should hold the stop: the one last focused, else the row for
   * the open table, else the first. The fallbacks are what make Tab land
   * somewhere useful after the list is replaced - a schema switch leaves
   * `rovingRowKey` pointing at a row that no longer exists. The open table's
   * pin wins over its table row, as it did in DOM order.
   */
  const stopKey = $derived.by(() => {
    const keys = navKeys
    if (rovingRowKey && keys.includes(rovingRowKey)) return rovingRowKey
    if (activeTable) {
      const cur = keys.find((k) =>
        k === `pin:${activeTable}` || k === `table:${activeTable}` ||
        (k.startsWith('recent:') && k.endsWith(`.${activeTable}`)))
      if (cur) return cur
    }
    return keys[0] ?? null
  })

  /** The rendered element for a row key, if it is in the DOM. @param {string | null} key */
  function rowEl(key) {
    const root = scrollContainerEl
    if (!root || !key) return null
    const el = root.querySelector(`[data-sidebar-row="${CSS.escape(key)}"]`)
    return el instanceof HTMLElement ? el : null
  }

  /**
   * Focus a row by key. A table row may be outside the window: it is scrolled
   * into view and rendered first, then focused.
   * @param {string | null} key
   */
  function focusRow(key) {
    if (!key) return false
    if (key.startsWith('table:')) {
      // Promoted before the flush: the stop is always rendered, wherever it is.
      rovingRowKey = key
      revealTableRow(tableIndexByName.get(key.slice(6)) ?? -1)
      flushSync()
    }
    const el = rowEl(key)
    if (!el) return false
    el.focus()
    return true
  }

  /** The tab stop's element, scrolled into view and rendered if it is a windowed row. */
  function rovingRow() {
    const key = stopKey
    if (key?.startsWith('table:')) {
      revealTableRow(tableIndexByName.get(key.slice(6)) ?? -1)
      flushSync()
    }
    return rowEl(key)
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
  /** Each list tab's scroller. @type {Record<ListTab, HTMLElement | null>} */
  let listScrollers = $state({ tables: null, databases: null, recent: null })
  /** The open list tab's scroller, null on the other tabs: every row query
   *  (rowEl, the pin anchor) stays inside what is on screen. */
  const scrollContainerEl = $derived(
    /** @type {Record<string, HTMLElement | null>} */ (listScrollers)[activePanel] ?? null,
  )

  // A display:none scroller loses its offset, so each list puts its own back.
  // Below the declarations on purpose: $effect.pre runs the moment it is
  // created, so above them the reads hit the TDZ and threw on mount.
  /** @type {Record<string, number>} */
  const listScroll = {}
  let lastPanel = untrack(() => activePanel)
  $effect.pre(() => {
    const p = activePanel
    untrack(() => {
      const el = /** @type {Record<string, HTMLElement | null>} */ (listScrollers)[lastPanel]
      if (lastPanel !== p && el) listScroll[lastPanel] = el.scrollTop
      lastPanel = p
    })
  })
  $effect(() => {
    const el = scrollContainerEl
    const at = untrack(() => listScroll[activePanel])
    if (el && at && el.scrollTop !== at) el.scrollTop = at
  })

  // ── Tables window ───────────────────────────────────────────────────────
  // The Tables list draws only the rows in view, plus an overscan. Rendering
  // all of them cost ~14 elements a row, and with a few thousand tables every
  // return to this tab and every filter clear paid style setup for all of it.
  //
  // It was windowed once before and taken out, for two bugs; both are designed
  // out here, the way the tree sidebar (SidebarTree) does it:
  //   - Jitter, from a fractional row stride that `offsetTop` reported as an
  //     integer. Row heights are now measured once from probes, rounded to
  //     whole px, and used for BOTH the stride and the painted height, so a
  //     row's position is exact integer maths and nothing is read back per row.
  //   - A blank list behind a full-height spacer, from a window computed off a
  //     stale offset after the list shrank. The offset is re-read from the
  //     scroller after every change to the list's shape (the browser has
  //     clamped it by then), and the window itself never runs past the end: a
  //     stale offset shows the last rows, not nothing.
  // The offset is read synchronously in the scroll event, so the window is
  // never a frame behind the content. Rows with a comment take two lines, so
  // the stride is per row (prefix sums), still whole px.
  // `content-visibility` was tried instead and does not help in WebKit.
  /** @type {HTMLUListElement | null} */
  let tableListEl = $state(null)
  /** @type {HTMLElement | null} */
  let rowProbe = $state(null)
  /** @type {HTMLElement | null} */
  let rowProbe2 = $state(null)
  /** One row's stride in whole px, one-line and with a comment line. */
  let rowH = $state(28)
  let rowH2 = $state(44)
  let tScrollTop = $state(0)
  let tViewH = $state(600)
  /** Where the list starts inside the scroller's content (pins and header sit above). */
  let tListTop = $state(0)
  const T_OVERSCAN = 8

  /** Row offsets, length n + 1: row i spans [o[i], o[i + 1]). */
  const tableOffsets = $derived.by(() => {
    const list = filteredRegularTables
    const o = new Float64Array(list.length + 1)
    const withComments = comments.size > 0
    for (let i = 0; i < list.length; i++) {
      o[i + 1] = o[i] + (withComments && comments.get(list[i].name) ? rowH2 : rowH)
    }
    return o
  })
  const tableIndexByName = $derived(new Map(filteredRegularTables.map((t, i) => [t.name, i])))

  /** The last row starting at or above `y`. @param {Float64Array} o @param {number} y */
  function rowAtY(o, y) {
    let lo = 0, hi = o.length - 2
    if (hi < 0) return 0
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1
      if (o[mid] <= y) lo = mid
      else hi = mid - 1
    }
    return lo
  }

  /** The rows to draw: the window, plus the focused row and the tab stop wherever they are. */
  const tableRendered = $derived.by(() => {
    const list = filteredRegularTables
    const o = tableOffsets
    const n = list.length
    if (n === 0) return /** @type {{ table: any, i: number }[]} */ ([])
    const total = o[n]
    let y0 = Math.max(0, tScrollTop - tListTop)
    // A stale offset past the end draws the last screenful, never nothing.
    if (y0 >= total) y0 = Math.max(0, total - tViewH)
    const y1 = y0 + Math.max(tViewH, 1)
    const start = Math.max(0, rowAtY(o, y0) - T_OVERSCAN)
    const end = Math.min(n, rowAtY(o, y1) + 1 + T_OVERSCAN)
    /** @type {number[]} */
    const extra = []
    for (const key of [rovingRowKey, stopKey]) {
      if (!key?.startsWith('table:')) continue
      const i = tableIndexByName.get(key.slice(6))
      if (i !== undefined && (i < start || i >= end) && !extra.includes(i)) extra.push(i)
    }
    const out = []
    for (const i of extra) if (i < start) out.push({ table: list[i], i })
    for (let i = start; i < end; i++) out.push({ table: list[i], i })
    for (const i of extra) if (i >= end) out.push({ table: list[i], i })
    out.sort((a, b) => a.i - b.i)
    return out
  })

  /** Read the scroller, the list's place in it and the row heights. */
  function measureTables() {
    const sc = listScrollers.tables
    // Hidden (another tab is up) measures 0: keep the last real values.
    if (!sc || sc.clientHeight === 0) return
    tViewH = sc.clientHeight
    const a = rowProbe?.getBoundingClientRect().height
    if (a && a > 0) rowH = Math.max(16, Math.round(a))
    const b = rowProbe2?.getBoundingClientRect().height
    if (b && b > 0) rowH2 = Math.max(rowH, Math.round(b))
    tScrollTop = sc.scrollTop
    if (tableListEl) tListTop = tableListEl.getBoundingClientRect().top - sc.getBoundingClientRect().top + sc.scrollTop
  }

  function onTablesScroll() {
    const sc = listScrollers.tables
    if (!sc) return
    tScrollTop = sc.scrollTop
    if (tableListEl) tListTop = tableListEl.getBoundingClientRect().top - sc.getBoundingClientRect().top + sc.scrollTop
  }

  // Zoom (rem changes the probes), a resize, and the panel coming back from
  // display:none all land here.
  $effect(() => {
    const sc = listScrollers.tables
    const p1 = rowProbe, p2 = rowProbe2
    if (!sc) return
    const ro = new ResizeObserver(() => measureTables())
    ro.observe(sc)
    if (p1) ro.observe(p1)
    if (p2) ro.observe(p2)
    return () => ro.disconnect()
  })

  // Any change to the list's shape: re-read the offset after the DOM updated.
  // A list that shrank under the offset has been clamped by the browser by now,
  // and the window follows it rather than sitting past the end.
  $effect(() => {
    void tableOffsets
    void filteredPinnedTables.length
    void tableListEl
    void loadingTables
    void activePanel
    untrack(measureTables)
  })

  /**
   * Scroll the scroller so row `i` of the Tables list is fully in view, nearest
   * edge; a row already in view stays put. Updates the window synchronously.
   * @param {number} i
   */
  function revealTableRow(i) {
    const sc = listScrollers.tables
    const o = tableOffsets
    if (!sc || i < 0 || i >= o.length - 1 || sc.clientHeight === 0) return
    const top = tListTop + o[i]
    const bottom = tListTop + o[i + 1]
    if (top < sc.scrollTop) sc.scrollTop = top
    else if (bottom > sc.scrollTop + sc.clientHeight) sc.scrollTop = bottom - sc.clientHeight
    tScrollTop = sc.scrollTop
  }

  // The open table's row follows the open tab: when another table becomes the
  // active one (a tab switch, Cmd+K, a pin), its row is brought into view.
  let revealedTable = untrack(() => activeTable)
  $effect(() => {
    const name = activeTable
    if (name === revealedTable) return
    revealedTable = name
    untrack(() => {
      if (!name || activePanel !== 'tables') return
      const i = tableIndexByName.get(name)
      if (i !== undefined) revealTableRow(i)
    })
  })
  const dbEntriesToRender = $derived(filteredDbEntries)

  // ── Objects tab ─────────────────────────────────────────────────────────
  /** True while the Objects tab loads, for the bar's refresh spinner. */
  let objectsLoading = $state(false)
  /** @type {SidebarObjects | null} */
  let objectsPanel = $state(null)
  /** The Tree sidebar (beta), when it is the open tab. @type {SidebarTree | null} */
  let treePanel = $state(null)
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


  // ── Connections at the sidebar's foot ────────────────────────────────────
  const CONNS_LIMIT = 10
  let connsOpen = $state((() => { try { return localStorage.getItem('stroke:sidebar-conns-open') !== '0' } catch { return true } })())
  let connsShowAll = $state(false)
  let connsQuery = $state('')
  /** @type {HTMLInputElement | null} */
  let connsSearchEl = $state(null)
  /** The list after the search: name, engine, host or database. */
  const connsShown = $derived.by(() => {
    const q = connsQuery.trim().toLowerCase()
    if (!q) return wsConnections.slice(0, connsShowAll ? undefined : CONNS_LIMIT)
    return wsConnections.filter((c) =>
      [c.name, c.type, c.host, c.database, /** @type {any} */ (c).filePath].some((v) => v && String(v).toLowerCase().includes(q)),
    )
  })
  /** This workspace's connections: pinned first, then most recently used. */
  const wsConnections = $derived(
    inWorkspace(connections, $activeWorkspaceId, $workspaces)
      .filter((c) => !(/** @type {any} */ (c).origin))
      .slice()
      .sort((a, b) => (Number(!!b.pinned) - Number(!!a.pinned)) || ((b.lastConnectedAt ?? 0) - (a.lastConnectedAt ?? 0))),
  )
  /** "3m", "2h", "1d", "5w": the T3 style. @param {number | null | undefined} ts */
  function ago(ts) {
    if (!ts) return ''
    const s = Math.max(0, (Date.now() - ts) / 1000)
    if (s < 3600) return `${Math.max(1, Math.round(s / 60))}m`
    if (s < 86400) return `${Math.round(s / 3600)}h`
    if (s < 86400 * 14) return `${Math.round(s / 86400)}d`
    return `${Math.round(s / (86400 * 7))}w`
  }
  /** @type {{ c: any, x: number, y: number } | null} */
  let connHover = $state(null)
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let connHoverTimer
  /** @param {any} c @param {EventTarget | null} el */
  function showConnCard(c, el) {
    clearTimeout(connHoverTimer)
    const r = /** @type {HTMLElement} */ (el)?.getBoundingClientRect?.()
    if (!r) return
    connHoverTimer = setTimeout(() => { connHover = { c, x: Math.round(r.right + 8), y: Math.round(r.top - 4) } }, 350)
  }
  function hideConnCard() {
    clearTimeout(connHoverTimer)
    connHover = null
  }

  /** macOS keeps the OS traffic lights over the sidebar's top-left: its top row leaves them room. */
  const isMacChrome = typeof document !== 'undefined' && document.documentElement.dataset.os === 'macos'
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

<!-- The single-table context menu: the Tables list's, and the Tree sidebar's
     for tables in the open schema, so the two can never drift apart. -->
{#snippet tableActions(/** @type {string} */ name, /** @type {boolean} */ inTree)}
  <ContextMenu.Item onSelect={() => { navigator.clipboard.writeText(name) }}>
    <Icon name="clipboard-copy" />
    Copy name
  </ContextMenu.Item>
  {#if sqlTables}
  <ContextMenu.Item onSelect={() => oncopycolumns(name)}>
    <Icon name="copy" />
    Copy columns
  </ContextMenu.Item>
  {/if}
  {#if openTableSet.has(name)}
    <ContextMenu.Item onSelect={() => onclosetable(name)}>
      <Icon name="x" />
      Close tab
    </ContextMenu.Item>
  {/if}
  <ContextMenu.Item onSelect={() => togglePin(name)}>
    {#if pinnedTables.includes(name)}
      <Icon name="pin-off" />
      Unpin table
    {:else}
      <Icon name="pin" />
      Pin table
    {/if}
  </ContextMenu.Item>
  {#if !inTree}
  <ContextMenu.Item onSelect={() => toggleSelect(name)}>
    {#if selectedItems.has(name)}
      <Icon name="square" />
      Deselect
    {:else}
      <Icon name="square-check" />
      Select
    {/if}
  </ContextMenu.Item>
  {/if}
  <ContextMenu.Separator />
  <ContextMenu.Item onSelect={() => onopeninconsole(name)}>
    <Icon name="terminal" />
    Open in SQL console
  </ContextMenu.Item>
  {#if sqlTables}
  <ContextMenu.Item onSelect={() => ongeneratesql(name)}>
    <Icon name="zap" />
    Generate SQL…
  </ContextMenu.Item>
  {/if}
  {#if sqlTables}
  <ContextMenu.Item onSelect={() => oncountrows(name)}>
    <Icon name="hash" />
    Count rows
  </ContextMenu.Item>
  {/if}
  {#if sqlTables}
  <ContextMenu.Separator />
  <ContextMenu.Item onSelect={() => onviewstructure(name)}>
    <Icon name="layout-list" />
    View structure
  </ContextMenu.Item>
  {/if}
  {#if sqlTables}
  <ContextMenu.Item onSelect={() => onviewddl(name)}>
    <Icon name="code-2" />
    View DDL
  </ContextMenu.Item>
  {/if}
  {#if sqlTables}
  <ContextMenu.Item onSelect={() => onexportsql(name)}>
    <Icon name="file-down" />
    Export as SQL
  </ContextMenu.Item>
  {/if}
  {#if sqlTables}
  <ContextMenu.Item onSelect={() => onexportdata(name)}>
    <Icon name="download" />
    Export data
  </ContextMenu.Item>
  {/if}
  {#if tableDdl}
  <ContextMenu.Separator />
  <ContextMenu.Item
    disabled={$readOnlyMode}
    title={$readOnlyMode ? READ_ONLY_HINT : undefined}
    onSelect={() => openDangerDialog('truncate', name)}
  >
    <Icon name="eraser" />
    Truncate table
  </ContextMenu.Item>
  <ContextMenu.Item
    variant="destructive"
    disabled={$readOnlyMode}
    title={$readOnlyMode ? READ_ONLY_HINT : undefined}
    onSelect={() => openDangerDialog('drop', name)}
  >
    <Icon name="trash-2" />
    Drop table
  </ContextMenu.Item>
  {/if}
{/snippet}
{#snippet treeTableMenu(/** @type {string} */ name)}
  {@render tableActions(name, true)}
{/snippet}

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
  {@const cls = cn(tight ? "" : "ml-auto", "shrink-0 font-mono text-ui-2xs tabular-nums text-muted-foreground")}
  {#if visible !== total}
    <span class={cls} title="{visible} shown · {total - visible} hidden of {total}"
      >{visible}<span class="text-muted-foreground">/{total}</span></span>
  {:else}
    <span class={cls}>{total}</span>
  {/if}
{/snippet}

<div
  class={cn("flex h-full shrink-0", side === "right" && "flex-row-reverse")}
  bind:this={navRootEl}
  style:width="min(calc({width}px * var(--app-scale, 1)), {NAV_SIDEBAR_MAX_VW}vw)"
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
    {#if navSidebarPanel === "tables" && (connection?.type === "redis" || connection?.type === "valkey")}
    <!-- Redis: no tables. The same full-height column, holding the keyspace's
         own navigation (views, databases, namespaces). -->
    <div class="flex min-h-0 flex-1 flex-col">
      <div class="flex h-[52px] shrink-0 items-center gap-2 pr-3 {isMacChrome ? 'pl-[92px]' : 'pl-4'}" data-tauri-drag-region>
        <span class="min-w-0 truncate text-ui-sm font-medium text-foreground" data-tauri-drag-region>{connection?.type === "valkey" ? "Valkey" : "Redis"}</span>
      </div>
      <RedisSidebarNav />
    </div>
    {@render connectionsSection()}
    {:else if navSidebarPanel === "tables"}
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
        <div class="flex h-[52px] shrink-0 items-center gap-1 pr-3 {isMacChrome ? 'pl-[92px]' : 'pl-3'}" data-tauri-drag-region>
          <div
            role="tablist"
            aria-label="Sidebar sections"
            class="flex min-w-0 flex-1 items-center gap-0.5 overflow-hidden"
            bind:clientWidth={tabStripW}
          >
            {#each stripTabs.shown as tab (tab.id)}
              {@const active = sidebarTab === tab.id}
              {@const count = tabCounts[tab.id]}
              <button
                type="button"
                role="tab"
                data-roving
                aria-selected={active}
                aria-label={count > 0 ? `${tab.label}, ${count}` : tab.label}
                title={tabTitle(tab.id)}
                data-sidebar-tab={tab.id}
                tabindex={active ? 0 : -1}
                disabled={!connectionName}
                class={cn(
                  // Full row height, 32px wide: the whole strip is the target, which
                  // clears 24x24 with room over and lets the accent rail sit on the
                  // row's own bottom edge. It was a 28px pill with the rail pushed
                  // 7px below it, so the rail floated in the gap between the pill and
                  // the border, attached to neither.
                  // No colour transition: the rail snaps on the switch, so a fading
                  // pill and icon left two tabs half-lit for 150ms on every change.
                  "group/tab relative inline-flex h-9 w-8 shrink-0 items-center justify-center disabled:pointer-events-none disabled:opacity-40",
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
                  const i = visibleTabs.findIndex((t) => t.id === sidebarTab)
                  const next = (i + (e.key === "ArrowRight" ? 1 : -1) + visibleTabs.length) % visibleTabs.length
                  // Focus by id once the strip re-renders: a folded tab only gets a
                  // button when it becomes active.
                  const strip = e.currentTarget.parentElement
                  const id = visibleTabs[next].id
                  sidebarTab = id
                  tick().then(() => {
                    const btn = strip?.querySelector(`[data-sidebar-tab="${id}"]`)
                    if (btn instanceof HTMLElement) btn.focus()
                  })
                }}
              >
                <!-- The pill is a child, not the button's own background: the
                     button spans the full row so its target is generous, while
                     the shape you see stays 28px and centred. -->
                <span
                  class={cn(
                    "pointer-events-none absolute inset-x-0.5 inset-y-1 rounded-md",
                    active ? "bg-sidebar-accent" : "group-hover/tab:bg-sidebar-accent/50",
                  )}
                  aria-hidden="true"
                ></span>
                <Icon name={tab.icon} class="relative size-4 shrink-0" />
              </button>
            {/each}
            {#if stripTabs.more.length}
              <DropdownMenu.Root>
                <DropdownMenu.Trigger
                  class="group/tab relative inline-flex h-9 w-8 shrink-0 items-center justify-center text-muted-foreground hover:text-foreground disabled:pointer-events-none disabled:opacity-40 data-[state=open]:text-foreground"
                  disabled={!connectionName}
                  title="More sections"
                  aria-label="More sections"
                >
                  <span class="pointer-events-none absolute inset-x-0.5 inset-y-1 rounded-md group-hover/tab:bg-sidebar-accent/50 group-data-[state=open]/tab:bg-sidebar-accent/50" aria-hidden="true"></span>
                  <Icon name="more-horizontal" class="relative size-4 shrink-0" />
                </DropdownMenu.Trigger>
                <DropdownMenu.Content align="start" class="min-w-44">
                  {#each stripTabs.more as tab (tab.id)}
                    {@const tabNo = SIDEBAR_TABS.indexOf(tab) + 1}
                    <DropdownMenu.Item onSelect={() => (sidebarTab = tab.id)}>
                      <Icon name={tab.icon} class="size-3.5 shrink-0" />
                      <span data-slot="menu-label">{tab.label}</span>
                      {#if tabCounts[tab.id] > 0}<span data-slot="menu-trailing" class="ml-auto font-mono text-ui-2xs tabular-nums text-muted-foreground">{tabCounts[tab.id]}</span>
                      {:else if tabNo <= 6 && bindingFor(`sidebar.tab.${tabNo}`)}<DropdownMenu.Shortcut command={`sidebar.tab.${tabNo}`} />{/if}
                    </DropdownMenu.Item>
                  {/each}
                </DropdownMenu.Content>
              </DropdownMenu.Root>
            {/if}
          </div>
          <div class="ml-auto flex shrink-0 items-center gap-0.5">
          {#if sidebarTab === 'tree'}
            <!-- The explorer's own actions, in the same slot so the bar keeps its width. -->
            <span class="invisible size-7 shrink-0" aria-hidden="true"></span>
            <button
              type="button"
              class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-40"
              title="Collapse all"
              aria-label="Collapse all"
              onclick={() => treePanel?.collapseAll()}
            >
              <Icon name="chevrons-down-up" class="size-3.5 shrink-0" />
            </button>
            <button
              type="button"
              class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-40"
              title="Refresh"
              aria-label="Refresh"
              disabled={!connectionName}
              onclick={() => { onrefresh(); treePanel?.refreshOpen() }}
            >
              <Icon name="refresh-cw" class={cn("size-3.5", loadingTables && "animate-spin")} />
            </button>
          {:else if sidebarTab === 'objects'}
            <!-- On Objects the bar's refresh and + act on objects, so the list
                 needs no header row of its own. The spacer holds the display
                 options' slot, so the group keeps its width across tabs. -->
            <span class="invisible size-7 shrink-0" aria-hidden="true"></span>
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
                <Icon name="plus" class="size-3.5 shrink-0" />
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
          <!-- These act on the table list only. On Queries and Find & replace
               they did nothing for the panel below, so they keep their space
               there (no shift between tabs) but are not shown or reachable. -->
          <div class={cn("flex items-center gap-0.5", (sidebarTab === 'queries' || sidebarTab === 'search') && "invisible")}>
          <DropdownMenu.Root>
            <DropdownMenu.Trigger
              class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground data-[state=open]:bg-accent data-[state=open]:text-foreground disabled:pointer-events-none disabled:opacity-40"
              title="Display options"
              disabled={!connectionName}
            >
              <Icon name="list-filter" class="size-3.5 shrink-0" />
            </DropdownMenu.Trigger>
            <DropdownMenu.Content align="start" class="min-w-52">
              <div class="px-2 pt-1 pb-1.5 text-ui-2xs text-muted-foreground leading-relaxed">
                <span class="font-mono tabular-nums text-foreground/80">{regularTables.length}</span> tables{#if views.length} · <span class="font-mono tabular-nums text-foreground/80">{views.length}</span> views{/if}{#if matViews.length} · <span class="font-mono tabular-nums text-foreground/80">{matViews.length}</span> mat.{/if}
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
              <DropdownMenu.Label>Sort by</DropdownMenu.Label>
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
            title={withShortcut('Refresh tables', 'view.refresh')}
            disabled={loadingTables || !connectionName}
            onclick={onrefresh}
          >
            <Icon name="refresh-cw"
              class={cn("size-3.5", loadingTables && "animate-spin")}
            />
          </button>
          {#if !sqlTables}
            <!-- NoSQL: collections are created from the query editor. -->
          {:else if supportsSchemas}
            <DropdownMenu.Root>
              <DropdownMenu.Trigger
                class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground data-[state=open]:bg-accent data-[state=open]:text-foreground disabled:pointer-events-none disabled:opacity-40"
                title="Create new…"
                disabled={!connectionName}
              >
                <Icon name="plus" class="size-3.5 shrink-0" />
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
                {#if supportsSchemas}
                <DropdownMenu.Item
                  onSelect={onnewschema}
                  disabled={$readOnlyMode}
                  title={$readOnlyMode ? READ_ONLY_HINT : undefined}
                  class="gap-2"
                >
                  <Icon name="box" class="size-3.5 shrink-0 text-muted-foreground" />
                  {$t('sidebar.newSchema')}
                </DropdownMenu.Item>
                {/if}
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
              <Icon name="plus" class="size-3.5 shrink-0" />
            </button>
          {/if}
          </div>
          {/if}
          </div>
        </div>

        <!-- The filter row belongs to the lists. Find & replace and Queries
             bring their own fields, so leaving this here would be a second
             search box with nothing to search. -->
        {#if sidebarTab !== 'search' && sidebarTab !== 'queries'}
        <!-- Filter row: the schema the list belongs to, and the filter itself. -->
        <div class="flex h-10 shrink-0 items-center gap-1 px-3">
          <!-- Shown when the engine actually has schemas to pick between, which
               is not the same question as `supportsSchemas` - that flag is
               postgres-only because Postgres is the only driver with a CREATE
               SCHEMA namespace, and it gates the "New schema" action below.
               MySQL and SQL Server both LIST schemas without supporting that,
               so gating the picker on it hid theirs. SQLite, D1 and Redis have
               no schemas at all and correctly show nothing. -->
          {#if showSchemaPicker && sidebarTab !== 'tree'}
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
                          class="flex h-7 w-full min-w-0 items-center gap-1 rounded-md px-1.5 text-ui-sm text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-foreground data-[state=open]:bg-sidebar-accent data-[state=open]:text-foreground"
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
                              <Icon name="lock" class="size-3 shrink-0 text-muted-foreground/70" />
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
            class="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground"
          />
          <input
            type="search"
            bind:this={filterEl}
            placeholder={connectionName ? "Search" : "Not connected"}
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
              if (e.key === 'Enter' && sidebarTab === 'tree') {
                flushFilter()
                flushSync()
                if (treePanel?.openSole()) e.preventDefault()
                return
              }
              if (((e.key === 'Tab' && !e.shiftKey) || e.key === 'ArrowDown') && sidebarTab === 'tree') {
                if (treePanel?.focusTree()) e.preventDefault()
                return
              }
              if (e.key === 'Enter') {
                flushFilter()
                // Commit the render too: `flushFilter` only sets state, and the
                // row has to be in the DOM to be clicked.
                flushSync()
                const keys = navKeys
                // A single row is clicked through its own button, so Enter and a
                // click cannot drift apart.
                const only = keys.length === 1 ? rowEl(keys[0]) : null
                if (only) {
                  e.preventDefault()
                  only.click()
                  return
                }
                // Nothing on screen to act on, or several. Fall back to the model
                // only where the list draws no rows at all.
                const sole = keys.length === 0 ? soleResult : null
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
                const first = navKeys[0]
                if (first) { e.preventDefault(); kbdNav = true; focusRow(first) }
              }
            }}
            class="no-field-frame h-7 w-full min-w-0 rounded-md bg-transparent pl-7 pr-7 text-ui-sm text-foreground outline-none placeholder:text-muted-foreground focus-visible:bg-sidebar-accent/60 disabled:cursor-not-allowed disabled:opacity-40 [&::-webkit-search-cancel-button]:appearance-none"
            aria-label="Filter {activeTabLabel.toLowerCase()}"
            aria-describedby="sidebar-filter-hint"
            data-sidebar-filter
          />
          {#if localFilter}
            <button
              type="button"
              tabindex="-1"
              class="hit-area absolute top-1/2 right-1.5 inline-flex size-4 -translate-y-1/2 items-center justify-center rounded-full bg-muted-foreground/25 text-sidebar transition-colors hover:bg-muted-foreground/45"
              title="Clear"
              aria-label="Clear filter"
              onclick={() => { handleFilterInput(''); flushFilter(); filterEl?.focus() }}
            >
              <Icon name="x" class="size-2.5 shrink-0" strokeWidth={3} />
            </button>
          {/if}
          </div>
          <!-- Only while a search has narrowed the list to one row. A schema
               that simply has one table showed it with an empty box, where it
               read as a stray key rather than "open your match". -->
          {#if renderedRowCount === 1 && debouncedFilter.trim() && sidebarTab !== 'tree'}
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
              class="hit-area inline-flex h-6 shrink-0 items-center gap-1 rounded-md px-1.5 text-ui-2xs text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-foreground"
              title={soleResult ? `Open ${soleResult.label} (Enter)` : 'Open the only match (Enter)'}
              aria-label={soleResult ? `Open ${soleResult.label}` : 'Open the only match'}
              onclick={() => { const only = navKeys.length === 1 ? rowEl(navKeys[0]) : null; if (only) only.click(); else soleResult?.open() }}
            >Open ↵</button>
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

      <!-- `contents` keeps each wrapper out of the flex layout, so a panel sits
           in the column exactly as it did unwrapped; `hidden` parks it. -->
      {#if panelMounted('queries')}
        <div class={activePanel === 'queries' ? 'contents' : 'hidden'}>
          {@render queriesPanel?.()}
        </div>
      {/if}
      {#if connectionName && panelMounted('objects')}
        <div class={activePanel === 'objects' ? 'contents' : 'hidden'}>
          <SidebarObjects
            bind:this={objectsPanel}
            bind:busyLoading={objectsLoading}
            panelActive={activePanel === 'objects'}
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
        </div>
      {/if}
      {#if panelMounted('tree')}
        <div class={activePanel === 'tree' ? 'contents' : 'hidden'}>
          <SidebarTree
            bind:this={treePanel}
            {connection}
            {connections}
            {schemas}
            {tables}
            {activeSchema}
            {activeTable}
            {loadingTables}
            {sqlTables}
            {databasesRefreshKey}
            filter={debouncedFilter}
            onclearfilter={() => {
              localFilter = ''
              debouncedFilter = ''
              if (filterDebounce) { clearTimeout(filterDebounce); filterDebounce = null }
              ontablefilter('')
            }}
            tableMenu={treeTableMenu}
            {ontableselect}
            onopentable={onrecentselect}
            {onschemachange}
            {onswitchconnection}
            {onswitchdatabase}
            onopenobjectsql={onopenobjectsql}
            onexittop={() => { filterEl?.focus(); filterEl?.select() }}
            {onnewtable}
            {onnewschema}
            {onnewdatabase}
            {onconnectionmeta}
          />
        </div>
      {/if}
      {#if activePanel === 'search'}
        <FindReplacePanel
          bind:focusFind={focusFindField}
          bind:findText={frFind}
          bind:replaceText={frReplace}
          bind:useRegex={frRegex}
          bind:wholeValue={frWhole}
          bind:caseSensitive={frCase}
          columns={frColumns}
          rows={frRows}
          primaryKey={frPrimaryKey}
          foreignKeys={frForeignKeys}
          tableName={frEnabled ? frTableName : null}
          onapply={onfindreplaceapply}
          onreveal={onrevealcell}
        />
      {/if}
      <!-- One mounted panel per list tab, each with its own scroller: a shared
           one re-rendered every row on each return to Tables. -->
      {#snippet listPanel(/** @type {ListTab} */ lt)}
        <div
          bind:this={listScrollers[lt]}
          class="app-scroll min-h-0 w-full flex-1 overflow-y-auto overscroll-y-contain [will-change:scroll-position]"
          role="none"
          use:smoothScroll={{ enabled: !$appNativeScroll }}
          onscroll={lt === 'tables' ? onTablesScroll : undefined}
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
              const keys = navKeys
              // The ROW, not document.activeElement: the row's checkbox span is
              // tabindex="-1", so a click near the icon focuses the span, which
              // is in no row list - every Shift+Tab from there fell through to
              // the filter instead of going to the previous table.
              const i = keys.indexOf(/** @type {HTMLElement} */ (onRow).dataset.sidebarRow ?? '')
              const next = i === -1 ? null : keys[i + (e.shiftKey ? -1 : 1)]
              if (next) {
                e.preventDefault(); kbdNav = true; focusRow(next)
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
            // Walked over the data, not the DOM: most table rows are not rendered.
            const keys = navKeys
            const focusedRow = e.target instanceof Element ? e.target.closest('[data-sidebar-row]') : null
            const i = focusedRow instanceof HTMLElement ? keys.indexOf(focusedRow.dataset.sidebarRow ?? '') : -1
            if (i === -1) return
            e.preventDefault()
            kbdNav = true
            if (e.key === 'Home') focusRow(keys[0])
            else if (e.key === 'End') focusRow(keys[keys.length - 1])
            else if (e.key === 'ArrowDown') focusRow(keys[i + 1] ?? null)
            else if (i === 0) filterEl?.focus()
            else focusRow(keys[i - 1])
          }}
        >
          {#if loadingTables}
            {@render listLoading('Loading tables')}
          {:else}
            <!-- ── Databases ──────────────────────────────────────
                 Other databases on the same server. Collapsed by default and
                 only fetched once expanded - see loadDatabases(). Engines that
                 cannot switch in place (SQLite, Redis) never render it. -->
            <!-- `!isTabEmpty(lt)`: with nothing to list, the tab-level empty state
                 below already says so in the middle of the panel. Rendering the
                 section as well put a second, quieter "No other databases" at the
                 top-left of the same empty panel - two answers to one question,
                 in two different places and two different type sizes. -->
            {#if lt === 'databases' && canSwitchDb && connectionName && !isTabEmpty(lt)}
              <div class="flex w-full items-center gap-1 px-2.5 pt-2 pb-1">
                <span class="text-ui-xs font-medium text-muted-foreground">Databases</span>
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
                      class="hit-area inline-flex size-4 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-foreground disabled:opacity-40"
                      onclick={onnewdatabase}
                      title={$readOnlyMode ? READ_ONLY_HINT : "New database"}
                      disabled={$readOnlyMode}
                    >
                      <Icon name="plus" class="size-3 shrink-0" />
                    </button>
                  {/if}
                  <button
                    type="button"
                    class="hit-area inline-flex size-4 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-foreground"
                    onclick={() => void loadDatabases()}
                    title="Refresh databases"
                    disabled={dbEntriesLoading}
                  >
                    <Icon name="refresh-cw" class={cn("size-3", dbEntriesLoading && "animate-spin")} />
                  </button>
                </div>
              </div>
              {#if lt === 'databases'}
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
                      {@const isCurrent = isCurrentDatabase(db, connection)}
                      <li>
                        <ContextMenu.Root>
                          <ContextMenu.Trigger class="w-full">
                            <button
                              type="button"
                              disabled={isCurrent}
                              data-sidebar-row="db:{db.key}"
                              data-roving
                              tabindex={stopKey === `db:${db.key}` ? 0 : -1}
                              class={cn(
                                "grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-2 rounded-md px-2 py-1.5 text-left transition-colors",
                                isCurrent
                                  ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
                                  : "text-sidebar-foreground hover:bg-sidebar-accent/60 hover:text-foreground",
                              )}
                              onclick={() => !isCurrent && onDbClick(db)}
                              ondblclick={() => !isCurrent && onDbDblClick(db)}
                              title={isCurrent ? `${db.label} (current)` : `Switch to ${db.label} · double-click to switch without confirming`}
                              aria-label={isCurrent ? `${db.label}, current database` : `Switch to ${db.label}`}
                            >
                              <Icon name="database" class="size-4 shrink-0 text-muted-foreground" />
                              <span class="min-w-0 truncate text-ui-sm leading-4">{@render hl(db.label)}</span>
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
            {#if lt === 'recent' && filteredRecent.length > 0 && connectionName}
              <div class="flex w-full items-center gap-1 px-2.5 pt-2 pb-1">
                <span class="text-ui-xs font-medium text-muted-foreground">Recent</span>
                {@render countBadge(Math.min(filteredRecent.length, 5), Math.min(recentTabs.length, 5))}
                <button
                  type="button"
                  class="shrink-0 text-ui-2xs text-muted-foreground transition-colors hover:text-destructive"
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
                            ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
                            : "text-sidebar-foreground hover:bg-sidebar-accent/60 hover:text-foreground",
                        )}
                        role="button"
                        tabindex={stopKey === `recent:${item.schema}.${item.table}` ? 0 : -1}
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
                            const keys = navKeys
                            const i = keys.indexOf(e.currentTarget.dataset.sidebarRow ?? '')
                            const next = keys[i + 1] ?? keys[i - 1] ?? null
                            onrecentremove(item.schema, item.table)
                            kbdNav = true
                            void tick().then(() => { if (!focusRow(next)) filterEl?.focus() })
                          }
                        }}
                      >
                        {#if item.tableKind === 'view'}
                          <Icon name="table-view" class="size-4 shrink-0 text-muted-foreground" />
                        {:else if item.tableKind === 'materialized_view'}
                          <Icon name="layers" class="size-4 shrink-0 text-muted-foreground" />
                        {:else}
                          <Icon name="table-2" class="size-4 shrink-0 text-muted-foreground" />
                        {/if}
                        <span class="min-w-0 truncate text-ui-sm leading-4">{@render hl(item.table)}</span>
                        <button
                          type="button"
                          tabindex="-1"
                          aria-label="Remove {item.table} from recent (Delete)"
                          class="hit-area inline-flex size-4 shrink-0 items-center justify-center rounded-md text-muted-foreground opacity-0 transition-opacity duration-150 group-hover/recent:opacity-100 hover:text-foreground focus-visible:opacity-100"
                          onclick={(e) => { e.stopPropagation(); onrecentremove(item.schema, item.table) }}
                        >
                          <Icon name="x" class="size-3 shrink-0" />
                        </button>
                      </div>
                    </li>
                  {/each}
                </ul>
              {/if}
            {/if}

            <!-- ── Pinned ─────────────────────────────────────────── -->
            {#if lt === 'tables' && filteredPinnedTables.length > 0 && connectionName}
              <!-- `pb-1.5`, because the action row is 20px tall now and 4px of
                   air put it on top of the first pin. -->
              <div class="flex w-full items-center gap-1.5 px-2.5 pt-2 pb-1.5">
                <Icon name="pin" class="size-3 shrink-0 text-muted-foreground" />
                <span class="text-ui-xs font-medium text-muted-foreground">Pinned</span>
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
                      'hit-area inline-flex size-5 items-center justify-center rounded-md transition-colors hover:bg-accent hover:text-foreground',
                      allPinnedSelected ? 'text-primary' : 'text-muted-foreground',
                    )}
                    onclick={(e) => { e.stopPropagation(); toggleSelectAllPinned() }}
                    title={allPinnedSelected
                      ? `Deselect all ${filteredPinnedTables.length} pinned tables`
                      : `Select all ${filteredPinnedTables.length} pinned tables`}
                  >
                    <Icon name={allPinnedSelected ? 'check-circle-2' : 'check'} class="size-3.5 shrink-0" />
                  </button>
                  <button
                    type="button"
                    class="hit-area inline-flex size-5 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                    onclick={(e) => { e.stopPropagation(); openAllPinned() }}
                    title="Open all {filteredPinnedTables.length} pinned tables in tabs"
                  >
                    <Icon name="external-link" class="size-3.5 shrink-0" />
                  </button>
                  {#if pinnedTables.length > 5}
                    <!-- A hairline before the destructive one. It is a word
                         among glyphs and the only action here that throws
                         something away, so it gets its own side of a rule
                         rather than sitting flush against "open all". -->
                    <span class="h-3 w-px shrink-0 bg-border/50" aria-hidden="true"></span>
                    <button
                      type="button"
                      class="hit-area inline-flex h-6 items-center rounded-md px-1.5 text-ui-2xs text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
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
                          tabindex={stopKey === `pin:${tableName}` ? 0 : -1}
                          data-sidebar-row="pin:{tableName}"
                          data-roving
                          data-sidebar-current={activeTable === tableName ? '' : undefined}
                          class={cn(
                            "group grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-2 rounded-md px-2 py-1.5 text-left transition-colors",
                            isSelected
                              ? "bg-primary/10 text-foreground"
                              : activeTable === tableName
                                ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
                                : "text-sidebar-foreground hover:bg-sidebar-accent/60 hover:text-foreground",
                          )}
                          onclick={(e) => {
                            if (e.shiftKey) { e.preventDefault(); selectItem(tableName, true) }
                            else if (e.metaKey || e.ctrlKey) { e.preventDefault(); selectItem(tableName, false) }
                            else { lastSelectedName = tableName; ontableselect(tableName) }
                          }}
                        >
                          <span
                            class="relative size-4 shrink-0"
                            onclick={(e) => { e.stopPropagation(); selectItem(tableName, e.shiftKey) }}
                            onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); e.stopPropagation(); selectItem(tableName, e.shiftKey); } }}
                            role="checkbox"
                            aria-checked={isSelected}
                            tabindex="-1"
                          >
                            {#if isSelected}
                              <Icon name="square-check" class="size-4 shrink-0 text-primary" />
                            {:else}
                              <Icon name="pin" class="absolute inset-0 size-4 text-muted-foreground group-hover:opacity-0" />
                              <Icon name="square" class="absolute inset-0 size-4 text-muted-foreground opacity-0 group-hover:opacity-100" />
                            {/if}
                          </span>
                          <span class="min-w-0 truncate text-ui-sm leading-4">{@render hl(tableName)}</span>
                          {#if showRowCount}
                          <span class="shrink-0 text-right font-mono text-ui-2xs leading-4 tabular-nums text-muted-foreground">
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
                        {#if sqlTables}
                        <ContextMenu.Item onSelect={() => oncopycolumns(tableName)}>
                          <Icon name="copy" />
                          Copy columns
                        </ContextMenu.Item>
                        {/if}
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
                        {#if sqlTables}
                        <ContextMenu.Item onSelect={() => ongeneratesql(tableName)}>
                          <Icon name="zap" />
                          Generate SQL…
                        </ContextMenu.Item>
                        {/if}
                        {#if sqlTables}
                        <ContextMenu.Item onSelect={() => oncountrows(tableName)}>
                          <Icon name="hash" />
                          Count rows
                        </ContextMenu.Item>
                        {/if}
                        {#if sqlTables}
                        <ContextMenu.Separator />
                        <ContextMenu.Item onSelect={() => onviewstructure(tableName)}>
                          <Icon name="layout-list" />
                          View structure
                        </ContextMenu.Item>
                        {/if}
                        {#if sqlTables}
                        <ContextMenu.Item onSelect={() => onopentableerd(tableName)}>
                          <Icon name="git-branch" />
                          Open in schema diagram
                        </ContextMenu.Item>
                        {/if}
                        {#if sqlTables}
                        <ContextMenu.Item onSelect={() => onviewddl(tableName)}>
                          <Icon name="code-2" />
                          View DDL
                        </ContextMenu.Item>
                        {/if}
                        {#if sqlTables}
                        <ContextMenu.Item onSelect={() => onexportsql(tableName)}>
                          <Icon name="file-down" />
                          Export as SQL
                        </ContextMenu.Item>
                        {/if}
                        {#if sqlTables}
                        <ContextMenu.Item onSelect={() => onexportdata(tableName)}>
                          <Icon name="download" />
                          Export data
                        </ContextMenu.Item>
                        {/if}
                        {/if}
                      </ContextMenu.Content>
                    </ContextMenu.Root>
                  </li>
                {/each}
              </ul>
            {/if}

            <!-- ── Tables ─────────────────────────────────────────── -->
            <!-- `!isTabEmpty(lt)` here and on Views / Materialized views, for the
                 reason given on Databases above: an empty tab gets exactly one
                 empty state, the tab-level one below. -->
            {#if lt === 'tables' && !isTabEmpty(lt)}
              <!-- Outside `tablesOpen`, like every other section header: a
                   collapsed list still has to say what it is and how much it is
                   hiding. `regularTablesUnpinned` is the denominator because
                   pinning relocates a row into the Pinned list rather than
                   filtering it out - counting against `regularTables` would read
                   as "one table went missing" every time one is pinned. -->
              <div class="flex w-full items-center gap-1 px-2.5 pt-2 pb-1">
                <span class="text-ui-xs font-medium text-muted-foreground">{$t('sidebar.tables')}</span>
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
                   per table, paid again on every schema switch and every filter
                   keystroke. Which row was clicked is read off the event instead. -->
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
                      bind:this={tableListEl}
                      class="relative w-full min-w-full"
                      style:height="calc({tableOffsets[tableOffsets.length - 1]}px + 0.25rem)"
                    >
                {#if regularTables.length === 0 && tables.length > 0}
                  <!-- The tab-level empty state covers this. -->
                {:else}
                  <!-- Height probes: one row, and one with a comment line, laid
                       out exactly like a real row. Their rounded heights are the
                       window's stride AND each row's painted height; they follow
                       the zoom because everything in them is rem. -->
                  <li bind:this={rowProbe} class="pointer-events-none invisible absolute inset-x-1.5 top-0 pb-0.5" aria-hidden="true">
                    <div class="grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-2 px-2 py-1.5">
                      <span class="size-4"></span><span class="text-ui-sm leading-4">x</span>
                    </div>
                  </li>
                  <li bind:this={rowProbe2} class="pointer-events-none invisible absolute inset-x-1.5 top-0 pb-0.5" aria-hidden="true">
                    <div class="grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-2 px-2 py-1.5">
                      <span class="size-4"></span><span class="text-ui-sm leading-4">x</span>
                      <span class="col-span-2 col-start-2 text-ui-2xs leading-4">x</span>
                    </div>
                  </li>
                  {#each tableRendered as { table, i } (table.name)}
                    {@const isSelected = selectedItems.has(table.name)}
                    <li
                      data-table={table.name}
                      class="absolute inset-x-1.5 pb-0.5"
                      style:top="{tableOffsets[i]}px"
                      style:height="{tableOffsets[i + 1] - tableOffsets[i]}px"
                      aria-setsize={filteredRegularTables.length}
                      aria-posinset={i + 1}
                    >
                    <button
                      type="button"
                      tabindex={stopKey === `table:${table.name}` ? 0 : -1}
                      data-sidebar-row="table:{table.name}"
                      data-roving
                      data-sidebar-current={activeTable === table.name ? '' : undefined}
                      class={cn(
                        "group grid h-full w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-2 rounded-md px-2 py-1.5 text-left transition-colors",
                        isSelected
                          ? "bg-primary/10 text-foreground"
                          : activeTable === table.name
                            ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
                            : "text-sidebar-foreground hover:bg-sidebar-accent/60 hover:text-foreground",
                      )}
                      onclick={(e) => {
                        if (e.shiftKey) { e.preventDefault(); selectItem(table.name, true) }
                        else if (e.metaKey || e.ctrlKey) { e.preventDefault(); selectItem(table.name, false) }
                        else { lastSelectedName = table.name; ontableselect(table.name) }
                      }}
                      title={comments.get(table.name) ? `${table.name}\n${comments.get(table.name)}` : table.name}
                    >
                      <span
                        class="relative size-4 shrink-0"
                        onclick={(e) => { e.stopPropagation(); selectItem(table.name, e.shiftKey) }}
                        onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); e.stopPropagation(); selectItem(table.name, e.shiftKey); } }}
                        role="checkbox"
                        aria-checked={isSelected}
                        tabindex="-1"
                      >
                        {#if isSelected}
                          <Icon name="square-check" class="size-4 shrink-0 text-primary" />
                        {:else}
                          <Icon name="table-2" class={cn("absolute inset-0 size-4 group-hover:opacity-0", activeTable === table.name ? "text-foreground" : "text-muted-foreground")} />
                          <Icon name="square" class="absolute inset-0 size-4 text-muted-foreground opacity-0 group-hover:opacity-100" />
                        {/if}
                      </span>
                      <span class="flex min-w-0 items-center gap-1.5">
                        <span class="min-w-0 truncate text-ui-sm leading-4">{@render hl(table.name)}</span>
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
              {@render tableActions(menuTable, false)}
              {/if}
            </ContextMenu.Content>
              </ContextMenu.Root>
              </div>
            {/if}
            {/if}

            <!-- ── Empty state, one per tab ──────────────────────── -->
            {#if isTabEmpty(lt)}
              {@const empty = TAB_EMPTY[lt]}
              <div class="flex flex-1 flex-col items-center justify-center gap-3 px-6 py-16 text-center">
                <Icon name={isTabEmptyFromFilter(lt) ? 'search' : empty.icon} class="size-5 shrink-0 text-muted-foreground" />
                <div class="max-w-[16rem]">
                  <p class="text-ui-sm text-foreground">
                    {isTabEmptyFromFilter(lt) ? 'No matches' : empty.title}
                  </p>
                  <p class="mt-1 text-ui-2xs leading-relaxed text-muted-foreground">
                    {#if isTabEmptyFromFilter(lt)}
                      Nothing here matches “{debouncedFilter}”.
                    {:else if lt === 'tables' && activeSchema}
                      Nothing in “{activeSchema}” yet.
                    {:else}
                      {empty.hint}
                    {/if}
                  </p>
                </div>
                {#if isTabEmptyFromFilter(lt)}
                  <Button variant="outline" size="sm" onclick={() => handleFilterInput('')}>
                    <Icon name="x" class="size-3.5 shrink-0" />
                    Clear filter
                  </Button>
                {:else if lt === 'databases'}
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
                        <Icon name="plus" class="size-3.5 shrink-0" />
                        New database
                      </Button>
                    {/if}
                  </div>
                {:else if lt === 'tables'}
                  <Button variant="outline" size="sm" onclick={onrefresh}>
                    <Icon name="refresh-cw" class="size-3.5 shrink-0" />
                    Refresh
                  </Button>
                {/if}
              </div>
            {/if}


          {/if}

        </div>
      {/snippet}
      {#each LIST_TABS as lt (lt)}
        {#if panelMounted(lt)}
          <div class={activePanel === lt ? 'flex min-h-0 flex-1 flex-col' : 'hidden'}>
            {@render listPanel(lt)}
          </div>
        {/if}
      {/each}
    </div>
    {@render connectionsSection()}
    {:else if navSidebarPanel === "connections"}
      <ConnectionsSidebarPanel
        {connections}
        activeId={activeConnectionId}
        onswitch={onswitchconnection}
        onadd={onaddconnection}
        onremove={onremoveconnection}
        onsetgroup={onsetconnectiongroup}
        onmetachange={onconnectionmeta}
        ondisconnect={ondisconnectconnection}
      />
    {:else if navSidebarPanel === "extensions"}
      <ExtensionsSidebarPanel onopendetail={onopenextensiondetail} />
    {/if}

    <!-- Footer, as in T3: app-wide controls as plain icons, one round action. -->
    <div class="flex h-12 shrink-0 items-center gap-1 px-3">
      <button type="button" class="inline-flex size-8 items-center justify-center rounded-md text-muted-foreground hover:text-foreground" title="Settings" aria-label="Settings" onclick={onopensettings}><Icon name="settings" class="size-4 shrink-0" /></button>
      <button type="button" class="inline-flex size-8 items-center justify-center rounded-md text-muted-foreground hover:text-foreground" title={$isCurrentThemeDark ? 'Light theme' : 'Dark theme'} aria-label="Toggle theme" onclick={() => toggleLightDark()}><Icon name={$isCurrentThemeDark ? 'sun' : 'moon'} class="size-4 shrink-0" /></button>
      <button type="button" class="inline-flex size-8 items-center justify-center rounded-md text-muted-foreground hover:text-foreground" title="Keyboard shortcuts" aria-label="Keyboard shortcuts" onclick={onopenshortcuts}><Icon name="keyboard" class="size-4 shrink-0" /></button>
      <button type="button" class="ml-auto inline-flex size-8 items-center justify-center rounded-full text-muted-foreground outline-none hover:bg-sidebar-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring" title="Check for updates" aria-label="Check for updates" onclick={oncheckupdates}><Icon name="arrow-up-circle" class="size-4 shrink-0" /></button>
    </div>
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
      resizeScale = getAppScale();
      // Start from what is on screen: past the window cap the stored width is
      // wider than the rendered one, and a drag from it would sit dead until
      // it crossed back under the cap.
      const shown = navRootEl ? navRootEl.getBoundingClientRect().width / resizeScale : width;
      resizeStartWidth = Math.min(width, Math.round(shown));
    }}
    onresize={(dx) => {
      width = clampNavSidebarWidth(resizeStartWidth + dx / resizeScale);
    }}
    onresizeend={() => {
      resizeStartWidth = width;
      setNavSidebarWidth(width);
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

{#snippet connectionsSection()}
  <!-- Connections, pinned to the sidebar's foot (T3's "Settled" pattern): a
       header that folds the list away, quiet rows with the engine and when you
       last used it, "Show more", and a hover card with the details. -->
  {#if wsConnections.length}
    <div class="flex max-h-[45%] shrink-0 flex-col pt-2">
      <button
        type="button"
        class="flex h-9 shrink-0 items-center gap-2 px-4 text-ui-sm text-foreground/80 hover:text-foreground"
        aria-expanded={connsOpen}
        onclick={() => {
          connsOpen = !connsOpen
          try { localStorage.setItem('stroke:sidebar-conns-open', connsOpen ? '1' : '0') } catch {}
          // Opening the list puts the cursor in its search, ready to type.
          if (connsOpen) tick().then(() => connsSearchEl?.focus())
        }}
      >
        <span class="flex-1 text-left">Connections</span>
        <span class="font-mono text-ui-2xs tabular-nums">{wsConnections.length}</span>
        <Icon name={connsOpen ? 'chevron-down' : 'chevron-right'} class="size-3 shrink-0" />
      </button>
      {#if connsOpen}
        {#if wsConnections.length > 5}
          <!-- Frameless search, T3-style: focus fills it, × clears it, Esc too. -->
          <div class="mx-2 mb-1 flex h-8 shrink-0 items-center gap-2 rounded-md px-2 focus-within:bg-sidebar-accent/60">
            <Icon name="search" class="size-3.5 shrink-0 text-muted-foreground" />
            <input
              bind:this={connsSearchEl}
              bind:value={connsQuery}
              type="text"
              placeholder="Search connections"
              aria-label="Search connections"
              class="no-field-frame h-full min-w-0 flex-1 bg-transparent text-ui-sm text-foreground outline-none placeholder:text-muted-foreground"
              onkeydown={(e) => {
                if (e.key === 'Escape') { connsQuery = ''; e.stopPropagation() }
                else if (e.key === 'Enter' && connsShown.length === 1 && connsShown[0].id !== activeConnectionId) onswitchconnection(connsShown[0])
              }}
            />
            {#if connsQuery}
              <button type="button" class="hit-area inline-flex size-5 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:text-foreground" aria-label="Clear search" onclick={() => (connsQuery = '')}>
                <Icon name="x" class="size-3 shrink-0" />
              </button>
            {/if}
          </div>
        {/if}
        <div class="app-scroll min-h-0 overflow-y-auto px-2 pb-2">
          {#each connsShown as c (c.id)}
            {@const current = c.id === activeConnectionId}
            <button
              type="button"
              class={cn(
                'flex h-8 w-full min-w-0 items-center gap-2.5 rounded-md px-2 text-left text-ui-sm',
                current ? 'bg-sidebar-accent text-foreground' : 'text-muted-foreground hover:bg-sidebar-accent/60 hover:text-foreground',
              )}
              onclick={() => { connHover = null; if (!current) onswitchconnection(c) }}
              onpointerenter={(e) => showConnCard(c, e.currentTarget)}
              onpointerleave={hideConnCard}
            >
              <DbIcon id={c.type} class="size-4 shrink-0" />
              <span class="min-w-0 flex-1 truncate">{c.name || c.database || c.host || c.filePath || 'Connection'}</span>
              {#if c.pinned}<Icon name="pin" class="size-3 shrink-0 opacity-60" />{/if}
              <span class="shrink-0 font-mono text-ui-2xs tabular-nums text-muted-foreground/80">{ago(c.lastConnectedAt)}</span>
            </button>
          {/each}
          {#if connsQuery.trim() && !connsShown.length}
            <p class="px-2 py-2 text-ui-xs text-muted-foreground">No connection matches “{connsQuery.trim()}”.</p>
          {/if}
          {#if !connsQuery.trim() && wsConnections.length > CONNS_LIMIT}
            <button
              type="button"
              class="flex h-8 w-full items-center gap-2.5 rounded-md px-2 text-ui-sm text-muted-foreground hover:bg-sidebar-accent/60 hover:text-foreground"
              onclick={() => (connsShowAll = !connsShowAll)}
            >
              <Icon name={connsShowAll ? 'chevron-up' : 'chevron-down'} class="size-4 shrink-0" />
              {connsShowAll ? 'Show less' : `Show ${wsConnections.length - CONNS_LIMIT} more`}
            </button>
          {/if}
        </div>
      {/if}
    </div>
  {/if}

  {#if connHover}
    <div
      class="pointer-events-none fixed z-[90] min-w-52 rounded-[10px] border border-border/60 bg-popover px-3 py-2.5 text-ui-xs elevate-2-rim"
      style:left="{connHover.x}px"
      style:top="{connHover.y}px"
      role="tooltip"
    >
      <p class="mb-1.5 font-medium text-foreground">{connHover.c.name || connHover.c.database || 'Connection'}</p>
      <div class="flex flex-col gap-1 text-muted-foreground">
        <span class="flex items-center gap-2"><DbIcon id={connHover.c.type} class="size-3.5 shrink-0" /> {connHover.c.type}</span>
        {#if connHover.c.host || connHover.c.filePath}
          <span class="flex items-center gap-2"><Icon name="server" class="size-3.5 shrink-0" /> <span class="truncate">{connHover.c.host ?? connHover.c.filePath}</span></span>
        {/if}
        {#if connHover.c.database}
          <span class="flex items-center gap-2"><Icon name="database" class="size-3.5 shrink-0" /> {connHover.c.database}</span>
        {/if}
        <span class="flex items-center gap-2"><Icon name="clock" class="size-3.5 shrink-0" /> {connHover.c.lastConnectedAt ? `Last used ${ago(connHover.c.lastConnectedAt)} ago` : 'Never connected'}</span>
      </div>
    </div>
  {/if}
{/snippet}

{#snippet hl(/** @type {string} */ text)}
  <!-- The filter's match inside a name, marked the way the tree marks it. -->
  {@const at = lf ? String(text).toLowerCase().indexOf(lf) : -1}
  {#if at >= 0}{String(text).slice(0, at)}<mark class="rounded-[2px] bg-primary/25 text-inherit">{String(text).slice(at, at + lf.length)}</mark>{String(text).slice(at + lf.length)}{:else}{text}{/if}
{/snippet}
