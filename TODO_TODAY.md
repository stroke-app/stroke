# TODO

What is open as of 2026-11-19 (nightly at 2.20.x, stable at 2.3.0). Security
items live in `TODO_SECURITY.md`.

## v3

Stable has been on 2.3.0 since 2026-11-02, with 273 commits on `nightly`
since. Every Stable user jumps across all of it in one update, so v3 is a
release with a freeze and a test pass, not a plain promote. In order:

- [x] **Feature freeze.** Fixes only on `nightly` until v3 ships.
- [x] **Decide what leaves Labs.** Decided for 3.0: AI chat in a tab stays
      in Labs but is on by default (turned on once for everyone); the tree
      sidebar and several connections stay opt-in.
      Was: Several connections at once and the tree
      sidebar: graduate both to the default (they are what the site and the
      email show) and test them as the default on every engine, or ship v3
      with them still opt-in.
- [ ] **Test the jump from 2.3.0.** Install 2.3.0 with real saved connections,
      settings, saved queries, a theme and a Pro license, then update to the
      v3 build through the updater. Connections, keychain passwords,
      settings, query history, notebooks and Pro all have to survive. On
      macOS, Windows and Linux: the updater is what breaks first.
- [x] **AI access levels for built-in models.** Done in 2.20.4: every
      execute_sql is classified by `access.rs` and refused above the level.
      Was: The Read only / Read & write
      control exists for local agents only (enforced by the MCP server).
      Built-in models (Stroke Free, API keys) can run writes, asking only
      before destructive ones. Enforce a level in `AiChat`'s `execute_sql`
      with the same classifier as `src-tauri/src/mcp/access.rs`, then show
      the control.
- [x] **echarts 6** (on 6.1.0, audit clean): clears the echarts XSS advisory; the charts need
      testing.
- [ ] **Real hardware.** Windows: theme switching and zoom without flicker,
      Alt+K in the SQL editor, the title bar with the sidebar on the right,
      the new fonts, and a click through every page. macOS: offline Pro (sign
      in as Pro, Wi-Fi off, Pro stays on).
- [ ] **Faster startup.** Lazy-load the heavy pages: the entry is 1.3MB and
      143 chunks (1.5MB, the DataTable alone 405KB) preload at startup. New
      users on Windows notice this first.
- [ ] **The license change**, settled before the release: the same wording on
      the site's legal pages, in the app (license page, About) and in the
      email.
- [x] **Release notes: what changed since 2.3.0.** Written once, used for the
      release page, the in-app What's new, the email and the launch posts.
- [x] **Rollback plan.** If v3 breaks on day one, run
      `gh release edit v2.3.0 --repo stroke-app/stroke --latest`: the Stable
      updater reads `releases/latest/download/latest.json`, so nobody else
      moves to v3. The updater never downgrades, so anyone already on v3 needs
      a 3.0.1 fix.
      Before: Keep the 2.3.0 installers easy to find, and know how
      to point the Stable updater back at 2.3.0 if v3 breaks something on day
      one.
- [ ] **Release candidate.** A few days of daily use on Nightly with nothing
      new landing.
- [x] **Ship it.** Label the release PR `release:major` (CONTRIBUTING.md), then
      Actions, *Promote nightly to stable*.

## Next up

- [x] **Layout control.** Dropped; the SQL results bottom/right switch
      replaced it (2.19.x). Was: Three buttons in the title bar for the sidebar (⌘B),
      the terminal panel (⌘J) and the docked AI panel (⌘⌥B), the open edge
      filled. Built on the `layout-and-dashboard` branch; check it in the
      running app on every OS before merging.
- [x] **Dashboard tab redesign.** Shipped, plus a dashboard window. Was: A 12-column grid of chart cards, each with a
      status dot, refresh, an auto-refresh interval, a title and its error on
      the card. Resize and rearrange cards, add one from any saved query, and
      save the layout.
- [ ] **Admin metrics page** on stroke.click, for me only: sign-ups, active
      trials, trial-to-Pro conversion, revenue, downloads, active devices,
      app versions in use, countries, backups and sponsors, over time.
- [x] **Local database servers.** Shipped as their own window. Was: Postgres, MySQL and Redis side by side in
      any version, with Start, Stop, the port and Open, per server on Docker
      or as native binaries; Homebrew services listed too.

## Website

The site is `stroke-web` (TanStack Start on Cloudflare Workers).

- [ ] **Redesign the whole site.** Every page, not a refresh of the home page:
      home, features, pricing, download, databases, compare, docs, changelog,
      blog and the legal pages. Keep the app's look (dark first, the same type
      scale and accent) so the site and the app read as one product.
- [ ] **New pictures.** Fresh screenshots of the app as it is now: the tree
      sidebar, several databases open at once, the data grid, the AI chat, ER
      diagrams, the sign-in flows. Light and dark, the same window size.
- [ ] **A demo on the site.** A short screen recording of real use (connect,
      browse, query, ask the AI) near the top of the home page, with a poster
      frame so the page doesn't wait on the video.
- [ ] **Measure it.** Downloads per OS and trial-to-Pro conversion, so the
      redesign has numbers to be judged by.

## Email

Mail goes out through Plunk (`stroke-web/src/lib/mail`). There are templates
for purchases and failed payments only; nothing sends to every user yet.

- [ ] **Major update and What's new.** One email to every user about the big
      release: what changed since 2.3.0, with the new pictures and a link to
      the changelog and the download. Send it when the release is on Stable.
- [ ] **License update.** One email to license holders about the license
      change. Decide the wording first; it has to match what the site's legal
      pages and the app say.
- [ ] Both need a broadcast path: a template per email, the recipient list
      (users, or license holders only), an unsubscribe link, and a test send to
      myself before the real one.

## Launch week

After v3 is on Stable: the website, then the emails, then the posts, the
same week.

- [ ] **First-run tour in the app.** Everyone the launch brings in lands on
      the empty connect page. A sample database (a bundled SQLite file) that
      opens in one click, and a short tour of the tree sidebar, the AI chat
      and the command palette.
- [ ] **Launch posts.** Product Hunt, Show HN, r/PostgreSQL and r/SQL, the
      same day as the email, with the demo recording and the new pictures.

## After v3

- [ ] **Schema diff** between two databases: tables, columns, indexes and
      constraints that differ, and the SQL that brings one up to the other.
- [ ] **Query parameters**: `:name` / `$1` in a query asks for values (and
      remembers the last ones) before it runs.
