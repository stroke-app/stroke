import { describe, expect, it } from 'vitest'
import {
  WINDOW_ROWS_DEFAULT,
  pickMirrorPage,
  WINDOW_ROWS_MAX,
  WINDOW_ROWS_MIN,
  measureRowBytes,
  pickWindowRows,
  seekKeyFor,
  shouldWindow,
  stableWindowOrder,
  windowKeepCount,
  windowsFullyCovered,
} from './row-window.js'

/** A row shaped like `events`: a handful of scalars. */
const narrowRow = [1, 'a3f9c2', 'click', '2026-01-01T00:00:00Z', 42]
/** A row shaped like `openai_docs`: scalars plus a 1536-dim embedding. */
const embeddingRow = [
  1,
  '4f9acecb-1ada-4f95-b6f9-c748ff87bef4',
  11,
  'contracts/2024.docx',
  'Chunk 11 of document 266',
  355,
  `[${Array.from({ length: 1536 }, (_, i) => (i / 1536 - 0.5).toFixed(8)).join(',')}]`,
]

describe('measureRowBytes', () => {
  it('measures a narrow row at tens of bytes and an embedding row at kilobytes', () => {
    expect(measureRowBytes([narrowRow])).toBeLessThan(200)
    expect(measureRowBytes([embeddingRow])).toBeGreaterThan(10_000)
  })

  it('returns 0 when there is nothing to measure', () => {
    expect(measureRowBytes([])).toBe(0)
    expect(measureRowBytes(/** @type {any} */ (null))).toBe(0)
  })

  it('survives a row that cannot be serialized', () => {
    const cyclic = /** @type {any[]} */ ([1])
    cyclic.push(cyclic)
    expect(measureRowBytes([cyclic])).toBe(0)
  })
})

describe('pickWindowRows', () => {
  it('caps narrow rows rather than fetching unbounded chunks', () => {
    expect(pickWindowRows(measureRowBytes([narrowRow]))).toBe(WINDOW_ROWS_MAX)
  })

  it('shrinks to a couple of hundred rows for embedding-sized rows', () => {
    const rows = pickWindowRows(measureRowBytes([embeddingRow]))
    expect(rows).toBeGreaterThanOrEqual(WINDOW_ROWS_MIN)
    expect(rows).toBeLessThan(500)
  })

  it('keeps one window near the byte target', () => {
    // 2KB rows → ~600KB per window, whatever the row shape happens to be: a
    // window is what a scroll waits on, so it stays under a second at 5 Mbit.
    const bytes = 2_000
    expect(pickWindowRows(bytes) * bytes).toBeLessThanOrEqual(600_000)
    expect(pickWindowRows(bytes) * bytes).toBeGreaterThan(400_000)
  })

  it('falls back to the default when the payload is unmeasurable', () => {
    expect(pickWindowRows(0)).toBe(WINDOW_ROWS_DEFAULT)
  })
})

describe('shouldWindow', () => {
  it('windows a long result', () => {
    expect(shouldWindow({ rowCount: 1_000_000, bytesPerRow: 130 })).toBe(true)
  })

  it('windows a short but heavy result - 10k embedding rows is ~175MB', () => {
    expect(shouldWindow({ rowCount: 10_000, bytesPerRow: 17_500 })).toBe(true)
  })

  it('loads a short, light result whole', () => {
    expect(shouldWindow({ rowCount: 10_000, bytesPerRow: 130 })).toBe(false)
  })

  it('streams a few thousand wide rows instead of one big response', () => {
    // 4,000 rows of 1.5 KB is 6 MB: windowed, so rows show as they arrive.
    expect(shouldWindow({ rowCount: 4_000, bytesPerRow: 1_500 })).toBe(true)
  })

  it('never windows without an exact row count', () => {
    expect(shouldWindow({ rowCount: 1_000_000, bytesPerRow: 130, countKnown: false })).toBe(false)
    expect(shouldWindow({ rowCount: 0, bytesPerRow: 130 })).toBe(false)
  })
})

describe('windowsFullyCovered', () => {
  it('counts only windows whose every slot is filled', () => {
    expect(windowsFullyCovered(200, 114)).toEqual([0])
    expect(windowsFullyCovered(200, 10_000)).toEqual([])
    expect(windowsFullyCovered(500, 100)).toEqual([0, 1, 2, 3, 4])
  })

  it('is empty for a nonsense window size instead of looping', () => {
    expect(windowsFullyCovered(500, 0)).toEqual([])
  })
})

describe('stableWindowOrder', () => {
  it('orders by the primary key when the user has no sort', () => {
    expect(stableWindowOrder(null, [], ['id'])).toEqual({
      sortColumn: 'id',
      sortDirection: 'asc',
      sorts: [{ column: 'id', direction: 'asc' }],
    })
  })

  it('keeps the user sort first and appends the key as a tiebreaker', () => {
    const order = stableWindowOrder({ column: 'source', direction: 'desc' }, [], ['id'])
    expect(order?.sortColumn).toBe('source')
    expect(order?.sortDirection).toBe('desc')
    expect(order?.sorts).toEqual([
      { column: 'source', direction: 'desc' },
      { column: 'id', direction: 'asc' },
    ])
  })

  it('does not duplicate a key the user is already sorting by', () => {
    const order = stableWindowOrder({ column: 'id', direction: 'desc' }, [], ['id'])
    expect(order?.sorts).toEqual([{ column: 'id', direction: 'desc' }])
  })

  it('carries every column of a composite key', () => {
    const order = stableWindowOrder(null, [], ['doc_id', 'chunk'])
    expect(order?.sorts).toEqual([
      { column: 'doc_id', direction: 'asc' },
      { column: 'chunk', direction: 'asc' },
    ])
  })

  it('keeps secondary sort keys ahead of the tiebreaker', () => {
    const order = stableWindowOrder(
      { column: 'source', direction: 'asc' },
      [{ column: 'tokens', direction: 'desc' }],
      ['id'],
    )
    expect(order?.sorts?.map((k) => k.column)).toEqual(['source', 'tokens', 'id'])
  })

  it('refuses to order a table with no primary key, so it is not windowed', () => {
    expect(stableWindowOrder(null, [], [])).toBeNull()
    expect(stableWindowOrder({ column: 'name' }, [], [])).toBeNull()
  })
})

describe('windowKeepCount', () => {
  it('keeps a row budget, so small windows are kept in greater number', () => {
    expect(windowKeepCount(10_000)).toBe(8)
    expect(windowKeepCount(100)).toBe(800)
  })

  it('always keeps a few windows either side', () => {
    expect(windowKeepCount(1_000_000)).toBe(3)
  })
})

describe('seekKeyFor', () => {
  /** `events`: a bigint PK, ordered by it - the case windowing is built for. */
  const eventsCols = [
    { name: 'id', dataType: 'int8' },
    { name: 'user_id', dataType: 'uuid' },
    { name: 'kind', dataType: 'text' },
  ]
  const view = (over = {}) => ({
    order: stableWindowOrder(null, [], ['id']),
    columns: eventsCols,
    primaryKey: ['id'],
    dialect: 'postgres',
    ...over,
  })

  it('seeks by the primary key, carrying its cast and column index', () => {
    expect(seekKeyFor(view())).toEqual({ column: 'id', sqlType: 'int8', desc: false, index: 0 })
  })

  it('follows the direction the windows are ordered by', () => {
    const desc = stableWindowOrder({ column: 'id', direction: 'desc' }, [], ['id'])
    expect(seekKeyFor(view({ order: desc }))?.desc).toBe(true)
  })

  it('refuses a sort the key alone cannot resolve', () => {
    // ORDER BY kind, id - the keyset predicate has no tiebreaker for `kind`.
    expect(seekKeyFor(view({ order: stableWindowOrder({ column: 'kind' }, [], ['id']) }))).toBeNull()
  })

  it('refuses a composite primary key', () => {
    expect(seekKeyFor(view({
      order: stableWindowOrder(null, [], ['tenant', 'id']),
      primaryKey: ['tenant', 'id'],
      columns: [{ name: 'tenant', dataType: 'int8' }, { name: 'id', dataType: 'int8' }],
    }))).toBeNull()
  })

  it('refuses a cast the backend would reject, rather than seeking pointlessly', () => {
    expect(seekKeyFor(view({ columns: [{ name: 'id', dataType: 'varchar(...)' }] }))).toBeNull()
    expect(seekKeyFor(view({ columns: [{ name: 'id', dataType: '' }] }))).toBeNull()
  })

  it('refuses engines that ignore the cursor', () => {
    for (const dialect of ['mysql', 'sqlite', 'd1']) {
      expect(seekKeyFor(view({ dialect }))).toBeNull()
    }
  })

  it('refuses a view with no stable order at all', () => {
    expect(seekKeyFor(view({ order: null }))).toBeNull()
  })
})

describe('pickMirrorPage', () => {
  it('keeps a page near 256 KB, within 200-5000 rows', () => {
    expect(pickMirrorPage(850)).toBe(301)
    expect(pickMirrorPage(20)).toBe(5000)
    expect(pickMirrorPage(50_000)).toBe(200)
    expect(pickMirrorPage(0)).toBe(2000)
  })
})
