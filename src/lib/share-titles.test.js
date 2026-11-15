import { describe, it, expect, vi } from 'vitest'

vi.mock('$lib/api.js', () => ({ shareCreate: async () => ({}) }))
vi.mock('$lib/share-crypto.js', () => ({ sealShare: async () => ({}), maskValue: (v) => v }))
vi.mock('$lib/stores/settings.js', () => ({ loadSettings: () => ({}), updateSettings: () => ({}) }))

const { chartTitle, resultsTitle } = await import('./share-flow.js')

describe('share titles', () => {
  it('names a chart after what it plots', () => {
    expect(chartTitle({ x: 'category', y: ['product_count'] })).toBe('Product count by category')
    expect(chartTitle({ x: 'month', y: ['totalRevenue'], group: 'region' })).toBe('Total revenue by month per region')
    expect(chartTitle({ x: '', y: [] })).toBe('Chart')
  })

  it('names results after the table they read', () => {
    expect(resultsTitle('SELECT c.name, COUNT(*) FROM categories c JOIN products p ON true')).toBe('categories')
    expect(resultsTitle('select * from public."Order Items" limit 5')).toBe('Order Items')
    expect(resultsTitle('SELECT 1')).toBe('SELECT 1')
  })
})
