<script>
  /**
   * Release notes for one or more versions, as the update window and What's
   * new both show them. Every change goes into one of three buckets (New,
   * Improved, Fixed) across all the versions in view, so an update that skipped
   * a dozen nightlies reads as one list rather than twelve short sections. A
   * filter narrows a long list; each item carries the version it arrived in.
   */
  import { cn } from '$lib/utils.js'

  /** @type {{ releases?: import('$lib/changelog.js').ChangelogRelease[], omitted?: number, compact?: boolean }} */
  let { releases = [], omitted = 0, compact = false } = $props()

  /** @typedef {'new' | 'improved' | 'fixed'} Bucket */
  /** @type {Record<import('$lib/changelog.js').ChangeType, Bucket>} */
  const BUCKET = { feature: 'new', change: 'improved', other: 'improved', fix: 'fixed' }
  /** @type {{ id: Bucket, label: string, dot: string }[]} */
  const BUCKETS = [
    { id: 'new', label: 'New', dot: 'bg-primary' },
    { id: 'improved', label: 'Improved', dot: 'bg-info' },
    { id: 'fixed', label: 'Fixed', dot: 'bg-success' },
  ]

  /** Items by bucket, newest version first, with the version each came in. */
  const grouped = $derived.by(() => {
    /** @type {Record<Bucket, { text: string, version: string }[]>} */
    const out = { new: [], improved: [], fixed: [] }
    for (const r of releases) {
      for (const s of r.sections) {
        for (const text of s.items) out[BUCKET[s.type] ?? 'improved'].push({ text, version: r.version })
      }
    }
    return out
  })
  const total = $derived(grouped.new.length + grouped.improved.length + grouped.fixed.length)
  const multi = $derived(releases.length > 1)

  /** @type {'all' | Bucket} */
  let filter = $state('all')
  const shown = $derived(BUCKETS.filter((b) => grouped[b.id].length && (filter === 'all' || filter === b.id)))

  /** `code` in a changelog line, split out so it renders as code. @param {string} s */
  function parts(s) {
    return String(s).split(/(`[^`]+`)/g).filter(Boolean).map((p) => (p.startsWith('`') && p.endsWith('`') ? { code: true, text: p.slice(1, -1) } : { code: false, text: p }))
  }
</script>

{#if total}
  <div class="flex flex-col gap-4">
    {#if total > 6}
      <!-- A segmented filter with counts: how much changed, at a glance. -->
      <div role="radiogroup" aria-label="Show changes" class="flex w-fit gap-0.5 rounded-lg bg-muted/40 p-0.5">
        {#each [{ id: 'all', label: 'All', n: total }, ...BUCKETS.map((b) => ({ id: b.id, label: b.label, n: grouped[b.id].length }))].filter((o) => o.n) as o (o.id)}
          <button
            type="button"
            role="radio"
            aria-checked={filter === o.id}
            class={cn(
              'flex h-7 items-center gap-1.5 rounded-md px-2.5 text-ui-xs font-medium transition-colors duration-150 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-ring',
              filter === o.id ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground',
            )}
            onclick={() => (filter = /** @type {any} */ (o.id))}
          >
            {o.label}<span class="tabular-nums text-muted-foreground">{o.n}</span>
          </button>
        {/each}
      </div>
    {/if}

    {#each shown as b (b.id)}
      <section class="flex flex-col gap-2" aria-label={b.label}>
        <h3 class="flex items-center gap-2 text-ui-sm font-semibold text-foreground">
          <span class={cn('size-1.5 shrink-0 rounded-full', b.dot)} aria-hidden="true"></span>
          {b.label}
          <span class="text-ui-xs font-normal tabular-nums text-muted-foreground">{grouped[b.id].length}</span>
        </h3>
        <ul class={cn('flex flex-col', compact ? 'gap-1.5' : 'gap-2')}>
          {#each grouped[b.id] as item, i (i)}
            <li class="flex items-baseline gap-2.5 ps-0.5">
              <!-- A small marker, so a list of long lines reads as separate
                   changes rather than one paragraph. -->
              <span class="size-1 shrink-0 translate-y-[-0.2em] rounded-full bg-muted-foreground/50" aria-hidden="true"></span>
              <span class={cn('min-w-0 flex-1 text-pretty leading-relaxed text-foreground/90', compact ? 'text-ui-xs' : 'text-ui-sm')}>
                {#each parts(item.text) as p, k (k)}
                  {#if p.code}<code class="rounded-md bg-muted/60 px-1 py-px font-mono text-ui-xs text-foreground">{p.text}</code>{:else}{p.text}{/if}
                {/each}
              </span>
              {#if multi}
                <span class="shrink-0 font-mono text-ui-3xs tabular-nums text-muted-foreground/80" title="Arrived in v{item.version}">{item.version}</span>
              {/if}
            </li>
          {/each}
        </ul>
      </section>
    {/each}

    {#if omitted > 0}
      <p class="text-ui-xs text-muted-foreground">And {omitted} earlier {omitted === 1 ? 'release' : 'releases'} in the full changelog.</p>
    {/if}
  </div>
{/if}
