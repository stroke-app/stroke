# TODO: security pass after v2.3.0

Findings from an audit of v2.3.0 (local agents, license, dependencies).
Sections 1-3 shipped in 2.3.x-2.5.x; what is left is unchecked below.

## 1. Agent access level has holes (dangerous, shipped in 2.3.0)

`src-tauri/src/mcp/server.rs` only checks the access level for `execute_sql`.
Agents can call every Stroke tool, so "Read only" is not read only.

- [x] `export_query` (`mcp/tools.rs`) writes its output to any `path`. An agent
      on Read only can `SELECT '<payload>'` into `~/.bashrc` or the Windows
      Startup folder and get code run on the machine. Text in a table row is
      enough to prompt it (prompt injection). Deny `path` for agent sessions.
- [x] `export_query`'s guard `is_read_only_sql` only looks at the first word,
      so `SELECT 1; DROP TABLE x` passes. Run it through
      `access::script_level` and refuse more than one statement.
- [x] `explain_query` pastes raw SQL after `EXPLAIN`. On MSSQL
      `SET SHOWPLAN_ALL OFF; DROP TABLE x` runs the DROP; DuckDB, libSQL and D1
      likely take multiple statements too. Gate it on the level and refuse
      multiple statements.
- [x] Gate on the level in one place for every tool that takes SQL, not per
      tool name.
- [x] The classifier treats every `SELECT` as read. Side effects still get
      through: `SELECT ... INTO newtable`, `SELECT pg_terminate_backend(..)`,
      `setval`, `dblink_exec`, MySQL `INTO OUTFILE`, SQLite `PRAGMA x = y`.
      At Read only, also run the statement in a read-only transaction where the
      engine has one (`SET TRANSACTION READ ONLY` on Postgres/MySQL).
- [x] Tests for each bypass above in `mcp/access.rs` and the server.
- [x] Same `export_query` gap exists in the normal MCP read-only mode; fix it
      there too.

- [x] MSSQL `explain_query` turns `SET SHOWPLAN_ALL ON` and never turns it off
      on the pooled connection, so later queries on it return plans instead of
      rows. Turn it off in the same call (or use a dedicated connection).
      (`db::mssql::explain_showplan_all`: ON / query / OFF as separate batches
      under one lock; OFF runs even when the query fails.)
- [x] Engines with no read-only transaction now enforce it at Read only too:
      ClickHouse with the server's `readonly=1` setting; libSQL, DuckDB and
      MSSQL inside a transaction that is always rolled back.
- [ ] Still classifier-only at Read only: D1 (no transactions over its HTTP
      API), PostHog (HogQL is read-only by design) and the NoSQL engines.

## 2. License can delete a paying user's key

`run_license_check` (`commands.rs`) deletes `license.json` on any answer that
is not `valid: true`.

- [x] Offline activation never registers the device, so the next online check
      gets `device_not_activated` and Pro disappears. Re-activate in that case
      instead of deleting.
- [x] Only delete on `revoked` or `expired`. Keep the key on `not_found`,
      unknown errors, or a bad deploy of `/api/license/check`.
- [x] Header comment in `license.rs` says a 5-day trial; `TRIAL_DAYS` is 17.
- [ ] Known and accepted: seat limit can be dodged by blocking stroke.click at
      activation (until the next online check), and rolling the clock back
      stretches the trial. Client-side checks; the LICENSE covers patching.
- [x] Delete the `audit-probe` device row I created in the prod trial table
      while probing `/api/license/trial`.

## 3. Dependencies (Dependabot: 22 open, 2 critical → 1 low, #138)

- [x] `zip` (critical, high): zip 6 came from libduckdb-sys 1.10503's build
      script, not the updater. duckdb 1.10506.1 builds with zip 9.
- [x] `rustls` 0.21 (via tiberius 0.12): tiberius 0.13 uses rustls 0.23.46.
      It also brings aws-lc-rs, so `run` installs ring as the process-wide
      TLS provider.
- [x] `dompurify`: 3.4.17 through the override.
- [x] `vite`, `@vitest/mocker`, `source-map-js`, `katex` (override ^0.18.2).
- [ ] `deepmerge` (low): no fixed release exists; build-time only (via
      @sveltejs/vite-plugin-svelte). Revisit when one ships.
- [ ] `echarts` < 6.1.0 (moderate, XSS; `npm audit`, not Dependabot): needs
      the echarts 6 upgrade (PR #14), which needs the charts tested.

## 4. Housekeeping

- [x] No secrets in the repo or history (checked: only sqlx doc examples match
      `PRIVATE KEY`). MCP binds 127.0.0.1 and always requires its bearer token.
