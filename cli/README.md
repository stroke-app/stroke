# strokedb

Open your project's database in [Stroke](https://stroke.click).

```sh
npx strokedb
```

- **Stroke installed?** The app opens with a New Connection form filled in from your project. Check it and press Connect.
- **No app?** [stroke.click/studio](https://stroke.click/studio) opens in your browser with your tables, rows and a SQL console (PostgreSQL, MySQL, MariaDB and SQLite).

## Where it finds the database

1. An argument: `npx strokedb postgres://user:pass@localhost:5432/app` or `npx strokedb ./dev.db`
2. `DATABASE_URL` (or `POSTGRES_URL`, `MYSQL_URL`, `DB_URL`, ...) in the environment
3. The same keys in `.env`, `.env.development`, `.env.local`, `.env.development.local`
4. A `.db` / `.sqlite` file in the folder, or in `prisma/`, `data/` or `db/`

`file:./dev.db` URLs (Prisma, Drizzle) resolve against the project folder.

## Options

| Flag | |
| --- | --- |
| `--web` | Use the web studio even if the app is installed |
| `--app` | Use the app; fail if it isn't installed |
| `--name <name>` | Name for the connection (default: the folder name) |
| `--port <port>` | Port for the local bridge (default: any free port) |
| `--no-open` | Print the studio link instead of opening a browser |
| `--share` | (Labs) Also print a link that opens this studio on any device. Read-only. Needs Node 22+ |
| `--write` | With `--share` or `mcp`: allow edits and any SQL (read-only otherwise) |
| `--table <table>` | Open on this table: `schema.table`, or just its name |
| `--json` | Print the links as one JSON line (`studio`, `share`, `app`); everything else goes to stderr |

## How the web studio is kept private

The browser can't open a database connection, so `strokedb` runs a small bridge on `127.0.0.1` and the studio page sends it queries. The bridge:

- listens on loopback only;
- answers only `https://stroke.click`, and only requests carrying a random token that lives in the link's `#fragment` (a fragment is never sent to a server);
- rejects any other `Host`, so a DNS-rebinding page can't reach it;
- stops when you press Ctrl+C.

Your queries go from your browser to your machine. They never pass through stroke.click.

Safari blocks secure pages from calling `127.0.0.1`; use Chrome, Edge, Firefox or the desktop app.

SQLite in the web studio needs Node 22.5 or newer.

## Sharing (Labs)

`npx strokedb --share` prints a second link that opens the studio on any device, for anyone you send it to, while the command runs.

- It goes through a relay at stroke.click: this command holds a connection open to it, and the studio page sends its calls there. Nothing has to be reachable from the internet.
- End-to-end encrypted. Every call is AES-256-GCM with a key that exists only in the link's `#fragment`; the relay passes ciphertext along and can't read it.
- Read-only unless you add `--write`: browsing, and SQL that only reads (SELECT, WITH, EXPLAIN, SHOW). The shared side has its own read-only connection (Postgres `BEGIN READ ONLY`, MySQL `START TRANSACTION READ ONLY`, SQLite opened read-only), so a statement can't talk its way into a write.
- Stop it with Ctrl+C; the link stops working until you share again (a new run makes a new link).

The desktop app can share an open connection the same way: turn on Settings → Labs → Share as a web studio, then pick **Share as web studio** in the command palette.

Everyone on a shared link sees everyone else: the cell each person is on, their pointer, and what they type before saving. A saved change reloads the table for the rest.

## AI agents

An AI coding agent can show you the database, even when it runs on another machine (a cloud box, a dev container). Add the MCP server to it once, in the project folder:

```sh
claude mcp add strokedb -- npx -y strokedb mcp
```

It finds the database the same way as above (or pass one: `npx -y strokedb mcp postgres://...`) and offers four tools:

| Tool | |
| --- | --- |
| `list_tables` | Tables and views, with row counts |
| `describe_table` | A table's columns, types and keys |
| `query` | One read-only statement, up to 100 rows back as JSON |
| `show_table` | Opens a table for you: a web studio link and a `stroke://` link for the app, both starting on that table |

After the first `show_table`, the agent's next ones switch your open studio to the table it names, with a note saying who opened it. The agent shows up in the studio as a participant.

It's read-only unless the server runs with `--write`. The links are shares like the ones above: end-to-end encrypted, and alive while the MCP server runs.

Without MCP, the same in one command:

```sh
npx strokedb --share --no-open --json --table users
```
