import { describe, it, expect } from 'vitest'
import { errorDuration } from './toast-durations.js'

describe('errorDuration', () => {
  it('gives a short error the usual 4.5 s and a long one more, up to 12 s', () => {
    expect(errorDuration('', {})).toBe(4500)
    expect(errorDuration('Copy failed')).toBe(4500 + 'Copy failed'.length * 40)
    const long = 'No ODBC data source named "ShopPG" on this computer. Add it in ~/.odbc.ini or /etc/odbc.ini, or connect with a connection string instead.'
    expect(errorDuration('Could not connect to Shop via ODBC', { description: long })).toBeGreaterThan(10_000)
    expect(errorDuration('x'.repeat(500))).toBe(12_000)
  })
})
