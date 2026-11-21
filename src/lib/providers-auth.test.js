import { describe, it, expect } from 'vitest'
import { AUTH_FAILURE } from './providers.js'

describe('AUTH_FAILURE', () => {
  it("recognizes Prisma Postgres's rejected credentials", () => {
    const prisma = 'Connection failed: error returned from database: Failed to identify your database: Your Postgres credentials are incorrect. Please check your username and password and try again.'
    expect(AUTH_FAILURE.test(prisma)).toBe(true)
  })
  it('still recognizes the usual engine wording', () => {
    expect(AUTH_FAILURE.test('password authentication failed for user "x"')).toBe(true)
    expect(AUTH_FAILURE.test('Access denied for user (1045)')).toBe(true)
  })
  it('leaves network errors alone', () => {
    expect(AUTH_FAILURE.test('Connection refused (os error 111)')).toBe(false)
    expect(AUTH_FAILURE.test('timed out')).toBe(false)
  })
})
