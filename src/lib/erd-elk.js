/**
 * ER diagram layout and line routing through ELK's layered algorithm.
 *
 * One engine does both jobs, which is the point. Dagre placed the cards and a
 * router of ours drew the lines afterwards, and the two never agreed: the
 * layout's own corridors went stale the moment a rank was folded or a card was
 * separated, lines then crossed cards, every line to a hub ran down the same
 * pixel column, and the A* that replaced them landed a few per frame, so the
 * diagram shifted under the reader for seconds. ELK routes while it places, so
 * a line is clear of every card by construction, parallel lines get a track
 * each inside a channel, and the whole picture arrives at once.
 *
 * Two passes per component. The first, without ports, decides which side of
 * each card a line leaves from (the layering fixes left and right). The second
 * pins every line to the row it is about - the foreign key column on one card,
 * the referenced column on the other - and routes.
 *
 * The hub problem (ninety tables carrying a key to `users` make one rank ninety
 * cards tall) is bounded here rather than folded afterwards: Coffman-Graham
 * layering caps the cards per layer, so a wide fan spreads across layers and
 * the lines are routed through the channels in between.
 *
 * Runs in a worker; the layout of a large schema is hundreds of milliseconds
 * the window should not freeze for.
 */
import ELK from 'elkjs/lib/elk-api.js'
import workerUrl from 'elkjs/lib/elk-worker.min.js?url'
import { STUB } from './erd-routing.js'

/** @typedef {{ x: number, y: number }} Pt */
/**
 * @typedef {{ id: string, w: number, h: number, rowY: (col: string | null) => number }} ElkCard
 * `rowY` is the y (from the card's top) of the row a line should land on, or
 * the header's middle when the column is not shown.
 * @typedef {{ id: string, source: string, target: string, sourceCol: string | null, targetCol: string | null }} ElkLink
 */

/** A line keeps this much from any card it passes, and its first bend sits
 *  this far from the card it leaves - past the stub the renderer draws. */
export const EDGE_NODE_GAP = STUB + 4
/** Parallel lines in one channel sit this far apart. */
export const EDGE_EDGE_GAP = 10
/** Up to this many links every line has its own track; above, lines into one row share a trunk. */
export const TRACKS_UP_TO = 60

/** @type {any} */
let _elk = null
/** Set once the worker failed; the bundled build then runs on the main thread. */
let _bundled = false

async function getElk() {
  if (_elk) return _elk
  if (!_bundled && typeof Worker !== 'undefined') {
    _elk = new ELK({ workerFactory: () => new Worker(workerUrl) })
    return _elk
  }
  const mod = await import('elkjs/lib/elk.bundled.js')
  _elk = new (mod.default ?? mod)()
  return _elk
}

/**
 * Cards per layer. The square root keeps the page near landscape whatever the
 * schema's size: 30 tables fold into layers of 6, 135 into layers of 12.
 * @param {number} n
 */
export function cardsPerLayer(n) {
  return Math.max(4, Math.min(12, Math.round(Math.sqrt(n) * 1.1)))
}

/**
 * The ELK graph for one pass.
 * @param {ElkCard[]} cards
 * @param {ElkLink[]} links
 * @param {{ rankSep: number, nodeSep: number, perLayer: number, sides?: Map<string, 1 | -1> | null, sharedPorts?: boolean }} opts
 *   `sides` is per link: 1 leaves the source's east side into the target's
 *   west, -1 the other way round. Without it no ports are placed at all.
 *   `sharedPorts` lets lines landing on one row share a port, which ELK merges
 *   into one trunk: less ink, at the cost of telling them apart in the channel.
 */
export function buildElkGraph(cards, links, opts) {
  const sides = opts.sides ?? null
  /** @type {Map<string, { id: string, x: number, y: number, width: number, height: number, layoutOptions: Record<string, string> }[]>} */
  const ports = new Map()
  /**
   * One port per line end, even where several lines land on the same row:
   * ELK merges every line that shares a port into one track, which is the
   * single trunk this replaces. Ten lines into `users.id` are ten tracks a
   * reader can follow, converging at the row they point at.
   */
  const portFor = (/** @type {ElkCard} */ card, /** @type {'EAST' | 'WEST'} */ side, /** @type {string | null} */ col, /** @type {string} */ link) => {
    const y = card.rowY(col)
    const id = opts.sharedPorts ? `${card.id}#${side}#${y}` : `${card.id}#${side}#${y}#${link}`
    let list = ports.get(card.id)
    if (!list) { list = []; ports.set(card.id, list) }
    if (!list.some((p) => p.id === id)) {
      list.push({
        id, width: 1, height: 1,
        x: side === 'EAST' ? card.w - 0.5 : -0.5, y: y - 0.5,
        layoutOptions: { 'elk.port.side': side },
      })
    }
    return id
  }
  const byId = new Map(cards.map((c) => [c.id, c]))

  const edges = links.map((l) => {
    const side = sides?.get(l.id)
    if (!side) return { id: l.id, sources: [l.source], targets: [l.target] }
    const s = byId.get(l.source), t = byId.get(l.target)
    if (!s || !t) return { id: l.id, sources: [l.source], targets: [l.target] }
    return {
      id: l.id,
      sources: [portFor(s, side === 1 ? 'EAST' : 'WEST', l.sourceCol, l.id)],
      targets: [portFor(t, side === 1 ? 'WEST' : 'EAST', l.targetCol, l.id)],
    }
  })

  return {
    id: 'root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': 'RIGHT',
      'elk.edgeRouting': 'ORTHOGONAL',
      'elk.padding': '[top=0,left=0,bottom=0,right=0]',
      'elk.spacing.nodeNode': String(opts.nodeSep),
      'elk.layered.spacing.nodeNodeBetweenLayers': String(opts.rankSep),
      'elk.spacing.edgeNode': String(EDGE_NODE_GAP),
      'elk.layered.spacing.edgeNodeBetweenLayers': String(EDGE_NODE_GAP),
      'elk.spacing.edgeEdge': String(EDGE_EDGE_GAP),
      'elk.layered.spacing.edgeEdgeBetweenLayers': String(EDGE_EDGE_GAP),
      'elk.layered.layering.strategy': 'COFFMAN_GRAHAM',
      'elk.layered.layering.coffmanGraham.layerBound': String(opts.perLayer),
      // Measured on 135 tables / 161 links: network-simplex placement with
      // thoroughness 7 took 17s for the first pass and 28s for the second.
      // Brandes-Köpf with thoroughness 3 lands the same schema in well under a
      // second, and the difference on screen is a few straight edges.
      'elk.layered.nodePlacement.strategy': 'BRANDES_KOEPF',
      'elk.layered.nodePlacement.bk.fixedAlignment': 'BALANCED',
      'elk.layered.crossingMinimization.strategy': 'LAYER_SWEEP',
      'elk.layered.thoroughness': '3',
      'elk.layered.unnecessaryBendpoints': 'true',
      'elk.layered.mergeEdges': 'false',
      // Stable: the same schema lays out the same way twice.
      'elk.layered.considerModelOrder.strategy': 'NODES_AND_EDGES',
      'elk.layered.cycleBreaking.strategy': 'GREEDY_MODEL_ORDER',
    },
    children: cards.map((c) => ({
      id: c.id, width: c.w, height: c.h,
      layoutOptions: { 'elk.portConstraints': sides ? 'FIXED_POS' : 'FREE' },
      ...(sides ? { ports: ports.get(c.id) ?? [] } : {}),
    })),
    edges,
  }
}

/**
 * Positions and routes out of a laid-out ELK graph. Coordinates come back
 * absolute (a flat graph, no padding), as the canvas wants them.
 * @param {any} graph
 */
export function readElkResult(graph) {
  /** @type {Map<string, Pt>} */
  const pos = new Map()
  for (const n of graph.children ?? []) pos.set(n.id, { x: n.x ?? 0, y: n.y ?? 0 })
  /** @type {Map<string, Pt[]>} */
  const routes = new Map()
  for (const e of graph.edges ?? []) {
    const s = e.sections?.[0]
    if (!s) continue
    routes.set(e.id, [s.startPoint, ...(s.bendPoints ?? []), s.endPoint].map((p) => ({ x: p.x, y: p.y })))
  }
  return { pos, routes }
}

/**
 * Lay out one connected component: card positions (top-left) and one
 * orthogonal polyline per link, port to port.
 * @param {ElkCard[]} cards
 * @param {ElkLink[]} links
 * @param {{ rankSep: number, nodeSep: number, perLayer?: number, elk?: any }} opts
 * @returns {Promise<{ pos: Map<string, Pt>, routes: Map<string, Pt[]> }>}
 */
export async function layoutWithElk(cards, links, opts) {
  const elk = opts.elk ?? (await getElk())
  const perLayer = opts.perLayer ?? cardsPerLayer(cards.length)
  // A track per line is readable up to a point; past it the channels are
  // bands of parallel lines wider than the cards. Big pages get trunks.
  const sharedPorts = links.length > TRACKS_UP_TO
  const base = { rankSep: opts.rankSep, nodeSep: opts.nodeSep, perLayer, sharedPorts }

  const run = async (/** @type {Map<string, 1 | -1> | null} */ sides) =>
    readElkResult(await elk.layout(buildElkGraph(cards, links, { ...base, sides })))

  let first
  try {
    first = await run(null)
  } catch (err) {
    if (opts.elk || _bundled || typeof Worker === 'undefined') throw err
    // The worker could not be started (a packaging or protocol restriction):
    // run the bundled build on the main thread from now on.
    _bundled = true
    _elk = null
    return layoutWithElk(cards, links, { ...opts, elk: await getElk() })
  }
  if (!links.length) return first

  const byId = new Map(cards.map((c) => [c.id, c]))
  /** @type {Map<string, 1 | -1>} */
  const sides = new Map()
  for (const l of links) {
    const s = first.pos.get(l.source), t = first.pos.get(l.target)
    const sw = byId.get(l.source)?.w ?? 0
    // Left to right unless the target sits wholly to the left of the source.
    sides.set(l.id, s && t && t.x + (byId.get(l.target)?.w ?? 0) <= s.x + sw && t.x < s.x ? -1 : 1)
  }
  return run(sides)
}
