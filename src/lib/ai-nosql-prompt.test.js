import { describe, it, expect } from 'vitest'
import { buildSystemPrompt, parseAssistantMessage, setQueryEngine, isDestructiveSql } from './ai.js'

const base = { activeSchema: 'shop', schemas: ['shop'], tables: [{ name: 'users', rowCount: 3 }, { name: 'orders', rowCount: 1200 }] }

describe('AI prompt per engine', () => {
  it('MongoDB: shell syntax, its rules, no SQL rules', () => {
    const p = buildSystemPrompt({ ...base, dbType: 'mongodb' })
    expect(p).toContain("database assistant for MongoDB")
    expect(p).toContain('QUERY RULES (MongoDB shell')
    expect(p).toContain('db.users.find(')
    expect(p).not.toContain('=== SQL RULES ===')
    expect(p).not.toContain('pg_enum')
    expect(p).toContain('users (3), orders (1.2k)')
  })

  it('DynamoDB and Cassandra get their SQL-like dialect rules', () => {
    expect(buildSystemPrompt({ ...base, dbType: 'dynamodb' })).toContain('PartiQL for DynamoDB')
    expect(buildSystemPrompt({ ...base, dbType: 'cassandra' })).toContain('ALLOW FILTERING')
  })

  it('Postgres is unchanged', () => {
    const p = buildSystemPrompt({ ...base, dbType: 'postgres' })
    expect(p).toContain('=== SQL RULES ===')
    expect(p).not.toContain('QUERY RULES')
  })
})

describe('runnable blocks and destructive statements', () => {
  it('a ```javascript block runs on MongoDB, not on Postgres', () => {
    const msg = 'Here:\n```javascript\ndb.users.find({}).limit(5)\n```'
    setQueryEngine('mongodb')
    expect(parseAssistantMessage(msg).some((p) => p.type === 'sql')).toBe(true)
    setQueryEngine('postgres')
    expect(parseAssistantMessage(msg).some((p) => p.type === 'sql')).toBe(false)
    setQueryEngine(null)
  })
  it('flags bulk deletes and flushes', () => {
    expect(isDestructiveSql('db.users.deleteMany({plan: "free"})')).toBe(true)
    expect(isDestructiveSql('db.users.drop()')).toBe(true)
    expect(isDestructiveSql('FLUSHALL')).toBe(true)
    expect(isDestructiveSql('db.users.find({})')).toBe(false)
  })
})
