<script>
  /**
   * Release notes for one or more versions, as the update window and What's
   * new both show them: one block per version, newest first. Within a version
   * the changes sit under their section (New, Improved, Fixed, and a
   * sub-section such as Performance), each as its title over a quieter line
   * of what changed. A tag on every row and title and text run together as
   * one paragraph made a wall of text.
   */
  import { cn } from '$lib/utils.js'
  import { splitInlineMarks, splitEntry } from '$lib/changelog.js'

  /** @type {{ releases?: import('$lib/changelog.js').ChangelogRelease[], omitted?: number, compact?: boolean }} */
  let { releases = [], omitted = 0, compact = false } = $props()

  /** @type {Record<import('$lib/changelog.js').ChangeType, { label: string, dot: string }>} */
  const KIND = {
    feature: { label: 'New', dot: 'bg-primary' },
    change: { label: 'Improved', dot: 'bg-info' },
    fix: { label: 'Fixed', dot: 'bg-success' },
    other: { label: 'Changes', dot: 'bg-muted-foreground/60' },
  }

  /**
   * Each version's sections with a heading: a top-level section by its kind,
   * a sub-section (#### Performance) by its own title.
   */
  const versions = $derived(
    releases
      .map((r) => ({
        version: r.version,
        date: r.date,
        groups: r.sections
          .filter((s) => s.items.length)
          .map((s) => ({
            heading: s.title !== s.group ? s.title : KIND[s.type].label,
            dot: KIND[s.type].dot,
            items: s.items.map(splitEntry),
          })),
      }))
      .filter((r) => r.groups.length),
  )
  /** One version needs no heading: the window's title already names it. */
  const multi = $derived(versions.length > 1)

  /** "2026-11-15" as "Nov 15"; anything else as written. @param {string} d */
  function shortDate(d) {
    const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(d ?? '')
    if (!m) return d ?? ''
    const date = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]))
    return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
  }
</script>

{#snippet inline(/** @type {string} */ text)}
  {#each splitInlineMarks(text) as p, k (k)}
    {#if p.code}<code class="rounded-sm bg-muted/60 px-1 py-px font-mono text-foreground">{p.text}</code>{:else if p.bold}<strong class="font-medium text-foreground">{p.text}</strong>{:else}{p.text}{/if}
  {/each}
{/snippet}

{#if versions.length}
  <div class={cn('flex flex-col', multi ? 'gap-6' : 'gap-4')}>
    {#each versions as r (r.version)}
      <section class={cn('flex flex-col', compact ? 'gap-3.5' : 'gap-4')} aria-label="Version {r.version}">
        {#if multi}
          <h3 class="flex items-baseline gap-2 border-b border-border/40 pb-1.5">
            <span class="font-mono text-ui-xs font-semibold tabular-nums text-foreground">{r.version}</span>
            {#if r.date}<span class="text-ui-2xs text-muted-foreground">{shortDate(r.date)}</span>{/if}
          </h3>
        {/if}
        {#each r.groups as g, gi (gi)}
          <div class="flex flex-col gap-2">
            <h4 class="flex items-center gap-1.5 text-ui-2xs font-medium text-muted-foreground">
              <span class={cn('size-1.5 shrink-0 rounded-full', g.dot)} aria-hidden="true"></span>
              {g.heading}
            </h4>
            <ul class={cn('flex flex-col', compact ? 'gap-2.5' : 'gap-3')}>
              {#each g.items as item, i (i)}
                <li class="flex min-w-0 flex-col gap-0.5 ps-3">
                  {#if item.title}
                    <p class="text-ui-sm font-medium text-pretty text-foreground">{@render inline(item.title)}</p>
                    {#if item.body}<p class="text-ui-xs leading-relaxed text-pretty text-muted-foreground">{@render inline(item.body)}</p>{/if}
                  {:else}
                    <p class="text-ui-sm leading-relaxed text-pretty text-foreground/90">{@render inline(item.body)}</p>
                  {/if}
                </li>
              {/each}
            </ul>
          </div>
        {/each}
      </section>
    {/each}

    {#if omitted > 0}
      <p class="text-ui-xs text-muted-foreground">And {omitted} earlier {omitted === 1 ? 'release' : 'releases'} in the full changelog.</p>
    {/if}
  </div>
{/if}
