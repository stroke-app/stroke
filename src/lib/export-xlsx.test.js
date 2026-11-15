import { describe, it, expect } from 'vitest'
import { rowsToXlsx, crc32, colName } from './export-xlsx.js'
import { inferParquetTypes, EXPORT_FORMATS } from './export.js'

const dec = new TextDecoder()

/** Read a stored zip back into { name: text }. @param {Uint8Array} zip */
function unzip(zip) {
  const v = new DataView(zip.buffer, zip.byteOffset, zip.byteLength)
  const out = /** @type {Record<string, string>} */ ({})
  let at = 0
  while (v.getUint32(at, true) === 0x04034b50) {
    const size = v.getUint32(at + 18, true)
    const nlen = v.getUint16(at + 26, true)
    const name = dec.decode(zip.subarray(at + 30, at + 30 + nlen))
    const data = zip.subarray(at + 30 + nlen, at + 30 + nlen + size)
    expect(crc32(data)).toBe(v.getUint32(at + 14, true))
    out[name] = dec.decode(data)
    at += 30 + nlen + size
  }
  return out
}

describe('rowsToXlsx', () => {
  it('writes a workbook with typed cells and a frozen header', () => {
    const files = unzip(rowsToXlsx([{ name: 'id' }, { name: 'name' }, { name: 'ok' }, { name: 'meta' }], [[1, 'A & <b>', true, { x: 1 }], [2.5, null, false, null]], 'users'))
    expect(Object.keys(files)).toContain('xl/worksheets/sheet1.xml')
    const sheet = files['xl/worksheets/sheet1.xml']
    expect(sheet).toContain('<c r="A2"><v>1</v></c>')
    expect(sheet).toContain('<c r="A3"><v>2.5</v></c>')
    expect(sheet).toContain('A &amp; &lt;b&gt;')
    expect(sheet).toContain('<c r="C2" t="b"><v>1</v></c>')
    expect(sheet).toContain('{&quot;x&quot;:1}')
    expect(sheet).toContain('state="frozen"')
    expect(files['xl/workbook.xml']).toContain('name="users"')
  })

  it('names columns past Z', () => {
    expect([0, 25, 26, 701, 702].map(colName)).toEqual(['A', 'Z', 'AA', 'ZZ', 'AAA'])
  })

  it('matches the standard CRC-32', () => {
    expect(crc32(new TextEncoder().encode('123456789'))).toBe(0xcbf43926)
  })
})

describe('inferParquetTypes', () => {
  it('picks the narrowest type that holds every value', () => {
    expect(inferParquetTypes(5, [[1, 1, true, 'x', null], [2, 2.5, false, 3, null]])).toEqual(['BIGINT', 'DOUBLE', 'BOOLEAN', 'VARCHAR', 'VARCHAR'])
  })
})

describe('EXPORT_FORMATS', () => {
  it('offers every format', () => {
    expect(EXPORT_FORMATS.map((f) => f.id).sort()).toEqual(['csv', 'json', 'jsonl', 'md', 'parquet', 'sql', 'tsv', 'xlsx'])
  })
})
