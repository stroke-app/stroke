<script>
  import { onDestroy, onMount, tick, untrack } from 'svelte'
  import { listTables, getTableRowCounts } from '$lib/api.js'
  import { formatTableRowCount, normalizeTableRowCount } from '$lib/table-list.js'
  import { hubTables } from '$lib/erd-filter.js'
  import { adjacency, layoutRoadmap, lineage, roadmapEdges, roundedPath, LOOSE_LABEL } from '$lib/erd-roadmap.js'
  import { cn } from '$lib/utils.js'
  import Loader from '@lucide/svelte/icons/loader'

  /**
   * @typedef {{ name: string, columns: { name: string }[] }} TableMeta
   * @typedef {{ source: string, target: string, sourceCol?: string | null }} Rel
   */

  let {
    /** The tables on the page. @type {TableMeta[]} */
    tables = [],
    /** Every relationship on the page, hub links included. @type {Rel[]} */
    rels = [],
    schema = 'public',
    /** Leave out the lines into hub tables (`tenants` from nearly every table). */
    hideHubs = true,
    /** Tables whose name holds this stay lit; the rest dim. */
    query = '',
    onopen = /** @type {(name: string) => void} */ (() => {}),
    /** One line about what is on screen, for the host's bar. */
    summary = $bindable(''),
    /** How many hubs the page has; the host shows the hub switch only when there are some. */
    hubCount = $bindable(0),
  } = $props()

  // ── Graph ─────────────────────────────────────────────────────────────────
  const ids = $derived(new Set(tables.map((t) => t.name)))
  const pageRels = $derived(rels.filter((r) => ids.has(r.source) && ids.has(r.target)))
  const hubs = $derived(hubTables(pageRels, tables.length))
  $effect(() => { hubCount = hubs.size })
  const allEdges = $derived(roadmapEdges(pageRels, ids))
  const edges = $derived(hideHubs && hubs.size ? allEdges.filter((e) => !hubs.has(e.from)) : allEdges)
  const adj = $derived(adjacency(pageRels))
  /** Tables pointing at each hub, for the pill on its card. */
  const hubRefs = $derived(new Map([...hubs].map((h) => [h, adj.down.get(h)?.size ?? 0])))
  const colCount = $derived(new Map(tables.map((t) => [t.name, t.columns?.length ?? 0])))

  // ── Row counts ────────────────────────────────────────────────────────────
  // The listing's numbers first, as the sidebar does: an estimate for a table
  // of 100k rows or more, "unknown" below that, where an exact COUNT(*) is
  // cheap. The unknown ones are then counted in chunks, so small tables get a
  // bar too; big tables are never counted here.
  /** @type {Map<string, number>} */
  let rowCounts = $state(new Map())
  let countsFor = ''
  $effect(() => {
    const key = schema
    untrack(() => void loadCounts(key))
  })
  /** @param {string} key */
  async function loadCounts(key) {
    if (key === countsFor) return
    countsFor = key
    try {
      const list = /** @type {{ name: string, kind?: string, rowCount?: number | null }[]} */ (await listTables(key))
      if (key !== countsFor) return
      /** @type {Map<string, number>} */
      const next = new Map()
      /** @type {string[]} */
      const unknown = []
      for (const t of list ?? []) {
        const n = normalizeTableRowCount(t.rowCount)
        if (n !== null) next.set(t.name, n)
        else if (!t.kind || t.kind === 'table') unknown.push(t.name)
      }
      rowCounts = next
      for (let i = 0; i < unknown.length; i += 12) {
        const counts = /** @type {{ name: string, rowCount: number }[]} */ (await getTableRowCounts(key, unknown.slice(i, i + 12)).catch(() => []))
        if (key !== countsFor) return
        if (!counts?.length) continue
        const more = new Map(rowCounts)
        for (const c of counts) {
          const n = normalizeTableRowCount(c.rowCount)
          if (n !== null) more.set(c.name, n)
        }
        rowCounts = more
      }
    } catch { /* counts are decoration; the drawing stands without them */ }
  }
  const maxRows = $derived(Math.max(0, ...tables.map((t) => rowCounts.get(t.name) ?? 0)))
  /** Bar length: log scale, so a 10-row table still shows next to a 10M-row one. @param {number} n */
  const barPct = (n) => (n <= 0 || maxRows <= 0 ? 0 : Math.max(4, (Math.log10(n + 1) / Math.log10(maxRows + 1)) * 100))

  // ── Layout ────────────────────────────────────────────────────────────────
  /**
   * @typedef {{ pos: Map<string, {x:number,y:number}>, routes: Map<string, {x:number,y:number}[]>,
   *   loose: string[], looseY: number, width: number, height: number,
   *   sizes: Map<string, {w:number,h:number}>, edges: { id: string, from: string, to: string, cols: string[], d: string }[] }} Layout
   */
  /** @type {Layout | null} */
  let layout = $state.raw(null)
  let laying = $state(false)
  let layoutError = $state('')
  let seq = 0
  /** Re-layout only when the drawing would change, not on every new array from the host. */
  const sig = $derived(`${tables.map((t) => t.name).join('\u0000')}\u0001${edges.map((e) => `${e.from}>${e.to}`).join('|')}`)
  $effect(() => {
    void sig
    untrack(() => void relayout())
  })

  /** @type {CanvasRenderingContext2D | null} */
  let measureCtx = null
  /**
   * Card sizes in px from the live type scale: the root size follows the
   * platform and the zoom setting, so nothing here is a fixed pixel count.
   */
  function measure() {
    const root = getComputedStyle(document.documentElement)
    const rem = parseFloat(root.fontSize) || 16
    const fs = root.getPropertyValue('--fs-xs').trim() || `${Math.round(rem * 0.78)}px`
    const family = root.getPropertyValue('--font-mono').trim() || 'monospace'
    measureCtx ??= document.createElement('canvas').getContext('2d')
    if (measureCtx) measureCtx.font = `500 ${fs} ${family}`
    const pad = rem * 0.75 // px-3 on each side
    const minW = rem * 7.5, maxW = rem * 14
    const h = Math.round(rem * 2.75)
    return {
      h,
      w: (/** @type {string} */ name) => {
        const text = measureCtx ? measureCtx.measureText(name).width : name.length * rem * 0.5
        return Math.round(Math.min(maxW, Math.max(minW, text + pad * 2 + 2)))
      },
    }
  }

  async function relayout() {
    const my = ++seq
    const snapshot = edges
    if (!tables.length) {
      layout = null
      laying = false
      return
    }
    const m = measure()
    const cards = tables.map((t) => ({ id: t.name, w: m.w(t.name), h: m.h }))
    laying = true
    layoutError = ''
    try {
      const aspect = host && host.clientHeight ? Math.max(0.8, Math.min(3, host.clientWidth / host.clientHeight)) : 1.6
      const out = await layoutRoadmap(cards, snapshot, { aspect })
      if (my !== seq) return
      layout = {
        ...out,
        sizes: new Map(cards.map((c) => [c.id, { w: c.w, h: c.h }])),
        edges: snapshot.map((e) => ({ ...e, d: roundedPath(out.routes.get(e.id) ?? [], 12) })),
      }
      await tick()
      firstView()
    } catch (e) {
      if (my === seq) layoutError = String(/** @type {any} */ (e)?.message ?? e)
    } finally {
      if (my === seq) laying = false
    }
  }

  // ── Camera ────────────────────────────────────────────────────────────────
  /** @type {HTMLDivElement | null} */
  let host = $state(null)
  let cam = $state({ x: 0, y: 0, k: 1 })
  const PAD = 32
  const MAX_K = 2.5
  /** Below this a fitted drawing is too small to read; the first view starts at the top instead. */
  const READABLE_K = 0.45
  let anim = 0
  /** Set while the view has no size (a hidden tab); the first resize fits. */
  let pendingFit = false

  const reducedMotion = () => typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches

  /** @param {{x:number,y:number,k:number}} to @param {number} ms */
  function flyTo(to, ms) {
    cancelAnimationFrame(anim)
    if (!ms || reducedMotion()) { cam = to; return }
    const from = { ...cam }
    const t0 = performance.now()
    const step = (/** @type {number} */ now) => {
      const t = Math.min(1, (now - t0) / ms)
      const e = 1 - Math.pow(1 - t, 3)
      cam = { x: from.x + (to.x - from.x) * e, y: from.y + (to.y - from.y) * e, k: from.k + (to.k - from.k) * e }
      if (t < 1) anim = requestAnimationFrame(step)
    }
    anim = requestAnimationFrame(step)
  }

  function fitK() {
    if (!layout || !host) return 1
    return Math.min(1, (host.clientWidth - PAD * 2) / Math.max(1, layout.width), (host.clientHeight - PAD * 2) / Math.max(1, layout.height))
  }
  const minK = () => Math.min(0.2, fitK())

  /** The whole drawing in view. @param {number} [ms] */
  function fit(ms = 260) {
    if (!layout || !host) return
    if (!host.clientWidth || !host.clientHeight) { pendingFit = true; return }
    const k = Math.max(0.05, fitK())
    flyTo({ k, x: (host.clientWidth - layout.width * k) / 2, y: Math.max(PAD, (host.clientHeight - layout.height * k) / 2) }, ms)
  }

  /**
   * Opening view. A drawing that fits at a readable size is fitted. A bigger
   * one opens on its top row at a size the names can be read at, since the
   * top is where a roadmap starts; Fit still shows the whole of it.
   */
  function firstView() {
    if (!layout || !host) return
    if (!host.clientWidth || !host.clientHeight) { pendingFit = true; return }
    if (fitK() >= READABLE_K) return fit(0)
    let minY = Infinity, x0 = Infinity, x1 = -Infinity
    for (const [id, p] of layout.pos) if (!layout.loose.includes(id)) minY = Math.min(minY, p.y)
    for (const [id, p] of layout.pos) {
      if (layout.loose.includes(id) || p.y !== minY) continue
      x0 = Math.min(x0, p.x)
      x1 = Math.max(x1, p.x + (layout.sizes.get(id)?.w ?? 0))
    }
    if (!Number.isFinite(x0)) return fit(0)
    const k = 0.75
    flyTo({ k, x: host.clientWidth / 2 - ((x0 + x1) / 2) * k, y: PAD - (Number.isFinite(minY) ? minY : 0) * k }, 0)
  }

  /** @param {number} factor @param {number} cx @param {number} cy @param {number} [ms] */
  function zoomAt(factor, cx, cy, ms = 0) {
    const k = Math.max(minK(), Math.min(MAX_K, cam.k * factor))
    const wx = (cx - cam.x) / cam.k, wy = (cy - cam.y) / cam.k
    flyTo({ k, x: cx - wx * k, y: cy - wy * k }, ms)
  }

  /** Zoom from the host's bar. @param {'in' | 'out' | 'fit'} how */
  export function zoom(how) {
    if (!host) return
    if (how === 'fit') return fit()
    zoomAt(how === 'in' ? 1.25 : 0.8, host.clientWidth / 2, host.clientHeight / 2, 160)
  }

  /** @param {string} id */
  function centerOn(id) {
    if (!layout || !host) return
    const p = layout.pos.get(id), s = layout.sizes.get(id)
    if (!p || !s) return
    const k = Math.max(cam.k, 0.8)
    flyTo({ k, x: host.clientWidth / 2 - (p.x + s.w / 2) * k, y: host.clientHeight / 2 - (p.y + s.h / 2) * k }, 320)
  }

  // ── Search ────────────────────────────────────────────────────────────────
  const q = $derived(query.trim().toLowerCase())
  /** Matches in reading order: row by row, left to right. */
  const matches = $derived.by(() => {
    if (!q || !layout) return /** @type {string[]} */ ([])
    const L = layout
    return tables
      .map((t) => t.name)
      .filter((n) => n.toLowerCase().includes(q) && L.pos.has(n))
      .sort((a, b) => {
        const pa = /** @type {{x:number,y:number}} */ (L.pos.get(a)), pb = /** @type {{x:number,y:number}} */ (L.pos.get(b))
        return pa.y - pb.y || pa.x - pb.x
      })
  })
  const matchSet = $derived(q ? new Set(matches) : null)
  let matchAt = -1
  $effect(() => { void q; matchAt = -1 })
  /** Select and centre the next match. Returns false when nothing matches. */
  export function next() {
    if (!matches.length) return false
    matchAt = (matchAt + 1) % matches.length
    selected = matches[matchAt]
    centerOn(selected)
    return true
  }

  // ── Lineage highlight ─────────────────────────────────────────────────────
  /** @type {string | null} */
  let hovered = $state(null)
  /** @type {string | null} */
  let selected = $state(null)
  $effect(() => { if (selected && !ids.has(selected)) selected = null })
  const active = $derived(hovered ?? selected)
  const lit = $derived(active ? lineage(active, adj) : null)
  /** @param {string} id */
  const dimCard = (id) => (lit ? !lit.has(id) : matchSet ? !matchSet.has(id) : false)
  /** @param {{from:string,to:string}} e */
  const litEdge = (e) => !!lit && lit.has(e.from) && lit.has(e.to)
  /** @param {{from:string,to:string}} e */
  const dimEdge = (e) => (lit ? !litEdge(e) : matchSet ? !(matchSet.has(e.from) && matchSet.has(e.to)) : false)
  const edgesBack = $derived(layout ? layout.edges.filter((e) => !litEdge(e)) : [])
  const edgesFront = $derived(layout && lit ? layout.edges.filter(litEdge) : [])

  // ── Summary ───────────────────────────────────────────────────────────────
  const hiddenHubLinks = $derived(allEdges.length - edges.length)
  const summaryText = $derived.by(() => {
    if (!tables.length) return ''
    const parts = [`${tables.length} table${tables.length === 1 ? '' : 's'}`, `${edges.length} link${edges.length === 1 ? '' : 's'}`]
    if (hiddenHubLinks) parts.push(`${hiddenHubLinks} hub link${hiddenHubLinks === 1 ? '' : 's'} hidden`)
    if (q) parts.push(`${matches.length} match${matches.length === 1 ? '' : 'es'}`)
    return parts.join(' · ')
  })
  $effect(() => { summary = summaryText })

  /** Heading over the grid of tables without lines. */
  const looseLabel = $derived.by(() => {
    if (!layout?.loose.length) return ''
    const n = layout.loose.length
    const viaHub = hideHubs && layout.loose.some((id) => (adj.up.get(id)?.size ?? 0) + (adj.down.get(id)?.size ?? 0) > 0)
    return `${viaHub ? 'Linked only to a hub, or not at all' : 'No foreign keys'} · ${n}`
  })

  /** @param {string} id */
  function cardTitle(id) {
    const n = rowCounts.get(id)
    const up = [...(adj.up.get(id) ?? [])]
    const down = adj.down.get(id)?.size ?? 0
    const lines = [id]
    if (n !== undefined) lines.push(`about ${n.toLocaleString()} row${n === 1 ? '' : 's'}`)
    lines.push(`${colCount.get(id) ?? 0} columns`)
    if (up.length) lines.push(`points at ${up.join(', ')}`)
    if (down) lines.push(`${down} table${down === 1 ? '' : 's'} point at it`)
    lines.push('Double-click or Enter to open')
    return lines.join('\n')
  }

  // ── Pointer ───────────────────────────────────────────────────────────────
  /** @type {{ id: number, sx: number, sy: number, x: number, y: number, moved: boolean } | null} */
  let pan = null
  let panning = $state(false)

  /** @param {PointerEvent} e */
  function onPointerDown(e) {
    if (e.button !== 0) return
    pan = { id: e.pointerId, sx: e.clientX, sy: e.clientY, x: cam.x, y: cam.y, moved: false }
  }
  /** @param {PointerEvent} e */
  function onPointerMove(e) {
    if (!pan || e.pointerId !== pan.id || !host) return
    const dx = e.clientX - pan.sx, dy = e.clientY - pan.sy
    if (!pan.moved) {
      if (Math.abs(dx) + Math.abs(dy) < 4) return
      // Captured only once it is a drag: captured from the press, the click
      // that follows would land on this element and never reach the card.
      pan.moved = true
      panning = true
      try { host.setPointerCapture(e.pointerId) } catch { /* the pointer is already gone */ }
      cancelAnimationFrame(anim)
    }
    cam = { ...cam, x: pan.x + dx, y: pan.y + dy }
  }
  /** @param {PointerEvent} e */
  function onPointerUp(e) {
    if (!pan || e.pointerId !== pan.id) return
    const moved = pan.moved
    if (host?.hasPointerCapture(e.pointerId)) host.releasePointerCapture(e.pointerId)
    pan = null
    panning = false
    // A click on the empty page lets go of the selection.
    if (!moved && !(e.target instanceof Element && e.target.closest('[data-card]'))) selected = null
  }
  /**
   * Ctrl/Cmd + wheel zooms at the cursor (a trackpad pinch arrives the same
   * way); a plain wheel scrolls, Shift turns it sideways. Same as the Diagram.
   * @param {WheelEvent} e
   */
  function onWheel(e) {
    e.preventDefault()
    if (!host) return
    cancelAnimationFrame(anim)
    const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? host.clientHeight : 1
    const dx = e.deltaX * unit, dy = e.deltaY * unit
    if (e.ctrlKey || e.metaKey) {
      const r = host.getBoundingClientRect()
      zoomAt(Math.exp(-dy * 0.0015), e.clientX - r.left, e.clientY - r.top)
      return
    }
    cam = e.shiftKey && dx === 0 ? { ...cam, x: cam.x - dy } : { ...cam, x: cam.x - dx, y: cam.y - dy }
  }
  /** @param {KeyboardEvent} e */
  function onKeyDown(e) {
    if (e.key === 'Escape' && selected) {
      selected = null
      e.stopPropagation()
    }
  }

  /** @type {ResizeObserver | null} */
  let ro = null
  onMount(() => {
    ro = new ResizeObserver(() => {
      if (pendingFit && host?.clientWidth && host.clientHeight) {
        pendingFit = false
        firstView()
      }
    })
    if (host) ro.observe(host)
  })
  onDestroy(() => {
    ro?.disconnect()
    cancelAnimationFrame(anim)
  })

  const uid = Math.random().toString(36).slice(2, 8)
</script>

<!-- The page pans and zooms under the pointer; the cards are the controls. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  bind:this={host}
  class={cn('relative h-full w-full touch-none select-none overflow-hidden bg-background', panning ? 'cursor-grabbing' : 'cursor-grab')}
  role="group"
  aria-label="Tables in foreign-key order"
  data-zoom-surface
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  onwheel={onWheel}
  onkeydown={onKeyDown}
>
  {#if layout}
    {@const L = layout}
    <div
      class="absolute left-0 top-0 origin-top-left"
      style="width: {L.width}px; height: {L.height}px; transform: translate({cam.x}px, {cam.y}px) scale({cam.k})"
    >
      <svg class="pointer-events-none absolute left-0 top-0 overflow-visible" width={L.width} height={L.height} aria-hidden="true">
        <defs>
          <marker id="rm-arrow-{uid}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" markerUnits="userSpaceOnUse" orient="auto">
            <path d="M0,1 L9,5 L0,9 z" style="fill: var(--muted-foreground)" />
          </marker>
          <marker id="rm-arrow-lit-{uid}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="9" markerHeight="9" markerUnits="userSpaceOnUse" orient="auto">
            <path d="M0,1 L9,5 L0,9 z" style="fill: var(--foreground)" />
          </marker>
        </defs>
        {#each edgesBack as e (e.id)}
          <path
            d={e.d}
            fill="none"
            stroke-width="1.5"
            stroke-linejoin="round"
            marker-end="url(#rm-arrow-{uid})"
            style="stroke: var(--muted-foreground); opacity: {dimEdge(e) ? 0.16 : 0.85}; transition: opacity 150ms"
          />
        {/each}
        {#each edgesFront as e (e.id)}
          <path
            d={e.d}
            fill="none"
            stroke-width="2"
            stroke-linejoin="round"
            marker-end="url(#rm-arrow-lit-{uid})"
            style="stroke: var(--foreground)"
          />
        {/each}
      </svg>

      {#if looseLabel}
        <p
          class="pointer-events-none absolute left-0 flex items-end justify-center text-ui-xs font-medium text-muted-foreground"
          style="top: {L.looseY}px; width: {L.width}px; height: {LOOSE_LABEL - 28}px"
        >{looseLabel}</p>
      {/if}

      {#each tables as t (t.name)}
        {@const p = L.pos.get(t.name)}
        {@const s = L.sizes.get(t.name)}
        {#if p && s}
          {@const n = rowCounts.get(t.name)}
          {@const isSel = selected === t.name}
          {@const refs = hideHubs ? hubRefs.get(t.name) : undefined}
          <button
            type="button"
            data-card
            class={cn(
              'absolute flex cursor-pointer flex-col justify-center gap-1.5 rounded-md px-3 text-left elevate-1 transition-[opacity,background-color,color] duration-150 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring',
              isSel ? 'bg-info text-info-foreground' : 'bg-primary text-primary-foreground',
              dimCard(t.name) && 'opacity-25',
            )}
            style="left: {p.x}px; top: {p.y}px; width: {s.w}px; height: {s.h}px"
            title={cardTitle(t.name)}
            aria-label="{t.name}{n !== undefined ? `, about ${n} rows` : ''}"
            aria-pressed={isSel}
            onclick={() => (selected = t.name)}
            ondblclick={() => onopen(t.name)}
            onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); onopen(t.name) } }}
            onpointerenter={() => { if (!panning) hovered = t.name }}
            onpointerleave={() => { if (hovered === t.name) hovered = null }}
            onfocus={() => (hovered = t.name)}
            onblur={() => { if (hovered === t.name) hovered = null }}
          >
            <span class="block truncate text-center font-mono text-ui-xs font-medium leading-tight">{t.name}</span>
            {#if n !== undefined}
              <span class="flex items-center gap-2">
                <span class="h-1 min-w-0 flex-1 overflow-hidden rounded-full bg-current/25">
                  <span class="block h-full rounded-full bg-current" style="width: {barPct(n)}%"></span>
                </span>
                <span class="shrink-0 font-mono text-ui-2xs leading-none tabular-nums">{formatTableRowCount(n)}</span>
              </span>
            {:else}
              <span class="block truncate text-center font-mono text-ui-2xs leading-none">{colCount.get(t.name) ?? 0} columns</span>
            {/if}
            {#if refs}
              <span
                class="pointer-events-none absolute bottom-full right-0 mb-1 rounded-full border border-border bg-background px-1.5 font-mono text-ui-2xs leading-4 tabular-nums text-muted-foreground"
                title="{refs} tables point at {t.name}; their lines are hidden"
              >← {refs}</span>
            {/if}
          </button>
        {/if}
      {/each}
    </div>
  {/if}

  {#if !tables.length}
    <div class="absolute inset-0 flex items-center justify-center">
      <p class="text-ui-sm text-muted-foreground">No tables on the page</p>
    </div>
  {:else if layoutError}
    <div class="absolute inset-0 flex items-center justify-center p-6">
      <p class="max-w-md text-center font-mono text-ui-xs text-destructive">{layoutError}</p>
    </div>
  {:else if laying && !layout}
    <div class="absolute inset-0 flex items-center justify-center gap-2">
      <Loader class="size-3.5 shrink-0 animate-spin text-muted-foreground" />
      <span class="text-ui-xs text-muted-foreground">Laying out {tables.length} tables…</span>
    </div>
  {:else if laying}
    <div class="pointer-events-none absolute left-4 top-4 flex h-6 items-center gap-2 rounded-full border border-border/50 bg-panel/85 px-2.5 text-ui-2xs text-muted-foreground backdrop-blur-sm">
      <span class="size-1.5 animate-pulse rounded-full bg-primary"></span>
      Laying out…
    </div>
  {/if}
</div>
