<script>
  import DataTable from './DataTable.svelte'
  import ChartView from './ChartView.svelte'
  // TableJsonView / TableTextView reach the CodeMirror editor statically; importing
  // them here would pull it into the boot chunk. Loaded lazily below.
  import TableRecordView from './TableRecordView.svelte'
  import ShikiBlock from './ShikiBlock.svelte'
  import Table2 from '@lucide/svelte/icons/table-2'
  import PanelLeft from '@lucide/svelte/icons/panel-left'
  import TerminalSquare from '@lucide/svelte/icons/terminal-square'
  import TabLoading from './TabLoading.svelte'
  import { tabDisplayTitle } from '$lib/studio-tabs.js'

  /** @typedef {import('$lib/studio-tabs.js').StudioTab} StudioTab */
  /** @typedef {import('$lib/studio-tabs.js').TableTabState} TableTabState */
  /** @typedef {import('$lib/studio-tabs.js').SqlTabState} SqlTabState */

  let {
    /** @type {StudioTab | null} */
    tab = null,
    /** Reserve a toolbar-height strip so a background table pane lines up with
        the focused pane (which renders the table toolbar), prevents the grid
        from jumping when focus moves between split panes. */
    toolbarSpacer = false,
    /** Global connection id - chart save/export needs it; not in tab.state. */
    connectionId = '',
    /** Global schema list - the ERD view fetches its own metadata and needs it. */
    schemas = [],
  } = $props()

  const tableState = $derived(
    tab?.kind === 'table' && tab.state ? /** @type {TableTabState} */ (tab.state) : null,
  )
  const sqlState = $derived(
    tab?.kind === 'sql' && tab.state ? /** @type {SqlTabState} */ (tab.state) : null,
  )

  // Replicate StudioShell's dataViewColumns/dataViewRows (hidden columns dropped)
  // so the json/text/chart views render the same shape they do when focused.
  const viewColumns = $derived(
    tableState ? tableState.columns.filter((c) => !tableState.hiddenColumns?.has?.(c.name)) : [],
  )
  const viewRows = $derived.by(() => {
    if (!tableState) return []
    const hidden = tableState.hiddenColumns
    if (!hidden || hidden.size === 0) return tableState.rows
    const keep = tableState.columns.map((c, i) => (hidden.has(c.name) ? -1 : i)).filter((i) => i >= 0)
    return tableState.rows.map((r) => keep.map((i) => r[i]))
  })

  // Local, throwaway bindable state so DataTable's editing/selection machinery
  // has somewhere to write. Background panes are read-only, so nothing here is
  // persisted back to the tab.
  let selected = $state(new Set())
  let focusedRow = $state(/** @type {number | null} */ (null))
  let inspectorRow = $state(/** @type {number | null} */ (null))
  let editingCell = $state(/** @type {any} */ (null))
</script>

{#if tableState}
  <div class="flex min-h-0 min-w-0 flex-1 flex-col">
    {#if toolbarSpacer}
      <!-- Matches the focused pane's TableToolbar (h-9) so the grid doesn't jump
           vertically when focus moves between split panes. -->
      <div class="h-9 shrink-0 border-b border-border bg-panel"></div>
    {/if}
  {#if !tableState.table}
    <div class="flex flex-1 flex-col items-center justify-center gap-2 p-8 text-center">
      <Table2 class="size-6 text-muted-foreground" />
      <p class="font-mono text-ui-xs text-muted-foreground">No table</p>
    </div>
  <!-- windowedHead: a huge result set whose rows live outside the tab state (only
       the focused pane holds the sparse array), so there is nothing to preview. -->
  {:else if tableState.columns.length === 0 || tableState.windowedHead}
    <div class="flex flex-1 flex-col items-center justify-center gap-2 p-8 text-center">
      <Table2 class="size-6 text-muted-foreground" />
      <p class="font-mono text-ui-xs text-muted-foreground">Focus this pane to load {tableState.table}</p>
    </div>
  {:else if tableState.dataViewMode === 'chart'}
    <!-- Preserve the active data-view when this pane is demoted from focused to
         a snapshot (each branch mirrors the live guard in StudioShell), without
         these the pane reverted to the plain grid regardless of view mode. -->
    <div class="flex min-h-0 min-w-0 flex-1">
      <ChartView columns={viewColumns} rows={viewRows} {connectionId} />
    </div>
  {:else if tableState.dataViewMode === 'json'}
    <div class="flex min-h-0 min-w-0 flex-1">
      {#await import('./TableJsonView.svelte')}<TabLoading />{:then { default: TableJsonView }}
        <TableJsonView columns={viewColumns} rows={viewRows} tableKey={`${tableState.schema}.${tableState.table}`} />
      {/await}
    </div>
  {:else if tableState.dataViewMode === 'record'}
    <div class="flex min-h-0 min-w-0 flex-1">
      <TableRecordView
        columns={tableState.columns}
        rows={tableState.rows}
        primaryKey={tableState.primaryKey}
        hiddenColumns={tableState.hiddenColumns}
        offset={Math.max(0, ((tableState.page ?? 1) - 1) * (tableState.pageSize ?? 0))}
        total={tableState.total}
        readonly={true}
        initialIndex={tableState.focusedRow ?? 0}
        hasPrevPage={false}
        hasNextPage={false}
      />
    </div>
  {:else if tableState.dataViewMode === 'text'}
    <div class="flex min-h-0 min-w-0 flex-1">
      {#await import('./TableTextView.svelte')}<TabLoading />{:then { default: TableTextView }}
        <TableTextView columns={viewColumns} rows={viewRows} tableName={tableState.table} />
      {/await}
    </div>
  {:else if tableState.dataViewMode === 'erd'}
    <div class="flex min-h-0 min-w-0 flex-1">
      {#await import('./EntityRelationPage.svelte')}
        <TabLoading />
      {:then { default: EntityRelationPage }}
        <EntityRelationPage schema={tableState.schema} {schemas} focusTable={tableState.table} />
      {/await}
    </div>
  {:else}
    <div class="flex min-h-0 min-w-0 flex-1">
      <DataTable
        columns={tableState.columns}
        rows={tableState.rows}
        primaryKey={tableState.primaryKey}
        foreignKeys={tableState.foreignKeys}
        schema={tableState.schema}
        tableName={tableState.table}
        hiddenColumns={tableState.hiddenColumns}
        rowSort={tableState.rowSort}
        searchQuery={tableState.rowSearch}
        columnWidthsKey={`${tableState.schema}.${tableState.table}`}
        loading={false}
        readonly={true}
        initialScroll={{ left: tableState.scrollLeft ?? 0, top: tableState.scrollTop ?? 0 }}
        initialExpandedRows={tableState.expandedRows ?? []}
        bind:selected
        bind:focusedRow
        bind:inspectorRow
        bind:editingCell
      />
    </div>
  {/if}
  </div>
{:else if sqlState}
  <!-- SQL / Query Editor snapshot: the query text plus its last results, read-only,
       so a defocused pane still shows its work instead of an empty placeholder. -->
  <div class="flex min-h-0 min-w-0 flex-1 flex-col">
    {#if sqlState.sqlText?.trim()}
      <div class="flex max-h-[38%] min-h-0 shrink-0 flex-col overflow-hidden border-b border-border/50">
        <ShikiBlock code={sqlState.sqlText} lang="sql" />
      </div>
    {/if}
    {#if sqlState.sqlColumns?.length}
      <div class="flex min-h-0 min-w-0 flex-1">
        <DataTable
          columns={sqlState.sqlColumns}
          rows={sqlState.sqlRows}
          primaryKey={[]}
          foreignKeys={[]}
          loading={false}
          readonly={true}
          bind:selected
          bind:focusedRow
          bind:inspectorRow
          bind:editingCell
        />
      </div>
    {:else}
      <div class="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 p-8 text-center">
        <TerminalSquare class="size-6 text-muted-foreground" />
        {#if sqlState.sqlError}
          <p class="max-w-md font-mono text-ui-2xs text-destructive">{sqlState.sqlError}</p>
        {:else if sqlState.sqlMessage}
          <p class="font-mono text-ui-xs text-muted-foreground">{sqlState.sqlMessage}</p>
        {:else}
          <p class="font-mono text-ui-xs text-muted-foreground">
            {sqlState.sqlText?.trim() ? 'No results yet' : 'Empty query'}
          </p>
        {/if}
      </div>
    {/if}
  </div>
{:else}
  <div class="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center">
    <PanelLeft class="size-7 text-muted-foreground" />
    <div class="space-y-1">
      <p class="text-ui-sm font-medium text-foreground/70">{tab ? tabDisplayTitle(tab) : 'Empty pane'}</p>
      <p class="max-w-xs text-ui-xs text-muted-foreground">
        Click this pane to focus it and interact with this view.
      </p>
    </div>
  </div>
{/if}
