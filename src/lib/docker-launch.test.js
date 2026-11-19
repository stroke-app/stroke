import { describe, it, expect, vi } from 'vitest'

vi.mock('@tauri-apps/api/event', () => ({ listen: async () => () => {} }))
vi.mock('$lib/api.js', () => ({ dockerCheck: async () => {}, dockerRunDb: async () => ({}) }))
vi.mock('$lib/components/ui/sonner/toast.svelte.js', () => ({ toast: { loading: () => 1, update: () => true, success: () => 1 } }))

const { readPullLine, pullFraction } = await import('./docker-launch.js')

describe('docker pull progress', () => {
  it('counts layers and moves them forward only', () => {
    const layers = new Map()
    expect(pullFraction(layers)).toBe(-1)
    expect(readPullLine(layers, '8.15.3: Pulling from elasticsearch/elasticsearch')).toBe(false)
    readPullLine(layers, 'a1b2c3d4e5f6: Pulling fs layer')
    readPullLine(layers, '0123456789ab: Already exists')
    expect(pullFraction(layers)).toBe(0.5)
    readPullLine(layers, 'a1b2c3d4e5f6: Downloading')
    readPullLine(layers, 'a1b2c3d4e5f6: Waiting')
    expect(layers.get('a1b2c3d4e5f6')).toBe('downloading')
    readPullLine(layers, 'a1b2c3d4e5f6: Pull complete')
    expect(pullFraction(layers)).toBe(1)
  })
})
