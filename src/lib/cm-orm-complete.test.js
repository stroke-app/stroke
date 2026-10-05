import { describe, it, expect } from 'vitest'
import { ormCompletions, chainBefore, openBrackets } from './cm-orm-complete.js'

const drizzle = { mode: /** @type {const} */ ('drizzle'), tables: ['users', 'posts'], columns: { users: ['id', 'email'], posts: ['id', 'title'] } }
const prisma = { ...drizzle, mode: /** @type {const} */ ('prisma') }
const labels = (text, model) => ormCompletions(text, model)?.options.map((o) => o.label) ?? null

describe('chainBefore', () => {
  it('reads a member chain through calls', () => {
    const t = 'db.select().from(users).where(eq(users.id, 1)).'
    expect(chainBefore(t, t.length - 1).map((s) => `${s.name}${s.call ? '()' : ''}`)).toEqual(['db', 'select()', 'from()', 'where()'])
  })
})

describe('openBrackets', () => {
  it('records keys and callees, skipping strings', () => {
    const stack = openBrackets("prisma.user.findMany({ where: { name: '{(', email: { ")
    expect(stack.map((f) => f.ch + (f.key || f.callee))).toEqual(['(prisma.user.findMany', '{', '{where', '{email'])
  })
})

describe('ormCompletions - Drizzle', () => {
  it('offers the db methods, then the builder methods for the chain', () => {
    expect(labels('db.', drizzle)).toEqual(['select', 'insert', 'update', 'delete'])
    expect(labels('db.select().from(users).', drizzle)).toContain('where')
    expect(labels('db.insert(users).', drizzle)).toEqual(['values', 'returning', 'onConflictDoNothing', 'onConflictDoUpdate', 'toSQL'])
  })

  it("offers a table's columns after its name", () => {
    expect(labels('db.select().from(users).where(eq(users.', drizzle)).toEqual(['id', 'email'])
  })

  it('asks for the columns of a table not loaded yet', () => {
    expect(ormCompletions('posts.', { ...drizzle, columns: {} })?.needsColumns).toBe('posts')
  })

  it('offers db, the tables and the helpers while a name is typed', () => {
    const l = labels('const q = e', drizzle)
    expect(l).toContain('db')
    expect(l).toContain('users')
    expect(l).toContain('eq')
  })
})

describe('ormCompletions - Prisma', () => {
  it('offers the models, then their methods', () => {
    expect(labels('prisma.', prisma)).toEqual(['users', 'posts'])
    expect(labels('prisma.users.', prisma)).toContain('findMany')
  })

  it("offers a method's arguments in its object", () => {
    expect(labels('prisma.users.findMany({ ', prisma)).toEqual(['where', 'orderBy', 'take', 'skip', 'select', 'cursor'])
    expect(labels('prisma.users.create({ ', prisma)).toEqual(['data', 'select'])
  })

  it('offers columns and logic in where, filters in a column, columns in AND', () => {
    expect(labels('prisma.users.findMany({ where: { ', prisma)).toEqual(['id', 'email', 'AND', 'OR', 'NOT'])
    expect(labels('prisma.users.findMany({ where: { email: { ', prisma)).toContain('contains')
    expect(labels('prisma.users.findMany({ where: { AND: [{ ', prisma)).toEqual(['id', 'email', 'AND', 'OR', 'NOT'])
    expect(labels('prisma.users.findMany({ where: { id: 1, ', prisma)).toEqual(['id', 'email', 'AND', 'OR', 'NOT'])
  })

  it('offers columns in select and data', () => {
    expect(labels('prisma.posts.findMany({ select: { ', prisma)).toEqual(['id', 'title'])
    expect(labels('prisma.posts.update({ where: { id: 1 }, data: { ', prisma)).toEqual(['id', 'title'])
  })

  it('stays quiet at a value', () => {
    expect(ormCompletions('prisma.users.findMany({ where: { id: ', prisma)).toBeNull()
  })
})
