import { describe, it, expect, beforeEach } from 'vitest'
import {
  normalizeFolders, buildQueryTree, nextUntitledName, copyName, uniqueFolderName, planFolderDelete,
} from '$lib/query-folders.js'
import { loadQueryFolders, saveQueryFolders, queryFoldersKey } from '$lib/stores/query-folders.js'

/** @param {string} id @param {string} name @param {string | null} [folderId] @param {string} [sql] */
const q = (id, name, folderId = null, sql = `SELECT '${id}'`) => ({
  id, connectionId: 'c1', name, sql, createdAt: 1, updatedAt: 1, ...(folderId ? { folderId } : {}),
})

describe('buildQueryTree', () => {
  const folders = [{ id: 'f2', name: 'Reports' }, { id: 'f1', name: 'Admin' }]

  it('puts queries from before folders, and ones whose folder is gone, at the root: none is lost', () => {
    const queries = [q('a', 'old'), q('b', 'orphan', 'deleted-folder'), q('c', 'in admin', 'f1')]
    const tree = buildQueryTree(queries, folders)
    expect(tree.root.map((s) => s.id)).toEqual(['a', 'b'])
    expect(tree.folders.map((g) => g.folder.name)).toEqual(['Admin', 'Reports'])
    expect(tree.folders[0].queries.map((s) => s.id)).toEqual(['c'])
    const shown = tree.root.length + tree.folders.reduce((n, g) => n + g.queries.length, 0)
    expect(shown).toBe(queries.length)
  })

  it('sorts by name the way people count (query 2 before query 10), folders kept when empty', () => {
    const tree = buildQueryTree([q('a', 'query 10'), q('b', 'query 2')], folders)
    expect(tree.root.map((s) => s.name)).toEqual(['query 2', 'query 10'])
    expect(tree.folders.every((g) => g.queries.length === 0)).toBe(true)
  })

  it('filters by name or text; a folder shows for a match inside it, or all of it for its own name', () => {
    const queries = [
      q('a', 'users by day', 'f2', 'SELECT * FROM users'),
      q('b', 'orders', 'f2', 'SELECT * FROM orders'),
      q('c', 'grants', 'f1'),
      q('d', 'loose', null, 'select count(*) from users'),
    ]
    const byText = buildQueryTree(queries, folders, 'users')
    expect(byText.root.map((s) => s.id)).toEqual(['d'])
    expect(byText.folders.map((g) => [g.folder.name, g.queries.map((s) => s.id)])).toEqual([['Reports', ['a']]])
    const byFolder = buildQueryTree(queries, folders, 'report')
    expect(byFolder.folders.map((g) => g.queries.length)).toEqual([2])
    expect(byFolder.root).toEqual([])
  })
})

describe('names', () => {
  it('numbers untitled queries past the highest in use', () => {
    expect(nextUntitledName([])).toBe('Untitled query 1')
    expect(nextUntitledName([{ name: 'Untitled query 3' }, { name: 'Untitled query 1' }, { name: 'x' }])).toBe('Untitled query 4')
  })

  it('names copies and folders without clashing', () => {
    expect(copyName('daily', [{ name: 'daily' }])).toBe('daily copy')
    expect(copyName('daily', [{ name: 'daily copy' }, { name: 'daily copy 2' }])).toBe('daily copy 3')
    expect(uniqueFolderName([])).toBe('New folder')
    expect(uniqueFolderName([{ id: '1', name: 'new folder' }, { id: '2', name: 'New folder 2' }])).toBe('New folder 3')
  })
})

describe('planFolderDelete', () => {
  const queries = [q('a', 'a', 'f1'), q('b', 'b', 'f1'), q('c', 'c', 'f2'), q('d', 'd')]

  it('moves the folder\'s queries to the root unless told to delete them', () => {
    expect(planFolderDelete(queries, 'f1')).toEqual({ toRoot: ['a', 'b'], remove: [] })
    expect(planFolderDelete(queries, 'f1', true)).toEqual({ toRoot: [], remove: ['a', 'b'] })
    expect(planFolderDelete(queries, 'nope')).toEqual({ toRoot: [], remove: [] })
  })
})

describe('folder store', () => {
  beforeEach(() => {
    const map = new Map()
    globalThis.localStorage = /** @type {any} */ ({
      getItem: (k) => (map.has(k) ? map.get(k) : null),
      setItem: (k, v) => { map.set(k, String(v)) },
      removeItem: (k) => { map.delete(k) },
    })
  })

  it('round-trips per connection and forgets an emptied list', () => {
    saveQueryFolders('c1', [{ id: 'f1', name: 'Admin' }])
    saveQueryFolders('c2', [{ id: 'f9', name: 'Other' }])
    expect(loadQueryFolders('c1')).toEqual([{ id: 'f1', name: 'Admin' }])
    saveQueryFolders('c1', [])
    expect(localStorage.getItem(queryFoldersKey('c1'))).toBeNull()
    expect(loadQueryFolders('c2')).toHaveLength(1)
  })

  it('reads damaged data as a clean list', () => {
    localStorage.setItem(queryFoldersKey('c1'), 'not json')
    expect(loadQueryFolders('c1')).toEqual([])
    expect(normalizeFolders([{ id: 'a', name: '  ' }, { id: 'a', name: 'dup' }, null, { name: 'no id' }, 'x']))
      .toEqual([{ id: 'a', name: 'Folder' }])
    expect(normalizeFolders({ not: 'a list' })).toEqual([])
  })
})
