#!/usr/bin/env node
// Per-command timings from the app's own perf log, and a regression check.
//
// Every IPC call is already logged as `perf: ipc <command> <N>ms` (commands.rs).
// This turns a session of normal use into a table of p50/p95 per command and,
// against a saved baseline, flags the ones that got slower - the "table opens
// were fast before" kind of regression, caught by a number instead of a feel.
//
//   npm run perf                      # report for the current log
//   npm run perf -- --save            # save this report as the baseline
//   npm run perf -- --check           # exit 1 when a command regressed
//   npm run perf -- --since 03:10     # only lines at/after this time (HH:MM, log time)
//   npm run perf -- --log path/to/Stroke.log
//
// The baseline is machine- and network-specific, so it lives outside the repo
// (.perf-baseline.json, gitignored). Compare like with like: same databases,
// same network, roughly the same things clicked.
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const BASELINE = path.join(ROOT, '.perf-baseline.json')
const APP_ID = 'com.broisnischal.stroke'
/** A command counts as regressed when its p50 is this many times the baseline's ... */
const RATIO = 1.5
/** ... and at least this much slower in absolute terms (noise on fast calls). */
const MIN_DELTA_MS = 100
/** Commands seen fewer times than this are reported but never judged. */
const MIN_SAMPLES = 3

function logDir() {
  const home = os.homedir()
  if (process.platform === 'darwin') return path.join(home, 'Library', 'Logs', APP_ID)
  if (process.platform === 'win32') return path.join(process.env.LOCALAPPDATA ?? path.join(home, 'AppData', 'Local'), APP_ID, 'logs')
  return path.join(process.env.XDG_DATA_HOME ?? path.join(home, '.local', 'share'), APP_ID, 'logs')
}

function arg(name) {
  const i = process.argv.indexOf(name)
  return i >= 0 ? process.argv[i + 1] : undefined
}

/** @param {string} text @param {string | undefined} since */
export function parse(text, since) {
  /** @type {Map<string, number[]>} */
  const samples = new Map()
  for (const line of text.split('\n')) {
    if (!line.includes('perf: ')) continue
    if (since) {
      const t = line.match(/\]\[(\d\d:\d\d(?::\d\d)?)\]/)?.[1]
      if (t && t < since) continue
    }
    for (const m of line.matchAll(/ipc ([a-z_][a-z0-9_]*) (\d+)ms( \(failed\))?/g)) {
      if (m[3]) continue // a failure's time says nothing about speed
      const list = samples.get(m[1]) ?? []
      list.push(Number(m[2]))
      samples.set(m[1], list)
    }
  }
  /** @type {Record<string, { n: number, p50: number, p95: number }>} */
  const stats = {}
  for (const [cmd, xs] of samples) {
    xs.sort((a, b) => a - b)
    const q = (/** @type {number} */ p) => xs[Math.min(xs.length - 1, Math.floor(p * xs.length))]
    stats[cmd] = { n: xs.length, p50: q(0.5), p95: q(0.95) }
  }
  return stats
}

/** @param {Record<string, {n:number,p50:number}>} now @param {Record<string, {n:number,p50:number}>} base */
export function compare(now, base) {
  /** @type {{ cmd: string, was: number, now: number }[]} */
  const worse = []
  for (const [cmd, s] of Object.entries(now)) {
    const b = base[cmd]
    if (!b || s.n < MIN_SAMPLES || b.n < MIN_SAMPLES) continue
    if (s.p50 > b.p50 * RATIO && s.p50 - b.p50 >= MIN_DELTA_MS) worse.push({ cmd, was: b.p50, now: s.p50 })
  }
  return worse
}

const isCli = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
if (isCli) {
  const logPath = arg('--log')
  const files = logPath
    ? [logPath]
    : (() => {
        const dir = logDir()
        try {
          return fs.readdirSync(dir).filter((f) => f.endsWith('.log')).map((f) => path.join(dir, f))
        } catch {
          return []
        }
      })()
  if (!files.length) {
    console.error(`No perf log found (looked in ${logPath ?? logDir()}). Use the app for a bit, or pass --log.`)
    process.exit(2)
  }
  const stats = parse(files.map((f) => fs.readFileSync(f, 'utf8')).join('\n'), arg('--since'))
  /** @type {Record<string, any>} */
  let base = {}
  try {
    base = JSON.parse(fs.readFileSync(BASELINE, 'utf8'))
  } catch {}

  const rows = Object.entries(stats).sort((a, b) => b[1].p50 * b[1].n - a[1].p50 * a[1].n)
  const pad = (/** @type {any} */ s, /** @type {number} */ w) => String(s).padStart(w)
  console.log(`${'command'.padEnd(36)}${pad('n', 5)}${pad('p50', 9)}${pad('p95', 9)}${pad('base p50', 11)}`)
  for (const [cmd, s] of rows) {
    const b = base[cmd]
    console.log(`${cmd.padEnd(36)}${pad(s.n, 5)}${pad(s.p50 + 'ms', 9)}${pad(s.p95 + 'ms', 9)}${pad(b ? b.p50 + 'ms' : '-', 11)}`)
  }

  if (process.argv.includes('--save')) {
    fs.writeFileSync(BASELINE, JSON.stringify(stats, null, 2) + '\n')
    console.log(`\nBaseline saved: ${path.relative(ROOT, BASELINE)}`)
  }
  const worse = compare(stats, base)
  if (worse.length) {
    console.log(`\nSlower than baseline (p50 > ${RATIO}x and +${MIN_DELTA_MS}ms):`)
    for (const w of worse) console.log(`  ${w.cmd}: ${w.was}ms -> ${w.now}ms`)
    if (process.argv.includes('--check')) process.exit(1)
  } else if (Object.keys(base).length) {
    console.log('\nNo regressions against the baseline.')
  }
}
