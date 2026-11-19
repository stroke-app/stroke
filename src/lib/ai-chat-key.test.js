import { describe, it, expect } from 'vitest'
import { aiChatKey, chatBelongsTo } from './ai-chat-key.js'

describe('aiChatKey', () => {
  it('is the same for every database on one server and login', () => {
    const a = { type: 'postgres', host: '127.0.0.1', port: 5432, user: 'me', database: 'cptpayment' }
    const b = { ...a, database: 'ecommerce' }
    expect(aiChatKey(a)).toBe(aiChatKey(b))
    expect(aiChatKey({ ...a, user: 'other' })).not.toBe(aiChatKey(a))
  })

  it('tells host-less engines apart instead of sharing one list', () => {
    const keys = [
      aiChatKey({ type: 'sqlite', filePath: '/a.db' }),
      aiChatKey({ type: 'sqlite', filePath: '/b.db' }),
      aiChatKey({ type: 'mongodb', uri: 'mongodb://u:secret@localhost:27017/shop' }),
      aiChatKey({ type: 'd1', accountId: 'acc1', databaseId: 'x' }),
      aiChatKey({ type: 'firestore', projectId: 'p1' }),
      aiChatKey({ type: 'dynamodb', region: 'us-east-1' }),
    ]
    expect(new Set(keys).size).toBe(keys.length)
    expect(keys.join(' ')).not.toContain('undefined')
  })

  it('keeps credentials and the database out of a URL key', () => {
    const key = aiChatKey({ type: 'mongodb', uri: 'mongodb://u:secret@localhost:27017/shop?tls=true' })
    expect(key).not.toContain('secret')
    expect(key).toBe(aiChatKey({ type: 'mongodb', uri: 'mongodb://u:secret@localhost:27017/other' }))
  })
})

describe('chatBelongsTo', () => {
  const conn = { type: 'postgres', host: 'db', port: 5432, user: 'me', database: 'shop' }
  it('finds chats saved per database before 2.6', () => {
    expect(chatBelongsTo('db:5432/shop@me', conn)).toBe(true)
    expect(chatBelongsTo('db:5432/billing@me', conn)).toBe(true)
    expect(chatBelongsTo('db:5432/shop@someone', conn)).toBe(false)
    expect(chatBelongsTo(aiChatKey(conn), conn)).toBe(true)
  })
})
