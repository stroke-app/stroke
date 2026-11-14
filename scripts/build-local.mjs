#!/usr/bin/env node
/**
 * Stroke — local release builds
 * =============================
 * Build installers on your own machine instead of CI. Plain Node, no deps.
 *
 *   node scripts/build-local.mjs [target] [-- extra tauri build args]
 *
 * Targets:
 *   host      (default) whatever this machine builds natively
 *   mac       macOS, this Mac's architecture       → .app + .dmg      (macOS only)
 *   mac-x64   macOS Intel (x86_64-apple-darwin)     → .app + .dmg      (macOS only)
 *   windows   Windows x64 NSIS installer            → *-setup.exe
 *             native on Windows; cross-compiled with cargo-xwin on macOS/Linux
 *   linux     .deb / .rpm / .AppImage                                (Linux only)
 *
 * Examples:
 *   npm run tauri:build:win
 *   npm run tauri:build:local -- mac-x64
 *   npm run tauri:build:local -- windows -- --debug
 *
 * Cross-building Windows from macOS/Linux needs NSIS, LLVM (clang-cl, llvm-rc)
 * and cargo-xwin - linking uses the rust-lld that ships with Rust. The script
 * checks for each and says how to install what is missing. macOS and Linux
 * bundles can only be built on their own OS (Tauri needs the platform's webview
 * SDK), so those targets refuse elsewhere.
 *
 * DuckDB (bundled C++) does not compile under clang-cl as shipped: it marks
 * deleted functions `__declspec(dllexport)`, which clang rejects. Defining
 * DUCKDB_STATIC_BUILD empties that macro - correct here, as it links statically.
 * It goes in through `CL`, the one channel left: cargo-xwin overwrites
 * CXXFLAGS_<target>, and `_CL_` is appended after `--`, where clang-cl reads it
 * as a file name.
 */

import { spawnSync } from 'node:child_process'
import { existsSync, readdirSync, statSync } from 'node:fs'
import { delimiter, join, resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const HOST = process.platform // 'darwin' | 'win32' | 'linux'
const IS_WIN = HOST === 'win32'
const WIN_TRIPLE = 'x86_64-pc-windows-msvc'

const argv = process.argv.slice(2)
const sep = argv.indexOf('--')
const ownArgs = sep === -1 ? argv : argv.slice(0, sep)
const passthrough = sep === -1 ? [] : argv.slice(sep + 1)
const target = (ownArgs[0] ?? 'host').toLowerCase()

/** @param {string} msg */
function fail(msg) {
  console.error(`\n✖ ${msg}\n`)
  process.exit(1)
}

/** @param {string} cmd @param {NodeJS.ProcessEnv} env */
function has(cmd, env = process.env) {
  const r = spawnSync(IS_WIN ? 'where' : 'which', [cmd], { env, stdio: 'ignore' })
  return r.status === 0
}

/** @param {string} cmd @param {string[]} args @param {NodeJS.ProcessEnv} env */
function run(cmd, args, env = process.env) {
  console.log(`\n$ ${cmd} ${args.join(' ')}\n`)
  const r = spawnSync(cmd, args, { cwd: ROOT, env, stdio: 'inherit', shell: IS_WIN })
  if (r.status !== 0) process.exit(r.status ?? 1)
}

/** @param {string} triple */
function ensureRustTarget(triple) {
  const r = spawnSync('rustup', ['target', 'list', '--installed'], { encoding: 'utf8' })
  if (r.status !== 0) fail('rustup not found - install Rust from https://rustup.rs')
  if (!r.stdout.split('\n').includes(triple)) run('rustup', ['target', 'add', triple])
}

/** @param {string[]} tauriArgs @param {NodeJS.ProcessEnv} [env] */
function tauriBuild(tauriArgs, env) {
  run('npx', ['tauri', 'build', ...tauriArgs, ...passthrough], env)
}

/** Every installer/bundle under a bundle dir, one level of subfolders deep. @param {string} dir */
function listArtifacts(dir) {
  if (!existsSync(dir)) return []
  const out = []
  for (const kind of readdirSync(dir)) {
    const sub = join(dir, kind)
    if (!statSync(sub).isDirectory()) continue
    for (const f of readdirSync(sub)) {
      if (/\.(dmg|app|exe|msi|deb|rpm|AppImage)$/.test(f)) out.push(join(sub, f))
    }
  }
  return out
}

/** @param {string | null} triple */
function report(triple) {
  const dir = join(ROOT, 'src-tauri', 'target', ...(triple ? [triple] : []), 'release', 'bundle')
  const files = listArtifacts(dir)
  console.log('\n✔ Build finished.' + (files.length ? ' Artifacts:' : ''))
  for (const f of files) console.log(`  ${f}`)
}

function buildMac(/** @type {string | null} */ triple) {
  if (HOST !== 'darwin') fail('macOS bundles can only be built on macOS.')
  if (triple) ensureRustTarget(triple)
  tauriBuild(triple ? ['--target', triple] : [])
  report(triple)
}

function buildLinux() {
  if (HOST !== 'linux') {
    fail('Linux bundles can only be built on Linux (they link against webkit2gtk). Use a Linux machine or VM.')
  }
  tauriBuild([])
  // Same post-step as the release (issue #112): use the host's libwayland-client.
  for (const f of listArtifacts(join(ROOT, 'src-tauri', 'target', 'release', 'bundle'))) {
    if (f.endsWith('.AppImage')) run('bash', ['scripts/appimage-strip-host-libs.sh', f])
  }
  report(null)
}

function buildWindows() {
  if (IS_WIN) {
    tauriBuild([])
    report(null)
    return
  }

  const env = { ...process.env }
  // Homebrew's llvm is keg-only: installed, but not on PATH.
  if (HOST === 'darwin' && !has('clang-cl', env)) {
    const brewLlvm = ['/opt/homebrew/opt/llvm/bin', '/usr/local/opt/llvm/bin'].find((p) => existsSync(join(p, 'clang-cl')))
    if (brewLlvm) env.PATH = `${brewLlvm}${delimiter}${env.PATH ?? ''}`
  }

  const mac = HOST === 'darwin'
  const missing = [
    ['makensis', mac ? 'brew install nsis' : 'sudo apt install nsis'],
    ['clang-cl', mac ? 'brew install llvm' : 'sudo apt install clang llvm'],
    ['llvm-rc', mac ? 'brew install llvm' : 'sudo apt install llvm'],
    ['cargo-xwin', 'cargo install --locked cargo-xwin'],
  ].filter(([cmd]) => !has(cmd, env))
  if (missing.length) {
    const hints = [...new Set(missing.map(([, hint]) => hint))]
    fail(`Missing tools for the Windows cross-build: ${missing.map(([c]) => c).join(', ')}\n  Install with:\n    ${hints.join('\n    ')}`)
  }

  ensureRustTarget(WIN_TRIPLE)
  env.CL = [env.CL, '/DDUCKDB_STATIC_BUILD'].filter(Boolean).join(' ')
  // First run downloads the MSVC CRT + Windows SDK (~700MB) into the cargo-xwin
  // cache; cargo-xwin accepts the Microsoft license on your behalf.
  tauriBuild(['--runner', 'cargo-xwin', '--target', WIN_TRIPLE], env)
  report(WIN_TRIPLE)
}

switch (target) {
  case 'host':
    if (HOST === 'darwin') buildMac(null)
    else if (IS_WIN) buildWindows()
    else buildLinux()
    break
  case 'mac':
  case 'macos':
    buildMac(null)
    break
  case 'mac-x64':
    buildMac('x86_64-apple-darwin')
    break
  case 'win':
  case 'windows':
    buildWindows()
    break
  case 'linux':
    buildLinux()
    break
  default:
    fail(`Unknown target "${target}". Use one of: host, mac, mac-x64, windows, linux.`)
}
