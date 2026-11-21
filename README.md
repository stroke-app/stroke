<div align="center">

<a href="https://stroke.click">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://stroke.click/icon-dark.png">
    <img src="https://stroke.click/icon.png" alt="Stroke" width="88" height="88">
  </picture>
</a>

# Stroke

**A fast, native database client for every database you run.**

Postgres, MySQL, SQLite, MongoDB, Redis and 16 more engines in one app for macOS, Windows and Linux.<br>
Built in Rust, with no Electron and no JVM: it starts in about a second, stays light on memory, and keeps scrolling smoothly through millions of rows.

[![Release](https://img.shields.io/github/v/release/stroke-app/stroke?label=release&color=ffc107&labelColor=1d1d1f)](https://github.com/stroke-app/stroke/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/stroke-app/stroke/total?color=ffc107&labelColor=1d1d1f)](https://github.com/stroke-app/stroke/releases)
[![Platforms](https://img.shields.io/badge/macOS%20·%20Windows%20·%20Linux-1d1d1f?labelColor=1d1d1f)](#install)
[![License](https://img.shields.io/badge/license-Stroke%20License-1d1d1f?labelColor=1d1d1f)](LICENSE)

[**Download**](https://stroke.click/download) · [Website](https://stroke.click) · [Features](https://stroke.click/features) · [Changelog](https://stroke.click/changelog) · [Docs](https://stroke.click/docs)

<br>

<img src="https://stroke.click/shots/mongo.webp" alt="Stroke with a MongoDB collection open in the data grid, and Postgres, Turso and other connections in the sidebar" width="920">

</div>

<br>

## Why Stroke

Any client can draw a grid. Stroke is built for the moments where most of them fall over.

- **Huge results don't freeze it.** Rows stream to disk as they arrive, so a 2 GB result fills the grid at once and memory stays flat.
- **Stop means stopped.** Cancelling a query cancels it on the server, on every engine that can, and the connection is ready for the next one.
- **Nothing changes by accident.** Edits are staged in the grid until you press Apply, transactions show how long they've held their locks, and read-only connections refuse every write.
- **Plans you can read.** `EXPLAIN` becomes a tree of steps with their time, the slow one marked, for SQL and document engines alike.
- **Lost tunnels come back.** If SSH drops, Stroke reconnects it on the same port and tells you it happened.
- **Your credentials stay yours.** Stroke connects straight from your machine, and passwords live in your OS keychain.

## Databases

| Kind | Engines |
|---|---|
| **Relational** | PostgreSQL · MySQL · MariaDB · SQLite · SQL Server · Oracle · CockroachDB · Turso / libSQL · Cloudflare D1 |
| **Document** | MongoDB · CouchDB · Firestore · Cassandra · ScyllaDB |
| **Key-value** | Redis · Valkey · DynamoDB |
| **Analytics** | DuckDB · ClickHouse · PostHog |
| **Search** | Elasticsearch · OpenSearch |

Oracle needs [Oracle Instant Client](https://www.oracle.com/database/technologies/instant-client.html) installed.

**Sign in and pick a database**, no connection string needed:

| Provider | Engine |
|---|---|
| Neon, Supabase, Prisma Postgres, Nile | Postgres |
| PlanetScale, TiDB Cloud | MySQL |
| Turso | libSQL |
| Cloudflare D1 | SQLite |
| Railway | Postgres, MySQL, Redis |
| Upstash, Xata, SingleStore, PostHog | with an API token |

Stroke also finds databases already running on your machine (Docker containers, local Postgres and MySQL, the SQLite file your ORM points at), so a local connection is usually one click. Any connection can go through an SSH tunnel.

## What's inside

**Browse and edit**
- A data grid that paints millions of rows, with filters that write the `WHERE`, multi-column sort, pinned columns and foreign-key jumps
- Inline editing, including Postgres arrays and JSON, staged until you apply it
- Seven views of the same rows: table, JSON, record, text, chart, schema diagram and structure

**Query**
- A SQL editor with schema-aware autocomplete, a result tab per statement, history, saved queries and `.sqlnb` notebooks
- Explicit transactions on Postgres, MySQL, SQLite, DuckDB, SQL Server and Oracle
- Query plans as readable steps, with the slowest one marked

**Beyond SQL**
- MongoDB aggregations built stage by stage, Redis keys with a live server overview, DynamoDB with your AWS profile
- Viewers for JSON, pgvector embeddings and PostGIS geometry, plus a map of every spatial layer

**Schema and data**
- ER diagrams, a relation tree, a schema timeline and data diff
- Prisma and Drizzle codegen
- Backup and restore as SQL dumps, across providers (D1 to Postgres included)

**Share**
- Rows, charts, JSON, queries or a schema as an end-to-end encrypted link that expires when you say. Mask the sensitive columns first

**AI and agents**
- An AI chat that runs queries and draws charts: the free tier, your own key, local models (Ollama, LM Studio) or GitHub Copilot
- A built-in **MCP server**, so Claude Code, Codex, Cursor and other agents query the same databases, held to the access level you set per connection. Destructive statements always ask first

**Everywhere**
- A command palette on `Cmd/Ctrl+K`, split panes, themes and extensions
- Encrypted sync of connections and settings between your computers

## Install

**macOS** with [Homebrew](https://brew.sh)

```bash
brew install --cask stroke-app/tap/stroke
```

**Windows** with [Scoop](https://scoop.sh)

```powershell
scoop bucket add stroke https://github.com/stroke-app/stroke
scoop install stroke
```

**Any system** from a terminal

```bash
curl -fsSL https://stroke.click/install.sh | sh          # macOS and Linux
irm https://stroke.click/install.ps1 | iex               # Windows (PowerShell)
```

Or download an installer from [Releases](https://github.com/stroke-app/stroke/releases):

| System | Package |
|---|---|
| macOS | `.dmg` for Apple Silicon or Intel |
| Windows | `-setup.exe` |
| Ubuntu 22.04+, Debian 12+ | `.deb` |
| Fedora 41+, RHEL 10 | `.rpm` |
| Arch and any distro with glibc 2.35+ | `.AppImage` |

Every feature is free for 17 days. After that, [Stroke Pro](https://stroke.click/pricing) is $9.99 once, with every update included.

<details>
<summary><b>Stroke Nightly</b>: try what's coming</summary>
<br>

Built every night from the `nightly` branch, it installs as a separate app beside Stroke with its own settings, so you can try new work without risking your setup. It updates itself whenever something lands. [Download the nightly](https://github.com/stroke-app/stroke/releases/tag/nightly).

</details>

<details>
<summary><b>First launch warnings and Linux display notes</b></summary>
<br>

- If macOS blocks the first launch, run `xattr -cr /Applications/Stroke.app`.
- If Windows SmartScreen warns, choose **More info → Run anyway**. [Sponsoring](https://stroke.click/sponsors) pays for the certificates that remove both warnings.
- On Wayland, Stroke runs natively. If a launch can't draw there (some GPU and driver setups), the next one falls back to X11 by itself. `STROKE_X11=1` forces X11; `STROKE_WAYLAND=1` forces Wayland and forgets the fallback.

</details>

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

Stroke is a [Tauri](https://tauri.app) app: a Rust core for every database driver, query and stream, and a Svelte interface drawn by the system's own webview.

You need [Node.js](https://nodejs.org) 20.19+ (CI uses 22) and the [Rust toolchain](https://rustup.rs).

```bash
git clone https://github.com/stroke-app/stroke
cd stroke
npm install
npm run tauri          # run in development
npm run tauri:build    # build a release
```

On Arch, `npm run tauri:build:arch` avoids an AppImage linker issue. `npm run tauri:build:mac` and `npm run tauri:build:win` build release installers locally; `scripts/build-local.mjs` lists what each one needs.

## Contributing

Fixes, features and docs are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) covers the dev setup, including one-command Docker test databases, and the PR checklist. Bugs and ideas go in [issues](https://github.com/stroke-app/stroke/issues), and the [roadmap](https://stroke.click/roadmap) shows what's next.

Stroke is built by one developer. [Sponsors](https://stroke.click/sponsors) keep it signed on macOS and Windows.

## License

Stroke is source-available under the [Stroke License](LICENSE). You can do anything you like with the source code, including reusing it in your own free or paid projects. If you publish a work derived from it, say that it comes from Stroke. If you publish a compiled app built from it, it must not contain malware, must not be called Stroke or look like it, and must keep Stroke's trial and license-key payments intact and free to users, unless it is a substantial improvement in its own right. Stroke is not open source by the OSI definition. A [Stroke Pro](https://stroke.click/pricing) license buys a key for the app and funds development.
