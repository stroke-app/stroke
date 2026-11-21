import { describe, it, expect } from 'vitest'
import { evalPrismaQuery } from './orm-builder.js'

const sql = (/** @type {string} */ code) => evalPrismaQuery(code, ['users']).sql

describe('Prisma to SQL', () => {
  it('quotes field names, so a camelCase field keeps its case on Postgres', () => {
    expect(sql('prisma.users.findMany({ where: { createdAt: { gt: 5 } }, orderBy: { createdAt: "desc" }, select: { firstName: true } })'))
      .toBe('SELECT "firstName" FROM "users" WHERE "createdAt" > 5 ORDER BY "createdAt" DESC')
  })

  it("escapes quotes in contains, startsWith and endsWith", () => {
    expect(sql(`prisma.users.findMany({ where: { name: { contains: "O'Neil" } } })`))
      .toBe(`SELECT * FROM "users" WHERE "name" LIKE '%O''Neil%'`)
    expect(sql(`prisma.users.findMany({ where: { name: { startsWith: "it's" } } })`)).toContain(`LIKE 'it''s%'`)
  })

  it('quotes columns in writes, and an unknown sort direction is ASC', () => {
    expect(sql('prisma.users.create({ data: { firstName: "Ada" } })')).toBe(`INSERT INTO "users" ("firstName") VALUES ('Ada')`)
    expect(sql('prisma.users.update({ where: { id: 1 }, data: { firstName: "Ada" } })')).toBe(`UPDATE "users" SET "firstName" = 'Ada' WHERE "id" = 1`)
    expect(sql('prisma.users.findMany({ orderBy: { id: "sideways; DROP" } })')).toBe('SELECT * FROM "users" ORDER BY "id" ASC')
  })

  it('keeps AND / OR / NOT as logic, not columns', () => {
    expect(sql('prisma.users.findMany({ where: { OR: [{ id: 1 }, { id: 2 }] } })')).toBe('SELECT * FROM "users" WHERE (("id" = 1) OR ("id" = 2))')
  })
})
