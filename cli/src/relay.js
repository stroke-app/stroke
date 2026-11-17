// Share the studio beyond this machine (Labs). The local bridge only answers
// 127.0.0.1; a share holds one WebSocket open to a room on the relay at
// stroke.click, and a studio page on any device sends its calls there.
//
// Everything is end-to-end encrypted: AES-256-GCM with a key that exists only
// in the share link's #fragment, which no browser sends to a server. The relay
// sees the room id and ciphertext. The same scheme is in stroke-web
// (src/lib/relay/relay-crypto.ts) and the desktop app (src/lib/studio-share.js).

import crypto from 'node:crypto'

const subtle = crypto.webcrypto.subtle

/** @param {Uint8Array | Buffer} b */
export const toB64u = (b) => Buffer.from(b).toString('base64url')
/** @param {string} s */
export const fromB64u = (s) => new Uint8Array(Buffer.from(s, 'base64url'))

/** A fresh share: a room on the relay, the link's key, and the host's claim on the room. */
export function newShare() {
  return {
    room: toB64u(crypto.randomBytes(16)),
    key: toB64u(crypto.randomBytes(32)),
    secret: toB64u(crypto.randomBytes(32)),
  }
}

/** The link someone else opens. Everything after # stays in their browser. */
export function shareUrl({ studio, room, key }) {
  return `${studio}#${new URLSearchParams({ relay: room, key })}`
}

/** @param {string} key */
export async function importKey(key) {
  return subtle.importKey('raw', fromB64u(key), 'AES-GCM', false, ['encrypt', 'decrypt'])
}

/** @param {CryptoKey} key @param {unknown} value */
export async function seal(key, value) {
  const iv = crypto.randomBytes(12)
  const ct = await subtle.encrypt({ name: 'AES-GCM', iv }, key, Buffer.from(JSON.stringify(value)))
  return { iv: toB64u(iv), ct: toB64u(new Uint8Array(ct)) }
}

/** @param {CryptoKey} key @param {{ iv: string, ct: string }} sealed */
export async function open(key, sealed) {
  const pt = await subtle.decrypt({ name: 'AES-GCM', iv: fromB64u(sealed.iv) }, key, fromB64u(sealed.ct))
  return JSON.parse(Buffer.from(pt).toString('utf8'))
}

/**
 * Answer a room's calls until closed. Reconnects with backoff if the socket
 * drops (laptop sleep, Wi-Fi change); the link keeps working when it's back.
 * @param {{
 *   routes: Record<string, (body: any) => Promise<unknown>>,
 *   room: string, key: string, secret: string,
 *   base?: string,
 *   onStatus?: (status: 'online' | 'offline', detail?: string) => void,
 * }} o
 */
export async function startRelayHost({ routes, room, key, secret, base = 'https://stroke.click', onStatus = () => {} }) {
  if (typeof WebSocket !== 'function') {
    throw new Error(`Sharing needs Node 22 or newer (this is ${process.version}).`)
  }
  const cryptoKey = await importKey(key)
  const url = `${base.replace(/^http/, 'ws')}/api/relay/${room}/host?${new URLSearchParams({ secret })}`
  let closed = false
  let delay = 1000
  /** @type {WebSocket | null} */
  let ws = null
  /** @type {ReturnType<typeof setInterval> | undefined} */
  let ping

  /** @param {{ id: number, iv: string, ct: string }} msg */
  const answer = async (msg) => {
    let reply
    try {
      const { path, body } = await open(cryptoKey, msg)
      const route = typeof path === 'string' ? routes[path] : undefined
      reply = route
        ? { status: 200, data: await route(body ?? {}) }
        : { status: 404, error: 'Not available in this shared studio.' }
    } catch (e) {
      reply = { status: 400, error: e instanceof Error ? e.message : String(e) }
    }
    try {
      ws?.send(JSON.stringify({ id: msg.id, ...(await seal(cryptoKey, reply)) }))
    } catch {
      // the socket went; the caller times out and retries
    }
  }

  const connect = () => {
    if (closed) return
    const sock = new WebSocket(url)
    ws = sock
    sock.addEventListener('open', () => {
      delay = 1000
      onStatus('online')
      clearInterval(ping)
      ping = setInterval(() => sock.readyState === 1 && sock.send('{"ping":1}'), 25_000)
    })
    sock.addEventListener('message', (ev) => {
      let msg
      try {
        msg = JSON.parse(String(ev.data))
      } catch {
        return
      }
      if (typeof msg?.id === 'number' && typeof msg.iv === 'string' && typeof msg.ct === 'string') void answer(msg)
    })
    sock.addEventListener('close', (ev) => {
      clearInterval(ping)
      if (ws !== sock || closed) return
      if (ev.code === 1008 || ev.code === 4003) {
        onStatus('offline', 'The relay refused this share.')
        return
      }
      onStatus('offline', 'Reconnecting')
      setTimeout(connect, delay)
      delay = Math.min(delay * 2, 30_000)
    })
    sock.addEventListener('error', () => {
      // 'close' follows and reconnects
    })
  }
  connect()

  return {
    close() {
      closed = true
      clearInterval(ping)
      try {
        ws?.close(1000, 'Stopped sharing')
      } catch {
        // already closed
      }
    },
  }
}
