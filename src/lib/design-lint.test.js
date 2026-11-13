// Runs the design-system ratchet (scripts/lint-design.mjs) with the unit tests,
// so a new one-off radius or font size fails `npm test`, not a screenshot review.
import { test, expect } from 'vitest'
import { scan, regressions, readBaseline } from '../../scripts/lint-design.mjs'

test('no new design-system violations (see DESIGN_SYSTEM.md)', () => {
  const { counts, hits } = scan()
  const bad = regressions(counts, readBaseline())
  const report = bad.map((b) => {
    const lines = hits[b.file].filter((h) => h.rule === b.rule).map((h) => `:${h.line} ${h.text}`)
    return `${b.file} ${b.rule} ${b.was} -> ${b.now}\n  ${lines.join('\n  ')}`
  })
  expect(report, 'run `node scripts/lint-design.mjs` for details').toEqual([])
})
