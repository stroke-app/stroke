<script>
  import { onMount, onDestroy } from 'svelte'
  import Plus from '@lucide/svelte/icons/plus'
  import Minus from '@lucide/svelte/icons/minus'
  import Maximize from '@lucide/svelte/icons/maximize'
  import { createRouteRun, corridorPathOrtho, pathHitsRect, CORNER, MAX_ROUTED_NODES } from '$lib/erd-routing.js'

  /**
   * High-FPS canvas ER renderer. Consumes the same node/edge model the page
   * builds for Dagre and draws it on a single 2D canvas with viewport culling
   * and level-of-detail, so large schemas stay smooth where a DOM graph chokes.
   *
   * @typedef {{ id:string, position:{x:number,y:number}, data:any }} FlowNode
   * @typedef {{ id:string, source:string, target:string, sourceHandle?:string,
   *   targetHandle?:string, many?:boolean, optional?:boolean }} FlowEdge
   */

  let {
    nodes = /** @type {FlowNode[]} */ ([]),
    edges = /** @type {FlowEdge[]} */ ([]),
    cfg = { NODE_W: 230, ROW_H: 22, HDR_H: 34, PAD_B: 4 },
    selectedId = /** @type {string|null} */ (null),
    /** The table the diagram was opened for - drawn with a primary ring + "current" chip. */
    focusId = /** @type {string|null} */ (null),
    /** 'smart' routes edges around cards; 'direct' is the cheap elbow (huge schemas). */
    routing = /** @type {'smart'|'direct'} */ ('smart'),
    /** Edge id → the polyline the layout reserved for it, when it routes its own
     *  edges. Present means curves along those corridors and no A* at all. */
    hints = /** @type {Map<string, {x:number,y:number}[]>} */ (new Map()),
    /** Relationships into hub tables: not laid out, not routed. Named on the
     *  referencing row, counted on the hub's header, drawn as lines only for
     *  the hovered or selected card. */
    hubEdges = /** @type {FlowEdge[]} */ ([]),
    hubs = /** @type {Set<string>} */ (new Set()),
    showTypes = true,
    grid = true,
    onselect = /** @type {(id:string|null)=>void} */ (() => {}),
    onopen = /** @type {(id:string)=>void} */ (() => {}),
    onnodemoved = /** @type {(id:string,x:number,y:number)=>void} */ (() => {}),
  } = $props()

  // Layout constants from the parent's config. $derived so they track `cfg` if
  // the parent swaps it - plain destructuring freezes the initial values, which
  // is what Svelte's `state_referenced_locally` warning flags.
  const NODE_W = $derived(cfg.NODE_W)
  const ROW_H = $derived(cfg.ROW_H)
  const HDR_H = $derived(cfg.HDR_H)
  const PAD_B = $derived(cfg.PAD_B)

  // Match the app's type system (Geist / Geist Mono) - the generic ui-monospace
  // fallback renders poorly on Linux/WebKitGTK. Names use sans, types use mono.
  const FONT_SANS = '"Geist Variable", ui-sans-serif, system-ui, sans-serif'
  const FONT_MONO = '"Geist Mono Variable", ui-monospace, monospace'

  // ── Element / context ───────────────────────────────────────────────────
  let host = $state(/** @type {HTMLDivElement|null} */ (null))
  let canvas = $state(/** @type {HTMLCanvasElement|null} */ (null))
  let mini = $state(/** @type {HTMLCanvasElement|null} */ (null))
  /** @type {CanvasRenderingContext2D|null} */
  let ctx = null
  let dpr = 1
  let cssW = 0
  let cssH = 0

  // ── Camera (world→screen: sx = wx*zoom + panX) ──────────────────────────
  const MIN_ZOOM = 0.04
  const MAX_ZOOM = 2.5
  let cam = { panX: 0, panY: 0, zoom: 1 }

  // Level-of-detail thresholds (world→screen zoom).
  const LOD_ROWS = 0.5   // draw full column rows at/above this
  const LOD_NAME = 0.16  // draw only the table name below LOD_ROWS
  const LOD_BARS = 0.18  // draw rows as bars (no text) below LOD_ROWS

  // ── Interaction state ────────────────────────────────────────────────────
  let hoveredId = $state(/** @type {string|null} */ (null))
  /** @type {{ mode:'none'|'pan'|'drag'|'mini', id?:string, ox:number, oy:number, sx:number, sy:number, moved:boolean }} */
  let drag = { mode: 'none', ox: 0, oy: 0, sx: 0, sy: 0, moved: false }
  /** Local position override for the node being dragged (avoids parent churn per frame). */
  let dragPos = /** @type {{id:string,x:number,y:number}|null} */ (null)

  // ── Theme palette (resolved from CSS tokens to concrete rgb) ─────────────
  /** @type {Record<string,string>} */
  let pal = {}
  // ERD ink, authored in OKLCH so the light/dark pairs are perceptually matched
  // (equal L steps read as equal brightness) and hue stays put. Rasterised to
  // sRGB bytes at runtime, like the theme tokens.
  // `focus` is teal: far enough from the FK blue (252) and the PK amber (82) to be
  // told apart instantly, without borrowing green's "success" or red's "danger"
  // meaning. It is the only accent on a card, so it stays low-key.
  const INK = {
    dark: {
      pk: 'oklch(0.80 0.12 82)', fk: 'oklch(0.70 0.11 252)',
      edge: 'oklch(0.55 0.03 255)', edgeSoft: 'oklch(0.32 0.015 255)',
      focus: 'oklch(0.80 0.11 192)', grid: 0.16, rim: 0.5, hdr: 0.6,
    },
    light: {
      pk: 'oklch(0.60 0.13 72)', fk: 'oklch(0.52 0.14 255)',
      edge: 'oklch(0.62 0.04 255)', edgeSoft: 'oklch(0.86 0.012 255)',
      focus: 'oklch(0.55 0.10 196)', grid: 0.34, rim: 0.95, hdr: 1,
    },
  }
  /** Polarity-dependent weights - a light theme needs firmer rims and darker dots. */
  let tone = INK.dark
  let PK = '251,191,36'
  let FK = '96,165,250'
  /** @type {HTMLSpanElement|null} */
  let probe = null

  /** @type {CanvasRenderingContext2D|null} */
  let probeCtx = null
  /**
   * Resolve a CSS custom property (e.g. '--card') to concrete "r,g,b".
   * Themes store tokens as full oklch() colors; the canvas fillStyle getter
   * preserves the oklch string rather than converting it, so we rasterize a
   * single pixel and read it back - that always yields device sRGB bytes,
   * correct for oklch / hsl / hex alike.
   * @param {string} name
   */
  function resolveRgb(name) {
    const el = probe || host || document.documentElement
    return resolveColor(getComputedStyle(el).getPropertyValue(name).trim())
  }
  /** @param {string} raw Any CSS color (oklch included). */
  function resolveColor(raw) {
    if (!raw) return '128,128,128'
    if (!probeCtx) {
      const cv = document.createElement('canvas')
      cv.width = cv.height = 1
      probeCtx = cv.getContext('2d', { willReadFrequently: true })
    }
    if (!probeCtx) return '128,128,128'
    probeCtx.clearRect(0, 0, 1, 1)
    probeCtx.fillStyle = '#808080' // fallback if `raw` is not a valid color
    probeCtx.fillStyle = raw
    probeCtx.fillRect(0, 0, 1, 1)
    const d = probeCtx.getImageData(0, 0, 1, 1).data
    return `${d[0]},${d[1]},${d[2]}`
  }
  function refreshTheme() {
    pal = {
      bg: resolveRgb('--background'),
      card: resolveRgb('--card'),
      panel: resolveRgb('--panel'),
      border: resolveRgb('--border'),
      fg: resolveRgb('--foreground'),
      mfg: resolveRgb('--muted-foreground'),
      muted: resolveRgb('--muted'),
      primary: resolveRgb('--primary'),
      pfg: resolveRgb('--primary-foreground'),
    }
    // One ink set per theme polarity, judged off the resolved background.
    const [r, g, b] = pal.bg.split(',').map(Number)
    tone = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255 < 0.5 ? INK.dark : INK.light
    PK = resolveColor(tone.pk)
    FK = resolveColor(tone.fk)
    pal.edge = resolveColor(tone.edge)
    pal.edgeSoft = resolveColor(tone.edgeSoft)
    pal.focus = resolveColor(tone.focus)
    refreshRowStyles()
    markDirty()
  }
  /** @param {string} name @param {number} a */
  const c = (name, a = 1) => `rgba(${pal[name] ?? '128,128,128'},${a})`

  // Row styles, assembled once per theme instead of once per column drawn.
  let FONT_ROW = '', FONT_ROW_PK = '', FONT_TYPE = '', FONT_BADGE = ''
  let PK_NAME = '', FK_NAME = '', PK_WASH = '', FK_WASH = '', PK_INK = '', FK_INK = ''
  function refreshRowStyles() {
    FONT_ROW = `12px ${FONT_SANS}`
    FONT_ROW_PK = `600 12px ${FONT_SANS}`
    FONT_TYPE = `11px ${FONT_MONO}`
    FONT_BADGE = `700 9px ${FONT_MONO}`
    PK_NAME = `rgba(${PK},0.95)`
    FK_NAME = `rgba(${FK},0.95)`
    PK_WASH = `rgba(${PK},0.16)`
    FK_WASH = `rgba(${FK},0.14)`
    PK_INK = `rgba(${PK},1)`
    FK_INK = `rgba(${FK},1)`
  }

  // ── Geometry helpers ───────────────────────────────────────────────────
  // A card with hidden columns gets one extra "+N more" row, so its height must
  // match what the page's layout pass reserved for it.
  /** @param {any} data */
  const nodeH = (data) =>
    HDR_H + (data?.columns?.length ?? 0) * ROW_H + (data?.hiddenCount ? ROW_H : 0) + PAD_B
  /** @param {FlowNode} n */
  const posOfRaw = (n) => (dragPos && dragPos.id === n.id ? dragPos : n.position)
  /** Where a card is drawn this frame: part way along its glide after a layout. */
  function posOf(/** @type {FlowNode} */ n) {
    const g = glide?.get(n.id)
    if (!g) return posOfRaw(n)
    const k = Math.min(1, (performance.now() - glideT0) / GLIDE_MS)
    const e = 1 - Math.pow(1 - k, 3)
    return { x: g.fx + (g.tx - g.fx) * e, y: g.fy + (g.ty - g.fy) * e }
  }

  /** @type {Map<string,FlowNode>} */
  let byId = new Map()
  /**
   * node id → handle name → row index. Resolving a handle by scanning the column
   * list built a template string per column per edge per frame - tens of
   * thousands of throwaway strings a frame on a schema-sized diagram, which is
   * most of what made panning feel heavy.
   * @type {Map<string, Map<string, number>>}
   */
  let handleIdx = new Map()
  /** Bumped whenever the node set changes - cache keys hang off it. */
  let graphVersion = 0
  // Auto-fit the first time nodes populate (they load async, after mount), then
  // never again until the graph empties - so search/rebuild doesn't reset the view.
  let _fitted = false
  // Zoom below which cards stop being legible - a big/deep schema whose whole
  // bounding box would fit only under this is NOT fit-to-view (that produces an
  // unreadable sliver). Instead we land at READABLE_ZOOM anchored at the graph's
  // top-left so real cards are visible and the user pans to explore.
  const READABLE_ZOOM = 0.62
  /**
   * Pick the most useful starting view: fit the whole graph when that stays
   * legible, otherwise land at a readable zoom - centred on the focused table if
   * there is one (a wide fan would otherwise open with it off-screen), else at
   * the graph's top-left so the first ranks are visible.
   */
  let _revealed = false
  export function reveal() {
    if (!cssW || !nodes.length) return
    // The first picture snaps into place; every later one (a re-layout, a new
    // scope) glides there from where the eye already is.
    const ms = _revealed ? 360 : 0
    _revealed = true
    // Graph bounding box (world coords).
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity
    for (const n of nodes) {
      const p = n.position, h = nodeH(n.data)
      x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y)
      x1 = Math.max(x1, p.x + NODE_W); y1 = Math.max(y1, p.y + h)
    }
    const bw = x1 - x0 || 1, bh = y1 - y0 || 1
    const zFit = Math.min(cssW / bw, cssH / bh) * (1 - 0.12)
    if (zFit >= READABLE_ZOOM) {
      fit(0.12, ms)
      return
    }
    const fn = focusId ? byId.get(focusId) : null
    if (fn) {
      const p = fn.position, h = nodeH(fn.data)
      flyTo({ zoom: READABLE_ZOOM, panX: cssW / 2 - (p.x + NODE_W / 2) * READABLE_ZOOM, panY: cssH / 2 - (p.y + h / 2) * READABLE_ZOOM }, ms)
      return
    }
    const M = 56 // screen-px margin from the viewport's top-left
    // Centre vertically when the graph is shorter than the viewport, else top-anchor.
    const panY = bh * READABLE_ZOOM < cssH - M
      ? (cssH - bh * READABLE_ZOOM) / 2 - y0 * READABLE_ZOOM
      : M - y0 * READABLE_ZOOM
    flyTo({ zoom: READABLE_ZOOM, panX: M - x0 * READABLE_ZOOM, panY }, ms)
  }
  function ensureFit() {
    if (_fitted || !cssW || !nodes.length) return
    _fitted = true
    reveal()
  }
  $effect(() => {
    startGlide(nodes)
    byId = new Map(nodes.map((n) => [n.id, n]))
    handleIdx = new Map()
    for (const n of nodes) {
      /** @type {Map<string, number>} */
      const m = new Map()
      const cols = n.data?.columns ?? []
      for (let i = 0; i < cols.length; i++) {
        m.set(`src-${cols[i].name}`, i)
        m.set(`tgt-${cols[i].name}`, i)
      }
      handleIdx.set(n.id, m)
    }
    graphVersion += 1
    if (!nodes.length) _fitted = false
    ensureFit()
    markDirty()
  })
  $effect(() => { void edges; void selectedId; void focusId; void routing; void hints; void showTypes; void grid; void hubs; markDirty() })
  /** Lines into each hub, for the count on its header. */
  let hubIn = new Map()
  $effect(() => {
    const m = new Map()
    for (const e of hubEdges) m.set(e.target, (m.get(e.target) ?? 0) + (e.mergedCount ?? 1))
    hubIn = m
    markDirty()
  })

  // ── Motion ───────────────────────────────────────────────────────────────
  /** The app's motion setting, with the OS preference behind "System". */
  function motionOff() {
    const m = document.documentElement.dataset.motion
    return m === 'reduced' || (m !== 'full' && matchMedia('(prefers-reduced-motion: reduce)').matches)
  }

  let camAnim = 0
  function cancelCamAnim() { if (camAnim) { cancelAnimationFrame(camAnim); camAnim = 0 } }
  /**
   * Ease the camera to a target: the world point under the viewport's centre
   * glides and the zoom eases in log space, so a 10x change feels even all the
   * way rather than fast at the start and dead at the end.
   * @param {{ panX: number, panY: number, zoom: number }} to @param {number} [ms]
   */
  function flyTo(to, ms = 360) {
    cancelCamAnim()
    if (ms <= 0 || motionOff()) { cam.panX = to.panX; cam.panY = to.panY; cam.zoom = to.zoom; markDirty(); return }
    const from = { ...cam }
    const centre = (/** @type {{ panX: number, panY: number, zoom: number }} */ c) =>
      ({ x: (cssW / 2 - c.panX) / c.zoom, y: (cssH / 2 - c.panY) / c.zoom })
    const a = centre(from), b = centre(to)
    const t0 = performance.now()
    const step = (/** @type {number} */ now) => {
      const k = Math.min(1, (now - t0) / ms)
      const e = 1 - Math.pow(1 - k, 3)
      const z = Math.exp(Math.log(from.zoom) + (Math.log(to.zoom) - Math.log(from.zoom)) * e)
      const cx = a.x + (b.x - a.x) * e, cy = a.y + (b.y - a.y) * e
      cam.zoom = z
      cam.panX = cssW / 2 - cx * z
      cam.panY = cssH / 2 - cy * z
      markDirty()
      camAnim = k < 1 ? requestAnimationFrame(step) : 0
    }
    camAnim = requestAnimationFrame(step)
  }

  /** Where every card was the last time the graph changed, to glide from. */
  let lastPos = /** @type {Map<string, {x:number,y:number}> | null} */ (null)
  /** Cards gliding from their previous place to a new layout. */
  let glide = /** @type {Map<string, {fx:number,fy:number,tx:number,ty:number}> | null} */ (null)
  let glideT0 = 0
  let glideAnim = 0
  const GLIDE_MS = 420
  /**
   * A new layout glides in rather than snapping: the eye keeps track of which
   * card went where. One card moving is a drop, not a layout, and lands where
   * it was let go.
   * @param {FlowNode[]} ns
   */
  function startGlide(ns) {
    const prev = lastPos
    lastPos = new Map(ns.map((n) => [n.id, { x: n.position.x, y: n.position.y }]))
    if (!prev || motionOff()) return
    const moves = new Map()
    for (const n of ns) {
      const q = prev.get(n.id)
      if (!q || dragPos?.id === n.id) continue
      if (Math.abs(q.x - n.position.x) < 1 && Math.abs(q.y - n.position.y) < 1) continue
      moves.set(n.id, { fx: q.x, fy: q.y, tx: n.position.x, ty: n.position.y })
    }
    if (moves.size < 2) { glide = null; return }
    glide = moves
    glideT0 = performance.now()
    if (glideAnim) cancelAnimationFrame(glideAnim)
    const step = (/** @type {number} */ now) => {
      markDirty()
      if (now - glideT0 < GLIDE_MS) { glideAnim = requestAnimationFrame(step); return }
      glideAnim = 0
      glide = null
      markDirty()
    }
    glideAnim = requestAnimationFrame(step)
  }

  // ── Render scheduling ───────────────────────────────────────────────────
  let rafId = 0
  function markDirty() {
    if (rafId || !ctx) return
    rafId = requestAnimationFrame(() => { rafId = 0; draw() })
  }

  function draw() {
    if (!ctx) return
    const { panX, panY, zoom } = cam

    // Clear to transparent - the themed page background shows through, so we
    // never depend on resolving --background (which can misparse per webview).
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.clearRect(0, 0, cssW, cssH)
    forgetStyle()

    // World transform - everything below is drawn in world coordinates.
    ctx.setTransform(dpr * zoom, 0, 0, dpr * zoom, dpr * panX, dpr * panY)
    const vx0 = -panX / zoom, vy0 = -panY / zoom
    const vx1 = (cssW - panX) / zoom, vy1 = (cssH - panY) / zoom

    if (grid && zoom > 0.45) drawGrid(vx0, vy0, vx1, vy1)

    ctx.lineCap = 'round'
    ctx.lineJoin = 'round'
    ensureRoutes(zoom >= LOD_ROWS)
    drawEdges(vx0, vy0, vx1, vy1)
    /** @type {FlowNode|null} */
    let focusNode = null
    for (const n of nodes) {
      const p = posOf(n), h = nodeH(n.data)
      if (p.x > vx1 || p.x + NODE_W < vx0 || p.y > vy1 || p.y + h < vy0) continue
      // The focused card draws last so its ring is never clipped by a neighbour.
      if (focusId && n.id === focusId) { focusNode = n; continue }
      drawNode(n, p, h, zoom)
    }
    if (focusNode) drawNode(focusNode, posOf(focusNode), nodeH(focusNode.data), zoom)
    drawMinimap()
  }

  /** @param {number} x0 @param {number} y0 @param {number} x1 @param {number} y1 */
  function drawGrid(x0, y0, x1, y1) {
    const G = 22
    setFill(c('mfg', tone.grid))
    const sx = Math.floor(x0 / G) * G, sy = Math.floor(y0 / G) * G
    for (let x = sx; x < x1; x += G)
      for (let y = sy; y < y1; y += G) ctx.fillRect(x, y, 1, 1)
  }

  /** @param {number} x @param {number} y @param {number} w @param {number} h @param {number} r */
  function roundRect(x, y, w, h, r) {
    r = Math.min(r, w / 2, h / 2)
    ctx.beginPath()
    ctx.moveTo(x + r, y)
    ctx.arcTo(x + w, y, x + w, y + h, r)
    ctx.arcTo(x + w, y + h, x, y + h, r)
    ctx.arcTo(x, y + h, x, y, r)
    ctx.arcTo(x, y, x + w, y, r)
    ctx.closePath()
  }

  // ── Edge routing ────────────────────────────────────────────────────────
  // Geometry lives in $lib/erd-routing.js (shared with the SVG/PNG export).
  // Routes are built once per layout - never per frame - and cached; anything
  // the router can't place falls back to a plain elbow below.
  /** @type {Map<string, {x:number,y:number}[]>} */
  let routes = new Map()
  /**
   * Routing state per layout signature. Crossing the row zoom threshold changes
   * the ports and so the signature, and a zoom gesture can cross it twice in a
   * second - keeping the last few runs means going back to a layout already
   * routed is free instead of another second of A*.
   * @type {Map<string, {routes: Map<string, {x:number,y:number}[]>,
   *   run: ReturnType<typeof createRouteRun>|null, spent: number}>}
   */
  const routeCache = new Map()
  const ROUTE_CACHE_MAX = 3
  /** Milliseconds of A* allowed per frame, and in total, per layout. */
  const ROUTE_BUDGET_MS = 8
  const ROUTE_MAX_MS = 2500
  /** 0-100 while a run is in flight, -1 when there is nothing left to route. */
  let routePct = $state(-1)
  /** Cards and routes the last run was built from, for the incremental path. */
  /** @type {Map<string, {id:string,x:number,y:number,w:number,h:number}>|null} */
  let lastBoxes = null
  /** @type {Map<string, {x:number,y:number}[]>|null} */
  let lastRoutes = null

  /**
   * Which cards moved since the last run, or null when too much changed for
   * carrying anything over to be worth it (a new schema, a re-layout, a card
   * resized by a column-mode switch).
   * @param {{id:string,x:number,y:number,w:number,h:number}[]} boxes
   * @returns {{id:string,x:number,y:number,w:number,h:number}[]|null}
   */
  function movedSince(boxes) {
    if (!lastBoxes || !lastRoutes || lastBoxes.size !== boxes.length) return null
    const moved = []
    for (const b of boxes) {
      const was = lastBoxes.get(b.id)
      if (!was || was.w !== b.w || was.h !== b.h) return null
      if (was.x !== b.x || was.y !== b.y) moved.push(was, b)
    }
    // Nothing moved but the signature changed: something else did, so re-route.
    if (!moved.length) return null
    // Past a handful this is a re-layout, not a drag, and every line is suspect.
    if (moved.length > 8) return null
    return moved
  }

  /**
   * Routes worth keeping: both endpoints stood still, and no card that moved
   * (at either its old or its new position, with the clearance a line keeps)
   * now lies across the path.
   * @param {import('$lib/erd-routing.js').Link[]} links
   * @param {{id:string,x:number,y:number,w:number,h:number}[]} stale
   */
  function reusableRoutes(links, stale) {
    /** @type {Map<string, {x:number,y:number}[]>} */
    const routes = new Map()
    if (!lastRoutes) return { routes }
    const shifted = new Set(stale.map(b => b.id))
    for (const l of links) {
      if (shifted.has(l.source) || shifted.has(l.target)) continue
      const pts = lastRoutes.get(l.id)
      if (!pts) continue
      if (!stale.some(b => pathHitsRect(pts, b))) routes.set(l.id, pts)
    }
    return { routes }
  }

  /** Positions come from `n.position` (not posOf) so a live drag doesn't thrash
   *  the cache - the dragged card's own lines are rebuilt per frame instead. */
  function routeSignature(/** @type {boolean} */ rowsShown) {
    let h = (nodes.length * 397 + edges.length * 31 + (rowsShown ? 7 : 0)) | 0
    for (const n of nodes) {
      h = (h * 33 + Math.round(n.position.x) * 7 + Math.round(n.position.y) * 13) | 0
      h = (h * 33 + (n.data?.columns?.length ?? 0)) | 0
    }
    return `${routing}:${hints.size}:${h}`
  }

  /**
   * Ports: which edge of each card the line leaves and enters, and where on it.
   *
   * A side port is the one worth having, because it sits on the row the
   * relationship is actually about - the foreign key column on one card, the
   * column it references on the other. So a side port is what you get whenever
   * the two cards are separated horizontally at all, however little.
   *
   * When they overlap in x there is no side to leave from: picking one anyway
   * sends the line out of the card, back across its own width and in again, and
   * a quarter of the relationships in a folded schema are in exactly that
   * position. Those get the near edge instead - bottom to top, or top to bottom -
   * which is the short hop the two cards were always asking for.
   */
  function portsOf(/** @type {FlowEdge} */ e, /** @type {FlowNode} */ s, /** @type {FlowNode} */ t, /** @type {boolean} */ rowsShown) {
    const sp = posOf(s), tp = posOf(t)
    const sh = nodeH(s.data), th = nodeH(t.data)
    const ci = e.sourceHandle ? (handleIdx.get(s.id)?.get(e.sourceHandle) ?? -1) : -1
    const ti = e.targetHandle ? (handleIdx.get(t.id)?.get(e.targetHandle) ?? -1) : -1
    // Past the row LOD the rows aren't drawn, so anchor to the card instead.
    const rowY = (/** @type {number} */ y, /** @type {number} */ i, /** @type {number} */ h) =>
      rowsShown && i >= 0 ? y + HDR_H + i * ROW_H + ROW_H / 2 : y + (rowsShown ? HDR_H / 2 : h / 2)

    const toRight = tp.x - (sp.x + NODE_W)
    const toLeft = sp.x - (tp.x + NODE_W)
    if (toRight >= 0 || toLeft >= 0) {
      const leftToRight = toRight >= toLeft
      return {
        ci,
        sx: leftToRight ? sp.x + NODE_W : sp.x,
        sy: rowY(sp.y, ci, sh),
        tx: leftToRight ? tp.x : tp.x + NODE_W,
        ty: rowY(tp.y, ti, th),
        sdx: leftToRight ? 1 : -1, sdy: 0,
        tdx: leftToRight ? -1 : 1, tdy: 0,
      }
    }
    // Stacked. There is no row to sit on across a horizontal edge, so both ends
    // take the middle of theirs, which is also where they meet if several lines
    // arrive from the same direction.
    const down = tp.y >= sp.y
    return {
      ci,
      sx: sp.x + NODE_W / 2,
      sy: down ? sp.y + sh : sp.y,
      tx: tp.x + NODE_W / 2,
      ty: down ? tp.y : tp.y + th,
      sdx: 0, sdy: down ? 1 : -1,
      tdx: 0, tdy: down ? -1 : 1,
    }
  }

  /**
   * Keep the routes for the current layout up to date, spending at most a few
   * milliseconds of A* per frame.
   *
   * Routing a whole schema is seconds of work - 400 tables measured at ~5s - and
   * doing it in one call inside the frame is what froze the window and got the
   * app killed. The run is stepped instead: every link still without a route is
   * drawn as a plain elbow, and the diagram sharpens over the next few frames.
   * A run that overruns its total budget stops, leaving the rest as elbows,
   * rather than burning frames forever on a graph too dense to route.
   * @param {boolean} rowsShown
   */
  function ensureRoutes(rowsShown) {
    const key = routeSignature(rowsShown)
    let entry = routeCache.get(key)
    if (!entry) {
      entry = { routes: new Map(), run: null, spent: 0 }
      // Ports for every line, once per layout: working them out per frame was
      // tens of thousands of throwaway objects a frame, which is what made
      // panning feel heavy.
      /** @type {import('$lib/erd-routing.js').Link[]} */
      const links = []
      for (const e of edges) {
        const s = byId.get(e.source), t = byId.get(e.target)
        if (!s || !t) continue
        const { sx, sy, tx, ty, sdx, sdy, tdx, tdy } = portsOf(e, s, t, rowsShown)
        links.push({
          id: e.id, source: e.source, target: e.target,
          sx, sy, tx, ty, sdx, sdy, tdx, tdy,
        })
      }

      if (hints.size) {
        // The layout routed these already. 'Direct' keeps the ports and the
        // stubs but ignores the detours, which is the honest reading of the
        // setting: fewer bends, may cross a card.
        //
        // Drawn as right angles, not as a straight line between the layout's
        // points: those points sit at different heights, so joining them
        // directly turned every relationship into a long diagonal across the
        // canvas. Turning only inside the gutters keeps each run in space the
        // layout kept clear, which is also what stops a line disappearing under
        // a card.
        const follow = routing !== 'direct'
        for (const l of links) {
          entry.routes.set(l.id, corridorPathOrtho(l, follow ? hints.get(l.id) : undefined))
        }
      } else if (links.length && routing !== 'direct' && nodes.length <= MAX_ROUTED_NODES) {
        const boxes = nodes.map(n => ({
          id: n.id, x: n.position.x, y: n.position.y, w: NODE_W, h: nodeH(n.data),
        }))
        // Dropping one card should not rebuild the whole diagram in front of
        // you. Only the cards that actually moved invalidate anything: a line
        // between two cards that stayed put is still the line it was, unless one
        // of the movers is now sitting on it. Everything else is carried over,
        // so a drag re-routes a handful of lines in one frame instead of playing
        // the whole routing run out again.
        const stale = movedSince(boxes)
        const keep = stale === null ? null : reusableRoutes(links, stale)
        if (keep) {
          entry.routes = keep.routes
          const todo = links.filter(l => !keep.routes.has(l.id))
          if (todo.length) entry.run = createRouteRun(boxes, todo)
        } else {
          entry.run = createRouteRun(boxes, links)
        }
        lastBoxes = new Map(boxes.map(b => [b.id, b]))
        lastRoutes = entry.routes
      } else {
        // 'Direct', or a graph past the routing cap. No corridor to follow, so
        // each line takes its own turn column - with the ports fanned, that is
        // still readable where a hub's lines all landing on one pixel is not.
        for (const l of links) entry.routes.set(l.id, corridorPathOrtho(l))
      }
      routeCache.set(key, entry)
      // Oldest first: a Map iterates in insertion order.
      while (routeCache.size > ROUTE_CACHE_MAX) {
        const oldest = routeCache.keys().next().value
        if (oldest === undefined) break
        routeCache.delete(oldest)
      }
    }
    routes = entry.routes

    const run = entry.run
    if (!run) { routePct = -1; return }
    const t0 = performance.now()
    run.step(ROUTE_BUDGET_MS)
    entry.spent += performance.now() - t0
    // The run owns its own map; ours may already hold lines carried over from
    // the last layout, so results are folded in rather than swapped for.
    if (entry.routes !== run.routes) {
      for (const [id, pts] of run.routes) entry.routes.set(id, pts)
    }
    if (run.done || entry.spent > ROUTE_MAX_MS) {
      // Whatever is still unrouted keeps its elbow - a diagram this dense has no
      // clean corridors left anyway, and frames matter more than the last line.
      entry.run = null
      routePct = -1
      return
    }
    routePct = Math.round((run.placed / run.total) * 100)
    // Another slice next frame; the draw already shows what has landed.
    markDirty()
  }

  /** Add a rounded orthogonal polyline to the current path.
   *  @param {{x:number,y:number}[]} pts */
  function tracePolyline(pts) {
    ctx.moveTo(pts[0].x, pts[0].y)
    for (let i = 1; i < pts.length - 1; i++) {
      const a = pts[i - 1], b = pts[i], d = pts[i + 1]
      const inLen = Math.abs(b.x - a.x) + Math.abs(b.y - a.y)
      const outLen = Math.abs(d.x - b.x) + Math.abs(d.y - b.y)
      ctx.arcTo(b.x, b.y, d.x, d.y, Math.min(CORNER, inLen / 2, outLen / 2))
    }
    const last = pts[pts.length - 1]
    ctx.lineTo(last.x, last.y)
  }

  // ── Cardinality markers ─────────────────────────────────────────────────
  // Crow's foot notation, read straight off the schema:
  //   child end  - fork ("many"), or a bar when the FK column is itself unique/PK
  //                (that makes it one-to-one)
  //   parent end - bar ("one"), preceded by a hollow ring when the FK is nullable
  //                (participation is optional: zero-or-one)
  const MARK_LOD = 0.3

  /**
   * Bounding box of a routed polyline, memoised on the array itself.
   *
   * The viewport cull needs one per edge per frame, and computing it walked
   * every point of every line - tens of thousands of reads a frame on a schema
   * this size, for a number that only changes when the line is re-routed. A
   * re-route hands back a new array, which misses the map and recomputes; a
   * stable one never does.
   * @type {WeakMap<{x:number,y:number}[], {x0:number,y0:number,x1:number,y1:number}>}
   */
  const _bbox = new WeakMap()
  /** @param {{x:number,y:number}[]} pts */
  function bboxOf(pts) {
    let b = _bbox.get(pts)
    if (b) return b
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity
    for (const p of pts) {
      if (p.x < x0) x0 = p.x
      if (p.x > x1) x1 = p.x
      if (p.y < y0) y0 = p.y
      if (p.y > y1) y1 = p.y
    }
    b = { x0, y0, x1, y1 }
    _bbox.set(pts, b)
    return b
  }
  /**
   * @param {number} x @param {number} y
   * @param {number} dx @param {number} dy unit vector pointing away from the card
   * @param {'fork'|'bar'} kind @param {boolean} ring
   */
  function traceMarker(x, y, dx, dy, kind, ring) {
    const s = 1 / cam.zoom            // markers keep a constant screen size
    const L = 11 * s, W = 5.5 * s     // length along the line, half-spread across
    const px = -dy, py = dx           // perpendicular
    if (kind === 'fork') {
      const bx = x + dx * L, by = y + dy * L
      ctx.moveTo(bx, by); ctx.lineTo(x + px * W, y + py * W)
      ctx.moveTo(bx, by); ctx.lineTo(x - px * W, y - py * W)
      ctx.moveTo(bx, by); ctx.lineTo(x, y)
    } else {
      const bx = x + dx * (L * 0.55), by = y + dy * (L * 0.55)
      ctx.moveTo(bx + px * W, by + py * W)
      ctx.lineTo(bx - px * W, by - py * W)
    }
    if (ring) {
      const cx = x + dx * (L * 1.5), cy = y + dy * (L * 1.5)
      ctx.moveTo(cx + 3 * s, cy)
      ctx.arc(cx, cy, 3 * s, 0, Math.PI * 2)
    }
  }

  /**
   * Draw all edges in three batched passes - dimmed, plain, highlighted - with a
   * single stroke each. One stroke of a compound path composites once, so two
   * lines that cross or briefly share a lane never paint brighter than one line;
   * the diagram stays even instead of glowing where it is busiest.
   */
  function drawEdges(vx0, vy0, vx1, vy1) {
    const rowsShown = cam.zoom >= LOD_ROWS
    const marks = cam.zoom >= MARK_LOD
    /** @type {{pts:{x:number,y:number}[], e:FlowEdge}[][]} */
    const bands = [[], [], []]

    for (const e of edges) {
      const s = byId.get(e.source), t = byId.get(e.target)
      if (!s || !t) continue
      const dragging = !!dragPos && (dragPos.id === e.source || dragPos.id === e.target)
      // Mid-glide the cards are between layouts and the routes belong to the
      // new one, so every line is a live elbow until the cards land.
      const cached = dragging || glide ? null : routes.get(e.id)

      // The ports only exist to build a path. When the route is already cached
      // - which is every line on every frame that isn't a drag - working them
      // out again allocated an object per edge per frame for nothing.
      let pts = cached
      if (!pts) {
        const { sx, sy, tx, ty, sdx, sdy, tdx, tdy } = portsOf(e, s, t, rowsShown)
        const port = { sx, sy, tx, ty, sdx, sdy, tdx, tdy }
        // A dragged card's lines are rebuilt every frame, and they have to come
        // out the same shape as the cached ones - otherwise the line snaps to a
        // different geometry the moment the card is picked up. There is no
        // corridor for a card that has moved off its own, so it turns on its own
        // lane like any unrouted line.
        pts = corridorPathOrtho(port)
      }
      // Cull: skip when the polyline's bounding box misses the viewport.
      const bb = bboxOf(pts)
      if (bb.x1 < vx0 || bb.x0 > vx1 || bb.y1 < vy0 || bb.y0 > vy1) continue

      const active = e.source === selectedId || e.target === selectedId
      const dim = s.data?.highlighted === false && t.data?.highlighted === false
      bands[active ? 2 : dim ? 0 : 1].push({ pts, e })
    }

    const styles = [
      { color: c('edgeSoft', 1), width: 1 },
      { color: c('edge', 1), width: 1.4 },
      { color: c('fg', 0.85), width: 2 },
    ]
    // Zoomed out, every line is still a full-strength screen-width stroke, and a
    // few hundred of them bury the cards they are there to connect. Below the row
    // LOD everything but the selected relationship recedes, so the cards stay the
    // subject and the lines read as texture.
    const wash = cam.zoom < LOD_ROWS
    for (let b = 0; b < bands.length; b++) {
      if (!bands[b].length) continue
      // Well under full strength: at fit zoom a hundred lines at 0.75 were the
      // whole picture and the cards a texture behind them. The selected card's
      // lines stay full, which is how one relationship is read at that zoom.
      ctx.globalAlpha = wash && b < 2 ? 0.3 : 1
      ctx.lineWidth = (wash && b < 2 ? styles[b].width * 0.75 : styles[b].width) / cam.zoom
      ctx.strokeStyle = styles[b].color
      ctx.lineJoin = 'round'
      ctx.lineCap = 'round'
      ctx.beginPath()
      for (const { pts } of bands[b]) tracePolyline(pts)
      ctx.stroke()

      if (!marks) continue
      ctx.beginPath()
      for (const { pts, e } of bands[b]) {
        const a0 = pts[0], a1 = pts[1] ?? pts[0]
        const z0 = pts[pts.length - 1], z1 = pts[pts.length - 2] ?? z0
        const sd = unit(a1.x - a0.x, a1.y - a0.y)
        const td = unit(z1.x - z0.x, z1.y - z0.y)
        traceMarker(a0.x, a0.y, sd.x, sd.y, e.many === false ? 'bar' : 'fork', false)
        traceMarker(z0.x, z0.y, td.x, td.y, 'bar', e.optional === true)
      }
      ctx.stroke()
    }

    // A merged line says how many foreign keys it stands for, so collapsing a
    // pair's columns into one line never quietly hides a relationship. Only
    // where the rows are readable: at fit-zoom it would be noise on noise.
    if (rowsShown) {
      for (const band of bands) {
        for (const { pts, e } of band) {
          const n = e.mergedCount ?? 1
          if (n < 2) continue
          const a0 = pts[0], a1 = pts[1] ?? a0
          const d = unit(a1.x - a0.x, a1.y - a0.y)
          drawEdgeCount(a0.x + d.x * 30, a0.y + d.y * 30 - (d.y === 0 ? 10 : 0), n)
        }
      }
    }

    // Hub relationships, for the hovered and the selected card only: live
    // elbows from the ports (they were never laid out or routed), over
    // everything else. Hover `tenants` and its whole fan appears; move off and
    // the page is clean again.
    const active = [selectedId, hoveredId].filter((id) => id && byId.has(id))
    if (active.length && hubEdges.length) {
      /** @type {{pts:{x:number,y:number}[], e:FlowEdge}[]} */
      const live = []
      for (const e of hubEdges) {
        if (!active.includes(e.source) && !active.includes(e.target)) continue
        const s = byId.get(e.source), t = byId.get(e.target)
        if (!s || !t) continue
        const { sx, sy, tx, ty, sdx, sdy, tdx, tdy } = portsOf(e, s, t, rowsShown)
        const pts = corridorPathOrtho({ sx, sy, tx, ty, sdx, sdy, tdx, tdy })
        const bb = bboxOf(pts)
        if (bb.x1 < vx0 || bb.x0 > vx1 || bb.y1 < vy0 || bb.y0 > vy1) continue
        live.push({ pts, e })
      }
      if (live.length) {
        ctx.globalAlpha = 1
        ctx.lineWidth = 1.8 / cam.zoom
        ctx.strokeStyle = c('fg', 0.8)
        ctx.lineJoin = 'round'
        ctx.lineCap = 'round'
        ctx.beginPath()
        for (const { pts } of live) tracePolyline(pts)
        ctx.stroke()
        if (marks) {
          ctx.beginPath()
          for (const { pts, e } of live) {
            const a0 = pts[0], a1 = pts[1] ?? pts[0]
            const z0 = pts[pts.length - 1], z1 = pts[pts.length - 2] ?? z0
            const sd = unit(a1.x - a0.x, a1.y - a0.y)
            const td = unit(z1.x - z0.x, z1.y - z0.y)
            traceMarker(a0.x, a0.y, sd.x, sd.y, e.many === false ? 'bar' : 'fork', false)
            traceMarker(z0.x, z0.y, td.x, td.y, 'bar', e.optional === true)
          }
          ctx.stroke()
        }
      }
    }
    ctx.globalAlpha = 1
  }

  /** The hub a foreign key points at, or null. `foreignKey` is "table.col" or "schema.table.col". */
  function hubRefOf(/** @type {string | undefined} */ fk) {
    if (!fk || !hubs.size) return null
    const parts = fk.split('.')
    const t = parts.length >= 3 ? parts[1] : parts[0]
    return hubs.has(t) ? t : null
  }

  /** Count chip on a merged relationship. @param {number} x @param {number} y @param {number} n */
  function drawEdgeCount(x, y, n) {
    const label = `×${n}`
    setFont(FONT_BADGE)
    const w = ctx.measureText(label).width + 9
    roundRect(x - w / 2, y - 7, w, 14, 4)
    setFill(c('panel', 0.94))
    ctx.fill()
    ctx.lineWidth = 1 / cam.zoom
    ctx.strokeStyle = c('border', 1)
    ctx.stroke()
    setFill(c('mfg', 0.95))
    ctx.textAlign = 'center'
    ctx.textBaseline = 'middle'
    ctx.fillText(label, x, y + 0.5)
  }

  /** @param {number} x @param {number} y */
  function unit(x, y) {
    const m = Math.hypot(x, y)
    return m < 0.001 ? { x: 1, y: 0 } : { x: x / m, y: y / m }
  }

  // Canvas state assignments are not free - WebKit re-parses a font string on
  // every set, and a card with 25 columns used to set the font three times a row.
  // Tracking the live values collapses that to one set per distinct value per
  // frame, across every card on screen.
  let _font = ''
  let _fill = ''
  /** @param {string} f */
  function setFont(f) { if (f !== _font) { ctx.font = f; _font = f } }
  /** @param {string} v */
  function setFill(v) { if (v !== _fill) { ctx.fillStyle = v; _fill = v } }
  /** ctx.save()/restore() rolls these back behind our back. */
  function forgetStyle() { _font = ''; _fill = '' }

  /** @param {FlowNode} n @param {{x:number,y:number}} p @param {number} h @param {number} zoom */
  function drawNode(n, p, h, zoom) {
    const d = n.data ?? {}
    const isFocus = !!focusId && n.id === focusId
    const dim = d.highlighted === false && !isFocus
    const sel = n.id === selectedId
    const hov = n.id === hoveredId
    ctx.globalAlpha = dim ? 0.18 : 1

    // The table this diagram was opened for: one hairline halo for separation, and
    // the rim below. A filled glow plus a tinted header plus a solid chip all at
    // once was what made this shout.
    if (isFocus) {
      roundRect(p.x - 5, p.y - 5, NODE_W + 10, h + 10, 12)
      ctx.lineWidth = 1 / zoom
      ctx.strokeStyle = c('focus', 0.22)
      ctx.stroke()
    }

    // Card + border.
    roundRect(p.x, p.y, NODE_W, h, 8)
    setFill(c('card', 1))
    ctx.fill()
    ctx.lineWidth = (isFocus ? 1.75 : sel ? 2 : 1) / zoom
    ctx.strokeStyle = isFocus
      ? c('focus', 0.9)
      : sel ? c('fg', 0.75)
      : hov ? c('border', 1) : c('border', tone.rim)
    ctx.stroke()

    // Header band.
    ctx.save()
    roundRect(p.x, p.y, NODE_W, Math.min(HDR_H + 8, h), 8)
    ctx.clip()
    ctx.fillStyle = c('muted', tone.hdr)
    ctx.fillRect(p.x, p.y, NODE_W, HDR_H)
    if (isFocus) {
      ctx.fillStyle = c('focus', 0.07)
      ctx.fillRect(p.x, p.y, NODE_W, HDR_H)
    }
    ctx.restore()
    forgetStyle()
    ctx.beginPath()
    ctx.moveTo(p.x, p.y + HDR_H); ctx.lineTo(p.x + NODE_W, p.y + HDR_H)
    ctx.lineWidth = 1 / zoom; ctx.strokeStyle = isFocus ? c('focus', 0.35) : c('border', tone.rim * 0.8); ctx.stroke()

    ctx.textBaseline = 'middle'
    if (zoom >= LOD_NAME) {
      // "current" chip in the header, so the focused table is readable even when
      // the ring is off-screen or the diagram is exported/zoomed out.
      let nameW = NODE_W - 32
      if (isFocus) {
        const label = 'current'
        setFont(FONT_BADGE)
        const bw = ctx.measureText(label).width + 12
        const bx = p.x + NODE_W - bw - 12
        roundRect(bx, p.y + HDR_H / 2 - 7, bw, 14, 4)
        ctx.lineWidth = 1 / zoom
        ctx.strokeStyle = c('focus', 0.55)
        ctx.stroke()
        setFill(c('focus', 0.95))
        ctx.textAlign = 'center'
        ctx.fillText(label, bx + bw / 2, p.y + HDR_H / 2 + 0.5)
        nameW = bx - p.x - 24
      }
      // How many tables point here. The lines themselves show on hover.
      const inCount = hubIn.get(n.id) ?? 0
      if (inCount) {
        const label = `← ${inCount}`
        setFont(FONT_BADGE)
        const bw = ctx.measureText(label).width + 12
        const bx = p.x + 16 + nameW - bw
        roundRect(bx, p.y + HDR_H / 2 - 7, bw, 14, 4)
        setFill(FK_WASH)
        ctx.fill()
        setFill(FK_INK)
        ctx.textAlign = 'center'
        ctx.fillText(label, bx + bw / 2, p.y + HDR_H / 2 + 0.5)
        nameW -= bw + 8
      }
      setFont(`600 13.5px ${FONT_SANS}`)
      setFill(c('fg', 1))
      ctx.textAlign = 'left'
      fillClipped(d.name ?? '', p.x + 16, p.y + HDR_H / 2, nameW, 13.5)
    }

    if (zoom < LOD_ROWS) {
      // Row text is sub-pixel down here and rightly skipped - but leaving the
      // body empty turns a schema-wide view into a field of blank rectangles,
      // which is what the whole diagram looks like at fit-zoom. The rows are
      // drawn as bars instead: same rhythm, same PK/FK colour, no glyphs.
      if (zoom >= LOD_BARS) drawRowBars(d, p, h)
      ctx.globalAlpha = 1
      return
    }

    // Column rows.
    const pk = d.pkCols ?? new Set()
    const cols = d.columns ?? []
    for (let i = 0; i < cols.length; i++) {
      const col = cols[i]
      const isPk = pk.has(col.name)
      const isFk = !!col.foreignKey
      const cy = p.y + HDR_H + i * ROW_H
      if (i > 0) {
        ctx.beginPath()
        ctx.moveTo(p.x, cy); ctx.lineTo(p.x + NODE_W, cy)
        ctx.lineWidth = 0.75 / zoom; ctx.strokeStyle = c('border', 0.12); ctx.stroke()
      }
      const midY = cy + ROW_H / 2
      // A key into a hub names the hub. That relationship has no line of its
      // own (see hubEdges), so the row is where it is read.
      const hubRef = isFk && !isPk ? hubRefOf(col.foreignKey) : null
      const badge = isPk ? 'pk' : hubRef ? `→ ${hubRef}` : isFk ? 'fk' : ''
      let bw = badge ? 26 : 0
      if (hubRef) { setFont(FONT_BADGE); bw = Math.min(110, Math.ceil(ctx.measureText(badge).width) + 14) }
      const typeRight = p.x + NODE_W - (badge ? bw + 26 : 16)
      const nameRight = showTypes ? typeRight - 4 : p.x + NODE_W - (badge ? bw + 20 : 16)

      // Column name.
      setFont(isPk ? FONT_ROW_PK : FONT_ROW)
      setFill(isPk ? PK_NAME : isFk ? FK_NAME : c('fg', 0.82))
      ctx.textAlign = 'left'
      fillClipped(col.name ?? '', p.x + 16, midY, nameRight - (p.x + 16), 12)

      // Data type.
      if (showTypes) {
        setFont(FONT_TYPE)
        setFill(c('mfg', 0.6))
        ctx.textAlign = 'right'
        fillClipped(String(col.dataType ?? ''), typeRight, midY, 72, 11, true)
      }

      // PK / FK badge.
      if (badge) {
        const bx = p.x + NODE_W - bw - 12
        roundRect(bx, cy + 6, bw, ROW_H - 12, 3)
        setFill(isPk ? PK_WASH : FK_WASH)
        ctx.fill()
        setFont(FONT_BADGE)
        setFill(isPk ? PK_INK : FK_INK)
        ctx.textAlign = 'center'
        ctx.fillText(badge, bx + bw / 2, midY + 0.5)
      }
    }

    // "Keys only" mode hides the plain columns - say how many, so a card is never
    // silently truncated.
    if (d.hiddenCount) {
      const cy = p.y + HDR_H + cols.length * ROW_H
      ctx.beginPath()
      ctx.moveTo(p.x, cy); ctx.lineTo(p.x + NODE_W, cy)
      ctx.lineWidth = 0.75 / zoom; ctx.strokeStyle = c('border', 0.12); ctx.stroke()
      setFont(FONT_TYPE)
      setFill(c('mfg', 0.45))
      ctx.textAlign = 'left'
      fillClipped(`+${d.hiddenCount} more`, p.x + 16, cy + ROW_H / 2, NODE_W - 32, 11)
    }
    ctx.globalAlpha = 1
  }

  /**
   * Draw monospace text clipped to a max pixel width (ellipsis by char count).
   * @param {string} text @param {number} x @param {number} y @param {number} maxW @param {number} size @param {boolean} [rightAlign]
   */
  function fillClipped(text, x, y, maxW, size, rightAlign = false) {
    const charW = size * 0.6
    const max = Math.max(1, Math.floor(maxW / charW))
    let s = text
    if (s.length > max) s = s.slice(0, Math.max(1, max - 1)) + '…'
    ctx.fillText(s, x, y)
    void rightAlign
  }

  // ── Minimap ──────────────────────────────────────────────────────────────
  const MINI_W = 168, MINI_H = 116, MINI_PAD = 8
  /** @type {{s:number,ox:number,oy:number}|null} */
  let miniXf = null
  /**
   * The cards on the minimap only move when the graph does, but the viewport
   * rectangle moves on every frame of every pan - so the cards are painted once
   * into an offscreen canvas and blitted, instead of a thousand fills a frame.
   * @type {HTMLCanvasElement|null}
   */
  let miniCache = null
  let miniKey = ''
  /**
   * A card's rows as coloured bars, for zoom levels where the text would not be
   * legible anyway. Batched into one path per tint, so a card costs three fills
   * rather than one per column.
   * @param {any} d @param {{x:number,y:number}} p @param {number} h
   */
  function drawRowBars(d, p, h) {
    const cols = d.columns ?? []
    if (!cols.length) return
    const pk = d.pkCols ?? new Set()
    const barH = Math.max(2, ROW_H * 0.4)
    const inset = 14
    const full = NODE_W - inset * 2
    const bottom = p.y + h
    // 0 = primary key, 1 = foreign key, 2 = everything else.
    for (let kind = 0; kind < 3; kind++) {
      ctx.beginPath()
      let any = false
      for (let i = 0; i < cols.length; i++) {
        const col = cols[i]
        const k = pk.has(col.name) ? 0 : col.foreignKey ? 1 : 2
        if (k !== kind) continue
        const by = p.y + HDR_H + i * ROW_H + (ROW_H - barH) / 2
        if (by + barH > bottom) break
        // A plain column gets a shorter bar, so a key still reads as the long one.
        ctx.rect(p.x + inset, by, kind === 2 ? full * 0.6 : full * 0.85, barH)
        any = true
      }
      if (!any) continue
      setFill(kind === 0 ? PK_WASH : kind === 1 ? FK_WASH : c('mfg', 0.2))
      ctx.fill()
    }
  }

  function drawMinimap() {
    if (!mini || !nodes.length) return
    const mctx = mini.getContext('2d')
    if (!mctx) return

    // Only on a real change: assigning width/height reallocates the backing
    // store and clears it, and this runs on every frame of every pan.
    if (mini.width !== MINI_W * dpr || mini.height !== MINI_H * dpr) {
      mini.width = MINI_W * dpr; mini.height = MINI_H * dpr
      mini.style.width = MINI_W + 'px'; mini.style.height = MINI_H + 'px'
      miniKey = ''
    }

    // Drag position is keyed by id only: following it per frame would repaint the
    // cache on every frame of the drag, which is the thing being avoided.
    const key = `${graphVersion}|${selectedId}|${focusId}|${dragPos?.id ?? ''}|${pal.fg}|${dpr}`
    if (key !== miniKey || !miniCache) {
      let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity
      for (const n of nodes) {
        const p = posOf(n), h = nodeH(n.data)
        x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y)
        x1 = Math.max(x1, p.x + NODE_W); y1 = Math.max(y1, p.y + h)
      }
      const bw = x1 - x0 || 1, bh = y1 - y0 || 1
      const s = Math.min((MINI_W - MINI_PAD * 2) / bw, (MINI_H - MINI_PAD * 2) / bh)
      const ox = MINI_PAD - x0 * s + (MINI_W - MINI_PAD * 2 - bw * s) / 2
      const oy = MINI_PAD - y0 * s + (MINI_H - MINI_PAD * 2 - bh * s) / 2
      miniXf = { s, ox, oy }

      if (!miniCache) miniCache = document.createElement('canvas')
      miniCache.width = MINI_W * dpr
      miniCache.height = MINI_H * dpr
      const cc = miniCache.getContext('2d')
      if (!cc) return
      cc.setTransform(dpr, 0, 0, dpr, 0, 0)
      cc.fillStyle = c('panel', 1); cc.fillRect(0, 0, MINI_W, MINI_H)
      for (const n of nodes) {
        const p = posOf(n), h = nodeH(n.data)
        const isFocus = !!focusId && n.id === focusId
        cc.fillStyle = isFocus ? c('focus', 1)
          : n.id === selectedId ? c('fg', 0.8)
          : c('mfg', n.data?.highlighted === false ? 0.2 : 0.45)
        cc.fillRect(p.x * s + ox, p.y * s + oy, NODE_W * s, h * s)
      }
      miniKey = key
    }
    if (!miniXf) return

    mctx.setTransform(1, 0, 0, 1, 0, 0)
    mctx.clearRect(0, 0, mini.width, mini.height)
    mctx.drawImage(miniCache, 0, 0)
    mctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    // Viewport rectangle.
    const { s, ox, oy } = miniXf
    const { panX, panY, zoom } = cam
    const rx = (-panX / zoom) * s + ox, ry = (-panY / zoom) * s + oy
    const rw = (cssW / zoom) * s, rh = (cssH / zoom) * s
    mctx.strokeStyle = c('fg', 0.5); mctx.lineWidth = 1
    mctx.strokeRect(rx, ry, rw, rh)
  }

  // ── Camera ops ─────────────────────────────────────────────────────────
  export function fit(padding = 0.12, ms = 360) {
    if (!nodes.length || !cssW) return
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity
    for (const n of nodes) {
      const p = n.position, h = nodeH(n.data)
      x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y)
      x1 = Math.max(x1, p.x + NODE_W); y1 = Math.max(y1, p.y + h)
    }
    const bw = x1 - x0 || 1, bh = y1 - y0 || 1
    const z = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, Math.min(cssW / bw, cssH / bh) * (1 - padding)))
    flyTo({ zoom: z, panX: (cssW - bw * z) / 2 - x0 * z, panY: (cssH - bh * z) / 2 - y0 * z }, ms)
  }

  /** @param {string} id */
  export function focus(id) {
    const n = byId.get(id)
    if (!n || !cssW) return
    const p = n.position, h = nodeH(n.data)
    const z = Math.max(cam.zoom, 0.8)
    flyTo({ zoom: z, panX: cssW / 2 - (p.x + NODE_W / 2) * z, panY: cssH / 2 - (p.y + h / 2) * z }, 420)
  }

  /** @param {number} factor @param {number} [cx] @param {number} [cy] */
  function zoomBy(factor, cx = cssW / 2, cy = cssH / 2, ms = 0) {
    const z = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, cam.zoom * factor))
    const wx = (cx - cam.panX) / cam.zoom, wy = (cy - cam.panY) / cam.zoom
    flyTo({ zoom: z, panX: cx - wx * z, panY: cy - wy * z }, ms)
  }
  // The buttons ease; the wheel is the hand on the dial and stays immediate.
  export function zoomIn() { zoomBy(1.25, undefined, undefined, 160) }
  export function zoomOut() { zoomBy(0.8, undefined, undefined, 160) }

  // ── Hit testing ───────────────────────────────────────────────────────
  /** @param {number} sx @param {number} sy */
  function hitNode(sx, sy) {
    const wx = (sx - cam.panX) / cam.zoom, wy = (sy - cam.panY) / cam.zoom
    for (let i = nodes.length - 1; i >= 0; i--) {
      const n = nodes[i], p = posOf(n), h = nodeH(n.data)
      if (wx >= p.x && wx <= p.x + NODE_W && wy >= p.y && wy <= p.y + h) return n
    }
    return null
  }

  // ── Pointer / wheel handlers ──────────────────────────────────────────
  /** @param {PointerEvent} e */
  function onPointerDown(e) {
    if (!canvas) return
    cancelCamAnim()
    canvas.setPointerCapture(e.pointerId)
    const r = canvas.getBoundingClientRect()
    const sx = e.clientX - r.left, sy = e.clientY - r.top
    const hit = hitNode(sx, sy)
    drag = { mode: hit ? 'drag' : 'pan', id: hit?.id, ox: sx, oy: sy, sx, sy, moved: false }
    if (hit) {
      const p = hit.position
      drag.ox = (sx - cam.panX) / cam.zoom - p.x
      drag.oy = (sy - cam.panY) / cam.zoom - p.y
    }
  }
  /** @param {PointerEvent} e */
  function onPointerMove(e) {
    if (!canvas) return
    const r = canvas.getBoundingClientRect()
    const sx = e.clientX - r.left, sy = e.clientY - r.top
    if (drag.mode === 'none') {
      const hit = hitNode(sx, sy)
      const id = hit?.id ?? null
      if (id !== hoveredId) { hoveredId = id; canvas.style.cursor = id ? 'pointer' : 'default'; markDirty() }
      return
    }
    if (Math.abs(sx - drag.sx) + Math.abs(sy - drag.sy) > 3) drag.moved = true
    if (drag.mode === 'pan') {
      cam.panX += sx - drag.ox; cam.panY += sy - drag.oy
      drag.ox = sx; drag.oy = sy; markDirty()
    } else if (drag.mode === 'drag' && drag.id) {
      dragPos = { id: drag.id, x: (sx - cam.panX) / cam.zoom - drag.ox, y: (sy - cam.panY) / cam.zoom - drag.oy }
      markDirty()
    }
  }
  /** @param {PointerEvent} e */
  function onPointerUp(e) {
    if (canvas?.hasPointerCapture(e.pointerId)) canvas.releasePointerCapture(e.pointerId)
    if (drag.mode === 'drag' && drag.id) {
      if (drag.moved && dragPos) onnodemoved(drag.id, dragPos.x, dragPos.y)
      else onselect(drag.id)
      dragPos = null
    } else if (drag.mode === 'pan' && !drag.moved) {
      onselect(null)
    }
    drag = { mode: 'none', ox: 0, oy: 0, sx: 0, sy: 0, moved: false }
    markDirty()
  }
  /** @param {MouseEvent} e */
  function onDblClick(e) {
    if (!canvas) return
    const r = canvas.getBoundingClientRect()
    const hit = hitNode(e.clientX - r.left, e.clientY - r.top)
    if (hit) onopen(hit.id)
  }
  /**
   * Ctrl/Cmd + wheel zooms at the cursor (a trackpad pinch arrives the same
   * way, as ctrl+wheel); a plain wheel scrolls the diagram, and Shift turns a
   * vertical wheel sideways. Every wheel used to zoom, so there was no way to
   * scroll a large schema, and Ctrl+wheel never arrived at all: the global
   * zoom blocker swallows it outside a [data-zoom-surface].
   * @param {WheelEvent} e
   */
  function onWheel(e) {
    e.preventDefault()
    if (!canvas) return
    cancelCamAnim()
    // Line and page deltas (a notched wheel on some engines) to pixels.
    const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? cssH : 1
    const dx = e.deltaX * unit, dy = e.deltaY * unit
    if (e.ctrlKey || e.metaKey) {
      const r = canvas.getBoundingClientRect()
      zoomBy(Math.exp(-dy * 0.0015), e.clientX - r.left, e.clientY - r.top)
      return
    }
    if (e.shiftKey && dx === 0) { cam.panX -= dy } else { cam.panX -= dx; cam.panY -= dy }
    markDirty()
  }

  // Click the minimap to recenter there.
  /** @param {PointerEvent} e */
  function onMiniDown(e) {
    if (!mini || !miniXf) return
    const r = mini.getBoundingClientRect()
    const wx = (e.clientX - r.left - miniXf.ox) / miniXf.s
    const wy = (e.clientY - r.top - miniXf.oy) / miniXf.s
    cam.panX = cssW / 2 - wx * cam.zoom
    cam.panY = cssH / 2 - wy * cam.zoom
    markDirty()
  }

  // ── Resize ────────────────────────────────────────────────────────────
  /** @type {ResizeObserver|null} */
  let ro = null
  function resize() {
    if (!host || !canvas) return
    dpr = window.devicePixelRatio || 1
    cssW = host.clientWidth; cssH = host.clientHeight
    canvas.width = Math.round(cssW * dpr); canvas.height = Math.round(cssH * dpr)
    canvas.style.width = cssW + 'px'; canvas.style.height = cssH + 'px'
    ensureFit()
    markDirty()
  }

  /** @type {MutationObserver|null} */
  let themeObs = null
  onMount(() => {
    ctx = canvas?.getContext('2d') ?? null
    refreshTheme()
    resize()
    ro = new ResizeObserver(resize)
    if (host) ro.observe(host)
    themeObs = new MutationObserver(refreshTheme)
    themeObs.observe(document.documentElement, { attributes: true, attributeFilter: ['class', 'data-theme', 'style'] })
    // Fit once positions are in (nodes may already be present, or arrive later).
    requestAnimationFrame(ensureFit)
    // Webfonts (Geist) load async; redraw once ready so text isn't stuck on the
    // fallback face.
    document.fonts?.ready?.then(() => markDirty())
  })
  onDestroy(() => {
    ro?.disconnect()
    themeObs?.disconnect()
    if (rafId) cancelAnimationFrame(rafId)
  })
</script>

<div bind:this={host} class="absolute inset-0 overflow-hidden">
  <span bind:this={probe} class="pointer-events-none absolute size-0 opacity-0" aria-hidden="true"></span>

  <canvas
    bind:this={canvas}
    class="block touch-none select-none"
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerUp}
    ondblclick={onDblClick}
    onwheel={onWheel}
    data-zoom-surface
  ></canvas>

  <!-- Zoom controls -->
  <div class="absolute left-4 bottom-4 flex flex-col overflow-hidden rounded-lg border border-border/50 bg-panel elevate-2-rim">
    <button type="button" class="flex size-8 items-center justify-center border-b border-border/40 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" onclick={zoomIn} title="Zoom in">
      <Plus class="size-3.5" />
    </button>
    <button type="button" class="flex size-8 items-center justify-center border-b border-border/40 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" onclick={zoomOut} title="Zoom out">
      <Minus class="size-3.5" />
    </button>
    <button type="button" class="flex size-8 items-center justify-center text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" onclick={() => fit()} title="Fit to view">
      <Maximize class="size-3.5" />
    </button>
  </div>

  <!-- Line routing runs in slices across frames; say so while it is landing. -->
  {#if routePct >= 0}
    <div class="pointer-events-none absolute left-4 top-4 flex h-6 items-center gap-2 rounded-full border border-border/50 bg-panel/85 px-2.5 font-mono text-ui-3xs tabular-nums text-muted-foreground backdrop-blur-sm">
      <span class="size-1.5 animate-pulse rounded-full bg-primary"></span>
      Routing lines {routePct}%
    </div>
  {/if}

  <!-- Minimap -->
  {#if nodes.length}
    <canvas
      bind:this={mini}
      class="absolute right-4 bottom-4 cursor-pointer rounded-lg border border-border/50 elevate-2-rim"
      onpointerdown={onMiniDown}
    ></canvas>
  {/if}
</div>
