<script>
  /**
   * Geo view - every spatial column in the database, drawn on a world map.
   *
   * The map is a canvas, not a tile layer: the country outlines ship with the
   * app (`public/world.geo.json`, the same file the choropleth chart uses), so
   * this makes no network requests and works offline, on a VPN, and against a
   * database that has no business talking to a tile server.
   *
   * Rendering has exactly two layers and one accent: filled land in a neutral,
   * the rows on top in colour. The land has to be filled - an outline-only
   * basemap vanishes the moment data covers it, and a continent you can't
   * recognise gives a cluster nothing to mean. Everything else is left out, so
   * the only thing competing for attention is the data.
   */
  import { onMount } from 'svelte'
  import { geoOverview, geoFeatures } from '$lib/api.js'
  import { getWorldGeoJson } from '$lib/geo.js'
  import { FILTER_OPS } from '$lib/table-query.js'
  import { cn } from '$lib/utils.js'
  import {
    worldView,
    toScreen,
    fromScreen,
    toLonLat,
    viewBounds,
    zoomAbout,
    panBy,
    fitExtent,
    degreesPerPixel,
    clusterCell,
    walkGeometry,
    hitTest,
  } from '$lib/geo-map.js'
  import * as Select from '$lib/components/ui/select/index.js'
  import Globe from '@lucide/svelte/icons/globe'
  import Layers from '@lucide/svelte/icons/layers'
  import Filter from '@lucide/svelte/icons/filter'
  import Maximize2 from '@lucide/svelte/icons/maximize-2'
  import RefreshCw from '@lucide/svelte/icons/refresh-cw'
  import Plus from '@lucide/svelte/icons/plus'
  import Minus from '@lucide/svelte/icons/minus'
  import X from '@lucide/svelte/icons/x'
  import Loader2 from '@lucide/svelte/icons/loader-2'
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert'

  let {
    /**
     * Restrict the layer list to one table, for the Map view embedded in a table
     * tab. The picker still lists that table's own geometry columns - a table can
     * have several, and they can mean different things (a point and its parcel).
     */
    scopeSchema = '',
    scopeTable = '',
  } = $props()

  /**
   * How many real features to ask for before the server switches to clusters.
   * Individual marks say strictly more than a grid summary, so the budget is set
   * as high as a canvas can draw in one frame rather than as low as is safe.
   */
  const FEATURE_BUDGET = 8000
  /** Viewport changes settle before refetching - a drag is many events, one query. */
  const REFETCH_DELAY = 220

  /**
   * Basemaps.
   *
   * "Minimal" is the only one that ships with the app: country outlines from
   * `public/world.geo.json`, drawn on canvas, no network. It is the default
   * precisely because a database client should not phone a third party the
   * moment you open a view.
   *
   * The rest are raster tiles from public providers, so choosing one starts
   * fetching images from that host. Satellite and street detail cannot be
   * shipped offline - the imagery is terabytes - so this is the trade being
   * offered, and it is offered explicitly rather than made the default.
   */
  const BASEMAPS = [
    { id: 'minimal', label: 'Minimal', offline: true, url: '', attribution: '', maxZoom: 22 },
    {
      id: 'dark',
      label: 'Dark',
      url: 'https://basemaps.cartocdn.com/dark_all/{z}/{x}/{y}.png',
      attribution: '© OpenStreetMap · © CARTO',
      maxZoom: 20,
    },
    {
      id: 'streets',
      label: 'Streets',
      url: 'https://tile.openstreetmap.org/{z}/{x}/{y}.png',
      attribution: '© OpenStreetMap contributors',
      maxZoom: 19,
    },
    {
      id: 'satellite',
      label: 'Satellite',
      url: 'https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}',
      attribution: 'Esri · Maxar · Earthstar Geographics',
      maxZoom: 19,
    },
  ]
  /** Tiles held in memory. Bounded: panning a big map would otherwise grow without end. */
  const TILE_CACHE_MAX = 320

  /** @type {HTMLDivElement|null} */
  let host = $state(null)
  /** @type {HTMLCanvasElement|null} */
  let canvas = $state(null)

  /** @type {any[]} */
  let layers = $state([])
  /** @type {any|null} */
  let layer = $state(null)
  let postgisVersion = $state('')
  let available = $state(true)

  /** @type {any[]} */
  let features = $state([])
  let mode = $state('features')
  let matched = $state(0)
  let queryMs = $state(0)
  let loading = $state(false)
  let error = $state('')
  /** @type {any|null} */
  let selected = $state(null)

  // One filter, applied on top of the viewport. It reuses the grid's operator
  // vocabulary and reaches the same `build_where` on the server, so "population
  // gt 1000000" means exactly what it means in the table view.
  let filterColumn = $state('')
  let filterOp = $state('eq')
  let filterValue = $state('')
  /** Columns offered to the filter - the keys of a fetched row, so no extra query. */
  let filterColumns = $state(/** @type {string[]} */ ([]))

  const opNeedsValue = $derived(FILTER_OPS.find((o) => o.value === filterOp)?.needsValue !== false)
  const activeFilters = $derived(
    filterColumn && (!opNeedsValue || filterValue !== '')
      ? [{ column: filterColumn, op: filterOp, value: opNeedsValue ? filterValue : null }]
      : null,
  )

  /** @type {{ scale: number, x: number, y: number }} */
  let view = $state({ scale: 800, x: 0.5, y: 0.5 })
  let width = $state(0)
  let height = $state(0)
  /** @type {{ lon: number, lat: number }|null} */
  let cursor = $state(null)

  let basemapId = $state('minimal')
  const basemap = $derived(BASEMAPS.find((b) => b.id === basemapId) ?? BASEMAPS[0])

  /** @type {Map<string, HTMLImageElement>} */
  const tileCache = new Map()
  /** Tiles the provider refused or the network dropped, for the current basemap. */
  let tileErrors = $state(0)

  /** @type {any|null} */
  let world = null
  /** @type {ReturnType<typeof setTimeout>|undefined} */
  let refetchTimer
  let frame = 0
  /** Bumped on every viewport fetch so a slow one can't overwrite a newer one. */
  let fetchSeq = 0
  /**
   * The grid size, in degrees, the server actually clustered by. Kept because a
   * cluster disc has to be drawn at the size of its own cell, and the view can
   * be zoomed between the fetch and the frame that draws its result.
   */
  let clusterCellDeg = 1

  // ── Theme ───────────────────────────────────────────────────────────────────
  // Tokens are oklch strings that canvas keeps verbatim, so each one is
  // rasterized to a pixel and read back as sRGB. Same approach as ErdCanvas.
  /** @type {CanvasRenderingContext2D|null} */
  let probeCtx = null
  let palette = $state({ land: '128,128,128', ink: '160,160,160', accent: '99,102,241', bg: '10,10,10' })
  /** Additive glow only reads on a dark canvas; on a light one it burns to white. */
  let darkCanvas = $state(true)

  /** @param {string} raw */
  function resolveColor(raw) {
    if (!raw) return '128,128,128'
    if (!probeCtx) {
      const cv = document.createElement('canvas')
      cv.width = cv.height = 1
      probeCtx = cv.getContext('2d', { willReadFrequently: true })
    }
    if (!probeCtx) return '128,128,128'
    probeCtx.clearRect(0, 0, 1, 1)
    // Over the theme's solid surface first, so a translucent Glass token reads
    // back as the colour it shows as, not as its pure unblended rgb.
    const base = getComputedStyle(document.documentElement).getPropertyValue('--popover').trim()
    if (base) { probeCtx.fillStyle = base; probeCtx.fillRect(0, 0, 1, 1) }
    probeCtx.fillStyle = '#808080'
    probeCtx.fillStyle = raw
    probeCtx.fillRect(0, 0, 1, 1)
    const d = probeCtx.getImageData(0, 0, 1, 1).data
    return `${d[0]},${d[1]},${d[2]}`
  }

  /**
   * The colour the data is drawn in.
   *
   * Normally the theme's accent, so the map belongs to the rest of the app. But
   * several themes resolve `--primary` to a near-white with no chroma at all
   * (`oklch(0.985 0 0)`), and white data over a neutral basemap on a black
   * canvas is indistinguishable from the basemap - which is the one thing the
   * map exists to show. When the accent has no colour to give, fall back to the
   * app's chart lead, which always does.
   * @param {string} rgb "r,g,b"
   */
  function dataColor(rgb) {
    const [r, g, b] = rgb.split(',').map(Number)
    const chroma = Math.max(r, g, b) - Math.min(r, g, b)
    return chroma >= 24 ? rgb : '99,102,241'
  }

  function refreshTheme() {
    const el = host || document.documentElement
    const read = (/** @type {string} */ n) =>
      resolveColor(getComputedStyle(el).getPropertyValue(n).trim())
    const bg = read('--background')
    palette = {
      land: read('--border'),
      ink: read('--foreground'),
      accent: dataColor(read('--primary')),
      bg,
    }
    const [r, g, b] = bg.split(',').map(Number)
    darkCanvas = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255 < 0.5
  }

  // ── Data ────────────────────────────────────────────────────────────────────

  async function loadLayers() {
    try {
      const res = await geoOverview()
      available = Boolean(res?.available)
      postgisVersion = res?.version || ''
      const all = res?.layers || []
      layers = scopeTable
        ? all.filter((l) => l.table === scopeTable && (!scopeSchema || l.schema === scopeSchema))
        : all
      if (layers.length && !layer) selectLayer(layers[0])
    } catch (err) {
      error = err instanceof Error ? err.message : String(err)
    }
  }

  /** @param {any} next */
  function selectLayer(next) {
    layer = next
    selected = null
    features = []
    matched = 0
    // A filter names a column of the old table; carrying it over would either
    // error or, worse, silently match a same-named column that means something
    // different.
    filterColumn = ''
    filterValue = ''
    filterColumns = []
    view = worldView(width || 800, height || 600)
    void fetchFeatures({ fit: true })
  }

  /** Re-run with the filter, and re-frame - the matching rows are usually elsewhere. */
  function applyFilter() {
    selected = null
    void fetchFeatures({ fit: true })
  }

  /** @param {{ fit?: boolean }} [opts] */
  async function fetchFeatures(opts = {}) {
    if (!layer) return
    const seq = ++fetchSeq
    loading = true
    error = ''
    const bounds = width && height ? viewBounds(view, width, height) : null
    const cell = clusterCell(view)
    try {
      const res = await geoFeatures({
        schema: layer.schema,
        table: layer.table,
        column: layer.column,
        kind: layer.kind,
        srid: layer.srid,
        geomType: layer.geomType,
        bbox: bounds
          ? {
              minX: bounds.minLon,
              minY: bounds.minLat,
              maxX: bounds.maxLon,
              maxY: bounds.maxLat,
            }
          : null,
        limit: FEATURE_BUDGET,
        // One screen pixel: vertices closer together than this cannot be seen.
        simplify: degreesPerPixel(view) * 0.75,
        clusterCell: cell,
        filters: activeFilters,
        includeExtent: Boolean(opts.fit),
      })
      // A viewport moved on while this was in flight - its answer is stale.
      if (seq !== fetchSeq) return
      features = res?.features || []
      if (res?.mode === 'clusters') clusterCellDeg = cell
      // The server sends the column list alongside the extent, i.e. once per
      // layer. Deriving it from a returned feature instead meant a table big
      // enough to always cluster never offered a filter at all.
      if (Array.isArray(res?.columns) && res.columns.length) filterColumns = res.columns
      mode = res?.mode || 'features'
      matched = Number(res?.matched ?? 0)
      queryMs = Number(res?.queryMs ?? 0)
      if (opts.fit && Array.isArray(res?.extent) && width && height) {
        view = fitExtent(
          /** @type {[number, number, number, number]} */ (res.extent),
          width,
          height,
        )
        // Fitting changed the viewport, so the features just fetched are for the
        // wrong box. Re-ask for the box we actually landed on.
        scheduleRefetch()
      }
      draw()
    } catch (err) {
      if (seq !== fetchSeq) return
      error = err instanceof Error ? err.message : String(err)
      features = []
    } finally {
      if (seq === fetchSeq) loading = false
    }
  }

  function scheduleRefetch() {
    clearTimeout(refetchTimer)
    refetchTimer = setTimeout(() => void fetchFeatures(), REFETCH_DELAY)
  }

  // ── Rendering ───────────────────────────────────────────────────────────────

  function requestDraw() {
    if (frame) return
    frame = requestAnimationFrame(() => {
      frame = 0
      draw()
    })
  }

  function draw() {
    if (!canvas || !width || !height) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    const dpr = Math.min(2, window.devicePixelRatio || 1)
    if (canvas.width !== Math.round(width * dpr) || canvas.height !== Math.round(height * dpr)) {
      canvas.width = Math.round(width * dpr)
      canvas.height = Math.round(height * dpr)
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.clearRect(0, 0, width, height)

    const project = (/** @type {number} */ x, /** @type {number} */ y) =>
      toScreen(view, width, height, x, y)

    // The bundled vector map is always drawn. Tiles go OVER it rather than
    // instead of it, so a provider that is slow, blocked, or offline degrades to
    // the map that ships with the app instead of to a black rectangle - and the
    // first paint after switching basemaps has something on it while the tiles
    // are still in flight.
    drawGraticule(ctx)
    drawWorld(ctx, project)
    if (!basemap.offline) drawTiles(ctx)
    drawFeatures(ctx, project)
  }

  /** @param {CanvasRenderingContext2D} ctx */
  function drawGraticule(ctx) {
    // Meridians and parallels every 15°, faint. They carry the sense of scale
    // and rotation that a bare outline map loses when you zoom in.
    ctx.save()
    ctx.strokeStyle = `rgba(${palette.ink},0.05)`
    ctx.lineWidth = 1
    ctx.beginPath()
    for (let lon = -180; lon <= 180; lon += 15) {
      const a = toScreen(view, width, height, (lon + 180) / 360, 0)
      if (a.px < -2 || a.px > width + 2) continue
      ctx.moveTo(Math.round(a.px) + 0.5, 0)
      ctx.lineTo(Math.round(a.px) + 0.5, height)
    }
    for (let lat = -75; lat <= 75; lat += 15) {
      const a = toScreen(view, width, height, 0, latToWorldY(lat))
      if (a.py < -2 || a.py > height + 2) continue
      ctx.moveTo(0, Math.round(a.py) + 0.5)
      ctx.lineTo(width, Math.round(a.py) + 0.5)
    }
    ctx.stroke()
    ctx.restore()
  }

  /**
   * One raster tile, or null while it loads. A tile that arrives later triggers
   * a redraw rather than blocking the frame that asked for it - a map that waits
   * for its slowest tile is a map that stutters on every pan.
   * @param {string} url
   */
  function getTile(url) {
    const hit = tileCache.get(url)
    if (hit) return hit.complete && hit.naturalWidth > 0 ? hit : null
    const img = new Image()
    img.decoding = 'async'
    img.referrerPolicy = 'no-referrer'
    img.onload = () => requestDraw()
    // A failed tile stays in the cache as a permanent miss, so a dead host is
    // requested once per tile instead of on every frame forever. The count is
    // surfaced: an online basemap that silently fails looks identical to one
    // that is merely slow, and the user can do nothing about either without
    // being told which it is.
    img.onerror = () => {
      tileErrors += 1
    }
    img.src = url
    if (tileCache.size >= TILE_CACHE_MAX) {
      const oldest = tileCache.keys().next().value
      if (oldest !== undefined) tileCache.delete(oldest)
    }
    tileCache.set(url, img)
    return null
  }

  /** @param {CanvasRenderingContext2D} ctx */
  function drawTiles(ctx) {
    const spec = basemap
    if (spec.offline || !spec.url) return
    // Tiles exist at power-of-two zooms; pick the one whose natural size is
    // closest to how big it will be drawn, so the imagery is neither blurry nor
    // downscaled from four times the pixels it needs.
    const z = Math.max(0, Math.min(spec.maxZoom, Math.round(Math.log2(view.scale / 256))))
    const n = 1 << z
    const size = view.scale / n
    const tl = fromScreen(view, width, height, 0, 0)
    const br = fromScreen(view, width, height, width, height)
    const x0 = Math.max(0, Math.floor(tl.x * n))
    const x1 = Math.min(n - 1, Math.ceil(br.x * n))
    const y0 = Math.max(0, Math.floor(tl.y * n))
    const y1 = Math.min(n - 1, Math.ceil(br.y * n))
    if ((x1 - x0 + 1) * (y1 - y0 + 1) > 400) return

    for (let ty = y0; ty <= y1; ty++) {
      for (let tx = x0; tx <= x1; tx++) {
        const url = spec.url
          .replace('{z}', String(z))
          .replace('{x}', String(tx))
          .replace('{y}', String(ty))
        const img = getTile(url)
        if (!img) continue
        const at = toScreen(view, width, height, tx / n, ty / n)
        // The extra pixel closes the hairline seams that fractional tile sizes
        // leave between neighbours.
        ctx.drawImage(img, Math.floor(at.px), Math.floor(at.py), Math.ceil(size) + 1, Math.ceil(size) + 1)
      }
    }
  }

  /** @param {number} lat */
  function latToWorldY(lat) {
    const φ = (lat * Math.PI) / 180
    return 0.5 - Math.log(Math.tan(Math.PI / 4 + φ / 2)) / (2 * Math.PI)
  }

  /**
   * @param {CanvasRenderingContext2D} ctx
   * @param {(x: number, y: number) => { px: number, py: number }} project
   */
  function drawWorld(ctx, project) {
    if (!world) return
    // Zoomed into one city, 216 of the 217 countries are off screen. Culling on
    // a precomputed bounding box skips their vertices entirely, which is what
    // keeps a drag at 60fps instead of re-walking the whole basemap per frame.
    const b = viewBounds(view, width, height)
    ctx.save()
    // Land is FILLED, not outlined. An outline-only basemap disappears the
    // moment data is drawn over it, and a map you cannot recognise is not a map
    // - the continent is the frame that makes a cluster mean something. The tone
    // comes from the foreground rather than the border token so it holds its
    // contrast in either polarity: 10% of near-white on black reads as land,
    // and 10% of near-black on white does too.
    ctx.strokeStyle = `rgba(${palette.ink},0.22)`
    ctx.fillStyle = `rgba(${palette.ink},0.1)`
    ctx.lineWidth = 1
    ctx.lineJoin = 'round'
    for (const feature of world.features || []) {
      const box = feature.__bbox
      if (
        box &&
        (box[2] < b.minLon || box[0] > b.maxLon || box[3] < b.minLat || box[1] > b.maxLat)
      ) {
        continue
      }
      ctx.beginPath()
      walkGeometry(feature.geometry, project, {
        point() {},
        ring(pts) {
          ctx.moveTo(pts[0], pts[1])
          for (let i = 2; i + 1 < pts.length; i += 2) ctx.lineTo(pts[i], pts[i + 1])
          ctx.closePath()
        },
      })
      ctx.fill()
      ctx.stroke()
    }
    ctx.restore()
  }

  /**
   * Stamp a lon/lat bounding box onto each basemap feature, once, at load.
   * @param {any} data
   */
  function indexWorld(data) {
    for (const feature of data?.features || []) {
      let minLon = Infinity
      let minLat = Infinity
      let maxLon = -Infinity
      let maxLat = -Infinity
      const visit = (/** @type {any} */ coords) => {
        if (typeof coords[0] === 'number') {
          if (coords[0] < minLon) minLon = coords[0]
          if (coords[0] > maxLon) maxLon = coords[0]
          if (coords[1] < minLat) minLat = coords[1]
          if (coords[1] > maxLat) maxLat = coords[1]
          return
        }
        for (const c of coords) visit(c)
      }
      if (feature?.geometry?.coordinates) visit(feature.geometry.coordinates)
      feature.__bbox = [minLon, minLat, maxLon, maxLat]
    }
    return data
  }

  /** Short form for a marker label: 1483 → 1.5k, 1250000 → 1.3M. */
  function abbreviate(n) {
    if (n >= 1e6) return `${(n / 1e6).toFixed(n >= 1e7 ? 0 : 1)}M`
    if (n >= 1e4) return `${Math.round(n / 1e3)}k`
    if (n >= 1e3) return `${(n / 1e3).toFixed(1)}k`
    return String(n)
  }

  /** Readable ink for text sitting on `rgb`. */
  function inkOn(rgb) {
    const [r, g, b] = rgb.split(',').map(Number)
    return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255 > 0.6 ? '17,17,17' : '255,255,255'
  }

  /**
   * @param {CanvasRenderingContext2D} ctx
   * @param {(x: number, y: number) => { px: number, py: number }} project
   */
  function drawFeatures(ctx, project) {
    if (!features.length) return
    const accent = palette.accent
    // Every mark carries a ring in the page background colour. Over the offline
    // basemap it is nearly invisible; over satellite imagery it is the only
    // reason a dark blue dot is still findable on a dark blue sea.
    const ring = `rgba(${palette.bg},0.9)`
    ctx.save()
    ctx.lineJoin = 'round'
    ctx.lineCap = 'round'

    if (mode === 'clusters') {
      const peak = features.reduce((m, f) => Math.max(m, Number(f?.properties?.count) || 0), 1)
      const cellPx = clusterCellDeg / degreesPerPixel(view)
      const rMax = Math.max(7, Math.min(cellPx * 0.42, 30))
      const rMin = Math.min(5, rMax)

      /** @type {Array<{x: number, y: number, r: number, n: number}>} */
      const marks = []
      for (const f of features) {
        const n = Number(f?.properties?.count) || 0
        let px = 0
        let py = 0
        walkGeometry(f.geometry, project, { point: (x, y) => ((px = x), (py = y)), ring() {} })
        if (px < -60 || px > width + 60 || py < -60 || py > height + 60) continue
        // Area, not radius, tracks the count - radius alone exaggerates a busy
        // cell by the square of how busy it is.
        marks.push({ x: px, y: py, r: rMin + (rMax - rMin) * Math.sqrt(n / peak), n })
      }
      // Biggest first, so a small cluster lands on top of a large one instead of
      // disappearing under it.
      marks.sort((a, b) => b.r - a.r)

      ctx.strokeStyle = ring
      ctx.lineWidth = 2
      for (const m of marks) {
        ctx.beginPath()
        ctx.arc(m.x, m.y, m.r, 0, Math.PI * 2)
        ctx.fillStyle = `rgba(${accent},0.88)`
        ctx.fill()
        ctx.stroke()
      }

      // Counts, on the marks with room for them. A cluster map that makes you
      // guess magnitude from area alone is throwing away the number it already
      // has.
      ctx.textAlign = 'center'
      ctx.textBaseline = 'middle'
      ctx.fillStyle = `rgb(${inkOn(accent)})`
      for (const m of marks) {
        if (m.r < 11) continue
        const label = abbreviate(m.n)
        const size = Math.max(9, Math.min(m.r * 0.72, 15))
        ctx.font = `600 ${size}px ui-monospace, SFMono-Regular, Menlo, monospace`
        if (ctx.measureText(label).width > m.r * 1.75) continue
        ctx.fillText(label, m.x, m.y + 0.5)
      }
      ctx.restore()
      return
    }

    // Individual rows. Points and areas are drawn in separate passes because they
    // want opposite fills: a polygon needs a translucent wash so overlapping ones
    // stay readable, a point needs an opaque centre. Sharing one fillStyle - as
    // this did - paints every dot at the polygon's alpha and turns the whole
    // layer into hollow rings.
    const dotted = features.length > 3000 ? 1.8 : 3

    /** @param {any} f @param {boolean} pointsOnly */
    const trace = (f, pointsOnly) => {
      let drew = false
      walkGeometry(f.geometry, project, {
        point(px, py) {
          if (!pointsOnly) return
          if (px < -8 || px > width + 8 || py < -8 || py > height + 8) return
          ctx.moveTo(px + dotted, py)
          ctx.arc(px, py, dotted, 0, Math.PI * 2)
          drew = true
        },
        ring(pts, closed) {
          if (pointsOnly) return
          ctx.moveTo(pts[0], pts[1])
          for (let i = 2; i + 1 < pts.length; i += 2) ctx.lineTo(pts[i], pts[i + 1])
          if (closed) ctx.closePath()
          drew = true
        },
      })
      return drew
    }

    // Areas and lines, one feature at a time so each polygon's wash is its own
    // and a hole is subtracted from its own shell rather than a neighbour's.
    ctx.strokeStyle = `rgba(${accent},0.85)`
    ctx.fillStyle = `rgba(${accent},0.18)`
    ctx.lineWidth = 1.4
    for (const f of features) {
      if (selected && f === selected.feature) continue
      ctx.beginPath()
      if (trace(f, false)) {
        ctx.fill()
        ctx.stroke()
      }
    }

    // Every point in one path: stroke first for a rim in the page colour, then
    // fill over the inner half of that rim. The rim is what keeps a dot findable
    // against satellite imagery of roughly its own colour.
    ctx.beginPath()
    let anyPoint = false
    for (const f of features) {
      if (selected && f === selected.feature) continue
      if (trace(f, true)) anyPoint = true
    }
    if (anyPoint) {
      ctx.strokeStyle = ring
      ctx.lineWidth = 2.5
      ctx.stroke()
      ctx.fillStyle = `rgba(${accent},0.95)`
      ctx.fill()
    }

    // The selection last, so it is never painted over by a neighbour.
    if (selected?.feature) {
      ctx.beginPath()
      const isArea = trace(selected.feature, false)
      if (isArea) {
        ctx.fillStyle = `rgba(${accent},0.3)`
        ctx.fill()
      }
      ctx.strokeStyle = ring
      ctx.lineWidth = 4
      ctx.stroke()
      ctx.strokeStyle = `rgb(${palette.ink})`
      ctx.lineWidth = 2
      ctx.stroke()

      ctx.beginPath()
      if (trace(selected.feature, true)) {
        ctx.strokeStyle = ring
        ctx.lineWidth = 4
        ctx.stroke()
        ctx.fillStyle = `rgb(${palette.ink})`
        ctx.fill()
      }
    }
    ctx.restore()
  }

  // ── Interaction ─────────────────────────────────────────────────────────────

  // Reactive because the cursor is derived from them. Driving the cursor off CSS
  // `:active` instead looks right until the first drag: the element holds a
  // captured pointer, `:active` stops tracking the button reliably, and the map
  // is left showing a closed hand forever.
  let dragging = $state(false)
  let hoveringFeature = $state(false)
  let dragMoved = false
  let lastX = 0
  let lastY = 0
  let hoverFrame = 0

  /** grab · grabbing while panning · pointer over something a click would select. */
  const mapCursor = $derived(dragging ? 'grabbing' : hoveringFeature ? 'pointer' : 'grab')

  /** @param {PointerEvent} e */
  function onPointerDown(e) {
    if (e.button !== 0) return
    dragMoved = false
    lastX = e.clientX
    lastY = e.clientY
    // Capture BEFORE arming the drag. If it throws - a stale pointer id, a
    // webview that lost the pointer - an exception thrown after `dragging = true`
    // would leave the map panning on plain hover with no button held, because
    // nothing would ever deliver the pointerup that clears it.
    try {
      /** @type {HTMLElement} */ (e.currentTarget).setPointerCapture(e.pointerId)
    } catch {
      // Capture is an optimisation: the window-level pointerup below still ends
      // the drag. Panning works without it.
    }
    dragging = true
  }

  /**
   * The coordinate readout and the hover test are both per-pixel work that only
   * has to be right once per frame - running them on every pointermove writes
   * reactive state ~120 times a second and hit-tests thousands of geometries in
   * between paints, for a readout nobody can read that fast.
   * @param {number} px @param {number} py
   */
  function scheduleHover(px, py) {
    if (hoverFrame) return
    hoverFrame = requestAnimationFrame(() => {
      hoverFrame = 0
      const w = fromScreen(view, width, height, px, py)
      cursor = toLonLat(w.x, w.y)
      // Clusters aren't selectable, so there is nothing to point at - and that
      // is also the mode with the most features to walk.
      // Hit-testing walks every feature, so it is skipped where it would cost
      // more than it tells you: mid-drag (the cursor is already 'grabbing'), in
      // cluster mode (nothing there is selectable), and on a set large enough
      // that the walk would eat the frame it is trying to stay ahead of.
      hoveringFeature =
        mode === 'features' &&
        !dragging &&
        features.length <= 4000 &&
        !!hitTest(features, (x, y) => toScreen(view, width, height, x, y), px, py, 7)
    })
  }

  /** @param {PointerEvent} e */
  function onPointerMove(e) {
    const rect = canvas?.getBoundingClientRect()
    if (rect) scheduleHover(e.clientX - rect.left, e.clientY - rect.top)
    if (!dragging) return
    const dx = e.clientX - lastX
    const dy = e.clientY - lastY
    if (Math.abs(dx) + Math.abs(dy) > 2) dragMoved = true
    lastX = e.clientX
    lastY = e.clientY
    view = panBy(view, dx, dy, width, height)
    requestDraw()
  }

  /** @param {PointerEvent} e */
  function onPointerUp(e) {
    if (!dragging) return
    dragging = false
    try {
      /** @type {HTMLElement} */ (e.currentTarget).releasePointerCapture?.(e.pointerId)
    } catch {
      // Already released, or never captured.
    }
    if (dragMoved) {
      scheduleRefetch()
      return
    }
    // A click that didn't drag is a pick.
    const rect = canvas?.getBoundingClientRect()
    if (!rect) return
    const hit = hitTest(
      features,
      (x, y) => toScreen(view, width, height, x, y),
      e.clientX - rect.left,
      e.clientY - rect.top,
      7,
    )
    selected = mode === 'clusters' ? null : hit
    requestDraw()
  }

  /** @param {WheelEvent} e */
  function onWheel(e) {
    e.preventDefault()
    const rect = canvas?.getBoundingClientRect()
    if (!rect) return
    const factor = Math.pow(1.0015, -e.deltaY)
    view = zoomAbout(view, width, height, factor, e.clientX - rect.left, e.clientY - rect.top)
    requestDraw()
    scheduleRefetch()
  }

  /** @param {number} factor */
  function zoomButton(factor) {
    view = zoomAbout(view, width, height, factor, width / 2, height / 2)
    requestDraw()
    scheduleRefetch()
  }

  function fitLayer() {
    void fetchFeatures({ fit: true })
  }

  /** @param {KeyboardEvent} e */
  function onKeyDown(e) {
    if (e.key === '+' || e.key === '=') { e.preventDefault(); zoomButton(1.6) }
    else if (e.key === '-' || e.key === '_') { e.preventDefault(); zoomButton(1 / 1.6) }
    else if (e.key === '0') { e.preventDefault(); fitLayer() }
    else if (e.key === 'Escape' && selected) { e.preventDefault(); selected = null; requestDraw() }
  }

  // ── Lifecycle ───────────────────────────────────────────────────────────────

  onMount(() => {
    // Safety net for the drag state. Pointer capture is supposed to guarantee the
    // canvas sees the matching pointerup, but a lost capture, a context menu, or
    // the window losing focus mid-drag can swallow it - and then the map is stuck
    // showing a closed hand and panning on hover with no way back. A window-level
    // release costs nothing and makes that unreachable.
    const releaseDrag = () => {
      if (dragging) dragging = false
    }
    window.addEventListener('pointerup', releaseDrag)
    window.addEventListener('pointercancel', releaseDrag)
    window.addEventListener('blur', releaseDrag)

    refreshTheme()
    const themeObserver = new MutationObserver(() => {
      refreshTheme()
      requestDraw()
    })
    themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class', 'data-theme'],
    })

    const ro = new ResizeObserver((entries) => {
      const box = entries[0]?.contentRect
      if (!box) return
      const first = !width
      width = box.width
      height = box.height
      if (first) view = worldView(width, height)
      requestDraw()
    })
    if (host) ro.observe(host)

    void getWorldGeoJson()
      .then((data) => {
        world = indexWorld(data)
        requestDraw()
      })
      .catch(() => {
        // The basemap is decoration. Losing it must not lose the data layer.
        world = null
      })
    void loadLayers()

    return () => {
      window.removeEventListener('pointerup', releaseDrag)
      window.removeEventListener('pointercancel', releaseDrag)
      window.removeEventListener('blur', releaseDrag)
      themeObserver.disconnect()
      ro.disconnect()
      clearTimeout(refetchTimer)
      if (frame) cancelAnimationFrame(frame)
      if (hoverFrame) cancelAnimationFrame(hoverFrame)
    }
  })

  const layerLabel = (/** @type {any} */ l) =>
    l ? `${l.schema === 'public' ? '' : `${l.schema}.`}${l.table}.${l.column}` : 'Select a layer'

  const rowsLabel = (/** @type {number} */ n) =>
    n < 0 ? '—' : n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1000 ? `${Math.round(n / 1000)}k` : String(n)

  // Switching basemap changes every pixel below the data.
  $effect(() => {
    void basemapId
    tileErrors = 0
    requestDraw()
  })

  /** Busiest cluster in view - the top of the density legend's scale. */
  const peakCount = $derived(
    features.reduce((m, f) => Math.max(m, Number(f?.properties?.count) || 0), 0),
  )

  const selectedEntries = $derived(
    selected?.feature?.properties ? Object.entries(selected.feature.properties) : [],
  )
</script>

<div class="flex min-h-0 flex-1 flex-col bg-background">
  <!-- Toolbar -->
  <div class="flex h-9 shrink-0 items-center gap-2 border-b border-border/60 px-2">
    <Globe class="size-3.5 shrink-0 text-muted-foreground" />
    <Select.Root
      type="single"
      value={layer ? layerLabel(layer) : ''}
      onValueChange={(v) => {
        const next = layers.find((l) => layerLabel(l) === v)
        if (next) selectLayer(next)
      }}
    >
      <Select.Trigger size="sm" class="w-[290px] text-ui-xs" aria-label="Spatial layer">
        <span class="truncate">{layer ? layerLabel(layer) : 'Select a layer'}</span>
      </Select.Trigger>
      <Select.Content class="z-[120] max-h-[22rem] min-w-[18rem] p-1" sideOffset={6}>
        {#each layers as l (layerLabel(l))}
          <Select.Item
            value={layerLabel(l)}
            label={layerLabel(l)}
            class="py-1.5 pl-2 text-ui-xs"
          >
            <span class="truncate">{layerLabel(l)}</span>
            <span class="ml-auto shrink-0 pl-3 text-ui-2xs text-muted-foreground">
              {l.geomType} · {rowsLabel(l.rows)}
            </span>
          </Select.Item>
        {/each}
      </Select.Content>
    </Select.Root>

    {#if filterColumns.length}
      <div class="flex items-center gap-1 border-l border-border/50 pl-2">
        <Filter class="size-3.5 shrink-0 text-muted-foreground" />
        <Select.Root type="single" value={filterColumn} onValueChange={(v) => (filterColumn = v ?? '')}>
          <Select.Trigger size="sm" class="w-[130px] text-ui-xs" aria-label="Filter column">
            <span class="truncate">{filterColumn || 'Column'}</span>
          </Select.Trigger>
          <Select.Content class="z-[120] max-h-[20rem] min-w-[10rem] p-1" sideOffset={6}>
            <Select.Item value="" label="No filter" class="py-1.5 pl-2 text-ui-xs">
              No filter
            </Select.Item>
            {#each filterColumns as c (c)}
              <Select.Item value={c} label={c} class="py-1.5 pl-2 text-ui-xs">{c}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>

        {#if filterColumn}
          <Select.Root type="single" value={filterOp} onValueChange={(v) => v && (filterOp = v)}>
            <Select.Trigger size="sm" class="w-[120px] text-ui-xs" aria-label="Filter operator">
              <span class="truncate">
                {FILTER_OPS.find((o) => o.value === filterOp)?.label ?? filterOp}
              </span>
            </Select.Trigger>
            <Select.Content class="z-[120] max-h-[20rem] min-w-[10rem] p-1" sideOffset={6}>
              {#each FILTER_OPS as o (o.value)}
                <Select.Item value={o.value} label={o.label} class="py-1.5 pl-2 text-ui-xs">
                  {o.label}
                </Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>

          {#if opNeedsValue}
            <input
            class= "field-surface h-7 w-[140px] bg-muted/30 px-2 text-ui-xs outline-none focus:border-ring"
              placeholder="Value"
              bind:value={filterValue}
              onkeydown={(e) => e.key === 'Enter' && applyFilter()}
            />
          {/if}
          <button
            type="button"
            class="h-7 rounded-md px-2 text-ui-xs text-muted-foreground hover:bg-muted/50 hover:text-foreground"
            onclick={applyFilter}
          >
            Apply
          </button>
        {/if}
      </div>
    {/if}

    <div class="ml-auto flex items-center gap-1">
      <Select.Root type="single" value={basemapId} onValueChange={(v) => v && (basemapId = v)}>
        <!-- No fixed width: 112px truncated "Minimal" to "Min…" and would have cut
             "Satellite" harder still. min-w keeps the control from jittering as the
             label changes while letting it grow for the longest one. Styled as a
             ghost like the Fit / Refresh buttons it sits beside - a bordered field
             in a three-control toolbar cluster read as a different component. -->
        <Select.Trigger
          size="sm"
          class="w-auto min-w-[8rem] border-0 bg-transparent text-ui-xs text-muted-foreground hover:bg-muted/50 hover:text-foreground data-[state=open]:bg-muted/50 data-[state=open]:text-foreground"
          aria-label="Basemap"
        >
          <span class="truncate">{basemap.label}</span>
        </Select.Trigger>
        <Select.Content class="z-[120] min-w-[15rem] p-1" sideOffset={6}>
          {#each BASEMAPS as b (b.id)}
            <Select.Item value={b.id} label={b.label} class="py-1.5 pl-2 text-ui-xs">
              <span class="truncate">{b.label}</span>
              <span class="ml-auto shrink-0 pl-3 text-ui-2xs text-muted-foreground">
                {b.offline ? 'offline' : 'online tiles'}
              </span>
            </Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
      <button
        type="button"
        class="flex h-7 items-center gap-1.5 rounded-md px-2 text-ui-xs text-muted-foreground hover:bg-muted/50 hover:text-foreground"
        onclick={fitLayer}
        title="Zoom to fit the layer (0)"
      >
        <Maximize2 class="size-3.5 shrink-0" />
        Fit
      </button>
      <button
        type="button"
        class="flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground"
        onclick={() => void fetchFeatures()}
        title="Refresh"
      >
        <RefreshCw class={cn('size-3.5 shrink-0', loading && 'animate-spin')} />
      </button>
    </div>
  </div>

  {#if error}
    <div
      class="flex shrink-0 items-center gap-2 border-b border-destructive/30 bg-destructive/10 px-3 py-1.5 text-ui-xs text-destructive"
    >
      <TriangleAlert class="size-3.5 shrink-0" />
      <span class="min-w-0 flex-1 truncate">{error}</span>
      <button type="button" class="shrink-0 opacity-70 hover:opacity-100" onclick={() => (error = '')}>
        <X class="size-3.5 shrink-0" />
      </button>
    </div>
  {/if}

  <!-- Map. `application` tells assistive tech to pass keys through, which is what
       a pan/zoom surface needs; the linter only knows the ARIA default. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    bind:this={host}
    class="relative min-h-0 flex-1 overflow-hidden outline-none"
    role="application"
    aria-label="Map of {layer ? layerLabel(layer) : 'no layer selected'}"
    tabindex="0"
    onkeydown={onKeyDown}
  >
    {#if !available}
      <div class="flex h-full flex-col items-center justify-center gap-2 px-8 text-center">
        <Globe class="size-8 shrink-0 text-muted-foreground" />
        <p class="text-ui-xl font-medium">No spatial data here</p>
        <p class="max-w-md text-ui-sm text-muted-foreground">
          The map reads PostGIS geometry and geography columns. Install the extension with
          <code class="rounded bg-muted/50 px-1 py-0.5 font-mono text-ui-xs">CREATE EXTENSION postgis</code>
          to map this database.
        </p>
      </div>
    {:else if !layers.length}
      <div class="flex h-full flex-col items-center justify-center gap-2 px-8 text-center">
        <Layers class="size-8 shrink-0 text-muted-foreground" />
        <p class="text-ui-xl font-medium">Nothing to map yet</p>
        <p class="max-w-md text-ui-sm text-muted-foreground">
          PostGIS {postgisVersion} is installed, but no table has a geometry or geography column.
        </p>
      </div>
    {:else}
      <canvas
        bind:this={canvas}
        class="block size-full touch-none"
        style="width:{width}px;height:{height}px;cursor:{mapCursor}"
        onpointerdown={onPointerDown}
        onpointermove={onPointerMove}
        onpointerup={onPointerUp}
        onpointercancel={onPointerUp}
        onpointerleave={() => { cursor = null; hoveringFeature = false }}
        onwheel={onWheel}
      ></canvas>

      <!-- Zoom controls -->
      <div class="absolute top-3 right-3 flex flex-col overflow-hidden rounded-lg border border-border/60 bg-popover elevate-2-rim">
        <button
          type="button"
          class="flex size-7 items-center justify-center text-muted-foreground hover:bg-accent hover:text-foreground"
          onclick={() => zoomButton(1.6)}
          title="Zoom in (+)"
        >
          <Plus class="size-3.5 shrink-0" />
        </button>
        <div class="h-px bg-border/60"></div>
        <button
          type="button"
          class="flex size-7 items-center justify-center text-muted-foreground hover:bg-accent hover:text-foreground"
          onclick={() => zoomButton(1 / 1.6)}
          title="Zoom out (−)"
        >
          <Minus class="size-3.5 shrink-0" />
        </button>
      </div>

      <!-- Selected feature -->
      {#if selected}
        <div
          class="absolute top-3 left-3 flex max-h-[70%] w-[300px] flex-col overflow-hidden rounded-lg border border-border/60 bg-popover elevate-2-rim"
        >
          <div class="flex h-8 shrink-0 items-center gap-2 border-b border-border/50 px-2.5">
            <span class="min-w-0 flex-1 truncate text-ui-xs font-medium">{layer?.table}</span>
            <button
              type="button"
              class="shrink-0 text-muted-foreground hover:text-foreground"
              onclick={() => { selected = null; requestDraw() }}
              title="Close (Esc)"
            >
              <X class="size-3.5 shrink-0" />
            </button>
          </div>
          <div class="min-h-0 flex-1 overflow-y-auto p-2.5">
            <dl class="grid grid-cols-[minmax(0,auto)_minmax(0,1fr)] gap-x-3 gap-y-1">
              {#each selectedEntries as [key, value] (key)}
                <dt class="truncate text-ui-2xs text-muted-foreground">{key}</dt>
                <dd class="min-w-0 break-words font-mono text-ui-2xs">
                  {value === null ? 'NULL' : typeof value === 'object' ? JSON.stringify(value) : String(value)}
                </dd>
              {/each}
            </dl>
          </div>
        </div>
      {/if}

      <!-- Readout. Floating over the map rather than pinned to a status strip:
           it describes what you are looking at, so it belongs on it, and a chip
           can hold a legend that a 24px bar cannot. -->
      <div class="pointer-events-none absolute inset-x-3 bottom-3 flex items-end justify-between gap-3">
        <div class="flex flex-col gap-1.5">
          {#if mode === 'clusters' && features.length > 400}
            <!-- Density legend. The ramp is doing the encoding, so it has to say
                 what it encodes; a heatmap without a scale is decoration. -->
            <div class="flex w-fit items-center gap-2 rounded-lg border border-border/50 bg-background px-2.5 py-1.5">
              <span class="text-ui-3xs uppercase tracking-[0.06em] text-muted-foreground">Density</span>
              <span
                class="h-1.5 w-24 rounded-full"
                style="background:linear-gradient(90deg, rgba({palette.accent},0.35), rgba({palette.accent},0.65), rgb({palette.accent}))"
              ></span>
              <span class="font-mono text-ui-3xs tabular-nums text-muted-foreground">
                {peakCount.toLocaleString()}
              </span>
            </div>
          {/if}

          {#if layer}
            <div class="flex w-fit items-center gap-2 rounded-lg border border-border/50 bg-background px-2.5 py-1.5 text-ui-2xs">
              {#if loading}
                <Loader2 class="size-3 shrink-0 animate-spin text-muted-foreground" />
              {:else}
                <span class="size-1.5 shrink-0 rounded-full" style="background:rgb({palette.accent})"></span>
              {/if}
              <span class="font-mono tabular-nums">
                {matched.toLocaleString()}
              </span>
              <span class="text-muted-foreground">
                {matched === 1 ? 'row' : 'rows'} in view
              </span>
              {#if mode === 'clusters'}
                <span class="text-muted-foreground">·</span>
                <span class="text-muted-foreground tabular-nums">
                  {features.length.toLocaleString()} clusters
                </span>
              {/if}
              <span class="text-muted-foreground">·</span>
              <span class="text-muted-foreground">SRID {layer.srid || 'unset'}</span>
              {#if queryMs}
                <span class="text-muted-foreground">·</span>
                <span class="text-muted-foreground tabular-nums">{queryMs} ms</span>
              {/if}
            </div>
          {/if}
        </div>

        {#if !basemap.offline}
          <span
            class={cn(
              'pointer-events-none absolute right-0 -top-5 rounded bg-background px-1.5 py-0.5 text-ui-3xs',
              tileErrors > 0 ? 'text-muted-foreground' : 'text-muted-foreground',
            )}
          >
            {#if tileErrors > 0}
              {tileErrors} tile{tileErrors === 1 ? '' : 's'} failed to load - showing the bundled map
            {:else}
              {basemap.attribution}
            {/if}
          </span>
        {/if}

        {#if cursor}
          <!-- Signed decimals make you work out which hemisphere you are in.
               Hemisphere letters and a fixed width mean the number stops jumping
               as you move and can be read at a glance. -->
          <div class="flex shrink-0 items-center gap-2.5 rounded-lg border border-border/50 bg-background px-2.5 py-1.5 font-mono text-ui-2xs tabular-nums">
            <span>
              <span class="text-muted-foreground">{cursor.lat >= 0 ? 'N' : 'S'}</span>
              {Math.abs(cursor.lat).toFixed(4)}°
            </span>
            <span>
              <span class="text-muted-foreground">{cursor.lon >= 0 ? 'E' : 'W'}</span>
              {Math.abs(cursor.lon).toFixed(4)}°
            </span>
          </div>
        {/if}
      </div>
    {/if}
  </div>

</div>

