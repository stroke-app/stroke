<script>
  import { chartRows } from '$lib/ai-chart-data.js'
  /**
   * Renders AI-generated charts using ECharts + the shared buildOption() utility.
   * Spec format: { type, title, data, x_col, y_col, z_col?, group_col? }
   */
  import { saveExportAs } from '$lib/api.js'
  import { canvasToPngBlob } from '$lib/svg-png.js'
  import { buildOption } from '$lib/chart-utils.js'
  import { isCurrentThemeDark } from '$lib/stores/settings.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
  import ChoroplethChart from './ChoroplethChart.svelte'
  import CarbonMeterChart from './CarbonMeterChart.svelte'

  let {
    spec = null,
    /** Suppress the in-chart title (caller shows it in the card header) */
    noTitle = false,
    /** When true (fullscreen mode), all scroll zooms - no interception needed */
    scrollZoom = false,
  } = $props()

  /** @type {HTMLDivElement | null} */
  let el = $state(null)
  /** @type {import('echarts').ECharts | null} */
  let chart = $state(null)
  /** @type {ResizeObserver | null} */
  let ro = null

  const isDark = $derived($isCurrentThemeDark)

  // Rows whatever shape the spec carries them in (ai-chart-data.js).
  const data = $derived(chartRows(spec?.data, spec ?? {}))
  const converted = $derived.by(() => {
    if (!data.length) return null
    const keys = Object.keys(data[0] ?? {})

    /** @param {unknown} v */
    function detectType(v) {
      if (typeof v === 'number') return 'numeric'
      if (typeof v === 'string') {
        if (/^\d{4}-\d{2}/.test(v)) return 'timestamp'
        if (v.trim() !== '' && !isNaN(Number(v))) return 'numeric'
      }
      return 'text'
    }

    const columns = keys.map(k => {
      const sample = data.find(r => r[k] != null)?.[k]
      const dt = detectType(sample)
      return { name: k, dataType: dt, data_type: dt }
    })

    const rows = data.map(obj =>
      keys.map(k => {
        const v = obj[k]
        const col = columns.find(c => c.name === k)
        if (col?.dataType === 'numeric' && typeof v === 'string') {
          const n = Number(v)
          return isNaN(n) ? v : n
        }
        return v
      })
    )
    return { columns, rows }
  })

  const option = $derived.by(() => {
    if (!converted || !spec) return {}
    const { columns, rows } = converted
    try {
      const base = buildOption({
        type: spec.type ?? 'bar',
        columns,
        rows,
        xCol: spec.x_col ?? columns[0]?.name ?? '',
        yCol: spec.y_col ?? columns[1]?.name ?? '',
        zCol: spec.z_col || undefined,
        groupCol: spec.group_col || undefined,
        isDark,
        title: noTitle ? undefined : (spec.title || undefined),
        noTitle,
      })
      // In scroll-zoom (fullscreen) mode, add inside dataZoom for all axis-based charts
      if (scrollZoom && base && Object.keys(base).length > 0) {
        return { ...base, dataZoom: [{ type: 'inside', zoomOnMouseWheel: true, moveOnMouseWheel: false }] }
      }
      return base
    } catch {
      return {}
    }
  })

  $effect(() => {
    const container = el
    if (!container) return
    let disposed = false
    let initializing = false
    const ac = new AbortController()

    async function tryInit() {
      if (disposed || chart || initializing) return
      if (container.clientWidth === 0 || container.clientHeight === 0) return
      initializing = true
      try {
        const { init } = await import('echarts')
        // Register the wordcloud series plugin lazily (keeps echarts out of startup bundle).
        await import('echarts-wordcloud')
        if (disposed) return
        const instance = init(container, null, {
          renderer: 'canvas',
          devicePixelRatio: window.devicePixelRatio || 2,
        })
        chart = instance
        ro = new ResizeObserver((entries) => {
          const { width, height } = entries[0].contentRect
          if (width === 0 || height === 0) return
          instance.resize()
        })
        ro.observe(container)
        // Scroll zoom:
        // • scrollZoom=true (fullscreen): let ECharts handle all wheel natively - no interception
        // • inline: Ctrl/Cmd+scroll → synthesize plain wheel for ECharts zoom
        //           plain scroll   → stopPropagation so browser scrolls the page naturally
        if (!scrollZoom) {
          container.addEventListener('wheel', (e) => {
            if (!e.isTrusted) return
            e.stopPropagation()
            if (e.ctrlKey || e.metaKey) {
              const canvas = container.querySelector('canvas')
              canvas?.dispatchEvent(new WheelEvent('wheel', {
                deltaY: e.deltaY, deltaMode: e.deltaMode,
                clientX: e.clientX, clientY: e.clientY,
                bubbles: false, cancelable: true,
              }))
            }
          }, { capture: true, passive: true, signal: ac.signal })
        }
        // Double-click → restore to initial view
        container.addEventListener('dblclick', () => {
          instance.dispatchAction({ type: 'restore' })
        }, { signal: ac.signal })
      } finally {
        initializing = false
      }
    }

    // Defer init until the container is on-screen
    const io = new IntersectionObserver(
      (entries) => { if (entries[0].isIntersecting) void tryInit() },
      { threshold: 0 },
    )
    io.observe(container)

    return () => {
      disposed = true
      ac.abort()
      io.disconnect()
      ro?.disconnect(); ro = null
      chart?.dispose(); chart = null
    }
  })

  $effect(() => {
    const c = chart
    const o = option
    if (c && o && Object.keys(o).length > 0) {
      c.setOption(o, { notMerge: true, lazyUpdate: true })
    }
  })

  export async function downloadPng(filename = 'chart.png') {
    const blob = await canvasToPngBlob(el?.querySelector('canvas') ?? null)
    if (!blob) { toast.error('Could not export chart'); return }
    try {
      const path = await saveExportAs(blob, filename, { name: 'PNG', extensions: ['png'] })
      if (!path) return  // dialog cancelled
      toast.success('Chart saved as PNG', { description: `Saved to ${path}` })
    } catch (e) {
      toast.error('Could not save the chart', { description: String(e) })
    }
  }

  /** @type {ChoroplethChart|null} */
  let choroplethRef = $state(null)

  export function resetView() {
    if (choroplethRef) { choroplethRef.resetView(); return }
    chart?.dispatchAction({ type: 'restore' })
  }

  const hasData = $derived(data.length > 0)
  /** The spec with its rows normalised, for the charts that read `spec.data` themselves. */
  const rowSpec = $derived(spec ? { ...spec, data } : spec)
</script>

{#if spec?.type === 'choropleth'}
  <ChoroplethChart bind:this={choroplethRef} spec={rowSpec} {noTitle} {scrollZoom} />
{:else if spec?.type === 'meter'}
  <CarbonMeterChart spec={rowSpec} {noTitle} />
{:else if hasData}
  <div bind:this={el} class="h-full w-full"></div>
{:else}
  <div class="flex items-center justify-center h-full text-ui-xs text-muted-foreground">
    No data
  </div>
{/if}
