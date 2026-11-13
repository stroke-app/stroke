<script>
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { executeSql } from '$lib/api.js'
  import Zap from '@lucide/svelte/icons/zap'
  import Tags from '@lucide/svelte/icons/tags'
  import Eye from '@lucide/svelte/icons/eye'
  import Layers from '@lucide/svelte/icons/layers'
  import GitBranch from '@lucide/svelte/icons/git-branch'
  import Hash from '@lucide/svelte/icons/hash'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import Search from '@lucide/svelte/icons/search'
  import Plus from '@lucide/svelte/icons/plus'
  import Trash2 from '@lucide/svelte/icons/trash-2'
  import { readOnlyMode, READ_ONLY_HINT } from '$lib/stores/read-only.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { cn } from '$lib/utils.js'
  import Lock from '@lucide/svelte/icons/lock'
  import DdlConfirmDialog from './DdlConfirmDialog.svelte'
  import CreateTriggerDialog from './CreateTriggerDialog.svelte'
  import CreateSequenceDialog from './CreateSequenceDialog.svelte'
  import { engineSupports, unsupportedReason } from '$lib/db-capabilities.js'

  let {
    schema = '',
    /** @type {string | null} */
    connectionType = null,
    /** @type {{ name: string, tableName: string, columns?: string, isUnique?: boolean, isPrimary?: boolean, indexType?: string }[]} */
    indexes = [],
    /** @type {{ name: string, values: string[], usedInTables: string[] }[]} */
    enums = [],
    /** @type {{ name: string, tableName: string, timing: string, events: string, functionName: string, enabled: boolean }[]} */
    triggers = [],
    /** @type {{ name: string, dataType: string, startValue: number, minValue: number, maxValue: number, increment: number, cycle: boolean, ownedBy: string|null }[]} */
    sequences = [],
    /** @type {{ name: string, kind?: string }[]} */
    tables = [],
    loading = false,
    active = false,
    onrefresh = () => {},
    /** Opens SQL in a new editor tab (the create dialogs' "Open in SQL editor"). @type {((spec: { text: string, title: string, snippet?: string }) => void) | null} */
    onopensql = null,
  } = $props()

  /** @type {'indexes' | 'triggers' | 'sequences' | 'enums' | 'views' | 'matviews'} */
  let activeType = $state('indexes')
  let filter = $state('')
  let filterEl = $state(/** @type {HTMLInputElement | null} */ (null))

  const views = $derived(tables.filter((t) => t.kind === 'view'))
  const matViews = $derived(tables.filter((t) => t.kind === 'materialized_view'))
  const allTables = $derived(tables.filter((t) => !t.kind || t.kind === 'table'))

  const lf = $derived(filter.toLowerCase())

  const filteredIndexes = $derived(indexes.filter((i) => !lf || i.name.toLowerCase().includes(lf) || i.tableName.toLowerCase().includes(lf)))
  const filteredEnums = $derived(enums.filter((e) => !lf || e.name.toLowerCase().includes(lf)))
  const filteredViews = $derived(views.filter((v) => !lf || v.name.toLowerCase().includes(lf)))
  const filteredMatViews = $derived(matViews.filter((v) => !lf || v.name.toLowerCase().includes(lf)))
  const filteredTriggers = $derived(triggers.filter((t) => !lf || t.name.toLowerCase().includes(lf) || t.tableName.toLowerCase().includes(lf) || t.functionName.toLowerCase().includes(lf)))
  const filteredSequences = $derived(sequences.filter((s) => !lf || s.name.toLowerCase().includes(lf) || (s.ownedBy ?? '').toLowerCase().includes(lf)))

  const tabs = [
    { id: 'indexes',   label: 'Indexes',    icon: Zap,       count: () => indexes.length },
    { id: 'triggers',  label: 'Triggers',   icon: GitBranch, count: () => triggers.length },
    { id: 'sequences', label: 'Sequences',  icon: Hash,      count: () => sequences.length },
    { id: 'enums',     label: 'Enums',      icon: Tags,      count: () => enums.length },
    { id: 'views',     label: 'Views',      icon: Eye,       count: () => views.length },
    { id: 'matviews',  label: 'Mat. Views', icon: Layers,    count: () => matViews.length },
  ]

  // Maps each tab to its capability key + user-facing label, so a tab that the
  // current engine can't support shows a clear "requires PostgreSQL" note rather
  // than a misleading empty pane.
  const TAB_FEATURE = /** @type {Record<string, { key: string, label: string }>} */ ({
    indexes:   { key: 'indexes',   label: 'Indexes' },
    triggers:  { key: 'triggers',  label: 'Triggers' },
    sequences: { key: 'sequences', label: 'Sequences' },
    enums:     { key: 'enums',     label: 'Enums' },
    views:     { key: 'views',     label: 'Views' },
    matviews:  { key: 'matviews',  label: 'Materialized views' },
  })
  const activeFeature = $derived(TAB_FEATURE[activeType])
  const activeSupported = $derived(engineSupports(activeFeature.key, connectionType))

  // ── DDL confirm ────────────────────────────────────────────────────────────
  let confirmOpen = $state(false)
  let confirmLoading = $state(false)
  let confirmSql = $state('')
  let confirmTitle = $state('')
  let confirmDesc = $state('')
  let confirmAction = $state(/** @type {(() => Promise<void>) | null} */ (null))

  /** Disabled buttons are the hint; this is what actually refuses the write. */
  function guardWrite() {
    if (!$readOnlyMode) return true
    toast.warning('Read-only connection', { description: READ_ONLY_HINT, duration: 5000 })
    return false
  }

  function askDrop(title, desc, sql, action) {
    if (!guardWrite()) return
    confirmTitle = title
    confirmDesc = desc
    confirmSql = sql
    confirmAction = action
    confirmOpen = true
  }

  async function handleConfirm() {
    if (!confirmAction) return
    confirmLoading = true
    try {
      await confirmAction()
      confirmOpen = false
    } finally {
      confirmLoading = false
      confirmAction = null
    }
  }

  // ── Create dialogs ─────────────────────────────────────────────────────────
  let createTriggerOpen = $state(false)
  let createSequenceOpen = $state(false)

  // ── Drop helpers ───────────────────────────────────────────────────────────
  function dropTrigger(trig) {
    const sql = `DROP TRIGGER "${trig.name}" ON "${schema}"."${trig.tableName}";`
    askDrop(
      `Drop trigger "${trig.name}"`,
      `This will permanently remove the trigger from "${trig.tableName}".`,
      sql,
      async () => {
        await executeSql(sql)
        toast.success(`Trigger "${trig.name}" dropped`)
        onrefresh()
      }
    )
  }

  function dropSequence(seq) {
    const sql = `DROP SEQUENCE "${schema}"."${seq.name}";`
    askDrop(
      `Drop sequence "${seq.name}"`,
      seq.ownedBy
        ? `Owned by ${seq.ownedBy}. Dropping may affect dependent columns.`
        : `This will permanently remove the sequence.`,
      sql,
      async () => {
        await executeSql(sql)
        toast.success(`Sequence "${seq.name}" dropped`)
        onrefresh()
      }
    )
  }

  /** Format large numbers for sequence bounds */
  function fmtNum(n) {
    if (n >= 9e15)  return '∞'
    if (n <= -9e15) return '-∞'
    if (Math.abs(n) >= 1e9) return (n / 1e9).toFixed(1).replace(/\.0$/, '') + 'B'
    if (Math.abs(n) >= 1e6) return (n / 1e6).toFixed(1).replace(/\.0$/, '') + 'M'
    return n.toLocaleString()
  }
</script>

<svelte:window onkeydown={(e) => {
  if (!active) return
  if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key === 'r') {
    e.preventDefault(); onrefresh()
  }
  if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key === 'f') {
    e.preventDefault(); filterEl?.focus(); filterEl?.select()
  }
}} />

<div class="flex min-h-0 flex-1 flex-col overflow-hidden">

  <!-- Toolbar -->
  <div class="studio-chrome flex h-9 shrink-0 items-center gap-2 border-b border-border bg-panel px-3" data-studio-chrome>
    <span class="text-ui-sm font-medium">Schema explorer</span>
    <div class="ml-auto flex items-center gap-1">
      {#if activeType === 'triggers' && activeSupported}
        <Button variant="ghost" size="sm" class="h-7 gap-1.5 px-2 text-muted-foreground hover:text-foreground" disabled={$readOnlyMode} title={$readOnlyMode ? READ_ONLY_HINT : ''} onclick={() => (createTriggerOpen = true)}>
          <Plus class="size-3.5 shrink-0" /><span class="text-ui-xs">New trigger</span>
        </Button>
      {:else if activeType === 'sequences' && activeSupported}
        <Button variant="ghost" size="sm" class="h-7 gap-1.5 px-2 text-muted-foreground hover:text-foreground" disabled={$readOnlyMode} title={$readOnlyMode ? READ_ONLY_HINT : ''} onclick={() => (createSequenceOpen = true)}>
          <Plus class="size-3.5 shrink-0" /><span class="text-ui-xs">New sequence</span>
        </Button>
      {/if}
      <Button variant="ghost" size="sm" class="h-7 gap-1.5 px-2 text-muted-foreground hover:text-foreground" disabled={loading} title="Refresh" onclick={onrefresh}>
        <RefreshCw class={cn('size-3.5', loading && 'animate-spin')} />
        <span class="text-ui-xs">Refresh</span>
      </Button>
    </div>
  </div>

  <!-- Tab bar + filter -->
  <div class="flex shrink-0 items-center gap-3 border-b border-border bg-panel/60 px-3 py-2">
    <div class= "field-surface app-scroll inline-flex h-7 shrink-0 items-center overflow-x-auto bg-muted/40 p-0.5 ring-1 ring-inset ring-border/40">
      {#each tabs as tab (tab.id)}
        {@const Icon = tab.icon}
        {@const cnt = tab.count()}
        <button
          type="button"
          class={cn(
            'inline-flex h-6 shrink-0 items-center gap-1 rounded-sm px-2.5 text-ui-2xs font-medium transition-colors',
            activeType === tab.id
              ? 'bg-card text-foreground ring-1 ring-border/50'
              : 'text-muted-foreground hover:text-foreground',
          )}
          onclick={() => { activeType = /** @type {any} */ (tab.id); filter = '' }}
        >
          <Icon class="size-3.5 shrink-0" />
          {tab.label}
          {#if cnt > 0}
            <span class="rounded-full bg-muted/80 px-1 font-mono text-ui-3xs tabular-nums {activeType === tab.id ? 'text-foreground/60' : 'text-muted-foreground'}">{cnt}</span>
          {/if}
        </button>
      {/each}
    </div>

    <div class="relative min-w-0 flex-1">
      <Search class="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
      <input
        type="search"
        placeholder="Filter…"
        bind:this={filterEl}
        bind:value={filter}
        class= "field-surface h-7 w-full bg-background/40 pl-7 pr-2.5 text-ui-xs text-foreground outline-none transition-colors hover:bg-background/60"
      />
    </div>
  </div>

  <!-- Content -->
  <div class="app-scroll min-h-0 flex-1 overflow-y-auto [will-change:transform]">

    <!-- Engine doesn't support the active section, explain why instead of a blank pane. -->
    {#if !activeSupported}
      {@render unsupported(activeFeature.label, activeFeature.key)}

    <!-- ── Indexes ───────────────────────────────────────────────────────── -->
    {:else if activeType === 'indexes'}
      {#if indexes.length === 0}
        {@render emptyState(Zap, 'No indexes', 'Indexes in this schema will appear here')}
      {:else if filteredIndexes.length === 0}
        {@render noMatch('indexes', filter)}
      {:else}
        <table class="border-collapse" style="table-layout:fixed;width:max-content;min-width:100%">
          <colgroup>
            <col style="min-width:200px;width:220px" />
            <col style="min-width:180px;width:200px" />
            <col style="min-width:90px;width:100px" />
            <col style="min-width:60px;width:70px" />
            <col style="min-width:160px;width:200px" />
            <col style="min-width:140px;width:180px" />
          </colgroup>
          <thead>
            <tr>
              {#each ['index_name','table','algorithm','unique','columns','condition'] as h}
                <th class="border-b border-r border-border bg-panel/80 px-3 py-2 text-left font-medium text-muted-foreground text-ui-2xs uppercase tracking-[0.06em] whitespace-nowrap">{h}</th>
              {/each}
            </tr>
          </thead>
          <tbody>
            {#each filteredIndexes as idx (idx.name)}
              <tr class="group/row h-9">
                <td class="border-b border-r border-border/40 px-3 font-mono text-ui-sm font-medium text-foreground">
                  <div class="flex items-center gap-1.5">
                    {idx.name}
                    {#if idx.isPrimary}<span class="rounded bg-primary/10 px-1 text-ui-3xs text-primary">PK</span>
                    {:else if idx.isUnique}<span class="rounded bg-muted px-1 text-ui-3xs text-muted-foreground">UNIQUE</span>{/if}
                  </div>
                </td>
                <td class="border-b border-r border-border/40 px-3 font-mono text-ui-sm text-muted-foreground">{idx.tableName}</td>
                <td class="border-b border-r border-border/40 px-3 font-mono text-ui-sm uppercase text-muted-foreground">{idx.indexType || 'btree'}</td>
                <td class="border-b border-r border-border/40 px-3 font-mono text-ui-xs {idx.isUnique ? 'text-success' : 'text-muted-foreground'}">{idx.isUnique ? 'TRUE' : 'FALSE'}</td>
                <td class="border-b border-r border-border/40 px-3 font-mono text-ui-xs text-muted-foreground overflow-hidden"><span class="block truncate">{idx.columns || '—'}</span></td>
                <td class="border-b border-border/40 px-3 font-mono text-ui-xs text-muted-foreground overflow-hidden"><span class="block truncate">{idx.condition ?? '—'}</span></td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}

    <!-- ── Triggers ──────────────────────────────────────────────────────── -->
    {:else if activeType === 'triggers'}
      {#if triggers.length === 0}
        {@render emptyState(GitBranch, 'No triggers', 'Triggers defined in this schema will appear here')}
      {:else if filteredTriggers.length === 0}
        {@render noMatch('triggers', filter)}
      {:else}
        <table class="border-collapse" style="table-layout:fixed;width:max-content;min-width:100%">
          <colgroup>
            <col style="min-width:180px;width:210px" />
            <col style="min-width:160px;width:190px" />
            <col style="min-width:90px;width:100px" />
            <col style="min-width:180px;width:220px" />
            <col style="min-width:180px;width:210px" />
            <col style="min-width:72px;width:72px" />
            <col style="min-width:36px;width:36px;max-width:36px" />
          </colgroup>
          <thead>
            <tr>
              {#each ['trigger_name','table','timing','events','function','status',''] as h}
                <th class="border-b border-r border-border bg-panel/80 px-3 py-2 text-left font-medium text-muted-foreground text-ui-2xs uppercase tracking-[0.06em] whitespace-nowrap">{h}</th>
              {/each}
            </tr>
          </thead>
          <tbody>
            {#each filteredTriggers as trig (`${trig.name}:${trig.tableName}`)}
              <tr class="group/row h-9">
                <td class="border-b border-r border-border/40 px-3 font-mono text-ui-sm font-medium text-foreground overflow-hidden">
                  <span class="block truncate">{trig.name}</span>
                </td>
                <td class="border-b border-r border-border/40 px-3 font-mono text-ui-sm text-muted-foreground overflow-hidden">
                  <span class="block truncate">{trig.tableName}</span>
                </td>
                <td class="border-b border-r border-border/40 px-3">
                  <span class="rounded-sm bg-muted/60 px-1.5 py-0.5 font-mono text-ui-3xs text-muted-foreground">{trig.timing}</span>
                </td>
                <td class="border-b border-r border-border/40 px-3 overflow-hidden">
                  <div class="flex flex-wrap gap-1 overflow-hidden">
                    {#each trig.events.split(', ').filter(Boolean) as ev (ev)}
                      <span class="shrink-0 rounded bg-primary/8 px-1.5 py-0.5 font-mono text-ui-3xs text-primary">{ev}</span>
                    {/each}
                  </div>
                </td>
                <td class="border-b border-r border-border/40 px-3 font-mono text-ui-xs text-muted-foreground overflow-hidden">
                  <span class="block truncate">{trig.functionName}()</span>
                </td>
                <td class="border-b border-r border-border/40 px-3">
                  {#if trig.enabled}
                    <span class="flex items-center gap-1 text-ui-3xs text-success">
                      <span class="size-1.5 rounded-full bg-success"></span>enabled
                    </span>
                  {:else}
                    <span class="flex items-center gap-1 font-mono text-ui-3xs text-muted-foreground">
                      <span class="size-1.5 rounded-full bg-muted-foreground/30"></span>disabled
                    </span>
                  {/if}
                </td>
                <td class="border-b border-border/40 p-0 align-middle">
                  <button
                    type="button"
                    class="flex h-full w-full items-center justify-center text-muted-foreground transition-colors enabled:hover:text-destructive enabled:group-hover/row:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-40"
                    disabled={$readOnlyMode}
                    title={$readOnlyMode ? READ_ONLY_HINT : 'Drop trigger'}
                    onclick={() => dropTrigger(trig)}
                  ><Trash2 class="size-3.5 shrink-0" /></button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}

    <!-- ── Sequences ─────────────────────────────────────────────────────── -->
    {:else if activeType === 'sequences'}
      {#if sequences.length === 0}
        {@render emptyState(Hash, 'No sequences', 'Sequences defined in this schema will appear here')}
      {:else if filteredSequences.length === 0}
        {@render noMatch('sequences', filter)}
      {:else}
        <table class="border-collapse" style="table-layout:fixed;width:max-content;min-width:100%">
          <colgroup>
            <col style="min-width:180px;width:210px" />
            <col style="min-width:80px;width:90px" />
            <col style="min-width:80px;width:90px" />
            <col style="min-width:80px;width:100px" />
            <col style="min-width:130px;width:160px" />
            <col style="min-width:80px;width:90px" />
            <col style="min-width:60px;width:60px" />
            <col style="min-width:180px;width:200px" />
            <col style="min-width:36px;width:36px;max-width:36px" />
          </colgroup>
          <thead>
            <tr>
              {#each ['sequence_name','data_type','start','increment','range','cycle','owned_by',''] as h}
                <th class="border-b border-r border-border bg-panel/80 px-3 py-2 text-left font-medium text-muted-foreground text-ui-2xs uppercase tracking-[0.06em] whitespace-nowrap last:border-r-0">{h}</th>
              {/each}
            </tr>
          </thead>
          <tbody>
            {#each filteredSequences as seq (seq.name)}
              <tr class="group/row h-9">
                <td class="border-b border-r border-border/40 px-3 font-mono text-ui-sm font-medium text-foreground overflow-hidden">
                  <span class="block truncate">{seq.name}</span>
                </td>
                <td class="border-b border-r border-border/40 px-3 font-mono text-ui-xs text-muted-foreground">{seq.dataType}</td>
                <td class="border-b border-r border-border/40 px-3 font-mono text-ui-xs tabular-nums text-foreground/70">{fmtNum(seq.startValue)}</td>
                <td class="border-b border-r border-border/40 px-3 font-mono text-ui-xs tabular-nums text-foreground/70">{fmtNum(seq.increment)}</td>
                <td class="border-b border-r border-border/40 px-3 font-mono text-ui-xs tabular-nums text-muted-foreground">
                  {fmtNum(seq.minValue)} … {fmtNum(seq.maxValue)}
                </td>
                <td class="border-b border-r border-border/40 px-3">
                  {#if seq.cycle}
                    <span class="rounded bg-primary/8 px-1.5 py-0.5 font-mono text-ui-3xs text-primary">YES</span>
                  {:else}
                    <span class="font-mono text-ui-3xs text-muted-foreground">NO</span>
                  {/if}
                </td>
                <td class="border-b border-r border-border/40 px-3 font-mono text-ui-xs text-muted-foreground overflow-hidden">
                  <span class="block truncate">{seq.ownedBy ?? '—'}</span>
                </td>
                <td class="border-b border-border/40 p-0 align-middle">
                  <button
                    type="button"
                    class="flex h-full w-full items-center justify-center text-muted-foreground transition-colors enabled:hover:text-destructive enabled:group-hover/row:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-40"
                    disabled={$readOnlyMode}
                    title={$readOnlyMode ? READ_ONLY_HINT : 'Drop sequence'}
                    onclick={() => dropSequence(seq)}
                  ><Trash2 class="size-3.5 shrink-0" /></button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}

    <!-- ── Enums ──────────────────────────────────────────────────────────── -->
    {:else if activeType === 'enums'}
      {#if enums.length === 0}
        {@render emptyState(Tags, 'No enum types', 'Enum types defined in this schema will appear here')}
      {:else if filteredEnums.length === 0}
        {@render noMatch('enums', filter)}
      {:else}
        <table class="w-full text-ui-xs">
          <thead class="studio-chrome sticky top-0 z-10 bg-panel text-left">
            <tr class="border-b border-border/50">
              <th class="w-56 px-3 py-2 font-normal text-muted-foreground">Name</th>
              <th class="px-3 py-2 font-normal text-muted-foreground">Values</th>
              <th class="w-52 px-3 py-2 font-normal text-muted-foreground">Used by</th>
              <th class="w-10 px-3 py-2 text-right font-normal text-muted-foreground">n</th>
            </tr>
          </thead>
          <tbody>
            {#each filteredEnums as e (e.name)}
              <tr class="group border-b border-border/25 align-top hover:bg-accent/10">
                <td class="px-3 py-2.5">
                  <span class="font-mono text-ui-xs font-medium text-foreground/90">{e.name}</span>
                </td>
                <td class="px-3 py-2.5">
                  <div class="flex flex-wrap gap-x-2 gap-y-1">
                    {#each e.values as val (val)}
                      <span class="font-mono text-ui-xs text-muted-foreground">{val}</span>
                    {/each}
                    {#if e.values.length === 0}
                      <span class="font-mono text-ui-xs text-muted-foreground">—</span>
                    {/if}
                  </div>
                </td>
                <td class="px-3 py-2.5">
                  {#if (e.usedInTables ?? []).length > 0}
                    <div class="flex flex-wrap gap-1">
                      {#each e.usedInTables as tbl (tbl)}
                        <span class="rounded-sm bg-muted/40 px-1.5 py-px font-mono text-ui-3xs text-foreground/60">{tbl}</span>
                      {/each}
                    </div>
                  {:else}
                    <span class="text-ui-xs text-muted-foreground">unused</span>
                  {/if}
                </td>
                <td class="px-3 py-2.5 text-right font-mono text-ui-xs tabular-nums text-muted-foreground">{e.values.length}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}

    <!-- ── Views ─────────────────────────────────────────────────────────── -->
    {:else if activeType === 'views'}
      {#if views.length === 0}
        {@render emptyState(Eye, 'No views', 'Views in this schema will appear here')}
      {:else if filteredViews.length === 0}
        {@render noMatch('views', filter)}
      {:else}
        <table class="border-collapse" style="table-layout:fixed;width:max-content;min-width:100%">
          <thead>
            <tr>
              <th class="border-b border-r border-border bg-panel/80 px-3 py-2 text-left font-medium text-muted-foreground text-ui-2xs uppercase tracking-[0.06em]">view_name</th>
            </tr>
          </thead>
          <tbody>
            {#each filteredViews as v (v.name)}
              <tr class="h-9">
                <td class="border-b border-border/40 px-3 font-mono text-ui-sm text-foreground">{v.name}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}

    <!-- ── Materialized Views ─────────────────────────────────────────────── -->
    {:else if activeType === 'matviews'}
      {#if matViews.length === 0}
        {@render emptyState(Layers, 'No materialized views', 'Materialized views in this schema will appear here')}
      {:else if filteredMatViews.length === 0}
        {@render noMatch('materialized views', filter)}
      {:else}
        <table class="border-collapse" style="table-layout:fixed;width:max-content;min-width:100%">
          <thead>
            <tr>
              <th class="border-b border-r border-border bg-panel/80 px-3 py-2 text-left font-medium text-muted-foreground text-ui-2xs uppercase tracking-[0.06em]">view_name</th>
              <th class="border-b border-border bg-panel/80 px-3 py-2 text-left font-medium text-muted-foreground text-ui-2xs uppercase tracking-[0.06em]">rows</th>
            </tr>
          </thead>
          <tbody>
            {#each filteredMatViews as mv (mv.name)}
              <tr class="h-9">
                <td class="border-b border-r border-border/40 px-3 font-mono text-ui-sm text-foreground">{mv.name}</td>
                <td class="border-b border-border/40 px-3 font-mono text-ui-xs tabular-nums text-muted-foreground">{mv.rowCount != null ? mv.rowCount.toLocaleString() : '—'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    {/if}

  </div>
</div>

<!-- Snippets -->
{#snippet emptyState(Icon, label, sub)}
  <div class="flex h-full flex-col items-center justify-center gap-3 p-12 text-center">
    <Icon class="size-10 shrink-0 text-muted-foreground" />
    <div>
      <p class="text-ui text-muted-foreground">{label}</p>
      <p class="mt-1 text-ui-xs text-muted-foreground">{sub}</p>
    </div>
  </div>
{/snippet}

{#snippet noMatch(type, q)}
  <p class="py-10 text-center text-ui-xs text-muted-foreground">No {type} match "{q}"</p>
{/snippet}

{#snippet unsupported(label, key)}
  <div class="flex h-full flex-col items-center justify-center gap-3 p-12 text-center">
    <Lock class="size-9 shrink-0 text-muted-foreground" />
    <div>
      <p class="text-ui text-muted-foreground">Not available on this database</p>
      <p class="mx-auto mt-1 max-w-xs text-ui-xs text-muted-foreground">{unsupportedReason(label, key)}</p>
    </div>
  </div>
{/snippet}

<!-- Dialogs -->
<DdlConfirmDialog
  bind:open={confirmOpen}
  title={confirmTitle}
  description={confirmDesc}
  sql={confirmSql}
  variant="destructive"
  confirmLabel="Drop"
  loading={confirmLoading}
  onconfirm={handleConfirm}
/>

<CreateTriggerDialog
  bind:open={createTriggerOpen}
  {schema}
  tables={allTables}
  onrefresh={onrefresh}
  {onopensql}
/>

<CreateSequenceDialog
  bind:open={createSequenceOpen}
  {schema}
  tables={allTables}
  onrefresh={onrefresh}
  {onopensql}
/>
