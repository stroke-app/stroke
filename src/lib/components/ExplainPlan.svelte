<script>
  import { untrack } from 'svelte'
  import ZoomIn from '@lucide/svelte/icons/zoom-in'
  import ZoomOut from '@lucide/svelte/icons/zoom-out'
  import Maximize2 from '@lucide/svelte/icons/maximize-2'

  /**
   * @typedef {{
   *   plan: object,
   *   planningTime: number,
   *   executionTime: number,
   *   driver: string,
   * }} ExplainResult
   */

  /** @type {{ result: ExplainResult }} */
  let { result } = $props()

  const isSqlite = $derived(result?.driver === 'sqlite')

  // NODE_H is used only as a midpoint estimate for arrow anchors - cards auto-size.
  // Derived (not a plain const) so it tracks the driver instead of freezing on the
  // initial value of `isSqlite`.
  const NODE_W = 216
  const NODE_H = $derived(isSqlite ? 48 : 136)
  const COL_W = NODE_W + 64
  const ROW_H = $derived(NODE_H + 40)
  const PAD = 32
  const QR_W = 110
  const QR_H = 40

  let scale = $state(1)
  let panX = $state(0)
  let panY = $state(0)
  let isPanning = $state(false)
  let panStart = $state({ x: 0, y: 0, px: 0, py: 0 })

  let containerW = $state(0)
  let containerH = $state(0)

  // Layout: col=0 = leaf (leftmost), col=N = root (rightmost)
  function buildLayout(node) {
    const ctr = { v: 0 }
    return _lay(node, ctr)
  }

  function _lay(node, ctr) {
    const plans = node['Plans'] ?? []
    if (!plans.length) {
      const row = ctr.v++
      return { node, col: 0, row, ch: [], leafRows: [row] }
    }
    const ch = plans.map(p => _lay(p, ctr))
    const col = Math.max(...ch.map(c => c.col)) + 1
    const leafRows = ch.flatMap(c => c.leafRows)
    const row = (Math.min(...leafRows) + Math.max(...leafRows)) / 2
    return { node, col, row, ch, leafRows }
  }

  const root = $derived(result?.plan ? buildLayout(result.plan) : null)

  const flat = $derived.by(() => {
    if (!root) return { nodes: [], edges: [] }
    const nodes = [], edges = []
    function walk(l) {
      nodes.push(l)
      for (const c of l.ch) { edges.push({ from: c, to: l }); walk(c) }
    }
    walk(root)
    return { nodes, edges }
  })

  const maxCol = $derived(flat.nodes.length ? Math.max(...flat.nodes.map(n => n.col)) : 0)
  const maxRow = $derived(flat.nodes.length ? Math.max(...flat.nodes.map(n => n.row)) : 0)

  function nx(n) { return PAD + n.col * COL_W }
  function ny(n) { return PAD + n.row * ROW_H }
  function nrx(n) { return nx(n) + NODE_W }
  function ncy(n) { return ny(n) + NODE_H / 2 }

  const qrX = $derived(root ? PAD + (maxCol + 1) * COL_W : 0)
  const qrY = $derived(root ? ncy(root) - QR_H / 2 : 0)
  const qrCY = $derived(qrY + QR_H / 2)

  const canvasW = $derived(qrX + QR_W + PAD)
  const canvasH = $derived(PAD + (maxRow + 1) * ROW_H + PAD)

  function arrowD(from, to) {
    const x1 = nrx(from), y1 = ncy(from)
    const x2 = nx(to), y2 = ncy(to)
    const mx = (x1 + x2) / 2
    return `M ${x1} ${y1} C ${mx} ${y1} ${mx} ${y2} ${x2} ${y2}`
  }

  function rootArrowD() {
    if (!root) return ''
    return `M ${nrx(root)} ${ncy(root)} H ${qrX}`
  }

  function fitView() {
    const cw = containerW || 600
    const ch = containerH || 300
    const fitS = Math.min(1, cw / canvasW * 0.9, ch / canvasH * 0.9)
    scale = fitS
    panX = (cw - canvasW * fitS) / 2
    panY = (ch - canvasH * fitS) / 2
  }

  function zoomIn() { scale = Math.min(2, +(scale + 0.15).toFixed(2)) }
  function zoomOut() { scale = Math.max(0.25, +(scale - 0.15).toFixed(2)) }

  // Auto-fit when a new plan loads and we have container dimensions
  let lastFitPlan = null
  $effect(() => {
    const plan = result?.plan
    const cw = containerW, ch = containerH
    if (!plan || !cw || !ch) return
    if (plan === lastFitPlan) return
    lastFitPlan = plan
    untrack(() => fitView())
  })

  function onPD(e) {
    if (e.button !== 0 || e.target.closest?.('button')) return
    isPanning = true
    e.currentTarget.setPointerCapture(e.pointerId)
    panStart = { x: e.clientX, y: e.clientY, px: panX, py: panY }
  }
  function onPM(e) {
    if (!isPanning) return
    panX = panStart.px + (e.clientX - panStart.x)
    panY = panStart.py + (e.clientY - panStart.y)
  }
  function onPU() { isPanning = false }

  function nodeRelStr(node) {
    const rel = node['Relation Name'] ?? ''
    const alias = node['Alias'] ?? ''
    const idx = node['Index Name'] ?? ''
    if (rel) return alias && alias !== rel ? `${rel} as ${alias}` : rel
    return idx
  }

  function exclCost(node) {
    const t = node['Total Cost'] ?? 0
    const s = (node['Plans'] ?? []).reduce((a, c) => a + (c['Total Cost'] ?? 0), 0)
    return Math.max(0, t - s)
  }

  function fmtN(n) {
    if (n == null) return null
    if (typeof n !== 'number') return String(n)
    return n % 1 === 0 ? n.toLocaleString() : n.toFixed(2)
  }

  function fmtMs(n) {
    if (n == null) return null
    return typeof n === 'number' ? `${n.toFixed(3)}ms` : String(n)
  }

  /** @param {object} node */
  function hasActualData(node) {
    return node['Actual Rows'] != null || node['Actual Total Time'] != null
  }
</script>

<div
  class="relative h-full w-full overflow-hidden select-none"
  bind:clientWidth={containerW}
  bind:clientHeight={containerH}
>
  <!-- Controls -->
  <div class="absolute right-3 top-3 z-10 flex items-center gap-2">
    {#if result?.driver === 'postgres' && (result.planningTime > 0 || result.executionTime > 0)}
      <div class="flex items-center gap-3 rounded-lg border border-border/30 bg-panel px-3 py-1.5 font-mono text-ui-3xs text-muted-foreground">
        {#if result.planningTime > 0}
          <span>Plan <b class="font-semibold text-foreground/70">{result.planningTime.toFixed(2)}ms</b></span>
        {/if}
        {#if result.executionTime > 0}
          <span>Execute <b class="font-semibold text-foreground/70">{result.executionTime.toFixed(2)}ms</b></span>
        {/if}
      </div>
    {/if}
    <div class="flex items-center gap-0.5 rounded-lg border border-border/40 bg-panel p-1">
      <button
        class="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={zoomIn} title="Zoom in"
      ><ZoomIn class="size-3.5 shrink-0" /></button>
      <button
        class="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={zoomOut} title="Zoom out"
      ><ZoomOut class="size-3.5 shrink-0" /></button>
      <button
        class="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
        onclick={fitView} title="Reset view"
      ><Maximize2 class="size-3.5 shrink-0" /></button>
    </div>
    <button
      class="rounded-md border border-border/40 bg-panel px-3 py-1.5 font-mono text-ui-2xs font-semibold text-foreground/80 transition-colors hover:bg-muted/60"
      onclick={fitView}
    >Expand all</button>
  </div>

  <!-- Pannable canvas, explicit top/left so transform is relative to container origin -->
  <div
    role="img"
    aria-label="Query execution plan diagram"
    style="position: absolute; top: 0; left: 0; transform: translate({panX}px,{panY}px) scale({scale}); transform-origin: 0 0; width: {canvasW}px; height: {canvasH}px; cursor: {isPanning ? 'grabbing' : 'grab'}"
    onpointerdown={onPD}
    onpointermove={onPM}
    onpointerup={onPU}
    onpointercancel={onPU}
  >
    <!-- Dot grid background -->
    <div
      class="pointer-events-none absolute inset-0 text-border/30"
      style="background-image: radial-gradient(circle, currentColor 1.2px, transparent 1.2px); background-size: 22px 22px"
    ></div>

    <!-- SVG arrows -->
    <svg class="pointer-events-none absolute inset-0 text-border/50" width={canvasW} height={canvasH}>
      {#each flat.edges as e}
        <path d={arrowD(e.from, e.to)} fill="none" stroke="currentColor" stroke-width="1.5" stroke-dasharray="5 4" />
      {/each}
      {#if root}
        <path d={rootArrowD()} fill="none" stroke="currentColor" stroke-width="1.5" stroke-dasharray="5 4" />
      {/if}
    </svg>

    <!-- Plan nodes, auto-height (no fixed height constraint) -->
    {#each flat.nodes as n}
      {@const rel = nodeRelStr(n.node)}
      {@const type = (n.node['Node Type'] ?? 'Unknown').toString()}
      {@const excl = exclCost(n.node)}
      {@const actualRows = n.node['Actual Rows']}
      {@const planRows = n.node['Plan Rows']}
      {@const actualTime = n.node['Actual Total Time']}
      {@const totalCost = n.node['Total Cost']}
      {@const withActual = hasActualData(n.node)}
      <div
        style="position: absolute; left: {nx(n)}px; top: {ny(n)}px; width: {NODE_W}px"
        class="overflow-hidden rounded-lg border border-border/50 bg-background elevate-2-rim"
      >
        <!-- Header -->
        <div class="border-b border-border/30 px-3 py-2">
          <span class="font-mono text-ui-3xs font-semibold leading-snug text-foreground/90">
            {type}{rel ? ' on ' : ''}<span class="font-normal text-primary">{rel}</span>
          </span>
        </div>

        <!-- Stats -->
        {#if !isSqlite}
          <div class="grid grid-cols-[auto_1fr] gap-x-4 px-3 py-2.5 font-mono text-ui-3xs leading-relaxed">
            {#if withActual && actualRows != null}
              <span class="text-muted-foreground">Actual rows</span>
              <span class="text-right text-foreground/80 tabular-nums">
                {fmtN(actualRows)}{#if planRows != null && planRows !== actualRows}<span class="text-muted-foreground"> (est {fmtN(planRows)})</span>{/if}
              </span>
            {:else if planRows != null}
              <span class="text-muted-foreground">Plan rows</span>
              <span class="text-right text-foreground/80 tabular-nums">{fmtN(planRows)}</span>
            {/if}

            {#if withActual && actualTime != null}
              <span class="text-muted-foreground">Actual time</span>
              <span class="text-right text-foreground/80 tabular-nums">{fmtMs(actualTime)}</span>
            {/if}

            <span class="text-muted-foreground">Excl. cost</span>
            <span class="text-right text-foreground/80 tabular-nums">{fmtN(excl) ?? '–'}</span>

            {#if totalCost != null && (excl == null || Math.abs(excl - totalCost) > 0.001)}
              <span class="text-muted-foreground">Total cost</span>
              <span class="text-right text-foreground/80 tabular-nums">{fmtN(totalCost)}</span>
            {/if}
          </div>
        {/if}
      </div>
    {/each}

    <!-- Query Result terminal node -->
    {#if root}
      <div
        style="position: absolute; left: {qrX}px; top: {qrY}px; width: {QR_W}px; height: {QR_H}px"
        class="flex items-center justify-center rounded-lg border border-border/50 bg-background font-mono text-ui-3xs font-semibold text-foreground/50 elevate-2-rim"
      >
        Query Result
      </div>
    {/if}
  </div>
</div>
