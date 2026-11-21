import { describe, it, expect } from 'vitest'
import { isNetworkError, connectionErrorKind } from './utils.js'

describe('connectionErrorKind', () => {
  it('reads sqlx closed-pool errors as a dropped connection', () => {
    const raw = 'Failed to count rows: attempted to acquire a connection on a closed pool'
    expect(connectionErrorKind(raw)).toBe('dropped')
    // Also has to reach `isNetworkError`, which is what triggers the silent
    // reconnect - a closed pool that reads as a SQL error never heals.
    expect(isNetworkError(raw)).toBe(true)
  })

  it('separates an unreachable host from a dropped pool', () => {
    expect(connectionErrorKind('tcp connect error: Connection refused (os error 111)')).toBe('unreachable')
    expect(connectionErrorKind('server closed the connection unexpectedly')).toBe('dropped')
  })

  it('leaves SQL errors alone', () => {
    expect(connectionErrorKind('relation "users" does not exist')).toBe(null)
    expect(connectionErrorKind('canceling statement due to statement timeout')).toBe(null)
    expect(isNetworkError('relation "users" does not exist')).toBe(false)
  })

  it('survives a null or undefined message', () => {
    expect(connectionErrorKind(null)).toBe(null)
    expect(isNetworkError(undefined)).toBe(false)
  })
})

describe('errorText', () => {
  it('reads an error as its message, without the class name', async () => {
    const { errorText } = await import('./utils.js')
    expect(errorText(new Error("Can't reach 127.0.0.1:15433"))).toBe("Can't reach 127.0.0.1:15433")
    expect(errorText('Error: boom')).toBe('boom')
    expect(errorText('Login failed')).toBe('Login failed')
    expect(errorText({ message: 'from an object' })).toBe('from an object')
    expect(errorText(42)).toBe('42')
  })
})
