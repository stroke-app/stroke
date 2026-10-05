import { describe, expect, it } from 'vitest'
import { EditorState } from '@codemirror/state'
import { snippet } from '@codemirror/autocomplete'
import {
  FAMILY_KINDS, OBJECT_KINDS, OBJECT_KIND_META, definitionTitle, dropObjectSql, objectFamily, objectKey, objectLabel, objectTemplate, snippetText,
} from './object-templates.js'
import { splitSqlStatements } from './sql-statements.js'
import { sqlRunEffects } from './sql-write.js'

/** One connection type per family. */
const TYPES = { postgres: 'postgres', mysql: 'mysql', mssql: 'mssql', sqlite: 'sqlite', duckdb: 'duckdb', clickhouse: 'clickhouse' }
const SCHEMA = { postgres: 'public', mysql: 'shop', mssql: 'dbo', sqlite: 'main', duckdb: 'main', clickhouse: 'default' }

/** What CodeMirror actually writes into an empty editor for a snippet. */
function expand(tpl) {
  let state = EditorState.create({ doc: '' })
  snippet(tpl)({ state, dispatch: (tr) => { state = tr.state } }, null, 0, 0)
  return state.doc.toString()
}

describe('objectTemplate', () => {
  for (const [family, type] of Object.entries(TYPES)) {
    for (const kind of OBJECT_KINDS) {
      const has = FAMILY_KINDS[family].includes(kind)
      it(`${family} ${kind}: ${has ? 'a runnable template' : 'none'}`, () => {
        const t = objectTemplate(type, kind, SCHEMA[family])
        if (!has) {
          expect(t).toBeNull()
          return
        }
        expect(t).not.toBeNull()
        const { title, snippet: tpl, text } = /** @type {NonNullable<typeof t>} */ (t)
        expect(title).toBe(`New ${OBJECT_KIND_META[kind].one}`)
        // The plain text is exactly what the editor's snippet expands to.
        expect(expand(tpl)).toBe(text)
        expect(text).not.toMatch(/[#$]\{/)
        // A Postgres trigger is its function plus the trigger; the rest are one
        // statement each, bodies included.
        const statements = splitSqlStatements(text)
        expect(statements).toHaveLength(family === 'postgres' && kind === 'trigger' ? 2 : 1)
        // Running it refreshes the group it belongs to.
        expect(sqlRunEffects(text).objects).toContain(kind === 'type' ? 'type' : kind)
        expect(sqlRunEffects(text).catalog).toBe(true)
      })
    }
  }

  it('puts the schema in front where it is not the default one, quoted when it must be', () => {
    expect(objectTemplate('postgres', 'view', 'public')?.text).toMatch(/^CREATE OR REPLACE VIEW new_view AS/)
    expect(objectTemplate('postgres', 'view', 'analytics')?.text).toMatch(/^CREATE OR REPLACE VIEW analytics\.new_view AS/)
    expect(objectTemplate('postgres', 'function', 'Sales Data')?.text).toMatch(/^CREATE OR REPLACE FUNCTION "Sales Data"\.new_function\(/)
    expect(objectTemplate('mysql', 'procedure', 'my-db')?.text).toMatch(/^CREATE PROCEDURE `my-db`\.new_procedure\(/)
    expect(objectTemplate('mssql', 'view', 'dbo')?.text).toMatch(/^CREATE OR ALTER VIEW dbo\.new_view AS/)
    expect(objectTemplate('sqlite', 'view', 'main')?.text).toMatch(/^CREATE VIEW new_view AS/)
    expect(objectTemplate('duckdb', 'function', 'main')?.text).toMatch(/^CREATE OR REPLACE MACRO new_macro\(/)
    // A brace in a schema name is not snippet syntax.
    expect(objectTemplate('postgres', 'view', 'a{b}')?.text).toMatch(/^CREATE OR REPLACE VIEW "a\{b\}"\.new_view AS/)
  })

  it('links the names a template repeats', () => {
    const pg = /** @type {{ snippet: string }} */ (objectTemplate('postgres', 'trigger', 'public'))
    expect(pg.snippet.match(/\$\{1:set_updated_at\}/g)).toHaveLength(2)
    const lite = /** @type {{ snippet: string }} */ (objectTemplate('sqlite', 'trigger'))
    expect(lite.snippet.match(/\$\{4:table_name\}/g)).toHaveLength(2)
  })

  it('has nothing for engines without objects', () => {
    expect(objectTemplate('redis', 'view')).toBeNull()
    expect(objectTemplate('posthog', 'function')).toBeNull()
    expect(objectTemplate(undefined, 'view')).toBeNull()
  })
})

describe('objectFamily', () => {
  it('maps every connection type to the dialect it writes', () => {
    expect(objectFamily('cockroachdb')).toBe('postgres')
    expect(objectFamily('mariadb')).toBe('mysql')
    expect(objectFamily('d1')).toBe('sqlite')
    expect(objectFamily('libsql')).toBe('sqlite')
    expect(objectFamily('redis')).toBeNull()
  })
})

describe('snippetText', () => {
  it('keeps placeholders and drops the syntax', () => {
    expect(snippetText('SELECT ${1:a}, ${b} FROM ${2:t}${0}')).toBe('SELECT a, b FROM t')
    expect(snippetText('x \\{y\\}')).toBe('x {y}')
  })
})

describe('object names', () => {
  it('shows overloads apart and names a trigger with its table', () => {
    expect(objectLabel({ kind: 'function', name: 'f', args: 'a integer' })).toBe('f(a integer)')
    expect(objectLabel({ kind: 'procedure', name: 'p', args: '' })).toBe('p()')
    expect(objectLabel({ kind: 'trigger', name: 't', table: 'x' })).toBe('t')
    expect(definitionTitle({ kind: 'trigger', name: 't', table: 'x' })).toBe('t on x')
    expect(objectKey({ kind: 'function', name: 'f', args: 'int' })).not.toBe(objectKey({ kind: 'function', name: 'f', args: 'text' }))
  })
})

describe('dropObjectSql', () => {
  it('spells the DROP the backend runs, per engine', () => {
    expect(dropObjectSql('postgres', 'public', { kind: 'function', name: 'f', args: 'a integer, b text' })).toBe('DROP FUNCTION "public"."f"(a integer, b text)')
    expect(dropObjectSql('postgres', 'public', { kind: 'function', name: 'agg', args: 'integer', subtype: 'aggregate' }, true)).toBe('DROP AGGREGATE "public"."agg"(integer) CASCADE')
    expect(dropObjectSql('cockroachdb', 's', { kind: 'trigger', name: 't', table: 'x' })).toBe('DROP TRIGGER "t" ON "s"."x"')
    expect(dropObjectSql('postgres', 'public', { kind: 'type', name: 'd', subtype: 'domain' })).toBe('DROP DOMAIN "public"."d"')
    expect(dropObjectSql('postgres', 'public', { kind: 'matview', name: 'mv' })).toBe('DROP MATERIALIZED VIEW "public"."mv"')
    expect(dropObjectSql('mariadb', 'shop', { kind: 'procedure', name: 'p`x' })).toBe('DROP PROCEDURE `shop`.`p``x`')
    expect(dropObjectSql('mysql', 'shop', { kind: 'event', name: 'e' })).toBe('DROP EVENT `shop`.`e`')
    expect(dropObjectSql('mssql', 'dbo', { kind: 'trigger', name: 'a]b' })).toBe('DROP TRIGGER [dbo].[a]]b]')
    expect(dropObjectSql('d1', 'main', { kind: 'trigger', name: 't' })).toBe('DROP TRIGGER "t"')
    expect(dropObjectSql('duckdb', 'main', { kind: 'function', name: 'm', subtype: 'table_macro' })).toBe('DROP MACRO TABLE "main"."m"')
    expect(dropObjectSql('clickhouse', 'default', { kind: 'function', name: 'f' })).toBe('DROP FUNCTION `f`')
    // CASCADE is Postgres only, and kinds an engine lacks have no DROP here.
    expect(dropObjectSql('mysql', 'shop', { kind: 'view', name: 'v' }, true)).toBe('DROP VIEW `shop`.`v`')
    expect(dropObjectSql('sqlite', 'main', { kind: 'function', name: 'f' })).toBeNull()
    expect(dropObjectSql('redis', '', { kind: 'view', name: 'v' })).toBeNull()
  })
})
