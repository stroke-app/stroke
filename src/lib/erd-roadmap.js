/**
 * The Hierarchy view: the tables on the page drawn top to bottom in
 * foreign-key order, read like a learning roadmap. A table sits below every
 * table it points at, so the top row is what the rest is built on, and a line
 * runs down from a table to each table that references it.
 *
 * ELK layered places and routes. Every card has one port in the middle of its
 * bottom edge and one in the middle of its top edge, and all lines share them:
 * the lines out of a table leave as one trunk and branch, the lines into a
 * table meet before they land. That is the roadmap look, and far less ink than
 * a line per key.
 *
 * Measured on generated schemas (one or two keys per table, skewed towards the
 * early tables): network-simplex layering and placement lay out 120 tables in
 * 0.2s and 320 in 1.2s at a third of the width Brandes-Köpf gives. Coffman-
 * Graham layering bounds the row width but put root tables at the bottom and,
 * with network-simplex placement, took 23s for 320 tables. Past PLACE_NS_MAX
 * cards placement drops to Brandes-Köpf, which stays fast at any size.
 */
import { readElkResult, runElk } from './erd-elk.js'

/** @typedef {{ x: number, y: number }} Pt */
/** @typedef {{ id: string, w: number, h: number }} RoadmapCard */
/**
 * @typedef {{ source: string, target: string, sourceCol?: string | null }} RoadmapRel
 * `source` holds the foreign key, `target` is the table it points at.
 * @typedef {{ id: string, from: string, to: string, cols: string[] }} RoadmapEdge
 * `from` is the referenced table, drawn above; `to` holds the key, drawn below.
 * @typedef {{ up: Map<string, Set<string>>, down: Map<string, Set<string>> }} Adjacency
 */

/** Between rows of cards. Leaves room for a trunk to branch in the channel. */
export const LAYER_GAP = 64
/** Between cards in a row. */
export const NODE_GAP = 28
/** Between the drawing and the grid of tables with no lines under it. */
export const LOOSE_GAP = 72
/** Height of the band over the grid: its heading, then room for a hub's pill above its card. */
export const LOOSE_LABEL = 48
/** Up to this many cards in the drawing, network-simplex placement (see above). */
export const PLACE_NS_MAX = 400

/**
 * One edge per pair of tables, pointing down from the referenced table to the
 * one holding the key, named after every column that joins them. Self
 * references and tables off the page are dropped, and so are links into
 * `skip` (the hubs, when their lines are hidden).
 * @param {RoadmapRel[]} rels
 * @param {Set<string>} ids
 * @param {Set<string>} [skip]
 * @returns {RoadmapEdge[]}
 */
export function roadmapEdges(rels, ids, skip) {
  /** @type {Map<string, RoadmapEdge>} */
  const byPair = new Map()
  for (const r of rels) {
    if (r.source === r.target || !ids.has(r.source) || !ids.has(r.target) || skip?.has(r.target)) continue
    const key = `${r.target}\u0000${r.source}`
    let e = byPair.get(key)
    if (!e) {
      e = { id: `e${byPair.size}`, from: r.target, to: r.source, cols: [] }
      byPair.set(key, e)
    }
    if (r.sourceCol && !e.cols.includes(r.sourceCol)) e.cols.push(r.sourceCol)
  }
  return [...byPair.values()]
}

/**
 * Who each table points at (`up`) and who points at it (`down`), self
 * references left out.
 * @param {RoadmapRel[]} rels
 * @returns {Adjacency}
 */
export function adjacency(rels) {
  /** @type {Map<string, Set<string>>} */
  const up = new Map()
  /** @type {Map<string, Set<string>>} */
  const down = new Map()
  const add = (/** @type {Map<string, Set<string>>} */ m, /** @type {string} */ k, /** @type {string} */ v) => {
    let s = m.get(k)
    if (!s) { s = new Set(); m.set(k, s) }
    s.add(v)
  }
  for (const r of rels) {
    if (r.source === r.target) continue
    add(up, r.source, r.target)
    add(down, r.target, r.source)
  }
  return { up, down }
}

/**
 * A table's line through the schema: everything it points at, all the way up,
 * and everything that points at it, all the way down. Includes the table.
 * @param {string} id
 * @param {Adjacency} adj
 */
export function lineage(id, adj) {
  const out = new Set([id])
  for (const m of [adj.up, adj.down]) {
    const queue = [id]
    const seen = new Set([id])
    while (queue.length) {
      const next = m.get(/** @type {string} */ (queue.pop()))
      if (!next) continue
      for (const t of next) {
        if (seen.has(t)) continue
        seen.add(t)
        out.add(t)
        queue.push(t)
      }
    }
  }
  return out
}

/**
 * The ELK graph: top to bottom, one port at the middle of each card's bottom
 * (lines out) and top (lines in).
 * @param {RoadmapCard[]} cards
 * @param {RoadmapEdge[]} edges
 * @param {number} [aspect] width over height the packed components aim for
 */
export function buildRoadmapGraph(cards, edges, aspect = 1.6) {
  const n = cards.length
  return {
    id: 'root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': 'DOWN',
      'elk.edgeRouting': 'ORTHOGONAL',
      'elk.padding': '[top=0,left=0,bottom=0,right=0]',
      'elk.spacing.nodeNode': String(NODE_GAP),
      'elk.layered.spacing.nodeNodeBetweenLayers': String(LAYER_GAP),
      'elk.spacing.edgeNode': '16',
      'elk.layered.spacing.edgeNodeBetweenLayers': '20',
      'elk.spacing.edgeEdge': '10',
      'elk.layered.spacing.edgeEdgeBetweenLayers': '10',
      'elk.layered.layering.strategy': 'NETWORK_SIMPLEX',
      'elk.layered.nodePlacement.strategy': n <= PLACE_NS_MAX ? 'NETWORK_SIMPLEX' : 'BRANDES_KOEPF',
      'elk.layered.nodePlacement.bk.fixedAlignment': 'BALANCED',
      'elk.layered.crossingMinimization.strategy': 'LAYER_SWEEP',
      'elk.layered.thoroughness': n <= 80 ? '7' : '3',
      // Stable: the same schema lays out the same way twice.
      'elk.layered.considerModelOrder.strategy': 'NODES_AND_EDGES',
      'elk.layered.cycleBreaking.strategy': 'GREEDY_MODEL_ORDER',
      'elk.separateConnectedComponents': 'true',
      'elk.spacing.componentComponent': String(LAYER_GAP),
      'elk.aspectRatio': String(aspect),
    },
    children: cards.map((c) => ({
      id: c.id,
      width: c.w,
      height: c.h,
      layoutOptions: { 'elk.portConstraints': 'FIXED_POS' },
      ports: [
        { id: `${c.id}#in`, x: c.w / 2 - 0.5, y: -0.5, width: 1, height: 1, layoutOptions: { 'elk.port.side': 'NORTH' } },
        { id: `${c.id}#out`, x: c.w / 2 - 0.5, y: c.h - 0.5, width: 1, height: 1, layoutOptions: { 'elk.port.side': 'SOUTH' } },
      ],
    })),
    edges: edges.map((e) => ({ id: e.id, sources: [`${e.from}#out`], targets: [`${e.to}#in`] })),
  }
}

/**
 * Cards in rows no wider than `maxW`, each row centred on `width`.
 * @param {RoadmapCard[]} cards
 * @param {number} top
 * @param {number} maxW
 * @param {number} width
 * @returns {{ pos: Map<string, Pt>, bottom: number }}
 */
export function packRows(cards, top, maxW, width) {
  /** @type {Map<string, Pt>} */
  const pos = new Map()
  /** @type {RoadmapCard[][]} */
  const rows = []
  let row = /** @type {RoadmapCard[]} */ ([])
  let rowW = 0
  for (const c of cards) {
    const add = (row.length ? NODE_GAP : 0) + c.w
    if (row.length && rowW + add > maxW) {
      rows.push(row)
      row = []
      rowW = 0
    }
    rowW += (row.length ? NODE_GAP : 0) + c.w
    row.push(c)
  }
  if (row.length) rows.push(row)
  let y = top
  for (const r of rows) {
    const w = r.reduce((s, c, i) => s + c.w + (i ? NODE_GAP : 0), 0)
    const h = Math.max(...r.map((c) => c.h))
    let x = Math.max(0, (width - w) / 2)
    for (const c of r) {
      pos.set(c.id, { x, y })
      x += c.w + NODE_GAP
    }
    y += h + NODE_GAP
  }
  return { pos, bottom: rows.length ? y - NODE_GAP : top }
}

/**
 * Lay out the page: the linked tables as the drawing, the rest in a grid
 * under it.
 * @param {RoadmapCard[]} cards every table on the page
 * @param {RoadmapEdge[]} edges from `roadmapEdges`
 * @param {{ aspect?: number, elk?: any }} [opts]
 * @returns {Promise<{ pos: Map<string, Pt>, routes: Map<string, Pt[]>, loose: string[], looseY: number, width: number, height: number }>}
 */
export async function layoutRoadmap(cards, edges, opts = {}) {
  const linked = new Set()
  for (const e of edges) {
    linked.add(e.from)
    linked.add(e.to)
  }
  const inGraph = cards.filter((c) => linked.has(c.id))
  const loose = cards.filter((c) => !linked.has(c.id))

  /** @type {Map<string, Pt>} */
  let pos = new Map()
  /** @type {Map<string, Pt[]>} */
  let routes = new Map()
  let width = 0
  let height = 0
  if (inGraph.length) {
    const graph = await runElk(buildRoadmapGraph(inGraph, edges, opts.aspect), opts.elk)
    ;({ pos, routes } = readElkResult(graph))
    width = graph.width ?? 0
    height = graph.height ?? 0
  }

  let looseY = 0
  if (loose.length) {
    looseY = inGraph.length ? height + LOOSE_GAP : 0
    // As wide as the drawing, and never narrower than eight average cards,
    // so a page of unlinked tables is a block and not one long strip.
    const avg = loose.reduce((s, c) => s + c.w, 0) / loose.length
    const maxW = Math.max(width, 8 * avg + 7 * NODE_GAP)
    // A grid wider than the drawing moves the drawing to its middle.
    const dx = Math.max(0, (maxW - width) / 2)
    if (dx) {
      for (const p of pos.values()) p.x += dx
      for (const pts of routes.values()) for (const p of pts) p.x += dx
    }
    const grid = packRows(loose, looseY + LOOSE_LABEL, maxW, Math.max(width, maxW))
    for (const [id, p] of grid.pos) pos.set(id, p)
    width = Math.max(width, maxW)
    height = grid.bottom
  }
  return { pos, routes, loose: loose.map((c) => c.id), looseY, width, height }
}

/**
 * An orthogonal polyline as an SVG path with its corners rounded. A corner
 * never takes more than half of either segment it joins, so short jogs stay
 * on their line.
 * @param {Pt[]} pts
 * @param {number} [r]
 */
export function roundedPath(pts, r = 10) {
  /** @type {Pt[]} */
  const p = []
  for (const q of pts) {
    const last = p[p.length - 1]
    if (last && Math.abs(last.x - q.x) < 0.01 && Math.abs(last.y - q.y) < 0.01) continue
    // A point in the middle of a straight run is no corner.
    const prev = p[p.length - 2]
    if (prev && last && ((prev.x === last.x && last.x === q.x) || (prev.y === last.y && last.y === q.y))) p.pop()
    p.push(q)
  }
  if (p.length < 2) return ''
  const f = (/** @type {number} */ v) => Math.round(v * 10) / 10
  let d = `M${f(p[0].x)},${f(p[0].y)}`
  for (let i = 1; i < p.length - 1; i++) {
    const a = p[i - 1], b = p[i], c = p[i + 1]
    const l1 = Math.hypot(b.x - a.x, b.y - a.y)
    const l2 = Math.hypot(c.x - b.x, c.y - b.y)
    const k = Math.min(r, l1 / 2, l2 / 2)
    const x1 = b.x + ((a.x - b.x) / l1) * k, y1 = b.y + ((a.y - b.y) / l1) * k
    const x2 = b.x + ((c.x - b.x) / l2) * k, y2 = b.y + ((c.y - b.y) / l2) * k
    d += `L${f(x1)},${f(y1)}Q${f(b.x)},${f(b.y)} ${f(x2)},${f(y2)}`
  }
  const end = p[p.length - 1]
  return `${d}L${f(end.x)},${f(end.y)}`
}
