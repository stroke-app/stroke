<div align="center">

# Stroke

**A fast desktop database client, for people and for their AI tools.**

[![Stroke launch video](assets/launch-video/stroke-launch.gif)](assets/launch-video/stroke-launch.mp4)

<sub>[Watch the 44-second launch video with sound](assets/launch-video/stroke-launch.mp4)</sub>

[Download](#install) · [What's inside](#whats-inside) · [Build from source](#build-from-source) · [stroke.click](https://stroke.click)

</div>

Stroke is a Rust + Svelte app for browsing, editing and querying databases. It stays quick on tables with millions of rows, works from the keyboard, and ships an MCP server so Claude, Cursor and other agents can query the same databases you do.

## Databases

**Engines:** PostgreSQL, MySQL, MariaDB, CockroachDB, SQLite, Turso / libSQL, Cloudflare D1, ClickHouse, DuckDB, SQL Server and Redis.

**Sign in and pick a database**, no connection string needed:

| Provider | Engine |
|---|---|
| Neon, Supabase, Prisma Postgres, Nile | Postgres |
| PlanetScale, TiDB Cloud | MySQL |
| Turso | libSQL |
| Cloudflare D1 | SQLite |
| Upstash | Redis |
| Railway | Postgres, MySQL, Redis |
| PostHog | HogQL, read-only |

Stroke also finds databases already running on your machine (Docker containers, local Postgres and MySQL, the SQLite file your ORM points at), so a local connection is usually one click, and a right-click restarts or stops a container. Anything can go through an SSH tunnel.

## What's inside

- **Data grid** that paints millions of rows, with filters, multi-column sort, pinned columns, foreign-key jumps and inline editing, including Postgres arrays and JSON
- **SQL console** with schema-aware autocomplete, EXPLAIN plans, history, saved queries and `.sqlnb` notebooks
- **Viewers** for JSON, pgvector embeddings and PostGIS geometry, plus a map view of every spatial layer
- **Schema tools:** ER diagrams, a relation tree, schema timeline, data diff, and Prisma or Drizzle codegen
- **Charts and dashboards** from any query result
- **Backup and restore** as SQL dumps for Postgres, MySQL, SQLite and D1
- **Read-only mode** for browsing production without the risk
- **AI chat** that runs queries and draws charts, with a free tier, your own key, local models (Ollama, LM Studio) or GitHub Copilot. Destructive statements always ask first
- **MCP server:** start it in Settings, copy the config for your client, done
- **Command palette** on `Cmd/Ctrl+K`, split panes, themes and extensions

Keys and provider tokens live in the OS keychain.

## Install

**macOS** ([Homebrew](https://brew.sh))

```bash
brew install --cask stroke-app/tap/stroke
```

**Windows** ([Scoop](https://scoop.sh))

```powershell
scoop bucket add stroke https://github.com/stroke-app/stroke
scoop install stroke
```

Or grab an installer from [Releases](https://github.com/stroke-app/stroke/releases): `.dmg` for macOS (Apple Silicon or Intel), `-setup.exe` for Windows, `.deb` or `.AppImage` for Linux.

If macOS blocks the first launch, run `xattr -cr /Applications/Stroke.app`. If Windows SmartScreen warns, choose **More info → Run anyway**.

## Shortcuts

| Keys | Action |
|---|---|
| `Cmd/Ctrl+K` | Command palette |
| `Cmd/Ctrl+Enter` | Run the query |
| `Cmd/Ctrl+T` | Search tables |
| `Cmd/Ctrl+B` | Toggle the sidebar |
| `Cmd/Ctrl+W` | Close the tab |
| `Alt+wheel` | Scroll the grid a few rows at a time |
| `Cmd/Ctrl+?` | Every shortcut |

## Build from source

Needs [Node.js](https://nodejs.org) 20.19+ (CI uses 22) and the [Rust toolchain](https://rustup.rs).

```bash
git clone https://github.com/stroke-app/stroke
cd stroke
npm install
npm run tauri          # dev
npm run tauri:build    # release build
```

On Arch, `npm run tauri:build:arch` avoids an AppImage linker issue. `npm run tauri:build:mac` and `npm run tauri:build:win` build release installers locally; `scripts/build-local.mjs` lists what each one needs.

## License

Source-available under the [Stroke Sustainable Use License](LICENSE). Use it anywhere, personally or at work, and read, modify and contribute to all of it. Reselling or rebranding it, or offering it as a hosted service, is not allowed. Pro features are key-gated in official builds and a [Stroke Pro](https://stroke.click/pricing) license funds development.

## Contributing

Fixes, features and docs are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) covers the dev setup, including one-command Docker test databases, and the PR checklist. Bugs and ideas go in [issues](https://github.com/stroke-app/stroke/issues).
