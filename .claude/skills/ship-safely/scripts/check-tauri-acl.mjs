#!/usr/bin/env node
// Cross-references the Tauri core APIs the frontend calls against the
// permissions src-tauri/capabilities/*.json actually grants, for every window
// label the app creates.
//
// This exists because of v2.1.0: the capability file granted
// `core:window:allow-close` but not `core:window:allow-destroy`. `close()` runs
// through to `destroy`, the ACL refused it, the rejected promise went nowhere,
// and the app shipped unquittable. A denied core call is silent in a release
// build, so it has to be caught here.

import { readFileSync, readdirSync, existsSync } from 'node:fs'
import { join } from 'node:path'
import { execSync } from 'node:child_process'

const root = process.argv[2] ?? process.cwd()
const manifestPath = join(root, 'src-tauri/gen/schemas/acl-manifests.json')
const capsDir = join(root, 'src-tauri/capabilities')

if (!existsSync(manifestPath)) {
  console.error(`No ACL manifest at ${manifestPath}.\nRun a Tauri build once (npm run tauri dev) to generate it, then re-run.`)
  process.exit(2)
}

const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'))

/** Expand one permission id into the `module:command` strings it allows. */
function expand(module, id, seen = new Set()) {
  const key = `${module}:${id}`
  if (seen.has(key)) return []
  seen.add(key)

  // A fully-qualified id re-targets the module: "core:window:allow-close".
  if (id.includes(':')) {
    const parts = id.split(':')
    const cmd = parts.pop()
    return expand(parts.join(':'), cmd, seen)
  }

  const mod = manifest[module]
  if (!mod) return []

  if (id === 'default') {
    const set = mod.default_permission
    if (!set) return []
    return (set.permissions ?? []).flatMap((p) => expand(module, p, seen))
  }

  const perm = mod.permissions?.[id]
  if (perm) return (perm.commands?.allow ?? []).map((c) => `${module}:${c}`)

  const set = mod.permission_sets?.[id]
  if (set) return (set.permissions ?? []).flatMap((p) => expand(module, p, seen))

  return []
}

// ── what the capabilities grant, per window label glob ──────────────────────
const capabilities = readdirSync(capsDir)
  .filter((f) => f.endsWith('.json'))
  .map((f) => JSON.parse(readFileSync(join(capsDir, f), 'utf8')))

const grants = capabilities.map((cap) => {
  const allowed = new Set()
  for (const p of cap.permissions ?? []) {
    const id = typeof p === 'string' ? p : p.identifier
    if (!id) continue
    for (const c of expand('core', id)) allowed.add(c)
    // Non-core plugins live under their own manifest key ("dialog", "opener").
    const head = id.split(':')[0]
    if (head !== 'core' && manifest[head]) {
      for (const c of expand(head, id.split(':').slice(1).join(':') || 'default')) allowed.add(c)
    }
  }
  return { windows: cap.windows ?? ['main'], allowed, identifier: cap.identifier }
})

const labelAllows = (glob, label) =>
  new RegExp('^' + glob.split('*').map((s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('.*') + '$').test(label)

// ── what the frontend calls ─────────────────────────────────────────────────
// JS method -> core command(s). `close` carries `destroy`: Tauri's close runs
// through to destroy once nothing prevents it, and destroy is its own gate.
const IMPLIED = { close: ['close', 'destroy'] }
const camelToSnake = (s) => s.replace(/[A-Z]/g, (c) => '_' + c.toLowerCase())

let hits = ''
try {
  hits = execSync(
    `grep -rhoE '(getCurrentWindow|getCurrentWebviewWindow|appWindow)\\(?\\)?\\.[a-zA-Z]+' ${join(root, 'src')} || true`,
    { encoding: 'utf8' },
  )
} catch { /* grep found nothing */ }

const used = new Set()
for (const line of hits.split('\n')) {
  const m = line.match(/\.([a-zA-Z]+)$/)
  if (!m) continue
  const js = m[1]
  for (const cmd of IMPLIED[js] ?? [camelToSnake(js)]) used.add(cmd)
}

// Window labels the backend creates, so second windows are checked too.
let labels = ['main']
try {
  const commands = readFileSync(join(root, 'src-tauri/src/commands.rs'), 'utf8')
  const m = commands.match(/format!\("(main-\{[a-z]+\})"\)/)
  if (m) labels.push('main-2')
} catch { /* no secondary windows */ }

// ── report ──────────────────────────────────────────────────────────────────
const problems = []
for (const label of labels) {
  const forLabel = grants.filter((g) => g.windows.some((w) => labelAllows(w, label)))
  if (forLabel.length === 0) {
    problems.push(`window "${label}" is covered by no capability at all - every core call from it is denied`)
    continue
  }
  const allowed = new Set(forLabel.flatMap((g) => [...g.allowed]))
  for (const cmd of [...used].sort()) {
    if (!allowed.has(`core:window:${cmd}`) && !allowed.has(`core:webview:${cmd}`)) {
      problems.push(`window "${label}" calls ${cmd}() but no capability grants core:window:allow-${cmd.replace(/_/g, '-')}`)
    }
  }
}

if (problems.length) {
  console.error('Tauri ACL gaps:\n' + problems.map((p) => `  ✗ ${p}`).join('\n'))
  console.error('\nAdd the permission to src-tauri/capabilities/default.json, or widen its "windows" globs.')
  process.exit(1)
}

console.log(`Tauri ACL ok - ${used.size} core window call(s) checked across ${labels.length} window label(s): ${labels.join(', ')}`)
