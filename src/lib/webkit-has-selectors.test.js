// A `:has()` outside the selector's subject (Tailwind's `group-has-*`, or
// `has-[…]:[&>x]:…`) makes WebKit re-check the whole document whenever a
// matching element is inserted or removed anywhere: with a 2,000-table sidebar
// that cost ~5ms per SVG icon mounted, so ⌘K and tab switches stalled for
// hundreds of ms. Keep these patterns out of the component sources.
import { describe, it, expect } from 'vitest'
import fs from 'node:fs'
import path from 'node:path'

const root = path.resolve(__dirname, '..')
/** @param {string} dir @returns {string[]} */
function files(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = path.join(dir, e.name)
    if (e.isDirectory()) return files(p)
    return /\.(svelte|js|css)$/.test(e.name) && !e.name.endsWith('.test.js') ? [p] : []
  })
}

describe('non-subject :has() selectors', () => {
  it('none in src', () => {
    const bad = []
    for (const f of files(root)) {
      // Comments may name the patterns; only code counts.
      const src = fs.readFileSync(f, 'utf8')
        .replace(/<!--[\s\S]*?-->/g, '')
        .replace(/\/\*[\s\S]*?\*\//g, '')
        .replace(/^\s*\/\/.*$/gm, '')
      for (const re of [/\bgroup-has-[\w[\]>=-]*\//g, /\bhas-\[[^\]\s]*\]\]?:\[&/g, /:has\([^)]*\)\s+[[.\w*]/g]) {
        for (const m of src.matchAll(re)) bad.push(`${path.relative(root, f)}: ${m[0]}`)
      }
    }
    expect(bad).toEqual([])
  })
})
