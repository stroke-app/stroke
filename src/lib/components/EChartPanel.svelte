<script>
  import { ECHARTS_THEME } from '$lib/chart-utils.js'
  import { onDestroy } from 'svelte'

  let {
    /** @type {import('echarts').EChartsOption} */
    option = {},
    height = '100%',
    /**
     * 'svg' (default): resolution-independent, always crisp, lower memory.
     * 'canvas': use only when GPU acceleration is needed (interactive 3D, etc).
     */
    renderer = /** @type {'svg' | 'canvas'} */ ('canvas'),
    class: cls = '',
    /**
     * Merge option updates in place instead of rebuilding the chart, for as long
     * as the option's structure is unchanged. Worth setting on a panel that is
     * fed a fresh option on a timer.
     *
     * OPT-IN, and deliberately so. A merge cannot express a property going away,
     * so it is only safe when the caller's option builder emits the same set of
     * keys every time. The generic builders in `chart-utils.js` do NOT: e.g. a
     * scatter series picks up `...(large ? { large: true } : {})` past 2000
     * points, and merging a later small result over it would leave the chart
     * stuck in large mode. Those callers keep the full rebuild.
     */
    mergeUpdates = false,
  } = $props()

  /** @type {HTMLDivElement | null} */
  let el = $state(null)
  /** @type {import('echarts').ECharts | null} */
  let chart = $state(null)
  /** @type {ResizeObserver | null} */
  let ro = null
  /** @type {IntersectionObserver | null} */
  let io = null
  /** @type {number} rAF handle for coalescing resize bursts */
  let resizeRaf = 0
  /** Fingerprint of the last applied option; '' until one is applied. */
  let lastShape = ''

  $effect(() => {
    const container = el
    const r = renderer  // track so effect re-runs if renderer prop changes
    if (!container) return

    let disposed = false
    let initializing = false

    async function tryInit() {
      if (disposed || chart || initializing) return
      // Guard: skip if container has no renderable area.
      // Avoids ECharts "Can't get DOM width or height" warning on hidden elements.
      if (container.clientWidth === 0 || container.clientHeight === 0) return
      initializing = true
      try {
        const { init } = await import('echarts')
        await import('echarts/theme/v5')
        // Register the wordcloud series plugin lazily (keeps echarts out of startup bundle).
        await import('echarts-wordcloud')
        if (disposed) return
        const opts = renderer === 'canvas'
          ? { renderer: /** @type {'canvas'} */ ('canvas'), devicePixelRatio: window.devicePixelRatio || 2 }
          : { renderer: /** @type {'svg'} */ ('svg') }
        chart = init(container, ECHARTS_THEME, opts)
      } finally {
        initializing = false
      }
    }

    // IntersectionObserver: defer init until the container is actually on-screen.
    // Charts in hidden tabs or below the fold are never initialized until visible,
    // which eliminates the 0-dimension warning and reduces idle memory usage.
    io = new IntersectionObserver(
      (entries) => { if (entries[0].isIntersecting) void tryInit() },
      { threshold: 0, rootMargin: '100px' },
    )
    io.observe(container)

    // ResizeObserver: react to layout changes.
    // Guard the 0-dimension case here too - hidden containers fire with 0×0
    // and calling chart.resize() on a 0×0 canvas triggers the ECharts warning.
    ro = new ResizeObserver((entries) => {
      const { width, height } = entries[0].contentRect
      if (width === 0 || height === 0) return
      if (!chart) { void tryInit(); return }
      // Coalesce resize bursts (pane drags / window resizes fire many events per
      // frame) into a single chart.resize() per animation frame - a synchronous
      // resize on every event janks the whole UI.
      if (resizeRaf) return
      resizeRaf = requestAnimationFrame(() => {
        resizeRaf = 0
        if (!disposed) chart?.resize()
      })
    })
    ro.observe(container)

    return () => {
      disposed = true
      if (resizeRaf) { cancelAnimationFrame(resizeRaf); resizeRaf = 0 }
      io?.disconnect(); io = null
      ro?.disconnect(); ro = null
      chart?.dispose(); chart = null
      lastShape = '' // a fresh chart has no prior option to merge into
    }
  })

  /**
   * Structural fingerprint of an option: which series it declares (type + name)
   * and the shape of its axes (count + type). Two options with the same
   * fingerprint differ only in their data, which echarts can merge in place.
   * Anything a merge could not express - a dropped series, a swapped axis kind -
   * changes the fingerprint and falls back to a full rebuild.
   * @param {any} o
   */
  function shapeKey(o) {
    const asList = (/** @type {any} */ v) => (Array.isArray(v) ? v : v ? [v] : [])
    const series = asList(o?.series).map((s) => `${s?.type ?? ''}:${s?.name ?? ''}`).join('|')
    const axis = (/** @type {any} */ v) => asList(v).map((a) => a?.type ?? '').join(',')
    return `${series}#${axis(o?.xAxis)}#${axis(o?.yAxis)}`
  }

  // Keep chart in sync with option changes.
  // notMerge tears the chart down and rebuilds every series. That is the correct
  // default - it is the only update that can express a property going away - but
  // it is pure waste for a panel on a refresh timer whose option keeps the same
  // shape and only moves its numbers. Opted-in callers merge while the
  // fingerprint holds and rebuild the moment it changes, which also lets echarts
  // animate between the two states instead of restarting the enter animation.
  // lazyUpdate batches setOption calls that arrive in the same task - reduces
  // unnecessary redraws when multiple reactive updates fire together.
  $effect(() => {
    const c = chart
    const o = option
    if (!c || !o || Object.keys(o).length === 0) return
    const shape = shapeKey(o)
    const notMerge = !mergeUpdates || shape !== lastShape
    lastShape = shape
    c.setOption(o, { notMerge, lazyUpdate: true })
  })

  onDestroy(() => {
    io?.disconnect()
    ro?.disconnect()
    chart?.dispose()
  })
</script>

<div
  bind:this={el}
  style={cls ? '' : `height: ${height}; width: 100%;`}
  class={cls || ''}
></div>
