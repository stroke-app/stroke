// Bump the app version and fold .changeset/*.md into CHANGELOG.md.
//
//   BUMP_TYPE=patch|minor|major node scripts/release-bump.cjs
//
// Patch versions run .1-.9; the patch after .9 is the next minor (2.5.9 -> 2.6.0).
// Writes new_version=X.Y.Z to $GITHUB_OUTPUT when set. Used by auto-release.yml
// (every PR merged into nightly) and runnable by hand.
const fs   = require('fs')
const path = require('path')
const bump = process.env.BUMP_TYPE

// ── Compute new version ──────────────────────────────────────────
const cfgPath = 'src-tauri/tauri.conf.json'
const cfg     = JSON.parse(fs.readFileSync(cfgPath, 'utf8'))
const [maj, min, pat] = cfg.version.split('.').map(Number)

// Patch versions run .1 to .9; the next patch after .9 is the next
// minor (2.3.9 -> 2.4.0), so a patch never reaches two digits.
const next =
  bump === 'major' ? `${maj + 1}.0.0` :
  bump === 'minor' || pat >= 9 ? `${maj}.${min + 1}.0` :
                     `${maj}.${min}.${pat + 1}`

// ── Update tauri.conf.json ───────────────────────────────────────
cfg.version = next
fs.writeFileSync(cfgPath, JSON.stringify(cfg, null, 2) + '\n')

// ── Update package.json ──────────────────────────────────────────
const pkgPath = 'package.json'
const pkg     = JSON.parse(fs.readFileSync(pkgPath, 'utf8'))
pkg.version   = next
fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n')

// ── Collect .changeset/*.md files ────────────────────────────────
const csDir = '.changeset'
const files = fs.existsSync(csDir)
  ? fs.readdirSync(csDir).filter(f => f.endsWith('.md') && f !== 'README.md')
  : []

// sections: { 'New Features': { '#### Canvas Table': ['- item'] } }
/** @type {Record<string, Record<string, string[]>>} */
const sections = {}

for (const file of files) {
  const content = fs.readFileSync(path.join(csDir, file), 'utf8')
  let h3 = null
  let h4 = null

  for (const line of content.split('\n')) {
    const t = line.trim()
    if (t.startsWith('### ')) {
      h3 = t.slice(4).trim()
      if (!sections[h3]) sections[h3] = {}
      h4 = null
    } else if (t.startsWith('#### ')) {
      h4 = t   // keep full "#### Foo"
      if (h3 && !sections[h3][h4]) sections[h3][h4] = []
    } else if (/^[-*]\s/.test(t) && h3) {
      const key = h4 ?? '__top__'
      if (!sections[h3][key]) sections[h3][key] = []
      sections[h3][key].push(t)
    }
  }
}

// ── Build the new CHANGELOG entry ────────────────────────────────
const today    = new Date().toISOString().slice(0, 10)
const order    = ['New Features', 'Bug Fixes', 'Changes']
const allKeys  = [...new Set([...order, ...Object.keys(sections)])]
let entry      = `## [${next}] - ${today}\n`
let hasContent = false

for (const heading of allKeys) {
  if (!sections[heading] || Object.keys(sections[heading]).length === 0) continue
  hasContent = true
  entry += `\n### ${heading}\n`

  // Top-level bullets (no sub-section)
  const top = sections[heading]['__top__'] ?? []
  for (const item of top) entry += `${item}\n`

  // Sub-sections
  for (const [sub, items] of Object.entries(sections[heading])) {
    if (sub === '__top__') continue
    entry += `\n${sub}\n`
    for (const item of items) entry += `${item}\n`
  }
}

if (!hasContent) {
  // No changeset files — generate a minimal entry from PR title
  const pr = process.env.PR_TITLE || 'No changelog provided'
  const num = process.env.PR_NUMBER
  entry += `\n### Changes\n- ${pr}${num ? ` (#${num})` : ''}\n`
}

// ── Inject at top of CHANGELOG.md (after the "---" separator) ───
const clPath = 'CHANGELOG.md'
const cl     = fs.existsSync(clPath) ? fs.readFileSync(clPath, 'utf8') : '# Changelog\n\n---\n'
const sep    = '---\n'
const idx    = cl.indexOf(sep)
const before = idx !== -1 ? cl.slice(0, idx + sep.length) : cl
const after  = idx !== -1 ? cl.slice(idx + sep.length)    : ''
fs.writeFileSync(clPath, `${before}\n${entry}\n${after}`)

// ── Delete consumed changeset files ──────────────────────────────
for (const file of files) fs.unlinkSync(path.join(csDir, file))

console.log(`v${next} — merged ${files.length} changeset(s)`)
if (process.env.GITHUB_OUTPUT) fs.appendFileSync(process.env.GITHUB_OUTPUT, `new_version=${next}\n`)
