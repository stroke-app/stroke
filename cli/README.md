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

## How the web studio is kept private

The browser can't open a database connection, so `strokedb` runs a small bridge on `127.0.0.1` and the studio page sends it queries. The bridge:

- listens on loopback only;
- answers only `https://stroke.click`, and only requests carrying a random token that lives in the link's `#fragment` (a fragment is never sent to a server);
- rejects any other `Host`, so a DNS-rebinding page can't reach it;
- stops when you press Ctrl+C.

Your queries go from your browser to your machine. They never pass through stroke.click.

Safari blocks secure pages from calling `127.0.0.1`; use Chrome, Edge, Firefox or the desktop app.

SQLite in the web studio needs Node 22.5 or newer.
