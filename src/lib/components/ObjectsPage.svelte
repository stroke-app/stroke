<script>
  import { executeSql } from '$lib/api.js'
  import { cn } from '$lib/utils.js'
  import Boxes from '@lucide/svelte/icons/boxes'
  import Table2 from '@lucide/svelte/icons/table-2'
  import Eye from '@lucide/svelte/icons/eye'
  import SquareFunction from '@lucide/svelte/icons/square-function'
  import GitBranch from '@lucide/svelte/icons/git-branch'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import Search from '@lucide/svelte/icons/search'
  import X from '@lucide/svelte/icons/x'

  /**
   * @type {{
   *   active?: boolean,
   *   connectionType?: string | null,
   *   onopen?: (args: { schema: string, name: string }) => void,
   * }}
   */
  let {
    active = false,
    connectionType = null,
    onopen = () => {},
    /** Assigned here; the shell calls it for ⌘F. */
    focusSearch = $bindable(/** @type {() => void} */ (() => {})),
  } = $props()

  /** @type {HTMLInputElement | null} */
  let searchEl = $state(null)
  $effect(() => {
    focusSearch = () => { searchEl?.focus(); searchEl?.select() }
  })

  /** @typedef {'tables' | 'views' | 'functions' | 'triggers'} SubTab */

  const TABS = /** @type {{ id: SubTab, label: string, icon: any }[]} */ ([
    { id: 'tables', label: 'Tables', icon: Table2 },
    { id: 'views', label: 'Views', icon: Eye },
    { id: 'functions', label: 'Functions', icon: SquareFunction },
    { id: 'triggers', label: 'Triggers', icon: GitBranch },
  ])

  /** @type {SubTab} */
  let activeSub = $state('tables')

  // One reactive slot per sub-tab. `data === null` means "not loaded yet".
  let slots = $state(
    /** @type {Record<SubTab, { data: Record<string, unknown>[] | null, loading: boolean, error: string }>} */ ({
      tables: { data: null, loading: false, error: '' },
      views: { data: null, loading: false, error: '' },
      functions: { data: null, loading: false, error: '' },
      triggers: { data: null, loading: false, error: '' },
    }),
  )

  const engineLabel = $derived(
    /** @type {Record<string, string>} */ ({
      postgres: 'PostgreSQL', cockroachdb: 'CockroachDB', mysql: 'MySQL', mariadb: 'MariaDB',
      sqlite: 'SQLite', d1: 'Cloudflare D1', libsql: 'libSQL', clickhouse: 'ClickHouse',
      duckdb: 'DuckDB', mssql: 'SQL Server',
    })[connectionType ?? ''] ?? (connectionType ?? 'this engine'),
  )

  /** @param {{ columns?: {name:string}[], rows?: unknown[][] } | null} result */
  function toRecords(result) {
    if (!result?.columns || !result?.rows) return []
    const cols = result.columns.map((c) => c.name)
    return result.rows.map(
      (row) => /** @type {Record<string, unknown>} */ (Object.fromEntries(cols.map((c, i) => [c, row[i]]))),
    )
  }

  // ── Per-dialect query catalog ──────────────────────────────────────────────
  // Returns { sql, fallback? } when the sub-tab is supported for the engine, or
  // null when it is not (SQLite has no functions, etc.). Every query aliases its
  // columns to a canonical, dialect-neutral set so the renderer stays generic.
  /**
   * @param {SubTab} sub
   * @returns {{ sql: string, fallback?: string } | null}
   */
  function queryFor(sub) {
    const ct = connectionType ?? ''
    const pg = ct === 'postgres' || ct === 'cockroachdb'
    const my = ct === 'mysql' || ct === 'mariadb'
    const lite = ct === 'sqlite' || ct === 'd1' || ct === 'libsql'

    if (pg) {
      if (sub === 'tables')
        return {
          sql: `
            SELECT c.relname AS name, n.nspname AS schema,
                   CASE c.relkind WHEN 'r' THEN 'table' WHEN 'p' THEN 'partitioned' ELSE c.relkind::text END AS kind,
                   pg_catalog.pg_get_userbyid(c.relowner) AS owner,
                   COALESCE(st.n_live_tup, 0) AS estimated_row,
                   pg_catalog.pg_total_relation_size(c.oid) AS total_size,
                   pg_catalog.pg_relation_size(c.oid) AS data_size,
                   pg_catalog.pg_indexes_size(c.oid) AS index_size,
                   COALESCE(pg_catalog.obj_description(c.oid, 'pg_class'), '') AS comment
            FROM pg_catalog.pg_class c
            JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
            LEFT JOIN pg_catalog.pg_stat_user_tables st ON st.relid = c.oid
            WHERE c.relkind IN ('r', 'p')
              AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
            ORDER BY c.relname`,
        }
      if (sub === 'views')
        return {
          sql: `
            SELECT table_name AS name, table_schema AS schema, 'view' AS kind, view_definition AS definition
            FROM information_schema.views
            WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
            UNION ALL
            SELECT matviewname AS name, schemaname AS schema, 'materialized' AS kind, definition
            FROM pg_catalog.pg_matviews
            ORDER BY name`,
          fallback: `
            SELECT table_name AS name, table_schema AS schema, 'view' AS kind, view_definition AS definition
            FROM information_schema.views
            WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
            ORDER BY table_name`,
        }
      if (sub === 'functions')
        return {
          sql: `
            SELECT routine_name AS name, routine_schema AS schema, routine_type AS kind, data_type AS returns
            FROM information_schema.routines
            WHERE routine_schema NOT IN ('pg_catalog', 'information_schema')
            ORDER BY routine_name`,
        }
      // triggers
      return {
        sql: `
          SELECT trigger_name AS name, event_object_table AS "table", action_timing AS timing,
                 event_manipulation AS event, trigger_schema AS schema
          FROM information_schema.triggers
          WHERE trigger_schema NOT IN ('pg_catalog', 'information_schema')
          ORDER BY trigger_name`,
      }
    }

    if (my) {
      if (sub === 'tables')
        return {
          sql: `
            SELECT TABLE_NAME AS name, TABLE_SCHEMA AS \`schema\`, ENGINE AS engine,
                   TABLE_ROWS AS estimated_row, DATA_LENGTH AS data_size, INDEX_LENGTH AS index_size,
                   (COALESCE(DATA_LENGTH, 0) + COALESCE(INDEX_LENGTH, 0)) AS total_size,
                   TABLE_COMMENT AS comment
            FROM information_schema.TABLES
            WHERE TABLE_SCHEMA = DATABASE() AND TABLE_TYPE = 'BASE TABLE'
            ORDER BY TABLE_NAME`,
        }
      if (sub === 'views')
        return {
          sql: `
            SELECT TABLE_NAME AS name, TABLE_SCHEMA AS \`schema\`, 'view' AS kind, VIEW_DEFINITION AS definition
            FROM information_schema.VIEWS
            WHERE TABLE_SCHEMA = DATABASE()
            ORDER BY TABLE_NAME`,
        }
      if (sub === 'functions')
        return {
          sql: `
            SELECT ROUTINE_NAME AS name, ROUTINE_SCHEMA AS \`schema\`, ROUTINE_TYPE AS kind, DATA_TYPE AS returns
            FROM information_schema.ROUTINES
            WHERE ROUTINE_SCHEMA = DATABASE()
            ORDER BY ROUTINE_NAME`,
        }
      // triggers
      return {
        sql: `
          SELECT TRIGGER_NAME AS name, EVENT_OBJECT_TABLE AS \`table\`, ACTION_TIMING AS timing,
                 EVENT_MANIPULATION AS event, TRIGGER_SCHEMA AS \`schema\`
          FROM information_schema.TRIGGERS
          WHERE TRIGGER_SCHEMA = DATABASE()
          ORDER BY TRIGGER_NAME`,
      }
    }

    if (lite) {
      if (sub === 'tables')
        return {
          sql: `SELECT name, 'table' AS kind FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name`,
        }
      if (sub === 'views')
        return { sql: `SELECT name, 'view' AS kind FROM sqlite_master WHERE type = 'view' ORDER BY name` }
      if (sub === 'functions') return null // SQLite exposes no catalog of functions
      // triggers
      return { sql: `SELECT name, tbl_name AS "table" FROM sqlite_master WHERE type = 'trigger' ORDER BY name` }
    }

    if (ct === 'clickhouse') {
      if (sub === 'tables')
        return {
          sql: `
            SELECT name, database AS schema, engine, total_rows AS estimated_row, total_bytes AS total_size
            FROM system.tables
            WHERE database NOT IN ('system', 'INFORMATION_SCHEMA', 'information_schema')
              AND engine NOT LIKE '%View%'
            ORDER BY name`,
        }
      if (sub === 'views')
        return {
          sql: `
            SELECT name, database AS schema, engine
            FROM system.tables
            WHERE database NOT IN ('system', 'INFORMATION_SCHEMA', 'information_schema')
              AND engine LIKE '%View%'
            ORDER BY name`,
        }
      return null // ClickHouse has no per-object triggers; builtin functions aren't useful here
    }

    if (ct === 'duckdb') {
      if (sub === 'tables')
        return {
          sql: `SELECT table_name AS name, schema_name AS schema, estimated_size AS estimated_row, column_count, index_count FROM duckdb_tables() ORDER BY table_name`,
        }
      if (sub === 'views')
        return { sql: `SELECT view_name AS name, schema_name AS schema FROM duckdb_views() WHERE NOT internal ORDER BY view_name` }
      return null
    }

    if (ct === 'mssql') {
      if (sub === 'tables')
        return {
          sql: `
            SELECT t.name AS name, s.name AS [schema], SUM(p.rows) AS estimated_row,
                   SUM(a.total_pages) * 8192 AS total_size,
                   SUM(a.used_pages) * 8192 AS data_size
            FROM sys.tables t
            JOIN sys.schemas s ON s.schema_id = t.schema_id
            JOIN sys.partitions p ON p.object_id = t.object_id AND p.index_id IN (0, 1)
            JOIN sys.allocation_units a ON a.container_id = p.partition_id
            GROUP BY t.name, s.name
            ORDER BY t.name`,
        }
      if (sub === 'views')
        return { sql: `SELECT name, SCHEMA_NAME(schema_id) AS [schema] FROM sys.views ORDER BY name` }
      if (sub === 'functions')
        return {
          sql: `SELECT name, type_desc AS kind, SCHEMA_NAME(schema_id) AS [schema] FROM sys.objects WHERE type IN ('FN', 'IF', 'TF', 'P') ORDER BY name`,
        }
      return { sql: `SELECT name, OBJECT_NAME(parent_id) AS [table], is_disabled FROM sys.triggers ORDER BY name` }
    }

    // Unknown engine → best-effort tables via the SQL standard, everything else unsupported.
    if (sub === 'tables')
      return {
        sql: `SELECT table_name AS name, table_schema AS schema, table_type AS kind FROM information_schema.tables WHERE table_type = 'BASE TABLE' ORDER BY table_name`,
      }
    return null
  }

  const currentSupported = $derived(queryFor(activeSub) !== null)

  /** @param {SubTab} sub */
  async function load(sub) {
    const q = queryFor(sub)
    const slot = slots[sub]
    if (!q) {
      slot.data = []
      slot.error = ''
      slot.loading = false
      return
    }
    slot.loading = true
    slot.error = ''
    try {
      slot.data = toRecords(await executeSql(q.sql))
    } catch (e) {
      if (q.fallback) {
        try {
          slot.data = toRecords(await executeSql(q.fallback))
          return
        } catch {
          /* fall through to error */
        }
      }
      slot.error = String(e)
      slot.data = []
    } finally {
      slot.loading = false
    }
  }

  // Lazy-load the active sub-tab the first time it is shown.
  $effect(() => {
    const slot = slots[activeSub]
    if (currentSupported && slot.data === null && !slot.loading) void load(activeSub)
  })

  export function refresh() {
    const slot = slots[activeSub]
    slot.data = null
    if (currentSupported) void load(activeSub)
  }

  const isLoading = $derived(slots[activeSub].loading || (currentSupported && slots[activeSub].data === null))

  // ── Cell formatting ─────────────────────────────────────────────────────────
  // Columns whose values are raw byte counts get a human-readable size; row-count
  // columns get thousands separators. Everything else renders as-is.
  const BYTE_COLS = new Set(['total_size', 'data_size', 'index_size', 'size', 'data_length', 'index_length', 'data_free'])
  const NUM_COLS = new Set(['estimated_row', 'table_rows', 'n_live_tup', 'row_estimate', 'column_count', 'index_count'])

  /** @param {unknown} n */
  function humanBytes(n) {
    const v = Number(n)
    if (n == null || n === '' || !Number.isFinite(v)) return '—'
    if (v === 0) return '0 B'
    const units = ['B', 'kB', 'MB', 'GB', 'TB', 'PB']
    let i = 0
    let x = v
    while (x >= 1024 && i < units.length - 1) {
      x /= 1024
      i += 1
    }
    return `${i === 0 ? Math.round(x) : x < 10 ? x.toFixed(1) : Math.round(x)} ${units[i]}`
  }

  /** @param {unknown} n */
  function humanNum(n) {
    const v = Number(n)
    if (n == null || n === '' || !Number.isFinite(v)) return '—'
    return v.toLocaleString('en-US')
  }

  /** @param {string} col @param {unknown} v */
  function fmt(col, v) {
    if (BYTE_COLS.has(col)) return humanBytes(v)
    if (NUM_COLS.has(col)) return humanNum(v)
    if (v == null || v === '') return '—'
    if (v === true) return 'true'
    if (v === false) return 'false'
    return String(v)
  }

  /** Columns of the current record set, name always first when present. */
  const columns = $derived.by(() => {
    const rows = slots[activeSub].data
    if (!rows || rows.length === 0) return /** @type {string[]} */ ([])
    const keys = Object.keys(rows[0])
    return keys.includes('name') ? ['name', ...keys.filter((k) => k !== 'name')] : keys
  })

  let objQuery = $state('')
  // Client-side filter across all string columns of the active object list.
  const rows = $derived.by(() => {
    const all = slots[activeSub].data ?? []
    const q = objQuery.trim().toLowerCase()
    if (!q) return all
    return all.filter((r) => Object.values(r).some((v) => v != null && String(v).toLowerCase().includes(q)))
  })
  const totalCount = $derived(slots[activeSub].data?.length ?? 0)
  const emptyIcon = $derived(TABS.find((t) => t.id === activeSub)?.icon ?? Boxes)
</script>

<svelte:window
  onkeydown={(e) => {
    if (!active) return
    if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key === 'r') {
      e.preventDefault()
      refresh()
    }
  }}
/>

<div class="flex min-h-0 flex-1 flex-col overflow-hidden bg-panel">
  <!-- Tab row -->
  <div class="flex shrink-0 items-center border-b border-border bg-panel">
    {#each TABS as tab (tab.id)}
      <button
        type="button"
        class={cn(
          'relative flex h-8 items-center gap-1.5 px-3 text-ui-xs transition-colors',
          activeSub === tab.id ? 'font-medium text-foreground' : 'text-muted-foreground hover:text-foreground',
        )}
        onclick={() => { activeSub = tab.id; objQuery = '' }}
      >
        {#if activeSub === tab.id}
          <span class="absolute inset-x-1 bottom-0 h-0.5 rounded-full bg-primary transition-colors" aria-hidden="true"></span>
        {/if}
        <tab.icon class="size-3.5 shrink-0" />
        {tab.label}
      </button>
    {/each}
    <div class="flex-1"></div>
    <button
      type="button"
      class="mr-1 inline-flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:text-foreground"
      title="Refresh (⌘R)"
      onclick={refresh}
    >
      <RefreshCw class={cn('size-3.5', isLoading && 'animate-spin')} />
    </button>
  </div>

  <!-- Count strip + search -->
  <div class="flex items-center justify-between gap-3 border-b border-border/40 px-3 py-2">
    <span class="shrink-0 font-mono text-ui-xs text-muted-foreground">
      {#if !currentSupported}
        {engineLabel} · {activeSub}
      {:else if isLoading}
        Loading…
      {:else if objQuery.trim()}
        {rows.length} of {totalCount}
        {activeSub === 'functions' && totalCount === 1 ? 'function' : activeSub}
      {:else}
        {totalCount}
        {activeSub === 'functions' && totalCount === 1 ? 'function' : activeSub}
      {/if}
    </span>
    {#if currentSupported && totalCount > 0}
      <div class="relative flex h-7 w-64 items-center">
        <Search class="pointer-events-none absolute left-2.5 size-3.5 shrink-0 text-muted-foreground" />
        <input
          bind:this={searchEl}
          type="text"
          role="searchbox"
          aria-label="Search {activeSub}"
          autocomplete="off"
          spellcheck="false"
          bind:value={objQuery}
          placeholder="Search {activeSub}…"
          onkeydown={(e) => {
            // Escape clears the filter rather than closing anything: the list
            // behind it is the page, and there is nothing to close.
            if (e.key === 'Escape' && objQuery) { e.preventDefault(); e.stopPropagation(); objQuery = '' }
          }}
          class="field-surface h-7 w-full border-transparent bg-accent/40 pl-8 pr-7 text-ui-sm text-foreground placeholder:text-muted-foreground transition-colors focus:bg-input/30 focus:outline-none"
        />
        {#if objQuery}
          <button
            type="button"
            class="absolute right-1 inline-flex size-5 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            aria-label="Clear search"
            onclick={() => (objQuery = '')}
          >
            <X class="size-3.5 shrink-0" />
          </button>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Content -->
  <div class="app-scroll min-h-0 flex-1 overflow-auto [will-change:transform]">
    {#if !currentSupported}
      <div class="flex h-full flex-col items-center justify-center gap-3 py-20 text-center">
        <Boxes class="size-10 shrink-0 text-muted-foreground" />
        <p class="font-mono text-ui text-muted-foreground">
          {TABS.find((t) => t.id === activeSub)?.label} overview isn't available for {engineLabel} yet
        </p>
      </div>
    {:else if isLoading}
      <div class="loading-reveal flex items-center justify-center gap-2 py-16 text-muted-foreground">
        <RefreshCw class="size-4 shrink-0 animate-spin" /><span class="font-mono text-ui-sm">Loading…</span>
      </div>
    {:else if slots[activeSub].error}
      <div class="px-4 py-8 text-center">
        <p class="font-mono text-ui-xs text-destructive">{slots[activeSub].error}</p>
        <button
          onclick={() => { slots[activeSub].data = null; void load(activeSub) }}
          class="mt-2 font-mono text-ui-xs text-muted-foreground underline">Retry</button
        >
      </div>
    {:else if rows.length === 0}
      {@const EmptyIcon = emptyIcon}
      <div class="flex h-full flex-col items-center justify-center gap-3 py-20 text-center">
        <EmptyIcon class="size-10 text-muted-foreground" />
        <p class="font-mono text-ui text-muted-foreground">No {activeSub} found</p>
      </div>
    {:else}
      <table class="obj-table w-full text-ui-xs">
        <colgroup>
          <col style="width:3rem" />
          {#each columns as col (col)}
            <col style={col === 'comment' || col === 'definition' || col === 'returns' ? '' : 'width:12rem'} />
          {/each}
        </colgroup>
        <thead class="studio-chrome sticky top-0 z-10 bg-panel text-left">
          <tr class="border-b border-border/50">
            <th class="px-3 py-2 font-mono font-normal text-muted-foreground">#</th>
            {#each columns as col (col)}
              <th class="whitespace-nowrap px-3 py-2 font-mono font-normal text-muted-foreground">{col}</th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each rows as row, i (i)}
            <!-- Tables and views open as a data tab; a function or a trigger has
                 no grid to open, so those rows stay inert rather than pretending
                 to be a link. -->
            {@const openable = (activeSub === 'tables' || activeSub === 'views') && !!row.name}
            <tr
              class={cn('obj-row border-b border-border/30 hover:bg-accent/25', openable && 'cursor-pointer')}
              onclick={openable
                ? () => onopen({ schema: String(row.schema ?? ''), name: String(row.name) })
                : undefined}
            >
              <td class="px-3 py-1.5 font-mono tabular-nums text-muted-foreground">{i + 1}</td>
              {#each columns as col (col)}
                {@const isSize = BYTE_COLS.has(col)}
                {@const isNum = NUM_COLS.has(col) || isSize}
                <td
                  class={cn(
                    'max-w-[22rem] truncate px-3 py-1.5 font-mono',
                    col === 'name' ? 'font-medium text-foreground'
                    : isNum ? 'tabular-nums text-foreground/80'
                    : 'text-muted-foreground',
                  )}
                  title={col === 'name' && openable ? `Open ${fmt(col, row[col])}` : fmt(col, row[col])}
                >
                  {#if col === 'name' && openable}
                    <!-- The row takes the click for the pointer; this button is
                         what the keyboard and a screen reader get, so the action
                         is not mouse-only. -->
                    <button
                      type="button"
                      class="max-w-full truncate text-left underline-offset-2 hover:underline focus-visible:underline"
                      onclick={(e) => { e.stopPropagation(); onopen({ schema: String(row.schema ?? ''), name: String(row.name) }) }}
                    >{fmt(col, row[col])}</button>
                  {:else}
                    {fmt(col, row[col])}
                  {/if}
                </td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
</div>

<style>
  /* Fixed layout skips per-cell column re-measurement; content-visibility lets the
     engine skip layout/paint of off-screen rows so scrolling large object lists
     (a few thousand tables/routines) stays smooth. */
  .obj-table {
    table-layout: fixed;
  }
  .obj-row {
    content-visibility: auto;
    contain-intrinsic-size: auto 28px;
  }
</style>
