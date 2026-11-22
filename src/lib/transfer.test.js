import { describe, it, expect } from 'vitest'
import { seal, unseal, planImportAgainst, FORMAT } from './transfer-core.js'
const planImport = (/** @type {string} */ t) => planImportAgainst(t, new Set())

describe('transfer', () => {
  it('seals secrets so only the passphrase opens them', async () => {
    const secrets = { connections: { a: { password: 'p@ss' } }, ai: { m: 'sk-1' } }
    const sealed = await seal(secrets, 'correct horse')
    expect(JSON.stringify(sealed)).not.toContain('p@ss')
    expect(await unseal(sealed, 'correct horse')).toEqual(secrets)
    expect(await unseal(sealed, 'wrong')).toBeNull()
  })

  it('plans an import without changing anything, and refuses other files', () => {
    const plan = planImport(JSON.stringify({ format: FORMAT, version: 1, connections: [{ id: 'x', type: 'postgres', name: 'pg' }, { bad: true }], snippets: [{}], settings: {} }))
    expect('error' in plan).toBe(false)
    if ('error' in plan) return
    expect(plan.connections).toHaveLength(1)
    expect(plan.connections[0].existing).toBe(false)
    expect(plan.snippets).toBe(1)
    expect(plan.settings).toBe(true)
    expect(planImport('{"format":"other"}')).toEqual({ error: 'That is not a Stroke export file.' })
    expect(planImport(JSON.stringify({ format: FORMAT, version: 2 }))).toMatchObject({ error: expect.stringContaining('newer version') })
  })
})
