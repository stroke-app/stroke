import { describe, it, expect } from 'vitest'
import { isWriteSql, sqlRunEffects, stripSqlComments } from './sql-write.js'

describe('isWriteSql', () => {
  it('allows plain reads', () => {
    expect(isWriteSql('SELECT * FROM users')).toBe(false)
    expect(isWriteSql('  select 1  ')).toBe(false)
    expect(isWriteSql('EXPLAIN ANALYZE SELECT 1')).toBe(false)
    expect(isWriteSql('SHOW TABLES')).toBe(false)
    expect(isWriteSql('WITH recent AS (SELECT 1) SELECT * FROM recent')).toBe(false)
    expect(isWriteSql('')).toBe(false)
    expect(isWriteSql(null)).toBe(false)
  })

  it('catches the obvious writes', () => {
    expect(isWriteSql('INSERT INTO t VALUES (1)')).toBe(true)
    expect(isWriteSql('update t set a = 1')).toBe(true)
    expect(isWriteSql('DELETE FROM t')).toBe(true)
    expect(isWriteSql('TRUNCATE TABLE t')).toBe(true)
    expect(isWriteSql('DROP INDEX "public"."idx"')).toBe(true)
    expect(isWriteSql('ALTER TABLE t ADD COLUMN c int')).toBe(true)
    expect(isWriteSql('CREATE INDEX idx ON t (a)')).toBe(true)
    expect(isWriteSql('GRANT SELECT ON t TO bob')).toBe(true)
  })

  it('catches storage rewrites that no user typed as data', () => {
    expect(isWriteSql('VACUUM FULL')).toBe(true)
    expect(isWriteSql('REINDEX TABLE t')).toBe(true)
    expect(isWriteSql('REFRESH MATERIALIZED VIEW mv')).toBe(true)
  })

  it('looks past a CTE to the real verb', () => {
    expect(isWriteSql('WITH d AS (SELECT id FROM t) DELETE FROM u USING d WHERE u.id = d.id')).toBe(true)
    expect(isWriteSql('WITH x AS (SELECT 1) INSERT INTO t SELECT * FROM x')).toBe(true)
    expect(isWriteSql('with s as (select 1) update t set a = 1')).toBe(true)
  })

  it('is not fooled by a comment in front of the verb', () => {
    expect(isWriteSql('-- just looking\nDROP TABLE t')).toBe(true)
    expect(isWriteSql('/* SELECT */ DELETE FROM t')).toBe(true)
  })

  it('flags any write inside a multi-statement script', () => {
    expect(isWriteSql('SELECT 1; SELECT 2')).toBe(false)
    expect(isWriteSql('SELECT 1; DELETE FROM t; SELECT 2')).toBe(true)
  })

  it('does not treat a semicolon or keyword inside a literal as syntax', () => {
    expect(isWriteSql("SELECT * FROM t WHERE note = 'a; delete from u'")).toBe(false)
    expect(isWriteSql("SELECT * FROM t WHERE note LIKE '%into%'")).toBe(false)
    expect(isWriteSql("SELECT * FROM t WHERE c = '-- drop table t'")).toBe(false)
  })

  it('distinguishes SELECT INTO from a read', () => {
    expect(isWriteSql('SELECT a, b INTO backup FROM t')).toBe(true)
    expect(isWriteSql("SELECT * INTO OUTFILE '/tmp/x' FROM t")).toBe(true)
    expect(isWriteSql('SELECT * FROM t WHERE label = 1')).toBe(false)
  })

  it('splits COPY by direction', () => {
    expect(isWriteSql('COPY t FROM \'/tmp/x.csv\'')).toBe(true)
    expect(isWriteSql('COPY t TO STDOUT')).toBe(false)
  })

  it('treats identity changes as writes but leaves plain SET alone', () => {
    expect(isWriteSql('SET ROLE admin')).toBe(true)
    expect(isWriteSql('SET SESSION AUTHORIZATION bob')).toBe(true)
    expect(isWriteSql("SET statement_timeout = '5s'")).toBe(false)
  })

  it('lets introspection PRAGMAs through but not assignments', () => {
    expect(isWriteSql("PRAGMA table_info('users')")).toBe(false)
    expect(isWriteSql('PRAGMA foreign_key_list(users)')).toBe(false)
    expect(isWriteSql('PRAGMA data_version')).toBe(false)
    expect(isWriteSql('PRAGMA journal_mode')).toBe(false)
    expect(isWriteSql('PRAGMA journal_mode = WAL')).toBe(true)
    expect(isWriteSql('PRAGMA foreign_keys=ON')).toBe(true)
    expect(isWriteSql('PRAGMA optimize')).toBe(true)
    expect(isWriteSql('PRAGMA incremental_vacuum')).toBe(true)
  })

  it('covers the Redis commands that share the execute path', () => {
    expect(isWriteSql('DEL "session:1"')).toBe(true)
    expect(isWriteSql('HDEL h field')).toBe(true)
    expect(isWriteSql('FLUSHDB')).toBe(true)
    expect(isWriteSql('EXPIRE k 60')).toBe(true)
    expect(isWriteSql('SET mykey "value"')).toBe(true)
    expect(isWriteSql('GET mykey')).toBe(false)
    expect(isWriteSql('SCAN 0 MATCH * COUNT 100')).toBe(false)
    expect(isWriteSql('TTL mykey')).toBe(false)
    expect(isWriteSql('HGETALL h')).toBe(false)
  })
})

describe('sqlRunEffects', () => {
  const t = (name, schema = null) => ({ schema, name })
  const NONE = { catalog: false, schemas: false, data: false, tables: [], objects: [] }

  it('reports nothing for reads', () => {
    expect(sqlRunEffects('SELECT * FROM users')).toEqual(NONE)
    expect(sqlRunEffects('SHOW TABLES; EXPLAIN SELECT 1')).toEqual(NONE)
    expect(sqlRunEffects("SELECT 'x; DROP TABLE t' -- DELETE FROM u")).toEqual(NONE)
    expect(sqlRunEffects('SELECT a INTO @v FROM t')).toEqual(NONE)
    expect(sqlRunEffects('COPY t TO STDOUT')).toEqual(NONE)
    expect(sqlRunEffects('')).toEqual(NONE)
  })

  it('names the table a row write lands in, quoted or not', () => {
    expect(sqlRunEffects('INSERT INTO "Public"."Users" (a) VALUES (1)')).toEqual({ ...NONE, data: true, tables: [t('users', 'public')] })
    expect(sqlRunEffects('update orders set a = 1 where id = 2').tables).toEqual([t('orders')])
    expect(sqlRunEffects('DELETE FROM ONLY logs WHERE x').tables).toEqual([t('logs')])
    expect(sqlRunEffects('TRUNCATE TABLE a, b.c').tables).toEqual([t('a'), t('c', 'b')])
    expect(sqlRunEffects("COPY items FROM '/tmp/x.csv'").tables).toEqual([t('items')])
  })

  it('reads the MySQL, SQLite and T-SQL spellings', () => {
    expect(sqlRunEffects('INSERT IGNORE INTO `shop`.`orders` VALUES (1)').tables).toEqual([t('orders', 'shop')])
    expect(sqlRunEffects('INSERT OR REPLACE INTO kv VALUES (1, 2)').tables).toEqual([t('kv')])
    expect(sqlRunEffects('REPLACE INTO kv VALUES (1, 2)').tables).toEqual([t('kv')])
    expect(sqlRunEffects('UPDATE LOW_PRIORITY items SET a = 1').tables).toEqual([t('items')])
    expect(sqlRunEffects('INSERT INTO [app].[dbo].[Users] VALUES (1)').tables).toEqual([t('users', 'dbo')])
    expect(sqlRunEffects("LOAD DATA INFILE 'x' INTO TABLE stock").tables).toEqual([t('stock')])
  })

  it('reports any table when the target cannot be read off the statement', () => {
    expect(sqlRunEffects('DELETE a FROM a JOIN b ON a.id = b.id').tables).toBeNull()
    expect(sqlRunEffects('UPDATE a JOIN b ON a.id = b.id SET a.x = b.x').tables).toBeNull()
    expect(sqlRunEffects('UPDATE a, b SET a.x = 1').tables).toBeNull()
    expect(sqlRunEffects('CALL rebuild()')).toEqual({ catalog: true, schemas: false, data: true, tables: null, objects: ['any'] })
    expect(sqlRunEffects('INSERT INTO a VALUES (1); CALL p()').tables).toBeNull()
  })

  it('treats DDL as a catalog change and names the tables it reshapes', () => {
    expect(sqlRunEffects('CREATE TABLE IF NOT EXISTS a (id int)')).toEqual({ ...NONE, catalog: true, tables: [t('a')] })
    expect(sqlRunEffects('DROP TABLE IF EXISTS a, b CASCADE').tables).toEqual([t('a'), t('b')])
    expect(sqlRunEffects('ALTER TABLE ONLY public.x ADD COLUMN c int').tables).toEqual([t('x', 'public')])
    expect(sqlRunEffects('CREATE OR REPLACE VIEW v AS SELECT 1').tables).toEqual([t('v')])
    expect(sqlRunEffects('RENAME TABLE a TO b, c TO d').tables).toEqual([t('a'), t('b'), t('c'), t('d')])
    expect(sqlRunEffects('SELECT * INTO backup FROM t')).toEqual({ ...NONE, catalog: true, tables: [t('backup')] })
  })

  it('changes the catalog without touching any table for other objects', () => {
    expect(sqlRunEffects('CREATE UNIQUE INDEX i ON t (a)')).toEqual({ ...NONE, catalog: true })
    expect(sqlRunEffects('DROP FUNCTION f()')).toEqual({ ...NONE, catalog: true, objects: ['function'] })
  })

  it('names the Objects groups a routine, trigger or type statement makes stale', () => {
    const objects = (sql) => sqlRunEffects(sql).objects
    expect(objects('CREATE OR REPLACE FUNCTION f() RETURNS int AS $$ SELECT 1 $$ LANGUAGE sql')).toEqual(['function'])
    expect(objects('CREATE DEFINER=`root`@`%` PROCEDURE p() SELECT 1')).toEqual(['procedure'])
    expect(objects('CREATE OR ALTER PROC dbo.p AS SELECT 1')).toEqual(['procedure'])
    expect(objects('CREATE CONSTRAINT TRIGGER t AFTER INSERT ON a FOR EACH ROW EXECUTE FUNCTION f()')).toEqual(['trigger'])
    expect(objects('CREATE ALGORITHM=MERGE SQL SECURITY INVOKER VIEW v AS SELECT 1')).toEqual(['view'])
    expect(objects('DROP MATERIALIZED VIEW IF EXISTS mv')).toEqual(['matview'])
    expect(objects('CREATE TEMP MACRO m(a) AS a + 1')).toEqual(['function'])
    expect(objects('CREATE AGGREGATE agg(int) (SFUNC = f, STYPE = int)')).toEqual(['function'])
    expect(objects('ALTER SEQUENCE s RESTART')).toEqual(['sequence'])
    expect(objects("CREATE TYPE mood AS ENUM ('a'); CREATE DOMAIN d AS int")).toEqual(['type'])
    expect(objects('ALTER EVENT e DISABLE')).toEqual(['event'])
    expect(objects("COMMENT ON FUNCTION f() IS 'x'")).toEqual(['comment', 'function'])
    expect(objects("COMMENT ON TABLE t IS 'x'")).toEqual(['comment'])
    expect(objects('CREATE TABLE t (id int); CREATE INDEX i ON t (id)')).toEqual([])
  })

  it('flags schema and database changes', () => {
    expect(sqlRunEffects('CREATE SCHEMA s').schemas).toBe(true)
    expect(sqlRunEffects('DROP DATABASE d').schemas).toBe(true)
    expect(sqlRunEffects("ATTACH DATABASE 'x.db' AS x").schemas).toBe(true)
    expect(sqlRunEffects('CREATE TABLE s.t (id int)').schemas).toBe(false)
  })

  it('adds up every statement of a script', () => {
    expect(sqlRunEffects('INSERT INTO a VALUES (1); SELECT 1; CREATE INDEX i ON b (x)')).toEqual({
      catalog: true, schemas: false, data: true, tables: [t('a')], objects: [],
    })
    expect(sqlRunEffects('WITH d AS (DELETE FROM a RETURNING *) INSERT INTO b SELECT * FROM d').tables).toEqual([t('a'), t('b')])
  })
})

describe('sqlRunEffects inside routine bodies', () => {
  it('reads a body as part of its CREATE, not as writes of its own', () => {
    // BEGIN ... END (MySQL, SQLite, T-SQL) and a dollar-quoted body (Postgres):
    // the DELETE inside is the routine's, so no table is refetched.
    for (const sql of [
      'CREATE PROCEDURE p() BEGIN DELETE a FROM a JOIN b ON a.id = b.id; END;',
      'CREATE FUNCTION f() RETURNS void AS $$ DELETE FROM a; $$ LANGUAGE sql;',
    ]) {
      const fx = sqlRunEffects(sql)
      expect(fx.catalog, sql).toBe(true)
      expect(fx.data, sql).toBe(false)
      expect(fx.tables, sql).toEqual([])
    }
    // The same DELETE on its own is a write to `a`.
    expect(sqlRunEffects('DELETE FROM a;')).toMatchObject({ data: true, tables: [{ name: 'a' }] })
  })
})

describe('stripSqlComments', () => {
  it('keeps string contents intact', () => {
    expect(stripSqlComments("SELECT '--x' AS a -- tail")).toBe("SELECT '--x' AS a ")
  })

  it('handles doubled quotes inside a literal', () => {
    expect(stripSqlComments("SELECT 'it''s fine' -- tail")).toBe("SELECT 'it''s fine' ")
  })
})
