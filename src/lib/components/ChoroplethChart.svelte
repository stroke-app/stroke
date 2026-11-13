<script>
  import { onDestroy } from 'svelte'
  import { isCurrentThemeDark } from '$lib/stores/settings.js'
  import { getWorldGeoJson } from '$lib/geo.js'

  let {
    spec = /** @type {any} */ (null),
    noTitle = false,
    scrollZoom = false,
  } = $props()

  /** @type {HTMLDivElement|null} */
  let el = $state(null)
  /** @type {any} */
  let chart = $state(null)
  let chartListeners = null
  /** @type {ResizeObserver|null} */
  let ro = null
  let loadError = $state('')
  let loading = $state(false)

  const isDark = $derived($isCurrentThemeDark)

  const ALIASES = {
    'usa':'United States','us':'United States','u.s.':'United States','u.s.a.':'United States','america':'United States',
    'uk':'United Kingdom','britain':'United Kingdom','great britain':'United Kingdom','england':'United Kingdom',
    'uae':'United Arab Emirates','emirates':'United Arab Emirates',
    'south korea':'Korea','republic of korea':'Korea','kor':'Korea',
    'north korea':'Dem. Rep. Korea','dprk':'Dem. Rep. Korea',
    'russia':'Russia','russian federation':'Russia',
    'iran':'Iran','islamic republic of iran':'Iran',
    'vietnam':'Vietnam','viet nam':'Vietnam',
    'taiwan':'Taiwan','republic of china':'Taiwan',
    'dr congo':'Dem. Rep. Congo','drc':'Dem. Rep. Congo','democratic republic of the congo':'Dem. Rep. Congo',
    'republic of congo':'Congo',
    'czech republic':'Czech Rep.','czechia':'Czech Rep.',
    'central african republic':'Central African Rep.',
    'dominican republic':'Dominican Rep.',
    'equatorial guinea':'Eq. Guinea',
    'solomon islands':'Solomon Is.',
    'western sahara':'W. Sahara',
    'falkland islands':'Falkland Is.',
    'people\'s republic of china':'China','prc':'China',
    'burma':'Myanmar',
    'south africa':'South Africa',
    'new zealand':'New Zealand',
    'saudi arabia':'Saudi Arabia',
  }

  function normalizeName(raw) {
    const s = raw.trim()
    return ALIASES[s.toLowerCase()] ?? s
  }

  const valueMap = $derived.by(() => {
    if (!spec?.data?.length) return new Map()
    const m = new Map()
    for (const row of spec.data) {
      const name = normalizeName(String(row[spec.x_col] ?? ''))
      const val = Number(row[spec.y_col] ?? 0)
      if (name) m.set(name, val)
    }
    return m
  })

  const values = $derived([...valueMap.values()])
  const minVal = $derived(values.length ? Math.min(...values) : 0)
  const maxVal = $derived(values.length ? Math.max(...values) : 100)

  async function initChart(container) {
    if (!container) return
    loadError = ''
    loading = true

    let geoJson
    try {
      geoJson = await getWorldGeoJson()
    } catch {
      loadError = 'Could not load world map data'
      loading = false
      return
    }

    // Container stays mounted throughout - safe to proceed
    const { init, registerMap } = await import('echarts')
    if (!container.isConnected) return

    registerMap('world', geoJson)

    if (chart) { chart.dispose(); chart = null }
    const instance = init(container, null, {
      renderer: 'canvas',
      devicePixelRatio: window.devicePixelRatio || 2,
    })
    chart = instance

    ro?.disconnect()
    ro = new ResizeObserver(([e]) => {
      if (e.contentRect.width > 0 && e.contentRect.height > 0) instance.resize()
    })
    ro.observe(container)

    // Re-init (theme/data change, IntersectionObserver re-trigger) must not stack
    // listeners on the same container - abort the prior set first.
    chartListeners?.abort()
    chartListeners = new AbortController()
    const { signal } = chartListeners

    // scrollZoom=true (fullscreen): ECharts handles all wheel natively
    // inline: only Ctrl/Cmd+scroll zooms; plain scroll → page
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
      }, { capture: true, passive: false, signal })
    }

    // Double-click → reset pan/zoom to initial world view
    container.addEventListener('dblclick', () => {
      instance.dispatchAction({ type: 'restore' })
    }, { signal })

    loading = false
    applyOption(instance)
  }

  function buildOption() {
    const fg = isDark ? '#e5e7eb' : '#111827'
    const muted = isDark ? 'rgba(255,255,255,0.45)' : 'rgba(0,0,0,0.45)'
    const land = isDark ? '#1e2433' : '#d1d5db'
    const border = isDark ? 'rgba(255,255,255,0.1)' : 'rgba(0,0,0,0.12)'

    return {
      backgroundColor: 'transparent',
      tooltip: {
        trigger: 'item',
        formatter: (p) => p.value != null
          ? `<div style="font-size:12px"><b>${p.name}</b><br/><span style="opacity:.7">${spec?.y_col ?? 'value'}:</span> <b>${Number(p.value).toLocaleString()}</b></div>`
          : `<b>${p.name}</b>`,
        backgroundColor: isDark ? '#1a1a2e' : '#ffffff',
        borderColor: isDark ? '#334' : '#e5e7eb',
        textStyle: { color: fg, fontSize: 12 },
        extraCssText: 'border-radius:8px;box-shadow:0 4px 16px rgba(0,0,0,0.25)',
      },
      visualMap: {
        type: 'continuous',
        calculable: false,
        min: minVal,
        max: maxVal,
        orient: 'horizontal',
        left: 'center',
        bottom: 4,
        // For horizontal orient: itemWidth = bar thickness (px tall), itemHeight = bar length (px wide)
        itemWidth: 12,
        itemHeight: 260,
        text: [maxVal.toLocaleString(), minVal.toLocaleString()],
        textGap: 10,
        textStyle: { color: muted, fontSize: 10, fontFamily: 'monospace' },
        inRange: {
          color: isDark
            ? ['#1e1b4b', '#3730a3', '#6366f1', '#a5b4fc']
            : ['#f5f3ff', '#7c3aed', '#4f46e5', '#1e1b4b'],
        },
      },
      series: [{
        type: 'map',
        map: 'world',
        roam: true,
        aspectScale: 0.75,
        layoutCenter: ['50%', '47%'],
        layoutSize: '95%',
        data: [...valueMap.entries()].map(([name, value]) => ({ name, value })),
        itemStyle: {
          areaColor: land,
          borderColor: border,
          borderWidth: 0.5,
        },
        emphasis: {
          itemStyle: { areaColor: '#f59e0b', borderColor: '#d97706' },
          label: { show: true, fontSize: 11, color: '#fff', fontWeight: 600 },
        },
        label: { show: false },
      }],
    }
  }

  function applyOption(instance) {
    instance?.setOption(buildOption(), { notMerge: true })
  }

  // Mount: always keep the chart div in DOM; init once it's visible
  $effect(() => {
    const container = el
    if (!container) return
    let disposed = false

    const io = new IntersectionObserver(([e]) => {
      if (!e.isIntersecting || disposed) return
      io.disconnect()
      void initChart(container)
    }, { threshold: 0 })
    io.observe(container)

    return () => {
      disposed = true
      io.disconnect()
      ro?.disconnect(); ro = null
      chartListeners?.abort(); chartListeners = null
      chart?.dispose(); chart = null
    }
  })

  // Re-apply when theme or data changes (after chart is ready)
  $effect(() => {
    isDark; valueMap
    if (chart && !loading) applyOption(chart)
  })

  export function resetView() {
    chart?.dispatchAction({ type: 'restore' })
  }

  onDestroy(() => { ro?.disconnect(); chartListeners?.abort(); chart?.dispose() })
</script>

{#if !spec?.data?.length}
  <div class="flex h-full items-center justify-center text-ui-xs text-muted-foreground">No data</div>
{:else}
  <!-- Chart div stays mounted always, overlays handle loading/error states -->
  <div class="relative h-full w-full">
    <div bind:this={el} class="h-full w-full"></div>

    {#if loadError}
      <div class="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-background/80">
        <p class="text-ui-xs text-destructive">{loadError}</p>
        <button
          type="button"
          class="rounded-md border border-border/50 px-3 py-1.5 font-mono text-ui-3xs text-muted-foreground hover:bg-accent"
          onclick={() => { loadError = ''; if (el) void initChart(el) }}
        >Retry</button>
      </div>
    {:else if loading}
      <div class="loading-reveal absolute inset-0 flex items-center justify-center gap-2 text-ui-xs text-muted-foreground">
        <span class="inline-block animate-spin">⟳</span> Loading map…
      </div>
    {/if}
  </div>
{/if}
