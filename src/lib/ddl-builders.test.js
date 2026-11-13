import { describe, expect, it } from 'vitest'
import {
  buildTriggerSql, validateTrigger, buildSequenceSql, validateSequence,
  buildTableSql, validateTable, hasPlaceholder, toSnippet, placeholderText, SEQUENCE_TYPES,
} from './ddl-builders.js'

const trig = (/** @type {any} */ o = {}) => ({
  schema: 'public', name: 'trg', table: 'users', timing: 'AFTER', events: ['INSERT'],
  rowLevel: true, functionName: 'fn', ...o,
})

describe('buildTriggerSql', () => {
  it('quotes identifiers and adds UPDATE OF and WHEN', () => {
    const sql = buildTriggerSql(trig({ events: ['UPDATE', 'INSERT'], updateColumns: ['email'], when: 'NEW.a <> OLD.a', functionArgs: "'x'" }))
    expect(sql).toBe(`CREATE TRIGGER "trg"
  AFTER INSERT OR UPDATE OF "email"
  ON "public"."users"
  FOR EACH ROW
  WHEN (NEW.a <> OLD.a)
  EXECUTE FUNCTION "public"."fn"('x');`)
  })

  it('shows placeholders for missing parts', () => {
    const sql = buildTriggerSql(trig({ name: '', table: '', functionName: '' }))
    expect(sql).toContain('CREATE TRIGGER ‹trigger name›')
    expect(sql).toContain('ON "public".‹table›')
    expect(hasPlaceholder(sql)).toBe(true)
    expect(Object.keys(validateTrigger(trig({ name: '', table: '', functionName: '' })))).toEqual(['name', 'table', 'functionName'])
  })

  it('rejects row-level TRUNCATE', () => {
    expect(validateTrigger(trig({ events: ['TRUNCATE'] })).events).toBeTruthy()
    expect(validateTrigger(trig({ events: ['TRUNCATE'], rowLevel: false }))).toEqual({})
  })
})

describe('buildSequenceSql', () => {
  const seq = (/** @type {any} */ o = {}) => ({
    schema: 'public', name: 'ids', dataType: 'integer', increment: '1', minValue: '1',
    maxValue: SEQUENCE_TYPES.integer.hi, startValue: '1', cache: '10', cycle: false, ...o,
  })

  it('builds with cache and owned by', () => {
    expect(buildSequenceSql(seq({ ownedTable: 'users', ownedColumn: 'id', cycle: true }))).toBe(`CREATE SEQUENCE "public"."ids"
  AS integer
  INCREMENT BY 1
  MINVALUE 1
  MAXVALUE 2147483647
  START WITH 1
  CACHE 10
  CYCLE
  OWNED BY "public"."users"."id";`)
  })

  it('validates ranges against the type', () => {
    expect(validateSequence(seq())).toEqual({})
    expect(validateSequence(seq({ dataType: 'smallint' })).maxValue).toBe('Out of range for smallint')
    expect(validateSequence(seq({ increment: '0' })).increment).toBeTruthy()
    expect(validateSequence(seq({ startValue: '0' })).startValue).toBeTruthy()
  })
})

describe('buildTableSql', () => {
  /** @param {any} o */
  const col = (o) => ({ id: o.name, type: 'text', pk: false, notNull: false, defaultVal: '', ...o })

  it('postgres: params, unique, fk, comments and indexes', () => {
    const stmts = buildTableSql({
      dbType: 'postgres', schema: 'public', name: 'posts',
      columns: [
        col({ name: 'id', type: 'serial', pk: true, notNull: true }),
        col({ name: 'title', type: 'varchar', length: '120', notNull: true, unique: true, comment: "It's" }),
        col({ name: 'price', type: 'numeric', length: '10', scale: '2' }),
        col({ name: 'user_id', type: 'integer' }),
      ],
      foreignKeys: [{ id: 'f', column: 'user_id', refTable: 'users', refColumn: 'id', onDelete: 'CASCADE', onUpdate: 'NO ACTION' }],
      indexes: [{ id: 'i', columns: ['user_id'], unique: false }],
      comment: 'Posts',
    })
    expect(stmts[0]).toBe(`CREATE TABLE "public"."posts" (
  "id" serial PRIMARY KEY,
  "title" varchar(120) NOT NULL UNIQUE,
  "price" numeric(10,2),
  "user_id" integer,
  FOREIGN KEY ("user_id") REFERENCES "public"."users" ("id") ON DELETE CASCADE
);`)
    expect(stmts.slice(1)).toEqual([
      `COMMENT ON TABLE "public"."posts" IS 'Posts';`,
      `COMMENT ON COLUMN "public"."posts"."title" IS 'It''s';`,
      `CREATE INDEX "posts_user_id_idx" ON "public"."posts" ("user_id");`,
    ])
  })

  it('mysql: backticks, AUTO_INCREMENT and inline comments', () => {
    const [sql] = buildTableSql({
      dbType: 'mysql', schema: 'app', name: 't', comment: 'c',
      columns: [col({ name: 'id', type: 'INT', pk: true, notNull: true, defaultVal: 'AUTO_INCREMENT' }), col({ name: 'n', type: 'VARCHAR', length: '255', comment: 'x' })],
    })
    expect(sql).toBe("CREATE TABLE `app`.`t` (\n  `id` INT AUTO_INCREMENT PRIMARY KEY,\n  `n` VARCHAR(255) COMMENT 'x'\n) COMMENT='c';")
  })

  it('sqlite: no schema, composite key as a constraint', () => {
    const [sql] = buildTableSql({
      dbType: 'd1', schema: 'main', name: 't',
      columns: [col({ name: 'a', type: 'INTEGER', pk: true }), col({ name: 'b', type: 'TEXT', pk: true })],
    })
    expect(sql).toBe('CREATE TABLE "t" (\n  "a" INTEGER,\n  "b" TEXT,\n  PRIMARY KEY ("a", "b")\n);')
  })

  it('flags duplicate and unnamed columns', () => {
    const e = validateTable({ dbType: 'postgres', schema: 'public', name: 't', columns: [col({ name: 'a', id: '1' }), col({ name: 'a', id: '2' }), col({ name: '', id: '3' })] })
    expect(Object.keys(e)).toEqual(['2', '3'])
  })
})

describe('placeholders', () => {
  it('turns them into shared snippet fields', () => {
    const sql = 'CREATE ‹x› ON ‹y› {a} ‹x›'
    expect(toSnippet(sql)).toBe('CREATE ${1:x} ON ${2:y} \\{a\\} ${1:x}')
    expect(placeholderText(sql)).toBe('CREATE x ON y {a} x')
  })
})
