<script>
  // The query plan for the browser's current find: a verdict line, the totals,
  // and the winning stage tree as nested rows. Raw explain output on request.
  import { cn } from '$lib/utils.js'
  import X from '@lucide/svelte/icons/x'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import LoaderCircle from '@lucide/svelte/icons/loader-circle'
  import Zap from '@lucide/svelte/icons/zap'
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert'

  /**
   * @type {{
   *   adapter: import('$lib/doc-adapters.js').DocAdapter,
   *   coll: string,
   *   filter: string,
   *   sort?: string,
   *   projection?: string,
   *   onclose?: () => void,
   * }}
   */
  let { adapter, coll, filter, sort = '', projection = '', onclose = () => {} } = $props()

  let plan = $state.raw(/** @type {import('$lib/doc-adapters.js').MongoExplain | null} */ (null))
  let error = $state('')
  let loading = $state(false)
  let showRaw = $state(false)
  let token = 0

  async function run() {
    if (!adapter.explain) return
    const t = ++token
    loading = true
    error = ''
    try {
      const p = await adapter.explain(coll, { filter, sort, projection })
      if (t === token) plan = p
    } catch (e) {
      if (t === token) error = e instanceof Error ? e.message : String(e)
    } finally {
      if (t === token) loading = false
    }
  }

  // Re-explain when the applied query changes.
  $effect(() => {
    void [coll, filter, sort, projection]
    void run()
  })

  const totals = $derived(plan ? [
    { label: 'Returned', n: plan.nReturned },
    { label: 'Keys examined', n: plan.keysExamined },
    { label: 'Docs examined', n: plan.docsExamined },
    { label: 'Time (ms)', n: plan.ms },
  ] : [])
  /** @param {number | null | undefined} n */
  const num = (n) => (n == null ? '—' : n.toLocaleString())
  /** @param {any} kp */
  const keyText = (kp) => (kp ? Object.entries(kp).map(([k, v]) => `${k} ${v === -1 ? '↓' : v === 1 ? '↑' : v}`).join(', ') : '')
  const iconBtn = 'inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-[color,background-color,scale] duration-[120ms] ease-[var(--ease-out)] active:scale-[0.97] hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-40'
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div class="flex h-9 shrink-0 items-center gap-2 pl-3 pr-1">
    <span class="min-w-0 flex-1 truncate text-ui-xs font-medium text-foreground">Query plan</span>
    <button type="button" class={iconBtn} onclick={() => void run()} disabled={loading} title="Explain again" aria-label="Explain again">
      <RefreshCw class={cn('size-3.5 shrink-0', loading && 'animate-spin')} />
    </button>
    <button type="button" class={iconBtn} onclick={onclose} title="Close" aria-label="Close the query plan"><X class="size-3.5 shrink-0" /></button>
  </div>

  <div class="app-scroll min-h-0 flex-1 overflow-auto px-3 pb-3">
    {#if error}
      <p class="rounded-lg bg-destructive/5 px-3 py-2 text-ui-2xs text-destructive">{error}</p>
    {:else if !plan}
      <div class="flex items-center gap-2 py-6 text-ui-xs text-muted-foreground"><LoaderCircle class="size-3.5 shrink-0 animate-spin" /> Explaining…</div>
    {:else}
      <div class={cn('flex items-start gap-2 rounded-xl px-3 py-2.5', plan.collscan ? 'bg-warning/10' : 'bg-muted/40')}>
        {#if plan.collscan}
          <TriangleAlert class="mt-0.5 size-3.5 shrink-0 text-warning" />
        {:else}
          <Zap class="mt-0.5 size-3.5 shrink-0 text-success" />
        {/if}
        <p class="min-w-0 text-ui-xs text-foreground">{plan.verdict}</p>
      </div>

      <dl class="mt-3 grid grid-cols-4 gap-2">
        {#each totals as t (t.label)}
          <div class="flex flex-col gap-0.5 rounded-lg bg-muted/40 px-2.5 py-1.5">
            <dt class="truncate text-ui-3xs text-muted-foreground">{t.label}</dt>
            <dd class="font-mono text-ui-xs tabular-nums text-foreground">{num(t.n)}</dd>
          </div>
        {/each}
      </dl>

      <p class="mt-4 mb-1 text-ui-2xs text-muted-foreground">Winning plan</p>
      <ol class="flex flex-col gap-0.5">
        {#each plan.stages as s, i (i)}
          <li class="flex min-w-0 items-center gap-2 rounded-lg py-1 pr-2 hover:bg-muted/40" style:padding-left="calc({0.5 + s.depth * 0.875}rem)">
            {#if s.depth > 0}<span class="h-px w-2 shrink-0 bg-border" aria-hidden="true"></span>{/if}
            <span class={cn('shrink-0 font-mono text-ui-xs', s.stage.toUpperCase() === 'COLLSCAN' ? 'text-warning' : 'text-foreground')}>{s.stage}</span>
            {#if s.indexName}
              <span class="min-w-0 truncate rounded-full bg-muted/60 px-2 font-mono text-ui-2xs text-foreground/80" title={keyText(s.keyPattern)}>{s.indexName}</span>
            {/if}
            <span class="flex-1"></span>
            <span class="shrink-0 font-mono text-ui-2xs tabular-nums text-muted-foreground" title="Keys examined · docs examined · returned">
              {#if s.keysExamined != null}{num(s.keysExamined)}k · {/if}{#if s.docsExamined != null}{num(s.docsExamined)}d · {/if}{num(s.nReturned)}
            </span>
          </li>
        {:else}
          <li class="px-2 py-1 text-ui-2xs text-muted-foreground">No stages reported</li>
        {/each}
      </ol>

      <button type="button" class="mt-4 inline-flex h-7 items-center rounded-md px-2.5 text-ui-xs text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground" onclick={() => (showRaw = !showRaw)} aria-expanded={showRaw}>
        {showRaw ? 'Hide raw explain' : 'Show raw explain'}
      </button>
      {#if showRaw}
        <pre class="app-scroll mt-1 max-h-96 overflow-auto rounded-lg bg-muted/40 p-3 font-mono text-ui-2xs leading-relaxed text-foreground/80">{JSON.stringify(plan.raw, null, 2)}</pre>
      {/if}
    {/if}
  </div>
</div>
