<script>
  // The collection's shape from a sample: one soft row per field path with a
  // type bar, presence, and (expanded) its frequent values and range.
  import { cn } from '$lib/utils.js'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import ChevronRight from '@lucide/svelte/icons/chevron-right'

  /**
   * @type {{
   *   adapter: import('$lib/doc-adapters.js').DocAdapter,
   *   coll: string,
   *   filter?: string,
   *   onfields?: (paths: string[]) => void,
   * }}
   */
  let { adapter, coll, filter = '', onfields = () => {} } = $props()

  const SIZES = [100, 1000, 5000]
  let size = $state(1000)
  let data = $state.raw(/** @type {import('$lib/doc-adapters.js').MongoSchema | null} */ (null))
  let error = $state('')
  let loading = $state(false)
  let open = $state(/** @type {Set<string>} */ (new Set()))
  let token = 0

  async function run() {
    if (!adapter.schema) return
    const t = ++token
    loading = true
    error = ''
    try {
      const d = await adapter.schema(coll, filter, size)
      if (t !== token) return
      data = d
      onfields(d.fields.filter((f) => !f.path.includes('[]')).map((f) => f.path))
    } catch (e) {
      if (t === token) error = e instanceof Error ? e.message : String(e)
    } finally {
      if (t === token) loading = false
    }
  }

  $effect(() => {
    void [coll, filter, size]
    open = new Set()
    void run()
  })

  /** @param {string} p */
  function toggle(p) {
    const next = new Set(open)
    if (next.has(p)) next.delete(p)
    else next.add(p)
    open = next
  }

  /** @type {Record<string, string>} */
  const TYPE_COLOR = {
    string: 'bg-info/70',
    int: 'bg-success/70',
    long: 'bg-success/50',
    double: 'bg-success/60',
    decimal: 'bg-success/40',
    date: 'bg-warning/70',
    objectId: 'bg-primary/60',
    bool: 'bg-link/60',
    null: 'bg-muted-foreground/30',
    object: 'bg-foreground/25',
    array: 'bg-foreground/40',
  }
  /** @param {string} t */
  const color = (t) => TYPE_COLOR[t] ?? 'bg-destructive/50'

  /** @param {any} v */
  function show(v) {
    if (v === null || v === undefined) return 'null'
    if (typeof v === 'object') {
      const k = Object.keys(v)
      if (k.length === 1 && k[0].startsWith('$') && typeof v[k[0]] !== 'object') return String(v[k[0]])
      return JSON.stringify(v)
    }
    return typeof v === 'string' ? JSON.stringify(v) : String(v)
  }
  /** @param {number} p */
  const pct = (p) => (p >= 0.995 ? '100%' : p < 0.01 && p > 0 ? '<1%' : `${Math.round(p * 100)}%`)
  /** Depth from dots and `[]`, for a gentle indent. @param {string} p */
  const depth = (p) => (p.match(/\.|\[\]/g) ?? []).length
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div class="flex h-10 shrink-0 items-center gap-2 pl-4 pr-2 text-ui-xs text-muted-foreground">
    {#if data}
      <span class="tabular-nums">Sampled <span class="text-foreground">{data.sampled.toLocaleString()}</span> of {data.total.toLocaleString()}{filter ? ' matching' : ''}</span>
      <span>· {data.fields.length.toLocaleString()} fields</span>
    {/if}
    <span class="flex-1"></span>
    <div class="flex items-center" role="group" aria-label="Sample size">
      {#each SIZES as n (n)}
        <button type="button" aria-pressed={size === n} class={cn('inline-flex h-7 items-center rounded-md px-2.5 text-ui-xs tabular-nums transition-colors', size === n ? 'bg-accent text-foreground' : 'hover:bg-accent/50 hover:text-foreground')} onclick={() => (size = n)}>{n.toLocaleString()}</button>
      {/each}
    </div>
    <button type="button" class="inline-flex size-7 shrink-0 items-center justify-center rounded-md transition-colors hover:bg-accent hover:text-foreground disabled:opacity-40" onclick={() => void run()} disabled={loading} title="Sample again" aria-label="Sample again">
      <RefreshCw class={cn('size-3.5 shrink-0', loading && 'animate-spin')} />
    </button>
  </div>

  <div class="app-scroll min-h-0 flex-1 overflow-auto px-2 pb-4">
    {#if error}
      <p class="mx-2 rounded-lg bg-destructive/5 px-3 py-2 text-ui-2xs text-destructive">{error}</p>
    {:else if !data}
      <div class="flex items-center gap-2 px-2 py-6 text-ui-xs text-muted-foreground"><LoaderCircle class="size-3.5 shrink-0 animate-spin" /> Sampling…</div>
    {:else if !data.fields.length}
      <p class="px-2 py-6 text-ui-xs text-muted-foreground">No documents to sample</p>
    {:else}
      <!-- A table, not bars: field, type, how often it is present, and what is in it. -->
      <div class={cn('min-w-[40rem] transition-opacity', loading && 'opacity-60')} role="table" aria-label="Fields">
        <div class="sticky top-0 z-10 grid grid-cols-[minmax(10rem,1.2fr)_minmax(8rem,1fr)_5rem_minmax(10rem,2fr)] items-center gap-4 bg-panel px-3 py-2 text-ui-xs text-muted-foreground" role="row">
          <span>Field</span><span>Type</span><span class="text-right">Present</span><span>Values</span>
        </div>
        {#each data.fields as f (f.path)}
          {@const expanded = open.has(f.path)}
          {@const sum = f.types.reduce((a, t) => a + t[1], 0) || 1}
          {@const d = Math.min(depth(f.path), 4)}
          {@const cut = Math.max(f.path.lastIndexOf('.'), f.path.lastIndexOf('[]') >= 0 && f.path.endsWith('[]') ? -1 : -1)}
          {@const parent = cut > 0 ? f.path.slice(0, cut + 1) : ''}
          {@const leaf = cut > 0 ? f.path.slice(cut + 1) : f.path}
          <div role="rowgroup">
            <button
              type="button"
              role="row"
              class={cn('grid w-full grid-cols-[minmax(10rem,1.2fr)_minmax(8rem,1fr)_5rem_minmax(10rem,2fr)] items-center gap-4 rounded-lg px-3 py-2 text-left transition-colors hover:bg-muted/40', expanded && 'bg-muted/30')}
              onclick={() => toggle(f.path)}
              aria-expanded={expanded}
            >
              <span class="flex min-w-0 items-baseline font-mono text-ui-xs" style:padding-left="{d * 0.9}rem" title={f.path}>
                {#if parent}<span class="shrink-0 text-muted-foreground/60">{parent}</span>{/if}<span class="truncate text-foreground">{leaf}</span>
              </span>
              <span class="flex min-w-0 flex-wrap gap-1">
                {#each f.types as [t, n] (t)}
                  <span class="inline-flex h-6 items-center gap-1.5 rounded-full bg-muted/40 px-2 text-ui-2xs text-foreground/85">
                    <span class={cn('size-1.5 shrink-0 rounded-full', color(t))}></span>{t}{#if f.types.length > 1}<span class="tabular-nums text-muted-foreground">{Math.round((n / sum) * 100)}%</span>{/if}
                  </span>
                {/each}
              </span>
              <span class={cn('text-right text-ui-xs tabular-nums', f.presence < 0.995 ? 'text-warning' : 'text-muted-foreground')} title="{f.count.toLocaleString()} of {data.sampled.toLocaleString()} sampled documents">{pct(f.presence)}</span>
              <span class="min-w-0 truncate font-mono text-ui-2xs text-muted-foreground">
                {#if f.min != null || f.max != null}{show(f.min)} – {show(f.max)}
                {:else if f.top.length}{f.top.slice(0, 4).map(([v]) => show(v)).join(' · ')}{#if f.distinct > 4} · {(f.distinct - 4).toLocaleString()}{f.distinctCapped ? '+' : ''} more{/if}
                {/if}
              </span>
            </button>
            {#if expanded && f.top.length}
              {@const max = f.top[0][1] || 1}
              <div class="mb-2 ml-3 mr-3 grid grid-cols-[minmax(10rem,1.2fr)_minmax(0,3fr)] gap-4 pt-1">
                <span class="text-ui-2xs text-muted-foreground" style:padding-left="{d * 0.9}rem">{f.distinct.toLocaleString()}{f.distinctCapped ? '+' : ''} distinct</span>
                <ul class="flex min-w-0 flex-col gap-1">
                  {#each f.top as [v, n], i (i)}
                    <li class="flex min-w-0 items-center gap-3 text-ui-2xs">
                      <span class="w-40 shrink-0 truncate font-mono text-foreground/85" title={show(v)}>{show(v)}</span>
                      <span class="h-1 min-w-0 flex-1 overflow-hidden rounded-full bg-muted/50"><span class="block h-full rounded-full bg-foreground/30" style:width="{(n / max) * 100}%"></span></span>
                      <span class="w-14 shrink-0 text-right tabular-nums text-muted-foreground">{n.toLocaleString()}</span>
                    </li>
                  {/each}
                </ul>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
