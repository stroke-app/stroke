import { describe, it, expect, beforeEach } from 'vitest'
import { get } from 'svelte/store'
import {
  normalizeSnippet,
  snippetsFor,
  snippetsToJson,
  snippetsFromJson,
  importSnippets,
  userSnippets,
  saveSnippet,
  removeSnippet,
} from './sql-snippets.js'

beforeEach(() => userSnippets.set([]))

describe('sql snippets', () => {
  it('keeps a snippet with a name, a one-word trigger and a body', () => {
    const s = normalizeSnippet({ name: ' Recent ', trigger: 'recent', body: 'SELECT 1  \n', scope: ['postgres', 'nope'] })
    expect(s).toMatchObject({ name: 'Recent', trigger: 'recent', body: 'SELECT 1', scope: ['postgres'] })
    expect(normalizeSnippet({ name: 'x', trigger: 'two words', body: 'SELECT 1' })).toBeNull()
    expect(normalizeSnippet({ name: '', trigger: 'x', body: 'SELECT 1' })).toBeNull()
    expect(normalizeSnippet({ name: 'x', trigger: 'x', body: '  ' })).toBeNull()
  })

  it('offers a snippet to its engines, or to every engine when it names none', () => {
    const all = normalizeSnippet({ name: 'a', trigger: 'a', body: 'x' })
    const pg = normalizeSnippet({ name: 'b', trigger: 'b', body: 'x', scope: ['postgres'] })
    const list = /** @type {any[]} */ ([all, pg])
    expect(snippetsFor(list, 'postgres').map((s) => s.trigger)).toEqual(['a', 'b'])
    expect(snippetsFor(list, 'mysql').map((s) => s.trigger)).toEqual(['a'])
  })

  it('round-trips through JSON and reads a bare list with prefix/sql keys', () => {
    saveSnippet({ name: 'Recent', trigger: 'recent', body: 'SELECT ${1:*}', description: 'd', scope: [] })
    const back = snippetsFromJson(snippetsToJson(get(userSnippets)))
    expect('snippets' in back && back.snippets[0]).toMatchObject({ name: 'Recent', trigger: 'recent', body: 'SELECT ${1:*}', description: 'd' })
    const bare = snippetsFromJson(JSON.stringify([{ name: 'Sel', prefix: 'sel', sql: 'SELECT 1' }, { name: 'bad' }]))
    expect(bare).toMatchObject({ skipped: 1 })
    expect(snippetsFromJson('nope')).toEqual({ error: 'That file is not JSON.' })
  })

  it('re-importing a file updates the same snippets instead of doubling them', () => {
    const file = snippetsFromJson(JSON.stringify([{ name: 'Sel', trigger: 'sel', body: 'SELECT 1' }]))
    if (!('snippets' in file)) throw new Error('parse')
    expect(importSnippets(file.snippets)).toEqual({ added: 1, replaced: 0 })
    expect(importSnippets(file.snippets)).toEqual({ added: 0, replaced: 1 })
    expect(get(userSnippets)).toHaveLength(1)
    removeSnippet(get(userSnippets)[0].id)
    expect(get(userSnippets)).toHaveLength(0)
  })
})
