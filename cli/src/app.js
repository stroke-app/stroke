// Is the Stroke desktop app installed, with stroke:// links? Older builds have
// no link handler, so "installed" means the scheme is registered, not just that
// the app is there - opening an unhandled link fails with an OS error dialog.

import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { execFileSync, spawn } from 'node:child_process'

/** Stable first: a link to it opens the app most people have. */
export const SCHEMES = ['stroke', 'stroke-nightly']

const MAC_APPS = { stroke: 'Stroke.app', 'stroke-nightly': 'Stroke Nightly.app' }

/**
 * @param {string} cmd
 * @param {string[]} args
 */
function run(cmd, args) {
  try {
    return execFileSync(cmd, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'], timeout: 5000 })
  } catch {
    return ''
  }
}

/** @param {string} scheme */
function handlesScheme(scheme) {
  switch (process.platform) {
    case 'darwin': {
      const name = MAC_APPS[/** @type {keyof typeof MAC_APPS} */ (scheme)]
      for (const dir of ['/Applications', path.join(os.homedir(), 'Applications')]) {
        try {
          const plist = fs.readFileSync(path.join(dir, name, 'Contents', 'Info.plist'), 'utf8')
          if (plist.includes(`<string>${scheme}</string>`)) return true
        } catch {
          // not there
        }
      }
      return false
    }
    case 'win32':
      return /URL Protocol/i.test(run('reg', ['query', `HKCU\\Software\\Classes\\${scheme}`])) ||
        /URL Protocol/i.test(run('reg', ['query', `HKCR\\${scheme}`]))
    default:
      return run('xdg-mime', ['query', 'default', `x-scheme-handler/${scheme}`]).trim() !== ''
  }
}

/** The first scheme an installed Stroke answers to, or null. */
export function installedScheme() {
  return SCHEMES.find(handlesScheme) ?? null
}

/**
 * @param {string} scheme
 * @param {string} uri
 * @param {string} [name]
 */
export function appLink(scheme, uri, name) {
  const q = new URLSearchParams({ url: uri })
  if (name) q.set('name', name)
  return `${scheme}://open?${q}`
}

/**
 * Hand a URL to the OS: the browser for https, the app for stroke://.
 * @param {string} url
 */
export function openUrl(url) {
  const [cmd, args] =
    process.platform === 'darwin' ? ['open', [url]]
    // `start` reads `&` as a command separator; rundll32 takes the URL whole.
    : process.platform === 'win32' ? ['rundll32', ['url.dll,FileProtocolHandler', url]]
    : ['xdg-open', [url]]
  return new Promise((resolve) => {
    try {
      const child = spawn(cmd, args, { stdio: 'ignore', detached: true })
      child.on('error', () => resolve(false))
      child.on('spawn', () => {
        child.unref()
        resolve(true)
      })
    } catch {
      resolve(false)
    }
  })
}
