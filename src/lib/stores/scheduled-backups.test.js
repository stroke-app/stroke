import { describe, it, expect, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

const { dueSlot, describeCron } = await import('./scheduled-backups.js')

/** @param {object} over */
const job = (over = {}) => /** @type {any} */ ({
  id: 'j', cron: '0 3 * * *', timezone: 'UTC', enabled: true, run_requested_at: null,
  created_at: Date.parse('2026-11-01T00:00:00Z'), runs: [], ...over,
})

describe('dueSlot', () => {
  const now = Date.parse('2026-11-18T05:00:00Z')
  it('missed ticks collapse into the latest one', () => {
    expect(dueSlot(job(), now)).toBe('2026-11-18T03:00:00.000Z')
  })
  it('a tick already run is not due again', () => {
    const runs = [{ slot: '2026-11-18T03:00:00.000Z' }]
    expect(dueSlot(job({ runs }), now)).toBeNull()
  })
  it('nothing is due before the first tick after creation', () => {
    expect(dueSlot(job({ created_at: Date.parse('2026-11-18T04:00:00Z') }), now)).toBeNull()
  })
  it('paused jobs only run when asked', () => {
    expect(dueSlot(job({ enabled: false }), now)).toBeNull()
    expect(dueSlot(job({ enabled: false, run_requested_at: 42 }), now)).toBe('manual:42')
  })
  it('reads the schedule in its time zone', () => {
    // 03:00 in Kolkata is 21:30 UTC the day before.
    expect(dueSlot(job({ timezone: 'Asia/Kolkata' }), now)).toBe('2026-11-17T21:30:00.000Z')
  })
})

describe('describeCron', () => {
  it('names the common schedules', () => {
    expect(describeCron('0 3 * * *')).toBe('Every day at 03:00')
    expect(describeCron('30 22 * * 1')).toBe('Every Monday at 22:30')
    expect(describeCron('0 */6 * * *')).toBe('Every 6 hours')
    expect(describeCron('0 * * * *')).toBe('Every hour at :00')
    expect(describeCron('5 4 1-7 * *')).toBe('5 4 1-7 * *')
  })
})
