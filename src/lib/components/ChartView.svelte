<script>
  import { Button } from '$lib/components/ui/button/index.js'
  import { untrack } from 'svelte'
  import EChartPanel from './EChartPanel.svelte'
  import {
    buildOption, guessXCol, guessYCol, isChartable,
    colType, getRequiredAxes, CHART_CATALOG, resolveChartAccent,
  } from '$lib/chart-utils.js'
  import { isCurrentThemeDark } from '$lib/stores/settings.js'
  import { chartGroups, saveChart, addGroup } from '$lib/stores/saved-charts.js'
  import { cn } from '$lib/utils.js'
  import { saveExportAs } from '$lib/api.js'
  import { canvasToPngBlob, copyPngToClipboard } from '$lib/svg-png.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import * as Select from '$lib/components/ui/select/index.js'
  import ChevronDown  from '@lucide/svelte/icons/chevron-down'
  import Download     from '@lucide/svelte/icons/download'
  import Copy         from '@lucide/svelte/icons/copy'
  import Bookmark     from '@lucide/svelte/icons/bookmark'
  import X            from '@lucide/svelte/icons/x'
  import Check        from '@lucide/svelte/icons/check'
  import BarChart2    from '@lucide/svelte/icons/bar-chart-2'
  import Plus         from '@lucide/svelte/icons/plus'
  import Search       from '@lucide/svelte/icons/search'
  import CarbonMeterChart from './CarbonMeterChart.svelte'

  /** @typedef {{ name: string, dataType?: string, data_type?: string }} ColInfo */

  let {
    /** @type {ColInfo[]} */
    columns = [],
    /** @type {unknown[][]} */
    rows = [],
    /** @type {string} */
    sql = '',
    /** @type {string} */
    connectionId = '',
    initialChartType = 'bar',
    oncharttypechange = undefined,
  } = $props()

  // ── Chart type ────────────────────────────────────────────────────────────
  let chartType = $state(untrack(() => initialChartType ?? 'bar'))

  const currentEntry = $derived(CHART_CATALOG.find(c => c.id === chartType) ?? CHART_CATALOG[0])

  // ── Picker popover ────────────────────────────────────────────────────────
  let pickerOpen = $state(false)
  let pickerSearch = $state('')
  /** @type {HTMLDivElement | null} */
  let pickerRef = $state(null)
  /** @type {HTMLButtonElement | null} */
  let pickerTrigger = $state(null)

  const GROUPS_ORDER = ['Bar','Line & Area','Part-to-Whole','Correlation','Distribution','Hierarchical','Flow','Other']

  const filteredCatalog = $derived.by(() => {
    const q = pickerSearch.toLowerCase().trim()
    if (!q) return CHART_CATALOG
    return CHART_CATALOG.filter(c =>
      c.label.toLowerCase().includes(q) ||
      c.group.toLowerCase().includes(q) ||
      c.description?.toLowerCase().includes(q)
    )
  })

  const filteredByGroup = $derived(
    GROUPS_ORDER
      .map(g => ({ group: g, charts: filteredCatalog.filter(c => c.group === g) }))
      .filter(g => g.charts.length > 0)
  )

  function selectChartType(id) {
    chartType = id
    oncharttypechange?.(id)
    pickerOpen = false
    pickerSearch = ''
  }

  function onPickerKeydown(/** @type {KeyboardEvent} */ e) {
    if (e.key === 'Escape') { pickerOpen = false; pickerSearch = '' }
  }

  // Close on outside click
  $effect(() => {
    if (!pickerOpen) return
    function handler(e) {
      if (
        pickerRef && !pickerRef.contains(/** @type {Node} */ (e.target)) &&
        pickerTrigger && !pickerTrigger.contains(/** @type {Node} */ (e.target))
      ) {
        pickerOpen = false
        pickerSearch = ''
      }
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  })

  // ── Axis selection ────────────────────────────────────────────────────────

  // Cap the rows that reach chart building. Past this, uniform sampling keeps
  // rendering responsive and prevents the O(n) coercion pass and buildOption's
  // aggregation/Math ops from freezing or crashing on huge (≈1M-row) results.
  // Aggregating charts (bar/pie/line) still represent the whole set closely; the
  // toolbar flags when data was sampled.
  const MAX_CHART_ROWS = 50_000
  // A windowed table browse hands over a sparse array as long as the whole
  // table, with holes where pages aren't loaded yet. Indexing into it sampled
  // holes (`undefined`) and the coercion pass threw on `row.map`. Chart the rows
  // that exist: forEach skips holes, and a dense array passes through as is.
  const loadedRows = $derived.by(() => {
    /** @type {any[]} */
    const out = []
    rows.forEach((r) => { if (r) out.push(r) })
    return out.length === rows.length ? rows : out
  })
  const partial = $derived(loadedRows.length < rows.length)
  const sampled = $derived(loadedRows.length > MAX_CHART_ROWS)
  const chartRows = $derived.by(() => {
    if (loadedRows.length <= MAX_CHART_ROWS) return loadedRows
    const step = loadedRows.length / MAX_CHART_ROWS
    const out = new Array(MAX_CHART_ROWS)
    for (let i = 0; i < MAX_CHART_ROWS; i++) out[i] = loadedRows[Math.floor(i * step)]
    return out
  })

  // Sniff row data to detect numeric columns that the DB reported with no/wrong type
  const effectiveColumns = $derived.by(() => {
    if (!loadedRows.length) return columns
    return columns.map((col, i) => {
      if (colType(col) === 'number') return col
      const samples = chartRows.slice(0, 20).map(r => /** @type {any} */ (r)[i]).filter(v => v != null && v !== '')
      if (samples.length === 0) return col
      const allNumeric = samples.every(v =>
        typeof v === 'number' || (typeof v === 'string' && !isNaN(Number(v)))
      )
      return allNumeric ? { ...col, dataType: 'number', data_type: 'number' } : col
    })
  })

  // Coerce string-encoded numerics to actual numbers in rows
  const effectiveRows = $derived.by(() => {
    if (!loadedRows.length) return loadedRows
    return chartRows.map(row =>
      /** @type {any[]} */ (row).map((v, i) => {
        if (effectiveColumns[i] && colType(effectiveColumns[i]) === 'number' && typeof v === 'string') {
          const n = Number(v)
          return isNaN(n) ? v : n
        }
        return v
      })
    )
  })

  const numericCols = $derived(effectiveColumns.filter(c => colType(c) === 'number'))
  const allCols = $derived(effectiveColumns.map(c => c.name))
  const requiredAxes = $derived(getRequiredAxes(chartType))

  let xCol = $state('')
  let yCol = $state('')
  let zCol = $state('')
  let groupCol = $state('')

  // Seed / repair the axis selections when the available columns change, but
  // KEEP the user's manual picks as long as they still refer to a real column.
  // (The old version reset all four on every columns change, silently undoing
  // whatever axes the user had chosen whenever the derived column list churned.)
  $effect(() => {
    const names = new Set(effectiveColumns.map((c) => c.name))
    untrack(() => {
      const gx = xCol && names.has(xCol) ? xCol : guessXCol(effectiveColumns)
      xCol = gx
      yCol = yCol && names.has(yCol) ? yCol : guessYCol(effectiveColumns, gx)
      if (zCol && !names.has(zCol)) zCol = ''
      if (groupCol && !names.has(groupCol)) groupCol = ''
    })
  })

  // ── Chart option ──────────────────────────────────────────────────────────
  const isDark = $derived($isCurrentThemeDark)

  // Resolve the app's --primary token to a concrete rgb() string so charts follow
  // the current theme. Reading the CSS var directly yields oklch(), which
  // zrender/ECharts can't parse - painting it onto a probe element lets the
  // browser normalise it to rgb(). Recomputes when the theme (isDark) flips.
  // Re-resolve the theme accent whenever the theme flips. buildOption also
  // resolves it internally as a default, so non-ChartView chart paths (AI charts,
  // previews) stay themed too; passing it here keeps the table/SQL charts
  // reactive to live theme changes.
  const accent = $derived.by(() => {
    void isDark // dep: re-resolve on theme change
    return resolveChartAccent()
  })

  // Canvas renderer is faster for data charts; SVG only for small previews.
  // Threshold at 500: below that SVG is fine, above that canvas wins noticeably.
  const renderer = $derived(effectiveRows.length > 500 ? 'canvas' : 'svg')

  const option = $derived.by(() => {
    if (!xCol || effectiveRows.length === 0) return {}
    const needsY = !['histogram', 'tree'].includes(chartType)
    if (needsY && !yCol) return {}
    // Degrade a bad build to an empty chart instead of throwing into the render
    // tree (the <svelte:boundary> below is the backstop for render-time throws).
    try {
      return buildOption({ type: chartType, columns: effectiveColumns, rows: effectiveRows, xCol, yCol: yCol || xCol, zCol: zCol || undefined, groupCol: groupCol || undefined, isDark, accent })
    } catch (e) {
      console.error('chart buildOption failed', e)
      return {}
    }
  })

  const meterSpec = $derived.by(() => {
    if (chartType !== 'meter') return null
    const xi2 = effectiveColumns.findIndex(c => c.name === xCol)
    const yi2 = effectiveColumns.findIndex(c => c.name === yCol)
    const zi2 = zCol ? effectiveColumns.findIndex(c => c.name === zCol) : -1
    return { data: effectiveRows, x_col: xi2, y_col: yi2, z_col: zi2 >= 0 ? zi2 : undefined }
  })

  // ── Actions ───────────────────────────────────────────────────────────────
  async function copyConfig() {
    try {
      await navigator.clipboard.writeText(JSON.stringify(option, null, 2))
      toast.success('ECharts config copied')
    } catch { toast.error('Could not copy') }
  }

  /** This instance's canvas host - a document-wide query can grab another
   *  ChartView's canvas when several are mounted (split panes, SQL console). */
  let hostEl = $state(/** @type {HTMLDivElement|null} */ (null))

  function chartCanvas() {
    return /** @type {HTMLCanvasElement|null} */ (hostEl?.querySelector('canvas') ?? null)
  }

  async function downloadPng() {
    const blob = await canvasToPngBlob(chartCanvas())
    if (!blob) { toast.error('Could not export chart'); return }
    try {
      const path = await saveExportAs(blob, `chart_${Date.now()}.png`, { name: 'PNG', extensions: ['png'] })
      if (!path) return  // dialog cancelled
      toast.success('Chart saved', { description: `Saved to ${path}` })
    } catch (e) {
      toast.error('Could not save the chart', { description: String(e) })
    }
  }

  async function copyPng() {
    try {
      await copyPngToClipboard(await canvasToPngBlob(chartCanvas()))
      toast.success('Chart copied as PNG')
    } catch (e) {
      toast.error('Copy failed', { description: String(e) })
    }
  }

  /**
   * The on-screen chart is a canvas, which has no vector to hand out. Re-render
   * the same option offscreen with the SVG renderer and serialize that - the
   * output is true vector, so it stays sharp at any size.
   */
  async function downloadSvg() {
    if (!Object.keys(option).length) { toast.error('Nothing to export'); return }
    const host = document.createElement('div')
    const live = chartCanvas()
    host.style.cssText = `position:fixed;left:-10000px;top:0;width:${live?.clientWidth || 1200}px;height:${live?.clientHeight || 800}px`
    document.body.appendChild(host)
    try {
      const { init } = await import('echarts')
      await import('echarts-wordcloud')
      const inst = init(host, null, { renderer: 'svg' })
      inst.setOption(option, { notMerge: true })
      const svg = host.querySelector('svg')?.outerHTML ?? ''
      inst.dispose()
      if (!svg) { toast.error('Could not export chart'); return }
      const path = await saveExportAs(svg, `chart_${Date.now()}.svg`, { name: 'SVG image', extensions: ['svg'] })
      if (!path) return  // dialog cancelled
      toast.success('Chart saved as SVG', { description: `Saved to ${path}` })
    } catch (e) {
      toast.error('Could not save the chart', { description: String(e) })
    } finally {
      host.remove()
    }
  }

  /**
   * Image exports, driven from the table toolbar's Export submenu so the chart
   * offers the same PNG / SVG / Copy set the ERD does.
   * @param {'png' | 'svg' | 'copy-png'} kind
   */
  export function exportChart(kind) {
    if (kind === 'png') return void downloadPng()
    if (kind === 'svg') return void downloadSvg()
    return void copyPng()
  }

  // ── Save panel ────────────────────────────────────────────────────────────
  let saveOpen = $state(false)
  let saveName = $state('')
  let saveGroup = $state('Default')
  let newGroupMode = $state(false)
  let newGroupName = $state('')

  function openSavePanel() {
    saveName = ''; saveGroup = $chartGroups[0] ?? 'Default'
    newGroupMode = false; newGroupName = ''; saveOpen = true
  }

  function commitSave() {
    const name = saveName.trim()
    if (!name) { toast.error('Enter a chart name'); return }
    let group = saveGroup
    if (newGroupMode) {
      const ng = newGroupName.trim()
      if (!ng) { toast.error('Enter a group name'); return }
      addGroup(ng); group = ng
    }
    saveChart({ name, group, connectionId, sql, config: { type: chartType, xCol, yCol, zCol: zCol||undefined, groupCol: groupCol||undefined }, previewOption: Object.keys(option).length > 0 ? option : undefined })
    toast.success(`"${name}" saved`)
    saveOpen = false
  }

  const chartable = $derived(isChartable(effectiveColumns))

  // The axis pickers are one token: a label chip welded to the value. The whole
  // token IS the Select trigger, so the border, hover and focus ring are drawn
  // once by the primitive instead of a wrapper and a nested control each
  // painting their own (which is what made the focused state look doubled).
  const axisToken = 'group h-8 shrink-0 gap-0 overflow-hidden rounded-md border-0 bg-sidebar-accent/40 p-0 pr-2 hover:bg-sidebar-accent [&_svg]:size-3.5 [&_svg]:text-muted-foreground'
  const axisLabel = 'flex h-full shrink-0 select-none items-center pl-2.5 pr-1.5 text-ui-xs text-muted-foreground'
  const axisValue = 'truncate pr-1 text-ui-sm text-foreground'
  const axisContent = 'z-[120] max-h-[20rem] min-w-[11rem] p-1'
  // No radius here: Select.Item's own rounded-lg is derived from the panel's
  // radius minus its padding, and overriding it breaks that relationship.
  const axisItem = ''
  const iconBtn = 'inline-flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground'
</script>

<div class="flex min-h-0 flex-1 flex-col overflow-hidden">

  {#if !chartable}
    <div class="flex flex-1 flex-col items-center justify-center gap-3 text-center">
      <BarChart2 class="size-9 shrink-0 text-muted-foreground" />
      <p class="text-ui-sm text-muted-foreground">Need at least one numeric column to chart</p>
    </div>
  {:else}

    <!-- ── Single-row toolbar ─────────────────────────────────────────── -->
    <div
      class="studio-chrome relative flex shrink-0 flex-wrap items-center gap-1.5 border-b border-border/50 bg-panel px-3 py-1.5"
      data-studio-chrome
    >

      <!-- Chart type trigger -->
      <div class="relative">
        <button
          bind:this={pickerTrigger}
          type="button"
          onclick={() => { pickerOpen = !pickerOpen; pickerSearch = '' }}
          class={cn(
            'flex h-8 items-center gap-2 rounded-md px-2.5 text-ui-sm text-foreground',
            pickerOpen ? 'bg-sidebar-accent' : 'bg-sidebar-accent/40 hover:bg-sidebar-accent',
          )}
        >
          <BarChart2 class="size-3.5 shrink-0 text-muted-foreground" />
          <span>{currentEntry?.label ?? 'Chart'}</span>
          <ChevronDown class={cn('size-3 shrink-0 text-muted-foreground transition-transform', pickerOpen && 'rotate-180')} />
        </button>

        <!-- Searchable chart picker popover -->
        {#if pickerOpen}
          <div
            role="listbox"
            tabindex="-1"
            bind:this={pickerRef}
            onkeydown={onPickerKeydown}
            class="absolute left-0 top-full z-50 mt-1 w-[34rem] overflow-hidden rounded-[10px] border border-border/60 bg-popover elevate-2-rim"
          >
            <!-- Search -->
            <div data-slot="menu-search" class="flex h-11 items-center gap-2 px-3">
              <Search class="size-4 shrink-0 text-muted-foreground" />
              <input
                type="text"
                placeholder="Search chart types…"
                bind:value={pickerSearch}
                class="no-focus-ring flex-1 bg-transparent text-ui-sm text-foreground outline-none placeholder:text-muted-foreground"
              />
              {#if pickerSearch}
                <button type="button" onclick={() => (pickerSearch = '')} class="text-muted-foreground hover:text-foreground">
                  <X class="size-3.5 shrink-0" />
                </button>
              {/if}
            </div>

            <!-- Chart grid -->
            <div class="max-h-[22rem] overflow-y-auto p-1.5">
              {#if filteredByGroup.length === 0}
                <p class="py-6 text-center text-ui-xs text-muted-foreground">No charts match "{pickerSearch}"</p>
              {:else}
                {#each filteredByGroup as grp (grp.group)}
                  <div class="mb-2">
                    <p class="px-2 pt-2 pb-1 text-ui-2xs font-medium text-muted-foreground">{grp.group}</p>
                    <!-- Even tiles, two to a row: the name and what it shows, so
                         "Pareto" or "Bullet" explain themselves before you click. -->
                    <div class="grid grid-cols-2 gap-0.5">
                      {#each grp.charts as t (t.id)}
                        <button
                          type="button"
                          title={t.description}
                          onclick={() => selectChartType(t.id)}
                          class={cn(
                            'flex min-w-0 items-start gap-2 rounded-md px-2.5 py-2 text-left',
                            chartType === t.id ? 'bg-sidebar-accent' : 'hover:bg-sidebar-accent/60',
                          )}
                        >
                          <span class="min-w-0 flex-1">
                            <span class={cn('block truncate text-ui-sm', chartType === t.id ? 'text-foreground' : 'text-foreground/85')}>{t.label}</span>
                            {#if t.description}<span class="block truncate text-ui-2xs text-muted-foreground">{t.description}</span>{/if}
                          </span>
                          {#if chartType === t.id}<Check class="mt-0.5 size-3.5 shrink-0 text-foreground" />{/if}
                        </button>
                      {/each}
                    </div>
                  </div>
                {/each}
              {/if}
            </div>
          </div>
        {/if}
      </div>

      <!-- Separator -->
      <span class="h-4 w-px shrink-0 bg-border/50"></span>

      <!-- Axis pickers, integrated label + select tokens -->
      {#snippet axisPicker(
        /** @type {string} */ label,
        /** @type {string} */ value,
        /** @type {string[]} */ options,
        /** @type {(v: string) => void} */ onpick,
        /** @type {boolean} */ optional,
      )}
        <Select.Root type="single" {value} onValueChange={(v) => onpick(v ?? '')}>
          <Select.Trigger size="sm" class={axisToken} aria-label={label}>
            <span class={axisLabel}>{label}</span>
            <span class={axisValue}>{value || '—'}</span>
          </Select.Trigger>
          <Select.Content class={axisContent} sideOffset={6}>
            {#if optional}<Select.Item value="" label="—" class={axisItem}>—</Select.Item>{/if}
            {#each options as col (col)}
              <Select.Item value={col} label={col} class={axisItem}>{col}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      {/snippet}

      {#if requiredAxes.x}
        {@render axisPicker(requiredAxes.x.split(' ')[0], xCol, allCols, (v) => (xCol = v), false)}
      {/if}

      {#if requiredAxes.y}
        {@render axisPicker(
          requiredAxes.y.split(' ')[0],
          yCol,
          ['scatter', 'bubble'].includes(chartType) ? allCols : numericCols.map(c => c.name),
          (v) => (yCol = v),
          false,
        )}
      {/if}

      {#if requiredAxes.z}
        {@render axisPicker(
          requiredAxes.z.split(' ')[0],
          zCol,
          numericCols.map(c => c.name).filter(n => n !== xCol && n !== yCol),
          (v) => (zCol = v),
          true,
        )}
      {/if}

      {#if requiredAxes.group}
        {@render axisPicker(
          'Group',
          groupCol,
          allCols.filter(c => c !== xCol && c !== yCol),
          (v) => (groupCol = v),
          true,
        )}
      {/if}

      <!-- Right actions -->
      <div class="ml-auto flex items-center gap-0.5">
        <span class="mr-1.5 tabular-nums font-mono text-ui-2xs text-muted-foreground">{loadedRows.length.toLocaleString()}{partial ? ` of ${rows.length.toLocaleString()} loaded` : ' rows'}</span>
        <button type="button" class={iconBtn} title="Save chart" onclick={openSavePanel}>
          <Bookmark class="size-3.5 shrink-0" />
        </button>
        <button type="button" class={iconBtn} title="Copy ECharts JSON (AI-ready)" onclick={copyConfig}>
          <Copy class="size-3.5 shrink-0" />
        </button>
        <button type="button" class={iconBtn} title="Download PNG" onclick={() => void downloadPng()}>
          <Download class="size-3.5 shrink-0" />
        </button>
      </div>
    </div>

    <!-- ── Save panel (inline below toolbar) ─────────────────────────── -->
    {#if saveOpen}
      <div class="flex h-12 shrink-0 items-center gap-2 border-b border-border/40 px-3">
        <span class="text-ui-xs text-muted-foreground">Save as</span>
        <input
          type="text"
          placeholder="Chart name…"
          bind:value={saveName}
          class="no-field-frame h-8 w-40 rounded-md border-0 bg-sidebar-accent/50 px-2.5 text-ui-sm text-foreground outline-none placeholder:text-muted-foreground focus:bg-sidebar-accent"
        />

        {#if !newGroupMode}
          <Select.Root type="single" value={saveGroup} onValueChange={(v) => v && (saveGroup = v)}>
            <Select.Trigger
              size="sm"
              aria-label="Chart group"
              class="h-8 gap-1.5 rounded-md border-0 bg-sidebar-accent/50 px-2.5 text-ui-sm [&_svg]:size-3.5 [&_svg]:text-muted-foreground"
            >
              <span class="truncate">{saveGroup}</span>
            </Select.Trigger>
            <Select.Content class={axisContent} sideOffset={6}>
              {#each $chartGroups as g (g)}<Select.Item value={g} label={g} class={axisItem}>{g}</Select.Item>{/each}
            </Select.Content>
          </Select.Root>
          <button type="button" class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground hover:text-foreground" title="New group" onclick={() => { newGroupMode = true }}>
            <Plus class="size-3.5 shrink-0" />
          </button>
        {:else}
          <input
            type="text"
            placeholder="New group name…"
            bind:value={newGroupName}
            class="no-field-frame h-8 w-36 rounded-md border-0 bg-sidebar-accent/50 px-2.5 text-ui-sm text-foreground outline-none placeholder:text-muted-foreground focus:bg-sidebar-accent"
          />
          <button type="button" class="inline-flex size-6 items-center justify-center rounded-md text-muted-foreground hover:text-foreground" onclick={() => (newGroupMode = false)}>
            <X class="size-3 shrink-0" />
          </button>
        {/if}

        <Button
          onclick={commitSave}
          size="sm"
          variant="secondary"
>
          <Check class="size-3 shrink-0" />Save
        </Button>
        <button
          type="button"
          onclick={() => (saveOpen = false)}
          class="inline-flex h-6 items-center gap-1 rounded-md px-2 text-ui-xs text-muted-foreground hover:bg-accent hover:text-foreground"
        >
          Cancel
        </button>
      </div>
    {/if}

    <!-- ── Chart ──────────────────────────────────────────────────────── -->
    <div bind:this={hostEl} class="chart-canvas-host relative min-h-0 flex-1">
      <!-- Contain any render/ECharts throw to this panel so a bad chart shows an
           inline message instead of taking down the tab / app. -->
      <svelte:boundary>
        {#if loadedRows.length === 0}
          <div class="absolute inset-0 flex items-center justify-center">
            <p class="text-ui-sm text-muted-foreground">No data to display</p>
          </div>
        {:else if chartType === 'meter' && meterSpec}
          <CarbonMeterChart spec={meterSpec} />
        {:else}
          <EChartPanel {option} {renderer} class="absolute inset-0" />
        {/if}
        {#if sampled}
          <div class="pointer-events-none absolute bottom-1 right-2 rounded bg-background/70 px-1.5 py-0.5 font-mono text-ui-3xs text-muted-foreground">
            sampled {MAX_CHART_ROWS.toLocaleString()} of {loadedRows.length.toLocaleString()} {partial ? 'loaded ' : ''}rows
          </div>
        {/if}
        {#snippet failed(error, reset)}
          <div class="absolute inset-0 flex flex-col items-center justify-center gap-2 p-6 text-center">
            <p class="text-ui-sm font-medium text-foreground/80">This chart couldn't render</p>
            <p class="max-w-md font-mono text-ui-2xs text-muted-foreground">{error instanceof Error ? error.message : String(error)}</p>
            <button type="button" class="mt-1 rounded-md border border-border px-2 py-1 text-ui-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" onclick={reset}>Retry</button>
          </div>
        {/snippet}
      </svelte:boundary>
    </div>

  {/if}
</div>
