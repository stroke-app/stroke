<script>
  /**
   * The Queries tab: what ran, what was saved, and the saved charts, for this
   * connection. These lived as buttons beside the SQL console's result views
   * and took the results pane over when opened; here they have a page of
   * their own. Picking a query opens it in the SQL editor.
   */
  import QueryHistoryPanel from './QueryHistoryPanel.svelte'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'

  let {
    /** @type {'history' | 'saved' | 'charts'} */
    view = $bindable('history'),
    /** @type {import('$lib/stores/query-history.js').QueryHistoryEntry[]} */
    history = [],
    /** @type {import('$lib/stores/query-history.js').SavedQuery[]} */
    saved = [],
    connectionName = '',
    /** Open a query in the SQL editor. @param {string} sql */
    onopen = (sql) => {},
    /** Open a query in the SQL editor and run it. @param {string} sql */
    onrun = (sql) => {},
    onrefresh = async () => {},
  } = $props()

  const VIEWS = /** @type {const} */ ([
    { id: 'history', label: 'History', icon: 'history', hint: 'Every query run on this connection, from any editor tab' },
    { id: 'saved', label: 'Saved', icon: 'bookmark', hint: 'Queries saved on this connection' },
    { id: 'charts', label: 'Charts', icon: 'bar-chart-3', hint: 'Charts saved on this connection' },
  ])
  const counts = $derived({ history: history.length, saved: saved.length, charts: null })
</script>

<div class="flex min-h-0 flex-1 flex-col bg-panel">
  <header class="flex shrink-0 items-center gap-4 border-b border-border/60 px-5 pt-4 pb-3">
    <div class="min-w-0">
      <h1 class="text-ui-lg font-semibold tracking-tight text-foreground">Queries</h1>
      {#if connectionName}<p class="truncate text-ui-xs text-muted-foreground">{connectionName}</p>{/if}
    </div>
    <div role="tablist" aria-label="Queries" class="ms-auto flex items-center gap-0.5 rounded-lg bg-muted/50 p-0.5">
      {#each VIEWS as v (v.id)}
        {@const on = view === v.id}
        <button
          type="button"
          role="tab"
          aria-selected={on}
          title={v.hint}
          onclick={() => (view = v.id)}
          class={cn(
            'flex h-7 items-center gap-1.5 rounded-md px-3 text-ui-xs font-medium transition-colors',
            on ? 'bg-background text-foreground elevate-1' : 'text-muted-foreground hover:text-foreground',
          )}
        >
          <Icon name={v.icon} class="size-3.5 shrink-0" />
          {v.label}
          {#if counts[v.id] != null}
            <span class="font-mono text-ui-3xs tabular-nums text-muted-foreground">{counts[v.id]}</span>
          {/if}
        </button>
      {/each}
    </div>
  </header>
  {#key view}
    <QueryHistoryPanel
      page
      {view}
      {history}
      {saved}
      onselect={(sql) => onopen(sql)}
      onrun={(sql) => onrun(sql)}
      {onrefresh}
    />
  {/key}
</div>
