import { describe, it, expect } from 'vitest'
import { checkObjectSql, objectProblemSummary } from './sql-object-check.js'

const sqlite = {
  activeSchema: 'main',
  tables: ['account', 'session', 'user', 'audit_log'],
  columnsByTable: {
    user: ['id', 'name', 'email', 'updated_at'],
    account: ['id', 'user_id'],
    audit_log: ['id', 'user_id', 'at'],
  },
}
const pg = {
  activeSchema: 'public',
  tables: ['users', 'orders', 'active_users'],
  columnsByTable: { users: [{ name: 'id' }, { name: 'email' }, { name: 'updated_at' }] },
}

const messages = (sql, hints = sqlite, dialect = 'sqlite') => checkObjectSql(sql, hints, dialect).diags.map((d) => d.message)

describe('checkObjectSql', () => {
  it('flags the template placeholder left in a trigger body', () => {
    const sql = `CREATE TRIGGER trg_after_update
AFTER UPDATE ON user
FOR EACH ROW
BEGIN
    UPDATE table_name SET updated_at = CURRENT_TIMESTAMP WHERE rowid = NEW.rowid;
END;`
    const { diags } = checkObjectSql(sql, sqlite, 'sqlite')
    expect(diags).toHaveLength(1)
    expect(diags[0].message).toMatch(/template's placeholder/)
    expect(sql.slice(diags[0].start, diags[0].end)).toBe('table_name')
  })

  it('accepts the same trigger once the table is real', () => {
    expect(messages(`CREATE TRIGGER t AFTER UPDATE ON user FOR EACH ROW
BEGIN
  UPDATE user SET updated_at = CURRENT_TIMESTAMP WHERE rowid = NEW.rowid;
END;`)).toEqual([])
  })

  it('flags the table a trigger is on when it does not exist', () => {
    expect(messages('CREATE TRIGGER t AFTER INSERT ON users FOR EACH ROW BEGIN SELECT 1; END;')).toEqual([
      'No table or view named "users" in main',
    ])
  })

  it('flags NEW and OLD columns the trigger table does not have', () => {
    expect(messages(`CREATE TRIGGER t BEFORE UPDATE ON user FOR EACH ROW
BEGIN
  INSERT INTO audit_log (user_id, at) VALUES (OLD.id, NEW.changed_at);
END;`)).toEqual(['user has no column "changed_at"'])
  })

  it('flags a SET column the updated table does not have', () => {
    expect(messages(`CREATE TRIGGER t AFTER INSERT ON account FOR EACH ROW
BEGIN
  UPDATE user SET modified = 1, name = NEW.id WHERE id = NEW.user_id;
END;`)).toEqual(['user has no column "modified"'])
  })

  it('flags a BEGIN with no END', () => {
    expect(messages('CREATE TRIGGER t AFTER INSERT ON user FOR EACH ROW BEGIN SELECT 1;')).toEqual([
      'This BEGIN has no END: the body runs to the end of the script',
    ])
  })

  it('does not count CASE ... END or END IF as the body closing', () => {
    expect(messages(`CREATE PROCEDURE p() BEGIN
  IF 1 THEN SELECT CASE WHEN 1 THEN 2 ELSE 3 END FROM user; END IF;
END`, sqlite, 'mysql')).toEqual([])
  })

  it('leaves trigger events, options and functions alone', () => {
    expect(messages(`CREATE TRIGGER t AFTER INSERT OR UPDATE OF name ON user FOR EACH ROW
BEGIN
  SELECT extract(year FROM NEW.updated_at), x.* FROM json_each(NEW.name) x;
END;`)).toEqual([])
  })

  it('reads SELECT ... INTO as a variable, not a table', () => {
    expect(messages(`CREATE FUNCTION f() RETURNS int LANGUAGE plpgsql AS $$
DECLARE n int;
BEGIN
  SELECT count(*) INTO n FROM users;
  RETURN n;
END;
$$;`, pg, 'postgres')).toEqual([])
  })

  it('checks the tables inside a Postgres function body', () => {
    expect(messages(`CREATE OR REPLACE FUNCTION f() RETURNS void LANGUAGE plpgsql AS $$
BEGIN
  DELETE FROM sessions WHERE true;
END;
$$;`, pg, 'postgres')).toEqual(['No table or view named "sessions" in public'])
  })

  it('checks NEW in a Postgres trigger function against the table its trigger is on', () => {
    expect(messages(`CREATE OR REPLACE FUNCTION set_updated_at() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  NEW.modified_at := now();
  RETURN NEW;
END;
$$;

CREATE TRIGGER trg BEFORE UPDATE ON users FOR EACH ROW EXECUTE FUNCTION set_updated_at();`, pg, 'postgres')).toEqual([
      'users has no column "modified_at"',
    ])
  })

  it('accepts views, CTEs, other schemas and tables the script creates first', () => {
    expect(messages(`CREATE TABLE staging (id int);
CREATE VIEW v AS
WITH recent AS (SELECT * FROM orders)
SELECT * FROM recent JOIN active_users a ON true JOIN staging s ON true JOIN audit.events e ON true;`, pg, 'postgres')).toEqual([])
  })

  it('skips a body in a language that is not SQL', () => {
    expect(messages(`CREATE FUNCTION f() RETURNS int LANGUAGE plpython3u AS $$
return len(plpy.execute("select * from nowhere"))
$$;`, pg, 'postgres')).toEqual([])
  })

  it('ignores plain queries and says nothing without a table list', () => {
    expect(messages('SELECT * FROM nowhere;')).toEqual([])
    expect(checkObjectSql('CREATE VIEW v AS SELECT * FROM nowhere;', { tables: [] }, 'sqlite').diags).toEqual([])
  })

  it('asks for the columns of a table it could check once loaded', () => {
    const { missing } = checkObjectSql('CREATE TRIGGER t AFTER UPDATE ON session FOR EACH ROW BEGIN SELECT NEW.x; END;', sqlite, 'sqlite')
    expect(missing).toEqual(['session'])
  })
})

describe('objectProblemSummary', () => {
  it('names the first error and counts the rest', () => {
    const { diags } = checkObjectSql('CREATE VIEW v AS SELECT * FROM a JOIN b ON true;', sqlite, 'sqlite')
    expect(objectProblemSummary(diags)).toBe('No table or view named "a" in main (and 1 more)')
  })
})
