import { clsx } from 'clsx'
import { extendTailwindMerge } from 'tailwind-merge'
import { createTV } from 'tailwind-variants'

/**
 * `text-ui-*` are this app's font sizes (see the scale in app.css / DESIGN_SYSTEM
 * §2), but tailwind-merge has no way to know that: it sees `text-<something>` it
 * doesn't recognise and files it under *text colour*. It then resolves the two as
 * conflicting, so a base class list and a call-site override like
 *
 *   cn('bg-primary text-primary-foreground', 'text-ui-sm font-semibold')
 *
 * silently loses `text-primary-foreground` - the element falls back to inheriting
 * the ambient colour. That is how the "Add connection" button rendered as a blank
 * white slab on the dark themes: white-on-white, label and icon invisible.
 *
 * Registering the scale as font sizes makes size and colour independent again,
 * while still collapsing two sizes (or two colours) to the last one.
 */
const twMergeConfig = {
  extend: {
    classGroups: {
      'font-size': [
        { text: ['ui', 'ui-sm', 'ui-xs', 'ui-2xs', 'ui-3xs', 'ui-lg', 'ui-xl', 'ui-2xl', 'ui-3xl'] },
      ],
    },
  },
}
const twMerge = extendTailwindMerge(twMergeConfig)

/**
 * `tv()` for the ui/* variant components. tailwind-variants runs its own
 * tailwind-merge with the default config, so it had the same bug as above one
 * level down: a size variant's `text-ui-xs` knocked out the colour variant's
 * `text-primary-foreground`, and every `size="sm"` primary button rendered as a
 * blank slab on the dark themes. Same config, so both merges agree.
 */
export const tv = createTV({ twMergeConfig })

// The pool on this side is gone (closed by a reconnect, a disconnect, or a
// dropped peer) - the host itself may be perfectly reachable. Rebuilding the
// pool fixes it, so these heal in place instead of reading as "you are offline".
const POOL_LOST_PATTERNS = [
  'closed pool',             // sqlx: "attempted to acquire a connection on a closed pool"
  'pool has been closed',
  'poolclosed',
  'pool timed out',          // sqlx pool acquire timeout (idle/dead peer)
  'connection closed',
  'connection was closed',
  'server closed the connection',
  'terminating connection',  // Postgres idle-timeout / admin close
  'connection is closed',
  'no active connection',
]

const NETWORK_ERROR_PATTERNS = [
  'failed to lookup address',
  'nodename nor servname',
  'error communicating with database',
  'connection refused',
  'connection reset',
  'connection timed out',
  'connect timeout',
  'broken pipe',
  'network is unreachable',
  'no route to host',
  'network error',
  'socket error',
  'tcp connect error',
  'failed to connect',
  'unable to connect',
  'os error 61',   // ECONNREFUSED macOS
  'os error 111',  // ECONNREFUSED Linux
  'os error 110',  // ETIMEDOUT
  'os error 113',  // EHOSTUNREACH
  'os error 51',   // ENETUNREACH macOS (Wi‑Fi dropped)
  'os error 65',   // EHOSTUNREACH macOS
  'cannot reach',            // our own preflight message
  ...POOL_LOST_PATTERNS,
  // NB: deliberately NOT matching bare 'timed out' / 'i/o error' - those also fire
  // on a statement/lock timeout (a slow query, not a dropped connection), which
  // would spuriously flag connectionLost and churn the pool on every subsequent tap.
]

/**
 * Returns true when the error message indicates a network / connectivity problem
 * rather than a SQL or application error.
 * @param {string} msg
 */
export function isNetworkError(msg) {
  const lower = String(msg ?? '').toLowerCase()
  return NETWORK_ERROR_PATTERNS.some((p) => lower.includes(p))
}

/**
 * Split the connectivity errors into the two states worth telling apart:
 * `dropped` - the pool on this side died, one reconnect away from working;
 * `unreachable` - the host itself is not answering.
 * Returns null for SQL / application errors.
 * @param {string} msg
 * @returns {'dropped' | 'unreachable' | null}
 */
export function connectionErrorKind(msg) {
  const lower = String(msg ?? '').toLowerCase()
  if (POOL_LOST_PATTERNS.some((p) => lower.includes(p))) return 'dropped'
  return NETWORK_ERROR_PATTERNS.some((p) => lower.includes(p)) ? 'unreachable' : null
}

/**
 * The noun for a count: `plural(1, 'row')` is "row", anything else "rows".
 * Pass `many` for irregular nouns (`plural(n, 'index', 'indexes')`).
 * @param {number} n @param {string} one @param {string} [many]
 */
export function plural(n, one, many = one + 's') {
  return n === 1 ? one : many
}

/** @param {...import('clsx').ClassValue} inputs */
export function cn(...inputs) {
  return twMerge(clsx(inputs))
}

/**
 * Like `cn` but skips `tailwind-merge`'s conflict resolution. Use ONLY for hot
 * render paths (per-row / per-cell) whose class lists are authored to have no
 * conflicting utilities - it just joins, which is much cheaper at scale.
 * @param {...import('clsx').ClassValue} inputs
 */
export function cx(...inputs) {
  return clsx(inputs)
}

/**
 * Returns a debounced wrapper around `fn`. Trailing-edge: only the final call
 * within `wait` ms runs. Use to coalesce high-frequency work (localStorage
 * writes on keystroke/drag, etc.). Exposes `.cancel()` and `.flush()`.
 * @template {(...args: any[]) => void} F
 * @param {F} fn
 * @param {number} wait
 */
export function debounce(fn, wait) {
  /** @type {ReturnType<typeof setTimeout> | null} */
  let timer = null
  /** @type {any[] | null} */
  let lastArgs = null
  const debounced = /** @type {F & { cancel(): void, flush(): void }} */ (
    (...args) => {
      lastArgs = args
      if (timer) clearTimeout(timer)
      timer = setTimeout(() => {
        timer = null
        const a = lastArgs
        lastArgs = null
        if (a) fn(...a)
      }, wait)
    }
  )
  debounced.cancel = () => {
    if (timer) clearTimeout(timer)
    timer = null
    lastArgs = null
  }
  debounced.flush = () => {
    if (timer) clearTimeout(timer)
    timer = null
    if (lastArgs) { const a = lastArgs; lastArgs = null; fn(...a) }
  }
  return debounced
}
