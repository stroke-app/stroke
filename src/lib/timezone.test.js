import { describe, it, expect } from 'vitest'
import { instantIn, resolveZone } from './timezone.js'

describe('timezone', () => {
  it('shows a timestamptz in another zone with its offset and every fractional digit', () => {
    expect(instantIn('2026-06-05 10:07:11.618398 UTC', 'Asia/Kathmandu')).toBe('2026-06-05 15:52:11.618398 +05:45')
    expect(instantIn('2026-01-15T23:30:00Z', 'America/New_York')).toBe('2026-01-15 18:30:00 -05:00')
    expect(instantIn('2026-01-15 10:00:00+05:30', 'UTC')).toBe('2026-01-15 10:00:00+05:30')
    expect(instantIn('2026-01-15 10:00:00+05:30', 'Europe/London')).toBe('2026-01-15 04:30:00 +00:00')
  })
  it('leaves values with no zone alone', () => {
    expect(instantIn('2026-06-05 10:07:11', 'Asia/Tokyo')).toBeNull()
    expect(instantIn('not a date at all here', 'Asia/Tokyo')).toBeNull()
  })
  it('reads the setting', () => {
    expect(resolveZone('utc')).toBe('UTC')
    expect(resolveZone('Asia/Tokyo')).toBe('Asia/Tokyo')
    expect(typeof resolveZone('auto')).toBe('string')
  })
})
