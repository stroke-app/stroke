import { describe, it, expect } from 'vitest'
import { pickSyncedSettings, syncLabel, SYNCED_SETTINGS } from './policy.js'

describe('synced settings subset', () => {
  it('keeps appearance and editor choices, drops device, window and consent settings', () => {
    const picked = pickSyncedSettings({
      theme: 'dark', font: 'geist', sqlEditor: { wrap: true }, vimMode: true,
      zoom: 1.2, nativeScroll: true, showMenuBar: false, launchAtLogin: true, autoUpdate: false,
      mcpAutoStart: true, telemetry: false, agentWebAccess: true, connectTimeoutMs: 5,
      apiKey: 'sk-secret',
    })
    expect(picked).toEqual({ theme: 'dark', font: 'geist', sqlEditor: { wrap: true }, vimMode: true })
    for (const k of ['zoom', 'nativeScroll', 'launchAtLogin', 'autoUpdate', 'mcpAutoStart', 'telemetry', 'agentWebAccess']) {
      expect(SYNCED_SETTINGS).not.toContain(k)
    }
  })
})

describe('sync label', () => {
  const base = { lastSyncedAt: 0, items: 0, error: null, keyChanged: false, undecryptable: 0 }
  it('reads the phase and the time since the last sync', () => {
    const now = 10_000_000
    expect(syncLabel({ ...base, phase: 'locked' }, now)).toBe('Sync locked')
    expect(syncLabel({ ...base, phase: 'ready' }, now)).toBe('Not synced yet')
    expect(syncLabel({ ...base, phase: 'ready', lastSyncedAt: now - 10_000 }, now)).toBe('Synced just now')
    expect(syncLabel({ ...base, phase: 'ready', lastSyncedAt: now - 120_000 }, now)).toBe('Synced 2 min ago')
    expect(syncLabel({ ...base, phase: 'ready', lastSyncedAt: now - 3 * 3600_000 }, now)).toBe('Synced 3 h ago')
  })
})
