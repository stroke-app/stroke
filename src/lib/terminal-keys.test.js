import { describe, it, expect } from 'vitest'
import { lineEditorFor, clearLineKeys, consoleKeystrokes, consoleInput, consoleClearCommand } from './terminal-keys.js'

const key = (k, mods = {}) => ({ key: k, ctrlKey: false, altKey: false, ...mods })

describe('lineEditorFor', () => {
  it('uses the console for the Windows builds without readline', () => {
    for (const name of ['psql', 'mysql', 'mariadb', 'sqlite3']) {
      expect(lineEditorFor({ name }, 'windows')).toBe('console')
      expect(lineEditorFor({ name }, 'linux')).toBe('readline')
      expect(lineEditorFor({ name }, 'macos')).toBe('readline')
    }
  })

  it('tells the ODBC sqlcmd from go-sqlcmd by its missing version', () => {
    expect(lineEditorFor({ name: 'sqlcmd', version: null }, 'windows')).toBe('console')
    expect(lineEditorFor({ name: 'sqlcmd', version: 'sqlcmd: 1.8.0' }, 'windows')).toBe('readline')
    expect(lineEditorFor(null, 'windows')).toBe('readline')
  })
})

describe('console keys', () => {
  it('empties the line with End and Ctrl+Home, never a lone Escape', () => {
    expect(clearLineKeys('console')).toBe('\x1b[F\x1b[1;5H')
    expect(clearLineKeys('readline')).toBe('\x05\x15')
  })

  it('sends the console its own word and line keys', () => {
    expect(consoleKeystrokes(key('Backspace', { ctrlKey: true }))).toBe('\x08')
    expect(consoleKeystrokes(key('Backspace', { altKey: true }))).toBe('\x08')
    expect(consoleKeystrokes(key('ArrowLeft', { altKey: true }))).toBe('\x1b[1;5D')
    expect(consoleKeystrokes(key('u', { ctrlKey: true }))).toBe('\x1b[1;5H')
    expect(consoleKeystrokes(key('e', { ctrlKey: true }))).toBe('\x1b[F')
  })

  it('leaves plain keys to xterm and drops Ctrl+Z, the console\'s end of input', () => {
    expect(consoleKeystrokes(key('Backspace'))).toBe(null)
    expect(consoleKeystrokes(key('Home'))).toBe(null)
    expect(consoleKeystrokes(key('z', { ctrlKey: true }))).toBe('')
  })

  it('drops the Ctrl keys the console would type into the line', () => {
    expect(consoleInput('\x17')).toBe('')
    expect(consoleInput('\x12')).toBe('')
    expect(consoleInput('\x0c')).toBe('')
    for (const kept of ['\r', '\x03', '\x08', '\x1b', '\x7f', 'a', '\x1b[A', '\x1b[1;1R']) expect(consoleInput(kept)).toBe(kept)
  })

  it('clears the screen through the client\'s shell escape where it has one', () => {
    expect(consoleClearCommand('psql')).toBe('\\! cls')
    expect(consoleClearCommand('sqlite3')).toBe('.shell cls')
    expect(consoleClearCommand('mysql')).toBe(null)
  })
})
