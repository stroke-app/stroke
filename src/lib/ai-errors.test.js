import { describe, it, expect } from 'vitest'
import { buildSystemPrompt, classifyDbError, describeAiError, humanizeDbError } from './ai.js'

describe('humanizeDbError', () => {
  it('lifts the cause out of a D1 HTTP envelope', () => {
    const raw =
      'Error: D1 API error 400 Bad Request: {"messages":[],"result":[],"success":false,' +
      '"errors":[{"code":7500,"message":"no such column: activated at offset 51: SQLITE_ERROR"}]}'
    expect(humanizeDbError(raw)).toBe('no such column: activated')
  })

  it('reads a bare {message} envelope', () => {
    expect(humanizeDbError('boom: {"message":"relation \\"users\\" does not exist"}'))
      .toBe('relation "users" does not exist')
  })

  it('recovers the message from a truncated payload it cannot parse', () => {
    // A cut-off body still contains the field; JSON.parse on it throws.
    expect(humanizeDbError('API error: {"errors":[{"message":"syntax error near FROM"'))
      .toBe('syntax error near FROM')
  })

  it('passes a plain driver message through, minus the Error: prefix', () => {
    expect(humanizeDbError('Error: permission denied for table users'))
      .toBe('permission denied for table users')
  })

  it('never returns empty', () => {
    expect(humanizeDbError('')).toBe('Query failed')
    expect(humanizeDbError(null)).toBe('Query failed')
    expect(humanizeDbError('   ')).toBe('Query failed')
  })

  it('leaves a message with no engine bookkeeping untouched', () => {
    expect(humanizeDbError('division by zero')).toBe('division by zero')
  })
})

describe('describeAiError', () => {
  const OMNIROUTE_503 =
    'AI API 503: {"error":{"message":"[503]: Upstream request failed: Endpoint is unavailable.",' +
    '"type":"server_error","code":"service_unavailable"},"diagnostics":{"poolSize":6,"attempted":0,' +
    '"excluded":[{"provider":"groq","reason":"cooling down after 429"},{"provider":"cerebras","reason":"no api key"}]}}'

  it('never puts raw JSON in the headline', () => {
    const d = describeAiError(OMNIROUTE_503)
    expect(d.title).toBe('The AI provider is unavailable')
    expect(d.title).not.toContain('{')
    expect(d.hint).not.toContain('{')
  })

  it('answers "why" from the router diagnostics', () => {
    const d = describeAiError(OMNIROUTE_503)
    expect(d.hint).toContain('None of the 6 endpoints')
    expect(d.hint).toContain('nothing was tried')
    expect(d.hint).toContain('cooling down after 429')
    expect(d.hint).toContain('no api key')
  })

  it('keeps the provider payload for the details view', () => {
    expect(describeAiError(OMNIROUTE_503).detail).toContain('"poolSize":6')
  })

  it('reads the status out of the thrown message', () => {
    expect(describeAiError('AI API 429: {"error":{"message":"slow down"}}').status).toBe(429)
    expect(describeAiError('AI API 401: nope').status).toBe(401)
    expect(describeAiError('network unreachable').status).toBeNull()
  })

  it('strips the status the provider repeated inside its own message', () => {
    const d = describeAiError('AI API 503: {"error":{"message":"[503]: Upstream request failed."}}')
    expect(d.detail.startsWith('[503]:')).toBe(false)
    expect(d.detail).toContain('Upstream request failed.')
  })

  it('recovers the message from a payload cut off mid-JSON', () => {
    // The Rust bridge forwards only the first slice of the body.
    const d = describeAiError('AI API 500: {"error":{"message":"internal failure","cod')
    expect(d.detail).toContain('internal failure')
  })

  it('titles the common statuses in plain words', () => {
    expect(describeAiError('AI API 429: {}').title).toBe('Rate limit reached')
    expect(describeAiError('AI API 401: {}').title).toBe('The provider rejected the API key')
    expect(describeAiError('AI API 404: {}').title).toBe('Model not found at this endpoint')
    expect(describeAiError('boom').title).toBe('The AI request failed')
  })
})

describe('describeAiError · quota', () => {
  const QUOTA_MESSAGE =
    "You've used today's free AI requests. They reset at midnight UTC - or add your own API key in Settings → AI."
  const QUOTA = `AI API 429: ${JSON.stringify({
    error: { code: 'device_quota_exhausted', message: QUOTA_MESSAGE, type: 'rate_limit_error' },
  })}`

  it('leads with the provider\'s own sentence, not the canned one', () => {
    const d = describeAiError(QUOTA)
    expect(d.title).toBe('Rate limit reached')
    expect(d.hint).toBe(QUOTA_MESSAGE)
    expect(d.hint).not.toContain('check your plan and usage limits')
  })

  it('hides an envelope that only repeats the message', () => {
    // The banner already says it; a Details toggle that reveals the same
    // sentence wrapped in JSON invites a click and teaches nothing.
    expect(describeAiError(QUOTA).detail).toBe('')
  })

  it('keeps a payload that carries more than the message', () => {
    const extra = `AI API 429: ${JSON.stringify({
      error: { message: QUOTA_MESSAGE, retry_after_seconds: 900 },
    })}`
    expect(describeAiError(extra).detail).toContain('retry_after_seconds')
  })
})

describe('classifyDbError with the failing SQL', () => {
  it('names ANSI_QUOTES when a double-quoted value was read as a column', () => {
    const hint = classifyDbError(
      "Statement failed: error returned from database: 1054 (42S22): Unknown column 'carol white' in 'field list'",
      'INSERT INTO customers (name) VALUES ("Carol White")',
    )
    expect(hint).toContain('ANSI_QUOTES')
    expect(hint).toContain("'carol white'")
  })

  it('keeps the schema hint for a genuinely missing column', () => {
    const hint = classifyDbError("Unknown column 'emial' in 'field list'", 'SELECT emial FROM customers')
    expect(hint).not.toContain('ANSI_QUOTES')
    expect(hint).toContain('describe_table')
  })
})

describe('the plan instruction', () => {
  const ctx = { dbType: 'mysql', schemas: ['shop'], activeSchema: 'shop', tables: [], topics: new Set() }

  it('is only in the prompt of a chat that draws plans', () => {
    expect(buildSystemPrompt({ ...ctx, planSteps: true })).toContain('<plan>')
    expect(buildSystemPrompt(ctx)).not.toContain('<plan>')
  })

  it('tells a MySQL model about single quotes and pooled USE', () => {
    const p = buildSystemPrompt({ ...ctx, topics: undefined })
    expect(p).toContain('ANSI_QUOTES')
    expect(p).toContain('USE does not carry over')
  })
})

it("a request with no key is called a missing key, not a rejected one", async () => {
  const { describeAiError } = await import('./ai.js')
  const f = describeAiError('AI API 401: {"error":{"message":"No cookie auth credentials found","code":401}}')
  expect(f.title).toBe('No API key was sent')
  expect(f.hint).toMatch(/Add the API key/)
})
