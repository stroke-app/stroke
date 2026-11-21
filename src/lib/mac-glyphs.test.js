import { describe, it, expect } from 'vitest'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'

/** @param {string} dir @returns {string[]} */
function svelteFiles(dir) {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name)
    return statSync(p).isDirectory() ? svelteFiles(p) : p.endsWith('.svelte') ? [p] : []
  })
}

// Text a person reads (a tooltip, a key hint) spells a shortcut with
// comboTitle() or a Kbd, which print ⌘ on macOS and Ctrl elsewhere. Written
// as a ⌘ glyph, Windows and Linux showed the glyph as it is.
const MAC_ONLY = [
  // The ⌘Q quit guard is a macOS setting, shown only there.
  'Press ⌘Q twice to quit',
  'The first ⌘Q shows a reminder',
]

describe('shortcut hints', () => {
  it('never hard-code a macOS glyph in a tooltip or key hint', () => {
    const offenders = []
    for (const file of svelteFiles('src/lib/components')) {
      const lines = readFileSync(file, 'utf8').split('\n')
      lines.forEach((line, i) => {
        const t = line.trim()
        if (t.startsWith('//') || t.startsWith('*') || t.startsWith('/*') || t.startsWith('<!--')) return
        if (MAC_ONLY.some((m) => line.includes(m))) return
        // Written for both platforms already.
        if (line.includes('IS_MAC') || line.includes('Ctrl/⌘')) return
        if (/title="[^"]*⌘|title=\{?['`][^'`]*⌘|<kbd>[^<]*⌘|['`"][A-Z][^'`"]*\(⌘[^)]*\)['`"]/.test(line)) offenders.push(`${file}:${i + 1}`)
      })
    }
    expect(offenders).toEqual([])
  })
})
