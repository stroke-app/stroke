import { describe, it, expect } from 'vitest'
import { AUTH_FAILURE, providerToSaved } from './providers.js'

describe('AUTH_FAILURE', () => {
  it("recognizes Prisma Postgres's rejected credentials", () => {
    const prisma = 'Connection failed: error returned from database: Failed to identify your database: Your Postgres credentials are incorrect. Please check your username and password and try again.'
    expect(AUTH_FAILURE.test(prisma)).toBe(true)
  })
  it('still recognizes the usual engine wording', () => {
    expect(AUTH_FAILURE.test('password authentication failed for user "x"')).toBe(true)
    expect(AUTH_FAILURE.test('Access denied for user (1045)')).toBe(true)
  })
  it('leaves network errors alone', () => {
    expect(AUTH_FAILURE.test('Connection refused (os error 111)')).toBe(false)
    expect(AUTH_FAILURE.test('timed out')).toBe(false)
  })
})

describe('Convex', () => {
  it('a revoked deploy key reads as an auth failure', () => {
    expect(AUTH_FAILURE.test('Convex refused the key (401). Use a deploy key or admin key for this deployment.')).toBe(true)
  })

  it('a built deployment saves as a convex connection with its key as a secret', () => {
    const built = { db_type: 'convex', host: 'https://happy-otter-123.convex.cloud', port: 443, username: '', password: 'prod:happy-otter-123|key', database: 'happy-otter-123', ssl: true, needs_password: false, name: 'Convex · Chat · prod', providerRef: 'ref' }
    const saved = [{ id: 'old', type: 'convex', url: 'https://happy-otter-123.convex.cloud' }]
    const row = providerToSaved(built, { provider: 'convex', saved, newId: () => 'new' })
    expect(row).toEqual({ name: 'Convex · Chat · prod', provider: 'convex', providerRef: 'ref', id: 'old', type: 'convex', url: 'https://happy-otter-123.convex.cloud', apiKey: 'prod:happy-otter-123|key', readOnly: undefined })
    expect(providerToSaved(built, { provider: 'convex', saved: [], newId: () => 'new' }).id).toBe('new')
  })
})
