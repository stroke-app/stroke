import { describe, it, expect } from 'vitest'
import { readFileSync } from 'node:fs'
import { KINDS } from './engine.js'

// The Rust side encrypts every pushed record and refuses kinds it doesn't
// list, which fails the whole push. Keep the two lists the same.
describe('sync kinds', () => {
  it('match the Rust list', () => {
    const rs = readFileSync(new URL('../../../src-tauri/src/sync.rs', import.meta.url), 'utf8')
    const m = /pub const KINDS: \[&str; \d+\] = \[([^\]]*)\]/.exec(rs)
    expect(m).toBeTruthy()
    const rust = [...m[1].matchAll(/"([^"]+)"/g)].map((x) => x[1])
    expect([...rust].sort()).toEqual([...KINDS].sort())
  })
})
