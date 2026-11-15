<script>
  /**
   * Release notes for one or more versions, as the update window and What's
   * new both show them: one block per version, newest first, so it is clear
   * what each build brought. A change from a "New Features" or "Bug Fixes"
   * section carries a small New or Fixed tag; plain changes carry none.
   */
  import { cn } from '$lib/utils.js'

  /** @type {{ releases?: import('$lib/changelog.js').ChangelogRelease[], omitted?: number, compact?: boolean }} */
  let { releases = [], omitted = 0, compact = false } = $props()

  /** @type {Partial<Record<import('$lib/changelog.js').ChangeType, { label: string, cls: string }>>} */
  const TAG = {
    feature: { label: 'New', cls: 'bg-primary/10 text-primary' },
    fix: { label: 'Fixed', cls: 'bg-success/10 text-success' },
  }

  /** Each version's items in file order, tagged by the section they sat in. */
  const versions = $derived(
    releases
      .map((r) => ({
        version: r.version,
        date: r.date,
        items: r.sections.flatMap((s) => s.items.map((text) => ({ text, type: s.type }))),
      }))
      .filter((r) => r.items.length),
  )
  /** One version needs no heading: the window's title already names it. */
  const multi = $derived(versions.length > 1)

  /** `code` in a changelog line, split out so it renders as code. @param {string} s */
  function parts(s) {
    return String(s).split(/(`[^`]+`)/g).filter(Boolean).map((p) => (p.startsWith('`') && p.endsWith('`') ? { code: true, text: p.slice(1, -1) } : { code: false, text: p }))
  }

  /** "2026-11-15" as "Nov 15"; anything else as written. @param {string} d */
  function shortDate(d) {
    const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(d ?? '')
    if (!m) return d ?? ''
    const date = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]))
    return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
  }
</script>

{#if versions.length}
  <div class={cn('flex flex-col', multi ? 'gap-5' : 'gap-2')}>
    {#each versions as r (r.version)}
      <section class="flex flex-col gap-2" aria-label="Version {r.version}">
        {#if multi}
          <h3 class="flex items-baseline gap-2">
            <span class="font-mono text-ui-xs font-semibold tabular-nums text-foreground">{r.version}</span>
            {#if r.date}<span class="text-ui-2xs text-muted-foreground">{shortDate(r.date)}</span>{/if}
          </h3>
        {/if}
        <ul class={cn('flex flex-col', compact ? 'gap-1.5' : 'gap-2')}>
          {#each r.items as item, i (i)}
            <li class="flex items-baseline gap-2.5">
              <span class="size-1 shrink-0 translate-y-[-0.2em] rounded-full bg-muted-foreground/50" aria-hidden="true"></span>
              <span class={cn('min-w-0 flex-1 text-pretty leading-relaxed text-foreground/90', compact ? 'text-ui-xs' : 'text-ui-sm')}>
                {#if TAG[item.type]}
                  <span class={cn('me-1.5 inline-flex h-4 items-center rounded px-1 align-[0.1em] text-ui-3xs font-medium', TAG[item.type]?.cls)}>{TAG[item.type]?.label}</span>
                {/if}
                {#each parts(item.text) as p, k (k)}
                  {#if p.code}<code class="rounded-md bg-muted/60 px-1 py-px font-mono text-ui-xs text-foreground">{p.text}</code>{:else}{p.text}{/if}
                {/each}
              </span>
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
