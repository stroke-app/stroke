import path from 'node:path'
import { parseArgs } from 'node:util'
import { findConnection, engineOf, redact, URL_KEYS } from './find.js'
import { installedScheme, appLink, openUrl } from './app.js'
import { openEngine } from './engines.js'
import { startBridge, studioUrl, STUDIO_ORIGIN } from './bridge.js'

const HELP = `strokedb - open your project's database in Stroke

Usage
  npx strokedb [connection]        Desktop app if installed, else the web studio
  npx strokedb --web [connection]  Always the web studio

  connection   A URL (postgres://, mysql://, ...) or a SQLite file. Left out,
               it's read from ${URL_KEYS.slice(0, 3).join(', ')}, ... in the
               environment or .env files, or a .db/.sqlite file in this folder.

Options
  --web            Use the web studio even if the app is installed
  --app            Use the desktop app; fail if it isn't installed
  --name <name>    Name for the connection (default: this folder's name)
  --port <port>    Port for the local bridge (default: any free port)
  --no-open        Print the studio link instead of opening a browser
  -h, --help       Show this help
  -v, --version    Show the version

The web studio talks to a bridge on 127.0.0.1 that only answers the tab this
command opens. It stops when you press Ctrl+C.`

/** @param {string[]} argv */
export async function main(argv) {
  const { values, positionals } = parseArgs({
    args: argv,
    allowPositionals: true,
    options: {
      web: { type: 'boolean' },
      app: { type: 'boolean' },
      name: { type: 'string' },
      port: { type: 'string' },
      'no-open': { type: 'boolean' },
      help: { type: 'boolean', short: 'h' },
      version: { type: 'boolean', short: 'v' },
    },
  })
  if (values.help) return void console.log(HELP)
  if (values.version) {
    const { default: pkg } = await import('../package.json', { with: { type: 'json' } })
    return void console.log(pkg.version)
  }

  const cwd = process.cwd()
  const found = findConnection({ arg: positionals[0], cwd })
  if (!found) {
    console.error(`No database found here.\n\nPass one (npx strokedb postgres://user:pass@host/db), set DATABASE_URL in .env,\nor run this in a folder with a .db or .sqlite file.`)
    return 1
  }
  const name = values.name ?? path.basename(cwd)
  console.log(`Database: ${redact(found.uri)}  (from ${found.source})`)

  if (!values.web) {
    const scheme = installedScheme()
    if (scheme) {
      const ok = await openUrl(appLink(scheme, found.uri, name))
      if (ok) {
        console.log(`Opened in ${scheme === 'stroke' ? 'Stroke' : 'Stroke Nightly'}. Check the details and press Connect.`)
        return 0
      }
      console.error('Could not hand the link to the app; using the web studio instead.')
    } else if (values.app) {
      console.error('The Stroke app is not installed (or is older than the version with stroke:// links).\nGet it at https://stroke.click/download')
      return 1
    }
  }

  const engine = engineOf(found.uri)
  if (!engine) {
    console.error(`The web studio speaks PostgreSQL, MySQL and SQLite. For this database, use the app: https://stroke.click/download`)
    return 1
  }
  quietSqliteWarning()
  let db
  try {
    db = await openEngine(engine, found.uri)
  } catch (e) {
    console.error(`Could not connect: ${e instanceof Error ? e.message : e}`)
    return 1
  }

  const studio = process.env.STROKE_STUDIO_URL || `${STUDIO_ORIGIN}/studio`
  const bridge = await startBridge({
    engine: db,
    name,
    origins: [new URL(studio).origin],
    port: values.port ? Number(values.port) : 0,
  })
  const link = studioUrl({ studio, port: bridge.port, token: bridge.token })
  const opened = !values['no-open'] ? await openUrl(link) : false
  console.log(`\n${opened ? 'Opened' : 'Open'} the studio${opened ? '' : ' at'}:\n  ${link}\n\nThe link is the key to this session; don't share it. Press Ctrl+C to stop.`)

  await new Promise((resolve) => {
    const stop = () => resolve(undefined)
    process.once('SIGINT', stop)
    process.once('SIGTERM', stop)
  })
  await bridge.close()
  await db.close().catch(() => {})
  console.log('\nStopped.')
  return 0
}

/** node:sqlite is marked experimental and says so on every run; it's fine here. */
function quietSqliteWarning() {
  const emit = process.emitWarning.bind(process)
  process.emitWarning = /** @type {typeof process.emitWarning} */ ((w, ...rest) => {
    const msg = typeof w === 'string' ? w : w?.message
    if (/SQLite is an experimental feature/.test(msg ?? '')) return
    return emit(/** @type {any} */ (w), .../** @type {any[]} */ (rest))
  })
}
