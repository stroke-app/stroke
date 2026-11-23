#!/usr/bin/env node
// Release notes for a version: every CHANGELOG.md section since the previous
// stable tag, merged by heading (src/lib/release-notes.js).
//
//   node scripts/release-notes.mjs 3.7.5            # GitHub release body
//   node scripts/release-notes.mjs 3.7.5 --plain    # latest.json (no bold)
//
// A pre-release version gets its own section only. The previous stable tag
// comes from `git ls-remote --tags origin`; RELEASE_NOTES_SINCE overrides it.
// `.github/release-highlights/<version>.md`, when present, goes on top of the
// GitHub body (not the plain one: the update dialog lists changes, not prose).

import fs from 'node:fs'
import { execFileSync } from 'node:child_process'
import { collectNotes, previousStable } from '../src/lib/release-notes.js'

const version = String(process.argv[2] ?? '').replace(/^v/, '')
const plain = process.argv.includes('--plain')
if (!version) {
  console.error('usage: release-notes.mjs <version> [--plain]')
  process.exit(2)
}

const changelog = fs.readFileSync('CHANGELOG.md', 'utf8')

let since = process.env.RELEASE_NOTES_SINCE || null
if (!since && !version.includes('-')) {
  try {
    const out = execFileSync('git', ['ls-remote', '--tags', '--refs', 'origin'], { encoding: 'utf8' })
    const tags = out.split('\n').map((l) => l.split('refs/tags/')[1]).filter(Boolean)
    since = previousStable(tags, version)
  } catch (e) {
    console.error(`release-notes: could not list tags, using ${version} alone: ${e instanceof Error ? e.message : e}`)
  }
}

let notes = collectNotes(changelog, version, since)
if (plain) {
  // Apps before 3.6.5 print markdown bold as asterisks; newer ones read
  // `Title - text` without them (changelog.js splitEntry).
  notes = notes.replace(/\*\*([^*\n]+)\*\*/g, '$1')
} else {
  const highlights = `.github/release-highlights/${version}.md`
  if (fs.existsSync(highlights)) notes = `${fs.readFileSync(highlights, 'utf8').trim()}\n\n${notes}`
  if (since && notes) notes += `\n\n_Everything since v${since}._`
}
process.stdout.write(notes)
