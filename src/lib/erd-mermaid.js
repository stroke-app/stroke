/**
 * Mermaid sources for the ER diagram page: an `erDiagram` of the tables on
 * the page, and a `flowchart` of how they point at each other. Both are text
 * the user can copy, edit in the Diagrams page or paste anywhere Mermaid
 * renders, which is what the canvas cannot be.
 */

/**
 * @typedef {{ name: string, dataType?: string, isNullable?: boolean, foreignKey?: string | null }} MermaidCol
 * @typedef {{ name: string, columns: MermaidCol[], pkCols?: Set<string>, uniqueCols?: Set<string> }} MermaidTable
 * @typedef {{ source: string, target: string, sourceCol?: string | null, many?: boolean, optional?: boolean }} MermaidRel
 *   `source` carries the foreign key, `target` is the table it points at.
 */

/** A Mermaid identifier: letters, digits and underscores, not starting with a digit. */
export function mermaidId(name) {
  const s = String(name ?? '').replace(/[^A-Za-z0-9_]/g, '_')
  return /^[0-9]/.test(s) ? `_${s}` : s || '_'
}

/** A data type as one word: `timestamp with time zone` → `timestamp_with_time_zone`, `varchar(255)` → `varchar`. */
export function mermaidType(dataType) {
  const base = String(dataType ?? '').trim().replace(/\(.*$/, '').trim()
  return mermaidId(base || 'unknown')
}

/**
 * An `erDiagram`. Entities list their columns with PK / FK / UK marks;
 * relationships read parent to child with crow's feet: one `tenants` row to
 * many `users` rows is `tenants ||--o{ users`, a nullable key makes the parent
 * side `|o`, a unique key makes the child side `||`.
 * @param {MermaidTable[]} tables
 * @param {MermaidRel[]} rels
 * @param {{ keysOnly?: boolean }} [opts] keysOnly lists PK and FK columns only
 */
export function erdToMermaid(tables, rels, opts = {}) {
  const keysOnly = opts.keysOnly === true
  const names = new Set(tables.map((t) => t.name))
  const out = ['erDiagram']
  for (const t of tables) {
    const pk = t.pkCols ?? new Set()
    const uq = t.uniqueCols ?? new Set()
    const cols = t.columns.filter((c) => !keysOnly || pk.has(c.name) || !!c.foreignKey)
    out.push(`    ${mermaidId(t.name)} {`)
    for (const c of cols) {
      const keys = []
      if (pk.has(c.name)) keys.push('PK')
      if (c.foreignKey) keys.push('FK')
      if (!pk.has(c.name) && uq.has(c.name)) keys.push('UK')
      out.push(`        ${mermaidType(c.dataType)} ${mermaidId(c.name)}${keys.length ? ' ' + keys.join(', ') : ''}`)
    }
    out.push('    }')
  }
  for (const r of rels) {
    if (!names.has(r.source) || !names.has(r.target)) continue
    const parent = r.optional ? '|o' : '||'
    const child = r.many === false ? '||' : 'o{'
    const label = r.sourceCol ? mermaidId(r.sourceCol) : 'references'
    out.push(`    ${mermaidId(r.target)} ${parent}--${child} ${mermaidId(r.source)} : ${label}`)
  }
  return out.join('\n')
}

/**
 * A `flowchart` of the relationships: an arrow from the table holding the key
 * to the table it points at, labelled with the column. With a `focus`, the
 * tables within `depth` hops of it (capped at `maxNodes`, nearest first);
 * without one, the whole page.
 * @param {MermaidTable[]} tables
 * @param {MermaidRel[]} rels
 * @param {{ focus?: string | null, depth?: number, maxNodes?: number, direction?: 'LR' | 'TD', merge?: boolean, plain?: boolean }} [opts]
 *   `merge` draws one line per pair of tables, named after every column that
 *   joins them, instead of a line per key. `plain` drops the arrowheads too.
 */
export function relationsToFlowchart(tables, rels, opts = {}) {
  const direction = opts.direction ?? 'LR'
  const names = new Set(tables.map((t) => t.name))
  const focus = opts.focus && names.has(opts.focus) ? opts.focus : null
  let edges = rels.filter((r) => names.has(r.source) && names.has(r.target))
  /** @type {Set<string>} */
  let used = new Set(focus ? [focus] : [])
  if (focus) {
    // Breadth first from the focus, so the cap keeps the nearest tables.
    const depth = Math.max(1, opts.depth ?? 1)
    const maxNodes = opts.maxNodes ?? 60
    let frontier = [focus]
    for (let d = 0; d < depth && frontier.length && used.size < maxNodes; d++) {
      /** @type {string[]} */
      const next = []
      for (const r of edges) {
        for (const [a, b] of [[r.source, r.target], [r.target, r.source]]) {
          if (frontier.includes(a) && !used.has(b) && used.size < maxNodes) { used.add(b); next.push(b) }
        }
      }
      frontier = next
    }
    edges = edges.filter((r) => used.has(r.source) && used.has(r.target))
  } else {
    for (const r of edges) { used.add(r.source); used.add(r.target) }
  }
  const shown = focus ? tables.filter((t) => used.has(t.name)) : tables
  const out = [`flowchart ${direction}`]
  for (const t of shown) out.push(`    ${mermaidId(t.name)}["${t.name.replace(/"/g, "'")}"]`)
  const arrow = opts.plain ? '---' : '-->'
  if (opts.merge || opts.plain) {
    // One line per pair, from the table holding the key(s) to the table they
    // point at, named after every column that joins the two.
    /** @type {Map<string, { source: string, target: string, cols: string[] }>} */
    const pairs = new Map()
    for (const r of edges) {
      if (r.source === r.target) continue
      const key = [r.source, r.target].sort().join('\0')
      let pair = pairs.get(key)
      if (!pair) { pair = { source: r.source, target: r.target, cols: [] }; pairs.set(key, pair) }
      if (r.sourceCol && !pair.cols.includes(r.sourceCol)) pair.cols.push(r.sourceCol)
    }
    for (const { source, target, cols } of pairs.values()) {
      const label = cols.length ? `|${cols.join(', ').replace(/\|/g, '/')}|` : ''
      out.push(`    ${mermaidId(source)} ${arrow}${label} ${mermaidId(target)}`)
    }
  } else {
    for (const r of edges) {
      const label = r.sourceCol ? `|${r.sourceCol.replace(/\|/g, '/')}|` : ''
      out.push(`    ${mermaidId(r.source)} -->${label} ${mermaidId(r.target)}`)
    }
  }
  if (focus) out.push(`    style ${mermaidId(focus)} fill:var(--_group-hdr),stroke:var(--fg),stroke-width:1.5px`)
  return out.join('\n')
}
