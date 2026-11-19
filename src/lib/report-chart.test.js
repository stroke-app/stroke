import { describe, it, expect } from 'vitest'
import { reportOption, reportNumber } from './report-chart.js'

const bars = {
  xAxis: { type: 'category', data: ['3206', '3218'] },
  yAxis: { type: 'value' },
  series: [{ type: 'bar', name: 'user_id', data: [11870, 12359] }],
  dataZoom: [{ type: 'slider' }],
}

describe('reportOption', () => {
  it('prints values on bars and mirrors the value axis', () => {
    const o = reportOption(bars, '#00b4ff')
    expect(o.yAxis.map((/** @type {any} */ y) => y.position)).toEqual(['left', 'right'])
    expect(o.series[0].label.show).toBe(true)
    expect(o.series[0].label.formatter({ value: 19815 })).toBe('19,815')
    expect(o.series[0].itemStyle.color).toBe('#00b4ff')
    // The right axis is driven by an invisible copy of the first series.
    expect(o.series[1]).toMatchObject({ yAxisIndex: 1, silent: true, label: { show: false } })
    expect(o.dataZoom).toBeUndefined()
    expect(o.legend.data).toEqual(['user_id'])
  })
  it('draws lines dashed with dots, and skips labels on long series', () => {
    const data = Array.from({ length: 80 }, (_, i) => i)
    const o = reportOption({ ...bars, series: [{ type: 'line', name: 'n', data }] }, '#f5b400')
    expect(o.series[0].lineStyle).toMatchObject({ type: 'dashed', color: '#f5b400' })
    expect(o.series[0].symbol).toBe('circle')
    expect(o.grid.top).toBe(20)
  })
  it('leaves other charts alone apart from colour', () => {
    const pie = { series: [{ type: 'pie', data: [] }] }
    const o = reportOption(pie, '#3cb80c')
    expect(o.series).toBe(pie.series)
    expect(o.color[0]).toBe('#3cb80c')
  })
  it('formats numbers for a wall', () => {
    expect(reportNumber(1234567)).toBe('1,234,567')
    expect(reportNumber(3.14159)).toBe('3.14')
    expect(reportNumber('x')).toBe('x')
  })
})
