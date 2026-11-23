<script>
  /**
   * A query plan, read for you: what the run cost, what is worth fixing, and
   * every step as a diagram or an indented tree. Pick a step for what it does,
   * its numbers and its conditions.
   */
  import { untrack } from 'svelte'
  import Icon from './Icon.svelte'
  import { cn } from '$lib/utils.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js'
  import {
    normalizePlan, annotatePlan, planInsights, describeNode, CONDITION_KEYS, conditionText, fmtMs, fmtCount, planText,
    planOutline, foldLabel,
  } from '$lib/explain-plan.js'

  /** @typedef {{ plan: object, planningTime: number, executionTime: number, driver: string }} ExplainResult */
  /** @typedef {import('$lib/explain-plan.js').PlanNode} PlanNode */
  /** @typedef {import('$lib/explain-plan.js').PlanFold} PlanFold */

  /**
   * @type {{
   *   result: ExplainResult,
   *   analyze?: boolean,
   *   onrerun?: (analyze: boolean) => void,
   *   onaskai?: (prompt: string) => void,
   *   sql?: string,
   * }}
   */
  let { result, analyze = true, onrerun, onaskai, sql = '' } = $props()

  const plan = $derived(normalizePlan(result?.plan, result?.driver ?? ''))
  // Findings write each node's warnings, so both come from one pass.
  const read = $derived.by(() => {
    const a = annotatePlan(plan)
    return { a, insights: planInsights(a) }
  })
  const a = $derived(read.a)
  const insights = $derived(read.insights)
  const byId = $derived(new Map(a.nodes.map((n) => [n.id, n])))
  const hasCost = $derived(a.nodes.some((n) => n.totalCost != null && n.totalCost > 0))
  const isPg = $derived(result?.driver === 'postgres')
  const rowsOut = $derived(a.root?.actualRows ?? null)

  /** @type {'diagram' | 'tree'} */
  let view = $state(/** @type {'diagram' | 'tree'} */ ((() => { try { return localStorage.getItem('stroke:explain-view') === 'tree' ? 'tree' : 'diagram' } catch { return 'diagram' } })()))
  $effect(() => { try { localStorage.setItem('stroke:explain-view', view) } catch { /* blocked */ } })

  let selectedId = $state(/** @type {string | null} */ (null))
  const selected = $derived(selectedId ? (byId.get(selectedId) ?? null) : null)
  let showAllInsights = $state(false)
  $effect(() => { void plan; untrack(() => { selectedId = null; showAllInsights = false }) })

  // ── Diagram layout: leaves on the left, the root on the right ──────────────
  const NODE_W = 232
  const COL_GAP = 72
  const ROW_GAP = 28
  const PAD = 40
  const QR_W = 96
  /**
   * Line heights of the card's two text sizes, measured: the type scale follows
   * the app zoom, so fixed pixel heights left the last row cut off (with the
   * share bar drawn over it) whenever the text ran larger than 14px.
   */
  let lineTitle = $state(18)
  let lineSmall = $state(16)
  /** Card height, from what the card shows. @param {PlanNode} n */
  const cardH = (n) => {
    const stats = (n.actualRows != null || n.planRows != null ? 1 : 0) + (n.selfTime != null || n.selfCost != null ? 1 : 0)
    // 10 top + title + relation + (8 gap + stats) + 10 bottom
    return Math.ceil(10 + lineTitle + lineSmall + (stats ? 8 + stats * (lineSmall + 2) : 0) + 10)
  }
  const ROW_H = $derived(Math.max(...a.nodes.map(cardH), 60) + ROW_GAP)

  // Wide steps (an Append over hundreds of partitions) draw the children that
  // ran and fold the rest into one card; `expanded` holds the ones opened.
  let expanded = $state(/** @type {Set<string>} */ (new Set()))
  $effect(() => { void plan; untrack(() => { expanded = new Set() }) })
  const outline = $derived(planOutline(a.root, expanded))
  /** @param {string} parentId */
  function toggleFold(parentId) {
    const next = new Set(expanded)
    if (next.has(parentId)) next.delete(parentId)
    else next.add(parentId)
    expanded = next
  }
  const FOLD_H = 32

  /** @typedef {{ id: string, parentId: string | null, n: PlanNode | null, fold: PlanFold | null, col: number, row: number }} Placed */
  const layout = $derived.by(() => {
    /** @type {Map<string, Placed>} */
    const at = new Map()
    let leaf = 0
    /** @param {PlanNode | PlanFold} item @returns {Placed} */
    const lay = (item) => {
      if ('kind' in item && item.kind === 'fold') {
        const p = { id: item.id, parentId: item.parentId, n: null, fold: item, col: 0, row: leaf++ }
        at.set(p.id, p)
        return p
      }
      const n = /** @type {PlanNode} */ (item)
      const kids = (outline.childrenOf.get(n.id) ?? []).map(lay)
      const p = kids.length
        ? { id: n.id, parentId: n.parentId, n, fold: null, col: Math.max(...kids.map((k) => k.col)) + 1, row: (Math.min(...kids.map((k) => k.row)) + Math.max(...kids.map((k) => k.row))) / 2 }
        : { id: n.id, parentId: n.parentId, n, fold: null, col: 0, row: leaf++ }
      at.set(n.id, p)
      return p
    }
    if (a.root) lay(a.root)
    return at
  })
  const placed = $derived([...layout.values()])
  const placedNodes = $derived(placed.filter((p) => p.n))
  const placedFolds = $derived(placed.filter((p) => p.fold))
  const maxCol = $derived(placed.reduce((m, p) => Math.max(m, p.col), 0))
  const maxRow = $derived(placed.reduce((m, p) => Math.max(m, p.row), 0))
  const px = (/** @type {Placed} */ p) => PAD + p.col * (NODE_W + COL_GAP)
  const py = (/** @type {Placed} */ p) => PAD + p.row * ROW_H
  const cy = (/** @type {Placed} */ p) => py(p) + (p.n ? cardH(p.n) : FOLD_H) / 2
  const qrX = $derived(PAD + (maxCol + 1) * (NODE_W + COL_GAP))
  const canvasW = $derived(qrX + QR_W + PAD)
  const canvasH = $derived(PAD * 2 + (maxRow + 1) * ROW_H)
  /** Edge width follows the rows passing along it, on a log scale. @param {PlanNode} n */
  const edgeW = (n) => {
    const r = n.actualRows ?? n.planRows ?? 1
    return Math.min(6, 1.25 + Math.log10(Math.max(1, r)) * 0.8)
  }
  const edges = $derived(placed.filter((p) => p.parentId).map((p) => ({ from: p, to: /** @type {Placed} */ (layout.get(/** @type {string} */ (p.parentId))) })))
  /** @param {Placed} f @param {Placed} t */
  const edgeD = (f, t) => {
    const x1 = px(f) + NODE_W, y1 = cy(f), x2 = px(t), y2 = cy(t), mx = (x1 + x2) / 2
    return `M ${x1} ${y1} C ${mx} ${y1} ${mx} ${y2} ${x2} ${y2}`
  }

  let scale = $state(1)
  let panX = $state(0)
  let panY = $state(0)
  let panning = $state(false)
  // State: the grabbing cursor reads `start.moved`, and as a plain object it
  // never switched while the plan was dragged.
  let start = $state({ x: 0, y: 0, px: 0, py: 0, moved: false })
  let cw = $state(0)
  let ch = $state(0)

  /** The smallest first view whose step text still reads. */
  const MIN_FIT = 0.75
  /** @param {boolean} [whole] the Fit button: the whole plan, however small */
  function fit(whole = false) {
    if (!cw || !ch) return
    const s = Math.min(1, (cw - 16) / canvasW, (ch - 16) / canvasH)
    // A plan wider than the pane used to shrink until it fit: four steps in
    // the results-beside pane came out at 27%, text a few pixels tall. Below
    // the floor, open readable at the leaves (the left, where the plan
    // starts) and let the pane pan; zoom out still shows the whole of it.
    scale = Math.max(whole ? 0.2 : MIN_FIT, s)
    panX = canvasW * scale <= cw ? (cw - canvasW * scale) / 2 : 8
    panY = Math.max(8, (ch - canvasH * scale) / 2)
  }
  /** @param {number} next @param {number} [ox] @param {number} [oy] */
  function zoomTo(next, ox = cw / 2, oy = ch / 2) {
    const s = Math.min(2, Math.max(0.2, next))
    panX = ox - ((ox - panX) * s) / scale
    panY = oy - ((oy - panY) * s) / scale
    scale = s
  }
  let fittedFor = /** @type {unknown} */ (null)
  $effect(() => {
    const p = plan, w = cw, h = ch, v = view
    if (!p || !w || !h || v !== 'diagram' || p === fittedFor) return
    fittedFor = p
    untrack(() => fit())
  })

  /** @param {PointerEvent} e */
  function down(e) {
    if (e.button !== 0) return
    panning = true
    start = { x: e.clientX, y: e.clientY, px: panX, py: panY, moved: false }
    ;/** @type {HTMLElement} */ (e.currentTarget).setPointerCapture(e.pointerId)
  }
  /** @param {PointerEvent} e */
  function move(e) {
    if (!panning) return
    const dx = e.clientX - start.x, dy = e.clientY - start.y
    if (!start.moved && Math.hypot(dx, dy) < 4) return
    start.moved = true
    panX = start.px + dx
    panY = start.py + dy
  }
  /** @param {PointerEvent} e */
  function up(e) {
    if (!panning) return
    panning = false
    if (start.moved) return
    // A click, not a drag: pick the card under the pointer, or clear.
    const el = /** @type {HTMLElement | null} */ (document.elementFromPoint(e.clientX, e.clientY)?.closest('[data-plan-node]') ?? null)
    selectedId = el?.dataset.planNode ?? null
  }
  /** Pinch or Mod+wheel zooms at the pointer; a plain wheel pans. @param {WheelEvent} e */
  function wheel(e) {
    e.preventDefault()
    if (e.ctrlKey || e.metaKey) {
      const r = /** @type {HTMLElement} */ (e.currentTarget).getBoundingClientRect()
      zoomTo(scale * Math.exp(-e.deltaY * 0.01), e.clientX - r.left, e.clientY - r.top)
    } else {
      panX -= e.deltaX
      panY -= e.deltaY
    }
  }

  /** Bring a node into view in the diagram. @param {string | null} id */
  function reveal(id) {
    selectedId = id
    if (!id || view !== 'diagram') return
    const p = layout.get(id)
    if (!p) return
    panX = cw / 2 - (px(p) + NODE_W / 2) * scale
    panY = ch / 2 - cy(p) * scale
  }

  // ── Words and numbers ──────────────────────────────────────────────────────
  /** Heat: how much of the run a step is. @param {number} share */
  const heat = (share) => (share >= 0.5 ? 'bg-foreground/80' : share >= 0.2 ? 'bg-foreground/45' : 'bg-foreground/25')
  const sevTone = { high: 'bg-destructive', medium: 'bg-warning', info: 'bg-muted-foreground/50' }
  /** @param {number | null} m */
  const misText = (m) => (m == null || Math.abs(m) < 2 ? '' : `${m > 0 ? '↑' : '↓'}${Math.round(Math.abs(m))}×`)
  const pct = (/** @type {number} */ s) => (s >= 0.995 ? '100%' : s < 0.01 ? '<1%' : `${Math.round(s * 100)}%`)

  const shownInsights = $derived(showAllInsights ? insights : insights.slice(0, 3))

  async function copy(/** @type {string} */ text, /** @type {string} */ what) {
    try { await navigator.clipboard.writeText(text); toast.success(`${what} copied`) } catch { toast.error("Couldn't copy") }
  }

  function askAi() {
    const findings = insights.map((i) => `- ${i.title}: ${i.detail}`).join('\n')
    onaskai?.(
      `Explain this ${result?.driver ?? 'SQL'} query plan in plain words and tell me how to make the query faster (indexes, rewrites, settings). Be specific to these tables.\n\nQuery:\n\`\`\`sql\n${sql.trim()}\n\`\`\`\n\nPlan:\n\`\`\`\n${planText(a.root)}\n\`\`\`${findings ? `\n\nWhat stood out:\n${findings}` : ''}`,
    )
  }

  const metaRows = $derived.by(() => {
    const n = selected
    if (!n) return []
    const r = n.raw
    /** @type {[string, string][]} */
    const rows = []
    if (n.actualRows != null) rows.push(['Rows', `${fmtCount(n.actualRows)}${n.planRows != null ? `  (est ${fmtCount(n.planRows)})` : ''}`])
    else if (n.planRows != null) rows.push(['Estimated rows', fmtCount(n.planRows)])
    if (n.loops > 1) rows.push(['Loops', fmtCount(n.loops)])
    if (n.selfTime != null) rows.push(['Own time', `${fmtMs(n.selfTime)} · ${pct(n.share)}`])
    if (n.totalTime != null) rows.push(['With inputs', fmtMs(n.totalTime)])
    if (n.selfCost != null && hasCost) rows.push(['Own cost', `${n.selfCost.toFixed(2)}${a.analyzed ? '' : ` · ${pct(n.share)}`}`])
    if (n.totalCost != null && hasCost) rows.push(['Total cost', `${num2(r['Startup Cost'])}..${n.totalCost.toFixed(2)}`])
    if (n.removed) rows.push(['Removed by filter', fmtCount(n.removed)])
    if (r['Plan Width'] != null) rows.push(['Row width', `${r['Plan Width']} B`])
    const hit = r['Shared Hit Blocks'], rd = r['Shared Read Blocks']
    if (hit != null || rd != null) rows.push(['Buffers', `${fmtCount(hit ?? 0)} cached · ${fmtCount(rd ?? 0)} read`])
    if (r['Workers Launched'] != null) rows.push(['Workers', `${r['Workers Launched']} of ${r['Workers Planned'] ?? '?'}`])
    if (r['Peak Memory Usage'] != null) rows.push(['Peak memory', `${r['Peak Memory Usage']} kB`])
    if (r['Sort Space Used'] != null) rows.push(['Sort space', `${r['Sort Space Used']} kB ${r['Sort Space Type'] ?? ''}`])
    if (r['Filtered %'] != null) rows.push(['Kept after filter', `${r['Filtered %']}%`])
    return rows
  })
  /** @param {any} v */
  function num2(v) { return typeof v === 'number' ? v.toFixed(2) : '0.00' }
  const conditions = $derived(selected ? CONDITION_KEYS.filter((k) => selected.raw[k] != null && selected.raw[k] !== '').map((k) => [k, conditionText(selected.raw[k])]) : [])
</script>

<div class="flex h-full min-h-0 w-full flex-col">
  <!-- Summary -->
  <!-- A size container: in the results-beside pane the controls on the right
       squeezed the summary until "Measured" read "Measur". Narrow, the stats
       and Ask AI's label give way first. -->
  <div class="@container/xp flex h-10 shrink-0 items-center gap-3 border-b border-border/50 px-3">
    <div class="flex min-w-0 flex-1 items-center gap-3 overflow-hidden text-ui-xs text-muted-foreground [&>*:not(:first-child)]:@max-[34rem]/xp:hidden">
      <span class="shrink-0 font-medium text-foreground">{a.analyzed ? 'Measured' : 'Estimated'}</span>
      <span class="h-3 w-px shrink-0 bg-border"></span>
      {#if result?.executionTime > 0}
        <span class="shrink-0 tabular-nums"><span class="text-foreground">{fmtMs(result.executionTime)}</span> run</span>
      {/if}
      {#if result?.planningTime > 0}
        <span class="shrink-0 tabular-nums"><span class="text-foreground">{fmtMs(result.planningTime)}</span> planning</span>
      {/if}
      {#if rowsOut != null}
        <span class="shrink-0 tabular-nums"><span class="text-foreground">{fmtCount(rowsOut)}</span> {rowsOut === 1 ? 'row' : 'rows'}</span>
      {:else if a.root?.totalCost}
        <span class="shrink-0 tabular-nums">cost <span class="text-foreground">{a.root.totalCost.toFixed(2)}</span></span>
      {/if}
      <span class="shrink-0 tabular-nums">{a.nodes.length} {a.nodes.length === 1 ? 'step' : 'steps'}</span>
    </div>

    <div class="flex shrink-0 items-center gap-1.5">
      {#if isPg && onrerun}
        <div class="flex h-7 items-center rounded-md bg-muted/60 p-0.5 text-ui-2xs" role="radiogroup" aria-label="Explain mode">
          {#each [[false, 'Estimate', "Planner estimates only. Nothing runs."], [true, 'Analyze', 'Runs the query for real numbers, then rolls it back.']] as [on, label, tip] (label)}
            <button type="button" role="radio" aria-checked={analyze === on} title={/** @type {string} */ (tip)}
              class={cn('h-6 rounded-sm px-2 font-medium transition-colors', analyze === on ? 'bg-background text-foreground shadow-xs' : 'text-muted-foreground hover:text-foreground')}
              onclick={() => { if (analyze !== on) onrerun?.(/** @type {boolean} */ (on)) }}>{label}</button>
          {/each}
        </div>
      {/if}
      <div class="flex h-7 items-center rounded-md bg-muted/60 p-0.5" role="radiogroup" aria-label="Plan view">
        {#each [['diagram', 'workflow', 'Diagram'], ['tree', 'list-tree', 'Tree']] as [v, icon, label] (v)}
          <button type="button" role="radio" aria-checked={view === v} aria-label={label} title={label}
            class={cn('flex size-6 items-center justify-center rounded-sm transition-colors', view === v ? 'bg-background text-foreground shadow-xs' : 'text-muted-foreground hover:text-foreground')}
            onclick={() => (view = /** @type {'diagram' | 'tree'} */ (v))}><Icon name={icon} class="size-3.5 shrink-0" /></button>
        {/each}
      </div>
      <DropdownMenu.Root>
        <DropdownMenu.Trigger class="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground data-[state=open]:bg-accent data-[state=open]:text-foreground" aria-label="Copy plan">
          <Icon name="clipboard-copy" class="size-3.5 shrink-0" />
        </DropdownMenu.Trigger>
        <DropdownMenu.Content align="end" class="min-w-44">
          <DropdownMenu.Item onSelect={() => void copy(planText(a.root), 'Plan')}><Icon name="file-text" />Copy as text</DropdownMenu.Item>
          <DropdownMenu.Item onSelect={() => void copy(JSON.stringify(result?.plan, null, 2), 'Plan JSON')}><Icon name="file-code" />Copy JSON</DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Root>
      {#if onaskai}
        <button type="button" class="inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" onclick={askAi}>
          <Icon name="sparkles" class="size-3.5 shrink-0" /><span class="@max-[34rem]/xp:sr-only">Ask AI</span>
        </button>
      {/if}
    </div>
  </div>

  <!-- What stood out: one line each, the detail is in the step's panel -->
  {#if insights.length}
    <div class="shrink-0 border-b border-border/50 py-1">
      {#each shownInsights as ins (ins.id)}
        <button type="button"
          class={cn('flex h-7 w-full items-center gap-2.5 px-3 text-left transition-colors hover:bg-muted/40', ins.nodeId && ins.nodeId === selectedId && 'bg-muted/40')}
          title={ins.detail}
          onclick={() => reveal(ins.nodeId)}>
          <span class={cn('size-1.5 shrink-0 rounded-full', sevTone[ins.severity])}></span>
          <span class="shrink-0 text-ui-xs text-foreground">{ins.title}</span>
          <span class="min-w-0 flex-1 truncate text-ui-xs text-muted-foreground">{ins.detail}</span>
          {#if ins.nodeId}<Icon name="chevron-right" class="size-3.5 shrink-0 text-muted-foreground" />{/if}
        </button>
      {/each}
      {#if insights.length > 3}
        <button type="button" class="flex h-6 items-center px-3 ps-7 text-ui-2xs text-muted-foreground hover:text-foreground" onclick={() => (showAllInsights = !showAllInsights)}>
          {showAllInsights ? 'Show fewer' : `${insights.length - 3} more`}
        </button>
      {/if}
    </div>
  {:else if a.nodes.length}
    <div class="flex h-8 shrink-0 items-center gap-2.5 border-b border-border/50 px-3 text-ui-xs text-muted-foreground">
      <span class="size-1.5 shrink-0 rounded-full bg-success"></span>Nothing stands out in this plan.
    </div>
  {/if}

  <div class="flex min-h-0 flex-1">
    <!-- Steps -->
    <div class="relative min-w-0 flex-1 overflow-hidden" bind:clientWidth={cw} bind:clientHeight={ch}>
      <!-- Line-height probes for cardH: the card text follows the app zoom. -->
      <span class="pointer-events-none invisible absolute text-ui-xs font-semibold" aria-hidden="true" bind:clientHeight={lineTitle}>Ag</span>
      <span class="pointer-events-none invisible absolute text-ui-2xs" aria-hidden="true" bind:clientHeight={lineSmall}>Ag</span>
      {#if view === 'diagram'}
        <div
          class={cn('absolute inset-0 select-none', panning && start.moved ? 'cursor-grabbing' : 'cursor-grab')}
          role="application"
          aria-label="Query plan diagram. Drag to pan, Mod plus wheel to zoom."
          onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={() => (panning = false)} onwheel={wheel}
        >
          <div class="pointer-events-none absolute inset-0 text-border/40" style="background-image: radial-gradient(circle, currentColor 1px, transparent 1px); background-size: {22 * scale}px {22 * scale}px; background-position: {panX}px {panY}px"></div>
          <div class="absolute top-0 left-0" style="width:{canvasW}px;height:{canvasH}px;transform:translate({panX}px,{panY}px) scale({scale});transform-origin:0 0">
            <svg class="pointer-events-none absolute inset-0 overflow-visible" width={canvasW} height={canvasH}>
              {#each edges as e (e.from.id)}
                <path d={edgeD(e.from, e.to)} fill="none" stroke="currentColor" stroke-linecap="round" stroke-width={e.from.n ? edgeW(e.from.n) : 1}
                  stroke-dasharray={e.from.fold ? '3 4' : undefined}
                  class={cn('transition-colors', selectedId && (selectedId === e.from.id || selectedId === e.to.id) ? 'text-foreground/35' : 'text-border')} />
              {/each}
              {#if a.root && layout.get(a.root.id)}
                {@const r = /** @type {Placed} */ (layout.get(a.root.id))}
                <path d={`M ${px(r) + NODE_W} ${cy(r)} H ${qrX}`} fill="none" stroke="currentColor" stroke-linecap="round" stroke-width={edgeW(a.root)} class="text-border" />
              {/if}
            </svg>

            {#each placedFolds as p (p.id)}
              {@const f = /** @type {PlanFold} */ (p.fold)}
              <button
                type="button"
                style="left:{px(p)}px;top:{py(p)}px;width:{NODE_W}px;height:{FOLD_H}px"
                class="absolute flex items-center gap-1.5 rounded-lg border border-dashed border-border px-3 text-left text-ui-2xs text-muted-foreground transition-colors hover:border-foreground/25 hover:text-foreground"
                onpointerdown={(e) => e.stopPropagation()}
                onclick={() => toggleFold(f.parentId)}
                aria-expanded={f.expanded}
              >
                <Icon name={f.expanded ? 'chevrons-down-up' : 'chevrons-up-down'} class="size-3 shrink-0" />
                <span class="truncate">{foldLabel(f)}</span>
              </button>
            {/each}

            {#each placedNodes as p (p.id)}
              {@const n = /** @type {PlanNode} */ (p.n)}
              {@const sel = selectedId === n.id}
              <div
                data-plan-node={n.id}
                style="left:{px(p)}px;top:{py(p)}px;width:{NODE_W}px;height:{cardH(n)}px"
                class={cn(
                  'absolute flex cursor-pointer flex-col overflow-hidden rounded-lg border bg-card transition-[border-color,box-shadow] duration-150',
                  sel ? 'border-foreground/40 ring-1 ring-foreground/15' : 'border-border/70 hover:border-foreground/25',
                )}
              >
                <div class="flex min-w-0 items-start gap-2 px-3 pt-2.5">
                  <div class="min-w-0 flex-1">
                    <p class="truncate text-ui-xs font-semibold text-foreground">{n.type}</p>
                    <p class="truncate font-mono text-ui-2xs text-muted-foreground">{n.relation || n.index || ' '}{n.relation && n.index ? ` · ${n.index}` : ''}</p>
                  </div>
                  {#if n.warnings.length}
                    <span class="mt-1.5 size-1.5 shrink-0 rounded-full bg-warning" title={n.warnings.join('\n')} aria-label={n.warnings.join('; ')}></span>
                  {/if}
                </div>
                {#if n.actualRows != null || n.planRows != null || n.selfTime != null || n.selfCost != null}
                  <div class="mt-2 flex flex-col gap-0.5 px-3 pb-2.5 text-ui-2xs tabular-nums">
                    {#if n.actualRows != null || n.planRows != null}
                      <div class="flex items-center justify-between gap-2 text-muted-foreground">
                        <span>rows</span>
                        <span class="truncate text-foreground">{fmtCount(n.actualRows ?? n.planRows)}{#if n.actualRows != null && n.planRows != null}<span class="text-muted-foreground">&nbsp;/ est {fmtCount(n.planRows)}</span>{/if}{#if misText(n.misestimate)}<span class={cn('ms-1', Math.abs(n.misestimate ?? 0) >= 10 ? 'text-foreground' : 'text-muted-foreground')}>{misText(n.misestimate)}</span>{/if}</span>
                      </div>
                    {/if}
                    {#if n.selfTime != null || n.selfCost != null}
                      <!-- The step's share of the plan as a small meter beside its
                           number, instead of a bar along the card's edge that read
                           as a border (and sat over this row when it overflowed). -->
                      <div class="flex items-center justify-between gap-2 text-muted-foreground">
                        <span>{n.selfTime != null ? 'own time' : 'own cost'}</span>
                        <span class="flex min-w-0 items-center gap-1.5">
                          <span class="truncate text-foreground">{n.selfTime != null ? fmtMs(n.selfTime) : (n.selfCost ?? 0).toFixed(2)}</span>
                          <span class="h-1 w-8 shrink-0 overflow-hidden rounded-full bg-muted" aria-hidden="true"><span class={cn('block h-full rounded-full', heat(n.share))} style="width:{Math.max(6, n.share * 100)}%"></span></span>
                          <span class="w-8 shrink-0 text-right">{pct(n.share)}</span>
                        </span>
                      </div>
                    {/if}
                  </div>
                {/if}
              </div>
            {/each}

            {#if a.root && layout.get(a.root.id)}
              {@const r = /** @type {Placed} */ (layout.get(a.root.id))}
              <div style="left:{qrX}px;top:{cy(r) - 16}px;width:{QR_W}px" class="absolute flex h-8 items-center justify-center gap-1.5 rounded-lg border border-border/70 bg-card text-ui-2xs font-medium text-muted-foreground">
                <Icon name="table-2" class="size-3 shrink-0" />Result
              </div>
            {/if}
          </div>
        </div>

        <div class="absolute right-3 bottom-3 flex items-center gap-0.5 rounded-lg border border-border/60 bg-popover p-0.5">
          <button type="button" class="flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" aria-label="Zoom out" onclick={() => zoomTo(scale / 1.2)}><Icon name="minus" class="size-3.5 shrink-0" /></button>
          <button type="button" class="h-7 min-w-11 rounded-md px-1 text-ui-2xs tabular-nums text-muted-foreground hover:bg-accent hover:text-foreground" title="Fit the whole plan" onclick={() => fit(true)}>{Math.round(scale * 100)}%</button>
          <button type="button" class="flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" aria-label="Zoom in" onclick={() => zoomTo(scale * 1.2)}><Icon name="plus" class="size-3.5 shrink-0" /></button>
        </div>
      {:else}
        <div class="app-scroll h-full overflow-auto">
          <table class="w-full border-collapse text-ui-xs">
            <thead class="sticky top-0 z-10 bg-background text-ui-2xs text-muted-foreground">
              <tr class="border-b border-border/50">
                <th class="h-8 px-3 text-left font-medium">Step</th>
                <th class="h-8 w-28 px-3 text-right font-medium">Rows</th>
                <th class="h-8 w-24 px-3 text-right font-medium">Estimate</th>
                <th class="h-8 w-24 px-3 text-right font-medium">{a.analyzed ? 'Own time' : 'Own cost'}</th>
                <th class="h-8 w-36 px-3 text-left font-medium">Share</th>
              </tr>
            </thead>
            <tbody>
              {#each outline.entries as entry (entry.kind === 'fold' ? entry.id : entry.node.id)}
                {#if entry.kind === 'fold'}
                  <tr class="h-8 border-b border-border/30">
                    <td colspan="5" class="px-3">
                      <button
                        type="button"
                        class="flex items-center gap-1.5 rounded text-ui-2xs text-muted-foreground transition-colors hover:text-foreground"
                        style="padding-left:{entry.depth * 16}px"
                        onclick={() => toggleFold(entry.parentId)}
                        aria-expanded={entry.expanded}
                      >
                        <Icon name={entry.expanded ? 'chevrons-down-up' : 'chevrons-up-down'} class="size-3 shrink-0" />
                        {foldLabel(entry)}
                      </button>
                    </td>
                  </tr>
                {:else}
                {@const n = entry.node}
                <tr
                  class={cn('h-8 cursor-pointer border-b border-border/30 transition-colors', selectedId === n.id ? 'bg-muted/60' : 'hover:bg-muted/30')}
                  onclick={() => (selectedId = selectedId === n.id ? null : n.id)}
                >
                  <td class="max-w-0 px-3">
                    <div class="flex min-w-0 items-center gap-1.5" style="padding-left:{n.depth * 16}px">
                      {#if n.depth}<span class="shrink-0 text-muted-foreground/60">↳</span>{/if}
                      <span class="shrink-0 font-medium text-foreground">{n.type}</span>
                      {#if n.relation}<span class="truncate font-mono text-ui-2xs text-muted-foreground">{n.relation}{n.index ? ` · ${n.index}` : ''}</span>{/if}
                      {#if n.warnings.length}<span class="size-1.5 shrink-0 rounded-full bg-warning" title={n.warnings.join('\n')} aria-label={n.warnings.join('; ')}></span>{/if}
                    </div>
                  </td>
                  <td class="px-3 text-right tabular-nums text-foreground">{n.actualRows != null ? fmtCount(n.actualRows) : '–'}</td>
                  <td class="px-3 text-right tabular-nums text-muted-foreground">{fmtCount(n.planRows)}{#if misText(n.misestimate)}<span class={cn('ms-1', Math.abs(n.misestimate ?? 0) >= 10 && 'text-foreground')}>{misText(n.misestimate)}</span>{/if}</td>
                  <td class="px-3 text-right tabular-nums text-foreground">{n.selfTime != null ? fmtMs(n.selfTime) : n.selfCost != null ? n.selfCost.toFixed(2) : '–'}</td>
                  <td class="px-3">
                    <div class="flex items-center gap-2">
                      <div class="h-1.5 flex-1 overflow-hidden rounded-full bg-muted"><div class={cn('h-full rounded-full', heat(n.share))} style="width:{n.share * 100}%"></div></div>
                      <span class="w-9 shrink-0 text-right text-ui-2xs tabular-nums text-muted-foreground">{pct(n.share)}</span>
                    </div>
                  </td>
                </tr>
                {/if}
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>

    <!-- The picked step -->
    {#if selected}
      <aside class="app-scroll flex w-80 shrink-0 flex-col overflow-y-auto border-l border-border/50 bg-background" aria-label="Step details">
        <div class="flex items-start gap-2 px-4 pt-3 pb-2">
          <div class="min-w-0 flex-1">
            <p class="text-ui-sm font-semibold text-foreground">{selected.type}</p>
            {#if selected.relation || selected.index}
              <p class="truncate font-mono text-ui-2xs text-muted-foreground">{selected.relation}{selected.relation && selected.index ? ' · ' : ''}{selected.index}</p>
            {/if}
          </div>
          <button type="button" class="flex size-6 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" aria-label="Close details" onclick={() => (selectedId = null)}>
            <Icon name="x" class="size-3.5 shrink-0" />
          </button>
        </div>
        {#if describeNode(selected.type)}
          <p class="px-4 pb-3 text-ui-xs leading-relaxed text-muted-foreground">{describeNode(selected.type)}</p>
        {/if}

        {#if selected.warnings.length}
          <div class="mx-4 mb-2 flex flex-col border-t border-border/40 pt-2">
            {#each insights.filter((i) => i.nodeId === selected.id && i.id !== 'hottest') as ins (ins.id)}
              <div class="flex gap-2.5 py-1.5">
                <span class={cn('mt-[7px] size-1.5 shrink-0 rounded-full', sevTone[ins.severity])}></span>
                <div class="min-w-0">
                  <p class="text-ui-xs font-medium text-foreground">{ins.title}</p>
                  <p class="mt-0.5 text-ui-xs leading-relaxed text-muted-foreground">{ins.detail}</p>
                </div>
              </div>
            {/each}
          </div>
        {/if}

        {#if metaRows.length}
          <dl class="mx-4 mb-3 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 border-t border-border/40 pt-3 text-ui-xs">
            {#each metaRows as [k, v] (k)}
              <dt class="text-muted-foreground">{k}</dt>
              <dd class="text-right tabular-nums text-foreground">{v}</dd>
            {/each}
          </dl>
        {/if}

        {#if conditions.length}
          <div class="mx-4 mb-4 flex flex-col gap-2 border-t border-border/40 pt-3">
            {#each conditions as [k, v] (k)}
              <div class="group">
                <div class="flex items-center justify-between">
                  <p class="text-ui-2xs font-medium text-muted-foreground">{k}</p>
                  <button type="button" class="text-ui-3xs text-muted-foreground opacity-0 group-hover:opacity-100 hover:text-foreground" onclick={() => void copy(v, k)}>Copy</button>
                </div>
                <p class="mt-0.5 font-mono text-ui-2xs leading-relaxed break-words text-foreground">{v}</p>
              </div>
            {/each}
          </div>
        {/if}
      </aside>
    {/if}
  </div>
</div>
