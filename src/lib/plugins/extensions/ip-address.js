// IP addresses - v4 or v6, and what kind: private, loopback, link-local,
// CGNAT, multicast, documentation, or public. A dot says it at a glance, the
// tooltip in words. Works on inet/cidr columns and on text columns named like
// an address (ip, client_ip, remote_addr, ...).

const NAME_RE = /(^|_)(ip|ips|ip_?addr(ess)?|ipv4|ipv6|remote_?addr(ess)?|client_?ip|source_?ip|src_?ip|dst_?ip|dest_?ip|peer_?ip|host_?ip|server_?ip|last_?ip|login_?ip|x_forwarded_for)($|_)/i

/** @param {string} s @returns {number[] | null} */
function v4(s) {
  const m = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(s)
  if (!m) return null
  const o = m.slice(1).map(Number)
  return o.every((x) => x <= 255) ? o : null
}

/** First 16 bits as a number, or null. Rough parse: enough to classify. @param {string} s */
function v6Head(s) {
  if (!/^[0-9a-f:.]+$/i.test(s) || !s.includes(':')) return null
  const groups = s.split('::')
  if (groups.length > 2) return null
  const head = s.startsWith('::') ? '0' : s.split(':')[0]
  const n = parseInt(head || '0', 16)
  return Number.isFinite(n) ? n : null
}

/** @param {string} raw @returns {{ version: 4 | 6, kind: string, tone: 'private' | 'local' | 'public' | 'special' } | null} */
export function classifyIp(raw) {
  const s = raw.trim().replace(/^\[|\]$/g, '').replace(/%.*$/, '').split('/')[0]
  const o = v4(s)
  if (o) {
    const [a, b] = o
    if (a === 127) return { version: 4, kind: 'loopback', tone: 'local' }
    if (a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168)) return { version: 4, kind: 'private (RFC 1918)', tone: 'private' }
    if (a === 100 && b >= 64 && b <= 127) return { version: 4, kind: 'carrier-grade NAT', tone: 'private' }
    if (a === 169 && b === 254) return { version: 4, kind: 'link-local', tone: 'local' }
    if (a >= 224 && a <= 239) return { version: 4, kind: 'multicast', tone: 'special' }
    if (a === 0 || a >= 240) return { version: 4, kind: 'reserved', tone: 'special' }
    if ((a === 192 && b === 0 && o[2] === 2) || (a === 198 && b === 51 && o[2] === 100) || (a === 203 && b === 0 && o[2] === 113)) return { version: 4, kind: 'documentation', tone: 'special' }
    return { version: 4, kind: 'public', tone: 'public' }
  }
  const h = v6Head(s)
  if (h === null) return null
  const low = s.toLowerCase()
  if (low === '::1') return { version: 6, kind: 'loopback', tone: 'local' }
  if (low === '::') return { version: 6, kind: 'unspecified', tone: 'special' }
  if (low.startsWith('::ffff:') && v4(low.slice(7))) {
    const inner = classifyIp(low.slice(7))
    return inner ? { ...inner, version: 6, kind: `IPv4-mapped, ${inner.kind}` } : null
  }
  if ((h & 0xfe00) === 0xfc00) return { version: 6, kind: 'unique local (private)', tone: 'private' }
  if ((h & 0xffc0) === 0xfe80) return { version: 6, kind: 'link-local', tone: 'local' }
  if ((h & 0xff00) === 0xff00) return { version: 6, kind: 'multicast', tone: 'special' }
  if (low.startsWith('2001:db8')) return { version: 6, kind: 'documentation', tone: 'special' }
  if ((h & 0xe000) === 0x2000) return { version: 6, kind: 'public', tone: 'public' }
  return { version: 6, kind: 'reserved', tone: 'special' }
}

const DOT = {
  public: 'rgba(59,130,246,0.9)',
  private: 'rgba(34,197,94,0.9)',
  local: 'rgba(148,163,184,0.9)',
  special: 'rgba(245,158,11,0.9)',
}

export const ipAddress = {
  id: 'ip-address',
  name: 'IP Address Tags',
  description: 'Mark IPv4 / IPv6 addresses as public, private, loopback, link-local and so on.',
  kind: 'formatter',
  defaultConfig: {},

  /** @param {string} type @param {string} name */
  appliesTo(type, name) {
    return /^(inet|cidr)$/i.test(String(type ?? '').trim()) || (!!name && NAME_RE.test(name)) || (!name && /char|text/i.test(type ?? ''))
  },

  /** @param {unknown} value */
  format(value) {
    if (typeof value !== 'string' || value.length > 64) return null
    const first = value.split(',')[0]
    const c = classifyIp(first)
    if (!c) return null
    const cidr = first.includes('/') ? ` · /${first.split('/')[1]}` : ''
    const more = value.includes(',') ? ` · first of ${value.split(',').length} (proxy chain)` : ''
    return { dot: DOT[c.tone], title: `IPv${c.version} · ${c.kind}${cidr}${more}` }
  },
}
