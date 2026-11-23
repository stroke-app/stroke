import { describe, it, expect, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), Channel: class {} }))
vi.mock('$lib/stores/settings.js', () => ({ loadSettings: () => ({}) }))
vi.mock('$lib/stores/query-log.js', () => ({ recordQuery: () => {} }))
vi.mock('$lib/stores/read-only.js', () => ({ assertWritable: () => {} }))
vi.mock('$lib/stores/connections.js', () => ({ getLastConnection: () => null }))

describe('withTlsAndParams', () => {
  it('carries verify-full, its CA and the URL parameters to the backend', async () => {
    const { withTlsAndParams, normalizeConnectionConfig } = await import('./api.js')
    const saved = { host: 'h', port: 5432, database: 'd', user: 'u', password: 'p', ssl: true, sslMode: 'verify-full', sslRootCert: '/ca.pem', params: 'application_name=x' }
    expect(withTlsAndParams(normalizeConnectionConfig(saved), saved)).toMatchObject({
      ssl: true, sslMode: 'verify-full', sslRootCert: '/ca.pem', params: 'application_name=x',
    })
  })

  it('sends no TLS mode with TLS off, so it cannot turn TLS back on', async () => {
    const { withTlsAndParams } = await import('./api.js')
    const out = withTlsAndParams({ ssl: false }, { ssl: false, sslMode: 'verify-full', sslRootCert: '/ca.pem' })
    expect(out).toEqual({ ssl: false })
  })
})
