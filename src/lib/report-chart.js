// The dashboard window's chart style: a report on a wall, not an editor.
// Values printed on the bars, the value axis on both sides, no grid lines, a
// dashed line with dots for series over time. Pure: takes the option the
// editor builds (chart-utils buildOption) and returns a restyled copy.

/** One colour per card, in order. Bright enough to read across a room on a dark wall. */
export const REPORT_COLORS = ['#00b4ff', '#3cb80c', '#f5b400', '#ff6b6b', '#b07cff', '#2dd4bf', '#ff8a3d']

const AXIS = { line: '#3a3a3a', label: '#a8a8a8', value: '#e6e6e6' }

/** 19815 → "19,815"; fractions keep two places. @param {unknown} v */
export function reportNumber(v) {
  const n = typeof v === 'number' ? v : Number(v)
  if (!Number.isFinite(n)) return String(v ?? '')
  return n.toLocaleString('en-US', { maximumFractionDigits: Number.isInteger(n) ? 0 : 2 })
}

/** @param {any} a */
const list = (a) => (Array.isArray(a) ? a : a ? [a] : [])

/**
 * @param {any} option an ECharts option from buildOption
 * @param {string} color the card's colour
 * @returns {any}
 */
export function reportOption(option, color) {
  if (!option) return option
  const xs = list(option.xAxis)
  const ys = list(option.yAxis)
  const series = list(option.series)
  // Pies, maps, scatter plots and the rest keep their own look, recoloured.
  const cartesian = xs.length === 1 && ys.length === 1 && xs[0]?.type === 'category' && series.every((s) => s.type === 'bar' || s.type === 'line')
  if (!cartesian) return { ...option, backgroundColor: 'transparent', color: [color, ...REPORT_COLORS.filter((c) => c !== color)] }

  const points = Math.max(0, ...series.map((s) => list(s.data).length))
  const labels = points > 0 && points <= 40
  const axisCommon = {
    axisLine: { show: true, lineStyle: { color: AXIS.line } },
    axisTick: { show: false },
    splitLine: { show: false },
    axisLabel: { color: AXIS.label, fontSize: 11, formatter: (/** @type {any} */ v) => reportNumber(v) },
  }
  const value = { ...ys[0], ...axisCommon, name: undefined }
  const restyled = series.map((s, i) => {
    const c = i === 0 ? color : REPORT_COLORS[(REPORT_COLORS.indexOf(color) + i) % REPORT_COLORS.length]
    if (s.type === 'bar') {
      return {
        ...s,
        barMaxWidth: 40,
        itemStyle: { ...(s.itemStyle ?? {}), color: c, borderRadius: 0 },
        label: { show: labels, position: 'top', color: AXIS.value, fontSize: 12, formatter: (/** @type {any} */ p) => reportNumber(Array.isArray(p.value) ? p.value[1] : p.value) },
      }
    }
    return {
      ...s,
      smooth: false,
      areaStyle: undefined,
      symbol: 'circle',
      symbolSize: points > 120 ? 0 : 6,
      showSymbol: points <= 120,
      itemStyle: { color: c },
      lineStyle: { color: c, width: 1.25, type: 'dashed' },
      label: { show: false },
    }
  })
  // The same scale again on the right: an invisible copy of the first series
  // drives it, so both sides always read the same numbers.
  const mirror = {
    ...restyled[0],
    name: undefined,
    yAxisIndex: 1,
    silent: true,
    tooltip: { show: false },
    label: { show: false },
    itemStyle: { opacity: 0 },
    lineStyle: { opacity: 0 },
    symbolSize: 0,
    emphasis: { disabled: true },
  }
  return {
    ...option,
    backgroundColor: 'transparent',
    animationDuration: 400,
    grid: { left: 64, right: 64, top: labels ? 36 : 20, bottom: 52, containLabel: false },
    legend: {
      show: true,
      left: 56,
      bottom: 4,
      icon: 'rect',
      itemWidth: 10,
      itemHeight: 10,
      textStyle: { color: '#d0d0d0', fontSize: 11 },
      data: restyled.map((s) => s.name).filter(Boolean),
    },
    xAxis: [{ ...xs[0], ...axisCommon, axisLabel: { color: AXIS.label, fontSize: 11, hideOverlap: true }, name: undefined }],
    yAxis: [
      { ...value, position: 'left' },
      { ...value, position: 'right', alignTicks: true },
    ],
    dataZoom: undefined,
    series: [...restyled, mirror],
  }
}
