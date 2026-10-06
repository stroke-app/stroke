<script>
  /**
   * A shortcut, as keycaps.
   *
   * One implementation for every place the app shows a chord. They used to be
   * written inline as glyph strings - `⌘⇧X`, `${mod}${alt}F` - which gave three
   * different results: a menu row printed three symbols butted together with no
   * shape at all, a dialog drew bordered caps with `+` between them, and a
   * search field had its own hand-rolled chips. Butted-together glyphs are the
   * worst of the three: at 10px, `⌘⇧X` is a smear, and it is the form that
   * appeared in the most places.
   *
   * Pass a combo in the grammar `createHotkey` parses (`Mod+Shift+X`) and both
   * platforms come out of it - `keycaps()` prints ⌘ on macOS and Ctrl
   * everywhere else, so no call site spells a platform out.
   *
   * A space separates the steps of a sequence: `Mod+K W` is ⌘K *then* W, and
   * comes out as two groups with a wider gap between them. Printed as one run of
   * caps it would read as a single three-key press, which is not a thing you can
   * do.
   *
   * @typedef {Object} Props
   * @property {string} [combo] e.g. `Mod+Shift+X`, or `Mod+K W` for a sequence
   * @property {string[]} [keys] pre-resolved caps, for a chord with no combo string
   * @property {'sm' | 'md'} [size] `sm` inside menus and fields, `md` in dialogs and empty states
   * @property {boolean} [wrap] let a chord break onto a second line instead of
   *   being clipped. Off macOS a modifier is a word, not a glyph, so
   *   `Ctrl Shift X` runs three times the width of `⌘⇧X` - in a narrow
   *   container the cap that gets cut is the last one, which is the one that
   *   identifies the shortcut.
   */
  import { keycaps } from '$lib/shortcuts.js'
  import { cn } from '$lib/utils.js'

  let {
    combo = '',
    /** @type {string[] | null} */
    keys = null,
    size = 'sm',
    wrap = false,
    class: className = '',
    ...rest
  } = $props()

  /** One group per step of the sequence; one step is the common case. */
  const groups = $derived(
    keys
      ? [keys]
      : combo
        ? combo
            .trim()
            .split(/\s+/)
            .filter(Boolean)
            .map((step) => keycaps(step))
        : [],
  )
  const total = $derived(groups.reduce((n, g) => n + g.length, 0))
  /** Keys that are a mark, not a letter: at keycap size a backtick is a speck. */
  const MARKS = new Set(['`', '~', "'", '"', ',', '.', ';', ':'])
</script>

{#if total}
  <span class={cn('inline-flex items-center gap-1.5', wrap ? 'min-w-0 flex-wrap gap-y-1' : 'shrink-0', className)} {...rest}>
    {#each groups as group, g (g)}<span class={cn('inline-flex items-center gap-0.5', wrap ? 'min-w-0 flex-wrap gap-y-1' : 'shrink-0')}
        >{#each group as cap, i (i)}<kbd data-size={size} data-mark={MARKS.has(cap) || undefined}>{cap}</kbd>{/each}</span
      >{/each}
  </span>
{/if}
