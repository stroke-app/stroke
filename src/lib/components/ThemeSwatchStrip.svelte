<script>
  /**
   * A theme's own colours as a row of chips: background, text, accent, red,
   * yellow, green. Read from the theme's CSS tokens (see theme-swatches.js),
   * so the strip is always what the theme actually paints.
   */
  import { cn } from '$lib/utils.js'
  import { themeSwatches } from '$lib/themes/theme-swatches.js'

  let {
    /** @type {import('$lib/themes/registry.js').ThemeId} */
    themeId,
    class: className = '',
  } = $props()

  const swatches = $derived(themeSwatches(themeId))
</script>

<span class={cn('flex shrink-0 items-center gap-0.5', className)} aria-hidden="true">
  {#each swatches as s (s.key)}
    <span
      class="size-3 shrink-0 rounded-sm ring-1 ring-inset ring-foreground/15"
      style:background-color={s.color}
    ></span>
  {/each}
</span>
