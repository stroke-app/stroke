// GeoJSON, projected once.
//
// The map used to walk every feature's GeoJSON on every frame: Mercator
// (a log and a tan) per vertex, an object per projected point and an array
// per ring, for every feature whether it was on screen or not. A drag over a
// few thousand polygons spent its frames in that and in the garbage it left.
//
// Here each geometry is projected to world units (the unit Mercator square)
// once, when it arrives, into flat Float64Arrays with a bounding box. A frame
// is then one multiply-add per visible vertex, and a feature off screen costs
// one box test. Coordinates stay float64 in JS and only become pixels at draw
// time, so street-level zoom keeps its precision.

import { toWorld } from '$lib/geo-map.js'

/**
 * @typedef {Object} Compiled
 * @property {Float64Array} points  x,y pairs of standalone points.
 * @property {{ xy: Float64Array, closed: boolean }[]} rings  Lines and polygon rings.
 * @property {number} polygons  How many of the rings belong to polygons (they come first).
 * @property {[number, number, number, number]} box  minX, minY, maxX, maxY in world units.
 */

/**
 * @param {any} geometry GeoJSON geometry in lon/lat.
 * @returns {Compiled}
 */
export function compileGeometry(geometry) {
  /** @type {number[]} */
  const pts = []
  /** @type {{ xy: Float64Array, closed: boolean }[]} */
  const polyRings = []
  /** @type {{ xy: Float64Array, closed: boolean }[]} */
  const lineRings = []
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity
  const track = (/** @type {number} */ x, /** @type {number} */ y) => {
    if (x < minX) minX = x
    if (x > maxX) maxX = x
    if (y < minY) minY = y
    if (y > maxY) maxY = y
  }
  const point = (/** @type {any} */ c) => {
    if (!Array.isArray(c) || c.length < 2) return
    const w = toWorld(c[0], c[1])
    pts.push(w.x, w.y)
    track(w.x, w.y)
  }
  const ring = (/** @type {any} */ coords, /** @type {boolean} */ closed, /** @type {typeof polyRings} */ into) => {
    if (!Array.isArray(coords) || coords.length < 2) return
    const xy = new Float64Array(coords.length * 2)
    let n = 0
    for (const c of coords) {
      if (!Array.isArray(c) || c.length < 2) continue
      const w = toWorld(c[0], c[1])
      xy[n++] = w.x
      xy[n++] = w.y
      track(w.x, w.y)
    }
    if (n >= 4) into.push({ xy: n === xy.length ? xy : xy.slice(0, n), closed })
  }
  const walk = (/** @type {any} */ g) => {
    switch (g?.type) {
      case 'Point': point(g.coordinates); break
      case 'MultiPoint': for (const c of g.coordinates ?? []) point(c); break
      case 'LineString': ring(g.coordinates, false, lineRings); break
      case 'MultiLineString': for (const l of g.coordinates ?? []) ring(l, false, lineRings); break
      case 'Polygon': for (const r of g.coordinates ?? []) ring(r, true, polyRings); break
      case 'MultiPolygon': for (const p of g.coordinates ?? []) for (const r of p) ring(r, true, polyRings); break
      case 'GeometryCollection': for (const x of g.geometries ?? []) walk(x); break
      default: break
    }
  }
  walk(geometry)
  return {
    points: Float64Array.from(pts),
    rings: [...polyRings, ...lineRings],
    polygons: polyRings.length,
    box: Number.isFinite(minX) ? [minX, minY, maxX, maxY] : [0, 0, -1, -1],
  }
}

/**
 * The transform from world units to canvas pixels for a view:
 * px = x * s + ox, py = y * s + oy.
 * @param {{ scale: number, x: number, y: number }} view @param {number} width @param {number} height
 */
export function screenTransform(view, width, height) {
  return { s: view.scale, ox: width / 2 - view.x * view.scale, oy: height / 2 - view.y * view.scale }
}

/**
 * Whether a compiled shape's box reaches the screen (with a margin in px).
 * @param {Compiled} c @param {{ s: number, ox: number, oy: number }} t
 * @param {number} width @param {number} height @param {number} [margin]
 */
export function onScreen(c, t, width, height, margin = 8) {
  const [a, b, x2, y2] = c.box
  if (x2 < a) return false
  return a * t.s + t.ox <= width + margin && x2 * t.s + t.ox >= -margin && b * t.s + t.oy <= height + margin && y2 * t.s + t.oy >= -margin
}

/**
 * Add a compiled shape's rings to the current path.
 * @param {CanvasRenderingContext2D | Path2D} ctx @param {Compiled} c
 * @param {{ s: number, ox: number, oy: number }} t
 * @param {'all' | 'areas' | 'lines'} [which]
 */
export function traceRings(ctx, c, t, which = 'all') {
  const from = which === 'lines' ? c.polygons : 0
  const to = which === 'areas' ? c.polygons : c.rings.length
  const { s, ox, oy } = t
  for (let r = from; r < to; r++) {
    const { xy, closed } = c.rings[r]
    ctx.moveTo(xy[0] * s + ox, xy[1] * s + oy)
    for (let i = 2; i < xy.length; i += 2) ctx.lineTo(xy[i] * s + ox, xy[i + 1] * s + oy)
    if (closed) ctx.closePath()
  }
}

/**
 * Squared distance from (px, py) to the segment a-b.
 * @param {number} px @param {number} py @param {number} ax @param {number} ay @param {number} bx @param {number} by
 */
function segDistSq(px, py, ax, ay, bx, by) {
  const dx = bx - ax
  const dy = by - ay
  const len = dx * dx + dy * dy
  let k = len === 0 ? 0 : ((px - ax) * dx + (py - ay) * dy) / len
  k = k < 0 ? 0 : k > 1 ? 1 : k
  const cx = ax + k * dx - px
  const cy = ay + k * dy - py
  return cx * cx + cy * cy
}

/**
 * The topmost shape at a world point: within `tol` world units of a point or
 * an edge, or inside a polygon. Points and edges win over a polygon they sit
 * in, so a dot inside a district is still pickable.
 * @param {Compiled[]} shapes @param {number} wx @param {number} wy @param {number} tol
 * @returns {number} index, or -1
 */
export function hitCompiled(shapes, wx, wy, tol) {
  const tolSq = tol * tol
  let inside = -1
  for (let i = shapes.length - 1; i >= 0; i--) {
    const c = shapes[i]
    if (!c) continue
    const [a, b, x2, y2] = c.box
    if (wx < a - tol || wx > x2 + tol || wy < b - tol || wy > y2 + tol) continue
    const p = c.points
    for (let j = 0; j < p.length; j += 2) {
      const dx = p[j] - wx
      const dy = p[j + 1] - wy
      if (dx * dx + dy * dy <= tolSq) return i
    }
    for (const { xy } of c.rings) {
      for (let j = 0; j + 3 < xy.length; j += 2) {
        if (segDistSq(wx, wy, xy[j], xy[j + 1], xy[j + 2], xy[j + 3]) <= tolSq) return i
      }
    }
    if (inside < 0 && c.polygons > 0) {
      let odd = false
      for (let r = 0; r < c.polygons; r++) {
        const xy = c.rings[r].xy
        const n = xy.length
        for (let j = 0, k = n - 2; j < n; k = j, j += 2) {
          const yj = xy[j + 1]
          const yk = xy[k + 1]
          if ((yj > wy) !== (yk > wy) && wx < ((xy[k] - xy[j]) * (wy - yj)) / (yk - yj) + xy[j]) odd = !odd
        }
      }
      if (odd) inside = i
    }
  }
  return inside
}

/**
 * Whether `inner` lies inside `outer` (lon/lat boxes).
 * @param {{ minLon: number, minLat: number, maxLon: number, maxLat: number }} inner
 * @param {{ minLon: number, minLat: number, maxLon: number, maxLat: number }} outer
 */
export function boundsWithin(inner, outer) {
  return inner.minLon >= outer.minLon && inner.maxLon <= outer.maxLon && inner.minLat >= outer.minLat && inner.maxLat <= outer.maxLat
}
