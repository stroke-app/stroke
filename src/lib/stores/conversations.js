import { getStudioDb, STORES } from '$lib/stores/studio-db.js'

const STORE = STORES.conversations

/**
 * @typedef {{
 *   id: string
 *   title: string
 *   createdAt: number
 *   updatedAt: number
 *   schema: string
 *   database?: string  - the database it last worked in (chats follow the connection)
 *   connectionId: string
 *   items: unknown[]
 *   apiHistory: unknown[]
 *   source?: 'chat' | 'sidebar'
 * }} Conversation
 */

function getDb() {
  return getStudioDb()
}

/**
 * @param {string | ((key: string) => boolean)} [connectionId] when provided, only return
 *   conversations for that connection: its key, or a test on the stored key (lib/ai-chat-key.js)
 * @param {'chat' | 'sidebar'} [source] when 'sidebar', return only sidebar chats; otherwise
 *   return full-chat conversations (excludes sidebar) so the two surfaces stay separate.
 * @returns {Promise<Conversation[]>} sorted newest first
 */
export async function listConversations(connectionId, source) {
  const db = await getDb()
  const all = await db.getAll(STORE)
  const belongs = typeof connectionId === 'function' ? connectionId : (/** @type {string} */ k) => k === connectionId
  let filtered = connectionId ? all.filter((c) => belongs(c.connectionId)) : all
  filtered = source === 'sidebar'
    ? filtered.filter((c) => c.source === 'sidebar')
    : filtered.filter((c) => c.source !== 'sidebar')
  return filtered.sort((a, b) => b.updatedAt - a.updatedAt)
}

/** @param {string} id @returns {Promise<Conversation | undefined>} */
export async function getConversation(id) {
  const db = await getDb()
  return db.get(STORE, id)
}

/** @param {Omit<Conversation, 'id' | 'createdAt' | 'updatedAt'>} data @returns {Promise<Conversation>} */
export async function createConversation(data) {
  const now = Date.now()
  const conv = /** @type {Conversation} */ ({
    id: crypto.randomUUID(),
    createdAt: now,
    updatedAt: now,
    ...data,
  })
  const db = await getDb()
  await db.put(STORE, conv)
  return conv
}

/** @param {string} id @param {Partial<Omit<Conversation, 'id' | 'createdAt'>>} patch */
export async function updateConversation(id, patch) {
  const db = await getDb()
  const existing = await db.get(STORE, id)
  if (!existing) return
  await db.put(STORE, { ...existing, ...patch, updatedAt: Date.now() })
}

/** @param {string} id */
export async function deleteConversation(id) {
  const db = await getDb()
  await db.delete(STORE, id)
}

/**
 * Delete every conversation matching the given connection + source.
 * @param {string} [connectionId]
 * @param {'chat' | 'sidebar'} [source]
 */
/** @param {string | ((key: string) => boolean)} [connectionId] @param {'chat' | 'sidebar'} [source] */
export async function clearConversations(connectionId, source) {
  const list = await listConversations(connectionId, source)
  const db = await getDb()
  await Promise.all(list.map((c) => db.delete(STORE, c.id)))
}
