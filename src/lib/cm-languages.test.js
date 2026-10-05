import { describe, it, expect } from 'vitest'
import { EditorState } from '@codemirror/state'
import { languageExtension, isKnownLanguage } from './cm-languages.js'

describe('cm-languages', () => {
  it('builds an editor state for every language a view asks for', () => {
    for (const id of ['sql', 'json', 'jsonl', 'html', 'javascript', 'typescript', 'prisma', 'csv', 'tsv', 'markdown', 'stroke-csv', 'plaintext', '']) {
      const state = EditorState.create({ doc: 'a,b\n1,"x"', extensions: [languageExtension(id, 'postgres')] })
      expect(state.doc.lines, id).toBe(2)
    }
  })

  it('knows its ids and aliases, and nothing else', () => {
    expect(isKnownLanguage('ts')).toBe(true)
    expect(isKnownLanguage('stroke-tsv')).toBe(true)
    expect(isKnownLanguage('plaintext')).toBe(false)
  })
})
