import { describe, it, expect, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

const { planLabel, initials, offlineGraceNote } = await import('./account.js')

/** @param {Partial<import('./account.js').AccountStatus>} over */
const acc = (over = {}) => /** @type {any} */ ({ signed_in: true, entitlement_fresh: true, entitlement: null, ...over })

describe('planLabel', () => {
  const valid = /** @type {const} */ ({ status: 'Valid', email: 'a', plan: 'pro', issued_at: 0, expires_at: null })
  it('follows the status the Pro gates read', () => {
    expect(planLabel(acc(), valid)).toEqual({ label: 'Pro', tone: 'pro' })
    expect(planLabel(acc(), { status: 'Trial', days_remaining: 1 }).label).toBe('Trial · 1 day')
    expect(planLabel(acc(), { status: 'Trial', days_remaining: 3 }).label).toBe('Trial · 3 days')
    expect(planLabel(acc(), { status: 'TrialExpired' }).tone).toBe('free')
  })
  it('a paid entitlement does not say Pro unless the status does', () => {
    const ent = { plan: 'pro', source: 'license', license: null, trial: null, issued_at: 0, valid_until: 0 }
    expect(planLabel(acc({ entitlement: ent }), { status: 'TrialExpired' }).tone).toBe('free')
  })
  it('signed out is never Pro', () => {
    expect(planLabel(acc({ signed_in: false }), valid).tone).toBe('free')
    expect(planLabel(null, valid).tone).toBe('free')
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

describe('offlineGraceNote', () => {
  const now = 1_000_000
  it('is null when not on the offline grace', () => {
    expect(offlineGraceNote(acc({ offline_grace_until: 0 }), now)).toBeNull()
    expect(offlineGraceNote(acc({ offline_grace_until: now - 1 }), now)).toBeNull()
    expect(offlineGraceNote(acc({ signed_in: false, offline_grace_until: now + 86400 }), now)).toBeNull()
    expect(offlineGraceNote(null, now)).toBeNull()
  })
  it('counts the days left, rounding up', () => {
    expect(offlineGraceNote(acc({ offline_grace_until: now + 1 }), now)?.days).toBe(1)
    expect(offlineGraceNote(acc({ offline_grace_until: now + 86400 }), now)?.text).toContain('within 1 day ')
    expect(offlineGraceNote(acc({ offline_grace_until: now + 10 * 86400 - 5 }), now)?.days).toBe(10)
  })
})
