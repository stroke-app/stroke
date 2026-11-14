import { describe, expect, it } from 'vitest'
import {
  countValueTuples,
  describeStatement,
  extractPlan,
  formatStepDuration,
  groupRetries,
  planProgress,
  retryLabel,
  sqlStepLabel,
  summarizeSteps,
} from './ai-steps.js'

/** @param {string} id @param {string} sql @param {'running'|'done'|'error'} status @param {object} [extra] */
const q = (id, sql, status = 'done', extra = {}) => ({ id, tool: 'execute_sql', op: 'query', detail: sql, status, ...extra })

describe('extractPlan', () => {
  it('takes a numbered plan out of the reply', () => {
    const r = extractPlan('Setting up the shop.\n<plan>\n1. Create table customers\n2. Create table orders\n3) Seed both\n</plan>\nStarting now.')
    expect(r.plan).toEqual(['Create table customers', 'Create table orders', 'Seed both'])
    expect(r.text).toBe('Setting up the shop.\n\nStarting now.')
  })

  it('hides a plan that is still streaming', () => {
    const r = extractPlan('<plan>\n1. Create table a\n2. Cre')
    expect(r.plan).toEqual(['Create table a', 'Cre'])
    expect(r.text).toBe('')
  })

  it('leaves a reply without a plan alone', () => {
    expect(extractPlan('Just an answer.')).toEqual({ plan: null, text: 'Just an answer.' })
    expect(extractPlan('<plan></plan>ok').plan).toBeNull()
  })

  it('accepts bullets and checkboxes', () => {
    expect(extractPlan('<plan>- one\n* two\n[ ] three</plan>').plan).toEqual(['one', 'two', 'three'])
  })
})

describe('describeStatement / sqlStepLabel', () => {
  it.each([
    ['CREATE TABLE `customers` (id INT PRIMARY KEY)', 'Create table customers'],
    ['create table if not exists shop.orders (id int)', 'Create table orders'],
    ['CREATE DATABASE ecommerce', 'Create database ecommerce'],
    ['CREATE SCHEMA IF NOT EXISTS "sales"', 'Create database sales'],
    ['CREATE UNIQUE INDEX idx_email ON customers (email)', 'Create index idx_email on customers'],
    ['CREATE OR REPLACE VIEW v_orders AS SELECT 1', 'Create view v_orders'],
    ['DROP TABLE IF EXISTS `order_items`', 'Drop table order_items'],
    ['ALTER TABLE orders ADD COLUMN note TEXT', 'Alter table orders'],
    ["INSERT INTO `customers` (`name`) VALUES ('Alice'), ('Bob (jr)'), ('O''Neil')", 'Insert 3 rows into customers'],
    ["INSERT INTO t VALUES (1, 'a')", 'Insert 1 row into t'],
    ['INSERT INTO archive SELECT * FROM orders', 'Insert into archive'],
    ["UPDATE orders SET status = 'x' WHERE id = 1", 'Update orders'],
    ['DELETE FROM orders WHERE id = 1', 'Delete from orders'],
    ['SELECT COUNT(*) FROM `shop`.`orders`', 'Query orders'],
    ['SELECT 1', 'Run query'],
    ['SHOW TABLES', 'Inspect schema'],
    ['USE ecommerce', 'Switch to ecommerce'],
    ['-- seed\nINSERT INTO t VALUES (1)', 'Insert 1 row into t'],
  ])('%s', (sql, label) => {
    expect(sqlStepLabel(sql)).toBe(label)
  })

  it('labels a script by its first statements', () => {
    expect(sqlStepLabel('CREATE DATABASE shop; USE shop; CREATE TABLE a (id int); CREATE TABLE b (id int)'))
      .toBe('Create database shop, Switch to shop +2 more')
    expect(sqlStepLabel('CREATE TABLE a (id int); CREATE TABLE b (id int);')).toBe('Create table a, Create table b')
  })

  it('stops counting tuples at ON DUPLICATE KEY', () => {
    expect(countValueTuples('INSERT INTO t (a) VALUES (1), (2) ON DUPLICATE KEY UPDATE a = VALUES(a)')).toBe(2)
    expect(countValueTuples('UPDATE t SET a = 1')).toBeNull()
    expect(describeStatement('INSERT INTO t VALUES (1),(2)').rows).toBe(2)
  })
})

describe('groupRetries', () => {
  it('folds a retry of the same statement under the failure', () => {
    const steps = [
      q('1', 'CREATE TABLE customers (id int)', 'error', { error: 'boom' }),
      { id: '2', tool: 'describe_table', op: 'describe', detail: 'customers', status: 'done' },
      q('3', 'CREATE TABLE `customers` (id INT PRIMARY KEY)', 'done'),
      q('4', 'CREATE TABLE orders (id int)', 'done'),
    ]
    const groups = groupRetries(/** @type {any} */ (steps))
    expect(groups.map((g) => g.step.id)).toEqual(['1', '2', '4'])
    expect(groups[0].retries.map((r) => r.id)).toEqual(['3'])
    expect(groups[0].status).toBe('done')
    expect(retryLabel(groups[0])).toBe('Retried · succeeded')
    expect(retryLabel(groups[2])).toBe('')
  })

  it('keeps a group failing while every retry fails', () => {
    const steps = [q('1', "INSERT INTO t VALUES (\"a\")", 'error'), q('2', "INSERT INTO t VALUES ('a', 1)", 'error'), q('3', "INSERT INTO t VALUES ('a')", 'error')]
    const [g] = groupRetries(/** @type {any} */ (steps))
    expect(g.retries).toHaveLength(2)
    expect(g.status).toBe('error')
    expect(retryLabel(g)).toBe('Retried twice · still failing')
  })

  it('does not treat a second success on the same table as a retry', () => {
    const steps = [q('1', 'INSERT INTO t VALUES (1)'), q('2', 'INSERT INTO t VALUES (2)')]
    expect(groupRetries(/** @type {any} */ (steps))).toHaveLength(2)
  })
})

describe('planProgress', () => {
  const plan = ['Create database ecommerce', 'Create customers and orders tables', 'Create order items table', 'Seed sample data']

  it('ticks lines whose objects are done and runs the next one', () => {
    const steps = [q('1', 'CREATE DATABASE ecommerce'), q('2', 'CREATE TABLE ecommerce.customers (id int)'), q('3', 'CREATE TABLE ecommerce.orders (id int)', 'running')]
    expect(planProgress(plan, /** @type {any} */ (steps), false)).toEqual(['done', 'running', 'pending', 'pending'])
  })

  it('marks a line failed while its step fails, and done after the retry', () => {
    const failing = [q('1', 'CREATE DATABASE ecommerce'), q('2', 'CREATE TABLE order_items (id int)', 'error')]
    expect(planProgress(plan, /** @type {any} */ (failing), false)[2]).toBe('error')
    const fixed = [...failing, q('3', 'CREATE TABLE `order_items` (id INT)')]
    // Line 2 named nothing that ran, but a later line has: it counts as done.
    expect(planProgress(plan, /** @type {any} */ (fixed), false)).toEqual(['done', 'done', 'done', 'running'])
  })

  it('ticks every line once the turn ends cleanly', () => {
    const steps = [q('1', 'CREATE DATABASE ecommerce')]
    expect(planProgress(plan, /** @type {any} */ (steps), true)).toEqual(['done', 'done', 'done', 'done'])
  })

  it('leaves unmatched lines open when the turn ended on a failure', () => {
    const steps = [q('1', 'CREATE DATABASE ecommerce', 'error')]
    expect(planProgress(plan, /** @type {any} */ (steps), true)).toEqual(['error', 'pending', 'pending', 'pending'])
  })
})

describe('summarizeSteps', () => {
  it('says what was created and how many rows went in', () => {
    const steps = [
      q('1', 'CREATE DATABASE ecommerce'),
      q('2', 'CREATE TABLE customers (id int); CREATE TABLE orders (id int)'),
      q('3', 'CREATE TABLE broken (', 'error'),
      q('4', "INSERT INTO customers VALUES (1, 'a'), (2, 'b')"),
      q('5', 'INSERT INTO orders VALUES (1), (2), (3)'),
      q('6', 'SELECT * FROM orders'),
    ]
    const s = summarizeSteps(/** @type {any} */ (steps))
    expect(s.text).toBe('Created database ecommerce · Created 2 tables · Inserted 5 rows into 2 tables')
    expect(s.sql).toContain('CREATE DATABASE ecommerce;')
    expect(s.sql).not.toContain('broken')
    expect(s.sql).not.toContain('SELECT')
  })

  it('uses the affected count for an insert without VALUES', () => {
    const s = summarizeSteps(/** @type {any} */ ([q('1', 'INSERT INTO archive SELECT * FROM orders', 'done', { affected: 7 })]))
    expect(s.text).toBe('Inserted 7 rows into archive')
  })

  it('is empty for a read-only turn', () => {
    expect(summarizeSteps(/** @type {any} */ ([q('1', 'SELECT 1')]))).toEqual({ text: '', sql: '' })
  })
})

describe('formatStepDuration', () => {
  it.each([[84, '84ms'], [1234, '1.2s'], [12_400, '12s'], [65_000, '1m 5s'], [null, ''], [-1, '']])('%s', (ms, out) => {
    expect(formatStepDuration(ms)).toBe(out)
  })
})
