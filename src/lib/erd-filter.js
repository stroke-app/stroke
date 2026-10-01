/**
 * Which tables an ER diagram draws.
 *
 * Three things decide it, in order: an explicit table selection, the scope the
 * diagram was opened at (one table, its neighbours, or the whole schema), and
 * the "only linked tables" toggle. An explicit selection wins over both of the
 * others - it is the one signal that came from the user naming tables, so a
 * heuristic must not quietly add to it or take from it.
 *
 * Keeping this out of the component means the rules are testable, and the
 * diagram page only has to render what comes back.
 *
 * @typedef {{ source: string, target: string }} Rel  a foreign key, child → parent
 * @typedef {'self' | 'related' | 'all'} Scope
 */

/**
 * Every table one foreign key hop from `seed`, including the seed itself.
 * @param {Iterable<string>} seed @param {Rel[]} rels
 * @returns {Set<string>}
 */
export function relatedTo(seed, rels) {
  // Hops are measured from the seed, never from what the sweep just added -
  // growing the set as we go would walk the whole component, and in an order
  // that depends on how the edge list happens to be sorted.
  const base = new Set(seed)
  const out = new Set(base)
  for (const r of rels) {
    if (base.has(r.source)) out.add(r.target)
    if (base.has(r.target)) out.add(r.source)
  }
  return out
}

/** Tables with at least one foreign key, in either direction. @param {Rel[]} rels */
export function linkedTables(rels) {
  /** @type {Set<string>} */
  const out = new Set()
  for (const r of rels) { out.add(r.source); out.add(r.target) }
  return out
}

/**
 * @param {{ tables: string[], rels: Rel[], focusTable?: string, scope?: Scope,
 *   connectedOnly?: boolean, picked?: Set<string> | null }} opts
 * @returns {Set<string>}
 */
export function visibleTables({ tables, rels, focusTable = '', scope = 'all', connectedOnly = false, picked = null }) {
  const known = new Set(tables)

  if (picked && picked.size) {
    const out = new Set()
    for (const name of picked) if (known.has(name)) out.add(name)
    // The table the diagram belongs to is always on it: a per-table diagram that
    // doesn't show its own table isn't that table's diagram any more.
    if (focusTable && known.has(focusTable)) out.add(focusTable)
    return out
  }

  const focused = focusTable && known.has(focusTable)
  if (focused && scope === 'self') return new Set([focusTable])
  if (focused && scope === 'related') return relatedTo([focusTable], rels)
  if (connectedOnly) {
    const linked = linkedTables(rels)
    return new Set(tables.filter(t => linked.has(t)))
  }
  return known
}

/**
 * The relations to draw between the visible tables.
 *
 * In a neighbours-only diagram, only the focused table's own relations are
 * drawn: a foreign key between two of its neighbours is a long line across the
 * canvas that says nothing about the table you opened. An explicit selection is
 * different - the point of picking a handful of tables is to see how *they*
 * relate - so there every relation among them is drawn.
 *
 * @param {Rel[]} rels @param {Set<string>} visible
 * @param {{ focusTable?: string, scope?: Scope, picked?: Set<string> | null }} [opts]
 * @returns {Rel[]}
 */
export function visibleRels(rels, visible, opts = {}) {
  const { focusTable = '', scope = 'all', picked = null } = opts
  const within = rels.filter(r => visible.has(r.source) && visible.has(r.target))
  const focusOnly = !(picked && picked.size) && focusTable && scope === 'related'
  return focusOnly
    ? within.filter(r => r.source === focusTable || r.target === focusTable)
    : within
}

/**
 * One line per pair of tables, not one per foreign key column.
 *
 * A table that carries `created_by` and `updated_by`, both pointing at
 * `users.id`, produced two relationships that leave the same card, cross the
 * same corridors and land on the same row. They are drawn twice, routed twice,
 * and tell the reader one thing: these two tables are related. On a real schema
 * that is a third of the lines on the canvas for no information at all.
 *
 * So parallel foreign keys in the same direction collapse into one line carrying
 * the count. Direction is part of the identity: `a -> b` and `b -> a` are two
 * different facts and stay two lines. The columns are not lost either - every
 * one of them still shows its `fk` badge in its own row.
 *
 * The survivor is the first of the group in the order given, so its ports are a
 * real column's ports and the layout still anchors the line on a row that means
 * something.
 *
 * Cardinality is merged the way a reader would read it: the pair is "many" if
 * any of its keys can repeat, and optional only if every one of them is
 * nullable - a single NOT NULL foreign key makes the relationship mandatory.
 *
 * @template {{ id: string, source: string, target: string, many?: boolean, optional?: boolean }} E
 * @param {E[]} edges
 * @returns {(E & { mergedCount: number, mergedIds: string[] })[]}
 */
export function mergeParallelEdges(edges) {
  /** @type {Map<string, (E & { mergedCount: number, mergedIds: string[] })>} */
  const bySide = new Map()
  /** @type {(E & { mergedCount: number, mergedIds: string[] })[]} */
  const out = []
  for (const e of edges) {
    const key = `${e.source}\u0000${e.target}`
    const head = bySide.get(key)
    if (!head) {
      const merged = { ...e, mergedCount: 1, mergedIds: [e.id] }
      bySide.set(key, merged)
      out.push(merged)
      continue
    }
    head.mergedCount += 1
    head.mergedIds.push(e.id)
    if (e.many) head.many = true
    if (!e.optional) head.optional = false
  }
  return out
}

/** A table is a hub once this many tables carry a key to it... */
export const HUB_MIN_REFS = 6
/** ...and that is at least this share of the tables on the diagram... */
export const HUB_SHARE = 0.15
/** ...or this many whatever the share: 25 lines into one card is a fan on any page. */
export const HUB_ABS_REFS = 25

/**
 * The hub tables of a diagram: the ones most of it points at.
 *
 * `tenant_id` on every table of a multi-tenant schema, `user_id` on half of
 * them. Drawn, those are a line from nearly every card to one place - most of
 * the ink on the page, and none of its structure. A hub's relationships are
 * kept and shown on demand; the clusters form around the links that remain.
 *
 * Counted per source table, not per key, so two keys into `users` from one
 * table are one vote.
 * @param {{ source: string, target: string }[]} edges
 * @param {number} tableCount tables on the diagram
 * @returns {Set<string>}
 */
export function hubTables(edges, tableCount) {
  /** @type {Map<string, Set<string>>} */
  const from = new Map()
  for (const e of edges) {
    if (e.source === e.target) continue
    let s = from.get(e.target)
    if (!s) { s = new Set(); from.set(e.target, s) }
    s.add(e.source)
  }
  const min = Math.min(HUB_ABS_REFS, Math.max(HUB_MIN_REFS, Math.round(tableCount * HUB_SHARE)))
  const out = new Set()
  for (const [t, s] of from) if (s.size >= min) out.add(t)
  return out
}
