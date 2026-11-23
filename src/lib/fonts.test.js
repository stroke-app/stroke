import { describe, expect, it } from 'vitest'
import { facesFor, faceStack, normalizeFontSettings, systemLabels, DEFAULT_FONT_SETTINGS } from './fonts.js'

describe('facesFor', () => {
  const all = () => true
  it('names the OS faces for the OS', () => {
    expect(facesFor(['sans'], { os: 'macos', installed: all })[0].label).toBe('SF Pro')
    expect(facesFor(['mono'], { os: 'windows', installed: all })[0].label).toBe('Cascadia Mono')
    expect(systemLabels('linux').system).toBe('System UI')
  })
  it('offers another OS\'s faces nowhere, and missing ones nowhere', () => {
    const mac = facesFor(['mono'], { os: 'macos', installed: (f) => f !== 'Monaco' }).map((f) => f.id)
    expect(mac).toContain('menlo')
    expect(mac).not.toContain('monaco')
    expect(mac).not.toContain('consolas')
    const linux = facesFor(['mono'], { os: 'linux', installed: all }).map((f) => f.id)
    expect(linux).toContain('dejavu-mono')
    expect(linux).not.toContain('menlo')
    // Bundled faces are on every OS.
    expect(linux).toContain('jetbrains-mono')
  })
  it('keeps kinds apart', () => {
    expect(facesFor(['mono'], { os: 'macos', installed: all }).every((f) => f.kind === 'mono')).toBe(true)
  })
})

describe('normalizeFontSettings', () => {
  it('defaults, and drops unknown faces', () => {
    expect(normalizeFontSettings(undefined)).toEqual(DEFAULT_FONT_SETTINGS)
    const n = normalizeFontSettings({ separate: true, ui: 'inter', code: 'comic-sans', smoothing: 'system' })
    expect(n).toMatchObject({ separate: true, ui: 'inter', code: '', smoothing: 'system' })
  })
  it('stacks for known faces only', () => {
    expect(faceStack('menlo')).toBe('Menlo, monospace')
    expect(faceStack('')).toBeNull()
    expect(faceStack('nope')).toBeNull()
  })
})
