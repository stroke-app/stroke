import { get } from 'svelte/store'
import { agentStart, agentSend, agentInterrupt, agentStop } from '$lib/api.js'
import {
  agentPrefs,
  AGENTS_WITH_EFFORT,
  agentSystemPrompt,
  strokeToolName,
  toolOutputText,
  cleanToolError,
} from '$lib/local-agents.js'
import { isReadOnly } from '$lib/stores/read-only.js'

/**
 * @typedef {{ dbType?: string, activeSchema?: string, activeTable?: string | null, tables?: string[], extra?: string, webAccess?: boolean }} AgentChatContext
 * @typedef {{
 *   text?: (delta: string) => void,
 *   toolCall?: (call: { id: string, tool: string, input: any }) => void,
 *   toolResult?: (res: { id: string, tool: string, text: string, isError: boolean }) => void,
 *   failed?: (message: string) => void,
 * }} AgentTurnHandlers
 */

/**
 * One chat's link to a local agent CLI (Claude Code, Codex, OpenCode).
 *
 * Starts the CLI session on first use and keeps it while the agent, model,
 * effort and access level stay the same; a change restarts it, resuming the
 * CLI's own session so the conversation carries on. A turn's events come back
 * through plain callbacks, so a chat surface only decides how to draw them.
 */
export function createAgentChat() {
  /** @type {{ key: string, agent: string, model: string, effort: string, access: string, ready: Promise<void> } | null} */
  let session = null
  /** The CLI's own session id, to resume after a restart. @type {{ agent: string, id: string } | null} */
  let native = null
  /** @type {((ev: any) => void) | null} */
  let onEvent = null
  /** @type {(() => void) | null} */
  let settle = null
  let key = `chat-${crypto.randomUUID()}`

  /**
   * End the running session. Resolves once the backend has let it go, so a
   * restart under the same key (a changed access level, effort or model) never
   * races the old process shutting down and gets stopped along with it.
   * @returns {Promise<void>}
   */
  function stop() {
    const s = session
    session = null
    return s ? agentStop(s.key).then(() => {}, () => {}) : Promise.resolve()
  }

  /**
   * @param {string} agent @param {string} model @param {AgentChatContext} ctx
   * @returns {Promise<string>} the backend key to send to
   */
  async function ensure(agent, model, ctx) {
    const prefs = get(agentPrefs)
    const effort = AGENTS_WITH_EFFORT.has(agent) ? prefs.effort : ''
    // A connection opened read-only stays read-only for the agent too.
    /** @type {import('$lib/local-agents.js').AgentAccess} */
    const access = isReadOnly() ? 'read' : prefs.access
    const s = session
    // Web access is a launch flag on the agent, so turning it on or off starts a fresh session.
    const webAccess = !!ctx.webAccess
    if (s?.agent === agent && s.model === model && s.effort === effort && s.access === access && s.webAccess === webAccess) {
      await s.ready
      return s.key
    }
    await stop()
    /** @type {(v?: unknown) => void} */
    let started = () => {}
    /** @type {(e: unknown) => void} */
    let failed = () => {}
    /** @type {Promise<void>} */
    const ready = new Promise((res, rej) => { started = res; failed = rej })
    ready.catch(() => {})
    const next = { key, agent, model, effort, access, webAccess, ready }
    session = next
    const prompt = agentSystemPrompt({ ...ctx, access, webAccess }) + (ctx.extra ? `\n\n${ctx.extra}` : '')
    try {
      await agentStart(
        {
          conversationId: key,
          agent,
          model,
          systemPrompt: prompt,
          resume: native?.agent === agent ? native.id : null,
          effort: effort || null,
          access,
          webAccess,
        },
        (ev) => {
          if (session !== next) return
          const id = ev.Started?.native_session_id
          if (id) native = { agent, id }
          if (ev.Fatal) session = null
          onEvent?.(ev)
        },
      )
      started()
    } catch (e) {
      if (session === next) session = null
      failed(e)
      throw e
    }
    return key
  }

  /**
   * Run one turn. Resolves when the agent ends it (or `interrupt` does).
   * @param {{ agent: string, model: string, text: string, ctx: AgentChatContext, on: AgentTurnHandlers }} turn
   */
  async function runTurn({ agent, model, text, ctx, on }) {
    const sendKey = await ensure(agent, model, ctx)
    /** @type {Map<string, string>} tool call id -> Stroke tool name */
    const tools = new Map()
    await new Promise((resolve, reject) => {
      /** @param {Error | null} err */
      const done = (err) => {
        onEvent = null
        settle = null
        if (err) reject(err)
        else resolve(undefined)
      }
      settle = () => done(null)
      onEvent = (ev) => {
        if (ev.Text) on.text?.(ev.Text.delta)
        else if (ev.ToolCall) {
          const id = String(ev.ToolCall.id)
          const tool = strokeToolName(String(ev.ToolCall.name ?? ''))
          // A CLI may report the same call again once its input is complete.
          const repeat = tools.has(id)
          tools.set(id, tool)
          if (!repeat) on.toolCall?.({ id, tool, input: ev.ToolCall.input ?? {} })
        } else if (ev.ToolResult) {
          const id = String(ev.ToolResult.id)
          const out = toolOutputText(ev.ToolResult.output)
          const isError = !!ev.ToolResult.is_error
          on.toolResult?.({ id, tool: tools.get(id) ?? '', text: isError ? cleanToolError(out) : out, isError })
        } else if (ev.TurnFailed) on.failed?.(ev.TurnFailed.message)
        else if (ev.TurnEnd) done(null)
        else if (ev.Fatal) done(new Error(ev.Fatal.message))
      }
      agentSend(sendKey, text).catch(async (e) => {
        if (String(e) !== 'NO_SESSION') return done(e instanceof Error ? e : new Error(String(e)))
        // The CLI exited between turns: start it again, resuming, and resend.
        session = null
        try {
          await agentSend(await ensure(agent, model, ctx), text)
        } catch (e2) {
          done(e2 instanceof Error ? e2 : new Error(String(e2)))
        }
      })
    })
  }

  /** Stop the running turn; the session stays for the next message. */
  function interrupt() {
    if (session && onEvent) void agentInterrupt(session.key).catch(() => {})
    settle?.()
  }

  /** A new conversation: a fresh CLI session, nothing resumed. */
  function reset() {
    interrupt()
    stop()
    native = null
    key = `chat-${crypto.randomUUID()}`
  }

  return { runTurn, interrupt, stop, reset }
}
