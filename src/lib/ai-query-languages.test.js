import { describe, it, expect } from 'vitest'
import { QUERY_LANGUAGES, queryLanguageFor, queryLanguagePrompt, sampleQuery, describeFromSample, runnableFences } from './ai-query-languages.js'

describe('query languages for the AI', () => {
  it('covers every NoSQL engine and nothing SQL', () => {
    for (const e of ['mongodb', 'couchdb', 'firestore', 'cassandra', 'dynamodb', 'redis', 'valkey']) expect(queryLanguageFor(e)).toBeTruthy()
    for (const e of ['postgres', 'mysql', 'sqlite', '', null, undefined]) expect(queryLanguageFor(e)).toBe(null)
  })

  it('builds a prompt with collections and sampled fields', () => {
    const p = queryLanguagePrompt('mongodb', { schema: 'shop', collections: ['users', 'orders'], sample: { users: ['_id', 'name', 'plan'] } })
    expect(p).toContain('MongoDB shell')
    expect(p).toContain('```javascript')
    expect(p).toContain('Collections in shop (2):')
    expect(p).toContain('- users: _id, name, plan')
    expect(p).toContain('- orders')
    expect(queryLanguagePrompt('postgres')).toBe('')
  })

  it("each guide's examples avoid SQL-only habits where they don't apply", () => {
    expect(QUERY_LANGUAGES.redis.guide).toMatch(/Never use KEYS/)
    expect(QUERY_LANGUAGES.dynamodb.guide).toMatch(/no LIMIT clause/)
    expect(QUERY_LANGUAGES.cassandra.guide).toMatch(/ALLOW FILTERING/)
  })
})

describe('sampling and describing', () => {
  it('samples in each engine\'s language, never with a scan', () => {
    expect(sampleQuery('mongodb', 'shop', 'users', 3)).toBe('db.users.find({}).limit(3)')
    expect(sampleQuery('mongodb', 'shop', 'my-coll', 3)).toBe('db.getCollection("my-coll").find({}).limit(3)')
    expect(sampleQuery('couchdb', 'main', 'products', 2)).toBe('products.find({}, {"limit": 2})')
    expect(sampleQuery('firestore', 'main', 'users', 2)).toBe('users.limit(2)')
    expect(sampleQuery('cassandra', 'shop', 'users', 2)).toBe('SELECT * FROM "shop"."users" LIMIT 2')
    expect(sampleQuery('dynamodb', 'main', 't', 2)).toBe(null)
    expect(sampleQuery('redis', '', 'k', 2)).toBe(null)
  })
  it('describes fields with their types', () => {
    const d = describeFromSample('users', { columns: [{ name: '_id' }, { name: 'tags' }, { name: 'age' }], rows: [['a', ['x'], 3], ['b', null, '3']] })
    expect(d.fields).toEqual([{ name: '_id', types: ['string'] }, { name: 'tags', types: ['array'] }, { name: 'age', types: ['number', 'string'] }])
    expect(d.sample).toHaveLength(2)
  })
  it('runnable fences per engine', () => {
    expect(runnableFences('mongodb')?.has('javascript')).toBe(true)
    expect(runnableFences('dynamodb')?.has('sql')).toBe(true)
    expect(runnableFences('postgres')).toBe(null)
  })
})
