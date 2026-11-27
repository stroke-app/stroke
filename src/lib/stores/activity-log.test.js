import { describe, expect, it } from 'vitest'

globalThis.requestAnimationFrame ??= /** @type {any} */ ((cb) => setTimeout(cb, 0))
import { clearActivityLog, recordActivity, subscribeActivityLog } from './activity-log.js'

describe('activity log clear undo', () => {
  it('puts the cleared entries back under any recorded since', async () => {
    recordActivity({ type: 'query', title: 'a' })
    recordActivity({ type: 'query', title: 'b' })
    const undo = clearActivityLog()
    recordActivity({ type: 'query', title: 'c' })
    undo()
    /** @type {any[]} */
    let seen = []
    const off = subscribeActivityLog((l) => { seen = l })
    off()
    expect(seen.map((e) => e.title)).toEqual(['c', 'b', 'a'])
  })
})
