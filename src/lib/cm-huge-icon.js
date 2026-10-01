/**
 * A Hugeicons glyph as a DOM node, for the places CodeMirror asks for an
 * element rather than a component (gutter markers, fold arrows).
 * @param {[string, Record<string, string>][]} icon Hugeicons data, e.g. Tick02Icon
 * @param {number} [strokeWidth]
 */
export function hugeSvg(icon, strokeWidth = 2) {
  const NS = 'http://www.w3.org/2000/svg'
  const svg = document.createElementNS(NS, 'svg')
  svg.setAttribute('viewBox', '0 0 24 24')
  svg.setAttribute('fill', 'none')
  svg.setAttribute('aria-hidden', 'true')
  for (const [tag, attrs] of icon) {
    const el = document.createElementNS(NS, tag)
    for (const [k, v] of Object.entries(attrs)) {
      if (k === 'key') continue
      // Hugeicons data is JSX-cased (strokeWidth); SVG wants stroke-width.
      el.setAttribute(k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`), k === 'strokeWidth' ? String(strokeWidth) : v)
    }
    svg.append(el)
  }
  return svg
}
