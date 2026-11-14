import { describe, it, expect } from 'vitest'
import {
  CREATE_SPECS,
  CREATE_ELSEWHERE,
  createSpec,
  nameProblem,
  suggestName,
  generatePassword,
  waitUntilReady,
} from './provider-create.js'

describe('create specs', () => {
  it('covers the providers whose API can create, and says why for the rest', () => {
    for (const id of ['neon', 'supabase', 'prisma', 'tidb', 'turso', 'nile', 'upstash', 'd1']) {
      expect(createSpec(id)).not.toBeNull()
    }
    expect(createSpec('planetscale')).toBeNull()
    expect(createSpec('railway')).toBeNull()
    expect(CREATE_ELSEWHERE.planetscale.url).toMatch(/^https:\/\//)
    expect(CREATE_ELSEWHERE.railway.reason).toMatch(/Railway/)
  })

  it('only Supabase asks for a password', () => {
    expect(Object.entries(CREATE_SPECS).filter(([, s]) => s.password).map(([id]) => id)).toEqual(['supabase'])
  })

  it('every default name passes its own rules', () => {
    for (const spec of Object.values(CREATE_SPECS)) expect(nameProblem(spec.defaultName, spec)).toBe('')
  })
})

describe('nameProblem', () => {
  it('requires a name', () => {
    expect(nameProblem('  ', createSpec('neon'))).toBe('Enter a name.')
  })

  it('applies the provider pattern and length', () => {
    expect(nameProblem('Shop', createSpec('turso'))).toMatch(/lowercase/)
    expect(nameProblem('shop-db', createSpec('turso'))).toBe('')
    expect(nameProblem('1db', createSpec('nile'))).toMatch(/starting with a letter/)
    expect(nameProblem('my_db', createSpec('nile'))).toBe('')
    expect(nameProblem('x'.repeat(65), createSpec('neon'))).toMatch(/64 characters/)
    expect(nameProblem('Any Name', createSpec('supabase'))).toBe('')
  })
})

describe('suggestName', () => {
  it('avoids names already on the account', () => {
    expect(suggestName(createSpec('neon'), [])).toBe('stroke-db')
    expect(suggestName(createSpec('neon'), ['stroke-db', 'Stroke-DB-2'])).toBe('stroke-db-3')
    expect(suggestName(createSpec('nile'), ['stroke_db'])).toBe('stroke_db_2')
  })
})

describe('generatePassword', () => {
  it('is long, alphanumeric, and different every time', () => {
    const a = generatePassword()
    expect(a).toHaveLength(24)
    expect(a).toMatch(/^[A-Za-z0-9]+$/)
    expect(generatePassword()).not.toBe(a)
  })

  it('skips bytes that would bias the alphabet', () => {
    // 255 is above the rejection limit for a 57-letter alphabet; 0 maps to 'A'.
    let calls = 0
    const fill = (/** @type {Uint8Array} */ b) => {
      calls++
      b.fill(calls === 1 ? 255 : 0)
      return b
    }
    expect(generatePassword(4, fill)).toBe('AAAA')
    expect(calls).toBe(2)
  })
})

describe('waitUntilReady', () => {
  const noWait = async () => {}

  it('polls until the provider says ready, reporting each state', async () => {
    const states = [
      { ready: false, failed: false, state: 'CREATING' },
      { ready: false, failed: false, state: 'CREATING' },
      { ready: true, failed: false, state: 'ACTIVE' },
    ]
    const seen = []
    const s = await waitUntilReady({ check: async () => states.shift(), onstate: (x) => seen.push(x.state), wait: noWait })
    expect(s.state).toBe('ACTIVE')
    expect(seen).toEqual(['CREATING', 'CREATING', 'ACTIVE'])
  })

  it('stops on a failed database', async () => {
    await expect(
      waitUntilReady({ check: async () => ({ ready: false, failed: true, state: 'INIT_FAILED' }), wait: noWait }),
    ).rejects.toThrow(/init_failed/)
  })

  it('rides out a flaky status call but not three in a row', async () => {
    let n = 0
    const flaky = async () => {
      n++
      if (n === 1) throw new Error('network')
      return { ready: true, failed: false, state: 'ready' }
    }
    expect((await waitUntilReady({ check: flaky, wait: noWait })).ready).toBe(true)
    await expect(
      waitUntilReady({ check: async () => { throw new Error('down') }, wait: noWait }),
    ).rejects.toThrow('down')
  })

  it('gives up after the timeout', async () => {
    let t = 0
    await expect(
      waitUntilReady({
        check: async () => ({ ready: false, failed: false, state: 'CREATING' }),
        wait: async () => { t += 5000 },
        now: () => t,
        timeoutMs: 12_000,
      }),
    ).rejects.toThrow(/longer than usual/)
  })

  it('aborts when the user stops waiting', async () => {
    const ctl = new AbortController()
    const p = waitUntilReady({
      check: async () => ({ ready: false, failed: false, state: 'CREATING' }),
      signal: ctl.signal,
      intervalMs: 60_000,
    })
    setTimeout(() => ctl.abort(), 5)
    await expect(p).rejects.toMatchObject({ name: 'AbortError' })
  })
})
