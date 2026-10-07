# Contributing to Stroke

Thanks for wanting to make Stroke better. Bug reports, fixes, features, docs,
and translations are all welcome — this file covers everything you need to get
a change from your machine into a release.

## Before you start

- **Bugs** — [open an issue](https://github.com/stroke-app/stroke/issues)
  with your OS, the database engine, and steps to reproduce. Screenshots or a
  screen recording help a lot.
- **Features** — open an issue first so we can agree on the shape before you
  invest time in an implementation.
- **Small fixes** (typos, obvious one-liners) — a direct PR is fine.

## Development setup

You need [Node.js](https://nodejs.org) 20.19+ (or 22.12+ — CI builds on 22)
and the [Rust toolchain](https://rustup.rs).

```bash
git clone https://github.com/stroke-app/stroke
cd stroke
npm install
npm run tauri          # full desktop app in dev mode (hot-reloads)
```

`npm run dev` runs the frontend alone in a browser — fine for pure UI work,
but anything touching a database needs the full Tauri app (`npm run tauri`).

### Test databases

Real Postgres fixtures (PostGIS and pgvector, seeded with ~1M rows) are one
command away if you have Docker:

```bash
npm run fixtures           # start + seed both (first run takes a few minutes)
npm run fixtures:status    # what's running and how much data is in it
npm run fixtures:down      # stop, keep the data
```

Connection strings are printed when they're ready.

### Checks

Run these before opening a PR — CI runs them too:

```bash
npm run test               # vitest unit tests
npm run build              # frontend production build
cd src-tauri && cargo check
```

## Project layout

| Where | What |
|-------|------|
| `src/lib/components/` | Svelte 5 UI — `StudioShell.svelte` is the main controller |
| `src/lib/api.js` | Frontend → Tauri command bridge |
| `src/lib/stores/` | Persisted local app state |
| `src-tauri/src/db/` | Rust database logic (one module per engine) |
| `src-tauri/src/commands.rs` | Tauri command handlers (keep them thin) |
| `CLAUDE.md` | Architecture and coding conventions in depth |
| `DESIGN_SYSTEM.md` | **Source of truth for all UI** — type scale, tokens, spacing |

## Code conventions

**Svelte** — Svelte 5 runes only (`$state`, `$derived`, `$effect`, `$props`).
Follow `DESIGN_SYSTEM.md` exactly: the `text-ui-*` font scale (never
`text-sm`/`text-[13px]`), fixed control heights (`h-7`/`h-8`/`h-9`),
`size-3.5` icons, semantic color classes (`bg-background`,
`text-muted-foreground`), and the shared components in
`src/lib/components/ui/` instead of ad-hoc primitives.

**Rust** — real logic lives in `src-tauri/src/db/`, command handlers stay
thin. Parameterize query values, quote identifiers, never hold a lock across
an `.await`, and return error messages the UI can show directly.

**Cleanup discipline** — every `addEventListener`, interval, observer, and
Tauri `listen()` needs a teardown path (`$effect` return or `onDestroy`).
Leaks are treated as bugs.

## Commits and pull requests

Commit messages follow Conventional Commits with a scope, matching the
existing history:

```
fix(table): don't claim a table is empty while its rows are still loading
feat(geo): map view for PostGIS layers
```

For PRs:

1. Branch from `master`. Keep the diff focused — unrelated refactors make
   review slow and risky.
2. Make sure the three checks above pass.
3. **Never bump the version number.** CI bumps it automatically when a
   labeled PR is merged; a hand-edited version causes double bumps.
4. Describe what changed and why. For UI changes, include a before/after
   screenshot.

## Contributor License Agreement

Your first PR requires signing the project CLA
([.github/cla/cla.md](.github/cla/cla.md)) — a bot will prompt you on the PR
with instructions. In short: you keep your copyright, and you grant the
project the right to distribute (and re-license) your contribution. This is
what allows the project to ship official builds under the
[Stroke License](LICENSE) and sell Stroke Pro license keys side by side.

## License

Stroke is source-available under the [Stroke License](LICENSE). You can do
anything you like with the source code. Published derived works must say they come from Stroke. Published
compiled apps must contain no malware, must not use the Stroke name or a
look-alike name or icon, and must leave the trial, license-key and payment
systems intact and paying the original author, unless the app is a substantial
improvement in its own right. All of the source, Pro features included, is in
this repository and open to contribution.
