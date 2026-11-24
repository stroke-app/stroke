import { describe, it, expect } from 'vitest'
import { isNightlyVersion } from './build-track.js'

describe('build track', () => {
  it('tells Nightly from Stable by the version', () => {
    expect(isNightlyVersion('3.7.7-nightly.202611240456')).toBe(true)
    expect(isNightlyVersion('3.7.7')).toBe(false)
    expect(isNightlyVersion('')).toBe(false)
  })
})
