import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

// Every built-in theme carries a contrast-solved text variant for each
// semantic colour (see "Semantic text colours" in app-themes.css). A theme
// added without them falls back to the :root 80% mix, which was never
// measured against its surfaces.
const css = readFileSync(new URL('./app-themes.css', import.meta.url), 'utf8')
const KEYS = ['success', 'warning', 'destructive', 'info', 'primary']

describe('semantic text colours', () => {
  const themes = [...new Set([...css.matchAll(/html\[data-theme="([^"]+)"\] \{[^}]*--background:/g)].map((m) => m[1]))]
  it('finds the themes', () => {
    expect(themes.length).toBeGreaterThan(20)
  })
  for (const t of themes) {
    it(`${t} has a text variant for every semantic colour`, () => {
      const blocks = [...css.matchAll(new RegExp(`html\\[data-theme="${t}"\\] \\{([^}]*)\\}`, 'g'))].map((m) => m[1]).join('\n')
      for (const k of KEYS) expect(blocks, `${t} --${k}-text`).toMatch(new RegExp(`--${k}-text:`))
    })
  }
})
