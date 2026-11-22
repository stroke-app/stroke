import { describe, it, expect } from 'vitest'
import en from './en.js'
import es from './es.js'
import fr from './fr.js'
import de from './de.js'
import pt from './pt.js'
import ja from './ja.js'

const vars = (/** @type {string} */ s) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort()

describe('locales', () => {
  for (const [name, dict] of Object.entries({ es, fr, de, pt, ja })) {
    it(`${name} has every English key, and no others`, () => {
      expect(Object.keys(dict).filter((k) => !(k in en))).toEqual([])
      expect(Object.keys(en).filter((k) => !(k in dict))).toEqual([])
    })
    it(`${name} keeps every {placeholder}`, () => {
      const broken = Object.keys(en).filter((k) => k in dict && JSON.stringify(vars(en[k])) !== JSON.stringify(vars(dict[k])))
      expect(broken).toEqual([])
    })
  }
})
