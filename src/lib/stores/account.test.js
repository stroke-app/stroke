import { describe, it, expect, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

const { planLabel, initials } = await import('./account.js')

/** @param {Partial<import('./account.js').AccountStatus>} over */
const acc = (over = {}) => /** @type {any} */ ({ signed_in: true, entitlement_fresh: true, entitlement: null, ...over })

describe('planLabel', () => {
  const now = 1_000_000
  it('a valid key is Pro whatever the account says', () => {
    expect(planLabel(null, { status: 'Valid', email: 'a', plan: 'pro', issued_at: 0, expires_at: null }, now).tone).toBe('pro')
  })
  it('account license is Pro', () => {
    const a = acc({ entitlement: { plan: 'pro', source: 'license', license: null, trial: null, issued_at: 0, valid_until: now + 1 } })
    expect(planLabel(a, null, now)).toEqual({ label: 'Pro', tone: 'pro' })
  })
  it('trial shows days', () => {
    expect(planLabel(null, { status: 'Trial', days_remaining: 1 }, now).label).toBe('Trial · 1 day')
    const a = acc({ entitlement: { plan: 'pro', source: 'trial', license: null, trial: { started_at: 0, ends_at: now + 3 * 86400 }, issued_at: 0, valid_until: now + 1 } })
    expect(planLabel(a, null, now).label).toBe('Trial · 3 days')
  })
  it('stale or signed-out entitlement is ignored', () => {
    const ent = { plan: 'pro', source: 'license', license: null, trial: null, issued_at: 0, valid_until: 0 }
    expect(planLabel(acc({ entitlement_fresh: false, entitlement: ent }), { status: 'TrialExpired' }, now).tone).toBe('free')
    expect(planLabel(acc({ signed_in: false, entitlement: ent }), null, now).tone).toBe('free')
  })
})

describe('initials', () => {
  it('uses name, then email', () => {
    expect(initials({ id: '1', email: 'x@y.z', name: 'Ada Lovelace' })).toBe('AL')
    expect(initials({ id: '1', email: 'nina@y.z', name: null })).toBe('N')
    expect(initials(null)).toBe('?')
  })
})

describe('parseSignInCode', async () => {
  const { parseSignInCode } = await import('./account.js')
  const code = 'AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_abcde'
  it('accepts the prefixed and the bare code', () => {
    expect(parseSignInCode(code)).toBe(code)
    expect(parseSignInCode(`stroke-code:${code}`)).toBe(code)
    expect(parseSignInCode(`  Stroke-Code: ${code}\n`)).toBe(code)
    expect(parseSignInCode(`"stroke-code:${code}"`)).toBe(code)
  })
  it('rejects anything else', () => {
    for (const bad of ['', 'stroke-code:', 'short', 'has spaces in the middle of it', `https://x/?code=${code}`, 'a'.repeat(257)]) {
      expect(parseSignInCode(bad)).toBeNull()
    }
  })
})
