import { describe, it, expect } from 'vitest'
import {
  enumConstName,
  buildOrmSchemaModel,
  renderPrismaSchema,
  renderDrizzleSchema,
  renderSqlSchema,
  renderSqlDatabase,
  toCamel,
  toPascal,
} from './orm-schema.js'

/** A two-table Postgres schema with an enum, an FK, a composite PK and indexes. */
function pgFixture() {
  return buildOrmSchemaModel({
    schema: 'public',
    dbType: 'postgres',
    enums: [{ name: 'Role', values: ['USER', 'ADMIN'] }],
    tables: [
      { name: 'users', kind: 'table' },
      { name: 'posts', kind: 'table' },
      { name: 'post_tags', kind: 'table' },
      { name: 'active_users', kind: 'view' },
    ],
    indexes: [
      { name: 'users_pkey', tableName: 'users', columns: 'id', isPrimary: true, isUnique: true },
      { name: 'users_email_key', tableName: 'users', columns: 'email', isPrimary: false, isUnique: true },
      { name: 'posts_pkey', tableName: 'posts', columns: 'id', isPrimary: true, isUnique: true },
      { name: 'posts_author_created_idx', tableName: 'posts', columns: 'author_id, created_at', isPrimary: false, isUnique: false },
      { name: 'post_tags_pkey', tableName: 'post_tags', columns: 'post_id, tag', isPrimary: true, isUnique: true },
    ],
    structures: [
      {
        table: 'users',
        columns: [
          { name: 'id', dataType: 'int4', isNullable: false, columnDefault: "nextval('users_id_seq'::regclass)" },
          { name: 'email', dataType: 'varchar(255)', isNullable: false },
          { name: 'full_name', dataType: 'text', isNullable: true },
          { name: 'role', dataType: 'Role', isNullable: false, columnDefault: "'USER'::\"Role\"" },
          { name: 'created_at', dataType: 'timestamptz', isNullable: false, columnDefault: 'now()' },
        ],
      },
      {
        table: 'posts',
        columns: [
          { name: 'id', dataType: 'int4', isNullable: false, columnDefault: "nextval('posts_id_seq'::regclass)" },
          { name: 'author_id', dataType: 'int4', isNullable: false, foreignKey: 'public.users.id' },
          { name: 'title', dataType: 'text', isNullable: false },
          { name: 'tags', dataType: 'text[]', isNullable: true },
          { name: 'created_at', dataType: 'timestamptz', isNullable: false, columnDefault: 'now()' },
        ],
      },
      {
        table: 'post_tags',
        columns: [
          { name: 'post_id', dataType: 'int4', isNullable: false, foreignKey: 'public.posts.id' },
          { name: 'tag', dataType: 'text', isNullable: false },
        ],
      },
      {
        table: 'active_users',
        columns: [{ name: 'id', dataType: 'int4', isNullable: true }],
      },
    ],
  })
}

describe('naming', () => {
  it('camelCases database names without mangling existing casing', () => {
    expect(toCamel('created_at')).toBe('createdAt')
    expect(toCamel('createdAt')).toBe('createdAt')
    expect(toCamel('ID')).toBe('id')
    expect(toPascal('user_profiles')).toBe('UserProfiles')
  })
})

describe('buildOrmSchemaModel', () => {
  it('leaves views out and records them as skipped', () => {
    const model = pgFixture()
    expect(model.tables.map((t) => t.name)).toEqual(['users', 'posts', 'post_tags'])
    expect(model.skipped).toEqual(['active_users'])
  })

  it('reads keys, uniques and foreign keys off the index list', () => {
    const model = pgFixture()
    const users = model.tables.find((t) => t.name === 'users')
    const postTags = model.tables.find((t) => t.name === 'post_tags')
    expect(users.primaryKey).toEqual(['id'])
    expect(users.columns.find((c) => c.name === 'email').isUnique).toBe(true)
    expect(users.columns.find((c) => c.name === 'id').autoIncrement).toBe(true)
    expect(postTags.primaryKey).toEqual(['post_id', 'tag'])
    expect(model.tables.find((t) => t.name === 'posts').columns.find((c) => c.name === 'author_id').references)
      .toEqual({ table: 'users', column: 'id' })
  })

  it('drops a foreign key that points at something not in the file', () => {
    const model = buildOrmSchemaModel({
      schema: 'public',
      dbType: 'postgres',
      structures: [{ table: 'posts', columns: [{ name: 'author_id', dataType: 'int4', foreignKey: 'other.users.id' }] }],
    })
    expect(model.tables[0].columns[0].references).toBeNull()
  })
})

describe('renderPrismaSchema', () => {
  const out = renderPrismaSchema(pgFixture())

  it('declares the datasource for the connected engine', () => {
    expect(out).toContain('provider = "postgresql"')
    expect(out).toContain('url      = env("DATABASE_URL")')
  })

  it('renders enums and uses them as field types with a default', () => {
    expect(out).toContain('enum Role {\n  USER\n  ADMIN\n}')
    expect(out).toMatch(/role\s+Role\s+@default\(USER\)/)
  })

  it('maps types, nullability and native attributes', () => {
    expect(out).toMatch(/id\s+Int\s+@id @default\(autoincrement\(\)\)/)
    expect(out).toMatch(/email\s+String\s+@unique @db\.VarChar\(255\)/)
    expect(out).toMatch(/fullName\s+String\?\s+@map\("full_name"\)/)
    expect(out).toMatch(/createdAt\s+DateTime\s+@default\(now\(\)\) @db\.Timestamptz @map\("created_at"\)/)
    expect(out).toMatch(/tags\s+String\[\]\?/)
  })

  it('writes both ends of a relation', () => {
    expect(out).toMatch(/author\s+Users\s+@relation\(fields: \[authorId\], references: \[id\]\)/)
    // Back-reference on Users, not pluralized a second time into `postses`.
    expect(out).toMatch(/\n  posts\s+Posts\[\]/)
  })

  it('emits composite keys, indexes and the table mapping', () => {
    expect(out).toContain('@@id([postId, tag])')
    expect(out).toContain('@@index([authorId, createdAt])')
    expect(out).toContain('@@map("post_tags")')
  })

  it('notes the views it left out', () => {
    expect(out).toContain('// Views not shown (1): active_users')
  })
})

describe('renderDrizzleSchema', () => {
  const out = renderDrizzleSchema(pgFixture())

  it('imports exactly the helpers it used', () => {
    const imports = out.match(/import \{ ([^}]+) \} from 'drizzle-orm\/pg-core'/)[1].split(', ')
    expect(imports).toContain('pgTable')
    expect(imports).toContain('varchar')
    expect(imports).toContain('serial')
    expect(imports).toContain('timestamp')
    expect(imports).toContain('index')
    expect(imports).toContain('primaryKey')
    expect(imports).not.toContain('mysqlTable')
    expect(out).toContain("import { relations } from 'drizzle-orm'")
  })

  it('builds columns with names, modifiers and references', () => {
    expect(out).toContain("export const users = pgTable('users', {")
    expect(out).toContain("id: serial('id').primaryKey(),")
    expect(out).toContain("email: varchar('email', { length: 255 }).notNull().unique(),")
    expect(out).toContain("createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),")
    expect(out).toContain("authorId: integer('author_id').notNull().references(() => users.id),")
    expect(out).toContain("tags: text('tags').array(),")
  })

  it('uses the array form for table extras', () => {
    expect(out).toContain('}, (table) => [')
    expect(out).toContain("index('posts_author_created_idx').on(table.authorId, table.createdAt),")
    expect(out).toContain('primaryKey({ columns: [table.postId, table.tag] }),')
  })

  it('declares enums as pgEnum before the tables that use them', () => {
    expect(out).toContain("export const roleEnum = pgEnum('Role', ['USER', 'ADMIN'])")
    expect(out).toContain("role: roleEnum('role')")
    expect(out.indexOf('roleEnum =')).toBeLessThan(out.indexOf('= pgTable('))
  })

  it('writes a relations block for both sides', () => {
    expect(out).toContain('export const postsRelations = relations(posts, ({ one, many }) => ({')
    expect(out).toContain('author: one(users, { fields: [posts.authorId], references: [users.id] }),')
    expect(out).toMatch(/export const usersRelations = relations\(users, \(\{ many \}\) => \(\{/)
  })
})

describe('two keys into the same table, plus a one-to-one', () => {
  const model = buildOrmSchemaModel({
    schema: 'public',
    dbType: 'postgres',
    tables: [{ name: 'users' }, { name: 'posts' }, { name: 'profile' }],
    indexes: [
      { name: 'users_pkey', tableName: 'users', columns: 'id', isPrimary: true, isUnique: true },
      { name: 'posts_pkey', tableName: 'posts', columns: 'id', isPrimary: true, isUnique: true },
      { name: 'profile_pkey', tableName: 'profile', columns: 'id', isPrimary: true, isUnique: true },
      { name: 'profile_user_key', tableName: 'profile', columns: 'user_id', isPrimary: false, isUnique: true },
    ],
    structures: [
      { table: 'users', columns: [{ name: 'id', dataType: 'int4', isNullable: false }] },
      {
        table: 'posts',
        columns: [
          { name: 'id', dataType: 'int4', isNullable: false },
          { name: 'author_id', dataType: 'int4', isNullable: false, foreignKey: 'public.users.id' },
          { name: 'reviewer_id', dataType: 'int4', isNullable: true, foreignKey: 'public.users.id' },
        ],
      },
      {
        table: 'profile',
        columns: [
          { name: 'id', dataType: 'int4', isNullable: false },
          { name: 'user_id', dataType: 'int4', isNullable: false, foreignKey: 'public.users.id' },
        ],
      },
    ],
  })

  it('names each back-reference after its key instead of numbering them', () => {
    const prisma = renderPrismaSchema(model)
    expect(prisma).toMatch(/authorPosts\s+Posts\[\]\s+@relation\("Posts_authorId"\)/)
    expect(prisma).toMatch(/reviewerPosts\s+Posts\[\]\s+@relation\("Posts_reviewerId"\)/)
    expect(prisma).not.toMatch(/posts2/)
  })

  it('destructures the relation helpers the body actually calls', () => {
    const drizzle = renderDrizzleSchema(model)
    // A unique FK is a one-to-one, so the parent side calls one() too.
    expect(drizzle).toContain('export const usersRelations = relations(users, ({ one, many }) => ({')
    expect(drizzle).toContain('profile: one(profile),')
    expect(drizzle).toContain("authorPosts: many(posts, { relationName: 'Posts_authorId' }),")
  })

  it('separates the import block from the declarations', () => {
    expect(renderDrizzleSchema(model)).toMatch(/from 'drizzle-orm'\n\nexport const/)
  })
})

describe('other engines', () => {
  const sqliteModel = buildOrmSchemaModel({
    schema: 'main',
    dbType: 'sqlite',
    indexes: [{ name: 'sqlite_autoindex_todo_1', tableName: 'todo', columns: 'id', isPrimary: true, isUnique: true }],
    structures: [
      {
        table: 'todo',
        columns: [
          { name: 'id', dataType: 'INTEGER', isNullable: false },
          { name: 'title', dataType: 'TEXT', isNullable: false },
          { name: 'done', dataType: 'BOOLEAN', isNullable: false, columnDefault: '0' },
        ],
      },
    ],
  })

  it('renders SQLite through sqlite-core with mode flags', () => {
    const out = renderDrizzleSchema(sqliteModel)
    expect(out).toContain("from 'drizzle-orm/sqlite-core'")
    expect(out).toContain("id: integer('id').primaryKey(),")
    // The stored `0` becomes `false`, matching how boolean mode reads it back.
    expect(out).toContain("done: integer('done', { mode: 'boolean' }).notNull().default(false),")
  })

  it('renders MySQL through mysql-core with lengths', () => {
    const out = renderDrizzleSchema(
      buildOrmSchemaModel({
        schema: 'shop',
        dbType: 'mysql',
        indexes: [{ name: 'PRIMARY', tableName: 'product', columns: 'id', isPrimary: true, isUnique: true }],
        structures: [
          {
            table: 'product',
            columns: [
              { name: 'id', dataType: 'int', isNullable: false, columnDefault: 'auto_increment' },
              { name: 'name', dataType: 'varchar(120)', isNullable: false },
              { name: 'price', dataType: 'decimal(10,2)', isNullable: false },
            ],
          },
        ],
      }),
    )
    expect(out).toContain("from 'drizzle-orm/mysql-core'")
    expect(out).toContain("name: varchar('name', { length: 120 }).notNull(),")
    expect(out).toContain("price: decimal('price', { precision: 10, scale: 2 }).notNull(),")
  })

  it('maps SQL Server types for Prisma', () => {
    const out = renderPrismaSchema(
      buildOrmSchemaModel({
        schema: 'dbo',
        dbType: 'mssql',
        structures: [
          {
            table: 'Account',
            columns: [
              { name: 'Id', dataType: 'uniqueidentifier', isNullable: false },
              { name: 'IsActive', dataType: 'bit', isNullable: false },
            ],
          },
        ],
      }),
    )
    expect(out).toContain('provider = "sqlserver"')
    expect(out).toMatch(/id\s+String/)
    expect(out).toMatch(/isActive\s+Boolean/)
    // Postgres-only native attributes must not leak into a SQL Server schema.
    expect(out).not.toContain('@db.Uuid')
  })
})

describe('renderSqlSchema', () => {
  it('writes a table with its columns, nullability and primary key', () => {
    const out = renderSqlSchema(pgFixture())
    expect(out).toContain('CREATE TABLE "users" (')
    expect(out).toMatch(/"id"\s+\S+\s+NOT NULL/)
    expect(out).toContain('PRIMARY KEY (')
  })

  it('creates enum types before the tables that use them', () => {
    const out = renderSqlSchema(pgFixture())
    const type = out.indexOf('CREATE TYPE')
    const table = out.indexOf('CREATE TABLE')
    expect(type).toBeGreaterThan(-1)
    expect(type).toBeLessThan(table)
  })

  it('emits foreign keys as ALTER after every table exists', () => {
    const out = renderSqlSchema(pgFixture())
    const lastCreate = out.lastIndexOf('CREATE TABLE')
    const firstFk = out.indexOf('ADD CONSTRAINT')
    expect(firstFk).toBeGreaterThan(-1)
    // Inline REFERENCES would require the parent to be created first, which no
    // ordering can guarantee once the schema has a cycle.
    expect(firstFk).toBeGreaterThan(lastCreate)
    expect(out).toContain('FOREIGN KEY')
  })

  it('leaves views out, as the ORM targets do', () => {
    expect(renderSqlSchema(pgFixture())).not.toContain('"active_users"')
  })

  it('quotes identifiers the way each engine does', () => {
    const my = buildOrmSchemaModel({
      schema: 'app',
      dbType: 'mysql',
      tables: [{ name: 'users', kind: 'table' }],
      structures: [{ table: 'users', columns: [{ name: 'id', dataType: 'int', isNullable: false }] }],
    })
    const out = renderSqlSchema(my)
    expect(out).toContain('CREATE TABLE `users`')
    expect(out).not.toContain('CREATE TABLE "users"')
  })

  it('only schema-qualifies where a schema is a real namespace', () => {
    // Postgres qualifies; MySQL's "schema" IS the database, so a prefix there
    // would name something that does not exist.
    expect(renderSqlSchema(pgFixture(), { qualify: true })).toContain('"public"."users"')
    const my = buildOrmSchemaModel({
      schema: 'app',
      dbType: 'mysql',
      tables: [{ name: 'users', kind: 'table' }],
      structures: [{ table: 'users', columns: [{ name: 'id', dataType: 'int', isNullable: false }] }],
    })
    expect(renderSqlSchema(my, { qualify: true })).not.toContain('`app`.`users`')
  })
})

describe('renderSqlDatabase', () => {
  it('keeps same-named tables from different schemas apart', () => {
    const a = pgFixture()
    const out = renderSqlDatabase([a, { ...a, schema: 'audit' }])
    expect(out).toContain('"public"."users"')
    expect(out).toContain('"audit"."users"')
    expect(out).toContain('CREATE SCHEMA IF NOT EXISTS "audit"')
  })

  it('states the preamble once, not per schema', () => {
    const a = pgFixture()
    const out = renderSqlDatabase([a, { ...a, schema: 'audit' }])
    expect(out.match(/SQL schema generated by Stroke/g)).toHaveLength(1)
  })

  it('says so rather than returning nothing when there are no schemas', () => {
    expect(renderSqlDatabase([])).toContain('No schemas')
  })
})

describe('enumConstName', () => {
  it('adds Enum once', () => {
    expect(enumConstName('order_status')).toBe('orderStatusEnum')
    expect(enumConstName('approval_policies_kind_enum')).toBe('approvalPoliciesKindEnum')
    expect(enumConstName('Role')).toBe('roleEnum')
  })
})
