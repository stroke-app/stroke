<script>
  import DbIcon from './DbIcon.svelte'
  import { colorOf } from '$lib/stores/workspaces.js'
  import { comboTitle } from '$lib/shortcuts.js'
  import { withShortcut } from '$lib/stores/keybindings.js'
  import { matchesCommand } from '$lib/command-hotkey.js'
  import Icon         from './Icon.svelte'
  import { IS_MAC }   from '$lib/shortcuts.js'
  import { tick, onMount } from 'svelte'
  import { cn }       from '$lib/utils.js'
  import { toast }    from '$lib/components/ui/sonner/toast.svelte.js'
  import { readOnlyMode, READ_ONLY_HINT } from '$lib/stores/read-only.js'
  import { aiProfiles, activeProfileId, setActiveProfile } from '$lib/stores/ai-settings.js'
  import { toggleLightDark, isCurrentThemeDark, appVimMode, appLiveMode, updateSettings } from '$lib/stores/settings.js'
  import {
    EASTER_EGG_THEME_ID,
    EASTER_EGG_CLICKS,
    easterEggFound,
    markEasterEggFound,
  } from '$lib/themes/registry.js'
  import { vimSubMode, VIM_MODE_LABEL } from '$lib/vim/vim.js'
  import { listDatabases, canSwitchDatabase, currentDatabaseKey, isCurrentDatabase } from '$lib/databases.js'
  import { engineFamily } from '$lib/stores/connections.js'
  import { oracleContainerName } from '$lib/api.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import AppearanceMenu from './AppearanceMenu.svelte'
  import CreateDatabaseDialog from './CreateDatabaseDialog.svelte'
  import SearchableMenu from './SearchableMenu.svelte'
  import BrandIcon from './BrandIcon.svelte'
  import { hasBrand } from '$lib/brand-icons.js'

  // App version - shown in the status bar; resolved from the Tauri app metadata.
  let appVersion = $state('')
  onMount(async () => {
    try { const { getVersion } = await import('@tauri-apps/api/app'); appVersion = await getVersion() }
    catch { appVersion = '' }
  })

  let {
    /** @type {import('$lib/stores/connections.js').SavedConnection | null} */
    connection = null,
    connectionLost = false,
    /** @type {import('$lib/stores/connections.js').SavedConnection[]} */
    savedConnections = [],
    activeConnectionId = '',
    mcpRunning = false,
    /** Round trip of the last database ping in ms; null before the first. */
    latencyMs = /** @type {number | null} */ (null),
    /** No network at all on this computer. */
    offline = false,
    /** Check the connection now (the connectivity chip's click). */
    onping = /** @type {() => void} */ (() => {}),
    hasUpdate = false,
    /** The update is downloaded and waiting: the badge offers the restart. */
    updateReady = false,
    onopenmcp = /** @type {() => void} */ (() => {}),
    onconnect = /** @type {() => void} */ (() => {}),
    onswitchtodb = /** @type {(db: string) => void} */ ((_db) => {}),
    onswitchd1database = /** @type {(db: { databaseId: string, name: string }) => void} */ ((_db) => {}),
    onswitchproviderdb = /** @type {(db: { provider: string, dbRef: string, name: string }) => void} */ ((_db) => {}),
    onswitchconnection = /** @type {(conn: import('$lib/stores/connections.js').SavedConnection) => void} */ ((_c) => {}),
    oncheckupdate = /** @type {() => void} */ (() => {}),
    onopenmodelsettings = /** @type {() => void} */ (() => {}),
    aiMode = false,
    /** The sidebar's drawn width in px (0 when hidden): the bar's left part sits under it. */
    sidebarWidth = 0,
    onopenaimode = /** @type {() => void} */ (() => {}),
    onopensettings = /** @type {() => void} */ (() => {}),
    /** Opens the license page from the account menu. */
    onopenlicense = /** @type {() => void} */ (() => {}),
    onopencommand = /** @type {() => void} */ (() => {}),
    onopenpages = /** @type {() => void} */ (() => {}),
    ondisconnect = /** @type {() => void} */ (() => {}),
    pendingEditCount = 0,
    /** True while staged changes are being written - shows a spinner on Apply. */
    applying = false,
    onapplyedits = /** @type {() => void} */ (() => {}),
    onresetedits = /** @type {() => void} */ (() => {}),
    /** Put the staged changes on the clipboard as SQL, leaving them staged. */
    oncopyeditssql = /** @type {() => void} */ (() => {}),
    showTableNav = false,
    onscrolltabletop = /** @type {() => void} */ (() => {}),
    onscrolltablebottom = /** @type {() => void} */ (() => {}),
    /** Show the go-to-left / go-to-right controls only when the grid overflows. */
    canScrollTableHorizontally = false,
    onscrolltableleft = /** @type {() => void} */ (() => {}),
    onscrolltableright = /** @type {() => void} */ (() => {}),
    /** Duration of the last data fetch (table load / refresh / query), in ms. */
    queryMs = 0,
    /** Rows currently check-selected in the grid. 0 hides the indicator. */
    selectedCount = 0,
    /** Live mode (auto-refresh active table) - Postgres/SQLite only. */
    live = false,
    liveSupported = false,
    ontogglelive = /** @type {() => void} */ (() => {}),
    oncreatedatabase = /** @type {(opts: import('./CreateDatabaseDialog.svelte').CreateDbOptions) => Promise<void>} */ (async () => {}),
    /** Global read-only toggle - prevents all writes across the whole session */
    readonly = $bindable(false),
    sidebarVisible = true,
    tabBarVisible = true,
    tableToolbarVisible = true,
    statusBarVisible = true,
    ontoggleSidebar = /** @type {() => void} */ (() => {}),
    ontoggletabbar = /** @type {() => void} */ (() => {}),
    ontoggletabletoolbar = /** @type {() => void} */ (() => {}),
    ontogglestatusbar = /** @type {() => void} */ (() => {}),
  } = $props()

  const activeProfile = $derived($aiProfiles.find((p) => p.id === $activeProfileId) ?? $aiProfiles[0])
  const modelName = $derived(activeProfile?.name ?? 'No model')

  let dbOpen = $state(false)
  let createDbOpen = $state(false)
  /** Unified database list - `key` is what identifies a db (name for SQL, uuid for D1), `label` is what we show. */
  let dbList = $state(/** @type {{ key: string, label: string }[]} */ ([]))
  let dbLoading = $state(false)
  let dbSearch = $state('')
  /** @type {HTMLInputElement | null} */
  let dbInputEl = $state(null)
  /** @type {HTMLDivElement | null} */
  let dbListEl = $state(null)
  /** Keyboard-highlighted row in the db list - driven from the filter input. */
  let dbHl = $state(0)

  // Reset the highlight whenever the visible list changes or the menu reopens.
  $effect(() => {
    dbSearch; dbOpen
    dbHl = 0
  })

  // Autofocus the filter input when the switcher opens (issue #3: "search box
  // is not active by default"). Primary path is onOpenAutoFocus on the menu
  // content, which pre-empts bits-ui's own focus management (it would focus
  // the content container after our focus() and undo it). This effect is the
  // fallback for open paths that skip that callback; the flag makes it fire
  // once per open instead of stealing focus back while navigating.
  let _dbFocusedThisOpen = false
  $effect(() => {
    if (!dbOpen) { _dbFocusedThisOpen = false; return }
    if (_dbFocusedThisOpen) return
    const el = dbInputEl
    if (!el) return
    _dbFocusedThisOpen = true
    tick().then(() => el?.focus())
  })

  /** Take over the menu's open-autofocus so the filter input starts focused.
   * @param {Event} e */
  function focusDbInputOnOpen(e) {
    e.preventDefault()
    _dbFocusedThisOpen = true
    tick().then(() => dbInputEl?.focus())
  }

  function scrollDbHlIntoView() {
    tick().then(() => {
      dbListEl?.querySelector('[data-hl]')?.scrollIntoView({ block: 'nearest' })
    })
  }

  /**
   * Keyboard driving for the db list while focus stays in the filter input:
   * arrows / Tab cycle the highlight, Enter switches, Escape closes. Handled
   * (and stopped) here so the menu's own typeahead/focus logic can't fight it.
   * @param {KeyboardEvent} e
   */
  function onDbInputKeydown(e) {
    if (e.key === 'Escape') { dbSearch = ''; dbOpen = false; return }
    const n = Math.min(dbFiltered.length, 200)
    if (n === 0) return
    if (e.key === 'ArrowDown' || (e.key === 'Tab' && !e.shiftKey)) {
      e.preventDefault(); e.stopPropagation()
      dbHl = (dbHl + 1) % n
      scrollDbHlIntoView()
    } else if (e.key === 'ArrowUp' || ((e.key === 'Tab' || e.code === 'Tab') && e.shiftKey)) {
      e.preventDefault(); e.stopPropagation()
      dbHl = (dbHl - 1 + n) % n
      scrollDbHlIntoView()
    } else if (e.key === 'Enter') {
      e.preventDefault(); e.stopPropagation()
      const db = dbFiltered[dbHl]
      if (db) switchDb(db)
    }
  }

  /** @param {EventTarget | null} t */
  function isEditableTarget(t) {
    const el = /** @type {HTMLElement | null} */ (t)
    if (!el || !(el instanceof HTMLElement)) return false
    return (
      el.tagName === 'INPUT' ||
      el.tagName === 'TEXTAREA' ||
      el.isContentEditable ||
      !!el.closest('.cm-editor')
    )
  }

  /**
   * Global shortcuts: ⌘D toggles the database switcher, ⌘⇧C the connection
   * switcher. Skipped while typing (inputs, textareas, code editors) so ⌘D keeps
   * its editor meaning there.
   * @param {KeyboardEvent} e
   */
  function onWindowKeydown(e) {
    if (!connection) return
    if (matchesCommand(e, 'database.switch')) {
      if (!canSwitchDb || isEditableTarget(e.target)) return
      e.preventDefault()
      connOpen = false
      if (dbOpen) {
        dbOpen = false
      } else {
        dbOpen = true
        void fetchDatabases()
      }
    } else if (matchesCommand(e, 'connection.switch')) {
      if (savedConnections.length === 0 || isEditableTarget(e.target)) return
      e.preventDefault()
      dbOpen = false
      connOpen = !connOpen
    }
  }

  const currentDb = $derived(
    connection?.type === 'libsql'
      ? (connection?.url ?? '').replace(/^(libsql|https?):\/\//, '').split('/')[0]
      : (connection?.database ?? connection?.filePath ?? '')
  )
  const isRedis = $derived(engineFamily(connection?.type) === 'redis')
  const isPostgres = $derived(!isRedis && (engineFamily(connection?.type) === 'postgres' || engineFamily(connection?.type) === 'mysql'))
  const isD1 = $derived(connection?.type === 'd1')
  /** Connection that originated from a provider sign-in (Neon/Supabase/…). Redis
   * connections may carry a stale `provider`/`db` field from an earlier edit, so
   * they're explicitly excluded here to avoid the Postgres-style switcher. */
  const isProvider = $derived(!isRedis && !!connection?.provider && engineFamily(connection?.type) !== 'mysql')
  /** Whether this connection supports switching databases in-place. Redis uses
   * numbered logical DBs (no in-place switch yet), so it shows a static label. */
  const canSwitchDb = $derived(canSwitchDatabase(connection))
  /** The numeric logical DB Redis actually connects to - matches api.js
   * normalizeRedis (`Number(config.db) || 0`), so a stale non-numeric `db`
   * ("postgres") correctly reads as 0 rather than being shown verbatim. */
  const redisDb = $derived(Number(connection?.db) || 0)
  /** Label shown in the trigger for the active db. Provider connections all use
   * the db name "postgres", so show the project name instead (from the connection
   * name, e.g. "Prisma · stroke-testing" → "stroke-testing"). */
  /** Oracle's "database" is the container the session landed in, which the
   *  saved config may not name: a blank service name goes to the listener's
   *  default, often the root. Asked once per connection. */
  let oracleContainer = $state('')
  $effect(() => {
    const id = connection?.type === 'oracle' && !connectionLost ? connection.id : null
    oracleContainer = ''
    if (!id) return
    let stale = false
    oracleContainerName()
      .then((name) => { if (!stale) oracleContainer = name })
      .catch(() => {})
    return () => { stale = true }
  })
  /** The root holds only Oracle's own schemas; user tables live in a PDB. */
  const oracleInRoot = $derived(oracleContainer === 'CDB$ROOT')

  const currentDbLabel = $derived(
    connection?.type === 'oracle' ? (oracleContainer || connection?.serviceName || connection?.sid || '')
    : isRedis ? `db ${redisDb}`
    : isProvider ? ((connection?.name?.split(' · ').pop()) || connection?.name || currentDb)
    : isD1 ? (connection?.database || connection?.name || '')
    : currentDb,
  )
  // Document and key-value stores (other than Redis, which has numbered dbs)
  // have no "current database" to show or switch; the chip only said "No database".
  const NO_DB_ENGINES = new Set(['dynamodb', 'couchdb', 'cassandra', 'firestore', 'valkey', 'mongodb'])
  const hideDbChip = $derived(NO_DB_ENGINES.has(connection?.type ?? '') && !currentDbLabel)
  /** Key of the active db, used to mark the current row. */
  const currentDbKey = $derived(currentDatabaseKey(connection))

  const dbFiltered = $derived(
    dbSearch.trim()
      ? dbList.filter((d) => d.label.toLowerCase().includes(dbSearch.toLowerCase()))
      : dbList,
  )

  async function fetchDatabases() {
    dbLoading = true
    try { dbList = await listDatabases(connection) }
    catch (e) { toast.error('Failed to list databases: ' + String(e)) }
    finally { dbLoading = false }
  }

  function switchDb(/** @type {import("$lib/databases.js").DatabaseEntry} */ db) {
    if (isCurrentDatabase(db, connection)) {
      dbOpen = false
      return
    }
    if (isProvider && connection?.provider && !db.onServer) {
      onswitchproviderdb({ provider: connection.provider, dbRef: db.key, name: db.label })
    } else if (db.key !== currentDbKey) {
      if (isD1) onswitchd1database({ databaseId: db.key, name: db.label })
      else onswitchtodb(db.label)
    }
    dbOpen = false
    dbSearch = ''
  }

  $effect(() => {
    connection
    dbList = []
  })

  const connType = $derived(
    connection?.type === 'sqlite' ? 'SQLite'
      : connection?.type === 'libsql' ? 'Turso'
      : connection?.type === 'mysql' ? 'MySQL'
      : connection?.type === 'mariadb' ? 'MariaDB'
      : connection?.type === 'cockroachdb' ? 'CockroachDB'
      : connection?.type === 'clickhouse' ? 'ClickHouse'
      : connection?.type === 'posthog' ? 'PostHog'
      : connection?.type === 'duckdb' ? 'DuckDB'
      : connection?.type === 'mssql' ? 'SQL Server'
      : connection?.type === 'oracle' ? 'Oracle'
      : connection?.type === 'd1' ? 'D1'
      : connection?.type === 'redis' ? 'Redis'
      : connection?.type === 'valkey' ? 'Valkey'
      : connection?.type === 'dynamodb' ? 'DynamoDB'
      : connection?.type === 'couchdb' ? 'CouchDB'
      : connection?.type === 'cassandra' ? 'Cassandra'
      : connection?.type === 'firestore' ? 'Firestore'
      : connection?.type === 'mongodb' ? 'MongoDB'
      : 'PostgreSQL',
  )
  const connLabel = $derived(connection?.name ?? connection?.host ?? '')
  let connOpen = $state(false)

  /** Searchable list for the connection switcher - keyword-matched on name/host/db/type. */
  const connItems = $derived(
    savedConnections.map((c) => ({
      value: c.id,
      label: c.name ?? c.host ?? c.filePath ?? 'Connection',
      keywords: [c.host, c.database, c.name, c.type].filter(Boolean),
    })),
  )

  /**
   * Subtitle for a connection row - the database (or host) shown under the name,
   * but only when it isn't already part of the title, so host/db-style names
   * like "1.2.3.4/mydb" don't repeat "mydb" on the line below.
   * @param {import('$lib/stores/connections.js').SavedConnection} c
   */
  function connSubtitle(c) {
    const title = c.name ?? c.host ?? c.filePath ?? 'Connection'
    const raw = c.database || c.host || ''
    return raw && raw !== title && !title.includes(raw) ? raw : ''
  }

  /** @param {import('$lib/stores/connections.js').SavedConnection} c */
  function connIcon(c) {
    if (c.type === 'sqlite') return 'hard-drive'
    if (c.type === 'd1') return 'cloud'
    if (c.type === 'libsql') return 'wifi'
    return 'database'
  }

  /** Shared icon-only button classes */
  // ── Easter egg ────────────────────────────────────────────────────────────
  // Seven clicks on the version number. The counter resets after a second of
  // stillness so a double-click while reading the number never creeps toward it,
  // and nothing on screen hints at the count - a progress indicator would turn a
  // thing you stumble into a thing you grind out.
  let eggClicks = 0
  /** @type {ReturnType<typeof setTimeout> | null} */
  let eggTimer = null
  $effect(() => () => { if (eggTimer) clearTimeout(eggTimer) })
  function bumpEgg() {
    if (eggTimer) clearTimeout(eggTimer)
    eggTimer = setTimeout(() => { eggClicks = 0; eggTimer = null }, 1000)
    eggClicks += 1
    if (eggClicks < EASTER_EGG_CLICKS) return
    eggClicks = 0
    const firstTime = !easterEggFound()
    markEasterEggFound()
    updateSettings({ theme: EASTER_EGG_THEME_ID })
    toast.success(firstTime ? 'Hotdog Stand unlocked' : 'Hotdog Stand', {
      description: firstTime
        ? "Windows 3.1's worst idea, now yours. It stays in Appearance - pick anything else to escape."
        : 'Back for more.',
    })
  }

  const iconBtn = 'inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-sidebar-accent/60 hover:text-foreground'
  /** Shared label+icon button */
  const labelBtn = 'flex h-7 items-center gap-2 rounded-md px-2.5 text-ui-xs hover:bg-sidebar-accent/60 hover:text-foreground data-[state=open]:bg-sidebar-accent/60 data-[state=open]:text-foreground'

  let aiModelMenuOpen = $state(false)

  /**
   * Double-clicking the connection pill jumps straight to the connection
   * manager. A single click only opens the switcher menu, so reaching "Manage
   * connections…" otherwise costs an extra hop.
   * @param {MouseEvent} e
   */
  function openConnectionManager(e) {
    e.preventDefault()
    e.stopPropagation()
    connOpen = false
    // The menu returns focus to its trigger as it closes; opening the dialog in
    // the same frame would lose that race and leave the modal unfocused.
    tick().then(onconnect)
  }

  /** "132ms" under a second, "1.24s" above - always tabular so it never jitters. */
  const queryMsLabel = $derived(
    queryMs >= 1000 ? `${(queryMs / 1000).toFixed(2)}s` : `${Math.round(queryMs)}ms`,
  )
</script>

<svelte:window onkeydown={onWindowKeydown} />

<!-- Vertical separator -->
{#snippet sep()}
  <span class="w-1.5 shrink-0" aria-hidden="true"></span>
{/snippet}

{#snippet globalTools()}
    <!-- Theme toggle -->
    <button
      type="button"
      class={cn(iconBtn, '@max-[45rem]/sb:hidden')}
      title={withShortcut(`Switch to ${$isCurrentThemeDark ? 'light' : 'dark'}`, 'theme.next')}
      aria-label={`Switch to ${$isCurrentThemeDark ? 'light' : 'dark'} theme`}
      onclick={() => toggleLightDark()}
    >
      {#if $isCurrentThemeDark}
        <Icon name="sun" class="size-3.5 shrink-0" />
      {:else}
        <Icon name="moon" class="size-3.5 shrink-0" />
      {/if}
    </button>

    <!-- Appearance -->
    <AppearanceMenu
      {sidebarVisible}
      {tabBarVisible}
      {tableToolbarVisible}
      {statusBarVisible}
      {ontoggleSidebar}
      {ontoggletabbar}
      {ontoggletabletoolbar}
      {ontogglestatusbar}
    />

    <!-- Settings -->
    <button type="button" class={iconBtn} onclick={onopensettings} title={withShortcut('Settings', 'settings.open')} aria-label="Settings">
      <Icon name="settings" class="size-3.5 shrink-0" />
    </button>

{/snippet}

{#snippet sidebarToggle()}
    <button
      type="button"
      class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground outline-none transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring"
      aria-pressed={sidebarVisible}
      title={withShortcut(`${sidebarVisible ? 'Hide' : 'Show'} sidebar`, 'view.sidebar')}
      aria-label={sidebarVisible ? 'Hide sidebar' : 'Show sidebar'}
      onclick={ontoggleSidebar}
    >
      <Icon name={sidebarVisible ? 'panel-left-close' : 'panel-left-open'} class="size-3.5 shrink-0" />
    </button>
{/snippet}

<div
  class={cn('@container/sb flex h-8 shrink-0 items-center gap-0.5 bg-panel text-ui-xs text-muted-foreground select-none', 'px-2')}
  data-studio-region="statusbar"
>
  <!-- ── Left group ──────────────────────────────────────────────────── -->
  <div class="flex min-w-0 flex-1 items-center gap-0.5 overflow-hidden">
    <!-- The sidebar toggle, first thing on the bar and hard against the left
         edge - the same corner the sidebar itself occupies, so the control sits
         on the thing it controls. It used to be in the title bar, beside the
         back/forward arrows that are now gone. -->

    {#if connection}

      <!-- Connection switcher -->
      {#snippet connTriggerInner(/** @type {boolean} */ showChevron)}
        <span class="relative flex shrink-0">
          <DbIcon id={connection?.type ?? ''} class="size-3.5 shrink-0" />
          <span class={cn('absolute -right-0.5 -bottom-0.5 size-1.5 rounded-full ring-2 ring-background', connectionLost ? 'bg-destructive' : 'bg-success')} aria-hidden="true"></span>
        </span>
        <span class={cn('max-w-[12rem] truncate', connectionLost ? 'text-destructive' : 'text-foreground/80')} title={connType}>{connLabel || connType}</span>
        {#if connection?.environment}
          <span class="size-1.5 shrink-0 rounded-full bg-muted-foreground/35" title={connection.environment}></span>
        {/if}

      {/snippet}

      {#if savedConnections.length > 1}
        <SearchableMenu
          bind:open={connOpen}
          side="top"
          align="start"
          contentClass="w-72"
          placeholder="Search connections…"
          items={connItems}
          onselect={(it) => { const c = savedConnections.find((x) => x.id === it.value); if (c && c.id !== activeConnectionId) onswitchconnection(c) }}
        >
          {#snippet trigger(props)}
            <button
              {...props}
              type="button"
              class={cn(labelBtn, 'text-muted-foreground')}
              title={`${withShortcut('Switch connection', 'connection.switch')} · double-click to manage`}
              ondblclick={openConnectionManager}
            >
              {@render connTriggerInner(true)}
            </button>
          {/snippet}
          {#snippet item(it)}
            {@const conn = savedConnections.find((c) => c.id === it.value)}
            {@const isCurrent = conn?.id === activeConnectionId}
            {@const title = conn?.name ?? conn?.host ?? conn?.filePath ?? 'Connection'}
            {@const subtitle = conn ? connSubtitle(conn) : ''}
            <span class={cn(
              'flex size-5 shrink-0 items-center justify-center rounded-md',
              isCurrent ? 'bg-success/12 text-success' : 'bg-muted/50 text-muted-foreground',
            )}>
              {#if conn?.provider && hasBrand(conn.provider)}
                <BrandIcon name={conn.provider} class="size-3.5" />
              {:else if conn}
                <Icon name={connIcon(conn)} class="size-3.5 shrink-0" />
              {/if}
            </span>
            <span
              class={cn('min-w-0 flex-1 truncate', isCurrent ? 'font-semibold text-foreground' : 'font-medium text-foreground/90')}
              title={subtitle ? `${title} · ${subtitle}` : title}
            >{title}</span>
            {#if conn?.environment}
              <span class="size-1.5 shrink-0 rounded-full bg-muted-foreground/30" title={conn.environment}></span>
            {/if}
            {#if isCurrent}<Icon name="check" class="size-3.5 shrink-0 text-success" />{/if}
          {/snippet}
          {#snippet footer()}
            <div class="border-t border-border/50 p-1">
              <button
                type="button"
                class="flex min-h-7 w-full items-center gap-2 rounded-sm px-2 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                onclick={() => { connOpen = false; onconnect() }}
              >
                <Icon name="wifi-off" class="size-3.5 shrink-0 text-muted-foreground" />
                Manage connections…
              </button>
              <button
                type="button"
                class="flex min-h-7 w-full items-center gap-2 rounded-sm px-2 text-ui-xs text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                onclick={() => { connOpen = false; ondisconnect() }}
              >
                <Icon name="unplug" class="size-3.5 shrink-0" />
                Disconnect
              </button>
            </div>
          {/snippet}
        </SearchableMenu>
      {:else}
        <button
          type="button"
          class={cn(labelBtn, 'text-muted-foreground')}
          title={withShortcut('Manage connections', 'connection.switch')}
          onclick={onconnect}
          ondblclick={openConnectionManager}
        >
          {@render connTriggerInner(false)}
        </button>
        {#if connection}
          <!-- Beside the connection it ends, not beside Settings where a near
               miss on one meant the other. -->
          <button
            type="button"
            class="{iconBtn} hover:text-destructive!"
            onclick={ondisconnect}
            title="Disconnect"
            aria-label="Disconnect"
          >
            <Icon name="unplug" class="size-3.5 shrink-0" />
          </button>
        {/if}
      {/if}

      {#if !hideDbChip}
      {@render sep()}

      <!-- Database switcher + create button -->
      <div class="flex items-center">
        <DropdownMenu.Root
          bind:open={dbOpen}
          onOpenChange={(o) => { if (o) void fetchDatabases(); if (!o) dbSearch = '' }}
        >
          <DropdownMenu.Trigger
            class={cn(labelBtn, 'text-muted-foreground', !canSwitchDb && 'cursor-default hover:bg-transparent hover:text-foreground')}
            disabled={!canSwitchDb}
            title={oracleInRoot
              ? 'Connected to the root container, which holds only Oracle\'s own schemas. Set the service name to a pluggable database such as FREEPDB1.'
              : canSwitchDb ? withShortcut('Switch database', 'database.switch') : currentDbLabel}
          >
            {#if connection?.type === 'sqlite'}
              <Icon name="hard-drive" class="size-3.5 shrink-0" />
            {:else if isD1}
              <Icon name="cloud" class="size-3.5 shrink-0" />
            {:else if oracleInRoot}
              <Icon name="alert-triangle" class="size-3.5 shrink-0 text-warning" />
            {:else}
              <Icon name="database" class="size-3.5 shrink-0" />
            {/if}
            <span class="max-w-[8rem] truncate tabular-nums">{currentDbLabel || 'No database'}</span>
          </DropdownMenu.Trigger>

          <DropdownMenu.Content
            side="top"
            align="start"
            class="w-72 overflow-hidden p-0"
            onOpenAutoFocus={focusDbInputOnOpen}
          >
            <!-- Always mounted (not gated on list size) so the search box is
                 active the moment the switcher opens and arrows/Tab/Enter work
                 for any number of databases. -->
            <!-- The palette's header: a borderless search row over a hairline,
                 not a framed field inside a framed panel. -->
            <div data-slot="menu-search" class="flex h-9 items-center gap-2 px-3">
              <Icon name="search" class="size-3.5 shrink-0 text-muted-foreground" />
              <input
                bind:this={dbInputEl}
                type="text"
                aria-label="Filter databases"
                placeholder={isD1 ? 'Filter D1 databases…' : 'Filter databases…'}
                class="no-focus-ring h-full min-w-0 flex-1 bg-transparent text-ui-xs text-foreground outline-none placeholder:text-muted-foreground"
                bind:value={dbSearch}
                onkeydown={onDbInputKeydown}
              />
            </div>

            <div bind:this={dbListEl} class="db-list-scroll max-h-[264px] overflow-y-auto p-1 [contain:layout_paint]">
              {#if dbLoading}
                <div class="loading-reveal flex items-center justify-center gap-2 py-4 text-muted-foreground">
                  <Icon name="refresh-cw" class="size-3 shrink-0 animate-spin" />
                  <span class="text-ui-2xs">Loading…</span>
                </div>
              {:else if dbFiltered.length === 0}
                <div class="py-3 text-center text-ui-2xs text-muted-foreground">
                  {dbSearch ? 'No match' : 'No databases found'}
                </div>
              {:else}
                <!-- Plain buttons (not DropdownMenu.Item): highlight is driven from
                     the filter input's keyboard handler, and menu-item roving focus
                     would fight it. Hover moves the highlight so both stay in sync. -->
                {#each dbFiltered.slice(0, 200) as db, i (db.key)}
                  {@const isCurrent = isCurrentDatabase(db, connection)}
                  <button
                    type="button"
                    data-hl={dbHl === i ? '' : undefined}
                    aria-current={isCurrent ? 'true' : undefined}
                    class={cn(
                      'flex h-7 w-full cursor-default items-center gap-2 rounded-sm px-2 text-left font-mono text-ui-xs transition-colors',
                      isCurrent ? 'text-foreground' : 'text-foreground/80',
                      dbHl === i && 'bg-accent text-foreground',
                    )}
                    onclick={() => switchDb(db)}
                    onpointerenter={() => (dbHl = i)}
                  >
                    {#if isD1}
                      <Icon name="cloud" class={cn('size-3.5 shrink-0', isCurrent ? 'text-primary' : 'text-muted-foreground')} />
                    {:else}
                      <Icon name="database" class={cn('size-3.5 shrink-0', isCurrent ? 'text-primary' : 'text-muted-foreground')} />
                    {/if}
                    <span class="min-w-0 flex-1 truncate">{db.label}</span>
                    {#if isCurrent}
                      <span class="ml-auto flex shrink-0 items-center gap-1 font-sans text-ui-3xs text-muted-foreground">
                        current <Icon name="check" class="size-3.5 shrink-0 text-foreground" />
                      </span>
                    {/if}
                  </button>
                {/each}
              {/if}
            </div>

            {#if canSwitchDb}
              <div class="flex h-9 items-center justify-between gap-2 border-t border-border/50 pr-1 pl-3">
                <span class="text-ui-2xs tabular-nums text-muted-foreground">
                  {dbList.length} database{dbList.length === 1 ? '' : 's'}
                </span>
                <div class="flex items-center gap-0.5">
                  <button
                    type="button"
                    class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                    onclick={fetchDatabases}
                    title="Refresh"
                    aria-label="Refresh databases"
                  >
                    <Icon name="refresh-cw" class={cn('size-3.5 shrink-0', dbLoading && 'animate-spin')} />
                  </button>
                  {#if isPostgres}
                    <button
                      type="button"
                      class="inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
                      onclick={() => { dbOpen = false; createDbOpen = true }}
                      disabled={$readOnlyMode}
                      title={$readOnlyMode ? READ_ONLY_HINT : 'Create database'}
                      aria-label="Create database"
                    >
                      <Icon name="plus" class="size-3.5 shrink-0" />
                    </button>
                  {/if}
                </div>
              </div>
            {/if}
          </DropdownMenu.Content>
        </DropdownMenu.Root>

      </div>
      {/if}

      <!-- Read-only is a fact about this connection, so it sits with it and says
           so in words; it used to be an orange lock among the tool icons. -->
      {#if connection}
        {#if readonly}
          <button
            type="button"
            class="flex h-7 shrink-0 items-center gap-1.5 rounded-md bg-warning/12 px-2.5 text-ui-xs font-medium text-warning transition-colors hover:bg-warning/20"
            title="Read-only: changes are blocked. Click to allow editing."
            aria-pressed="true"
            onclick={() => (readonly = false)}
          >
            <Icon name="lock" class="size-3.5 shrink-0" />
            Read-only
          </button>
        {:else}
          <button
            type="button"
            class="{iconBtn} ml-0.5"
            title="Editable. Click to make this connection read-only."
            aria-label="Make read-only"
            aria-pressed="false"
            onclick={() => (readonly = true)}
          >
            <Icon name="lock-open" class="size-3.5 shrink-0" />
          </button>
        {/if}
      {/if}

      <!-- Live mode toggle (experimental, only shown when enabled in Settings) -->
      {#if showTableNav && liveSupported && $appLiveMode}
        {@render sep()}
        <button
          type="button"
          class={cn(
            'flex h-7 items-center gap-1.5 rounded-md px-2 transition-[background-color,color] duration-150',
            live
              ? 'font-medium text-success bg-success/10 hover:bg-success/16'
              : 'text-muted-foreground hover:bg-muted/50 hover:text-foreground',
          )}
          onclick={ontogglelive}
          aria-pressed={live}
          title={live ? 'Live: on, auto-refreshes when this table changes' : 'Live: off, click to auto-refresh on changes'}
        >
          {#if live}
            <span class="size-1.5 shrink-0 rounded-full bg-success motion-safe:animate-pulse"></span>
            <span>Live</span>
          {:else}
            <Icon name="radio" class="size-3 shrink-0" />
            <span>Live</span>
          {/if}
        </button>
      {/if}

      <!-- Last fetch timing, updates on every table load / refresh / query -->
      {#if showTableNav || queryMs > 0}
        <!-- Below the connection and database in priority: it goes before they
             would be clipped at high zoom. -->
        <div class="contents @max-[40rem]/sb:hidden">
        {@render sep()}
        <!-- A fixed slot on table tabs: it held its place only while a time was
             known, so it vanished and came back around every table open. -->
        <span
          class="min-w-[6ch] shrink-0 px-1.5 text-ui-xs tabular-nums text-muted-foreground"
          title={queryMs > 0 ? `Last data fetch took ${queryMs.toLocaleString('en-US')}ms` : undefined}
        >{queryMs > 0 ? queryMsLabel : ''}</span>
        </div>
      {/if}

      <!-- Selected row count, sits to the right of the fetch timing -->
      {#if selectedCount > 0}
        {@render sep()}
        <span
          class="shrink-0 px-1.5 text-ui-xs tabular-nums text-foreground/80"
          title="{selectedCount.toLocaleString('en-US')} row{selectedCount === 1 ? '' : 's'} selected"
        >{selectedCount.toLocaleString('en-US')} selected</span>
      {/if}

    {:else}
      <!-- Not connected -->
      <button
        type="button"
        class={cn(labelBtn, 'text-muted-foreground')}
        onclick={onconnect}
        title="No connection, click to connect"
      >
        <Icon name="wifi-off" class="size-3.5 shrink-0" />
        <span>Not connected</span>
      </button>
    {/if}
  </div>

  <!-- ── Right group ─────────────────────────────────────────────────── -->
  <div class="flex shrink-0 items-center gap-0.5">

    <!-- Vim mode indicator (experimental) -->
    {#if $appVimMode}
      <span
        class={cn(
          'inline-flex h-6 items-center rounded-md px-1.5 font-mono text-ui-3xs font-semibold uppercase tracking-[0.06em]',
          $vimSubMode === 'insert' && 'bg-success/15 text-success',
          $vimSubMode === 'visual' && 'bg-warning/15 text-warning',
          $vimSubMode === 'command' && 'bg-primary/15 text-primary',
          $vimSubMode === 'normal' && 'bg-muted/40 text-muted-foreground',
        )}
        title="Vim mode · {VIM_MODE_LABEL[$vimSubMode]} (experimental)"
      >{VIM_MODE_LABEL[$vimSubMode]}</span>
    {/if}


    <!-- Pending edits.
         Reset then Apply, in that order and tight against each other: they are
         one decision about one set of changes, and as two loose buttons spaced
         like unrelated status items they read as two. Apply carries the count
         and the split caret, which is where the same action's other form (copy
         the SQL instead of running it) belongs - not as a third button. -->
    {#if pendingEditCount > 0}
      <div class="flex items-center gap-1">
        <button
          type="button"
          class="inline-flex h-6 items-center gap-1 rounded-md px-1.5 text-ui-2xs text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground disabled:opacity-40"
          onclick={onresetedits}
          disabled={applying}
          title={withShortcut(`Discard ${pendingEditCount} unsaved change${pendingEditCount === 1 ? '' : 's'}`, 'table.discard')}
        >
          <Icon name="undo-2" class="size-3 shrink-0" />
          <span class="@max-[48.75rem]/sb:hidden">Reset</span>
        </button>
        <div class="inline-flex h-6 items-stretch overflow-hidden rounded-md bg-primary text-primary-foreground">
          <button
            type="button"
            class="inline-flex items-center gap-1 pl-2 pr-1.5 text-ui-2xs font-medium transition-opacity hover:opacity-85 disabled:opacity-60"
            onclick={onapplyedits}
            disabled={applying}
            title={withShortcut(`Apply ${pendingEditCount} unsaved change${pendingEditCount === 1 ? '' : 's'}`, 'table.apply')}
          >
            {#if applying}
              <span class="size-3 shrink-0 animate-spin rounded-full border border-current/40 border-t-current"></span>
              Applying…
            {:else}
              <Icon name="check" class="size-3 shrink-0" />
              Apply
              <span class="tabular-nums opacity-80">{pendingEditCount}</span>
            {/if}
          </button>
          <DropdownMenu.Root>
            <DropdownMenu.Trigger
              class="inline-flex w-5 items-center justify-center border-l border-primary-foreground/25 transition-opacity hover:opacity-85 disabled:opacity-60"
              disabled={applying}
              aria-label="More apply options"
              title="More apply options"
            >
              <Icon name="chevron-down" class="size-3 shrink-0" />
            </DropdownMenu.Trigger>
            <DropdownMenu.Content align="end" side="top" class="min-w-52">
              <DropdownMenu.Item onSelect={onapplyedits}>
                <Icon name="check" class="size-3.5 shrink-0" />
                Apply changes
                <DropdownMenu.Shortcut command="table.apply" />
              </DropdownMenu.Item>
              <DropdownMenu.Item onSelect={oncopyeditssql}>
                <Icon name="clipboard-copy" class="size-3.5 shrink-0" />
                Copy to SQL
                <DropdownMenu.Shortcut command="table.copySql" />
              </DropdownMenu.Item>
              <DropdownMenu.Separator />
              <DropdownMenu.Item onSelect={onresetedits} variant="destructive">
                <Icon name="undo-2" class="size-3.5 shrink-0" />
                Discard changes
                <DropdownMenu.Shortcut command="table.discard" />
              </DropdownMenu.Item>
            </DropdownMenu.Content>
          </DropdownMenu.Root>
        </div>
      </div>
      {@render sep()}
    {/if}


    {#if !sidebarWidth}{@render globalTools()}{@render sep()}{/if}

    <!-- Update badge -->
    {#if hasUpdate}
      <button
        type="button"
        class="flex h-7 items-center gap-2 rounded-md px-2.5 text-ui-xs font-medium text-warning transition-colors hover:bg-muted/50 hover:text-warning"
        onclick={oncheckupdate}
        title={updateReady ? 'Update downloaded. It installs when you quit, or restart now.' : 'Update available'}
      >
        <Icon name={updateReady ? 'refresh-cw' : 'arrow-up-circle'} class="size-3.5 shrink-0" />
        <span class="@max-[52.5rem]/sb:hidden">{updateReady ? 'Restart to update' : 'Update'}</span>
      </button>
      {@render sep()}
    {/if}

    <!-- Connectivity: the database's round trip from the health ping, so a
         slow link is visible before a query is. The number is always shown;
         colour only adds to it. -->
    {#if connection}
      {@const tone = offline || connectionLost ? 'bad' : latencyMs == null ? 'idle' : latencyMs < 150 ? 'good' : latencyMs < 400 ? 'fair' : 'bad'}
      <button
        type="button"
        class={cn(
          labelBtn,
          tone === 'bad' ? 'text-destructive' : tone === 'fair' ? 'text-warning' : 'text-muted-foreground',
        )}
        onclick={onping}
        title={offline
          ? 'This computer is offline. Stroke reconnects when the network is back.'
          : connectionLost
            ? 'The database is not answering. Stroke keeps trying in the background. Click to check now.'
            : latencyMs == null
              ? 'Checking the connection…'
              : `The database answers in ${latencyMs} ms${tone === 'fair' ? ' (slow)' : tone === 'bad' ? ' (very slow)' : ''}. Checked every 30 s; click to check now.`}
        aria-label={offline ? 'Offline' : connectionLost ? 'Database not responding' : latencyMs == null ? 'Checking connection' : `Database round trip ${latencyMs} milliseconds`}
      >
        <Icon name={offline || connectionLost ? 'wifi-off' : 'wifi'} class="size-3.5 shrink-0" />
        <span class="tabular-nums @max-[40rem]/sb:hidden">{offline ? 'Offline' : connectionLost ? 'No response' : latencyMs == null ? '…' : `${latencyMs} ms`}</span>
      </button>
    {/if}

    <!-- MCP status -->
    <button
      type="button"
      class={cn(
        labelBtn,
        mcpRunning ? 'text-muted-foreground' : 'text-muted-foreground',
      )}
      onclick={onopenmcp}
      title={mcpRunning ? 'MCP running, click to manage' : 'MCP stopped, click to manage'}
    >
      <span class={cn('size-1.5 shrink-0 rounded-full', mcpRunning ? 'bg-success' : 'bg-muted-foreground/30')}></span>
      <span class="@max-[40rem]/sb:hidden">MCP</span>
    </button>

    <!-- AI model picker -->
    <SearchableMenu
      bind:open={aiModelMenuOpen}
      side="top"
      align="end"
      contentClass="w-64"
      placeholder="Search models…"
      items={$aiProfiles.map((p) => ({ value: p.id, label: p.name, keywords: [p.model] }))}
      onselect={(it) => setActiveProfile(it.value)}
    >
      {#snippet trigger(props)}
        <button {...props} type="button" class={cn(labelBtn, 'text-muted-foreground')} title="Switch AI model">
          <Icon name="sparkles" class="size-3.5 shrink-0" />
          <span class="max-w-[10rem] truncate @max-[56.25rem]/sb:hidden">{modelName.replace(/^[^·]*·\s*/, '')}</span>
        </button>
      {/snippet}
      {#snippet item(it)}
        {@const profile = $aiProfiles.find((p) => p.id === it.value)}
        {#if profile && hasBrand(profile.provider)}
          <BrandIcon name={profile.provider} class="size-4 shrink-0 text-muted-foreground" />
        {:else}
          <Icon name="bot" class="size-4 shrink-0 text-muted-foreground" />
        {/if}
        <span class="min-w-0 flex-1 truncate text-ui-sm font-medium" title={profile?.model ?? it.label}>{profile?.name ?? it.label}</span>
        {#if $activeProfileId === it.value}<Icon name="check" class="size-3.5 shrink-0 text-primary" />{/if}
      {/snippet}
      {#snippet footer()}
        <div class="border-t border-border/50 p-1">
          <button
            type="button"
            class="flex min-h-7 w-full items-center gap-1.5 rounded-sm px-2 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            onclick={() => { aiModelMenuOpen = false; onopenmodelsettings() }}
          >
            <Icon name="settings-2" class="size-3.5 shrink-0 text-muted-foreground" />
            Manage models…
          </button>
        </div>
      {/snippet}
    </SearchableMenu>
  </div>
</div>

<CreateDatabaseDialog
  bind:open={createDbOpen}
  connType={connection?.type ?? 'postgres'}
  oncreate={async (opts) => {
    await oncreatedatabase(opts)
    dbList = []
  }}
/>
