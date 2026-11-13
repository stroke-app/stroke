// What the AI needs to know to write queries for engines whose query editor
// doesn't take SQL (or takes a SQL-like dialect with its own rules). Each entry
// is the language's name, a fence tag for code blocks, and a compact guide with
// examples that the query editor accepts as written. Pure data: the prompt
// builders pick the entry for the connection's engine.

/**
 * @typedef {{ name: string, fence: string, collectionNoun: string, guide: string }} QueryLanguage
 */

/** @type {Record<string, QueryLanguage>} */
export const QUERY_LANGUAGES = {
  mongodb: {
    name: 'MongoDB shell (mongosh) syntax',
    fence: 'javascript',
    collectionNoun: 'collection',
    guide: `Write MongoDB shell statements, one per line, ending with ";". Not SQL.
- Reads: db.users.find({plan: "pro", age: {$gt: 30}}).sort({age: -1}).limit(20);
  db.users.findOne({_id: ObjectId("65a1b2c3d4e5f60718293a4b")});
  db.orders.aggregate([{$match: {status: "paid"}}, {$group: {_id: "$userId", total: {$sum: "$total"}}}, {$sort: {total: -1}}, {$limit: 10}]);
  db.users.countDocuments({plan: "pro"}); db.users.distinct("plan"); db.users.getIndexes();
- Writes: insertOne, insertMany, updateOne/updateMany with operators ({$set: {...}}), replaceOne, deleteOne/deleteMany (with a filter).
- Values: ObjectId("…"), ISODate("2026-01-01"), NumberLong(…), NumberDecimal("…"). Field names may be unquoted.
- "db" is the database selected in the sidebar; "use <db>;" switches it. For odd names use db.getCollection("name").
- Always add .limit() to find() unless the user asks for everything.`,
  },
  couchdb: {
    name: 'CouchDB Mango queries (shell-style)',
    fence: 'javascript',
    collectionNoun: 'database',
    guide: `Write one statement per line. Not SQL. The query editor takes:
- <db>.find({selector}) or <db>.find({selector}, {"limit": 50, "sort": [{"price": "asc"}], "fields": ["name", "price"]})
  Selectors are Mango: {"type": "product", "price": {"$gt": 50}}, operators $eq $ne $gt $gte $lt $lte $in $nin $exists $regex $elemMatch $and $or.
  Sorting needs an index on the sort fields.
- <db>.get("doc-id") for one document, <db>.all() for every document (capped).
- Keys in selectors and options are quoted JSON strings. The editor is read-only for CouchDB: changes are made in the Browse page.`,
  },
  firestore: {
    name: 'Firestore query chain',
    fence: 'javascript',
    collectionNoun: 'collection',
    guide: `Write one statement per line. Not SQL. The query editor takes:
- users.where("age", ">", 30).where("plan", "==", "pro").orderBy("age", "desc").limit(10)
  Operators: == != < <= > >= in not-in array-contains array-contains-any. Values are JSON.
- A subcollection is a path: users/u1/orders.limit(20)
- One document: users.get("u1"). A bare collection path lists its documents: products
- Range filters on one field plus orderBy on another need a composite index in Firestore.
  The editor is read-only for Firestore: changes are made in the Browse page.`,
  },
  cassandra: {
    name: 'CQL (Cassandra Query Language)',
    fence: 'sql',
    collectionNoun: 'table',
    guide: `Write CQL statements ending with ";". CQL looks like SQL but:
- No JOINs, no subqueries, no OR in WHERE, GROUP BY only on primary key columns.
- WHERE must restrict the partition key (equality or IN) unless the query ends with ALLOW FILTERING; only use ALLOW FILTERING when the user accepts a full scan.
- Clustering columns can be ranged in key order; ORDER BY only on clustering columns, and only with the partition key restricted.
- Always add LIMIT to SELECTs. Qualify tables as keyspace.table unless the sidebar's keyspace is meant.
- Collections: set<…>, list<…>, map<…,…>; literals {'a','b'}, ['a'], {'k': 'v'}. Writes: INSERT, UPDATE … SET, DELETE (by primary key).`,
  },
  dynamodb: {
    name: 'PartiQL for DynamoDB',
    fence: 'sql',
    collectionNoun: 'table',
    guide: `Write PartiQL statements. PartiQL looks like SQL but:
- Table names are double-quoted: SELECT * FROM "stroke-test" WHERE pk = 'USER#1';
- String literals use single quotes; numbers are bare. Indexes: SELECT * FROM "table"."index-name" WHERE gsi1pk = 'X';
- A SELECT without the partition key in WHERE scans the whole table: put the key in WHERE whenever possible.
- No JOINs, no GROUP BY, no aggregate functions except size(), no LIMIT clause (results are paged).
- Functions: begins_with(sk, 'ORDER#'), contains(tags, 'beta'), attribute_exists(x), attribute_type(x, 'S'), size(x).
- Writes: INSERT INTO "t" VALUE {'pk': 'a', 'sk': 'b'}; UPDATE "t" SET x = 1 WHERE pk = 'a' AND sk = 'b'; DELETE FROM "t" WHERE pk = 'a' AND sk = 'b';`,
  },
  redis: {
    name: 'Redis commands',
    fence: 'redis',
    collectionNoun: 'key',
    guide: `Write Redis commands, one per line, as redis-cli takes them. Not SQL.
- Reads: GET k, HGETALL k, HGET k f, LRANGE k 0 49, SMEMBERS k, ZRANGE k 0 9 WITHSCORES, TYPE k, TTL k, SCAN 0 MATCH user:* COUNT 100.
- Never use KEYS * (it blocks the server); use SCAN with MATCH and COUNT.
- Writes: SET, HSET, LPUSH/RPUSH, SADD, ZADD, EXPIRE, DEL. Quote values with spaces.`,
  },
}

QUERY_LANGUAGES.valkey = QUERY_LANGUAGES.redis

/** @param {string | null | undefined} engine @returns {QueryLanguage | null} */
export function queryLanguageFor(engine) {
  // Valkey is Redis on the wire: same commands, same prompt.
  if (engine === 'valkey') engine = 'redis'
  return (engine && QUERY_LANGUAGES[engine]) || null
}

/**
 * The prompt section for a non-SQL engine: language, its rules, and the
 * collections the user has (names only; documents are schemaless).
 * @param {string} engine
 * @param {{ schema?: string, collections?: string[], sample?: Record<string, string[]> }} [ctx]
 *   `sample` maps a collection to field names seen in its documents.
 */
export function queryLanguagePrompt(engine, ctx = {}) {
  const lang = queryLanguageFor(engine)
  if (!lang) return ''
  const lines = [`Query language: ${lang.name}. Code blocks use \`\`\`${lang.fence}.`, '', lang.guide]
  const colls = ctx.collections ?? []
  if (colls.length) {
    lines.push('', `${ctx.schema ? `${cap(lang.collectionNoun)}s in ${ctx.schema}` : `${cap(lang.collectionNoun)}s`} (${colls.length}):`)
    for (const c of colls.slice(0, 200)) {
      const fields = ctx.sample?.[c]
      lines.push(fields?.length ? `- ${c}: ${fields.slice(0, 40).join(', ')}` : `- ${c}`)
    }
    if (colls.length > 200) lines.push(`- … ${colls.length - 200} more`)
  }
  return lines.join('\n')
}

/** @param {string} s */
const cap = (s) => s.charAt(0).toUpperCase() + s.slice(1)

/**
 * A statement that reads a few records of one collection, in the engine's
 * own language; null where any such read would scan (DynamoDB without a key)
 * or there are no collections (Redis).
 * @param {string} engine @param {string} schema @param {string} coll @param {number} n
 */
export function sampleQuery(engine, schema, coll, n) {
  const q = (/** @type {string} */ s) => `"${s.replace(/"/g, '""')}"`
  switch (engine === 'valkey' ? 'redis' : engine) {
    case 'mongodb':
      return /^[A-Za-z_$][\w$]*$/.test(coll) ? `db.${coll}.find({}).limit(${n})` : `db.getCollection(${JSON.stringify(coll)}).find({}).limit(${n})`
    case 'couchdb':
      return `${coll}.find({}, {"limit": ${n}})`
    case 'firestore':
      return `${coll}.limit(${n})`
    case 'cassandra':
      return `SELECT * FROM ${schema ? `${q(schema)}.` : ''}${q(coll)} LIMIT ${n}`
    default:
      return null
  }
}

/**
 * What describe_table reports for a schemaless collection: each field seen in
 * the sample with its JSON type(s), and the sample itself.
 * @param {string} coll @param {{ columns?: { name: string }[], rows?: any[][] }} result
 */
export function describeFromSample(coll, result) {
  const names = (result.columns ?? []).map((c) => c.name)
  /** @type {Map<string, Set<string>>} */
  const types = new Map(names.map((n) => [n, new Set()]))
  const docs = (result.rows ?? []).map((r) => Object.fromEntries(names.map((n, i) => [n, r[i]])))
  for (const d of docs) {
    for (const [k, v] of Object.entries(d)) {
      if (v === null || v === undefined) continue
      types.get(k)?.add(Array.isArray(v) ? 'array' : typeof v === 'object' ? 'object' : typeof v)
    }
  }
  return {
    collection: coll,
    fields: names.map((n) => ({ name: n, types: [...(types.get(n) ?? [])] })),
    sample: docs.slice(0, 3),
    note: docs.length ? `Fields seen in ${docs.length} sampled record(s); others may exist.` : 'No records to sample.',
  }
}

/** Query fences whose code blocks are runnable for the given engine. @param {string | null | undefined} engine */
export function runnableFences(engine) {
  const lang = queryLanguageFor(engine)
  if (!lang) return null
  const extra = { mongodb: ['javascript', 'js', 'mongodb', 'mongo'], couchdb: ['javascript', 'js', 'json'], firestore: ['javascript', 'js'], cassandra: ['sql', 'cql'], dynamodb: ['sql', 'partiql'], redis: ['redis', 'text', 'bash'], valkey: ['redis', 'text', 'bash'] }
  return new Set([lang.fence, ...(/** @type {Record<string, string[]>} */ (extra)[/** @type {string} */ (engine)] ?? [])])
}
