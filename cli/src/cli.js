import path from 'node:path'
import { parseArgs } from 'node:util'
import { findConnection, engineOf, redact, URL_KEYS } from './find.js'
import { installedScheme, appLink, openUrl } from './app.js'
import { openEngine } from './engines.js'
import { startBridge, studioUrl, makeRoutes, STUDIO_ORIGIN } from './bridge.js'
import { newShare, shareUrl, startRelayHost } from './relay.js'

const HELP = `strokedb - open your project's database in Stroke

Usage
  npx strokedb [connection]        Desktop app if installed, else the web studio
  npx strokedb --web [connection]  Always the web studio
  npx strokedb mcp [connection]    An MCP server for AI agents (see below)

  connection   A URL (postgres://, mysql://, ...) or a SQLite file. Left out,
               it's read from ${URL_KEYS.slice(0, 3).join(', ')}, ... in the
               environment or .env files, or a .db/.sqlite file in this folder.

Options
  --web            Use the web studio even if the app is installed
  --app            Use the desktop app; fail if it isn't installed
  --name <name>    Name for the connection (default: this folder's name)
  --port <port>    Port for the local bridge (default: any free port)
  --no-open        Print the studio link instead of opening a browser
  --share          (Labs) Also print a link that opens this studio on any
                   device, through stroke.click's relay. Read-only: browse
                   and run SELECTs. End-to-end encrypted; the key is only in
                   the link. Needs Node 22+.
  --write          With --share or mcp: allow edits (read-only otherwise)
  --table <table>  Open on this table (schema.table or just its name)
  --json           Print the links as one JSON line, for scripts and agents
  -h, --help       Show this help
  -v, --version    Show the version

The web studio talks to a bridge on 127.0.0.1 that only answers the tab this
command opens. It stops when you press Ctrl+C.

AI agents
  An agent on another machine (a cloud box, a container) can show you the
  database. Add the MCP server to it once:

    claude mcp add strokedb -- npx -y strokedb mcp

  It reads the database (list_tables, describe_table, query) and its
  show_table tool hands you a link that opens a table in the web studio or
  the Stroke app; later calls switch your open studio to the next table.
  Without MCP: npx strokedb --share --no-open --json --table users`

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
      share: { type: 'boolean' },
      write: { type: 'boolean' },
      table: { type: 'string' },
      json: { type: 'boolean' },
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
  const mcp = positionals[0] === 'mcp'
  const found = findConnection({ arg: positionals[mcp ? 1 : 0], cwd })
  if (mcp) {
    // stdout belongs to the protocol: everything else goes to stderr.
    if (!found) {
      console.error('strokedb mcp: no database found. Pass one (npx strokedb mcp postgres://...), or set DATABASE_URL.')
      return 1
    }
    const engine = engineOf(found.uri)
    if (!engine) {
      console.error('strokedb mcp: PostgreSQL, MySQL and SQLite only.')
      return 1
    }
    quietSqliteWarning()
    console.error(`strokedb mcp: ${redact(found.uri)} (from ${found.source})${values.write ? ', writable' : ', read-only'}`)
    const { runMcp } = await import('./mcp.js')
    await runMcp({
      uri: found.uri,
      engine,
      name: values.name ?? path.basename(cwd),
      write: !!values.write,
      studio: process.env.STROKE_STUDIO_URL || `${STUDIO_ORIGIN}/studio`,
    })
    return 0
  }
  if (!found) {
    console.error(`No database found here.\n\nPass one (npx strokedb postgres://user:pass@host/db), set DATABASE_URL in .env,\nor run this in a folder with a .db or .sqlite file.`)
    return 1
  }
  const name = values.name ?? path.basename(cwd)
  // --json keeps stdout to the one line; the rest goes to stderr.
  const say = values.json ? (/** @type {string} */ m) => console.error(m) : (/** @type {string} */ m) => console.log(m)
  say(`Database: ${redact(found.uri)}  (from ${found.source})`)
  const openTable = values.table?.trim() || ''
  /** The table to open first, as a fragment parameter. */
  const withTable = (/** @type {string} */ link) => (openTable ? `${link}&${new URLSearchParams({ open: openTable })}` : link)

  if (values.write && !values.share && !mcp) {
    console.error('--write goes with --share.')
    return 1
  }
  // Sharing keeps this process running, so it uses the web studio here too.
  if (!values.web && !values.share) {
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
  const link = withTable(studioUrl({ studio, port: bridge.port, token: bridge.token }))
  const opened = !values['no-open'] ? await openUrl(link) : false
  say(`\n${opened ? 'Opened' : 'Open'} the studio${opened ? '' : ' at'}:\n  ${link}\n\nThe link is the key to this session; don't share it. Press Ctrl+C to stop.`)
  /** @type {Record<string, string>} */
  const out = { database: redact(found.uri), studio: link }

  /** @type {{ close(): void } | null} */
  let relay = null
  /** @type {import('./engines.js').Engine | null} */
  let sharedDb = null
  if (values.share) {
    try {
      // Its own connection, read-only unless --write: the engine itself
      // refuses writes, not just the routes.
      sharedDb = values.write ? db : await openEngine(engine, found.uri, { readOnly: true })
      const share = newShare()
      relay = await startRelayHost({
        routes: makeRoutes({ engine: sharedDb, name, shared: true, write: !!values.write }),
        ...share,
        base: new URL(studio).origin,
        onStatus: (status, detail) => say(status === 'online' ? '  Sharing: online' : `  Sharing: ${detail ?? 'offline'}`),
      })
      const shared = withTable(shareUrl({ studio, ...share }))
      out.share = shared
      out.app = `stroke://studio?${new URLSearchParams({ relay: share.room, key: share.key, ...(openTable ? { open: openTable } : {}) })}`
      say(`\nShare link (Labs, ${values.write ? 'can edit' : 'read-only'}):\n  ${shared}\n\nIn the Stroke app:\n  ${out.app}\n\nAnyone with this link can ${values.write ? 'read and change' : 'read'} this database while this runs. It's\nend-to-end encrypted: the key is only in the link, stroke.click sees ciphertext.`)
    } catch (e) {
      console.error(`Could not start sharing: ${e instanceof Error ? e.message : e}`)
    }
  }

  if (values.json) console.log(JSON.stringify(out))

  await new Promise((resolve) => {
    const stop = () => resolve(undefined)
    process.once('SIGINT', stop)
    process.once('SIGTERM', stop)
  })
  relay?.close()
  if (sharedDb && sharedDb !== db) await sharedDb.close().catch(() => {})
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
