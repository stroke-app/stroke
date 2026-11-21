/**
 * Post-layout separation for the ER diagram.
 *
 * Two cards must never sit on top of each other, and every pair needs a channel
 * between them wide enough for the relationship lines that run there. Dagre
 * reserves space for the cards it lays out, but that guarantee doesn't survive
 * everything that happens afterwards - a hand-dragged card keeps its saved
 * position, a re-flowed rank packs sub-columns on its own terms, and a card whose
 * column list changed is a different size than the one the layout measured. Any
 * of those can leave cards overlapping, and an overlapping card is worse than a
 * crossed line: it hides data.
 *
 * So the positions are pushed apart until every pair is clear, as the last step
 * before anything is drawn. It's a relaxation, not a re-layout: cards move as
 * little as possible, so the shape Dagre found survives.
 */

/**
 * @typedef {{ id: string, x: number, y: number, w: number, h: number }} Card
 */

/**
 * A pass is one sweep over every pair. Relaxation is the right tool when a
 * layout is mostly clear and a few cards need nudging, which is the real case
 * (Dagre output plus a handful of hand-dragged cards) - but it converges slowly,
 * and from a degenerate start (everything piled on everything) it oscillates
 * instead of settling. So the passes are capped and anything still overlapping
 * afterwards is spilled into open space, which always terminates.
 */
const MAX_PASSES = 60

/**
 * Push cards apart until no two are closer than the given gaps.
 *
 * Overlap is resolved along the axis of *least* penetration, which is what keeps
 * a row a row and a column a column - correcting the long way would scatter the
 * layout into noise. Both cards in a pair move half the correction each, so no
 * single card drifts far from where the layout put it.
 *
 * @param {Card[]} cards
 * @param {{ gapX?: number, gapY?: number, fixed?: Set<string> }} [opts]
 *   `fixed` cards never move (the focused table, say); their partners absorb the
 *   whole correction instead.
 * @returns {Map<string, { x: number, y: number }>} id → adjusted top-left
 */
/** Stable left-to-right order. Ties break on y then id so a re-run agrees. */
const byPos = (/** @type {Card} */ a, /** @type {Card} */ b) =>
  a.x - b.x || a.y - b.y || (a.id < b.id ? -1 : 1)

export function separateCards(cards, opts = {}) {
  const gapX = opts.gapX ?? 120
  const gapY = opts.gapY ?? 80
  const fixed = opts.fixed ?? new Set()

  // Work on a copy, in a stable order: two runs on the same input must agree,
  // or the diagram would jitter every time it re-renders.
  const items = cards.map((c) => ({ ...c })).sort(byPos)

  for (let pass = 0; pass < MAX_PASSES; pass++) {
    // Re-sort every pass, because the sweep below depends on the order: cards
    // are visited left to right, so once a candidate starts beyond the current
    // card's x-extent plus the gap, no later candidate can overlap it either and
    // the inner loop stops. That turns a full pass from every-pair into
    // every-neighbour, which is the difference between a schema of 600 tables
    // laying out in milliseconds and freezing the window for seconds.
    items.sort(byPos)
    let moved = false
    for (let i = 0; i < items.length; i++) {
      for (let j = i + 1; j < items.length; j++) {
        const a = items[i], b = items[j]
        if (b.x >= a.x + a.w + gapX) break
        // Penetration of the two rects once each is grown by half a gap.
        const overlapX = Math.min(a.x + a.w + gapX, b.x + b.w + gapX) - Math.max(a.x, b.x)
        if (overlapX <= 0) continue
        const overlapY = Math.min(a.y + a.h + gapY, b.y + b.h + gapY) - Math.max(a.y, b.y)
        if (overlapY <= 0) continue

        // The real gap needed is one gap, not two - measure against the pair.
        const needX = (a.w + b.w) / 2 + gapX
        const needY = (a.h + b.h) / 2 + gapY
        const dx = (b.x + b.w / 2) - (a.x + a.w / 2)
        const dy = (b.y + b.h / 2) - (a.y + a.h / 2)
        const pushX = needX - Math.abs(dx)
        const pushY = needY - Math.abs(dy)
        if (pushX <= 0 || pushY <= 0) continue

        moved = true
        // Cheaper direction wins. A dead heat (two cards at the same point)
        // resolves vertically, which reads better than a sideways drift.
        if (pushY <= pushX) {
          const sign = dy === 0 ? (a.id < b.id ? 1 : -1) : Math.sign(dy)
          shift(a, b, 0, -sign * pushY, fixed)
        } else {
          const sign = dx === 0 ? (a.id < b.id ? 1 : -1) : Math.sign(dx)
          shift(a, b, -sign * pushX, 0, fixed)
        }
      }
    }
    if (!moved) break
  }

  spillStragglers(items, gapX, gapY, fixed)

  const out = new Map()
  for (const it of items) out.set(it.id, { x: Math.round(it.x), y: Math.round(it.y) })
  return out
}

/**
 * Last resort: give any card still overlapping its own place in open space.
 *
 * Relaxation can stall - three cards in a tight triangle push each other around
 * forever - and a diagram is allowed to be wider, but never allowed to hide a
 * card behind another. Stragglers are laid out in a grid below everything that
 * did settle, in stable order, so the result is deterministic and the invariant
 * holds no matter what came in.
 * @param {Card[]} items @param {number} gapX @param {number} gapY @param {Set<string>} fixed
 */
function spillStragglers(items, gapX, gapY, fixed) {
  const overlaps = (/** @type {Card} */ a, /** @type {Card} */ b) =>
    !(b.x >= a.x + a.w + gapX || a.x >= b.x + b.w + gapX) &&
    !(b.y >= a.y + a.h + gapY || a.y >= b.y + b.h + gapY)

  // Cards are indexed into a uniform grid whose cell is one card plus its gap,
  // so a card spans at most two cells per axis and a candidate only has to be
  // compared against the handful of cards sharing its own cells. Scanning every
  // card already kept is what made this quadratic on a large schema.
  let cellW = 1, cellH = 1
  for (const it of items) {
    cellW = Math.max(cellW, it.w + gapX)
    cellH = Math.max(cellH, it.h + gapY)
  }
  /** @type {Map<string, Card[]>} */
  const buckets = new Map()
  /** Cell keys covered by a card's expanded rect. @param {Card} c */
  const cellsOf = (c) => {
    /** @type {string[]} */
    const keys = []
    const cx0 = Math.floor(c.x / cellW), cx1 = Math.floor((c.x + c.w + gapX) / cellW)
    const cy0 = Math.floor(c.y / cellH), cy1 = Math.floor((c.y + c.h + gapY) / cellH)
    for (let cx = cx0; cx <= cx1; cx++)
      for (let cy = cy0; cy <= cy1; cy++) keys.push(`${cx},${cy}`)
    return keys
  }

  /** @type {Card[]} */
  const kept = []
  /** @type {Card[]} */
  const spill = []
  for (const it of items) {
    const keys = cellsOf(it)
    // A pinned card is never the one that moves.
    let clear = true
    if (!fixed.has(it.id)) {
      for (const key of keys) {
        const near = buckets.get(key)
        if (near && near.some((k) => overlaps(k, it))) { clear = false; break }
      }
    }
    if (!clear) { spill.push(it); continue }
    kept.push(it)
    for (const key of keys) {
      const list = buckets.get(key)
      if (list) list.push(it)
      else buckets.set(key, [it])
    }
  }
  if (!spill.length) return

  let baseX = 0
  let baseY = -Infinity
  let colW = 0
  let rowH = 0
  if (kept.length) {
    baseX = Infinity
    for (const k of kept) {
      baseX = Math.min(baseX, k.x)
      baseY = Math.max(baseY, k.y + k.h)
    }
    baseY += gapY * 2
  } else {
    baseY = 0
  }
  for (const c of spill) {
    colW = Math.max(colW, c.w)
    rowH = Math.max(rowH, c.h)
  }
  colW += gapX
  rowH += gapY
  // Roughly square, so a big spill spreads across the canvas instead of forming
  // one endless column.
  const cols = Math.max(1, Math.ceil(Math.sqrt(spill.length)))
  spill.forEach((c, i) => {
    c.x = baseX + (i % cols) * colW
    c.y = baseY + Math.floor(i / cols) * rowH
  })
}

/**
 * Move `a` by (dx, dy) and `b` the opposite way, splitting the correction -
 * unless one of them is pinned, in which case the other takes all of it.
 * @param {Card} a @param {Card} b @param {number} dx @param {number} dy @param {Set<string>} fixed
 */
function shift(a, b, dx, dy, fixed) {
  const aFixed = fixed.has(a.id), bFixed = fixed.has(b.id)
  if (aFixed && bFixed) return // nothing we're allowed to move
  if (aFixed) { b.x -= dx; b.y -= dy; return }
  if (bFixed) { a.x += dx; a.y += dy; return }
  a.x += dx / 2
  a.y += dy / 2
  b.x -= dx / 2
  b.y -= dy / 2
}

/**
 * True when every pair of cards clears the given gaps. Used by the tests, and
 * cheap enough to assert with when something looks wrong.
 * @param {Card[]} cards @param {number} gapX @param {number} gapY
 */
export function cardsAreSeparated(cards, gapX = 0, gapY = 0) {
  for (let i = 0; i < cards.length; i++) {
    for (let j = i + 1; j < cards.length; j++) {
      const a = cards[i], b = cards[j]
      const clearX = b.x >= a.x + a.w + gapX || a.x >= b.x + b.w + gapX
      const clearY = b.y >= a.y + a.h + gapY || a.y >= b.y + b.h + gapY
      if (!clearX && !clearY) return false
    }
  }
  return true
}

/**
 * Wrap over-tall ranks into sub-columns.
 *
 * A ranked layout puts every node of a rank in one column, and a schema with a
 * hub puts most of the schema in one rank: a `users` table that ninety others
 * carry a foreign key to gives a second rank ninety cards tall. The diagram comes
 * out around 1:10 - a vertical strip unreadable at any zoom, every line in it
 * running the full height. Nothing about the layout is wrong; that is just the
 * shape a rank is.
 *
 * So a rank taller than the page is folded into as many sub-columns as it needs,
 * in the order the layout chose, and the ranks after it shift right to make room.
 * The crossing work the layout did is kept - only the column it did it in
 * changes.
 *
 * Two things this deliberately does not do. It leaves a diagram that is already a
 * reasonable shape completely alone, because folding one costs its vertical
 * alignment for nothing. And when it does fold, it re-flows every rank from a
 * shared top: a rank that keeps its old absolute y sits thousands of pixels below
 * the rank it belongs beside, which was most of the height in the first place.
 *
 * Cards land on a uniform grid per rank, so no two can overlap by construction.
 * The layout's own edge corridors do NOT survive: the space they were reserved in
 * has moved, so a caller that folds has to route those lines instead.
 *
 * @param {Card[]} cards top-left positions
 * @param {{ gapX: number, gapY: number, aspect?: number, minFold?: number }} opts
 * @returns {{ moved: boolean, pos: Map<string, { x: number, y: number }> }}
 */
export function wrapTallRanks(cards, opts) {
  /** @type {Map<string, { x: number, y: number }>} */
  const pos = new Map()
  const identity = () => {
    for (const c of cards) pos.set(c.id, { x: c.x, y: c.y })
    return { moved: false, pos }
  }
  if (cards.length < 2) return identity()

  const gapX = opts.gapX
  const gapY = opts.gapY
  const aspect = opts.aspect ?? 16 / 9
  // Fewer than this in a rank and folding buys a column or two of nothing.
  const minFold = opts.minFold ?? 5

  // A rank is a column of cards sharing an x - which is exactly what the layout
  // produces, so grouping on a rounded x recovers it.
  /** @type {Map<number, Card[]>} */
  const ranks = new Map()
  for (const c of cards) {
    const k = Math.round(c.x)
    const at = ranks.get(k)
    if (at) at.push(c)
    else ranks.set(k, [c])
  }
  const keys = [...ranks.keys()].sort((a, b) => a - b)

  let x0 = Infinity, x1 = -Infinity, y0 = Infinity, y1 = -Infinity
  let area = 0
  let tallest = 0
  for (const c of cards) {
    x0 = Math.min(x0, c.x); x1 = Math.max(x1, c.x + c.w)
    y0 = Math.min(y0, c.y); y1 = Math.max(y1, c.y + c.h)
    area += (c.w + gapX) * (c.h + gapY)
    tallest = Math.max(tallest, c.h + gapY)
  }
  // Already a shape you can read: leave it exactly as the layout left it.
  if (y1 - y0 <= (x1 - x0) * 2) return identity()

  // The height a layout of this much card area would have at the target aspect.
  const targetH = Math.max(tallest, Math.sqrt(area / aspect))

  /** Columns a rank folds into at `targetH`. @param {Card[]} rank */
  const foldInto = (rank) => {
    /** @type {Card[][]} */
    const columns = [[]]
    let colH = 0
    for (const c of rank) {
      const step = c.h + gapY
      // Never leave a column empty: the first card goes in whatever it is.
      if (colH > 0 && colH + step > targetH) {
        columns.push([])
        colH = 0
      }
      columns[columns.length - 1].push(c)
      colH += step
    }
    return columns
  }

  /** Stable: the layout's own vertical order, ties broken on id. @param {Card[]} r */
  const ordered = (r) => [...r].sort((a, b) => a.y - b.y || (a.id < b.id ? -1 : 1))

  const folds = keys.map((k) => {
    const rank = ordered(ranks.get(k) ?? [])
    return rank.length >= minFold ? foldInto(rank) : [rank]
  })
  if (!folds.some((f) => f.length > 1)) return identity()

  let x = x0
  for (const columns of folds) {
    let colW = 0
    for (const col of columns) {
      // Every rank starts at the same top. See the note above.
      let y = y0
      let w = 0
      for (const c of col) {
        pos.set(c.id, { x: Math.round(x + colW), y: Math.round(y) })
        y += c.h + gapY
        w = Math.max(w, c.w)
      }
      colW += w + gapX
    }
    x += colW
  }
  return { moved: true, pos }
}

/**
 * Tables with no relations, packed as masonry on a landscape page.
 *
 * They went into a grid of at most six columns whose every row was as tall as
 * the tallest of them: a 40-column table set the pitch for rows of 5-column
 * ones, and PostGIS's 34 `tiger` tables fit to a narrow ribbon of boxes too
 * small to read. Here the column count aims at 16:9 for the cards' total area,
 * and each card drops into the shortest column so far, in input order.
 * @param {{ id: string, w: number, h: number }[]} cards all the same width
 * @param {{ gapX: number, gapY: number, x?: number, y?: number, aspect?: number }} opts
 * @returns {Map<string, { x: number, y: number }>}
 */
export function packOrphans(cards, opts) {
  /** @type {Map<string, { x: number, y: number }>} */
  const out = new Map()
  if (!cards.length) return out
  const { gapX, gapY, x = 0, y = 0, aspect = 16 / 9 } = opts
  const colW = Math.max(...cards.map((c) => c.w)) + gapX
  const area = cards.reduce((sum, c) => sum + colW * (c.h + gapY), 0)
  const cols = Math.max(1, Math.min(cards.length, Math.round(Math.sqrt(area * aspect) / colW)))
  const heights = new Array(cols).fill(0)
  for (const c of cards) {
    let k = 0
    for (let i = 1; i < cols; i++) if (heights[i] < heights[k]) k = i
    out.set(c.id, { x: x + k * colW, y: y + heights[k] })
    heights[k] += c.h + gapY
  }
  return out
}
