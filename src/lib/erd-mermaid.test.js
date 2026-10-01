import { expect, test } from 'vitest'
import { erdToMermaid, mermaidId, mermaidType, relationsToFlowchart } from './erd-mermaid.js'

const tables = [
  { name: 'tenants', columns: [{ name: 'id', dataType: 'uuid' }, { name: 'name', dataType: 'text' }], pkCols: new Set(['id']) },
  {
    name: 'users',
    columns: [
      { name: 'id', dataType: 'uuid' },
      { name: 'tenant_id', dataType: 'uuid', foreignKey: 'public.tenants.id' },
      { name: 'created at', dataType: 'timestamp with time zone' },
      { name: 'email', dataType: 'varchar(255)' },
    ],
    pkCols: new Set(['id']),
    uniqueCols: new Set(['email']),
  },
  { name: 'profiles', columns: [{ name: 'user_id', dataType: 'uuid', foreignKey: 'public.users.id' }], pkCols: new Set(['user_id']) },
]
const rels = [
  { source: 'users', target: 'tenants', sourceCol: 'tenant_id', many: true, optional: false },
  { source: 'profiles', target: 'users', sourceCol: 'user_id', many: false, optional: true },
  { source: 'users', target: 'ghosts', sourceCol: 'ghost_id' },
]

test('identifiers and types become single Mermaid words', () => {
  expect(mermaidId('created at')).toBe('created_at')
  expect(mermaidId('2fa_codes')).toBe('_2fa_codes')
  expect(mermaidType('timestamp with time zone')).toBe('timestamp_with_time_zone')
  expect(mermaidType('varchar(255)')).toBe('varchar')
})

test('erDiagram: entities with key marks, crow\'s feet by cardinality, unknown tables skipped', () => {
  const src = erdToMermaid(tables, rels)
  expect(src.startsWith('erDiagram\n')).toBe(true)
  expect(src).toContain('    users {')
  expect(src).toContain('        uuid id PK')
  expect(src).toContain('        uuid tenant_id FK')
  expect(src).toContain('        varchar email UK')
  expect(src).toContain('        uuid user_id PK, FK')
  expect(src).toContain('    tenants ||--o{ users : tenant_id')
  expect(src).toContain('    users |o--|| profiles : user_id')
  expect(src).not.toContain('ghosts')
})

test('keys only drops the plain columns', () => {
  const src = erdToMermaid(tables, rels, { keysOnly: true })
  expect(src).not.toContain('email')
  expect(src).toContain('tenant_id FK')
})

test('flowchart: every table and labelled arrows, or just the focus and its neighbours', () => {
  const all = relationsToFlowchart(tables, rels)
  expect(all.startsWith('flowchart LR\n')).toBe(true)
  expect(all).toContain('    users["users"]')
  expect(all).toContain('    users -->|tenant_id| tenants')
  expect(all).not.toContain('ghosts')
  const around = relationsToFlowchart(tables, rels, { focus: 'tenants' })
  expect(around).toContain('tenants["tenants"]')
  expect(around).toContain('users["users"]')
  expect(around).not.toContain('profiles')
  expect(around).toContain('style tenants fill:var(--_group-hdr),stroke:var(--fg),stroke-width:1.5px')
})

test('flowchart depth reaches the neighbours of the neighbours, within the cap', () => {
  const two = relationsToFlowchart(tables, rels, { focus: 'tenants', depth: 2 })
  expect(two).toContain('profiles["profiles"]')
  expect(two).toContain('profiles -->|user_id| users')
  const capped = relationsToFlowchart(tables, rels, { focus: 'tenants', depth: 2, maxNodes: 2 })
  expect(capped).toContain('users["users"]')
  expect(capped).not.toContain('profiles')
})

test('merge: one arrow per pair named after its columns; plain drops the arrowhead', () => {
  const twice = [...rels, { source: 'users', target: 'tenants', sourceCol: 'owner_tenant_id' }]
  const merged = relationsToFlowchart(tables, twice, { merge: true })
  expect(merged).toContain('    users -->|tenant_id, owner_tenant_id| tenants')
  expect(merged.match(/users -->/g)?.length).toBe(1)
  const plain = relationsToFlowchart(tables, twice, { plain: true })
  expect(plain).toContain('    users ---|tenant_id, owner_tenant_id| tenants')
  expect(plain).not.toContain('-->')
})
