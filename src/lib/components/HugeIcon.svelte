<script module>
  /**
   * Inner SVG markup by icon data, then by stroke width. An icon's data never
   * changes, so it is turned into markup once per weight for the whole app.
   * @type {WeakMap<object, Map<string, string>>}
   */
  const markup = new WeakMap()

  const kebab = (/** @type {string} */ s) => s.replace(/([a-z0-9])([A-Z])/g, '$1-$2').toLowerCase()
  const esc = (/** @type {unknown} */ v) => String(v).replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;')

  /**
   * The children of a Hugeicons <svg>, as @hugeicons/svelte writes them: each
   * element's attributes kebab-cased, plus a stroke width and currentColor
   * stroke when a weight is given.
   * @param {[string, Record<string, unknown>][]} icon @param {number | undefined} strokeWidth
   */
  function innerOf(icon, strokeWidth) {
    const key = String(strokeWidth)
    let byWeight = markup.get(icon)
    if (!byWeight) markup.set(icon, (byWeight = new Map()))
    const hit = byWeight.get(key)
    if (hit !== undefined) return hit
    let out = ''
    for (const [tag, attrs] of icon) {
      const a = strokeWidth === undefined ? attrs : { ...attrs, 'stroke-width': strokeWidth, stroke: 'currentColor' }
      out += `<${tag}`
      for (const [k, v] of Object.entries(a)) out += ` ${kebab(k)}="${esc(v)}"`
      out += `></${tag}>`
    }
    byWeight.set(key, out)
    return out
  }
</script>

<script>
  /**
   * A Hugeicons glyph, drawn like @hugeicons/svelte's HugeiconsIcon but built
   * once per icon and weight. That component created every child element on
   * mount and again from an effect straight after (two full renders per
   * icon), so a sidebar re-rendering its rows on a table switch spent most of
   * its time building identical table icons.
   */
  let {
    /** @type {[string, Record<string, unknown>][]} */
    icon,
    /** @type {number | undefined} */
    strokeWidth = undefined,
    class: className = '',
    ...rest
  } = $props()

  const inner = $derived(innerOf(icon, strokeWidth))
</script>

<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" color="currentColor" class={className} {...rest}>{@html inner}</svg>
