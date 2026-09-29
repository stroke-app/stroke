import { describe, it, expect } from 'vitest'
import { providerOf, engineLabel } from './connection-provider.js'

describe('providerOf', () => {
  it('trusts the provider a sign-in recorded', () => {
    expect(providerOf({ type: 'postgres', provider: 'neon', host: 'localhost' })).toBe('neon')
    expect(providerOf({ type: 'postgres', provider: 'prisma-postgres' })).toBe('prisma')
  })

  it('falls back to the host for pasted connections', () => {
    expect(providerOf({ type: 'postgres', host: 'us-west-2.db.thenile.dev' })).toBe('nile')
    expect(providerOf({ type: 'mysql', host: 'gateway01.ap-northeast-1.prod.aws.tidbcloud.com' })).toBe('tidb')
    expect(providerOf({ type: 'redis', host: 'fine-cat-1234.upstash.io' })).toBe('upstash')
    expect(providerOf({ type: 'libsql', url: 'libsql://cool-me.aws-ap-south-1.turso.io' })).toBe('turso')
    expect(providerOf({ type: 'postgres', host: 'shortline.proxy.rlwy.net' })).toBe('railway')
  })

  it('knows PostHog by type, with HogQL as its engine chip', () => {
    expect(providerOf({ type: 'posthog', host: 'https://eu.posthog.com' })).toBe('posthog')
    expect(engineLabel('posthog')).toBe('HogQL')
  })

  it('knows Cloudflare D1 by type and leaves self-hosted alone', () => {
    expect(providerOf({ type: 'd1' })).toBe('d1')
    expect(providerOf({ type: 'postgres', host: '13.205.111.67' })).toBeNull()
    expect(providerOf({ type: 'sqlite' })).toBeNull()
  })
})

describe('engineLabel', () => {
  it('shortens engine ids for the chip', () => {
    expect(engineLabel('postgres')).toBe('Postgres')
    expect(engineLabel('libsql')).toBe('libSQL')
    expect(engineLabel('mystery')).toBe('mystery')
  })
})
