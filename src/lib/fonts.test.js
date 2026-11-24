import { describe, expect, it } from 'vitest'
import { facesFor, faceStack, normalizeFontSettings, systemLabels, cleanFamily, DEFAULT_FONT_SETTINGS } from './fonts.js'

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
    const n = normalizeFontSettings({ separate: true, ui: 'inter', code: 'comic-sans', smoothing: 'grayscale' })
    expect(n).toMatchObject({ separate: true, ui: 'inter', code: '', smoothing: 'grayscale' })
    expect(normalizeFontSettings({ smoothing: 'system' }).smoothing).toBe('system')
  })
  it('stacks for known faces only', () => {
    expect(faceStack('menlo')).toBe('Menlo, monospace')
    expect(faceStack('')).toBeNull()
    expect(faceStack('nope')).toBeNull()
  })
})

describe('installed families', () => {
  const all = () => true
  const system = [
    { family: 'Adwaita Mono', mono: true },
    { family: 'Adwaita Sans', mono: false },
    { family: 'Ubuntu', mono: false },
    { family: 'Inter', mono: false },
  ]
  it('follow the curated faces, once each, mono-only where the surface is', () => {
    const code = facesFor(['mono'], { os: 'linux', installed: all, system })
    expect(code.filter((f) => f.installed).map((f) => f.label)).toEqual(['Adwaita Mono'])
    const ui = facesFor(['sans', 'serif'], { os: 'linux', installed: all, system }).filter((f) => f.installed)
    // Ubuntu is curated and Inter is bundled: neither is listed again.
    expect(ui.map((f) => f.label)).toEqual(['Adwaita Mono', 'Adwaita Sans'])
    expect(ui[0].id).toBe('sys:Adwaita Mono')
  })
  it('fall back to the face they replace', () => {
    expect(faceStack('sys:Adwaita Mono', '"JetBrains Mono Variable", monospace')).toBe('"Adwaita Mono", "JetBrains Mono Variable", monospace')
  })
  it('are kept through normalising, and cannot break out of the CSS string', () => {
    const n = normalizeFontSettings({ separate: true, code: 'sys:Adwaita Mono', app: 'sys:Liberation Mono', appMono: true })
    expect(n).toMatchObject({ code: 'sys:Adwaita Mono', app: 'sys:Liberation Mono', appMono: true })
    expect(normalizeFontSettings({ app: 'sys:', appMono: true })).toMatchObject({ app: '', appMono: false })
    expect(cleanFamily('Evil"; } body { x: "')).toBe('Evil  body  x:')
    expect(faceStack('sys:a"b')).toBe('"ab", system-ui, sans-serif')
  })
})
