import { describe, expect, it } from 'vitest'
import {
  compareVersions,
  parseChangelog,
  parseReleases,
  releasesBetween,
  splitInlineCode,
} from './changelog.js'

const CHANGELOG = `# Changelog

All notable changes to Stroke are listed here, newest first.

---

## [2.3.0] - 2026-10-20

### New Features

#### Sidebar
- Objects tab (#101)
- Sort tables by creation time

### Bug Fixes
- \`Esc\` ends an AI reply

## [2.2.5] - 2026-10-10

### Changes
- Faster startup

## [2.2.4] - 2026-10-05

### Bug Fixes
- Shortcuts fire once

## [2.2.3] - 2026-10-01

### Bug Fixes
- Older fix
`

describe('parseChangelog', () => {
  it('types sub-sections by their parent and keeps the group', () => {
    const sections = parseChangelog('### New Features\n\n#### Sidebar\n- One\n\n### Bug Fixes\n- Two (#12)\n')
    expect(sections).toEqual([
      { title: 'Sidebar', group: 'New Features', type: 'feature', items: ['One'] },
      { title: 'Bug Fixes', group: 'Bug Fixes', type: 'fix', items: ['Two'] },
    ])
  })

  it('ignores horizontal rules', () => {
    expect(parseChangelog('### Changes\n---\n- Real item')).toEqual([
      { title: 'Changes', group: 'Changes', type: 'change', items: ['Real item'] },
    ])
  })

  it('returns nothing for empty notes', () => {
    expect(parseChangelog('')).toEqual([])
  })
})

describe('compareVersions', () => {
  it('orders by number, not by string', () => {
    expect(compareVersions('2.10.0', '2.9.9')).toBeGreaterThan(0)
    expect(compareVersions('v2.2.4', '2.2.4')).toBe(0)
  })

  it('puts a release after its pre-releases', () => {
    expect(compareVersions('2.3.0', '2.3.0-beta.1')).toBeGreaterThan(0)
    expect(compareVersions('2.3.0-beta.2', '2.3.0-beta.10')).toBeLessThan(0)
  })

  it('is NaN for something that is not a version', () => {
    expect(compareVersions('dev', '2.2.4')).toBeNaN()
  })
})

describe('parseReleases', () => {
  it('splits the file per version with its date', () => {
    const releases = parseReleases(CHANGELOG)
    expect(releases.map((r) => r.version)).toEqual(['2.3.0', '2.2.5', '2.2.4', '2.2.3'])
    expect(releases[0].date).toBe('2026-10-20')
    expect(releases[0].sections.map((s) => s.title)).toEqual(['Sidebar', 'Bug Fixes'])
  })
})

describe('releasesBetween', () => {
  it('returns (previous, current], newest first', () => {
    const { releases, omitted } = releasesBetween(CHANGELOG, '2.2.4', '2.3.0')
    expect(releases.map((r) => r.version)).toEqual(['2.3.0', '2.2.5'])
    expect(omitted).toBe(0)
  })

  it('caps the list and counts what it left out', () => {
    const { releases, omitted } = releasesBetween(CHANGELOG, '2.0.0', '2.3.0', 2)
    expect(releases.map((r) => r.version)).toEqual(['2.3.0', '2.2.5'])
    expect(omitted).toBe(2)
  })

  it('is empty for a downgrade, the same version or an unknown one', () => {
    expect(releasesBetween(CHANGELOG, '2.3.0', '2.2.4').releases).toEqual([])
    expect(releasesBetween(CHANGELOG, '2.3.0', '2.3.0').releases).toEqual([])
    expect(releasesBetween(CHANGELOG, 'dev', '2.3.0').releases).toEqual([])
  })
})

describe('splitInlineCode', () => {
  it('marks backtick spans as code', () => {
    expect(splitInlineCode('Press `Esc` to stop')).toEqual([
      { text: 'Press ', code: false },
      { text: 'Esc', code: true },
      { text: ' to stop', code: false },
    ])
  })

  it('leaves an unclosed backtick alone', () => {
    expect(splitInlineCode('a ` b')).toEqual([{ text: 'a ` b', code: false }])
  })
})
