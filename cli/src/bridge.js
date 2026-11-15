// The local bridge the web studio talks to. The browser can't open a database
// socket, so the page sends JSON here and this runs it on the project's
// database.
//
// It runs whatever SQL it is sent, so it is locked down to one page, one
// session and this machine:
//   - listens on 127.0.0.1 only;
//   - every call carries a random token that exists only in the URL fragment
//     of the studio tab this process opened (a fragment never reaches a server);
//   - the Host header must be the loopback address it listens on, so a DNS
//     rebinding page can't borrow the browser to reach it;
//   - CORS answers only the studio's origin.

import http from 'node:http'
import crypto from 'node:crypto'

export const STUDIO_ORIGIN = 'https://stroke.click'
/** Bumped with the package; the page reads `features` to know what it may offer. */
export const BRIDGE_VERSION = '0.2.0'
export const FEATURES = ['structure', 'where', 'edit', 'rowCounts']
const MAX_BODY = 1 << 20

/**
 * @param {{
 *   engine: import('./engines.js').Engine,
 *   name: string,
 *   origins?: string[],
 *   port?: number,
 *   token?: string,
 * }} o
 * @returns {Promise<{ port: number, token: string, close(): Promise<void>, server: http.Server }>}
 */
export async function startBridge({ engine, name, origins = [STUDIO_ORIGIN], port = 0, token = crypto.randomBytes(24).toString('base64url') }) {
  const want = Buffer.from(token)
  /** @param {string | undefined} got */
  const tokenOk = (got) => {
    const b = Buffer.from(String(got ?? '').replace(/^Bearer\s+/i, ''))
    return b.length === want.length && crypto.timingSafeEqual(b, want)
  }

  /** @type {Record<string, (body: any) => Promise<unknown>>} */
  const routes = {
    '/info': async () => ({ ...(await engine.info()), name, bridge: BRIDGE_VERSION, features: FEATURES }),
    '/tables': () => engine.tables(),
    '/columns': (b) => engine.columns(str(b.schema), str(b.table)),
    '/structure': (b) => engine.structure(str(b.schema), str(b.table)),
    '/rows': (b) => engine.rows({
      schema: str(b.schema),
      table: str(b.table),
      limit: Number(b.limit) || 100,
      offset: Number(b.offset) || 0,
      sort: b.sort ?? null,
      where: typeof b.where === 'string' ? b.where : '',
    }),
    '/update': (b) => engine.update({ schema: str(b.schema), table: str(b.table), key: obj(b.key), set: obj(b.set) }),
    '/insert': (b) => engine.insert({ schema: str(b.schema), table: str(b.table), values: obj(b.values) }),
    '/delete': (b) => {
      if (!Array.isArray(b.keys) || !b.keys.length) throw new Error('Expected keys')
      return engine.remove({ schema: str(b.schema), table: str(b.table), keys: b.keys.map(obj) })
    },
    '/query': async (b) => {
      const t = performance.now()
      const r = await engine.query(str(b.sql))
      return { ...r, durationMs: Math.round(performance.now() - t) }
    },
  }

  /** @type {number} */
  let boundPort = 0
  const server = http.createServer(async (req, res) => {
    const origin = req.headers.origin
    const allowed = typeof origin === 'string' && origins.includes(origin)
    if (allowed) {
      res.setHeader('Access-Control-Allow-Origin', origin)
      res.setHeader('Vary', 'Origin')
    }
    // DNS rebinding: a page on evil.test resolving to 127.0.0.1 sends its own Host.
    const host = String(req.headers.host ?? '')
    if (host !== `127.0.0.1:${boundPort}` && host !== `localhost:${boundPort}`) return send(res, 421, { error: 'Wrong host' })

    if (req.method === 'OPTIONS') {
      if (!allowed) return send(res, 403, { error: 'Origin not allowed' })
      res.setHeader('Access-Control-Allow-Methods', 'POST, OPTIONS')
      res.setHeader('Access-Control-Allow-Headers', 'authorization, content-type')
      // Chrome's Private Network Access: a public page asking for loopback.
      if (req.headers['access-control-request-private-network']) res.setHeader('Access-Control-Allow-Private-Network', 'true')
      res.setHeader('Access-Control-Max-Age', '600')
      res.writeHead(204).end()
      return
    }
    if (!allowed) return send(res, 403, { error: 'Origin not allowed' })
    if (req.method !== 'POST') return send(res, 405, { error: 'POST only' })
    if (!tokenOk(req.headers.authorization)) return send(res, 401, { error: 'Bad token' })
    const route = routes[new URL(req.url ?? '/', 'http://x').pathname]
    if (!route) return send(res, 404, { error: 'Not found' })
    try {
      const body = await readJson(req)
      send(res, 200, await route(body))
    } catch (e) {
      send(res, 400, { error: e instanceof Error ? e.message : String(e) })
    }
  })

  await new Promise((resolve, reject) => {
    server.once('error', reject)
    server.listen(port, '127.0.0.1', () => resolve(undefined))
  })
  boundPort = /** @type {import('node:net').AddressInfo} */ (server.address()).port
  return {
    port: boundPort,
    token,
    server,
    close: () => new Promise((resolve) => {
      server.closeAllConnections?.()
      server.close(() => resolve())
    }),
  }
}

/** @param {unknown} v @returns {Record<string, unknown>} */
function obj(v) {
  if (!v || typeof v !== 'object' || Array.isArray(v)) throw new Error('Expected an object')
  return /** @type {Record<string, unknown>} */ (v)
}

/** @param {unknown} v */
function str(v) {
  if (typeof v !== 'string') throw new Error('Expected a string')
  return v
}

/** @param {http.IncomingMessage} req */
function readJson(req) {
  return new Promise((resolve, reject) => {
    let size = 0
    /** @type {Buffer[]} */
    const chunks = []
    req.on('data', (c) => {
      size += c.length
      if (size > MAX_BODY) {
        reject(new Error('Request too large'))
        req.destroy()
      } else chunks.push(c)
    })
    req.on('end', () => {
      try {
        resolve(chunks.length ? JSON.parse(Buffer.concat(chunks).toString('utf8')) : {})
      } catch {
        reject(new Error('Body is not JSON'))
      }
    })
    req.on('error', reject)
  })
}

/** @param {http.ServerResponse} res @param {number} status @param {unknown} body */
function send(res, status, body) {
  if (res.headersSent) return
  res.writeHead(status, { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store' })
  res.end(JSON.stringify(body))
}

/**
 * The studio page for this bridge. Everything after `#` stays in the browser.
 * @param {{ studio: string, port: number, token: string }} o
 */
export function studioUrl({ studio, port, token }) {
  return `${studio}#${new URLSearchParams({ bridge: `http://127.0.0.1:${port}`, token })}`
}
