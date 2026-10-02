<script>
  import { renderMermaidSync } from 'beautiful-mermaid'
  import { normalizeThemeId } from '$lib/themes/registry.js'
  import { cn } from '$lib/utils.js'
  import { svgToPngBlob, downloadBlob, copyPngToClipboard } from '$lib/svg-png.js'

  /**
   * @type {{ code: string, class?: string, spacing?: { nodeSpacing?: number, layerSpacing?: number, padding?: number } }}
   * `spacing` goes to the renderer's layout (px between siblings, between layers,
   * around the canvas); unset, the renderer's own defaults apply.
   */
  let { code, class: className = '', spacing = undefined } = $props()

  // ── Theme helpers ───────────────────────────────────────────────────────────

  /**
   * The diagram's colours, read off the app's own theme tokens, so the picture
   * sits on the page like any other surface in every theme. A fixed palette
   * per theme drifted from the themes it was copied from and gave themes
   * without an entry the generic grey. Read as values rather than var()
   * references, so an exported SVG or PNG keeps them away from the app.
   */
  function resolveMermaidTheme() {
    const cs = getComputedStyle(document.documentElement)
    const v = (/** @type {string} */ name) => cs.getPropertyValue(name).trim()
    /** @type {{ bg: string, fg: string, muted?: string, border?: string, line?: string, accent?: string }} */
    const theme = { bg: v('--background') || '#1c1c1c', fg: v('--foreground') || '#f0f0f0' }
    const muted = v('--muted-foreground'), border = v('--border')
    if (muted) theme.muted = muted
    if (border) theme.border = border
    return theme
  }

  /** @param {SVGSVGElement} svg @param {ReturnType<typeof resolveMermaidTheme>} theme */
  function applyMermaidThemeVars(svg, theme) {
    svg.style.setProperty('--bg', theme.bg)
    svg.style.setProperty('--fg', theme.fg)
    // Every variable the renderer reads is set here, on the SVG. Left unset,
    // `var(--muted)` and friends resolve from the page, where the same names
    // are the app's chrome fill tokens: edge labels came out in the muted
    // SURFACE colour (invisible on dark) and arrows in the accent fill.
    const mix = (/** @type {number} */ pct) => `color-mix(in srgb, ${theme.fg} ${pct}%, ${theme.bg})`
    svg.style.setProperty('--muted', theme.muted ?? mix(62))
    svg.style.setProperty('--line', theme.line ?? mix(45))
    svg.style.setProperty('--accent', theme.accent ?? mix(85))
    svg.style.setProperty('--border', theme.border ?? mix(22))
    svg.style.setProperty('--surface', mix(4))
    svg.style.background = theme.bg
    // Arrowheads in the line colour, not the accent: an orange triangle on
    // every edge was the loudest thing on the page. And smaller - the
    // renderer's 8x5 head reads as a flag once the diagram is zoomed in.
    svg.style.setProperty('--_arrow', theme.line ?? mix(45))
    for (const m of svg.querySelectorAll('marker')) {
      const w = m.getAttribute('markerWidth'), h = m.getAttribute('markerHeight')
      if (w && h && !m.getAttribute('viewBox')) m.setAttribute('viewBox', `0 0 ${w} ${h}`)
      m.setAttribute('markerWidth', '6')
      m.setAttribute('markerHeight', '3.75')
    }
    // Key badges in the key colours the cards use (ErdCanvas INK), so PK and FK
    // read the same in the picture as on the canvas instead of one grey chip.
    const dark = document.documentElement.classList.contains('dark')
    const ink = dark
      ? { PK: 'oklch(0.80 0.12 82)', FK: 'oklch(0.70 0.11 252)', UK: 'oklch(0.72 0.03 255)' }
      : { PK: 'oklch(0.60 0.13 72)', FK: 'oklch(0.52 0.14 255)', UK: 'oklch(0.55 0.03 255)' }
    for (const t of svg.querySelectorAll('text')) {
      const key = /** @type {keyof typeof ink} */ ((t.textContent ?? '').trim())
      if (!(key in ink)) continue
      t.setAttribute('fill', ink[key])
      const chip = t.previousElementSibling
      if (chip instanceof SVGRectElement) chip.setAttribute('fill', `color-mix(in srgb, ${ink[key]} 18%, transparent)`)
    }
    const bgRect = /** @type {SVGRectElement|null} */ (svg.querySelector('rect.background, rect[class*="background"]'))
    if (bgRect) bgRect.style.fill = theme.bg
  }

  // ── Rendering pipeline ──────────────────────────────────────────────────────

  function normalizeMermaidCode(c) {
    return c.replace(/^usecaseDiagram\b/m, 'flowchart TD')
  }

  /** @type {Map<string, string>} */
  const mermaidCache = new Map()
  const MERMAID_CACHE_MAX = 30
  const ASYNC_MAX = 20
  let _asyncDiagrams = $state(/** @type {Record<string,string>} */ ({}))
  let _asyncKeys = /** @type {string[]} */ ([])
  let _mermaidJsInit = false
  /** @type {typeof import('mermaid').default | null} Lazily-loaded mermaid module (kept out of startup bundle). */
  let _mermaid = null

  async function _ensureMermaidJs() {
    if (_mermaidJsInit) return _mermaid
    _mermaid = (await import('mermaid')).default
    _mermaid.initialize({ startOnLoad: false, securityLevel: 'loose' })
    _mermaidJsInit = true
    return _mermaid
  }

  async function _renderWithMermaidJs(c, asyncKey) {
    if (_asyncDiagrams[asyncKey] !== undefined) return
    _asyncDiagrams[asyncKey] = ''
    _asyncKeys.push(asyncKey)
    if (_asyncKeys.length > ASYNC_MAX) {
      const evicted = _asyncKeys.shift()
      if (evicted) { const { [evicted]: _, ...rest } = _asyncDiagrams; _asyncDiagrams = rest }
    }
    try {
      const mermaid = await _ensureMermaidJs()
      const id = `mermaid-${Math.random().toString(36).slice(2)}`
      const { svg } = await mermaid.render(id, c)
      _asyncDiagrams[asyncKey] = svg
    } catch (e) {
      const msg = String(e).replace(/</g, '&lt;').replace(/>/g, '&gt;')
      _asyncDiagrams[asyncKey] = `<div class="flex flex-col gap-1.5 p-3 rounded border border-destructive/30 bg-destructive/5 text-ui-xs"><p class="font-medium text-destructive">Render failed</p><pre class="font-mono text-ui-3xs text-muted-foreground whitespace-pre-wrap">${msg}</pre></div>`
    }
  }

  function processMermaidSvg(c) {
    const themeId = normalizeThemeId(document.documentElement.dataset.theme)
    const normalized = normalizeMermaidCode(c)
    const cacheKey = `${themeId}:${normalized}`
    if (mermaidCache.has(cacheKey)) return /** @type {string} */ (mermaidCache.get(cacheKey))
    const asyncKey = `async:${cacheKey}`
    if (_asyncDiagrams[asyncKey] !== undefined) {
      return _asyncDiagrams[asyncKey] === ''
        ? `<div class="flex items-center gap-2 p-4 text-ui-xs text-muted-foreground"><span class="size-3 animate-spin rounded-full border-2 border-border border-t-muted-foreground inline-block shrink-0"></span>Rendering…</div>`
        : _asyncDiagrams[asyncKey]
    }
    try {
      const svg = renderMermaidSync(normalized, { ...resolveMermaidTheme(), font: 'var(--font-sans)', ...(spacing ?? {}) })
      if (mermaidCache.size >= MERMAID_CACHE_MAX) mermaidCache.delete(/** @type {string} */ (mermaidCache.keys().next().value))
      mermaidCache.set(cacheKey, svg)
      return svg
    } catch { /* fall through to async */ }
    void _renderWithMermaidJs(normalized, asyncKey)
    return `<div class="flex items-center gap-2 p-4 text-ui-xs text-muted-foreground"><span class="size-3 animate-spin rounded-full border-2 border-border border-t-muted-foreground inline-block shrink-0"></span>Rendering…</div>`
  }

  // ── Pan / zoom action ───────────────────────────────────────────────────────

  // Scale is relative to the SVG's natural size. Zooming out stops where the
  // whole diagram fits (a page-wide map needs well under 0.1 for that), and
  // never goes below half size: a few tables at 0.1 are a row of specks.
  const MAX_ZOOM = 4
  const MIN_ZOOM_CAP = 0.5

  /** Svelte action applied to the container. Uses a MutationObserver to detect
   *  when the SVG is injected (handles both sync and async rendering), then
   *  attaches pan/zoom/theme listeners. Re-initialises when the SVG is replaced. */
  function mermaidInteractiveAction(node) {
    /** @type {{ destroy: () => void } | null} */
    let handle = null

    function applyBg() {
      const theme = resolveMermaidTheme()
      node.style.background = theme.bg
      const svg = /** @type {SVGSVGElement|null} */ (node.querySelector('svg'))
      if (svg) applyMermaidThemeVars(svg, theme)
    }

    function teardown() {
      handle?.destroy()
      handle = null
    }

    function setup() {
      teardown()
      const svg = /** @type {SVGSVGElement|null} */ (node.querySelector('svg'))
      if (!svg) return

      // ── Theme ──
      const theme = resolveMermaidTheme()
      applyMermaidThemeVars(svg, theme)
      node.style.background = theme.bg

      const themeObs = new MutationObserver(applyBg)
      themeObs.observe(document.documentElement, { attributes: true, attributeFilter: ['class', 'data-theme'] })

      // ── Pan / zoom ──
      // Zoom resizes the SVG itself; the transform only pans. A scale()
      // transform stretches a bitmap painted at one size, which is the blur
      // at 4x - laid out at the zoomed size, the vector paints sharp at every
      // step. One update per frame, however many wheel events land in it.
      svg.style.transformOrigin = '0 0'
      svg.style.maxWidth = 'none'
      const vb = svg.viewBox?.baseVal
      const natW = vb?.width || svg.width.baseVal.value
      const natH = vb?.height || svg.height.baseVal.value
      let scale = 1, tx = 0, ty = 0
      /** The scale the SVG's width and height were last set for. */
      let laidOut = 1
      let dragging = false, ox = 0, oy = 0
      let rafId = 0

      function minZoom() {
        if (!natW || !natH || !node.clientWidth) return MIN_ZOOM_CAP
        return Math.min(MIN_ZOOM_CAP, node.clientWidth / natW, node.clientHeight / natH)
      }
      const clampZoom = (/** @type {number} */ s) => Math.max(minZoom(), Math.min(MAX_ZOOM, s))

      function applyTransform() {
        if (natW && natH && laidOut !== scale) {
          laidOut = scale
          svg.setAttribute('width', String(natW * scale))
          svg.setAttribute('height', String(natH * scale))
        }
        // Whole pixels: a half-pixel offset softens every glyph.
        svg.style.transform = `translate(${Math.round(tx)}px,${Math.round(ty)}px)`
      }
      function schedule() {
        if (rafId) return
        rafId = requestAnimationFrame(() => { rafId = 0; applyTransform() })
      }

      const onWheel = (/** @type {WheelEvent} */ e) => {
        if (!e.ctrlKey && !e.metaKey) return
        e.preventDefault()
        const { left, top } = node.getBoundingClientRect()
        const mx = e.clientX - left, my = e.clientY - top
        const factor = e.deltaY < 0 ? 1.12 : 0.89
        const ns = clampZoom(scale * factor)
        tx = mx - (mx - tx) * (ns / scale)
        ty = my - (my - ty) * (ns / scale)
        scale = ns
        schedule()
      }

      const onMove = (/** @type {MouseEvent} */ e) => {
        if (!dragging) return
        tx = e.clientX - ox; ty = e.clientY - oy
        schedule()
      }

      const onUp = () => {
        if (!dragging) return
        dragging = false
        node.style.cursor = 'grab'
        // Back to painted in place, so the next zoom is drawn, not stretched.
        svg.style.willChange = ''
        window.removeEventListener('mousemove', onMove)
        window.removeEventListener('mouseup', onUp)
      }

      const onDown = (/** @type {MouseEvent} */ e) => {
        if (e.button !== 0) return
        dragging = true
        ox = e.clientX - tx; oy = e.clientY - ty
        node.style.cursor = 'grabbing'
        // Its own layer for the drag only: a pan then moves a painted layer
        // instead of repainting every glyph per frame.
        svg.style.willChange = 'transform'
        window.addEventListener('mousemove', onMove)
        window.addEventListener('mouseup', onUp)
      }

      const onDblClick = () => { scale = 1; tx = 0; ty = 0; applyTransform() }
      const onZoomIn = () => { scale = clampZoom(scale * 1.25); applyTransform() }
      const onZoomOut = () => { scale = clampZoom(scale / 1.25); applyTransform() }
      const onZoomReset = () => { scale = 1; tx = 0; ty = 0; applyTransform() }

      node.style.cursor = 'grab'
      node.addEventListener('wheel', onWheel, { passive: false })
      node.addEventListener('mousedown', onDown)
      node.addEventListener('dblclick', onDblClick)
      node.addEventListener('diagram:zoomin', onZoomIn)
      node.addEventListener('diagram:zoomout', onZoomOut)
      node.addEventListener('diagram:reset', onZoomReset)

      handle = {
        destroy() {
          themeObs.disconnect()
          if (rafId) cancelAnimationFrame(rafId)
          node.style.cursor = ''
          node.removeEventListener('wheel', onWheel)
          node.removeEventListener('mousedown', onDown)
          window.removeEventListener('mousemove', onMove)
          window.removeEventListener('mouseup', onUp)
          node.removeEventListener('dblclick', onDblClick)
          node.removeEventListener('diagram:zoomin', onZoomIn)
          node.removeEventListener('diagram:zoomout', onZoomOut)
          node.removeEventListener('diagram:reset', onZoomReset)
        },
      }
    }

    // Apply background immediately (before SVG arrives)
    applyBg()
    const bgObs = new MutationObserver(applyBg)
    bgObs.observe(document.documentElement, { attributes: true, attributeFilter: ['class', 'data-theme'] })

    // Detect SVG arrival / replacement via DOM mutations on the container
    setup()
    const svgObs = new MutationObserver(setup)
    svgObs.observe(node, { childList: true })

    return {
      destroy() {
        bgObs.disconnect()
        svgObs.disconnect()
        teardown()
      },
    }
  }

  // ── Reactive rendering ──────────────────────────────────────────────────────

  let svgContent = $state('')

  // The first picture is drawn at once. Every later change (a keystroke in the
  // code pane, a hop switch) waits until the input has been still for a
  // moment: a render is synchronous, and on a page of tables it is tens of
  // milliseconds that used to land on every keystroke.
  let _renderedOnce = false
  $effect(() => {
    const c = code
    void spacing
    if (!_renderedOnce) {
      _renderedOnce = true
      svgContent = processMermaidSvg(c)
      return
    }
    const t = setTimeout(() => { svgContent = processMermaidSvg(c) }, 160)
    return () => clearTimeout(t)
  })

  // ── Container ref (needed for dispatch + export) ────────────────────────────

  /** @type {HTMLDivElement | null} */
  let container = $state(null)

  // ── Public API ──────────────────────────────────────────────────────────────

  /** Dispatch a pan/zoom control event on the canvas. */
  export function dispatch(name) {
    container?.dispatchEvent(new CustomEvent(name))
  }

  /**
   * Download the current diagram as an SVG file.
   * @returns {Promise<string | null>} saved path ('' in browser dev, null if cancelled)
   */
  export async function exportSvg(filename = 'diagram.svg') {
    const svg = /** @type {SVGSVGElement|null} */ (container?.querySelector('svg'))
    if (!svg) return null
    // At its own size, without the viewer's zoom and pan.
    const clone = /** @type {SVGSVGElement} */ (svg.cloneNode(true))
    const vb = svg.viewBox?.baseVal
    if (vb?.width && vb?.height) {
      clone.setAttribute('width', String(vb.width))
      clone.setAttribute('height', String(vb.height))
    }
    for (const prop of ['transform', 'transition', 'will-change', 'transform-origin']) clone.style.removeProperty(prop)
    const xml = new XMLSerializer().serializeToString(clone)
    const blob = new Blob(['<?xml version="1.0" encoding="UTF-8"?>\n', xml], { type: 'image/svg+xml' })
    return await downloadBlob(blob, filename)
  }

  /**
   * Download the current diagram as a PNG file.
   * @returns {Promise<string | null>} saved path ('' in browser dev, null if cancelled)
   */
  export async function exportPng(filename = 'diagram.png') {
    const svg = /** @type {SVGSVGElement|null} */ (container?.querySelector('svg'))
    if (!svg) return null
    const blob = await svgToPngBlob(svg, { scale: window.devicePixelRatio || 2 })
    return await downloadBlob(blob, filename)
  }

  /**
   * Put the diagram on the clipboard as a PNG, at the same density the file
   * export uses - a diagram pasted into a doc should not be softer than one
   * saved to disk.
   * @returns {Promise<boolean>} false when there is no diagram rendered yet
   */
  export async function copyPng() {
    const svg = /** @type {SVGSVGElement|null} */ (container?.querySelector('svg'))
    if (!svg) return false
    await copyPngToClipboard(await svgToPngBlob(svg, { scale: window.devicePixelRatio || 2 }))
    return true
  }
</script>

<div
  bind:this={container}
  use:mermaidInteractiveAction
  class={cn('mermaid-canvas relative overflow-hidden', className)}
>
  {@html svgContent}
</div>

<style>
  .mermaid-canvas { user-select: none; }
  .mermaid-canvas :global(svg) { display: block; max-width: none; }
</style>
