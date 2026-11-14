// Subtle per-engine icon tint (color-500/600), theme-aware via Tailwind tokens.
export const ENGINE_TINT = {
  postgres: "text-sky-500/80",
  cockroachdb: "text-teal-500/80",
  mysql: "text-cyan-500/80", // the dolphin is teal, not amber
  mariadb: "text-orange-500/80",
  sqlite: "text-blue-500/80",
  "sqlite-memory": "text-blue-500/80",
  mssql: "text-red-500/80",
  oracle: "text-red-600/80",
  clickhouse: "text-yellow-500/80",
  duckdb: "text-yellow-500/80",
  "duckdb-memory": "text-yellow-500/80",
  d1: "text-orange-500/80",
  libsql: "text-emerald-500/80",
  docker: "text-sky-500/80",
  neon: "text-emerald-500/80",
  supabase: "text-emerald-500/80",
  planetscale: "text-foreground/70",
  prisma: "text-indigo-500/80",
  tidb: "text-red-500/80",
  turso: "text-teal-500/80",
  railway: "text-foreground/80",
  nile: "text-violet-500/80",
  upstash: "text-emerald-500/80",
  posthog: "text-amber-500/80",
  drizzle: "text-lime-500/80",
  redis: "text-red-500/80",
  valkey: "text-sky-500/80",
  dynamodb: "text-blue-500/80",
  couchdb: "text-red-600/80",
  mongodb: "text-green-600/80",
  cassandra: "text-sky-600/80",
  firestore: "text-amber-500/80",
};
/** @param {string} id */
export function engineTint(id) {
  return ENGINE_TINT[id] ?? "text-muted-foreground";
}
