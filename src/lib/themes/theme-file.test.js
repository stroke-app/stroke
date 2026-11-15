import { describe, it, expect } from 'vitest'
import { parseThemeFile, themeCss, safeColor, toThemeFile } from './theme-file.js'

describe('parseThemeFile', () => {
  it('reads a Stroke theme', () => {
    const t = parseThemeFile({ version: 1, name: 'Aurora', author: 'me', appearance: 'dark', colors: { background: '#0f1117', foreground: '#e6e6e6', primary: '#7aa2f7', border: '#222' } })
    expect(t).toMatchObject({ id: 'aurora', name: 'Aurora', author: 'me', appearance: 'dark' })
    expect(t.colors.border).toBe('#222')
  })

  it('maps a VS Code theme and reads its type', () => {
    const t = parseThemeFile({ name: 'Solar Light', type: 'light', colors: { 'editor.background': '#fdf6e3', 'editor.foreground': '#586e75', 'button.background': '#268bd2', 'sideBar.background': '#eee8d5' } })
    expect(t.appearance).toBe('light')
    expect(t.colors).toMatchObject({ background: '#fdf6e3', foreground: '#586e75', primary: '#268bd2', sidebar: '#eee8d5' })
  })

  it('guesses appearance from the background', () => {
    expect(parseThemeFile({ name: 'x', colors: { background: '#ffffff', foreground: '#111', primary: '#06f' } }).appearance).toBe('light')
    expect(parseThemeFile({ name: 'x', colors: { background: '#101010', foreground: '#eee', primary: '#06f' } }).appearance).toBe('dark')
  })

  it('says which colour is missing', () => {
    expect(() => parseThemeFile({ name: 'x', colors: { background: '#000', foreground: '#fff' } })).toThrow(/primary/)
    expect(() => parseThemeFile('nope')).toThrow(/JSON object/)
  })

  it('drops anything that could break out of the stylesheet', () => {
    expect(safeColor('red; } body { display:none')).toBeNull()
    expect(safeColor('rgb(1 2 3)')).toBe('rgb(1 2 3)')
    const t = parseThemeFile({ name: 'x', colors: { background: '#000', foreground: '#fff', primary: '#06f', border: '#000;}*{color:red' } })
    expect(t.colors.border).toBeUndefined()
    expect(themeCss(t)).not.toContain('}*{')
  })

  it('round-trips through the file format', () => {
    const t = parseThemeFile({ version: 1, name: 'Ink', appearance: 'light', colors: { background: '#fff', foreground: '#000', primary: '#333' } })
    expect(parseThemeFile(toThemeFile(t))).toEqual(t)
    expect(themeCss(t)).toMatch(/^html\[data-theme="imported"\] \{/)
  })
})
