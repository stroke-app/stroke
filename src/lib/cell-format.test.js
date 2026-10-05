import { describe, it, expect } from 'vitest'
import {
  pgArrayElem, pgArrayText, isSqlArrayType, isVectorType, foldLines, valuesEqual,
  cellJsonString, csvCell, cellSqlLiteral, mdCell, isOversizeValue,
} from './cell-format.js'

describe('cell-format', () => {
  it('writes Postgres array literals, quoting only what needs it', () => {
    expect(pgArrayText(['a', 'b'])).toBe('{a,b}')
    expect(pgArrayText([])).toBe('{}')
    expect(pgArrayText(['a b', 'x,y', 'q"', '', null, 'NULL'])).toBe('{"a b","x,y","q\\"","",NULL,"NULL"}')
    expect(pgArrayText([[1, 2], [3]])).toBe('{{1,2},{3}}')
    expect(pgArrayElem({ k: 1 })).toBe('{"k":1}')
  })

  it('recognises array and pgvector column types', () => {
    expect(isSqlArrayType('text[]')).toBe(true)
    expect(isSqlArrayType('text')).toBe(false)
    expect(isVectorType('vector(1536)')).toBe(true)
    expect(isVectorType('halfvec')).toBe(true)
    expect(isVectorType('varchar')).toBe(false)
  })

  it('folds a multi-line value onto one line', () => {
    expect(foldLines('[\n  "a",\n  "b"\n]')).toBe('[ "a", "b" ]')
    expect(foldLines('one line')).toBe('one line')
  })

  it('compares cell values, objects by content', () => {
    expect(valuesEqual(1, 1)).toBe(true)
    expect(valuesEqual({ a: [1] }, { a: [1] })).toBe(true)
    expect(valuesEqual({ a: 1 }, { a: 2 })).toBe(false)
    expect(valuesEqual(null, undefined)).toBe(false)
    expect(valuesEqual('1', 1)).toBe(false)
  })

  it('escapes CSV per RFC 4180', () => {
    expect(csvCell(null)).toBe('')
    expect(csvCell('plain')).toBe('plain')
    expect(csvCell('a,b')).toBe('"a,b"')
    expect(csvCell('say "hi"')).toBe('"say ""hi"""')
    expect(csvCell('two\nlines')).toBe('"two\nlines"')
    expect(csvCell({ a: 1 })).toBe('"{""a"":1}"')
  })

  it('writes SQL literals for INSERT', () => {
    expect(cellSqlLiteral(null)).toBe('NULL')
    expect(cellSqlLiteral(true)).toBe('TRUE')
    expect(cellSqlLiteral(42)).toBe('42')
    expect(cellSqlLiteral("it's")).toBe("'it''s'")
    expect(cellSqlLiteral({ q: "o'k" })).toBe(`'{"q":"o''k"}'`)
  })

  it('escapes Markdown table cells', () => {
    expect(mdCell(null)).toBe('NULL')
    expect(mdCell('a|b\nc')).toBe('a\\|b c')
  })

  it('shows an oversize stand-in as its marker, and knows one', () => {
    const over = { __strokeOversize: true, bytes: 2048, preview: '{"a"' }
    expect(isOversizeValue(over)).toBe(true)
    expect(isOversizeValue({ a: 1 })).toBe(false)
    expect(cellJsonString({ a: 1 })).toBe('{"a":1}')
    expect(typeof cellJsonString(over)).toBe('string')
  })
})
