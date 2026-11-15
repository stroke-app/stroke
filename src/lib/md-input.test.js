import { describe, expect, test } from 'vitest'
import { markdownEnter, markdownIndent, inOpenFence } from './md-input.js'

/** Run Enter with the caret at `|`. */
const enter = (s) => {
  const at = s.indexOf('|')
  const text = s.replace('|', '')
  const r = markdownEnter(text, at, at)
  return r ? r.text.slice(0, r.start) + '|' + r.text.slice(r.start) : null
}

describe('markdownEnter', () => {
  test('continues bullets, numbers and checkboxes', () => {
    expect(enter('- one|')).toBe('- one\n- |')
    expect(enter('  * nested|')).toBe('  * nested\n  * |')
    expect(enter('1. first|')).toBe('1. first\n2. |')
    expect(enter('9) nine|')).toBe('9) nine\n10) |')
    expect(enter('- [x] done|')).toBe('- [x] done\n- [ ] |')
  })
  test('an empty item ends the list', () => {
    expect(enter('- one\n- |')).toBe('- one\n|')
    expect(enter('- one\n2. |\nafter')).toBe('- one\n|\nafter')
  })
  test('plain text, mid-line, and before the marker send', () => {
    expect(enter('hello|')).toBeNull()
    expect(enter('- one| two')).toBeNull()
    expect(enter('|- one')).toBeNull()
  })
  test('inside an open fence it is a newline', () => {
    expect(enter('```sql\nselect 1|')).toBe('```sql\nselect 1\n|')
    expect(enter('```\nx\n```\ndone|')).toBeNull()
    expect(inOpenFence('```\na', 5)).toBe(true)
  })
})

describe('markdownIndent', () => {
  test('indents and outdents list lines only', () => {
    expect(markdownIndent('- a\n- b', 4, 7, false)?.text).toBe('- a\n  - b')
    expect(markdownIndent('  - b', 3, 3, true)?.text).toBe('- b')
    expect(markdownIndent('- a\ntext\n- c', 0, 11, false)?.text).toBe('  - a\ntext\n  - c')
  })
  test('null when nothing selected is a list item', () => {
    expect(markdownIndent('hello', 0, 0, false)).toBeNull()
  })
})
