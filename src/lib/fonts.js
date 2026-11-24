/**
 * Faces for Settings > Appearance > Separate fonts: one face each for the
 * interface, the SQL editor, the data grid, the AI chat and the terminal.
 *
 * Two kinds of face. Bundled ones ship with the app (fontsource imports in
 * app.css) and look the same on every OS. System ones belong to the OS: SF Pro
 * and SF Mono on a Mac, Segoe UI and Cascadia Mono on Windows, the desktop's
 * own faces on Linux. A system face is only offered where the OS has it
 * (`isInstalled`), so the list on each machine is what that machine can draw.
 */
import { detectOs } from '$lib/platform.js'

/** @typedef {'sans' | 'mono' | 'serif'} FaceKind */
/** @typedef {{ id: string, label: string, kind: FaceKind, stack: string, os?: ('macos' | 'windows' | 'linux')[], probe?: string }} Face */

/** @type {Face[]} */
export const FACES = [
  // The OS interface face first: what native apps on this machine use.
  { id: 'system', label: 'System UI', kind: 'sans', stack: '-apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif' },
  { id: 'inter', label: 'Inter', kind: 'sans', stack: '"Inter Variable", ui-sans-serif, system-ui, sans-serif' },
  { id: 'geist', label: 'Geist', kind: 'sans', stack: '"Geist Variable", ui-sans-serif, system-ui, sans-serif' },
  { id: 'plex-sans', label: 'IBM Plex Sans', kind: 'sans', stack: '"IBM Plex Sans", ui-sans-serif, system-ui, sans-serif' },
  { id: 'space-grotesk', label: 'Space Grotesk', kind: 'sans', stack: '"Space Grotesk Variable", ui-sans-serif, system-ui, sans-serif' },
  { id: 'helvetica', label: 'Helvetica Neue', kind: 'sans', stack: '"Helvetica Neue", Helvetica, Arial, sans-serif', os: ['macos'], probe: 'Helvetica Neue' },
  { id: 'avenir', label: 'Avenir Next', kind: 'sans', stack: '"Avenir Next", Avenir, sans-serif', os: ['macos'], probe: 'Avenir Next' },
  { id: 'segoe', label: 'Segoe UI', kind: 'sans', stack: '"Segoe UI Variable Text", "Segoe UI", sans-serif', os: ['windows'], probe: 'Segoe UI' },
  { id: 'ubuntu', label: 'Ubuntu', kind: 'sans', stack: 'Ubuntu, sans-serif', os: ['linux'], probe: 'Ubuntu' },
  { id: 'cantarell', label: 'Cantarell', kind: 'sans', stack: 'Cantarell, sans-serif', os: ['linux'], probe: 'Cantarell' },
  { id: 'noto-sans', label: 'Noto Sans', kind: 'sans', stack: '"Noto Sans", sans-serif', os: ['linux', 'windows'], probe: 'Noto Sans' },
  { id: 'new-york', label: 'New York', kind: 'serif', stack: 'ui-serif, "New York", "Iowan Old Style", Georgia, serif', os: ['macos'] },
  { id: 'source-serif', label: 'Source Serif', kind: 'serif', stack: '"Source Serif 4 Variable", ui-serif, Georgia, serif' },
  { id: 'georgia', label: 'Georgia', kind: 'serif', stack: 'Georgia, serif', probe: 'Georgia' },

  // Monospace. The OS one first.
  { id: 'system-mono', label: 'System Mono', kind: 'mono', stack: 'ui-monospace, "SF Mono", SFMono-Regular, "Cascadia Mono", Consolas, "DejaVu Sans Mono", monospace' },
  { id: 'jetbrains-mono', label: 'JetBrains Mono', kind: 'mono', stack: '"JetBrains Mono Variable", ui-monospace, monospace' },
  { id: 'geist-mono', label: 'Geist Mono', kind: 'mono', stack: '"Geist Mono Variable", ui-monospace, monospace' },
  { id: 'fira-code', label: 'Fira Code', kind: 'mono', stack: '"Fira Code Variable", ui-monospace, monospace' },
  { id: 'plex-mono', label: 'IBM Plex Mono', kind: 'mono', stack: '"IBM Plex Mono", ui-monospace, monospace' },
  { id: 'source-code', label: 'Source Code Pro', kind: 'mono', stack: '"Source Code Pro Variable", ui-monospace, monospace' },
  { id: 'menlo', label: 'Menlo', kind: 'mono', stack: 'Menlo, monospace', os: ['macos'], probe: 'Menlo' },
  { id: 'monaco', label: 'Monaco', kind: 'mono', stack: 'Monaco, monospace', os: ['macos'], probe: 'Monaco' },
  { id: 'cascadia', label: 'Cascadia Mono', kind: 'mono', stack: '"Cascadia Mono", "Cascadia Code", monospace', os: ['windows'], probe: 'Cascadia Mono' },
  { id: 'consolas', label: 'Consolas', kind: 'mono', stack: 'Consolas, monospace', os: ['windows'], probe: 'Consolas' },
  { id: 'dejavu-mono', label: 'DejaVu Sans Mono', kind: 'mono', stack: '"DejaVu Sans Mono", monospace', os: ['linux'], probe: 'DejaVu Sans Mono' },
  { id: 'ubuntu-mono', label: 'Ubuntu Mono', kind: 'mono', stack: '"Ubuntu Mono", monospace', os: ['linux'], probe: 'Ubuntu Mono' },
  { id: 'noto-mono', label: 'Noto Sans Mono', kind: 'mono', stack: '"Noto Sans Mono", monospace', os: ['linux'], probe: 'Noto Sans Mono' },
]

const BY_ID = new Map(FACES.map((f) => [f.id, f]))

/** What the OS calls its own faces, so the list says "SF Pro" on a Mac. */
export function systemLabels(os = detectOs()) {
  return {
    system: os === 'macos' ? 'SF Pro' : os === 'windows' ? 'Segoe UI' : 'System UI',
    'system-mono': os === 'macos' ? 'SF Mono' : os === 'windows' ? 'Cascadia Mono' : 'System Mono',
  }
}

/**
 * Whether a named font is installed: text drawn in it measures differently from
 * the same text in each generic fallback. Bundled faces are always there.
 * @param {string} family
 */
export function isInstalled(family) {
  if (typeof document === 'undefined') return true
  const ctx = document.createElement('canvas').getContext('2d')
  if (!ctx) return true
  const sample = 'mmmmmmmmmmlli10OQ@#'
  return ['monospace', 'serif', 'sans-serif'].some((fallback) => {
    ctx.font = `72px ${fallback}`
    const base = ctx.measureText(sample).width
    ctx.font = `72px "${family}", ${fallback}`
    return ctx.measureText(sample).width !== base
  })
}

/**
 * Faces to offer on this machine for a surface: bundled ones, plus the OS's
 * own faces it actually has. `kinds` picks proportional, mono or both.
 * @param {FaceKind[]} kinds
 * @param {{ os?: string, installed?: (family: string) => boolean }} [o]
 */
export function facesFor(kinds, { os = detectOs(), installed = isInstalled } = {}) {
  const labels = /** @type {Record<string, string>} */ (systemLabels(/** @type {any} */ (os)))
  return FACES.filter((f) => kinds.includes(f.kind))
    .filter((f) => !f.os || f.os.includes(/** @type {any} */ (os)))
    .filter((f) => !f.probe || installed(f.probe))
    .map((f) => ({ ...f, label: labels[f.id] ?? f.label }))
}

/** A face's CSS stack, or null for "follow the theme font". @param {string | null | undefined} id */
export function faceStack(id) {
  return (id && BY_ID.get(id)?.stack) || null
}

/** Surfaces that can take their own face, and the CSS variable each reads. */
export const FONT_SURFACES = /** @type {const} */ ([
  { key: 'ui', cssVar: '--font-sans', kinds: ['sans', 'serif'] },
  { key: 'code', cssVar: '--editor-font-family', kinds: ['mono'] },
  { key: 'grid', cssVar: '--grid-font', kinds: ['mono', 'sans'] },
  { key: 'chat', cssVar: '--reading-font', kinds: ['sans', 'serif'] },
  { key: 'terminal', cssVar: '--terminal-font', kinds: ['mono'] },
])

/** @typedef {{ separate: boolean, ui: string, code: string, grid: string, chat: string, terminal: string, smoothing: 'grayscale' | 'system' }} FontSettings */

/** @type {FontSettings} */
export const DEFAULT_FONT_SETTINGS = { separate: false, ui: '', code: '', grid: '', chat: '', terminal: '', smoothing: 'system' }

/** @param {unknown} v @returns {FontSettings} */
export function normalizeFontSettings(v) {
  const o = /** @type {Record<string, unknown>} */ (v && typeof v === 'object' ? v : {})
  const face = (/** @type {unknown} */ id) => (typeof id === 'string' && BY_ID.has(id) ? id : '')
  return {
    separate: o.separate === true,
    ui: face(o.ui),
    code: face(o.code),
    grid: face(o.grid),
    chat: face(o.chat),
    terminal: face(o.terminal),
    smoothing: o.smoothing === 'grayscale' ? 'grayscale' : 'system',
  }
}
