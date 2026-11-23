import { describe, expect, it } from 'vitest'
import { detectSensitiveColumns, maskValue, openShare, sealShare } from './share-crypto.js'

describe('share encryption', () => {
  const payload = { v: 1, kind: 'result', title: 'Top customers', createdAt: 1, columns: [{ name: 'id' }], rows: [[1], [2]] }

  it('round-trips a payload with the link key', async () => {
    const s = await sealShare(payload)
    expect(s.blob[0]).toBe(1)
    expect(s.passwordSalt).toBeNull()
    expect(await openShare(s.blob, s.linkKey, null, null)).toEqual(payload)
  })

  it('needs the password as well as the link when there is one', async () => {
    const s = await sealShare(payload, { password: 'hunter2' })
    expect(s.passwordSalt).toBeTruthy()
    expect(await openShare(s.blob, s.linkKey, 'hunter2', s.passwordSalt)).toEqual(payload)
    await expect(openShare(s.blob, s.linkKey, 'wrong', s.passwordSalt)).rejects.toThrow()
    await expect(openShare(s.blob, s.linkKey, null, null)).rejects.toThrow()
  }, 20000)

  it('fails on another link key', async () => {
    const a = await sealShare(payload)
    const b = await sealShare(payload)
    await expect(openShare(a.blob, b.linkKey, null, null)).rejects.toThrow()
  })

  it('never puts the plaintext in the uploaded bytes', async () => {
    const s = await sealShare({ ...payload, title: 'SECRET-TITLE-123' })
    expect(new TextDecoder().decode(s.blob)).not.toContain('SECRET-TITLE-123')
  })
})

describe('sensitive columns', () => {
  it('flags personal data by name and by content', () => {
    const cols = [{ name: 'id' }, { name: 'email' }, { name: 'contact' }, { name: 'api_key' }, { name: 'note' }, { name: 'mobile' }]
    const rows = [
      [1, 'a@b.co', 'ada@example.com', 'sk_live_abcdefghijklmnopqrstuv', 'hello', '+1 415 555 0100'],
      [2, 'c@d.io', 'bob@example.org', 'sk_live_zyxwvutsrqponmlkjihgfe', 'world', '+44 20 7946 0958'],
    ]
    const hits = detectSensitiveColumns(cols, rows).map((h) => `${h.name}:${h.why}`)
    expect(hits).toEqual(['email:name', 'contact:emails', 'api_key:name', 'mobile:name'])
  })

  it('masks while keeping the shape', () => {
    expect(maskValue('ada@example.com')).toBe('a••@e••••••.com')
    expect(maskValue('+1 415 555 0100')).toMatch(/^\+1•+00$/)
    expect(maskValue(null)).toBeNull()
    expect(maskValue('abc')).toBe('••••')
  })
})

describe('sensitive columns: what is not a phone number', () => {
  it('leaves dates and bare digit ids alone', () => {
    const cols = [{ name: 'joined' }, { name: 'ext_id' }, { name: 'contact' }]
    const rows = [
      ['2024-01-03', '9223372036854775807', '(415) 555-0100'],
      ['2024-02-10', '1234567890123', '415-555-0199'],
    ]
    expect(detectSensitiveColumns(cols, rows).map((h) => `${h.name}:${h.why}`)).toEqual(['contact:phone numbers'])
  })
})
