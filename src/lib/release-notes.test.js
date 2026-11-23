import { describe, expect, it } from 'vitest'
import { collectNotes, previousStable, splitVersions } from './release-notes.js'
import { parseChangelog } from './changelog.js'

const CHANGELOG = `# Changelog

---

## [3.1.1] - 2026-11-20

### Bug Fixes
- **B** - newer fix.

## [3.1.0] - 2026-11-20

### New Features
- **F** - a feature.

### Changes

#### Performance
- **P** - faster,
  over two lines.

### Bug Fixes
- **A** - older fix.

## [3.0.0] - 2026-11-19

### New Features
- **Old** - already on stable.
`

describe('previousStable', () => {
  it('picks the newest stable tag below the version', () => {
    expect(previousStable(['v2.3.0', 'v3.0.0', 'nightly', 'v3.1.0-beta.1', 'v3.2.0'], '3.1.1')).toBe('3.0.0')
  })
  it('is null with no older stable tag', () => {
    expect(previousStable(['nightly', 'v3.1.1'], '3.1.1')).toBeNull()
  })
})

describe('collectNotes', () => {
  it('returns one version alone without `since`', () => {
    expect(collectNotes(CHANGELOG, '3.1.1')).toBe('### Bug Fixes\n- **B** - newer fix.')
  })

  it('merges every version down to the previous stable, by heading', () => {
    const notes = collectNotes(CHANGELOG, '3.1.1', '3.0.0')
    expect(notes).toBe(
      [
        '### New Features',
        '- **F** - a feature.',
        '',
        '### Bug Fixes',
        '- **B** - newer fix.',
        '- **A** - older fix.',
        '',
        '### Changes',
        '',
        '#### Performance',
        '- **P** - faster,\n  over two lines.',
      ].join('\n'),
    )
    expect(notes).not.toContain('Old')
  })

  it('stays readable by the update dialog', () => {
    const sections = parseChangelog(collectNotes(CHANGELOG, '3.1.1', '3.0.0'))
    expect(sections.map((s) => [s.title, s.type, s.items.length])).toEqual([
      ['New Features', 'feature', 1],
      ['Bug Fixes', 'fix', 2],
      ['Performance', 'change', 1],
    ])
  })

  it('is empty for a version the changelog lacks', () => {
    expect(collectNotes(CHANGELOG, '9.9.9', '3.0.0')).toBe('')
    expect(splitVersions(CHANGELOG).map((s) => s.version)).toEqual(['3.1.1', '3.1.0', '3.0.0'])
  })
})
