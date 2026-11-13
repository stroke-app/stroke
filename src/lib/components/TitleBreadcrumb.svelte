<script>
  /**
   * The macOS title row's breadcrumb: workspace / connection / current page.
   * The leading crumbs are muted context; the last one is the page you are on.
   * Plain text, no buttons, so the whole thing stays part of the window's drag
   * region (TitleBar tags the row `data-tauri-drag-region="deep"`).
   *
   * @typedef {Object} Props
   * @property {{ workspaceId?: string | null, color?: string | null } | null} [savedConnection]
   * @property {string} [connectionLabel]
   * @property {string} [pageTitle]
   * @property {number} [maxWidth] Room the title bar has for the trail, in px.
   */
  import { cn } from '$lib/utils.js'
  import DbIcon from './DbIcon.svelte'

  /** @type {Props} */
  let { savedConnection = null, connectionLabel = '', pageTitle = '', maxWidth = Infinity } = $props()

  // `[engine icon] connection / page`, like T3's `[icon] project / thread`. The
  // workspace is not repeated here: the switcher on the right names it.
  const engine = $derived(/** @type {any} */ (savedConnection)?.type ?? '')

  /** @type {{ key: string, label: string, icon: string }[]} */
  const crumbs = $derived(
    [
      connectionLabel ? { key: 'connection', label: connectionLabel, icon: engine } : null,
      pageTitle ? { key: 'page', label: pageTitle, icon: '' } : null,
    ].filter((c) => c !== null),
  )

  // Shorter trails to fall back to as the room shrinks: drop the workspace,
  // then the connection, leaving the page you are on. Each is measured off
  // screen at its natural width, so the choice is exact at every zoom and the
  // bar never shows a trail squeezed into fragments ("• / a / |").
  const trails = $derived(
    [crumbs, crumbs.filter((c) => c.key === 'page')]
      .filter((t, i, all) => t.length > 0 && (i === 0 || t.length < all[i - 1].length)),
  )
  /** Natural width of each trail, in px. @type {number[]} */
  let widths = $state([])

  // Below this a lone page name truncates to a stub that says nothing, so the
  // trail hides instead. rem-based so it holds the same few characters at any zoom.
  const MIN_PAGE_REM = 6
  const shown = $derived.by(() => {
    if (!(maxWidth > 0)) return null
    const fit = trails.findIndex((_, i) => (widths[i] ?? Infinity) <= maxWidth)
    if (fit >= 0) return trails[fit]
    const last = trails[trails.length - 1]
    if (last?.length === 1 && last[0].key === 'page' && maxWidth >= MIN_PAGE_REM * 16) return last
    return null
  })
</script>

{#snippet trail(/** @type {typeof crumbs} */ list)}
  {#each list as crumb, i (crumb.key)}
    {@const last = i === list.length - 1}
    {#if i > 0}
      <span class="shrink-0 px-2.5 text-muted-foreground/50" aria-hidden="true">/</span>
    {/if}
    <span
      class={cn(
        'flex min-w-0 items-center gap-1.5',
        last ? 'max-w-[18rem] text-foreground' : 'max-w-[14rem] text-muted-foreground',
      )}
      aria-current={last ? 'page' : undefined}
      title={crumb.label}
    >
      {#if crumb.icon}
        <DbIcon id={crumb.icon} class="size-4 shrink-0" />
      {/if}
      <span class="truncate">{crumb.label}</span>
    </span>
  {/each}
{/snippet}

<!-- Off-screen measures, one per candidate trail. -->
{#each trails as list, i (i)}
  <div class="invisible absolute top-0 left-0 flex w-max items-center text-ui-sm" aria-hidden="true" bind:clientWidth={widths[i]}>
    {@render trail(list)}
  </div>
{/each}

{#if shown}
  <nav aria-label="Location" class="flex min-w-0 items-center text-ui-sm select-none" style:max-width={Number.isFinite(maxWidth) ? `${maxWidth}px` : undefined}>
    {@render trail(shown)}
  </nav>
{/if}
