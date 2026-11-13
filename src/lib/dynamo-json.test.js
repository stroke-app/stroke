import { describe, it, expect } from 'vitest'
import {
  unmarshall, unmarshallItem, marshall, marshallItem, cellText, columnsFor, keyShapes,
  keyId, queryBody, scanBody, saveRequest, newItemTemplate, plainNumber, dynamoGrid,
} from './dynamo-json.js'

const item = {
  pk: { S: 'USER#1' },
  sk: { S: 'PROFILE' },
  age: { N: '31' },
  big: { N: '12345678901234567890' },
  tags: { SS: ['beta', 'admin'] },
  scores: { NS: ['1', '2.5'] },
  blob: { B: 'aGk=' },
  prefs: { M: { theme: { S: 'dark' }, notify: { BOOL: true }, gone: { NULL: true } } },
  hist: { L: [{ S: 'a' }, { N: '2' }, { SS: ['x'] }] },
}

describe('plain <-> typed', () => {
  it('unmarshalls every type, keeping unsafe numbers as text', () => {
    expect(unmarshallItem(item)).toEqual({
      pk: 'USER#1', sk: 'PROFILE', age: 31, big: '12345678901234567890', tags: ['beta', 'admin'],
      scores: [1, 2.5], blob: 'aGk=', prefs: { theme: 'dark', notify: true, gone: null }, hist: ['a', 2, ['x']],
    })
    expect(plainNumber('1.50')).toBe('1.50')
  })

  it('round-trips through plain JSON with the original as hint', () => {
    expect(marshallItem(unmarshallItem(item), item)).toEqual(item)
  })

  it('without a hint, arrays are lists and strings are strings', () => {
    expect(marshall(['a', 'b'])).toEqual({ L: [{ S: 'a' }, { S: 'b' }] })
    expect(marshall('12')).toEqual({ S: '12' })
    expect(marshall(12)).toEqual({ N: '12' })
  })

  it('a set that gains a duplicate or goes empty becomes a list', () => {
    expect(marshall(['a', 'a'], { SS: ['a'] })).toEqual({ L: [{ S: 'a' }, { S: 'a' }] })
    expect(marshall([], { SS: ['a'] })).toEqual({ L: [] })
  })

  it('a numeric attribute edited to non-numeric text becomes a string', () => {
    expect(marshall('n/a', { N: '1' })).toEqual({ S: 'n/a' })
  })

  it('unmarshall leaves non-objects alone', () => {
    expect(unmarshall(undefined)).toBe(undefined)
  })
})

describe('display', () => {
  it('cell text', () => {
    expect(cellText(item.tags)).toBe('{beta, admin}')
    expect(cellText(item.prefs)).toBe('{"theme":"dark","notify":true,"gone":null}')
    expect(cellText({ S: 'x'.repeat(10) }, 4)).toBe('xxxx…')
    expect(cellText(undefined)).toBe('')
  })
  it('columns put keys first', () => {
    expect(columnsFor([{ b: {}, pk: {} }, { a: {} }], ['pk', 'sk'])).toEqual(['pk', 'sk', 'b', 'a'])
  })
})

const table = {
  TableName: 't',
  AttributeDefinitions: [
    { AttributeName: 'pk', AttributeType: 'S' },
    { AttributeName: 'sk', AttributeType: 'S' },
    { AttributeName: 'g', AttributeType: 'N' },
  ],
  KeySchema: [{ AttributeName: 'pk', KeyType: 'HASH' }, { AttributeName: 'sk', KeyType: 'RANGE' }],
  GlobalSecondaryIndexes: [{ IndexName: 'byG', KeySchema: [{ AttributeName: 'g', KeyType: 'HASH' }] }],
}
const [tk, gsi] = keyShapes(table)

describe('requests', () => {
  it('reads key shapes', () => {
    expect(tk).toEqual({ name: 't', kind: 'table', hash: { name: 'pk', type: 'S' }, range: { name: 'sk', type: 'S' } })
    expect(gsi).toEqual({ name: 'byG', kind: 'gsi', hash: { name: 'g', type: 'N' }, range: null })
  })

  it('builds queries', () => {
    expect(queryBody('t', tk, { hash: 'U#1', sortOp: 'begins_with', sort: 'ORD', descending: true })).toEqual({
      TableName: 't',
      KeyConditionExpression: '#h = :h AND begins_with(#r, :r)',
      ExpressionAttributeNames: { '#h': 'pk', '#r': 'sk' },
      ExpressionAttributeValues: { ':h': { S: 'U#1' }, ':r': { S: 'ORD' } },
      ScanIndexForward: false,
      Limit: 100,
    })
    const b = queryBody('t', tk, { hash: 'x', sortOp: 'between', sort: 'a', sort2: 'm' })
    expect(b.KeyConditionExpression).toBe('#h = :h AND #r BETWEEN :r AND :r2')
    const g = queryBody('t', gsi, { hash: '7', startKey: { pk: { S: 'a' } } })
    expect(g.IndexName).toBe('byG')
    expect(g.ExpressionAttributeValues[':h']).toEqual({ N: '7' })
    expect(g.ExclusiveStartKey).toEqual({ pk: { S: 'a' } })
    expect(() => queryBody('t', gsi, { hash: 'seven' })).toThrow('not a number')
    // A sort operator with no value is ignored.
    expect(queryBody('t', tk, { hash: 'x', sortOp: '=', sort: '' }).KeyConditionExpression).toBe('#h = :h')
  })

  it('builds scans', () => {
    expect(scanBody('t', gsi, { limit: 5 })).toEqual({ TableName: 't', Limit: 5, IndexName: 'byG' })
  })

  it('saves: overwrite, create, and key change', () => {
    const prev = { pk: { S: 'a' }, sk: { S: '1' }, v: { N: '1' } }
    expect(saveRequest('t', tk, { ...prev, v: { N: '2' } }, prev).op).toBe('PutItem')
    const created = saveRequest('t', tk, prev, null)
    expect(created.body.ConditionExpression).toBe('attribute_not_exists(#h)')
    const moved = saveRequest('t', tk, { ...prev, sk: { S: '2' } }, prev)
    expect(moved.op).toBe('TransactWriteItems')
    expect(moved.body.TransactItems[1]).toEqual({ Delete: { TableName: 't', Key: { pk: { S: 'a' }, sk: { S: '1' } } } })
    expect(() => saveRequest('t', tk, { pk: { S: 'a' } }, null)).toThrow('key attribute "sk"')
    expect(() => saveRequest('t', tk, { pk: { N: '1' }, sk: { S: 'x' } }, null)).toThrow('must be a string')
  })

  it('key ids ignore attribute order and non-key attributes', () => {
    expect(keyId({ sk: { S: '1' }, pk: { S: 'a' }, x: { S: 'y' } }, tk)).toBe(keyId({ pk: { S: 'a' }, sk: { S: '1' } }, tk))
  })

  it('new item template', () => {
    expect(newItemTemplate(tk)).toEqual({ pk: '', sk: '' })
    expect(newItemTemplate(gsi)).toEqual({ g: 0 })
  })
})

describe('dynamoGrid', () => {
  it('types columns by attribute type, keys first', () => {
    const g = dynamoGrid([item, { pk: { S: 'USER#2' }, sk: { S: 'X' }, age: { S: 'n/a' } }], ['pk', 'sk'])
    const cols = Object.fromEntries(g.columns.map((c) => [c.name, c.data_type]))
    expect(g.columns.slice(0, 2).map((c) => c.name)).toEqual(['pk', 'sk'])
    expect(cols).toMatchObject({ pk: 'string', age: 'mixed', tags: 'string set', prefs: 'map', hist: 'list', blob: 'binary' })
    expect(g.rows[0][g.columns.findIndex((c) => c.name === 'tags')]).toEqual(['beta', 'admin'])
    expect(g.rows[1][g.columns.findIndex((c) => c.name === 'tags')]).toBe(null)
  })
})
