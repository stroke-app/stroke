<script>
  /**
   * Every shape in a query result on one map, beside the grid.
   *
   * Rows and features are the same thing seen two ways, so selection is
   * shared: clicking a feature selects its row (shift or cmd adds to the
   * selection) and opens a card with the row's other columns; rows selected in
   * the grid stand out on the map. Offline outline by default; tiles are one
   * labelled click away, like the cell viewer.
   */
  import { onMount, untrack } from 'svelte'
  import { worldView, toWorld, toScreen, fromScreen, toLonLat, viewBounds, zoomAbout, panBy, fitExtent, walkGeometry, distanceToSegmentSq } from '$lib/geo-map.js'
  import { BASEMAPS, basemapById, loadWorldOutline, createTileCache, drawTileLayer, readMapPalette } from '$lib/geo-basemap.js'
  import { formatLonLat, geometrySummary } from '$lib/geometry-cell.js'
  import { resultLayer, unplacedHint, insideRings } from '$lib/result-geo.js'
  import Icon from './Icon.svelte'
  import SelectMenu from './SelectMenu.svelte'
  import { cn } from '$lib/utils.js'

  let {
    /** @type {{ name: string, data_type?: string }[]} */
    columns = [],
    /** @type {unknown[][]} */
    rows = [],
    /** Column indices that hold shapes (see spatialColumns). */
    spatial = /** @type {number[]} */ ([]),
    /** Selected row indices, shared with the grid. */
    selected = /** @type {Set<number>} */ (new Set()),
    /** @type {(row: number | null, additive: boolean) => void} */
    onselect = () => {},
    /** @type {() => void} */
    onclose = () => {},
    /** Rows not loaded yet (a windowed result): said, not hidden. */
    partial = false,
  } = $props()

  const STORE = 'stroke:result-map:basemap'
  let basemapId = $state((() => { try { return localStorage.getItem(STORE) || 'minimal' } catch { return 'minimal' } })())
  const basemap = $derived(basemapById(basemapId))
  /** Chosen spatial column; the first one until picked. */
  let colPick = $state(/** @type {number | null} */ (null))
  const col = $derived(colPick != null && spatial.includes(colPick) ? colPick : (spatial[0] ?? -1))
  const layer = $derived(col >= 0 ? resultLayer(rows, col) : null)
  const hint = $derived(layer && col >= 0 ? unplacedHint(layer, columns[col]?.name ?? 'geom') : '')

  /** @type {HTMLDivElement | null} */
  let host = $state(null)
  /** @type {HTMLCanvasElement | null} */
  let canvas = $state(null)
  /** @type {CanvasRenderingContext2D | null} */
  let ctx = null
  let width = $state(0)
  let height = $state(0)
  let view = $state({ scale: 512, x: 0.5, y: 0.5 })
  let palette = $state({ land: '128,128,128', ink: '160,160,160', accent: '99,102,241', bg: '10,10,10', dark: true })
  /** @type {any} */
  let world = null
  let frame = 0
  const tiles = createTileCache(() => requestDraw())
  /** @type {{ lon: number, lat: number } | null} */
  let cursor = $state(null)
  /** The row whose card is open, and where it was clicked. */
  let card = $state(/** @type {{ row: number, x: number, y: number } | null} */ (null))
  let hoverRow = $state(/** @type {number | null} */ (null))

  function requestDraw() {
    if (frame) return
    frame = requestAnimationFrame(() => { frame = 0; draw() })
  }

  function resize() {
    if (!host || !canvas) return
    const w = Math.max(1, Math.round(host.clientWidth))
    const h = Math.max(1, Math.round(host.clientHeight))
    const dpr = Math.min(window.devicePixelRatio || 1, 2)
    const first = !width
    width = w
    height = h
    canvas.width = Math.round(w * dpr)
    canvas.height = Math.round(h * dpr)
    canvas.style.width = `${w}px`
    canvas.style.height = `${h}px`
    ctx = canvas.getContext('2d')
    ctx?.setTransform(dpr, 0, 0, dpr, 0, 0)
    if (first) frameAll()
    requestDraw()
  }

  /** @param {[number, number, number, number] | null} extent */
  function frameExtent(extent) {
    if (!width || !height) return
    if (!extent) { view = worldView(width, height); requestDraw(); return }
    const [a, b, c, d] = extent
    view = c - a < 1e-9 && d - b < 1e-9 ? { scale: 1 << 15, ...toWorld((a + c) / 2, (b + d) / 2) } : fitExtent(extent, width, height, 36)
    requestDraw()
  }

  function frameAll() {
    frameExtent(layer?.extent ?? null)
  }

  /** The selected features' extent, for "frame the selection". */
  function selectionExtent() {
    if (!layer) return null
    let e = /** @type {[number, number, number, number] | null} */ (null)
    for (const f of layer.features) {
      if (!selected.has(f.row)) continue
      const [a, b, c, d] = f.shape.extent
      e = e ? [Math.min(e[0], a), Math.min(e[1], b), Math.max(e[2], c), Math.max(e[3], d)] : [a, b, c, d]
    }
    return e
  }

  onMount(() => {
    palette = readMapPalette(host)
    resize()
    const ro = new ResizeObserver(() => resize())
    if (host) ro.observe(host)
    const themeObs = new MutationObserver(() => { palette = readMapPalette(host); requestDraw() })
    themeObs.observe(document.documentElement, { attributes: true, attributeFilter: ['class', 'data-theme'] })
    loadWorldOutline().then((w) => { world = w; requestDraw() }).catch(() => {})
    return () => {
      ro.disconnect()
      themeObs.disconnect()
      if (frame) cancelAnimationFrame(frame)
    }
  })

  // A new result or another column: frame what's there and close the card.
  $effect(() => {
    void layer
    untrack(() => { card = null; frameAll() })
  })
  // Selection and hover only repaint.
  $effect(() => {
    void selected
    void hoverRow
    requestDraw()
  })

  /** @param {[number, number]} p */
  const project = (p) => {
    const w = toWorld(p[0], p[1])
    return toScreen(view, width, height, w.x, w.y)
  }

  function draw() {
    if (!ctx || !width) return
    const c = ctx
    c.clearRect(0, 0, width, height)
    c.fillStyle = `rgb(${palette.bg})`
    c.fillRect(0, 0, width, height)
    if (basemap.offline) drawWorld(c)
    else drawTileLayer(c, basemap, view, width, height, tiles, fromScreen, toScreen)
    if (!layer) return
    const b = viewBounds(view, width, height)
    const visible = (/** @type {[number, number, number, number]} */ e) => !(e[2] < b.minLon || e[0] > b.maxLon || e[3] < b.minLat || e[1] > b.maxLat)
    // Unselected first, then selected and hovered on top, so they're never buried.
    const lift = (/** @type {number} */ row) => selected.has(row) || row === hoverRow
    // Plain points are stamped from one pre-drawn sprite. Each had its own
    // save/restore, colours, fill and stroke: 30,000 points took 1.1s a draw
    // (one big path was slower still on Cairo), and every pan and zoom redraws.
    const dim = selected.size > 0
    /** @type {{ px: number, py: number }[]} */
    const dots = []
    for (const f of layer.features) {
      if (lift(f.row) || !visible(f.shape.extent)) continue
      if (f.shape.polygons.length || f.shape.lines.length) drawFeature(c, f.shape, false, dim)
      else for (const p of f.shape.points) dots.push(project(p))
    }
    if (dots.length) drawDots(c, dots, dim)
    for (const f of layer.features) if (lift(f.row) && visible(f.shape.extent)) drawFeature(c, f.shape, true, false)
  }

  /** The plain point, drawn once per colour and dimming at device pixels. */
  let sprite = /** @type {{ key: string, img: HTMLCanvasElement, size: number } | null} */ (null)
  /** @param {boolean} dim */
  function pointSprite(dim) {
    const dpr = Math.min(window.devicePixelRatio || 1, 2)
    const key = `${palette.accent}|${palette.bg}|${dim}|${dpr}`
    if (sprite?.key === key) return sprite
    const size = 12
    const img = document.createElement('canvas')
    img.width = Math.ceil(size * dpr)
    img.height = Math.ceil(size * dpr)
    const g = /** @type {CanvasRenderingContext2D} */ (img.getContext('2d'))
    g.scale(dpr, dpr)
    // Exactly drawFeature's plain point: a 4.5 dot with a 1.5 halo.
    g.beginPath()
    g.arc(size / 2, size / 2, 4.5, 0, Math.PI * 2)
    g.fillStyle = `rgba(${palette.accent},${dim ? 0.45 : 1})`
    g.fill()
    g.lineWidth = 1.5
    g.strokeStyle = `rgba(${palette.bg},0.9)`
    g.stroke()
    sprite = { key, img, size }
    return sprite
  }

  /** Points that aren't lifted, stamped. @param {CanvasRenderingContext2D} c @param {{ px: number, py: number }[]} dots @param {boolean} dim */
  function drawDots(c, dots, dim) {
    const { img, size } = pointSprite(dim)
    const h = size / 2
    // Zoomed out, thousands of points share a pixel; a second stamp of the
    // same sprite on the same pixel changes nothing, so each pixel gets one.
    const seen = new Set()
    for (const s of dots) {
      const x = Math.round(s.px), y = Math.round(s.py)
      const k = x * 65536 + y
      if (seen.has(k)) continue
      seen.add(k)
      c.drawImage(img, x - h, y - h, size, size)
    }
  }

  /** @param {CanvasRenderingContext2D} c */
  function drawWorld(c) {
    if (!world) return
    const b = viewBounds(view, width, height)
    c.save()
    c.strokeStyle = `rgba(${palette.ink},0.2)`
    c.fillStyle = `rgba(${palette.ink},0.09)`
    c.lineWidth = 1
    c.lineJoin = 'round'
    const proj = (/** @type {number} */ x, /** @type {number} */ y) => toScreen(view, width, height, x, y)
    for (const feature of world.features || []) {
      const box = feature.__bbox
      if (box && (box[2] < b.minLon || box[0] > b.maxLon || box[3] < b.minLat || box[1] > b.maxLat)) continue
      c.beginPath()
      walkGeometry(feature.geometry, proj, {
        point() {},
        ring(pts) {
          c.moveTo(pts[0], pts[1])
          for (let i = 2; i + 1 < pts.length; i += 2) c.lineTo(pts[i], pts[i + 1])
          c.closePath()
        },
      })
      c.fill()
      c.stroke()
    }
    c.restore()
  }

  /**
   * @param {CanvasRenderingContext2D} c
   * @param {import('$lib/geometry-cell.js').GeoShape} shape
   * @param {boolean} hot  selected or hovered
   * @param {boolean} dim  something else is selected
   */
  function drawFeature(c, shape, hot, dim) {
    const accent = palette.accent
    const a = dim ? 0.45 : 1
    c.save()
    c.lineJoin = 'round'
    c.lineCap = 'round'
    for (const rings of shape.polygons) {
      c.beginPath()
      for (const ring of rings) {
        ring.forEach((p, i) => {
          const s = project(p)
          if (i === 0) c.moveTo(s.px, s.py)
          else c.lineTo(s.px, s.py)
        })
        c.closePath()
      }
      c.fillStyle = `rgba(${accent},${hot ? 0.38 : 0.16 * a})`
      c.strokeStyle = `rgba(${accent},${hot ? 1 : 0.8 * a})`
      c.lineWidth = hot ? 2.5 : 1.5
      c.fill('evenodd')
      c.stroke()
    }
    c.strokeStyle = `rgba(${accent},${hot ? 1 : 0.9 * a})`
    c.lineWidth = hot ? 3.5 : 2
    for (const line of shape.lines) {
      c.beginPath()
      line.forEach((p, i) => {
        const s = project(p)
        if (i === 0) c.moveTo(s.px, s.py)
        else c.lineTo(s.px, s.py)
      })
      c.stroke()
    }
    for (const p of shape.points) {
      const s = project(p)
      if (hot) {
        c.beginPath()
        c.arc(s.px, s.py, 11, 0, Math.PI * 2)
        c.fillStyle = `rgba(${accent},0.25)`
        c.fill()
      }
      c.beginPath()
      c.arc(s.px, s.py, hot ? 6 : 4.5, 0, Math.PI * 2)
      c.fillStyle = `rgba(${accent},${a})`
      c.fill()
      c.lineWidth = 1.5
      c.strokeStyle = `rgba(${palette.bg},0.9)`
      c.stroke()
    }
    c.restore()
  }

  /**
   * The row under a screen point: points and lines within a few pixels win
   * over polygons they sit inside, and the topmost of each kind wins.
   * @param {number} px @param {number} py
   */
  function rowAt(px, py) {
    if (!layer) return null
    const tol = 7
    let inPolygon = /** @type {number | null} */ (null)
    for (let i = layer.features.length - 1; i >= 0; i--) {
      const { row, shape } = layer.features[i]
      for (const p of shape.points) {
        const s = project(p)
        if ((s.px - px) ** 2 + (s.py - py) ** 2 <= tol * tol) return row
      }
      for (const l of shape.lines) {
        const pts = l.map(project)
        for (let j = 0; j + 1 < pts.length; j++) {
          if (distanceToSegmentSq(px, py, pts[j].px, pts[j].py, pts[j + 1].px, pts[j + 1].py) <= tol * tol) return row
        }
      }
      if (inPolygon == null) {
        for (const poly of shape.polygons) {
          const rings = poly.map((r) => r.map(project))
          if (insideRings(px, py, rings)) { inPolygon = row; break }
        }
      }
    }
    return inPolygon
  }

  // ── Interaction ─────────────────────────────────────────────────────────────
  let dragFrom = $state(/** @type {{ x: number, y: number } | null} */ (null))
  let dragged = false

  /** @param {PointerEvent} e */
  function onPointerDown(e) {
    if (e.button !== 0) return
    dragFrom = { x: e.clientX, y: e.clientY }
    dragged = false
    /** @type {HTMLCanvasElement} */ (e.currentTarget).setPointerCapture(e.pointerId)
  }

  /** @param {PointerEvent} e */
  function onPointerMove(e) {
    const rect = /** @type {HTMLCanvasElement} */ (e.currentTarget).getBoundingClientRect()
    const x = e.clientX - rect.left
    const y = e.clientY - rect.top
    const w = fromScreen(view, width, height, x, y)
    cursor = toLonLat(Math.min(1, Math.max(0, w.x)), Math.min(1, Math.max(0, w.y)))
    if (!dragFrom) {
      hoverRow = rowAt(x, y)
      return
    }
    const dx = e.clientX - dragFrom.x
    const dy = e.clientY - dragFrom.y
    if (Math.abs(dx) > 2 || Math.abs(dy) > 2) dragged = true
    dragFrom = { x: e.clientX, y: e.clientY }
    view = panBy(view, dx, dy, width, height)
    if (card) card = null
    requestDraw()
  }

  /** @param {PointerEvent} e */
  function onPointerUp(e) {
    const was = dragFrom
    dragFrom = null
    try { /** @type {HTMLCanvasElement} */ (e.currentTarget).releasePointerCapture(e.pointerId) } catch { /* released */ }
    if (!was || dragged) return
    const rect = /** @type {HTMLCanvasElement} */ (e.currentTarget).getBoundingClientRect()
    const x = e.clientX - rect.left
    const y = e.clientY - rect.top
    const row = rowAt(x, y)
    const additive = e.shiftKey || e.metaKey || e.ctrlKey
    onselect(row, additive)
    card = row == null ? null : { row, x, y }
  }

  /** @param {WheelEvent} e */
  function onWheel(e) {
    e.preventDefault()
    const rect = /** @type {HTMLCanvasElement} */ (e.currentTarget).getBoundingClientRect()
    view = zoomAbout(view, width, height, Math.pow(0.999, e.deltaY), e.clientX - rect.left, e.clientY - rect.top)
    if (card) card = null
    requestDraw()
  }

  /** @param {number} factor */
  function zoomBy(factor) {
    view = zoomAbout(view, width, height, factor, width / 2, height / 2)
    card = null
    requestDraw()
  }

  /** @param {string} id */
  function pickBasemap(id) {
    basemapId = id
    try { localStorage.setItem(STORE, id) } catch { /* blocked */ }
    tiles.reset()
    requestDraw()
  }

  /** The card's fields: every column but the shape, short. */
  const cardFields = $derived.by(() => {
    if (!card) return []
    const r = rows[card.row] ?? []
    return columns
      .map((c, i) => ({ name: c.name, i }))
      .filter((c) => c.i !== col)
      .slice(0, 12)
      .map((c) => {
        const v = r[c.i]
        const text = v == null ? 'NULL' : spatial.includes(c.i) ? geometrySummary(v) : typeof v === 'object' ? JSON.stringify(v) : String(v)
        return { name: c.name, text: text.length > 80 ? `${text.slice(0, 79)}…` : text, isNull: v == null }
      })
  })

  /** Keep the card inside the map. */
  const cardPos = $derived.by(() => {
    if (!card) return null
    const w = 240
    const left = Math.min(Math.max(8, card.x + 12), Math.max(8, width - w - 8))
    const top = Math.min(Math.max(8, card.y + 12), Math.max(8, height - 180))
    return { left, top, w }
  })

  const shown = $derived(layer?.features.length ?? 0)

  const btn = 'inline-flex size-6 items-center justify-center rounded-md border border-border/50 bg-background text-muted-foreground transition-colors hover:bg-muted hover:text-foreground'
</script>

<div class="flex h-full min-h-0 flex-col border-l border-border/50 bg-background">
  <!-- Header: which column, how many shapes, and the way out. -->
  <div class="flex h-9 shrink-0 items-center gap-2 border-b border-border/50 px-2">
    <Icon name="globe" class="size-3.5 shrink-0 text-muted-foreground" />
    {#if spatial.length > 1}
      <SelectMenu
        ariaLabel="Column to map"
        triggerClass="h-7 w-auto min-w-0 max-w-48 font-mono text-ui-xs"
        contentClass="min-w-[12rem]"
        value={col}
        onValueChange={(v) => (colPick = Number(v))}
        items={spatial.map((i) => ({ value: i, label: columns[i]?.name ?? String(i) }))}
      />
    {:else}
      <span class="min-w-0 truncate font-mono text-ui-xs text-foreground">{columns[col]?.name ?? ''}</span>
    {/if}
    <span class="min-w-0 truncate text-ui-2xs text-muted-foreground tabular-nums">
      {shown} {shown === 1 ? 'shape' : 'shapes'}{selected.size ? ` · ${selected.size} selected` : ''}{partial ? ' · loaded rows' : ''}
    </span>
    <button type="button" class={cn(btn, 'ml-auto border-transparent')} onclick={onclose} title="Close the map" aria-label="Close the map">
      <Icon name="x" class="size-3.5 shrink-0" />
    </button>
  </div>

  <div bind:this={host} class="relative min-h-0 flex-1 overflow-hidden">
    <!-- svelte-ignore a11y_no_interactive_element_to_noninteractive_role -->
    <canvas
      bind:this={canvas}
      role="img"
      aria-label="Map of the result's {columns[col]?.name ?? ''} values"
      class={cn('block touch-none select-none', dragFrom ? 'cursor-grabbing' : hoverRow != null ? 'cursor-pointer' : 'cursor-grab')}
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={() => (dragFrom = null)}
      onpointerleave={() => { cursor = null; hoverRow = null }}
      onwheel={onWheel}
      ondblclick={() => zoomBy(2)}
    ></canvas>

    <div class="absolute left-2 top-2 flex items-center gap-0.5 rounded-md border border-border/50 bg-background p-0.5">
      {#each BASEMAPS as b (b.id)}
        <button
          type="button"
          onclick={() => pickBasemap(b.id)}
          title={b.offline ? 'Offline outline: no network requests' : `Tiles from ${b.attribution}`}
          class={cn('inline-flex h-5 items-center rounded-md px-1.5 text-ui-3xs transition-colors', basemapId === b.id ? 'bg-muted text-foreground' : 'text-muted-foreground hover:text-foreground')}
        >{b.label}</button>
      {/each}
    </div>

    <div class="absolute right-2 top-2 flex flex-col gap-1">
      <button type="button" class={btn} onclick={() => zoomBy(2)} title="Zoom in" aria-label="Zoom in"><Icon name="plus" class="size-3 shrink-0" /></button>
      <button type="button" class={btn} onclick={() => zoomBy(0.5)} title="Zoom out" aria-label="Zoom out"><Icon name="minus" class="size-3 shrink-0" /></button>
      <button type="button" class={btn} onclick={frameAll} title="Frame every shape" aria-label="Frame every shape"><Icon name="maximize-2" class="size-3 shrink-0" /></button>
      <button type="button" class={btn} disabled={!selected.size} onclick={() => frameExtent(selectionExtent())} title="Frame the selected rows" aria-label="Frame the selected rows"><Icon name="crosshair" class="size-3 shrink-0" /></button>
    </div>

    {#if card && cardPos}
      <div
        class="absolute z-10 rounded-lg border border-border/60 bg-popover p-2.5 shadow-lg"
        style="left:{cardPos.left}px; top:{cardPos.top}px; width:{cardPos.w}px"
        role="dialog"
        aria-label="Row {card.row + 1}"
      >
        <div class="mb-1.5 flex items-center gap-2">
          <span class="text-ui-xs font-medium text-foreground">Row {card.row + 1}</span>
          <button type="button" class={cn(btn, 'ml-auto size-5 border-transparent')} onclick={() => (card = null)} aria-label="Close"><Icon name="x" class="size-3 shrink-0" /></button>
        </div>
        <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5">
          {#each cardFields as f (f.name)}
            <dt class="truncate text-ui-2xs text-muted-foreground">{f.name}</dt>
            <dd class={cn('min-w-0 truncate font-mono text-ui-2xs', f.isNull ? 'text-muted-foreground' : 'text-foreground')} title={f.text}>{f.text}</dd>
          {/each}
        </dl>
      </div>
    {/if}

    {#if layer && !shown}
      <div class="pointer-events-none absolute inset-0 flex items-center justify-center p-6 text-center">
        <p class="max-w-64 rounded-md bg-background/90 px-3 py-2 text-ui-xs text-muted-foreground">
          {hint || 'No shapes in this column to draw.'}
        </p>
      </div>
    {/if}

    <div class="pointer-events-none absolute inset-x-2 bottom-2 flex items-end justify-between gap-2">
      <span class="rounded bg-background px-1.5 py-0.5 font-mono text-ui-3xs tabular-nums text-muted-foreground">
        {cursor ? formatLonLat(cursor.lon, cursor.lat) : 'drag to pan · scroll to zoom · click a shape'}
      </span>
      {#if !basemap.offline}
        <span class="truncate rounded bg-background px-1.5 py-0.5 text-ui-3xs text-muted-foreground">{basemap.attribution}</span>
      {/if}
    </div>
  </div>

  {#if hint && shown}
    <p class="shrink-0 border-t border-border/50 px-2.5 py-1.5 text-ui-2xs text-muted-foreground">{hint}</p>
  {:else if layer?.assumed}
    <p class="shrink-0 border-t border-border/50 px-2.5 py-1.5 text-ui-2xs text-muted-foreground">
      {layer.assumed} {layer.assumed === 1 ? 'value has' : 'values have'} no SRID and {layer.assumed === 1 ? 'is' : 'are'} drawn as WGS 84 (EPSG:4326).
    </p>
  {/if}
</div>
