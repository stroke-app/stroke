<script>
  /**
   * A geometry on a real map, small enough to live inside a dialog.
   *
   * The point of showing a coordinate on a map is answering "where is that
   * actually" - which a fixed preview can't do, because the answer is at a scale
   * you have to choose. So this pans and zooms: it opens framed on the geometry
   * with enough context to place it, and you can pull back to the continent or
   * push in to the street.
   *
   * Offline by default (country outlines ship with the app). The tiled basemaps
   * are one click away and clearly labelled, because turning one on means this
   * value's location is sent to a third-party tile host.
   */
  import { onMount } from 'svelte'
  import {
    worldView, toWorld, toScreen, fromScreen, toLonLat,
    viewBounds, zoomAbout, panBy, fitExtent, walkGeometry,
  } from '$lib/geo-map.js'
  import {
    BASEMAPS, basemapById, loadWorldOutline, createTileCache, drawTileLayer, readMapPalette,
  } from '$lib/geo-basemap.js'
  import { formatLonLat } from '$lib/geometry-cell.js'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'

  let {
    /** @type {import('$lib/geometry-cell.js').GeoShape} */
    shape,
    /** Canvas height in px. Width follows the container. */
    height = 260,
    class: extraClass = '',
  } = $props()

  /** @type {HTMLDivElement|null} */
  let host = $state(null)
  /** @type {HTMLCanvasElement|null} */
  let canvas = $state(null)
  /** @type {CanvasRenderingContext2D|null} */
  let ctx = null

  let width = $state(0)
  /** @type {{ scale: number, x: number, y: number }} */
  let view = $state({ scale: 512, x: 0.5, y: 0.5 })
  let basemapId = $state('minimal')
  const basemap = $derived(basemapById(basemapId))

  /** @type {{ lon: number, lat: number }|null} */
  let cursor = $state(null)
  let palette = $state({ land: '128,128,128', ink: '160,160,160', accent: '99,102,241', bg: '10,10,10', dark: true })

  /** @type {any|null} */
  let world = null
  let frame = 0
  const tiles = createTileCache(() => requestDraw())

  /**
   * A single point has no extent to frame, so it gets a neighbourhood-level
   * scale - close enough to recognise the place, wide enough to know which one.
   * Everything else is fitted with padding so the shape isn't flush to the edge.
   */
  function frameShape() {
    if (!width) return
    const [minLon, minLat, maxLon, maxLat] = shape.extent
    const degenerate = maxLon - minLon < 1e-9 && maxLat - minLat < 1e-9
    view = degenerate
      ? { scale: 1 << 15, ...toWorld((minLon + maxLon) / 2, (minLat + maxLat) / 2) }
      : fitExtent(shape.extent, width, height, 28)
  }

  function requestDraw() {
    if (frame) return
    frame = requestAnimationFrame(() => { frame = 0; draw() })
  }

  function resize() {
    if (!host || !canvas) return
    const w = Math.max(1, Math.round(host.clientWidth))
    const dpr = Math.min(window.devicePixelRatio || 1, 2)
    width = w
    canvas.width = Math.round(w * dpr)
    canvas.height = Math.round(height * dpr)
    canvas.style.width = `${w}px`
    canvas.style.height = `${height}px`
    ctx = canvas.getContext('2d')
    ctx?.setTransform(dpr, 0, 0, dpr, 0, 0)
    requestDraw()
  }

  onMount(() => {
    palette = readMapPalette(host)
    resize()
    frameShape()
    const ro = new ResizeObserver(() => { resize(); })
    if (host) ro.observe(host)
    // Theme changes repaint: the canvas holds resolved sRGB, not live tokens.
    const themeObs = new MutationObserver(() => { palette = readMapPalette(host); requestDraw() })
    themeObs.observe(document.documentElement, { attributes: true, attributeFilter: ['class', 'data-theme'] })
    loadWorldOutline().then((w) => { world = w; requestDraw() }).catch(() => {})
    return () => {
      ro.disconnect()
      themeObs.disconnect()
      if (frame) cancelAnimationFrame(frame)
    }
  })

  // Re-frame when the dialog opens on a different value.
  $effect(() => {
    void shape
    if (width) frameShape()
  })

  const project = (/** @type {number} */ x, /** @type {number} */ y) =>
    toScreen(view, width, height, x, y)
  const projectLonLat = (/** @type {[number, number]} */ p) => {
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

    drawShape(c)
  }

  /** @param {CanvasRenderingContext2D} c */
  function drawWorld(c) {
    if (!world) return
    const b = viewBounds(view, width, height)
    // Land is filled, not outlined: an outline-only basemap vanishes the moment
    // data covers it, and a continent you can't recognise gives a point nothing
    // to mean. The tone comes from the foreground so it holds in either polarity.
    c.save()
    c.strokeStyle = `rgba(${palette.ink},0.2)`
    c.fillStyle = `rgba(${palette.ink},0.09)`
    c.lineWidth = 1
    c.lineJoin = 'round'
    for (const feature of world.features || []) {
      const box = feature.__bbox
      if (box && (box[2] < b.minLon || box[0] > b.maxLon || box[3] < b.minLat || box[1] > b.maxLat)) continue
      c.beginPath()
      walkGeometry(feature.geometry, project, {
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

  /** @param {CanvasRenderingContext2D} c */
  function drawShape(c) {
    const accent = palette.accent
    c.save()
    c.lineJoin = 'round'
    c.lineCap = 'round'

    for (const rings of shape.polygons) {
      c.beginPath()
      for (const ring of rings) {
        ring.forEach((p, i) => {
          const s = projectLonLat(p)
          if (i === 0) c.moveTo(s.px, s.py)
          else c.lineTo(s.px, s.py)
        })
        c.closePath()
      }
      c.fillStyle = `rgba(${accent},0.22)`
      c.strokeStyle = `rgba(${accent},0.9)`
      c.lineWidth = 1.5
      c.fill('evenodd')
      c.stroke()
    }

    c.strokeStyle = `rgba(${accent},0.95)`
    c.lineWidth = 2
    for (const line of shape.lines) {
      c.beginPath()
      line.forEach((p, i) => {
        const s = projectLonLat(p)
        if (i === 0) c.moveTo(s.px, s.py)
        else c.lineTo(s.px, s.py)
      })
      c.stroke()
    }

    // A marker, not a dot: a 4px circle on satellite imagery is a speck. The
    // halo is what makes it findable over any basemap.
    for (const p of shape.points) {
      const s = projectLonLat(p)
      c.beginPath()
      c.arc(s.px, s.py, 11, 0, Math.PI * 2)
      c.fillStyle = `rgba(${accent},0.18)`
      c.fill()
      c.beginPath()
      c.arc(s.px, s.py, 5, 0, Math.PI * 2)
      c.fillStyle = `rgb(${accent})`
      c.fill()
      c.lineWidth = 2
      c.strokeStyle = `rgba(${palette.bg},0.85)`
      c.stroke()
    }
    c.restore()
  }

  // ── Interaction ─────────────────────────────────────────────────────────────
  /** @type {{ x: number, y: number }|null} */
  let dragFrom = null
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
    const w = fromScreen(view, width, height, e.clientX - rect.left, e.clientY - rect.top)
    cursor = toLonLat(Math.min(1, Math.max(0, w.x)), Math.min(1, Math.max(0, w.y)))
    if (!dragFrom) return
    const dx = e.clientX - dragFrom.x
    const dy = e.clientY - dragFrom.y
    if (Math.abs(dx) > 2 || Math.abs(dy) > 2) dragged = true
    dragFrom = { x: e.clientX, y: e.clientY }
    view = panBy(view, dx, dy, width, height)
    requestDraw()
  }

  /** @param {PointerEvent} e */
  function onPointerUp(e) {
    dragFrom = null
    try { /** @type {HTMLCanvasElement} */ (e.currentTarget).releasePointerCapture(e.pointerId) } catch { /* already released */ }
  }

  /** @param {WheelEvent} e */
  function onWheel(e) {
    // The dialog scrolls; the map zooms. Claiming the event is the whole point
    // of a zoomable map inside a scrollable panel.
    e.preventDefault()
    const rect = /** @type {HTMLCanvasElement} */ (e.currentTarget).getBoundingClientRect()
    view = zoomAbout(view, width, height, Math.pow(0.999, e.deltaY), e.clientX - rect.left, e.clientY - rect.top)
    requestDraw()
  }

  /** @param {number} factor */
  function zoomBy(factor) {
    view = zoomAbout(view, width, height, factor, width / 2, height / 2)
    requestDraw()
  }

  const btn =
    'inline-flex size-6 items-center justify-center rounded-md border border-border/50 bg-background text-muted-foreground transition-colors hover:bg-muted hover:text-foreground'
</script>

<div bind:this={host} class={cn('relative overflow-hidden rounded-md border border-border/40', extraClass)} style="height:{height}px">
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <canvas
    bind:this={canvas}
    role="img"
    aria-label="Map showing the geometry"
    class={cn('block touch-none select-none', dragFrom ? 'cursor-grabbing' : 'cursor-grab')}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerUp}
    onpointerleave={() => (cursor = null)}
    onwheel={onWheel}
    ondblclick={() => zoomBy(2)}
  ></canvas>

  <!-- Basemap picker. Segmented rather than a dropdown: four options, and the
       cost of the choice (offline vs a third-party host) should be visible
       rather than hidden one click deep. -->
  <div class="absolute left-2 top-2 flex items-center gap-0.5 rounded-md border border-border/50 bg-background p-0.5">
    {#each BASEMAPS as b (b.id)}
      <button
        type="button"
        onclick={() => { basemapId = b.id; tiles.reset(); requestDraw() }}
        title={b.offline ? 'Offline outline - no network requests' : `Tiles from ${b.attribution}`}
        class={cn(
          'inline-flex h-5 items-center rounded-md px-1.5 text-ui-3xs transition-colors',
          basemapId === b.id ? 'bg-muted text-foreground' : 'text-muted-foreground hover:text-foreground',
        )}
      >{b.label}</button>
    {/each}
  </div>

  <div class="absolute right-2 top-2 flex flex-col gap-1">
    <button type="button" class={btn} onclick={() => zoomBy(2)} title="Zoom in" aria-label="Zoom in">
      <Icon name="plus" class="size-3 shrink-0" />
    </button>
    <button type="button" class={btn} onclick={() => zoomBy(0.5)} title="Zoom out" aria-label="Zoom out">
      <Icon name="minus" class="size-3 shrink-0" />
    </button>
    <button type="button" class={btn} onclick={frameShape} title="Frame the geometry" aria-label="Frame the geometry">
      <Icon name="crosshair" class="size-3 shrink-0" />
    </button>
    <button
      type="button"
      class={btn}
      onclick={() => { view = worldView(width, height); requestDraw() }}
      title="Whole world"
      aria-label="Whole world"
    >
      <Icon name="globe" class="size-3 shrink-0" />
    </button>
  </div>

  <!-- Cursor position, and the attribution the tile providers require. -->
  <div class="pointer-events-none absolute inset-x-2 bottom-2 flex items-end justify-between gap-2">
    <span class="rounded bg-background px-1.5 py-0.5 font-mono text-ui-3xs tabular-nums text-muted-foreground">
      {cursor ? formatLonLat(cursor.lon, cursor.lat) : 'drag to pan · scroll to zoom'}
    </span>
    {#if !basemap.offline}
      <span class="truncate rounded bg-background px-1.5 py-0.5 text-ui-3xs text-muted-foreground">
        {basemap.attribution}
      </span>
    {/if}
  </div>
</div>
