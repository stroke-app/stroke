/**
 * The AI chat's list of conversations, shared with the sidebar when AI chat
 * opens as a tab (beta). AiChat owns the conversations; it publishes what the
 * sidebar shows and takes the sidebar's requests (open this chat, start a new
 * one, rename, delete) through here.
 *
 * Before the AI tab has mounted there is no AiChat to publish, so the sidebar
 * reads the stored list itself, and a request waits in `pending` for the tab
 * to pick it up.
 */
import { writable, get } from 'svelte/store'

/**
 * @typedef {{ id: string, title: string, updatedAt: number, where?: string }} ChatRow  - `where`: "database · schema" it last worked in
 * @typedef {{ connectionId: string, chats: ChatRow[], activeId: string | null, busyId: string | null, live: boolean }} ChatListState
 * @typedef {{ type: 'open', id: string } | { type: 'new' }} ChatRequest
 */

/** @type {import('svelte/store').Writable<ChatListState>} */
export const aiChatList = writable({ connectionId: '', chats: [], activeId: null, busyId: null, live: false })

/** A request the mounted AiChat has not taken yet. @type {import('svelte/store').Writable<ChatRequest | null>} */
export const aiChatPending = writable(null)

/** @param {Partial<ChatListState>} patch */
export function publishChatList(patch) {
  aiChatList.update((s) => ({ ...s, ...patch }))
}

/** @param {ChatRequest} req */
export function requestChat(req) {
  aiChatPending.set(req)
}

/** The waiting request, cleared. */
export function takeChatRequest() {
  const req = get(aiChatPending)
  if (req) aiChatPending.set(null)
  return req
}
