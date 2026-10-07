import { describe, it, expect } from 'vitest'
import {
  activeFilters,
  filtersForApi,
  sortForApi,
  hasTableQuery,
  buildSelectSql,
  createFilter,
  clampPageSize,
  fetchLimitFor,
  pageWithInsertedRow,
  pageSizeLabel,
  readRowsResponse,
  ANY_COLUMN,
  PAGE_SIZE_ALL,
  MAX_PAGE_SIZE,
  DEFAULT_PAGE_SIZE,
} from './table-query.js'

/** @param {Partial<import('./table-query.js').TableFilter>} f */
const mk = (f) => ({ ...createFilter('col', 'eq'), ...f })

/** Build SQL for a single filter and return the generated statement. */
const sqlFor = (/** @type {any} */ filter, engine = 'postgres') =>
  buildSelectSql({ table: 't', engine, filters: [filter] })

describe('activeFilters', () => {
  it('drops filters with no column', () => {
    expect(activeFilters([mk({ column: '', value: 'x' })])).toHaveLength(0)
  })

  it('drops value-requiring filters with empty or whitespace value', () => {
    expect(activeFilters([mk({ op: 'eq', value: '   ' })])).toHaveLength(0)
    expect(activeFilters([mk({ op: 'eq', value: '' })])).toHaveLength(0)
  })

  it('keeps null-check ops without a value', () => {
    expect(activeFilters([mk({ op: 'is_null', value: '' })])).toHaveLength(1)
    expect(activeFilters([mk({ op: 'is_not_null', value: '' })])).toHaveLength(1)
  })

  it('keeps between when a value is present', () => {
    expect(activeFilters([mk({ op: 'between', value: '1,10' })])).toHaveLength(1)
  })

  it('drops filters with an unknown op', () => {
    expect(activeFilters([mk({ op: /** @type {any} */ ('bogus'), value: 'x' })])).toHaveLength(0)
  })

  it('requires a value for ANY_COLUMN search', () => {
    expect(activeFilters([mk({ column: ANY_COLUMN, op: 'contains', value: '' })])).toHaveLength(0)
    expect(activeFilters([mk({ column: ANY_COLUMN, op: 'contains', value: 'hi' })])).toHaveLength(1)
  })
})

describe('filtersForApi', () => {
  it('omits the conjunct on the first filter and attaches dataType', () => {
    const out = filtersForApi(
      [mk({ column: 'a', op: 'eq', value: '1' }), mk({ column: 'b', op: 'eq', value: '2', conjunct: 'or' })],
      [{ name: 'a', dataType: 'int4' }, { name: 'b', dataType: 'text' }],
    )
    expect(out[0].conjunct).toBeUndefined()
    expect(out[0].dataType).toBe('int4')
    expect(out[1].conjunct).toBe('or')
  })

  it('defaults a missing conjunct to "and" on non-first filters', () => {
    const out = filtersForApi([
      mk({ column: 'a', op: 'eq', value: '1' }),
      mk({ column: 'b', op: 'eq', value: '2', conjunct: /** @type {any} */ (undefined) }),
    ])
    expect(out[1].conjunct).toBe('and')
  })

  it('reads snake_case data_type and trims the value', () => {
    const out = filtersForApi(
      [mk({ column: 'a', op: 'eq', value: '  1  ' })],
      [{ name: 'a', data_type: 'int8' }],
    )
    expect(out[0].dataType).toBe('int8')
    expect(out[0].value).toBe('1')
  })

  it('never attaches a dataType to an ANY_COLUMN filter', () => {
    const out = filtersForApi(
      [mk({ column: ANY_COLUMN, op: 'contains', value: 'x' })],
      [{ name: ANY_COLUMN, dataType: 'text' }],
    )
    expect(out[0].dataType).toBeUndefined()
  })
})

describe('sortForApi', () => {
  it('returns undefined fields for null sort', () => {
    expect(sortForApi(null)).toEqual({ sortColumn: undefined, sortDirection: undefined })
  })
  it('maps column + direction and omits `sorts` for a single key', () => {
    expect(sortForApi({ column: 'created_at', direction: 'desc' })).toEqual({
      sortColumn: 'created_at',
      sortDirection: 'desc',
    })
  })
  it('emits a `sorts` array for multi-column sort', () => {
    expect(sortForApi({ column: 'a', direction: 'asc' }, [{ column: 'b', direction: 'desc' }])).toEqual({
      sortColumn: 'a',
      sortDirection: 'asc',
      sorts: [
        { column: 'a', direction: 'asc' },
        { column: 'b', direction: 'desc' },
      ],
    })
  })
  it('drops columnless keys and falls back to the first real key', () => {
    expect(sortForApi(null, [{ column: 'b', direction: 'desc' }])).toEqual({
      sortColumn: 'b',
      sortDirection: 'desc',
    })
    expect(sortForApi({ column: 'a', direction: 'asc' }, [{ column: '', direction: 'desc' }]).sorts).toBeUndefined()
  })
})

describe('hasTableQuery', () => {
  it('is false when nothing is set', () => {
    expect(hasTableQuery('', [], null)).toBe(false)
    expect(hasTableQuery('   ', [], null)).toBe(false)
  })
  it('is true for a search term, a filter, or a sort', () => {
    expect(hasTableQuery('foo', [], null)).toBe(true)
    expect(hasTableQuery('', [mk({ value: '1' })], null)).toBe(true)
    expect(hasTableQuery('', [], { column: 'a', direction: 'asc' })).toBe(true)
  })
})

describe('clampPageSize', () => {
  it('passes through the "All" sentinel', () => {
    expect(clampPageSize(PAGE_SIZE_ALL)).toBe(PAGE_SIZE_ALL)
  })
  it('falls back to the default for non-numeric or sub-1 input', () => {
    expect(clampPageSize('abc')).toBe(DEFAULT_PAGE_SIZE)
    expect(clampPageSize(0)).toBe(DEFAULT_PAGE_SIZE)
    expect(clampPageSize(0.5)).toBe(DEFAULT_PAGE_SIZE)
    expect(clampPageSize(NaN)).toBe(DEFAULT_PAGE_SIZE)
  })
  it('floors fractional sizes and caps at MAX_PAGE_SIZE', () => {
    expect(clampPageSize(99.9)).toBe(99)
    expect(clampPageSize('250')).toBe(250)
    expect(clampPageSize(10_000_000)).toBe(MAX_PAGE_SIZE)
  })
})

describe('pageSizeLabel', () => {
  it('compacts the "All" sentinel and exact millions only', () => {
    expect(pageSizeLabel(PAGE_SIZE_ALL)).toBe('All')
    expect(pageSizeLabel(1_000_000)).toBe('1M')
    expect(pageSizeLabel(5_000_000)).toBe('5M')
    expect(pageSizeLabel(250)).toBe('250')
    expect(pageSizeLabel(1_500_000)).toBe('1500000') // non-exact millions stay plain
  })
})

describe('readRowsResponse', () => {
  it('reads snake_case keys and coerces numeric fields', () => {
    const out = readRowsResponse({ rows: [[1]], primary_key: ['id'], query_ms: 5, total: '42' })
    expect(out.rows).toEqual([[1]])
    expect(out.primaryKey).toEqual(['id'])
    expect(out.queryMs).toBe(5)
    expect(out.total).toBe(42)
    expect(out.columns).toEqual([])
  })
  it('prefers camelCase keys when present', () => {
    const out = readRowsResponse({ primaryKey: ['x'], queryMs: 3 })
    expect(out.primaryKey).toEqual(['x'])
    expect(out.queryMs).toBe(3)
    expect(out.total).toBe(0)
  })
})

describe('buildSelectSql, shape & quoting', () => {
  it('builds a minimal SELECT with schema quoting and default limit', () => {
    expect(buildSelectSql({ schema: 'public', table: 'users', engine: 'postgres', limit: 100 }))
      .toBe('SELECT * FROM "public"."users" LIMIT 100;')
  })

  it('omits the schema prefix when none is given', () => {
    expect(buildSelectSql({ table: 'users', engine: 'postgres' })).toBe('SELECT * FROM "users" LIMIT 100;')
  })

  it('escapes single quotes in values', () => {
    expect(sqlFor(mk({ column: 'name', op: 'eq', value: "O'Brien" }))).toContain(`"name" = 'O''Brien'`)
  })

  it('escapes embedded identifier quotes per engine', () => {
    expect(sqlFor(mk({ column: 'a"b', op: 'eq', value: '1' }), 'postgres')).toContain(`"a""b" = '1'`)
    expect(sqlFor(mk({ column: 'a`b', op: 'eq', value: '1' }), 'mysql')).toContain('`a``b` = \'1\'')
  })
})

// Operator matrix - the surface where per-engine bugs historically hid.
describe('buildSelectSql, operator matrix (postgres)', () => {
  /** @type {[import('./table-query.js').FilterOp, string, string][]} */
  const cases = [
    ['eq', '5', `"age" = '5'`],
    ['neq', '5', `"age" <> '5'`],
    ['gt', '5', `"age" > '5'`],
    ['gte', '5', `"age" >= '5'`],
    ['lt', '5', `"age" < '5'`],
    ['lte', '5', `"age" <= '5'`],
    ['contains', '5', `"age"::text ILIKE '%5%'`],
    ['not_contains', '5', `"age"::text NOT ILIKE '%5%'`],
    ['starts_with', '5', `"age"::text ILIKE '5%'`],
    ['ends_with', '5', `"age"::text ILIKE '%5'`],
    ['is_null', '', `"age" IS NULL`],
    ['is_not_null', '', `"age" IS NOT NULL`],
    ['between', '1,10', `"age" BETWEEN '1' AND '10'`],
  ]
  for (const [op, value, fragment] of cases) {
    it(`${op} → ${fragment}`, () => {
      expect(sqlFor(mk({ column: 'age', op, value }), 'postgres')).toContain(fragment)
    })
  }
})

describe('buildSelectSql, engine differences for substring search', () => {
  it('postgres casts to ::text and uses ILIKE', () => {
    expect(sqlFor(mk({ column: 'age', op: 'contains', value: '5' }), 'postgres'))
      .toContain(`"age"::text ILIKE '%5%'`)
  })
  it('mysql uses backticks + LIKE with no cast', () => {
    expect(sqlFor(mk({ column: 'age', op: 'contains', value: '5' }), 'mysql'))
      .toContain('`age` LIKE \'%5%\'')
    expect(sqlFor(mk({ column: 'age', op: 'not_contains', value: '5' }), 'mysql'))
      .toContain('`age` NOT LIKE \'%5%\'')
  })
  it('mariadb behaves like mysql', () => {
    expect(sqlFor(mk({ column: 'age', op: 'eq', value: '5' }), 'mariadb')).toContain('`age` = \'5\'')
  })
  it('sqlite double-quotes and does NOT cast (currently emits ILIKE, display-only)', () => {
    // NOTE: buildSelectSql is an editable starting point, not executed SQL. SQLite
    // has no native ILIKE; if that ever becomes an issue this assertion flags it.
    expect(sqlFor(mk({ column: 'age', op: 'contains', value: '5' }), 'sqlite'))
      .toContain(`"age" ILIKE '%5%'`)
    expect(sqlFor(mk({ column: 'age', op: 'eq', value: '5' }), 'sqlite')).toContain(`"age" = '5'`)
  })
})

describe('buildSelectSql, WHERE assembly', () => {
  it('emits the search OR-group first, then AND-chains filters, with ORDER BY + LIMIT', () => {
    const sql = buildSelectSql({
      schema: 'public',
      table: 'users',
      columns: [{ name: 'id' }, { name: 'email' }],
      engine: 'postgres',
      search: 'acme',
      filters: [
        mk({ column: 'email', op: 'contains', value: '@x.com' }),
        mk({ column: 'id', op: 'gt', value: '100', conjunct: 'and' }),
      ],
      sort: { column: 'id', direction: 'desc' },
      limit: 50,
    })
    expect(sql).toContain(`WHERE ("id"::text ILIKE '%acme%' OR "email"::text ILIKE '%acme%')`)
    expect(sql).toContain(`"email"::text ILIKE '%@x.com%'`)
    expect(sql).toContain(`"id" > '100'`)
    expect(sql).toContain('ORDER BY "id" DESC')
    expect(sql.endsWith('LIMIT 50;')).toBe(true)
  })

  it('chains a second filter with its OR conjunct', () => {
    const sql = buildSelectSql({
      table: 't',
      engine: 'postgres',
      filters: [
        mk({ column: 'a', op: 'eq', value: '1' }),
        mk({ column: 'b', op: 'eq', value: '2', conjunct: 'or' }),
      ],
    })
    expect(sql).toContain(`WHERE "a" = '1'`)
    expect(sql).toContain(`OR "b" = '2'`)
  })

  it('skips the search group when no columns are known', () => {
    const sql = buildSelectSql({ table: 't', engine: 'postgres', search: 'x' })
    expect(sql).not.toContain('WHERE')
  })

  it('uses backtick quoting and LIKE for MySQL null checks', () => {
    const sql = buildSelectSql({
      table: 'orders',
      engine: 'mysql',
      filters: [mk({ column: 'status', op: 'is_null', value: '' })],
    })
    expect(sql).toContain('FROM `orders`')
    expect(sql).toContain('`status` IS NULL')
  })
})

describe('buildSelectSql, LIMIT handling', () => {
  it('emits an explicit limit', () => {
    expect(buildSelectSql({ table: 't', engine: 'postgres', limit: 250 })).toContain(' LIMIT 250;')
  })
  it('omits LIMIT for the "all rows" sentinels and the max page size', () => {
    expect(buildSelectSql({ table: 't', engine: 'postgres', limit: PAGE_SIZE_ALL })).not.toContain('LIMIT')
    expect(buildSelectSql({ table: 't', engine: 'postgres', limit: 0 })).not.toContain('LIMIT')
    expect(buildSelectSql({ table: 't', engine: 'postgres', limit: MAX_PAGE_SIZE })).not.toContain('LIMIT')
  })
})

describe('fetchLimitFor', () => {
  it('passes a numeric page size straight through', () => {
    expect(fetchLimitFor(50, 752, true)).toBe(50)
    expect(fetchLimitFor(250, 0, false)).toBe(250)
  })

  it('sizes "All" from a count that describes the current view', () => {
    expect(fetchLimitFor(PAGE_SIZE_ALL, 752, true)).toBe(752)
  })

  it('asks for the ceiling when the count belongs to another predicate', () => {
    // The regression: a filter narrowed a 752-row table to one row, so `total`
    // was 1. Clearing the filter must not fetch LIMIT 1.
    expect(fetchLimitFor(PAGE_SIZE_ALL, 1, false)).toBe(MAX_PAGE_SIZE)
  })

  it('asks for the ceiling while the count is unknown or still running', () => {
    expect(fetchLimitFor(PAGE_SIZE_ALL, 0, true)).toBe(MAX_PAGE_SIZE)
    expect(fetchLimitFor(PAGE_SIZE_ALL, -1, true)).toBe(MAX_PAGE_SIZE)
  })

  it('never asks for more than the max page size', () => {
    expect(fetchLimitFor(PAGE_SIZE_ALL, MAX_PAGE_SIZE * 3, true)).toBe(MAX_PAGE_SIZE)
  })
})

describe('pageWithInsertedRow', () => {
  const page = [[1], [2], [3]]

  it('keeps every row on "All", where the limit is the count from before the insert', () => {
    // The limit for "All" is the row count (3); the insert makes 4.
    expect(pageWithInsertedRow(page, [4], PAGE_SIZE_ALL, fetchLimitFor(PAGE_SIZE_ALL, 3, true))).toEqual([[4], [1], [2], [3]])
  })

  it('keeps a fixed-size page at its size, the last row moving to the next page', () => {
    expect(pageWithInsertedRow(page, [4], 3, 3)).toEqual([[4], [1], [2]])
  })

  it('grows a page that is not full yet', () => {
    expect(pageWithInsertedRow(page, [4], 50, 50)).toEqual([[4], [1], [2], [3]])
  })
})
