<script>
  /**
   * The shape of a table that is still loading: rows the height of the real
   * ones (h-9), a bar per column at a steady width, hairlines between rows.
   * The pages that load a list (Security, Database objects) showed a centred
   * spinner and "Loading…", and then the table jumped in where the spinner
   * had been; this holds the table's place, so the rows land where the bars
   * were. Held back like the spinner it replaces (`loading-reveal`), so a
   * fast load never flashes it.
   *
   * @typedef {Object} Props
   * @property {number} [rows]
   * @property {number[]} [cols] each column's bar width, in percent of its cell
   * @property {string} [class]
   */
  import Shimmer from './Shimmer.svelte'
  import { cn } from '$lib/utils.js'

  /** @type {Props} */
  let { rows = 8, cols = [55, 30, 20, 40], class: className = '' } = $props()
  /** Bars a little shorter on alternate rows, so it reads as text, not a grid of blocks. */
  const width = (/** @type {number} */ w, /** @type {number} */ r) => `${Math.max(12, w - (r % 3) * 9)}%`
</script>

<div class={cn('loading-reveal w-full', className)} role="status" aria-busy="true">
  <span class="sr-only">Loading</span>
  {#each Array.from({ length: rows }) as _, r (r)}
    <div class="flex h-9 items-center gap-6 border-b border-border/30 px-4" aria-hidden="true">
      {#each cols as w, c (c)}
        <div class="min-w-0 flex-1">
          <Shimmer class="h-2.5" style="width: {width(w, r + c)}" rounded="full" />
        </div>
      {/each}
    </div>
  {/each}
</div>
