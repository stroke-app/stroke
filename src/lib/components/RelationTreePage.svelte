<script>
  import { tick, untrack } from 'svelte'
  import { listTables, getTableColumnStructure, listIndexes, getTableRowCounts } from '$lib/api.js'
  import { formatTableRowCount } from '$lib/table-list.js'
  import RelationTreeNode from './RelationTreeNode.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import Loader from '@lucide/svelte/icons/loader'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import Search from '@lucide/svelte/icons/search'
  import X from '@lucide/svelte/icons/x'
  import KeyRound from '@lucide/svelte/icons/key-round'
  import Link from '@lucide/svelte/icons/link'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'
  import ChevronDown from '@lucide/svelte/icons/chevron-down'
  import GitBranch from '@lucide/svelte/icons/git-branch'
  import ArrowUpRight from '@lucide/svelte/icons/arrow-up-right'
  import ArrowDownRight from '@lucide/svelte/icons/arrow-down-right'
  import ExternalLink from '@lucide/svelte/icons/external-link'
  import Table2 from '@lucide/svelte/icons/table-2'
  import ZoomIn from '@lucide/svelte/icons/zoom-in'
  import ZoomOut from '@lucide/svelte/icons/zoom-out'
  import Maximize2 from '@lucide/svelte/icons/maximize-2'
  import MermaidViewer from './MermaidViewer.svelte'
  import { relationsToFlowchart } from '$lib/erd-mermaid.js'
  import { hubTables } from '$lib/erd-filter.js'
  import PanelLeftClose from '@lucide/svelte/icons/panel-left-close'
  import PanelLeftOpen from '@lucide/svelte/icons/panel-left-open'

  let {
    schema = 'public',
    schemas = /** @type {string[]} */ ([]),
    onopentable = /** @type {((schema:string, table:string)=>void)|undefined} */ (undefined),
    /** Inside another page (the schema map): no title row of its own, and the
     *  flowchart is the first thing shown. */
    embedded = false,
    /** Tables already loaded by the parent. Set, nothing is fetched here but
     *  the row counts. @type {Map<string, TableMeta> | null} */
    initialMeta = null,
  } = $props()

  /**
   * @typedef {{ name: string, dataType: string, isNullable: boolean,
   *   columnDefault: string|null, foreignKey: string|null,
   *   fkConstraintName: string|null, ordinalPosition: number }} Col
   * @typedef {{ name: string, columns: Col[], pkCols: Set<string> }} TableMeta
   */

  // ── State ─────────────────────────────────────────────────────────────────
  let loading = $state(false)
  let loadedCount = $state(0)
  let totalCount = $state(0)
  let error = $state('')
  let activeSchema = $state(untrack(() => schema))
  let schemaOpen = $state(false)
  let listSearch = $state('')
  let listSearchEl = $state(/** @type {HTMLInputElement | null} */ (null))
  /** @type {string|null} */
  let focusedTable = $state(null)

  /** @type {Map<string, TableMeta>} */
  let tableMeta = $state(new Map())

  // ── Flow / list ───────────────────────────────────────────────────────────
  /** Around the focused table as a flowchart, the whole page as one, or the list. */
  let view = $state(/** @type {'flow' | 'all' | 'list'} */ ('list'))
  /** Hops from the focused table the flowchart reaches. Two is plenty: three is the schema. */
  let depth = $state(1)
  /** The table list on the left; folded away, the drawing gets its width.
   *  Inside the data model it starts folded - that page is about the picture. */
  let listOpen = $state(untrack(() => !embedded))
  /** @type {MermaidViewer | null} */
  let flowViewer = $state(null)
  /** Every relationship on the page, in the generator's shape. */
  const flowRels = $derived.by(() => {
    const rels = []
    for (const [table, refs] of outbound) for (const r of refs) rels.push({ source: table, target: r.refTable, sourceCol: r.col })
    return rels
  })
  const flowCode = $derived(
    focusedTable && tableMeta.has(focusedTable)
      ? relationsToFlowchart([...tableMeta.values()], flowRels, { focus: focusedTable, depth })
      : '',
  )
  /** Above this many tables the whole-page map waits for a click: it is seconds of rendering. */
  const ALL_AUTO_MAX = 80
  let allForce = $state(false)
  const allGated = $derived(tableMeta.size > ALL_AUTO_MAX && !allForce)
  /**
   * The whole page, left to right, arrows from the key to what it points at.
   * Links into hub tables (`tenants` from nearly every table) are left out and
   * counted: they are most of the ink and none of the shape. One arrow per
   * pair, named after every column that joins the two.
   */
  const flowHubs = $derived(hubTables(flowRels, tableMeta.size))
  const allRels = $derived(flowRels.filter((r) => !flowHubs.has(r.target)))
  const allCode = $derived.by(() => {
    if (!tableMeta.size) return ''
    return relationsToFlowchart([...tableMeta.values()], allRels, { merge: true, direction: 'LR' })
  })

  /**
   * Per-table exact row counts, filled in a BACKGROUND pass after the tree has
   * already rendered - the schema/column load (`load()`) never waits on these,
   * so opening the relation view and running the main queries stays instant.
   * Counts patch in optimistically as `pg_table_row_counts` resolves.
   * @type {Map<string, number>}
   */
  let rowCounts = $state(new Map())

  // ── Expand / column-show tracking ────────────────────────────────────────
  /** @type {Set<string>} */
  let expanded = $state(new Set())
  /** @type {Set<string>} */
  let showCols = $state(new Set())

  /** @param {string} key */
  function toggleExpand(key) {
    const n = new Set(expanded); n.has(key) ? n.delete(key) : n.add(key); expanded = n
  }
  /** @param {string} key */
  function toggleCols(key) {
    const n = new Set(showCols); n.has(key) ? n.delete(key) : n.add(key); showCols = n
  }

  // Reset when focused table changes
  $effect(() => { focusedTable; expanded = new Set(); showCols = new Set() })

  // ── Derived relationship maps ──────────────────────────────────────────────
  const outbound = $derived.by(() => {
    /** @type {Map<string, {col:string, refTable:string, refCol:string}[]>} */
    const m = new Map()
    for (const t of tableMeta.values()) {
      const refs = []
      for (const c of t.columns) {
        if (!c.foreignKey) continue
        const p = c.foreignKey.split('.')
        if (p.length < 3) continue
        refs.push({ col: c.name, refTable: p[1], refCol: p[2] })
      }
      m.set(t.name, refs)
    }
    return m
  })

  const inbound = $derived.by(() => {
    /** @type {Map<string, {fromTable:string, fromCol:string, refCol:string}[]>} */
    const m = new Map()
    for (const t of tableMeta.values()) m.set(t.name, [])
    for (const t of tableMeta.values()) {
      for (const c of t.columns) {
        if (!c.foreignKey) continue
        const p = c.foreignKey.split('.')
        if (p.length < 3) continue
        const list = m.get(p[1])
        if (list) list.push({ fromTable: t.name, fromCol: c.name, refCol: p[2] })
      }
    }
    return m
  })

  // ── Filtered table list ───────────────────────────────────────────────────
  const filteredTables = $derived.by(() => {
    const q = listSearch.trim().toLowerCase()
    const all = [...tableMeta.values()].sort((a, b) => a.name.localeCompare(b.name))
    return q ? all.filter(t => t.name.toLowerCase().includes(q)) : all
  })

  // ── Load - batched ────────────────────────────────────────────────────────
  const BATCH = 8

  /**
   * Background, non-blocking row-count pass. Fired (not awaited) once the tree
   * is built so it never delays schema loading or the main query path. Only
   * Postgres/MySQL return counts - other engines resolve to `[]` and this is a
   * silent no-op. Best-effort: failures are swallowed, the tree stays usable.
   */
  async function loadRowCounts() {
    const names = [...tableMeta.keys()]
    if (names.length === 0) return
    try {
      const counts = /** @type {{ name: string, rowCount: number }[]} */ (
        await getTableRowCounts(activeSchema, names)
      )
      const next = new Map(rowCounts)
      for (const { name, rowCount } of counts) {
        if (typeof rowCount === 'number' && rowCount >= 0) next.set(name, rowCount)
      }
      rowCounts = next
    } catch { /* counts are best-effort - never block or surface an error */ }
  }

  async function load() {
    loading = true; loadedCount = 0; error = ''; tableMeta = new Map(); rowCounts = new Map()
    try {
      const tableList = initialMeta ? [] : /** @type {{ name: string }[]} */ (await listTables(activeSchema))
      if (initialMeta) tableMeta = new Map(initialMeta)
      totalCount = tableList.length
      for (let i = 0; i < tableList.length; i += BATCH) {
        const chunk = tableList.slice(i, i + BATCH)
        const results = await Promise.allSettled(
          chunk.map(async t => {
            const cols = /** @type {Col[]} */ (await getTableColumnStructure(activeSchema, t.name))
            const pkCols = new Set(cols.filter(c =>
              c.columnDefault?.includes('nextval') || (c.name === 'id' && !c.isNullable && !c.foreignKey)
            ).map(c => c.name))
            return /** @type {TableMeta} */ ({ name: t.name, columns: cols, pkCols })
          })
        )
        for (const r of results) {
          if (r.status === 'fulfilled') tableMeta.set(r.value.name, r.value)
        }
        loadedCount += chunk.length
        tableMeta = new Map(tableMeta)
        await tick()
      }
      // Refine PKs from indexes - the parent's tables already carry them.
      try {
        if (initialMeta) throw null
        const idxs = /** @type {{ tableName:string, isPrimary:boolean, columns:string }[]} */ (
          await listIndexes(activeSchema)
        )
        for (const idx of idxs) {
          if (!idx.isPrimary) continue
          const m = tableMeta.get(idx.tableName)
          if (m) m.pkCols = new Set(idx.columns.split(',').map(s => s.trim().replace(/"/g, '')))
        }
        tableMeta = new Map(tableMeta)
      } catch { /* non-critical */ }

      // Auto-focus first table with relationships
      if (!focusedTable) {
        const first = [...tableMeta.values()].find(t =>
          (outbound.get(t.name)?.length ?? 0) > 0 || (inbound.get(t.name)?.length ?? 0) > 0
        )
        focusedTable = first?.name ?? (tableMeta.size > 0 ? [...tableMeta.keys()][0] : null)
      }
      // Fire-and-forget: counts stream in after the tree is already interactive.
      void loadRowCounts()
    } catch (e) {
      error = String(e)
    } finally {
      loading = false
    }
  }

  $effect(() => {
    void activeSchema
    void initialMeta
    // Untracked on purpose. With the tables handed in there is no await before
    // load() reads and writes the tree's own state (tableMeta, focusedTable),
    // and an effect that reads what it writes runs itself into the ground.
    untrack(() => void load())
  })
  // Embedded, the schema is the parent's choice; the picker here is hidden.
  $effect(() => { if (embedded) activeSchema = schema })
</script>

<svelte:window onkeydown={(e) => {
  if (!listSearchEl || !listSearchEl.offsetParent) return
  if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key === 'f') {
    e.preventDefault(); listSearchEl.focus(); listSearchEl.select()
  }
}} />

<div class="flex min-h-0 flex-1 overflow-hidden">

  <!-- ── Left: table list ──────────────────────────────────────────────────── -->
  {#if listOpen}
  <div class="flex w-56 shrink-0 flex-col border-r border-border/50 bg-panel">
    {#if !embedded}
      <div class="studio-chrome flex h-10 shrink-0 items-center gap-2 border-b border-border/60 px-3" data-studio-chrome>
        <GitBranch class="size-3.5 shrink-0 text-muted-foreground" />
        <span class="font-mono text-ui-xs font-semibold text-foreground/70">Relation Tree</span>
        <button
          type="button"
          disabled={loading}
          class="ml-auto flex size-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-40"
          onclick={() => void load()}
        >
          <RefreshCw class="size-3 {loading ? 'animate-spin' : ''}" />
        </button>
      </div>
    {/if}

    <!-- Search -->
    <div class="relative px-2 py-1.5">
      <Search class="pointer-events-none absolute left-4 top-1/2 size-3 -translate-y-1/2 text-muted-foreground" />
      <input
        type="text"
        bind:this={listSearchEl}
        bind:value={listSearch}
        placeholder="Filter tables…"
        class= "field-surface h-7 w-full bg-background/60 pl-7 pr-2 font-mono text-ui-xs outline-none placeholder:text-muted-foreground"
      />
    </div>

    <!-- Progress -->
    {#if loading && totalCount > 0}
      <div class="mx-2 mb-1 h-0.5 overflow-hidden rounded-full bg-muted/30">
        <div class="h-full rounded-full bg-primary/50 transition-all" style="width:{Math.round(loadedCount/totalCount*100)}%"></div>
      </div>
    {/if}

    <!-- Table list -->
    <div class="min-h-0 flex-1 overflow-y-auto">
      {#if loading && tableMeta.size === 0}
        <div class="flex items-center gap-2 px-3 py-3">
          <Loader class="size-3.5 animate-spin text-muted-foreground" />
          <span class="font-mono text-ui-xs text-muted-foreground">Loading…</span>
        </div>
      {:else if error}
        <p class="px-3 py-3 font-mono text-ui-xs text-destructive">{error}</p>
      {:else}
        {#each filteredTables as t (t.name)}
          {@const hasOut = (outbound.get(t.name)?.length ?? 0) > 0}
          {@const hasIn = (inbound.get(t.name)?.length ?? 0) > 0}
          {@const active = focusedTable === t.name}
          <button
            type="button"
            class="flex w-full items-center gap-2 border-b border-border/10 px-3 py-1.5 text-left transition-colors last:border-0
              {active ? 'bg-primary/10 text-foreground' : 'text-muted-foreground hover:bg-accent/30 hover:text-foreground'}"
            onclick={() => (focusedTable = t.name)}
          >
            <Table2 class="size-3 shrink-0 {active ? 'text-primary' : 'text-muted-foreground'}" />
            <span class="min-w-0 flex-1 truncate font-mono text-ui-xs">{t.name}</span>
            <span class="flex shrink-0 items-center gap-0.5">
              {#if hasOut}<ArrowUpRight class="size-3 text-info" />{/if}
              {#if hasIn}<ArrowDownRight class="size-3 text-success" />{/if}
            </span>
          </button>
        {:else}
          <p class="px-3 py-4 font-mono text-ui-xs text-muted-foreground">No tables</p>
        {/each}
      {/if}
    </div>
  </div>
  {/if}

  <!-- ── Right: the focused table's relationships, drawn or listed ────────── -->
  <div class="flex min-h-0 min-w-0 flex-1 flex-col bg-background">
  <div class="flex h-9 shrink-0 items-center gap-2 border-b border-border/40 bg-panel px-2">
    <button
      type="button"
      title={listOpen ? 'Hide the table list' : 'Show the table list'}
      aria-pressed={listOpen}
      onclick={() => (listOpen = !listOpen)}
      class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
    >{#if listOpen}<PanelLeftClose class="size-3.5" />{:else}<PanelLeftOpen class="size-3.5" />{/if}</button>
    {#if view === 'all'}
      <span class="min-w-0 truncate font-mono text-ui-xs font-semibold text-foreground/80">{activeSchema}</span>
      <span class="shrink-0 font-mono text-ui-3xs tabular-nums text-muted-foreground">
        {tableMeta.size} tables · {allRels.length} links{flowRels.length - allRels.length ? ` · ${flowRels.length - allRels.length} hub links left out` : ''}
      </span>
    {:else if focusedTable && tableMeta.has(focusedTable)}
      <span class="min-w-0 truncate font-mono text-ui-xs font-semibold text-foreground/80">{focusedTable}</span>
      <span class="shrink-0 font-mono text-ui-3xs tabular-nums text-muted-foreground">
        {(outbound.get(focusedTable)?.length ?? 0)} out · {(inbound.get(focusedTable)?.length ?? 0)} in
      </span>
    {/if}
    <span class="ml-auto"></span>
    {#if view === 'flow'}
      <span class="text-ui-2xs text-muted-foreground">Hops</span>
      <div class="field-surface inline-flex h-6 items-center bg-muted/25 p-0.5" title="How far from the table to draw: its neighbours, or their neighbours too">
        {#each [1, 2] as d (d)}
          <button
            type="button"
            aria-pressed={depth === d}
            onclick={() => (depth = d)}
            class="inline-flex h-5 min-w-6 items-center justify-center rounded-[5px] px-2 text-ui-2xs tabular-nums transition-[background-color,color] {depth === d ? 'bg-background text-foreground ring-1 ring-inset ring-border/70' : 'text-muted-foreground hover:text-foreground'}"
          >{d}</button>
        {/each}
      </div>
    {/if}
    {#if view === 'flow' || view === 'all'}
      <div class="flex items-center gap-0.5">
        <button type="button" title="Zoom out" onclick={() => flowViewer?.dispatch('diagram:zoomout')} class="flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"><ZoomOut class="size-3.5" /></button>
        <button type="button" title="Zoom in" onclick={() => flowViewer?.dispatch('diagram:zoomin')} class="flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"><ZoomIn class="size-3.5" /></button>
        <button type="button" title="Fit" onclick={() => flowViewer?.dispatch('diagram:reset')} class="flex size-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"><Maximize2 class="size-3.5" /></button>
      </div>
    {/if}
    <div class="field-surface inline-flex h-6 items-center bg-muted/25 p-0.5">
      {#each [{ id: 'flow', label: 'Flow', hint: 'A flowchart around the table: arrows from the key to what it points at' }, { id: 'all', label: 'All', hint: 'Every table on the page, one line per link' }, { id: 'list', label: 'List', hint: 'Every relationship, table by table, with row counts' }] as m (m.id)}
        <button
          type="button"
          title={m.hint}
          aria-pressed={view === m.id}
          onclick={() => (view = /** @type {'flow'|'all'|'list'} */ (m.id))}
          class="inline-flex h-5 items-center rounded-[5px] px-2 text-ui-2xs transition-[background-color,color] {view === m.id ? 'bg-background text-foreground ring-1 ring-inset ring-border/70' : 'text-muted-foreground hover:text-foreground'}"
        >{m.label}</button>
      {/each}
    </div>
  </div>
  {#if view === 'all' && allGated}
    <div class="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
      <p class="text-ui-sm text-foreground">{tableMeta.size} tables is a few seconds of rendering.</p>
      <p class="max-w-sm text-ui-xs text-muted-foreground">Flow draws one table and its neighbours instantly. Or draw the whole page anyway.</p>
      <Button size="sm" onclick={() => (allForce = true)}>Draw {tableMeta.size} tables</Button>
    </div>
  {:else if (view === 'flow' && focusedTable && tableMeta.has(focusedTable)) || (view === 'all' && tableMeta.size > 0)}
    <div class="min-h-0 flex-1 overflow-hidden">
      <MermaidViewer bind:this={flowViewer} code={view === 'all' ? allCode : flowCode} spacing={{ nodeSpacing: 36, layerSpacing: 96, padding: 48 }} class="h-full w-full" />
    </div>
  {:else}
  <div class="min-h-0 min-w-0 flex-1 overflow-y-auto p-4">
    {#if !focusedTable || !tableMeta.has(focusedTable)}
      <div class="flex h-full min-h-[200px] flex-col items-center justify-center gap-3">
        <GitBranch class="size-10 text-muted-foreground" />
        <p class="font-mono text-ui-xs text-muted-foreground">Select a table to explore its relationships</p>
      </div>
    {:else}
      {@const rootMeta = tableMeta.get(focusedTable)}
      {@const rootOut = outbound.get(focusedTable) ?? []}
      {@const rootIn = inbound.get(focusedTable) ?? []}
      {@const shared = { tableMeta, outbound, inbound, expanded, showCols, rowCounts, toggleExpand, toggleCols, activeSchema, onopentable, onfocustable: (name) => (focusedTable = name) }}
      {@const rootCount = rowCounts.get(focusedTable)}

      <div class="mx-auto max-w-4xl">
        <!-- The table itself: a card (DESIGN_SYSTEM §9) with its columns behind
             a disclosure. Plain surfaces, one primary action. -->
        <div class="mb-5 rounded-lg border border-border bg-panel">
          <div class="flex items-center gap-3 px-4 py-3">
            <Table2 class="size-4 shrink-0 text-muted-foreground" />
            <div class="min-w-0 flex-1">
              <p class="truncate font-mono text-ui font-semibold text-foreground">{focusedTable}</p>
              <p class="font-mono text-ui-xs tabular-nums text-muted-foreground">
                {rootMeta?.columns.length ?? 0} columns · {rootOut.length} references · {rootIn.length} referenced by{#if rootCount !== undefined} · {formatTableRowCount(rootCount)} rows{/if}
              </p>
            </div>
            <Button size="sm" onclick={() => onopentable?.(activeSchema, focusedTable)}>
              <ExternalLink class="size-3.5" />Open
            </Button>
          </div>
          {#if rootMeta}
            {@const ck = 'root-cols'}
            {@const open = showCols.has(ck)}
            <button
              type="button"
              aria-expanded={open}
              class="flex h-8 w-full items-center gap-2 border-t border-border/50 px-4 text-left text-ui-xs text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
              onclick={() => toggleCols(ck)}
            >
              {#if open}<ChevronDown class="size-3.5" />{:else}<ChevronRight class="size-3.5" />{/if}
              Columns <span class="tabular-nums">({rootMeta.columns.length})</span>
            </button>
            {#if open}
              <div class="border-t border-border/50 px-4 py-1.5">
                {#each rootMeta.columns as col (col.name)}
                  {@const isPk = rootMeta.pkCols.has(col.name)}
                  {@const isFk = !!col.foreignKey}
                  <div class="flex h-6 items-center gap-2">
                    {#if isPk}<KeyRound class="size-3.5 shrink-0 text-warning" aria-label="Primary key" />
                    {:else if isFk}<Link class="size-3.5 shrink-0 text-info" aria-label="Foreign key" />
                    {:else}<span class="size-3.5 shrink-0"></span>{/if}
                    <span class="min-w-0 flex-1 truncate font-mono text-ui-xs {isPk ? 'text-warning' : isFk ? 'text-info' : 'text-foreground/85'}">{col.name}</span>
                    <span class="shrink-0 font-mono text-ui-2xs text-muted-foreground">{col.dataType}</span>
                  </div>
                {/each}
              </div>
            {/if}
          {/if}
        </div>

        <!-- Outgoing FKs (this → other) -->
        {#if rootOut.length > 0}
          <section class="mb-4">
            <div class="mb-1 flex items-center gap-2 px-2.5">
              <h3 class="text-ui-3xs font-semibold uppercase tracking-[0.06em] text-muted-foreground/55">
                References <span class="tabular-nums">({rootOut.length})</span>
              </h3>
              <span class="ml-auto text-ui-2xs text-muted-foreground">this key → their row</span>
            </div>
            <div class="flex flex-col">
              {#each rootOut as rel (rel.col)}
                <RelationTreeNode
                  tableName={rel.refTable}
                  fromCol={rel.col}
                  toCol={rel.refCol}
                  direction="out"
                  depth={1}
                  path="{focusedTable}>{rel.refTable}"
                  {...shared}
                />
              {/each}
            </div>
          </section>
        {/if}

        <!-- Incoming FKs (other → this) -->
        {#if rootIn.length > 0}
          <section class="mb-4">
            <div class="mb-1 flex items-center gap-2 px-2.5">
              <h3 class="text-ui-3xs font-semibold uppercase tracking-[0.06em] text-muted-foreground/55">
                Referenced by <span class="tabular-nums">({rootIn.length})</span>
              </h3>
              <span class="ml-auto text-ui-2xs text-muted-foreground">their key → this row</span>
            </div>
            <div class="flex flex-col">
              {#each rootIn as rel (`${rel.fromTable}${rel.fromCol}`)}
                <RelationTreeNode
                  tableName={rel.fromTable}
                  fromCol={rel.fromCol}
                  toCol={rel.refCol}
                  direction="in"
                  depth={1}
                  path="{focusedTable}<{rel.fromTable}"
                  {...shared}
                />
              {/each}
            </div>
          </section>
        {/if}

        {#if rootOut.length === 0 && rootIn.length === 0}
          <p class="px-2.5 py-6 text-ui-xs text-muted-foreground">No foreign keys in or out of this table.</p>
        {/if}
      </div>
    {/if}
  </div>
  {/if}
  </div>
</div>
