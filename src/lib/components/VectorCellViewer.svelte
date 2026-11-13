<script>
  // A vector cell, read rather than dumped.
  //
  // The generic text editor showed an embedding as 1,537 characters of
  // `0.1,0.1,0.1,…` - technically the value, practically unreadable. What you
  // want from an embedding is its shape: how many dimensions, how the values are
  // distributed, whether it's unit-normalised (models return normalised vectors,
  // so a norm that isn't 1 means something), and where the extremes sit. So the
  // default view is a distribution strip plus those figures, with the numbers
  // themselves one click away and the raw literal one more.
  import Icon from './Icon.svelte'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import { parseVector, vectorBars, vectorHistogram, short } from '$lib/vector-cell.js'
  import { cn } from '$lib/utils.js'

  let {
    open = $bindable(false),
    /** Column name, for the header. */
    column = '',
    /** Column type: `vector`, `halfvec`, `sparsevec`. */
    dataType = 'vector',
    nullable = false,
    readOnly = false,
    /** The cell's raw literal, e.g. `[0.1,0.2]`. */
    value = '',
    /** Called with the new literal, or null for SQL NULL. */
    onsave = /** @type {(next: string | null) => void} */ (() => {}),
  } = $props()

  /** @type {'chart' | 'values' | 'raw'} */
  let tab = $state('chart')
  let draft = $state('')
  let hovered = $state(/** @type {number | null} */ (null))

  const info = $derived(parseVector(value))
  const dirty = $derived(tab === 'raw' && draft.trim() !== String(value ?? '').trim())

  $effect(() => {
    if (!open) return
    draft = String(value ?? '')
    tab = 'chart'
    hovered = null
    histHovered = null
  })

  // 96 bars is about one per 3px of dialog width - dense enough to read as a
  // distribution, coarse enough that each bar is clickable.
  const BARS = 96
  const bars = $derived(info ? vectorBars(info.values, BARS) : [])
  /** Bar height is relative to the largest magnitude, so small vectors still fill. */
  const scale = $derived(Math.max(...bars.map((b) => Math.abs(b)), 1e-9))
  const perBar = $derived(info && bars.length ? info.values.length / bars.length : 1)

  // The strip is indexed by dimension; the histogram is indexed by value. A
  // healthy embedding reads roughly gaussian around zero here - spikes and
  // heavy tails are the "something is wrong with this vector" signal.
  const HIST_BINS = 48
  const hist = $derived(info ? vectorHistogram(info.values, HIST_BINS) : { counts: [], from: 0, to: 0, max: 0 })
  let histHovered = $state(/** @type {number | null} */ (null))

  const stats = $derived(
    info
      ? [
          { label: 'dim', value: String(info.dim) },
          { label: 'norm', value: short(info.norm), hint: info.unit ? 'unit length' : 'not normalised' },
          { label: 'min', value: short(info.min), hint: `at ${info.minIndex}` },
          { label: 'max', value: short(info.max), hint: `at ${info.maxIndex}` },
          { label: 'mean', value: short(info.mean) },
          { label: 'std', value: short(info.std) },
          ...(info.zeros ? [{ label: 'zeros', value: String(info.zeros) }] : []),
        ]
      : [],
  )

  async function copyRaw() {
    try {
      await navigator.clipboard.writeText(String(value ?? ''))
      toast.success(`Copied ${info ? `${info.dim} values` : 'value'}`)
    } catch (e) {
      toast.error('Could not copy', { description: String(e) })
    }
  }

  function save() {
    if (readOnly) return
    onsave(draft.trim() === '' ? null : draft)
    open = false
  }
</script>

<Dialog.Root bind:open>
  <!-- The dialog's own close button is absolutely positioned top-right, where it
       landed on top of the "Raw" tab. Ours sits in the header row instead. -->
  <Dialog.Content showCloseButton={false} class="max-w-2xl gap-0 overflow-hidden p-0">
    <!-- Header: what this is, and the one number that identifies it. -->
    <div class="flex h-11 shrink-0 items-center gap-2 border-b border-border/50 px-4">
      <Icon name="bar-chart-2" class="size-3.5 shrink-0 text-primary" />
      <span class="min-w-0 truncate font-mono text-ui-sm font-medium">{column}</span>
      <span class="shrink-0 rounded bg-muted/60 px-1.5 py-0.5 font-mono text-ui-2xs text-muted-foreground">
        {dataType}{#if info}({info.dim}){/if}
      </span>
      {#if nullable}
        <span class="shrink-0 rounded bg-muted/40 px-1.5 py-0.5 font-mono text-ui-3xs text-muted-foreground">nullable</span>
      {/if}

      <div class="ml-auto flex shrink-0 items-center gap-0.5 rounded-md bg-muted/40 p-0.5">
        {#each [['chart', 'Shape'], ['values', 'Values'], ['raw', 'Raw']] as [id, label] (id)}
          <button
            type="button"
            onclick={() => (tab = /** @type {'chart'|'values'|'raw'} */ (id))}
            class={cn(
              'inline-flex h-6 items-center rounded-md px-2 text-ui-2xs transition-colors',
              tab === id ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground',
            )}
          >{label}</button>
        {/each}
      </div>

      <Dialog.Close
        class="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none"
        aria-label="Close"
      >
        <Icon name="x" class="size-3.5 shrink-0" />
      </Dialog.Close>
    </div>

    {#if !info}
      <div class="px-4 py-10 text-center">
        <p class="text-ui-sm text-muted-foreground">This value isn't a vector literal.</p>
        <p class="mt-1 font-mono text-ui-2xs text-muted-foreground">{String(value ?? '').slice(0, 120)}</p>
      </div>
    {:else}
      <!-- ── Shape ─────────────────────────────────────────────────────────── -->
      {#if tab === 'chart'}
        <div class="px-4 pt-4">
          <!-- Distribution strip: one bar per bucket, above/below a zero line so
               sign is visible at a glance. Each bar keeps its bucket's extreme,
               so a single spike in 1536 dimensions still shows. -->
          <div class="relative h-28 w-full overflow-hidden rounded-md border border-border/40 bg-muted/10">
            <div class="absolute inset-x-0 top-1/2 h-px bg-border/50"></div>
            <div class="flex h-full items-center gap-px px-1">
              {#each bars as b, i (i)}
                {@const mag = Math.abs(b) / scale}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                  class="group relative flex h-full flex-1 cursor-default flex-col justify-center"
                  onmouseenter={() => (hovered = i)}
                  onmouseleave={() => (hovered = null)}
                >
                  <div
                    class={cn(
                      'w-full rounded-[1px] transition-colors',
                      b >= 0 ? 'self-end bg-primary/70' : 'self-start bg-warning/70',
                      hovered === i && 'bg-foreground/80',
                    )}
                    style="height: {Math.max(1, mag * 46)}%; margin-{b >= 0 ? 'bottom' : 'top'}: 50%"
                  ></div>
                </div>
              {/each}
            </div>
            {#if hovered !== null}
              {@const from = Math.floor(hovered * perBar)}
              {@const to = Math.min(info.values.length, Math.floor((hovered + 1) * perBar)) - 1}
              <div class="pointer-events-none absolute right-1.5 top-1.5 rounded bg-background/90 px-1.5 py-0.5 font-mono text-ui-3xs text-muted-foreground">
                {from === to ? `[${from}]` : `[${from}-${to}]`} peak {short(bars[hovered])}
              </div>
            {/if}
          </div>

          <!-- Value histogram: distribution across the value range, not the
               index. Gaussian-around-zero is the healthy shape for a dense
               embedding; anything else is worth noticing. -->
          {#if hist.counts.length}
            <div class="relative mt-2 h-14 w-full overflow-hidden rounded-md border border-border/40 bg-muted/10">
              <div class="flex h-full items-end gap-px px-1 pb-3.5">
                {#each hist.counts as c, i (i)}
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <div
                    class="flex h-full flex-1 cursor-default items-end"
                    onmouseenter={() => (histHovered = i)}
                    onmouseleave={() => (histHovered = null)}
                  >
                    <div
                      class={cn('w-full rounded-t-[1px]', histHovered === i ? 'bg-foreground/80' : 'bg-primary/50')}
                      style="height: {c ? Math.max(4, (c / hist.max) * 100) : 0}%"
                    ></div>
                  </div>
                {/each}
              </div>
              <div class="pointer-events-none absolute inset-x-1.5 bottom-0.5 flex justify-between font-mono text-ui-3xs text-muted-foreground">
                <span>{short(hist.from)}</span>
                <span>value distribution</span>
                <span>{short(hist.to)}</span>
              </div>
              {#if histHovered !== null}
                {@const binW = (hist.to - hist.from) / hist.counts.length}
                <div class="pointer-events-none absolute right-1.5 top-1 rounded bg-background/90 px-1.5 py-0.5 font-mono text-ui-3xs text-muted-foreground">
                  {short(hist.from + binW * histHovered)}…{short(hist.from + binW * (histHovered + 1))} · {hist.counts[histHovered]}×
                </div>
              {/if}
            </div>
          {/if}

          <!-- The figures that tell you whether this vector is what you expect. -->
          <div class="mt-3 grid grid-cols-[repeat(auto-fit,minmax(96px,1fr))] gap-2">
            {#each stats as s (s.label)}
              <div class="rounded-md border border-border/40 bg-muted/10 px-2.5 py-1.5">
                <p class="text-ui-3xs uppercase tracking-[0.06em] text-muted-foreground">{s.label}</p>
                <p class="mt-0.5 font-mono text-ui-sm tabular-nums text-foreground/90">{s.value}</p>
                {#if s.hint}
                  <p class={cn(
                    'mt-0.5 truncate text-ui-3xs',
                    s.label === 'norm' && !info.unit ? 'text-warning' : 'text-muted-foreground',
                  )}>{s.hint}</p>
                {/if}
              </div>
            {/each}
          </div>
        </div>

      <!-- ── Values ────────────────────────────────────────────────────────── -->
      {:else if tab === 'values'}
        <div class="max-h-[22rem] overflow-y-auto px-4 pt-3">
          {#if info.kind === 'sparse'}
            <div class="grid grid-cols-[repeat(auto-fill,minmax(140px,1fr))] gap-1">
              {#each info.entries ?? [] as e (e.index)}
                <div class="flex items-baseline gap-2 rounded px-1.5 py-1 hover:bg-muted/30">
                  <span class="w-10 shrink-0 text-right font-mono text-ui-3xs text-muted-foreground">{e.index}</span>
                  <span class="font-mono text-ui-2xs tabular-nums">{short(e.value)}</span>
                </div>
              {/each}
            </div>
          {:else}
            <!-- Index beside every value: without it you cannot tell dimension 900
                 from dimension 901, which is the only reason to read the list. -->
            <div class="grid grid-cols-[repeat(auto-fill,minmax(124px,1fr))] gap-x-2">
              {#each info.values as v, i (i)}
                <div class={cn(
                  'flex items-baseline gap-2 rounded px-1.5 py-0.5 hover:bg-muted/30',
                  (i === info.maxIndex || i === info.minIndex) && 'bg-primary/5',
                )}>
                  <span class="w-9 shrink-0 text-right font-mono text-ui-3xs text-muted-foreground">{i}</span>
                  <span class={cn(
                    'font-mono text-ui-2xs tabular-nums',
                    v === 0 ? 'text-muted-foreground' : 'text-foreground/85',
                  )}>{short(v)}</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>

      <!-- ── Raw ───────────────────────────────────────────────────────────── -->
      {:else}
        <div class="px-4 pt-3">
          <textarea
            bind:value={draft}
            readonly={readOnly}
            spellcheck="false"
            class="h-[19rem] w-full resize-none rounded-md border-2 border-border bg-transparent p-3 font-mono text-ui-2xs leading-relaxed text-foreground/90 outline-none transition-colors focus:border-ring/55 read-only:opacity-70"
          ></textarea>
        </div>
      {/if}
    {/if}

    <!-- Footer -->
    <div class="mt-3 flex shrink-0 items-center gap-2 border-t border-border/50 px-4 py-2.5">
      {#if nullable && !readOnly}
        <Button variant="ghost" size="sm" class="h-7 px-2 text-ui-xs text-muted-foreground hover:text-foreground"
          onclick={() => { onsave(null); open = false }}>Set NULL</Button>
      {/if}
      <Button variant="ghost" size="sm" class="h-7 gap-1.5 px-2 text-ui-xs text-muted-foreground hover:text-foreground" onclick={copyRaw}>
        <Icon name="copy" class="size-3.5 shrink-0" />
        Copy
      </Button>
      <span class="ml-auto font-mono text-ui-3xs text-muted-foreground">
        {String(value ?? '').length.toLocaleString()} chars
      </span>
      <Button variant="ghost" size="sm" class="h-7 px-2 text-ui-xs" onclick={() => (open = false)}>
        {dirty ? 'Cancel' : 'Close'}
      </Button>
      {#if !readOnly}
        <Button size="sm" class="h-7 px-3 text-ui-xs" disabled={!dirty} onclick={save}>Save</Button>
      {/if}
    </div>
  </Dialog.Content>
</Dialog.Root>
