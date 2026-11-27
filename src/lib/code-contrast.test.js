import { describe, expect, it } from 'vitest'
import { CODE_CONTRAST_MIN, contrastRatio, readableCodeHtml, readableHex } from './code-contrast.js'

const rgb = (/** @type {string} */ h) => /** @type {[number, number, number]} */ ([0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)))

describe('readable code colours', () => {
  it('lifts a dim keyword on a dark surface past the target', () => {
    const out = readableHex('4D9375', 'dark')
    expect(out).not.toBe('4D9375')
    expect(contrastRatio(rgb(out), [0x2a, 0x2a, 0x2e])).toBeGreaterThanOrEqual(CODE_CONTRAST_MIN)
  })
  it('leaves a colour that already reads alone', () => {
    expect(readableHex('dbd7ca', 'dark')).toBe('dbd7ca')
    expect(readableHex('1e1e1e', 'light')).toBe('1e1e1e')
  })
  it('darkens on light surfaces', () => {
    const out = readableHex('b1a96f', 'light')
    expect(contrastRatio(rgb(out), [0xee, 0xef, 0xf2])).toBeGreaterThanOrEqual(CODE_CONTRAST_MIN)
  })
  it('rewrites the colours in highlighter HTML only', () => {
    const html = '<pre style="background-color:#121212;color:#dbd7ca"><span style="color:#666666">.</span></pre>'
    const out = readableCodeHtml(html, 'dark')
    expect(out).toContain('background-color:#121212')
    expect(out).toContain('color:#dbd7ca')
    expect(out).not.toContain('color:#666666')
  })
})
