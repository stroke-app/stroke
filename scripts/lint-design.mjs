#!/usr/bin/env node
// Design-system lint (DESIGN_SYSTEM.md), as a ratchet.
//
// The rules below are the ones that drifted: one-off corner radii and font
// sizes that bypass the token scale, which is how buttons ended up with five
// different corners. Existing violations are recorded per file in
// scripts/design-lint-baseline.json; a file may only go DOWN. A new violation
// (or a new file with any) fails, so the drift can't come back while the old
// ones are cleaned up at whatever pace.
//
//   node scripts/lint-design.mjs            # check (exit 1 on regressions)
//   node scripts/lint-design.mjs --update   # rewrite the baseline (after a cleanup)
//
// `src/lib/components/ui/*` is exempt: the primitives are where tokens are defined.
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const BASELINE = path.join(ROOT, 'scripts', 'design-lint-baseline.json')

/** @type {{ id: string, re: RegExp, why: string }[]} */
export const RULES = [
  { id: 'text-px', re: /(?<![\w-])text-\[\d[\w.]*\]/g, why: 'arbitrary font size; use the text-ui-* scale' },
  { id: 'text-tw', re: /(?<![\w-])text-(?:xs|sm|base|lg|xl|2xl|3xl)(?![\w-])/g, why: 'Tailwind font size bypasses the 14px base; use text-ui-*' },
  { id: 'rounded-px', re: /(?<![\w-])rounded(?:-[trbl]{1,2})?-\[\d+px\]/g, why: 'one-off radius; use rounded-sm / rounded-md / rounded-lg' },
  { id: 'rounded-xl', re: /(?<![\w-])rounded-xl(?![\w-])/g, why: 'not on the radius scale' },
  { id: 'h-px', re: /(?<![\w-])h-\[\d+px\]/g, why: 'arbitrary height; controls are h-7 / h-8 / h-9' },
]

function walk(dir, out = []) {
  for (const f of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, f.name)
    if (f.isDirectory()) {
      if (f.name === 'node_modules' || p.endsWith(path.join('components', 'ui'))) continue
      walk(p, out)
    } else if (/\.(svelte|js)$/.test(f.name) && !/\.test\.js$/.test(f.name)) out.push(p)
  }
  return out
}

/** Violations per file: { [relPath]: { [ruleId]: count } }, plus line hits for reporting. */
export function scan(root = ROOT) {
  /** @type {Record<string, Record<string, number>>} */
  const counts = {}
  /** @type {Record<string, { rule: string, line: number, text: string }[]>} */
  const hits = {}
  for (const file of walk(path.join(root, 'src'))) {
    const rel = path.relative(root, file).split(path.sep).join('/')
    const lines = fs.readFileSync(file, 'utf8').split('\n')
    lines.forEach((line, i) => {
      for (const r of RULES) {
        const m = line.match(r.re)
        if (!m) continue
        ;(counts[rel] ??= {})[r.id] = (counts[rel][r.id] ?? 0) + m.length
        ;(hits[rel] ??= []).push({ rule: r.id, line: i + 1, text: m.join(' ') })
      }
    })
  }
  return { counts, hits }
}

/** Files/rules whose count went up against the baseline. */
export function regressions(counts, baseline) {
  /** @type {{ file: string, rule: string, was: number, now: number }[]} */
  const out = []
  for (const [file, rules] of Object.entries(counts)) {
    for (const [rule, now] of Object.entries(rules)) {
      const was = baseline[file]?.[rule] ?? 0
      if (now > was) out.push({ file, rule, was, now })
    }
  }
  return out
}

export function readBaseline() {
  try {
    return JSON.parse(fs.readFileSync(BASELINE, 'utf8'))
  } catch {
    return {}
  }
}

const isCli = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
if (isCli) {
  const { counts, hits } = scan()
  if (process.argv.includes('--update')) {
    const sorted = Object.fromEntries(Object.keys(counts).sort().map((k) => [k, counts[k]]))
    fs.writeFileSync(BASELINE, JSON.stringify(sorted, null, 2) + '\n')
    const total = Object.values(counts).reduce((n, r) => n + Object.values(r).reduce((a, b) => a + b, 0), 0)
    console.log(`design-lint baseline written: ${total} known violations in ${Object.keys(counts).length} files`)
    process.exit(0)
  }
  const bad = regressions(counts, readBaseline())
  if (!bad.length) {
    console.log('design-lint: no new violations')
    process.exit(0)
  }
  const why = Object.fromEntries(RULES.map((r) => [r.id, r.why]))
  console.error('design-lint: new violations of DESIGN_SYSTEM.md\n')
  for (const b of bad) {
    console.error(`  ${b.file}  ${b.rule} ${b.was} -> ${b.now}  (${why[b.rule]})`)
    for (const h of hits[b.file].filter((h) => h.rule === b.rule)) console.error(`    :${h.line}  ${h.text}`)
  }
  console.error('\nFix them, or after a deliberate cleanup run: node scripts/lint-design.mjs --update')
  process.exit(1)
}
