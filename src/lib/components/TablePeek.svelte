<script module>
  /**
   * Column lists already fetched, by `schema.table`, for the life of the page:
   * peeking the same table again, or arrowing back to it, paints at once.
   * In-flight requests are shared, so a quick arrow past a row and back makes
   * one call. Cleared when the connection changes (`clearPeekCache`).
   * @type {Map<string, Promise<any[]>>}
   */
  const cache = new Map()
  export function clearPeekCache() { cache.clear() }
</script>

<script>
  /**
   * A table's columns, shown beside its sidebar row without opening it: Space
   * on a focused row (Linear's peek). The row keeps focus, so the arrows move
   * the peek down the list and Enter opens the table being looked at.
   */
  import { getTableColumnStructure } from '$lib/api.js'
  import Icon from './Icon.svelte'
  import Kbd from './Kbd.svelte'

  let {
    schema = '',
    table = '',
    /** Formatted row count, '' when unknown. */
    rows = '',
    /** Viewport position of the row's right edge and top. */
    x = 0,
    y = 0,
  } = $props()

  /** @type {any[] | null} */
  let columns = $state(null)
  let error = $state('')
  /** @type {HTMLDivElement | null} */
  let el = $state(null)
  let top = $state(0)

  $effect(() => {
    const key = `${schema}.${table}`
    columns = null
    error = ''
    let p = cache.get(key)
    if (!p) {
      p = Promise.resolve(getTableColumnStructure(schema, table))
      cache.set(key, p)
      p.catch(() => cache.delete(key))
    }
    let live = true
    p.then((c) => { if (live) columns = Array.isArray(c) ? c : [] }, (e) => { if (live) error = String(e?.message ?? e) })
    return () => { live = false }
  })

  // Kept inside the window: a row near the bottom would put the card past it.
  $effect(() => {
    void columns
    const h = el?.offsetHeight ?? 0
    top = Math.max(8, Math.min(y - 4, window.innerHeight - h - 8))
  })

  /** "8 columns · 26 rows": built as one string, since a template block trims the spaces around the dot. */
  const meta = $derived([
    columns ? `${columns.length} column${columns.length === 1 ? '' : 's'}` : '',
    rows ? `${rows} rows` : '',
  ].filter(Boolean).join(' · '))

  /** Skeleton rows while the columns load: the card opens at its final width and does not jump. */
  const SKELETON = [62, 48, 70, 40, 56]
</script>

<div
  bind:this={el}
  class="fixed z-[90] flex max-h-[min(28rem,calc(100vh-1rem))] w-72 flex-col overflow-hidden rounded-menu border border-border/60 bg-popover text-ui-xs elevate-2-rim"
  style:left="{x}px"
  style:top="{top}px"
  role="dialog"
  aria-label="Columns of {table}"
  data-surface-in
>
  <!-- The name over the counts: side by side, the counts kept their width and
       "application_forms" was cut to "application...". -->
  <div class="flex min-w-0 flex-col gap-0.5 px-3 pt-2.5 pb-2">
    <p class="min-w-0 truncate font-medium text-foreground" title={table}>{table}</p>
    <p class="font-mono text-ui-2xs tabular-nums text-muted-foreground">
      {meta}
    </p>
  </div>
  <div class="app-scroll min-h-0 flex-1 overflow-y-auto border-t border-border/50 px-1 py-1">
    {#if error}
      <p class="px-2 py-2 text-muted-foreground">Couldn't read the columns: {error}</p>
    {:else if !columns}
      {#each SKELETON as w, i (i)}
        <div class="flex h-6 items-center gap-2 px-2" aria-hidden="true">
          <span class="h-2 rounded-full bg-muted-foreground/15" style:width="{w}%"></span>
        </div>
      {/each}
    {:else if columns.length === 0}
      <p class="px-2 py-2 text-muted-foreground">No columns</p>
    {:else}
      {#each columns as c (c.name)}
        <div class="flex h-6 min-w-0 items-center gap-2 rounded-sm px-2">
          {#if c.isPrimaryKey}
            <Icon name="key-round" class="size-3 shrink-0 text-warning" aria-label="Primary key" />
          {:else if c.foreignKey}
            <Icon name="link" class="size-3 shrink-0 text-muted-foreground" aria-label="Foreign key" />
          {:else}
            <span class="size-3 shrink-0" aria-hidden="true"></span>
          {/if}
          <span class="min-w-0 flex-1 truncate font-mono text-foreground">{c.name}</span>
          <span class="max-w-[45%] shrink-0 truncate font-mono text-ui-2xs text-muted-foreground" title={c.dataType}>{c.dataType}</span>
        </div>
      {/each}
    {/if}
  </div>
  <div class="flex items-center gap-3 border-t border-border/50 px-3 py-1.5 text-ui-2xs text-muted-foreground">
    <span class="flex items-center gap-1"><Kbd combo="Enter" /> open</span>
    <span class="flex items-center gap-1"><Kbd combo="Up" /><Kbd combo="Down" /> move</span>
    <span class="ml-auto flex items-center gap-1"><Kbd combo="Space" /> close</span>
  </div>
</div>
