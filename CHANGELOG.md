# Changelog

All notable changes to Stroke are listed here, newest first.

---

## [2.2.3] - 2026-10-03

### New Features

#### SQL editor
- The console and notebook cells run on CodeMirror: faster to open and type in, with suggestions that stay at the caret
- Schema-aware completion: tables after FROM, UPDATE and INTO, the named table's columns with their types first, `alias.` and `schema.` lookups, functions with fill-in arguments, enum values and your own functions
- Columns are suggested for any table in the schema, opened or not, and typing `"` lists column names
- 34 snippets named after their keyword (`sel`, `ins`, `upd`, `join`), with a preview beside the list
- SQL editor settings in Settings → Database and behind the toolbar's sliders: text size, line wrap (Alt+Z), line numbers, fold arrows, suggestions while typing and problem markers
- A spinner beside each running statement turns into a ✓ when it finishes
- A failed statement shows the database's message and where it failed, with the failing text underlined in the editor and in the results
- Run comes first in the console toolbar and Stop takes its place, so nothing moves when a run starts

#### Data model (was ER diagram)
- The ER diagram is now the Data model: one tab with five views of the same tables, Diagram, Mermaid, Tree, Dictionary and DDL
- Each view keeps its own controls in one toolbar: search and settings on the Diagram, the column search on the Dictionary, copy and Export .sql on the DDL, and the table filter, hops, zoom and Flow / All / List on the Tree
- The Diagram places cards and routes lines in one pass, so a line never crosses a card and lines sharing a channel get a track each. A 135-table schema lays out in under a second instead of 45
- Hub tables (`tenants`, `users`) are named in a pill on each referencing row instead of drawing a line from every card; their lines show for the card under the pointer or the selected one
- Mermaid shows the page as `erDiagram` source beside a live preview, to copy, edit, or save as a diagram with Edit in Diagrams
- Tree shows what a table points at and what points at it: as a map of the whole page (the default), a flowchart around one table, or a list with row counts and 1:1 / N:1 / 1:N on each link
- Dictionary lists every column on the page with its type, nullability, keys and references, searchable with ⌘F; a table name opens the table
- DDL shows the CREATE statements of the tables on the page, with copy and export to a .sql file
- The table filter shows what is on the page and changes it a tick at a time, with All, Linked and + Neighbours, Enter to keep only the search matches, and Focus for one table and its links
- Full screen from the toolbar or ⌘⇧↵ (Ctrl+Shift+Enter); Esc leaves it
- Mermaid diagrams follow the app theme, stay sharp at every zoom, and zoom out only until the whole picture fits

#### Data grid
- Staged edits are reviewed in a resizable dock under the grid instead of a dialog, so the rows stay in view; the SQL follows the grid and can be edited with completion
- Following a foreign key to a single row opens that row as JSON (Settings → Data grid → Expand single related row)

#### AI
- A plain chat turn sends about 2k tokens instead of 8k: skills and tools come along only when the conversation needs them, and old results are trimmed from the history
- The conversation title no longer costs a second model call, which kept tripping the free tier's rate limit
- Chat and code text follow the app zoom

### Bug Fixes
- A very large result no longer closes the app: a 5-million-row `SELECT` used to come back as one message. Postgres results now spool into a store in the backend as they arrive, the grid loads the rows near the screen as you scroll, and sorting runs in the store by the column's type
- A console query on Postgres waits on half the network round trips (4 instead of 8 on a fresh connection), so a small query to a far database comes back in about half the time
- Primary keys come from the catalog on every engine, so a `uuid` or text key shows as a key on the cards, in keys-only mode and in the Mermaid source
- Scrolling with an expanded JSON row open no longer drops to 11-20 fps, and the grid no longer shows torn rows after a zoom or opening a dock
- Switching back to a tab with an expanded JSON row no longer jumps
- The title bar no longer slides up under the window edge
- The connecting spinner turns with Reduce Motion on, and the Stop button's spinner is centred
- Format keeps short clauses on one line
- A query without a trailing `;` is no longer flagged; a missing `;` between two queries is
- Line numbers stay aligned after a zoom or font change

### Changes
- `id`, `name`, `type` and other everyday column names are no longer coloured as keywords
- The diagram settings are one compact row each, with Copy as PNG and one Download row for PNG, SVG and Mermaid


## [2.2.2] - 2026-09-29

### New Features
- Connect PostHog with a personal API key and browse or query your product analytics with HogQL, read-only
- Railway is available in the provider picker
- Right-click a Docker database to restart or stop it, or copy its connection URL
- Connecting shows a full loading screen with the database's logo, its host, elapsed time and Cancel

### Bug Fixes
- The expanded row JSON and the row panel update right after a cell edit
- MySQL 8 and 9 tables show their columns when empty, and inserted rows appear without a refresh
- Nile no longer drops its connection after every query
- The connect dialog footer names the host being dialled, and Resume only spins when you pressed it

### Changes
- Connecting to a far database costs one handshake instead of two (Prisma Postgres: 6.2s to 0.6s)
- Far databases keep a warm pool, so opening a table never waits on new connections
- Every query to a far database is one round trip shorter, and a table's first open skips a lookup
- Provider pickers answer sooner: the API connection opens with the dialog, and hovering a database starts building its connection
- PlanetScale lists databases and connects in fewer API calls
- MySQL tables open in one round trip
- Provider sign-in is sturdier across IPv4 and IPv6 callbacks
- A tidier menu bar, and the sidebar shows its Enter hint only while searching


## [2.2.1] - 2026-09-29

### New Features
- Connect TiDB Cloud, Turso, Nile and Upstash from the provider picker. TiDB shows its sign-in code right in the app
- Alt+wheel scrolls the table a few rows at a time, for landing on a precise row in a huge table
- Sign out of a provider from its card, with a confirm first
- Saved connections show their provider's logo and a small chip naming the engine
- Keyboard shortcuts in the connections dialog: new connection, paste a URL, filter and resume the last one
- The MCP dialog copies a ready config for every client
- A Claude font preset in Settings → Appearance

### Bug Fixes
- A SQL run that ends in a SELECT keeps its earlier writes instead of rolling them back
- Picking a saved provider database connects to that database, never one with a similar name
- Passwords with a raw %, #, ? or @ import from a connection URL whole
- Stopping a query in the editor stops it on the server too
- Deleted connections stay deleted, and deleting one asks first
- Provider sign-in survives a restart on Windows
- PlanetScale lists databases across organizations and creates passwords again
- Upstash connections keep their provider after a restart
- One organization the token can't read no longer empties the whole database list
- The splash screen no longer flashes one colour and then another
- A chart on a windowed table uses only the rows that are loaded

### Changes
- Provider databases open on the last list instead of a spinner, and reconnecting skips the API calls entirely
- Connecting to far away databases is faster: no restarted handshakes, no double connect, one Redis connection reused
- Geist is the default font again
- The connect page uses the full width with four cards per row, and the New table dialog reads as one table
- An empty sidebar tab shows one empty state instead of two
- Failed database picks show as a toast and leave the picker open


## [2.2.0] - 2026-09-25

### New Features
- Alt+J previews the whole row as JSON in the bottom dock, and follows the cursor from row to row. Also on the cell right-click menu
- JSON colours are selectable in Settings → Appearance: Auto, Vivid, Ocean, Solarized, GitHub and Monochrome

### Bug Fixes
- Opening an array cell no longer takes the view down with "This view hit an error"
- Scrolling no longer tears, leaving a band of repeated rows above a seam
- A cell holding multi-line text reads on one line instead of showing gaps where the line breaks were
- Control characters show what they are instead of an empty box
- The switch keeps its thumb inside its track at every zoom level

### Changes
- Array columns read as JSON, the same form the editor and the jsonb column beside them already used
- The array editor reads as one list instead of a stack of separate fields, and its row controls stay put instead of appearing under the pointer
- The expanded row and its JSON tree sit on one type scale, and a key's colon sits against the key
- Settings descriptions are one line each instead of a paragraph
- Opening a schema with hundreds of tables costs one icon-settings subscription instead of one per icon


## [2.1.3] - 2026-09-25

### New Features
- Space previews the focused cell, Shift+Space opens it with the caret already in the editor
- Line numbers in the cell editor toggle with Alt+L
- Stroke's mark and name show while the app starts

### Bug Fixes
- Soft wrap no longer freezes the app on a cell held on one very long line
- Space no longer types a space into the cell it was meant to preview
- Escape from the cell editor puts focus back on the cell it came from
- The window no longer starts on a black screen
- Confirm dialogs show which button is focused
- Related rows follow the cursor sideways across foreign key columns
- Related rows draw in the table style you selected
- The Load button on a cell updates the preview below it
- A value past the size cap opens in the preview instead of a message about it
- Ctrl+F focuses the search box in Find in database
- The cell editor bar keeps its buttons on screen at any width

### Changes
- Filter, sort, columns and reset move to Alt+A, Alt+S, Alt+C and Alt+R
- Alt+A opens the filter on the column the cursor is already on
- Find in database searches as you type, and Escape clears it
- Soft wrap and line numbers are remembered from one cell to the next
- Opening and scrolling related rows is faster


## [2.1.2] - 2026-09-25

### Bug Fixes
- The update dialog offers a Retry button when a check or a download fails, instead of leaving the error on screen with nothing to do about it.
- A failed check no longer says "Checking for updates…" in its header while showing the error underneath, and a long endpoint URL wraps inside the dialog rather than running past its edge.

### Changes
- Update checks give up after 15 seconds. Without a bound they inherited the HTTP stack's default and could sit on "Checking for updates…" long after the request was dead.
- Downloading an update no longer redraws the dialog on every network chunk. Progress is published once a frame, which takes the redraw count for a 100MB build from thousands down to about one per 16ms.


## [2.1.1] - 2026-09-25

### Bug Fixes
- Closing the window quits Stroke again. 2.1.0 dropped the tray and the hide-on-close handler that went with it, which let close run through to `destroy` for the first time. `destroy` was never in the capability file, so the ACL refused it and the window stayed put. With no tray and no window decorations, that left no way to quit the app at all.
- Second windows (`Ctrl/Cmd+Shift+N`) respond to their title bar again. The capability only covered the window labelled `main`, so every control on a second window (close, minimize, maximize, and dragging the bar) was denied.


## [2.1.0] - 2026-09-24

### New Features
- Cell values open in a code editor with line numbers, folding, and find and replace
- ER diagram: Ctrl+scroll zooms, scroll pans, Shift+scroll pans sideways
- Build release installers locally with `npm run tauri:build:mac` and `npm run tauri:build:win`

### Bug Fixes
- Window no longer flickers on startup
- A quick reconnect on startup opens straight onto your tables
- Closing the window quits Stroke instead of leaving it running in the tray
- No clipboard permission prompt on Windows when opening the connect dialog
- Grid no longer flickers when switching tables or opening related rows
- Related rows stay on screen while the next lookup loads
- Hide all in the columns menu stays available after relationship columns load
- Shift+Tab moves up the sidebar, database and connection lists on Linux
- Clicking a saved connection no longer leaves a focus outline on it
- License activation no longer stutters or drops confetti over the app

### Changes
- The new tab no longer repeats connection details already shown in the title and status bar


## [2.0.0] - 2026-09-22

### New Features

#### Editing
- **Stage as many new rows as you want.** Add and Duplicate append, so pressing either three times gives three rows stacked under the header, each editable, each with its own tick and its own discard. The first row's tick inserts the batch, as do Apply and ⌘↵, under one confirm - three inserts is one list of three statements, not three dialogs. A row that fails stays staged with everything still typed into it. Right-click a staged cell to generate a UUID or CUID, drop in now/today, copy, paste, clear back to the column's default, or fill that column in every staged row. `Alt+N` stages a row, `Alt+D` copies the focused row into one, `Mod+Escape` discards them all, and the band belongs to the table it was opened on rather than following you between tabs.
- **Duplicate no longer writes on the click.** It fills the insert draft instead, so nothing reaches the database until it is submitted, and the key and any generated column take their defaults rather than the source row's values.
- **Import rows from a file.** CSV, TSV and JSON, mapped to the table's columns before anything is written.
- **Run SQL inside an explicit transaction.** The console can open one, run against it and commit or roll back, instead of every statement standing alone.
- **Generate migration SQL from a schema diff.** Take a snapshot, change the schema, and read back the statements that would take one to the other.
- **A cell transform edits the cell.** It used to compute a value and put it on the clipboard, which is one step short of the thing you wanted.

#### The cell dock
- **The cell editor lives under the grid.** A docked panel rather than a floating box over the rows you are trying to read, with the other panels matched to it, Escape to close, and Find inside the value.
- **A JSON cell opens as a tree.** No Monaco, so it costs nothing until you ask for it, and it reads as structure rather than as one long line.

#### Connections
- **The connection screen is rebuilt.** One tab stop with arrows walking the list, Enter to open and ⌘↵ to connect, a plain Connect button, database names instead of connection strings, and provider panels that no longer hang while they load.
- **Every provider gets the same searchable database dropdown.** Neon asks for the scope that can read a connection URI, a paused Supabase project says so and falls back to its direct host, and saving a connection that points at a database already on file asks first.
- **The paste bar fills itself from the clipboard.** On every window focus, so a URL copied from a dashboard is already waiting when you switch back.

#### Interface
- **Four new themes**, a menu bar in the title bar, and a second window from the same session.
- **Every shortcut draws as keycaps.** One menu recipe across the app, so a chord looks the same wherever it is shown.
- **Per-tab structure view**, settings for the row-number gutter, the menu bar and grid appearance, and a sidebar that folds Pinned into Tables, keeps its counts in the section headers and stops the list jumping.
- **Find and replace moved into the sidebar** and gained an Any column option, the editor's find chords (`Alt+C`, `Alt+R`, `Alt+W`) and `Alt+X` to clear.
- **The app says which version it just updated to** and links that version's changelog.

### Bug Fixes

#### The data grid
- **The staged rows sit on their columns at any scroll offset.** The insert band worked out its own geometry instead of reading the canvas's: it summed the gutters by hand and left the row-number one out, and its horizontal pin was a sticky box whose behaviour depends on how wide it is, so how well it held varied with the table. Dragging a wide table sideways slid the band against the grid under it. Every x and width now comes from the same `geom` the canvas draws from, and the band shares the canvas's sticky anchor, so the two measure the viewport's left edge from one box. Tab inside the band scrolls the column into view, which a pinned band cannot ask the browser to do for it.
- **A field inside the grid gets the keys a field gets.** The grid's undo/redo/copy listener is bound on `window`, so it ran before every other capture handler in the app - including the one that gives inputs their undo stack and their word deletion. ⌘Z with the caret in a staged row undid a cell edit somewhere else in the table and took the field's own ⌘Z with it. It now stands down for anything you type into, and Alt+Backspace deletes a word on Linux and Windows as well as macOS, which also stops the global "discard staged changes" binding from throwing the row away mid-sentence.
- **A large value loads into the cell it belongs to.** The in-cell load stopped at 1MB on the grounds that a cell's value is what the canvas formats and the search walks - but the canvas cuts the drawn text to 400 characters and caches it, the highlighter matches that same cut string, and the row search runs in SQL. A 1.2MB resume was being refused for a cost nobody was paying. The ceiling is now the dock's 8MB, and past 1MB a JSON value stays the text it arrived as rather than being parsed and re-serialized to show forty characters of itself.
- **The search highlight follows how the search matched.** It ran `indexOf` over a lowercased copy of the text, so with match case on it highlighted `Aarav` for a search that returned nothing containing it, and a regex search marked the pattern's literal characters.
- **`F5` refreshes the table, not the app.** It was bound to nothing, so the webview reloaded the document - tearing down the session, redialling the connection and re-reading the catalog.
- **The Add button names its shortcut.** Its tooltip read "Insert row (Add)", which is the label, not a chord. It says `Alt+N` now, and `Alt+N` works from inside the staged band too.
- **A timestamp cell keeps the instant it was edited at**, and Postgres `oid`, `xid`, `xid8` and `pg_lsn` decode instead of showing their bytes.
- **An expanded row stops swallowing clicks on the header**, the related-rows panel can be selected and copied, and the structure grid no longer draws a field inside every cell.
- **The database list stops blanking on every switch**, and switching database lands on Tables.

#### Connections and queries
- **A dropped pool reads as a dropped pool.** `attempted to acquire a connection on a closed pool` matched none of the connectivity patterns, so it read as a SQL error: no reconnect fired, and the banner sat there. Failed table loads now state themselves where the grid would be, with Reconnect for a dropped pool and Retry for everything else.
- **Disconnect survives a restart.** Auto-reconnect read the last-connection id as "resume this", so quitting while disconnected came back connected.
- **A dead pooled connection no longer stalls the first query after an idle spell**, and a Postgres backup no longer writes NULL where the data was.
- **SQL parameter values are escaped by the target engine's rules**, not by one engine's rules everywhere.

#### Keyboard and focus
- **The focus ring was invisible app-wide.** Tailwind v4 compiles every `outline-*` utility to `outline-style: var(--tw-outline-style)`, and `outline-none` sets that variable to `none` on the element - so the `outline-none` + `focus-visible:outline-2` pattern this app uses in about forty places applied a 2px-wide outline whose style was still `none`.
- Keyboard reaches the rest of the app: the connection rail is one tab stop with arrows walking it, relationship columns join the cell cursor (Enter previews, `⇧↵` opens in a new tab, `⌘↵` in place), `Alt+F` / `Alt+E` filter by or exclude the focused value, and the hand-rolled dialogs trap focus, restore it and inert the page behind them.
- **Tab cycles correctly on every platform, in both directions**, a chord that wraps keeps its last key instead of losing it, and close-all-tabs has a chord that can actually fire.

#### Elsewhere
- Every numeric server setting applies, the JSONPath union selector answers correctly, Instance Insights tables unstick, a Data Diff picker says why it is empty instead of going dead, an AI turn that finishes with no message no longer hangs, and the fixable dependency advisories are patched.

### Changes
- **The repo moved to the `stroke-app` organization.** Downloads, the updater manifest and the Scoop bucket live at `github.com/stroke-app/stroke`, and Homebrew is now `brew install --cask stroke-app/tap/stroke`. Builds shipped before this release reach the new location through GitHub's redirect, so nothing installed needs reinstalling.
- **Apply and Reset are one control**, and staged changes can be read as SQL without running them.
- **Every dialect runs in Docker against the same schema**, so what the app claims each one supports is what was tested rather than what was assumed. The SQL Server handshake says only what was verified.

### Performance

#### Tables with very large values
- **A table of half-megabyte cells opens in about a second instead of eleven.** A `jsonb` column holding an uploaded file averaged 601KB a row, and `SELECT *` over a page moved roughly 100MB of it out of TOAST, through sqlx, across the IPC bridge and into a webview parse, for a grid that can draw forty characters of it. Wide columns are now fetched as their size: the page asks for `pg_column_size`, which reads the varlena header and touches no TOAST chunk, and the value itself loads per cell on demand. Which columns are wide is measured rather than assumed, and deliberately not taken from `pg_stats.avg_width` - ANALYZE measures the datum in the tuple, so a column whose values all live in TOAST reports the width of the 18-byte pointer; the column above measures 601KB and `pg_stats` says 18. One sampling query settles it in 0.67ms over 37 buffers, cached per table, and skipped entirely unless the table has TOAST worth searching. The size check also runs against the value's text length, not its compressed size: these payloads compress 2x or better, so rows comfortably under the cap on disk were still shipping megabytes each. Settings → Load large values on demand turns the whole thing off and restores the old behaviour.
- **A capped cell says what it holds and loads on request.** The grid draws `361 KB jsonb` with a download button beside the copy one; it turns into a spinner and fills that cell, and only that cell. The dock's own Load button fetches up to 8MB - a `<textarea>` handed more renders nothing at all in this webview - and past 2MB the panel stops doing whole-value work: the JSON tree waits until asked for, Find stops walking the string twice per keystroke, the highlight layer stands down, and wrapping is off, because one 18MB line laid out across the pane is the most expensive thing that panel can be asked to do. Values that large are read through a paged window rather than a textarea, editable when the whole value is present and read-only when it is not, since staging a value that was cut at the ceiling would write what loaded over what did not.

#### Scrolling and rendering
- **The webview renders at the display's real refresh rate**, and a high-refresh Linux display can opt into the GPU compositor.
- **Scroll lag, resize judder and grid zoom fonts are fixed.** The eased scroll stays fractional and rounds its offset every frame rather than only the last one, the per-cell path is off store reads, and the grid counts the rows on screen instead of a model of them.
- **Telemetry no longer fires on dev runs** and tears itself down on hot update.


## [1.24.0] - 2026-08-31

### Bug Fixes
- **Restoring a session no longer fires one foreign-key query per tab at once.** The reverse foreign-key loader checked its cache before awaiting but only filled it after, so every restored tab missed and issued its own query. Measured against D1, where each call is several HTTP round trips to Cloudflare, that was nine concurrent calls totalling 37.7s of backend time, and everything the user was actually waiting for queued behind them. In-flight requests are now shared, and a generation counter discards a reply that arrives after a connection change instead of writing another database's foreign keys into the cache under the same key.
- **Catalog replies from a previous connection are discarded.** Switching connections while a catalog request was in flight could let the old database's answer land in the new connection's state.

### Changes
- Serialisation crates build optimised in the dev profile. Every command result is serialised to JSON for the webview and a browse is megabytes of it, so an unoptimised build spent most of a row fetch inside serialisation rather than in the database. These are dependencies, so it costs nothing on incremental rebuilds.
- `canvas-confetti` is declared to the dev server's dependency pre-bundler. It is only reachable through a lazy import on the license pages, so the initial scan missed it and discovering it mid-session rehashed every pre-bundled chunk, after which the running app 404'd on the paths it already held.

### Performance

#### The data grid
- **Text-heavy tables hold 60fps.** The grid was recomputing the identical truncation for every visible cell on every frame: 113 `measureText` calls shaping 16,014 characters through the text engine, 3.08ms of an 11.7ms draw, to arrive at the same answer as the frame before. Truncation is now memoised by font, column width and string. On a 10k-row table of embeddings and prose that is 113 to 31 calls per frame, 16,014 to 1,521 characters, 3.08ms to 0.46ms, and a draw of 11.7ms to 8.7ms. Frame times went from p50 18ms / p95 22ms / max 30ms to p50 17ms / p95 17ms / max 20ms, so 1-3 of 118 frames now miss the 16.6ms budget instead of most of them.
- **Per-column work happens once per column, not once per cell.** Whether a column is boolean, an array, a vector, a geometry or right-aligned is a pure function of its declared type, but each test was re-running per cell per frame: four regexes and about six throwaway strings each, roughly 40,000 regex executions a second while scrolling. They are computed once and cached against the column.
- **Drawing a cell no longer walks the reactive graph.** `drawCell` read about 20 values straight off the reactive graph per cell, and a derived read is not a property access, it re-checks against every dependency's write version. A 150-cell viewport paid roughly 3,000 graph reads a frame for values that cannot change mid-frame. They are snapshotted once per frame instead. The row loop also stopped re-entering the geometry derived once per column per row, and the pinned-column pass is skipped entirely when nothing is pinned rather than scanning every column of every row.
- **A scroll repaints the strip it uncovered, not the whole grid.** When a frame differs from the one before it only by a vertical scroll, the still-valid band is moved with a single image copy and only the newly exposed rows are painted. A one-row step paints one row instead of every visible one. The path is gated hard: no animating skeletons, no insert draft, no expanded rows, integer offsets landing on whole device pixels, and any content, hover, focus, selection, theme or geometry change forces a full repaint through the existing draw scheduler. The copy runs with the canvas transform reset so it is an exact 1:1 move of device pixels and cannot resample. Scrolling to a stop forces a full repaint, so nothing this path could produce can persist in a resting grid. The arithmetic is verified across 2,280 combinations of device pixel ratio, header height, viewport height and scroll delta: every body pixel is either copied from exactly the right source row or falls inside the repainted strip.
- **Changing pages reuses the row count.** Paging re-counted the table each time, which on a large table is the slowest part of the request.

#### Memory
- **The app gives memory back when you move on from a big table.** Opening one 1M-row table took the process from a 45MB/48MB idle baseline to 587MB Rust and 891MB WebKit, and closing every tab released none of it. On the Rust side `smaps` showed the cause: four roughly 50MB allocator arenas, one per worker thread, retained for the rest of the session. Switching the global allocator to mimalloc takes retained growth over the same open-scroll-seek workload from **527MB to 27MB**.

#### Startup and bundle
- **The editor bundle carries the six languages the app has, not eighty.** `monaco-editor`'s default entry ships 80-odd basic languages plus the CSS, HTML, JSON and TypeScript services, each service pulling its own worker. The app displays six languages: SQL, JSON, JavaScript, TypeScript, Rust and Markdown. The entry is now composed by hand from the same pieces the full one uses. Over the built assets that is **34,821,807 to 32,585,794 bytes, -2.24MB (-6.4%)** and 81 chunks gone, `css.worker` (1.05MB) and `html.worker` (719KB) among them, both of which were being emitted for grammars nothing in the app can open. The TypeScript worker stays, because the ORM runner really does use it for IntelliSense.
- **The startup prefetch only warms pages this connection can open.** A warmed chunk is never freed, and on a Redis connection every relational page is unreachable UI, the SQL editor and its editor bundle included. About 4.3MB of the 6.01MB warm set was being pinned for tabs that cannot exist. The list is now gated per entry on the same engine flags the tab affordances use, waits for a connection before it runs, and warms the Redis keyspace page only on Redis, where it goes first. The rest of the tail stays warm deliberately: measured against the static import graph, 22 pages cost 0.66MB between them, so trimming them buys almost nothing and would regress first-open latency on all of them.
- **The wheel path stopped querying the style system.** Smooth scrolling resolved the reduced-motion media query on every wheel tick, about 5us of style resolution each time, on the one path that must not touch the style system. It is resolved once and kept current by the query itself.


## [1.23.0] - 2026-08-26

### New Features

#### Security
- **Lock Stroke behind a PIN.** Settings → General → Security sets a four-digit PIN. With one set, Stroke opens onto a lock screen and the app is not mounted behind it: no connection, no license check, no auto-reconnect until the PIN is right. The PIN is never stored, only a PBKDF2-HMAC-SHA256 digest over a random salt, kept in the OS keychain next to the saved credentials, so the switch that turns the lock on cannot be flipped from a devtools console the way a localStorage flag can. Wrong entries earn a growing delay that tops out at three seconds, which is nothing after a typo and fatal to a script. There is no recovery code by design; the lock screen says exactly which keychain entry to delete if you are locked out.
- **The PIN also guards connecting.** "Ask when connecting" confirms the PIN before opening or reconnecting to a database, whether that comes from the connection dialog or from switching connections in the status bar. The reconnect right after you unlock the app is exempt, and the silent background heal that repairs a dropped pool never prompts. Auto-lock puts the screen back up after 1, 5, 15 or 60 minutes of inactivity, over a session that is kept intact: open tabs, queries and scroll position are all still there when you come back, and the keys typed at the lock screen never reach the app underneath it. `Lock Stroke` in the command palette locks on demand.

#### Connections
- **Every password field has an eye.** Database passwords, the libSQL auth token, the Redis password and the Cloudflare D1 API token can all be revealed while you type them. A masked field with no way to check what is in it is how one wrong character survives three failed connects.

#### ER diagram
- **Pick the tables the diagram shows.** A filter beside the search box lists every table in the schema with its foreign key count; tick the ones worth seeing and the diagram redraws to exactly those, with every relationship among them. The first tick means "just this one" rather than starting from all 900 selected and asking for 899 clicks. `Linked` narrows to the tables that have a foreign key at all, `+ Related` grows the selection by one hop, `All` clears it, and `only` on any row jumps straight to that table and what it links to. An explicit selection beats the scope buttons and the linked-only toggle: it is the one signal that came from someone naming tables, so nothing quietly adds to it. The filter belongs to the schema and the session, not to your saved settings.
- **Isolate or hide a table from its own card.** Selecting a card opens the inspector; the crosshair there scopes the diagram to that table and its neighbours, the eye takes it off the diagram. Hiding is disabled for the table a per-table diagram belongs to, which would otherwise come straight back.

#### Sidebar
- **Manage databases, not just switch between them.** Right-click any row in the sidebar's Databases section for the server-level operations: rename, duplicate, drop, close other sessions, copy the name, and a Database info panel with owner, encoding, collation, size on disk and how many sessions are connected. A `+` in the section header creates one. PostgreSQL and MySQL, with each item enabled only where the engine can actually do it: MySQL has no `RENAME DATABASE` and no single-statement copy, and Postgres cannot rename, copy or drop the database the session is attached to, so those items say why rather than failing at the server. Every one of them shows the exact statement before it runs, dropping asks for the name typed back, and the drop dialog offers `WITH (FORCE)` since a single idle connection is enough to make Postgres refuse.

#### Extensions
- **Plugins can now come from a folder.** Stroke loads third-party formatters from `<app data>/plugins/<id>/`: a `manifest.json` and one JavaScript file. Each one runs inside its own Web Worker with no DOM, no app state, no Tauri bridge, and with `fetch`, `XMLHttpRequest`, `WebSocket`, `EventSource`, `Worker`, `importScripts`, `indexedDB`, `caches`, `BroadcastChannel` and both storages removed before its code is evaluated, so a generation 1 plugin has no route to the network at all. Everything it returns passes a whitelist on the way to the grid: only the render-directive fields the app draws survive, colours have to be hex or `rgb()`, links have to be `http`, `https`, `mailto` or `tel`, strings are capped and stripped of control characters. Install from the Extensions panel with `+`, turn it on, and Reload re-reads the file from disk, which is the whole development loop: no build step, no bundler, no restart. A plugin that times out or throws three times is switched off with the reason shown under its row, because a formatter must never be able to take the grid down. Manifests are validated in Rust before any code is read: the id has to be a folder-safe slug matching its folder, the entry has to be a bare `.js` filename, and a folder that fails is listed as Broken with the reason rather than silently ignored. `docs/PLUGIN_API.md` has the contract, `types/stroke-plugin.d.ts` the types, and `examples/plugins/traffic-light/` a working plugin in forty lines.

### Bug Fixes

#### The data grid
- **A row fetch now belongs to the tab that started it.** Loads wrote into shell-level state guarded by a single global token, so switching tabs mid-fetch dropped a million-row result into whichever table happened to be on screen, and coming back to the tab you left restarted its work. Every load is owned by its tab: results land in that tab's state, the visible grid is only touched while that tab is in front, and a load left running finishes into the tab that asked for it.
- **Opening several large tables at once no longer locks the app.** A background tab was prefetching its full page size, so three tables opened on a large limit fired three enormous requests at the same time. A limit past the windowing bar waits for the tab to be activated.
- **The skeleton no longer looks like the layout shifted when rows arrive.** Placeholder bars sat hard left at a flat half-width in every column, so the moment real rows landed every right-aligned value jumped to the other side of its cell. Bars now take the alignment their column will use, vary in width like text of differing length, sit on the band the glyphs will occupy, and shimmer while the window is in flight.
- **Column headers line up with each other and with their own values.** The red `NOT NULL` asterisk led the column name, which indented the name by its own width on required columns only - so those headers sat a few pixels right of every other header. It now trails the name.
- **An expanded JSON row no longer paints over the column header.** The panel has to sit above the canvas, which paints an opaque background, so scrolling its row up behind the sticky header drew the panel over the header. It is clipped to the band below it.
- **Alternating row shading is visible.** It was painted at 7% of a muted tone, which against the panel background in a dark theme is nothing at all - the Striped and Dots presets looked like they did nothing either.
- **A window whose fetch failed no longer shimmers forever.** A failed window was only retried by the next visible-range emit, so stopping the scroll over it left those rows as skeletons with nothing coming. Each window now retries on its own up to three times before it gives up, and giving up is visible and retryable rather than silent.
- **One slow page no longer turns read-ahead off for the rest of the view.** A single measurement past the slow threshold latched the fetcher down to one window in flight - and one deep offset page on a big table crosses that threshold on its own merits, so the tables that need read-ahead most were the ones that lost it. The decision reads a rolling mean now.
- **A reload that keeps your scroll position refills the rows under it.** Reloading in place hands the grid a fresh rows array holding nothing but its probe, but the viewport had not moved, so the range it re-emitted matched the one the shell had already heard and was dropped as a duplicate - leaving every row under the viewport a skeleton until the user happened to scroll.
- **The skeleton shimmer stopped blinking once a period.** The sweep restarted with its highlight band still half on screen, so the gradient vanished mid-grid on the beat. It now travels a full band clear of both edges, and the whole frame's bars share one path and one gradient instead of a fill each - per-bar fills were the most expensive thing on screen in exactly the state that has to feel smooth.
- **Deep exports chunk by weight.** Fifty thousand rows of a table with an embedding column is most of a gigabyte in a single response.

#### SQL editor
- **Two editor tabs can run at once, and Stop hits the right query.** Cancellation was keyed by a single slot, so the second run overwrote the first's handle and Stop cancelled whichever query registered last rather than the one whose button was pressed. Each run carries its own id.
- **A query that finishes while you are on another tab no longer leaves the editor looking busy.** The shared loading flag was only cleared when the finishing run's own tab was still in front, so the next snapshot of any editor tab could capture that stale state as its own.

#### Settings
- **The Settings pane stopped being slow to open.** The SQL formatting section mounted nine popover selects - each a floating layer with a focus trap and a search box - merely to show the Database tab. Those are inline choice controls now, and the section starts collapsed; search still expands it. Formatter options are also read through a validator, so settings written by an older version cannot throw the whole pane into the error boundary.

#### ER diagram
- **A line connects to the near edge of a card, not always to its side.** A port on a card's left or right sits on the row the relationship is about, which is why it is the one worth having, so it is still what you get whenever two cards are separated horizontally at all. When they overlap in x there is no side to leave from, and picking one anyway sent the line out of the card, back across its own width and in again. A quarter of the relationships in a folded schema are in exactly that position. Those take the near horizontal edge instead, bottom to top or top to bottom. Measured on a stacked pair: 748px over four corners becomes 240px over two.
- **Lines that reference one column land on it, as one line.** Lines converging on the same column were fanned apart so they would not all arrive on the same pixel. Spread across the card that read as a lie - ten tables referencing `users.id` arrived beside `password`, `role` and `cash_balance` - and confining the spread to the row's own band only shrank the lie into a comb of elbows on the card's edge. It is the same idea as the anti-overlap penalty, and wrong for the same reason: ten lines that mean one thing should look like one thing. Ports are not spread at all now. Every line lands on the column it references, the router brings them in along a shared trunk, and one line arrives. Which table you are looking at is what selecting it answers.
- **Dropping a card no longer rebuilds every line in front of you.** Committing a drag changed the layout signature, which threw the whole route cache away and played the routing run out again from nothing, in slices, while you watched. Only the cards that actually moved invalidate anything now: a line between two cards that stayed put is still the line it was, unless a mover is now lying across it. That last test is per segment rather than per bounding box, which matters more than it sounds - a line routed around a card has a box covering that card, so answering from the box threw away every route near anything that moved. Moving one card of 135 re-routes 8 lines instead of 132.
- **Panning and zooming a large diagram is smooth again.** Three things were on the per-frame path that had no business being there. The nodes, the edges, the routing hints and the table metadata were all deep-reactive state, so every `n.position.x` and every `columns[i].name` the canvas read went through a proxy trap - and the canvas reads those for every card and every row of every frame, with a proxy allocated per column object the first time it is touched. They are raw now; nothing was ever mutated in place, so no reactivity is lost. The viewport cull recomputed each line's bounding box from all of its points every frame, for a number that only changes when the line is re-routed; it is memoised on the polyline itself. And the port geometry for every edge was recalculated on every frame even when the route was already cached and the ports were about to be thrown away. Curves are also traced as straight segments below the row LOD, where a corner radius is two or three screen pixels.
- **The lines follow the layout instead of fighting it.** Every relationship was routed by A* on a grid built after the cards were placed, which is the wrong order: by then the space between them is already taken, so on a wide schema most lines ran out of budget and fell back to a plain elbow drawn straight across whatever was in the way. The result was a circuit board of overlapping horizontal runs with lines through cards. Dagre already routes every edge while it places the cards, through dummy nodes it reserved space for, so those points are the one line guaranteed to miss every card - and they cost nothing, because the layout has already paid for them. They are what gets drawn now, as smooth curves rather than right angles, because a curve keeps its identity across a busy canvas where dozens of parallel elbows do not. Measured on a 239-table schema: **zero lines crossing a card**, layout and lines together in **131 ms**. The SVG and PNG exports draw the same curves.
- **Two foreign keys between the same pair of tables are two lines again.** Dagre folds unnamed parallel edges into one, so they shared a corridor and a set of crow's feet. Every edge is named now, on a multigraph, and `edgesep` keeps their corridors apart.
- **A hub is no longer a starburst.** Every foreign key points at the same primary key, so a table referenced by twenty others had twenty lines converging on one pixel and nothing traceable out of it. Ends that share a port now fan out around it, ordered by where each line came from so the fan never crosses itself, and never spread wider than the card can hold.
- **Disconnected clusters are packed, not spread.** A schema is rarely one graph: it is a few clusters plus a long tail of small pairs. Laying them out together put every cluster in the same rank columns, so the diagram came out mostly empty space with the cards too small to read at fit-zoom. Each connected component is laid out on its own now and packed onto shelves of columns aimed at a landscape page, which is what Graphviz calls `pack`. Small components also lay out faster than one big one, Dagre being superlinear.
- **The rank gutter is no longer wider than a card.** `ranksep` was 320px at the default spacing, sized for an orthogonal router that needed a corridor of stacked lanes between every pair of ranks. Curves need far less: 190px, and the compact and spacious presets in proportion.
- **Panning stopped allocating.** Resolving which column a line attaches to built a template string per column per edge on every frame - tens of thousands of throwaway strings a frame on a schema-sized diagram. Handles resolve through a map built once per layout, the drawable lines are built once per layout rather than per frame, the minimap paints its cards into an offscreen canvas and only the viewport rectangle is redrawn as you pan, and the font and fill a card's rows share are set once instead of three times a row.
- **The per-table diagram's routing no longer freezes the window.** A table and its neighbours are packed by hand rather than ranked, so there are no corridors to follow and those lines are still routed by A*. That ran in one synchronous call inside a single animation frame: 400 tables measured at **5.4 seconds** with nothing painted and no input handled, which on a large schema is indistinguishable from a hang, and long enough for the webview to be killed as unresponsive. Routing now runs in slices of 8 ms per frame - lines appear immediately as plain elbows and sharpen into routed ones over the next few frames, with a pill saying how far along it is. A run that overruns 2.5 seconds in total stops and leaves the rest as elbows rather than burning frames on a graph with no clear corridors left. The last few completed runs are kept, so crossing the zoom threshold that changes where lines attach no longer re-routes from scratch.
- **Card separation is a sweep, not every pair.** The pass that guarantees no two cards overlap compared all of them against each other, sixty times over: **349 ms** on 1200 cards, on top of everything else a large diagram already pays. Cards are visited left to right now and each one only looks at its horizontal neighbours - the same 1200 cards take **86 ms**, and the invariant is unchanged.

#### Sidebar
- **Row counts fill in on a large schema.** Every count in a schema went out as one request and was applied only once all of them came back, so a 135-table production schema had to finish every `COUNT(*)` before a single number appeared - and a connection swapped or dropped part-way through threw the finished counts away with the rest. That is why the sidebar sat on blanks with one lone number in it. Counts go out in chunks of twelve in list order now, landing in waves down the list, and a chunk that fails costs only its own tables.

#### Data grid
- **Clearing a filter loads the rows again.** On "All", the fetch limit is the row count, and the count left over from the filtered view was being used to size the fetch that cleared it: filter a 752-row table down to one row, clear the filter, and the next query ran `LIMIT 1`. The grid drew a single row under a footer reading "1-752 of 752". A count now carries the search and filters it was taken under, and is only used as a limit for the view it actually describes; otherwise the fetch asks for the ceiling and the existing probe sizes the view from a fresh count.
- **An applied cell edit shows the new value.** Cell text is cached per row so the canvas is not re-formatting the same strings on every scroll frame, and the cache was dropped only when the whole `rows` array was replaced. Saving a cell does not do that: it swaps that one row and bumps a repaint counter, so the repaint ran and drew the cached pre-edit text straight back. The cache now checks the row it was built from, so an edit appears the moment it is applied instead of after a refresh. Reported in #82.

### Changes
- **The Advisor tells you what is wrong with your database.** A new tab runs a read-only lint pass over the connected database and reports what it finds, grouped by check and ranked worst-first: tables without row-level security, foreign keys with no index, indexes nothing has scanned, tables bloated with dead rows, tables whose rows are so wide that reading a page of them moves megabytes, and tables with no primary key. Every check is a catalog query - it never reads a row of your data, never writes, and depends on no extension being installed, so it is safe to point at production. Findings carry the SQL that would fix them, to copy and read before you run it; the Advisor never executes DDL. Severity is decided per finding rather than per check, so an unused 8 GB index and an unused 2 MB one do not shout equally loudly, and a check that cannot run - a role without permission on a catalog - says so instead of quietly counting as a pass. Export the report as CSV, JSON or Markdown. Postgres for now; other engines get an honest "no checks yet" rather than a clean bill of health they have not earned.
- **Large tables no longer arrive in one enormous response.** Opening a table on a large page size used to be a single request for every row in it - a million-row table meant roughly 130 MB of JSON decoded on the main thread, per tab, which froze the whole app for as long as it took. A large fetch now reads a 200-row probe, measures how heavy the rows actually are, and either loads the rest normally or streams the table in windows sized to keep each request near 2 MB. A 143,000-row table opens in **185 ms** where it used to sit there for tens of seconds. This applies to any large limit, not just "All rows" - page size 1M is an ordinary option in the toolbar and taking it literally was the whole problem.
- **Window size is measured in bytes, not rows.** What stalls the main thread is payload, and rows differ by three orders of magnitude: five thousand narrow rows are half a megabyte, five thousand rows of a table with an embedding column are ninety. Notably, `pg_stats` cannot see this - a TOASTed embedding is reported as the 18-byte pointer stored in the main tuple - so the measurement uses table size over row count and catches exactly the tables that hurt.
- **A windowed table keeps its rows when you switch away and back.** It used to re-run the fetch and the count every time, so returning to a big table meant waiting again. Its window bookkeeping and its resident rows are preserved, and the scroll position with them.
- **Every window of a view slices one ordering.** A windowed view is assembled from many separate `LIMIT/OFFSET` queries, and SQL guarantees nothing about the order of rows between two queries that do not ask for one - Postgres ships with `synchronize_seqscans = on`, so a second scan of the same table can legitimately begin at a different page. Assembled from unordered windows, one view could show a row twice, miss others, and change which row sat at a given position between fetches. Windowed views are now ordered by primary key (a user's sort keeps the lead, with the key appended as a tiebreaker), and a table with no key is never windowed - a single query is internally consistent, many unordered ones are not.
- **A far window is an index seek, not a walk over everything before it.** `LIMIT n OFFSET 995000` makes Postgres walk 995,000 index entries before it can return a row - **345 ms** on a million-row table, where the same page fetched by key (`WHERE id > $last ORDER BY id LIMIT n`) takes **1.5 ms**. Every window slices one total order, so the last row of the window before it is exactly the anchor that page needs, and a window next to one already resident is now read that way. The conditions are deliberately narrow, because being wrong here means missing rows rather than slow ones: Postgres, a single ordering column that is the whole primary key (a non-unique key would skip every row sharing the anchor's value), and a type the backend will accept. Anything else, or the first error on the seek path, and the view stays on offsets. The check the backend runs before each keyset page is memoised for ten seconds, so the fast path does not put a catalog round-trip in front of every window it just saved a scan on.
- **The grid says which rows it is loading.** Skeleton bars only say that rows are coming, never which ones - on a million-row table a jump can leave a screenful of them up for a second with nothing to read. A pill now names the row span being fetched, appearing only after a window has been in flight long enough to be worth mentioning, so a window that lands quickly does not flash it on the way past. When a window has exhausted its retries the pill says so and offers Retry.
- **Smooth scrolling, and a switch to turn it off.** The grid and the sidebar ease each wheel tick to its destination, which reads as smoother on mice that scroll in big jumps. Settings → Appearance → **Native scrolling** hands both back to the OS, momentum and all, with nothing of ours in front of the wheel. Off by default. Someone who has asked their system for reduced motion gets the jump, not the sweep.
- **Row spacing, and alternating row colours.** Settings → Appearance offers Compact, Standard and Relaxed row heights - compact fits about a third more rows on screen - and a shading toggle for every other row, independent of the grid-style preset.
- **Auto-refresh, per tab.** The caret beside Refresh sets an interval of 5 seconds to 5 minutes for the table you are looking at, so a table you are watching re-polls while one you are editing stays still. It skips a tick while a load is in flight or a cell editor is open, and refreshes in place rather than jumping you back to the top.
- **SQL formatting is configurable.** Settings → Database → SQL formatting exposes keyword, data type, function and identifier casing, tab width, tabs versus spaces, whether `AND`/`OR` lead or trail the line break, expression width and blank lines between statements. Identifier casing defaults to preserve, because a quoted identifier is case-sensitive and rewriting it can change what a statement means.
- **Executed queries can file themselves under Saved Queries.** Off by default; when on, a successful run is saved as well as logged, deduplicated by its SQL so re-running the statement you are iterating on adds one entry rather than fifty.
- **The relationship column's chip stops changing shape under the cursor.** It was a fully rounded capsule with no border at rest, so hovering conjured a lozenge out of what read as plain text, and a long table name inside a full-radius pill read as a balloon. The chip now keeps one shape and one radius that matches the rest of the app, and hover moves its surface and border rather than its geometry.
- **No more em dashes anywhere in the app.** Every dash in the interface, in the settings copy and in the messages the backend sends up is now ordinary punctuation.
- **The whole sidebar is windowed, not just the tables list.** Views, materialized views and the databases list rendered a capped 500 rows, which both instantiated 500 context menus and hid whatever came after row 500 without saying so. All four lists now render only what fits the viewport plus a buffer, off one shared piece of window maths measured from the real row stride.
- **Revisiting a schema no longer re-reads its catalog.** Indexes, enums, triggers and sequences were reloaded on the strength of a single "which schema was loaded last" string, so switching schema A to B and back cost all four round trips every time. Every catalog read (table lists, schema catalogs, incoming foreign keys) now shares one cache keyed connection-first, with a freshness window per kind and a single invalidation point that every refresh and every DDL path already goes through.
- **A JSONPath suggestion says whether its sample is representative.** The value preview beside a key under a wildcard was read from the first element alone, so it never changed, which read as "this value is the same in every row" without having checked. Keys are now measured across up to 200 elements: the type comes from the first non-null value (a null first row used to mislabel the whole key), the type reads `string | null` when the key is genuinely both, and the row says `same in all 1,204` or `37 distinct` next to the sample.

#### Filters
- **The caret lands in the value.** Opening the filter bar, from the toolbar or from ⌘F, now focuses the value of the row it seeds, as does "+ Add filter". Every control in a filter row also shares one border, one surface and one hover: column, condition and value each had their own pairing, so a clause that reads as one sentence was drawn as three unrelated boxes. The value field is capped rather than free-running, which stops it stretching into an empty box the width of the window.

#### Sidebar
- **A count that has not arrived draws nothing.** The placeholder mark on every uncounted row made a long table list read as a column of dashes, which says "empty" much louder than "counting". Counts also sit a size down from the table names now, so the names lead.

#### ER diagram
- **One line per pair of tables, not one per foreign key column.** A table carrying `created_by` and `updated_by`, both pointing at `users.id`, drew two relationships that leave the same card, cross the same corridors and land on the same row. They were drawn twice, routed twice, and told the reader one thing. Parallel foreign keys in the same direction now collapse into a single line carrying a count chip, so nothing is hidden and every column still shows its own `fk` badge in its row. Direction stays part of the identity: `a` referencing `b` and `b` referencing `a` are two facts and stay two lines. Cardinality merges the way a reader reads it, many if any of the keys can repeat, optional only if every one of them is nullable. Merging runs before the layout, so Dagre and the router each have fewer lines to work through, which is the difference between a large schema finishing its routing budget and running out of it.
- **A schema is no longer one tall strip.** A ranked layout puts every node of a rank in one column, and a schema with a hub puts most of the schema in one rank: a `users` table ninety others carry a foreign key to gave a second rank ninety cards tall, and a diagram around 1:10 that was unreadable at any zoom with every line in it running the full height. A rank taller than the page is folded into as many sub-columns as it needs now, in the order the layout chose, with the ranks after it shifted right - so the crossing work the layout did is kept and only the column it did it in changes. A diagram that is already a reasonable shape is left exactly as the layout left it, since folding one costs its vertical alignment for nothing. Folding does cost the layout's own edge corridors, because the space they were reserved in has moved, so a folded diagram routes its lines around the cards instead of following them - which it can now afford, being compact.
- **Every line is drawn as right angles.** The layout hands back a polyline per relationship, and joining its points with straight segments was the obvious reading and the wrong one: in a left-to-right layout those points sit at different heights, so every relationship came out as a long diagonal, and a canvas of crossing diagonals is unreadable however it is stroked. The same points are now walked one axis at a time - horizontal runs along the lane the layout reserved, the turn between two lanes taken midway between their points, which in a ranked layout is the gutter between two ranks. So every segment still lies in space the layout kept clear of cards, which is what stops a line vanishing under one, and the diagram reads as the right-angled connector an ER diagram is meant to have. Unrouted lines (direct mode, a graph past the routing cap, a card mid-drag) take the same shape on their own turn column, so there is one geometry on screen and in the SVG and PNG exports. Lines are a touch heavier and hold more of their contrast when zoomed out, since they are traceable now rather than noise.
- **Relationships share their corridors instead of each taking a lane.** The router charged a route for reusing a segment another one had taken, on the theory that two lines drawn on top of each other cannot be told apart. Measured on a 135-card schema with 164 relationships, that penalty was the worst thing in the diagram: every line detoured into a free lane rather than run alongside its neighbours, which took the average line from 3.7 corners to 10.6, put 247k pixels of line on a canvas that needs 70k, and made routing take 1.67s instead of 0.21s. What it produced was not separable lines, it was a mesh. Lines going the same way now run together as one trunk and split at the ends where their ports differ, which is what every tool that draws a whole schema does, and tracing one relationship is what selecting its table is for. The cliff is sharp - a penalty of 1 already doubles the ink - so the constant and its per-segment counters are gone rather than tuned. Detours got shorter too, 1.17x average down to 1.03x, and nothing crosses a card.
- **Cards are not empty rectangles any more.** Below the zoom where column text is legible the rows were skipped entirely, which is most of a schema-wide view: the diagram was a field of blank boxes joined by lines. The rows are drawn as bars at those zooms instead, in the same PK and FK colours and on the same rhythm, so a card still reads as a table. Lines other than the selected relationship also recede at that zoom, so a few hundred of them stop burying the cards they connect.
- **Tighter gutters.** Every spacing preset came down by roughly a fifth. The corridors were sized for an orthogonal router that no longer draws the lines.
- **A schema too wide to draw waits to be asked.** Past 240 tables the diagram no longer lays itself out on arrival - it says how many tables it found and offers the table filter or an explicit "Draw all". Laying out and drawing that many cards is seconds of work, and the result is a wall nobody reads.
- **A wide schema opens keys-only.** Above 120 tables, cards show their primary and foreign keys instead of every column, unless you have chosen otherwise. Card height is what a layout pass pays for, twice over: taller cards mean taller ranks mean a wider graph.


## [1.22.0] - 2026-08-09

### Bug Fixes

#### Instance Insights
- **The Config tab no longer stalls when it opens.** Postgres reports 350-odd settings and MySQL over 600, and every one of them was built into the page at once. Rows are now constructed a screenful at a time and the rest fill in during idle, so the tab opens immediately and ⌘F still finds every setting a moment later.
- **The session and lock tables stopped rebuilding their column list once per row.** A 200-session table allocated 200 throwaway arrays on every render of the page, and read each cell twice over.
- **The server payloads stopped being deep-proxied.** The settings list, session list and replication stats are replaced whole every refresh and only ever read, so wrapping every row and every field in a reactive proxy cost more than the render it fed.
- **`pg_stat_activity` and `pg_prepared_xacts` are bounded.** They had no `LIMIT` while the lock query beside them had one, so a busy server could hand back an unbounded result — and the session query calls `pg_blocking_pids` once per row.
- **Live charts update instead of rebuilding.** Every five-second refresh tore down all three charts and built them again from nothing; they now merge the new numbers while the shape of the data holds, and rebuild only when it actually changes.

#### OmniRoute
- **Installing it works on macOS and Linux.** It installed with `npm install -g`, which writes to a prefix owned by root on a default nodejs.org or distribution install — so it failed with a permissions error for exactly the people who followed Stroke's own advice to install Node from nodejs.org, and all the app could do was tell them to go run npm themselves. Stroke now keeps its own copy under its data directory, which needs no elevation anywhere and leaves your global npm prefix alone.
- **No more console window on Windows.** Checking for Node, installing the package and starting the gateway each went through `cmd.exe`, and a windowed app on Windows gets a visible console for any console program it starts. Every process Stroke runs — npm, Docker, ssh tunnels, the local port scan, the licence check — is now started without one. The gateway itself is launched directly through Node, so on Windows there is no console to suppress in the first place, and stopping it actually stops it: under `cmd.exe` the Node process outlived the shell that was killed.
- **The install shows progress.** It pulls roughly 1200 packages and takes minutes, and npm prints nothing at all until it finishes, so the panel sat on "resolving package…" long enough to look hung.
- **JSONPath suggestions read like an editor's.** Each row used to print the same identifier twice with a generic badge and no sign of which characters matched what you typed — because the widget was throwing away the type, the detail and the value preview the completion engine had already worked out, then rebuilding a worse version from what was left. Suggestions now show what kind of value is behind a key, highlight the part you typed, preview the value on the highlighted row, arm the top match so Enter takes it immediately, and state their own keys along the bottom. There were three copies of this list, one of them good; there is one now.

#### Keyboard
- **`/` focuses the search box for whatever you are looking at.** The app has around thirty search and filter fields and this works on all of them, including ones added later, because the key finds its target from what is on screen rather than being wired to each field. It follows you into a dialog, ignores hidden fields belonging to tabs you left open, and selects the existing text so typing replaces a stale query.
- **`?` no longer interrupts you mid-sentence.** It opens the shortcuts panel, but only checked for a plain text field — so typing a question mark in the AI chat or certain editor contexts popped the panel open instead.
- **⌘B toggles the sidebar again.** The binding went through a listener on the document in the bubble phase, so anything between the focused element and the document that stopped the event swallowed it first — which is every Monaco editor, the grid canvas, and the dialog overlays. It is now bound in the capture phase, ahead of all of them, and matches ⌘ on macOS and Ctrl elsewhere rather than accepting either (Ctrl+B still moves the caret in a text field on a Mac, as it should).
- **Shortcuts are written down in one place.** `src/lib/shortcuts.js` holds every binding as a single combo string; the macOS and Windows spellings are derived from it rather than typed out twice, which is what let the help dialog drift from what the app actually bound. Adding a shortcut now gets both platforms for free.

#### Connections
- **The saved-connections filter is always there once you have more than one.** It only appeared past five connections, so it materialised out of nowhere as the list grew and the people most likely to want it had never seen it. It also gained Enter to take the top match, arrow keys into the list, and a clear button big enough to hit.
- **Filtering no longer replays the list's entrance animation on every keystroke.** Rows carried a staggered rise of up to half a second, which is a nice first impression and reads as lag when it fires on each character typed. Both the connections rail and the database picker opposite it were affected.
- **The engine blurb is gone from the connection form header.** "Local file-based database" under a heading that says SQLite is describing a decision you have already made.

#### SQL editor
- **A running query survives leaving its tab.** Two Query Editor tabs share one editor component, and switching between them swapped its state out from under an in-flight query — the spinner stopped, the results pane showed the other tab's, and whatever came back landed in whichever tab happened to be in front. The query itself never stopped; only the app lost track of which tab had asked. A run now belongs to the tab that started it.
- **JSON has its colours back.** The viewer was rendering correctly-indented monochrome — keys, strings and numbers all the same shade. Every editor theme in the app was written for SQL and named none of the scopes JSON actually produces, so nearly every token fell through to the plain foreground. All 28 themes now colour it, with property names distinct from values so a page of rows can be scanned.
- **Wrap long JSON values, everywhere, from one switch.** An embedding or a document chunk ran off the right edge with no way to read the rest of it. There is now a Wrap button on every JSON view and a matching setting in Settings → General, all driving the same preference — flip it anywhere and every open view reflows. The row detail panel used to keep its own separate switch, so the same preference existed twice and applied to two of six views. Off by default: unwrapped keeps the structure scannable, and one embedding wrapped can bury every row around it.
- **The JSON view no longer freezes on a large result.** It built one object per row, flattened all of them to a single string, then parsed that string straight back into objects — three passes over the whole result and two copies of it in memory, with no limit. `SELECT *` over a million rows spent its time assembling text far too large to read. It now renders the first thousand rows, says so, and Export still carries everything.

#### Codegen
- **A SQL target, beside Prisma and Drizzle.** The schema as the statements that would rebuild it: enum types, `CREATE TABLE` with nullability, defaults and keys, the indexes, and the foreign keys as `ALTER TABLE` after every table exists — inline `REFERENCES` only works if the parent is created first, which no ordering can promise once the schema has a cycle. It covers every engine, because it is the engine's own language.
- **The whole database in one script.** A scope switch on the SQL target emits every schema at once, schema-qualified, so two schemas owning a `users` table produce two statements rather than a collision. Read only when you ask for it.
- **The Prisma/Drizzle switch is reachable from the keyboard.** It was two plain buttons — no tablist, no arrow keys, invisible to a screen reader — for the primary control on the page. Its pills were also 24px next to 28px buttons; they match now.

#### Interface
- **Dropdown menus were rebuilt to fit their own panel.** Item corners were rounder than the space they sat in, leaving a crescent of gap at each one; rows were small enough to mis-click; the highlight jumped from row to row with no transition, so running the pointer down a list strobed; and the selected value was marked only by a tick at the far right edge, away from the label you were reading. Fixed in the shared component, so every menu in the app gets it.

#### Ask AI
- **You can select and copy an answer.** The command palette is rendered outside the app's DOM root, so the rule that makes dialog text selectable never applied to it — and a quick answer you cannot quote is most of the point of asking.
- **The panel stops moving while you type.** It had a maximum height but no height, so the box grew as the answer streamed and as query results appeared, shifting everything already on screen out from under the cursor. A transcript wants a window that scrolls, not one that resizes.
- **It follows the answer to the end.** Auto-scroll measured the page the instant new text arrived, but the markdown renders a moment later — so it scrolled to a bottom that did not exist yet, fell further behind with each token, and gave up entirely once it had drifted far enough, which is exactly when there was most left to read. It now waits for the text to actually land, and scrolling up to re-read something no longer fights the stream.
- **Typing during an answer is smoother.** Every token re-ran a walk up the DOM asking the browser to recompute styles, for the whole length of the answer.

### Changes
- **The app starts in a fraction of the time it used to.** Stroke was compiling **6.8 MB** of JavaScript and CSS before it could paint a single pixel, and about 3.9 MB of that was the Monaco editor — loaded at boot whether or not you ever opened a SQL tab. The cause was one static import four components deep: the split-pane snapshot imported the JSON view, which imported the Monaco text view, which imported Monaco. Being in a separate chunk file does not make code lazy; only being unreachable from a plain `import` does. Monaco, Shiki and the confetti library are now off that path entirely, along with eight tab pages that were shipped to every user at boot despite being behind "only if opened" guards. **The startup payload is 2.6 MB, down 61%.** Every one of those chunks is still warmed during idle time, so opening a tab is no slower than before — the work simply no longer happens between launching the app and seeing it.
- **Required columns are marked in the grid header.** A `NOT NULL` column carries a red asterisk before its name, the same mark a required form field carries. The constraint used to be discoverable only by trying — clearing a cell and being told "Cannot set NULL" — which is late to find out.


## [1.21.0] - 2026-08-08

### New Features
#### Data
- **Geometry cells have a viewer, and it's a real map.** A PostGIS column used to render as a wall of EWKT in a grid cell and open a one-line text input for editing. It now shows the geometry type and SRID in the grid, and clicking through opens a map you can pan and zoom — because "where is that" is the question you opened the value for, and it has an answer at a scale you have to choose. Offline by default (the country outlines ship with the app); the tiled basemaps are one click away and named with their provider, since turning one on sends the location to a third party.
- **Inserting a row says what the database will do.** An identity column was labelled "Required" — backwards, since sending a value there overrides the sequence. No type string can identify one (a Postgres `serial` reports as `bigint`), so the catalog is asked directly, and each blank field now says `auto-increment`, `generated`, `default`, `NULL` or `Required`.
- **Enum and boolean fields on the insert row take typing.** Type to filter, arrows to move, Enter to pick — and leaving the field empty is a real, captioned choice rather than a blank you have to guess at.
- **Every insert field takes typing, including generated ones.** A generated column rendered as a static label, so importing a row that must keep its key, or backfilling a gap in a sequence, meant leaving the grid for raw SQL over one cell. It shows `auto-increment` as a placeholder now — blank still omits the column, so the sequence is untouched unless you overrule it. The timestamp field pairs an input with the calendar, and shows the column's own text, so an epoch column stays editable as digits.
- **Ollama Cloud models are pickable without pulling anything.** `/v1/models` lists only what's on disk, so a cloud model could never appear in the picker. Suggestions now come from Ollama's own registry — a model name written in the source is wrong within months — split into what fits on your machine and what runs on theirs.
- **Anonymous usage data**, off with one switch in Settings. It reports which features get used, the app version and the OS. It does not report queries, table or database names, connection details, or anything about the data you browse — events are names, not payloads.
- **Vectors show their standard deviation and value distribution.** The existing strip is indexed by dimension, which tells you where the spikes are but not whether the embedding is shaped right. The histogram reads roughly gaussian around zero for a healthy dense embedding; spikes and heavy tails are the signal that something is off.
- **The status bar says how many rows you have selected.**
#### Connections
- **Reconnecting lands on the schema you were last using** instead of resetting to `public`.
- **The window title says which database you are in.** It preferred the file path, so a local D1 connection showed its whole miniflare path.
- **Closing an edited connection form asks first** rather than throwing the edit away.
#### AI
- **The agent can read SQLite, D1 and libSQL schemas from the sidebar.** `describe_table`, `get_schema` and the schema cache all queried `information_schema`, which those engines don't have — every call failed and the agent worked blind.
- **Stopping generation actually stops the download.** Aborting closed the browser side only, so a local model server kept generating the whole completion into nothing. `Esc` now stops it too, as the button has always claimed.
### Bug Fixes
#### Data integrity
- **MySQL inserts read back the right row.** `LAST_INSERT_ID()` is connection-scoped, and it was being read on a separate pooled connection — so whenever background work was also using the pool, the re-fetch came back with `0` or another statement's id.
- **MySQL backups no longer corrupt decimals, dates and large ids.** `DECIMAL` was consumed by the integer decoder and written with its fraction dropped (`9.99` → `9`); `DATETIME`/`DATE`/`TIME` matched no decoder at all and exported as `NULL`; `BIGINT UNSIGNED` was rejected by the signed decoder.
- **Restoring a MySQL dump can no longer write into the wrong database.** The dump's per-schema `USE …;` directives are connection-scoped, and the statements depending on them could land on a different pooled connection.
- **A Postgres backup says what it couldn't export.** Every secondary-object query ended in `unwrap_or_default()`, so a permission error produced a dump silently missing its enums, sequences, foreign keys, views, functions or triggers.
- **A failed schema capture is no longer stored as an empty snapshot** — the Schema Timeline diffed it against a real one and reported every table as removed.
#### Storage
- **Deleting a connection deletes its data.** Its recents, charts, dashboards, diagrams, per-table preferences, SQL draft, query history, saved queries, conversations and schema snapshots all stayed behind forever. Enough deleted connections eventually exhausted the storage quota, at which point unrelated saves started failing.
- **An edit made just before switching connection is no longer lost** — or, worse, written under the new connection's key.
- **Saved column order is scoped to the connection.** Two databases with a `public.users` shared one order.
- **Switching database drops the cached table structure.** The caches are keyed by `schema.table` alone, so the old database's columns were served for same-named tables on the new one.
#### Connections & providers
- **Cloudflare and provider sign-in errors are visible.** A failed D1 listing left the picker showing "No D1 databases in this account" over a real error, and closing the dialog mid-authorize never released the callback port — so the next attempt hit "Callback port in use" until the five-minute timeout expired.
- A saved connection with no port no longer defaults to `5432` regardless of engine.
- The one-click fixes on a connection error now work on the ClickHouse, SQL Server and Redis forms, whose fields use prefixed ids.
- Live mode stops polling a connection you have switched away from, instead of erroring every second forever.
#### Interface
- The keyboard-shortcuts dialog opens with an empty search box, focused, so the `⌘F`/`Esc` keys it advertises work without clicking into it first.
- Leaving the search tab, or closing the command palette mid-question, stops the background queries it had running.
- Backup log lines emitted in the same millisecond no longer collide, and the log stops at 1000 lines.
- Chart export takes the panel's own canvas rather than the first one on the page.
- Plain views no longer show a row count — they have no entry in the row-statistics source, so it was always `0`.
- **AI errors say what the provider said, once.** A rate limit led with "check your plan and usage limits" while the sentence that mattered — "they reset at midnight UTC, or add your own API key" — sat behind a Details toggle next to a second copy of itself wrapped in JSON.
- **The AI model picker no longer spins forever.** It fetched Ollama's registry from the webview, which can never work — ollama.com sends no CORS header — and retried on failure, so the spinner never stopped. It goes through the backend now, which also makes it behave identically on macOS, Linux and Windows.
- **"No models installed" is no longer dressed as a crash.** Ollama running with nothing pulled rendered as a red failure card advising you to start a server that had just answered.
- **Installing OmniRoute no longer looks hung.** The backend had been streaming every npm line all along and nothing was listening, so a legitimate 30-second install showed a bare spinner.
- **OmniRoute finds your Node.** A `.app` is started by launchd with a minimal `PATH`, so Homebrew, nvm, fnm and Volta installs were invisible and the app told people who plainly had Node that they had none.
- **The table toolbar stops clipping when zoomed in.** The pager ran off the right edge past about 250% zoom because the search box could not shrink.
- **The status bar sits on one rhythm.** Three different control heights shared the row, so the spacing read as uneven however the gaps were set.
### Performance
- **Reading cells is substantially cheaper on every engine.** Decoding tried each type in order, and every mismatch makes the driver allocate a formatted error — up to 12 per cell on a Postgres text column, 9 on MySQL. Common types are now routed by name first.
- **Large results no longer double their peak memory.** Rows were collected from the driver and then mapped into JSON, so both copies were resident at once.
- **Deleting many rows is one statement per 100, not one per row** — against D1 and Turso that was one HTTPS round-trip each.
- **Paging, sorting and filtering skip the primary/foreign-key lookups.** They can't have changed since the table was opened, and they cost two serialized statements on SQLite and two full requests on D1/Turso, every fetch.
- **Finding what references a table is one query for D1 and libSQL**, not one round-trip per table in the database.
- **A background row count can't starve the pool.** It inherited the session's 10-minute statement timeout; it now gets 30 seconds and degrades to "unknown".
- **The grid stops painting each frame twice** while scrolling, and stops walking the columns scrolled off to the left once per visible row.
- **Infinite scroll keeps its rows out of the reactive proxy** — the grid indexes `rows[row][col]` per visible cell per frame, and each of those reads was going through a proxy trap.
- The AI agent fetches SQLite table info in parallel instead of one round-trip per table before it can answer.
- Also: the schema timeline caps its diff matrix, the data diff debounces its filter and yields while comparing, the Redis keyspace bounds its `TYPE` fan-out, query history finds the last statement with a cursor instead of loading the whole log, and the sidebar stops forcing layout on every keystroke in the app.
### Changes
- Release builds abort on panic instead of unwinding — every command returns a `Result`, so a panic is a bug rather than a recoverable error.
- The OmniRoute proxy is killed when the app quits; it used to outlive it.
- `AGENT.md` and `.sqlnb` notebooks are no longer tracked — a notebook stores its query results inline, so it can carry real rows.


## [1.20.0] - 2026-08-05

### New Features

#### AI agent
- **The agent reads real rows before it writes SQL** — a few rows from each table a question is actually about now go into its context alongside the column list. Types say a column is `text`; they don't say it holds `active` rather than `ACTIVE`, that a date is an epoch string rather than ISO, or that a nullable column is null for every row that matters. Queries written from the type alone got those wrong.
- **Web search** — the agent can search the web and read pages for what your database can't answer: an error code, the syntax of an unfamiliar function, current documentation. Off by default in Settings → Agent; everything else the agent does stays between the app and your database, and searching sends your question to a third party.
- **Show query cards** setting, for hiding the SQL the agent ran and the rows it returned. Failed queries stay visible either way.

#### Editor & grid
- **View DDL opens a read-only editor** instead of a full SQL console — no Run button, no query toolbar, no empty results pane around a statement you can't execute against the table it already describes. Word wrap on, DDL formatted, with Copy and Save.
- **Cell alignment** setting — Left, Numbers right, or Right. "Numbers right" lines digits up by place value so magnitudes are comparable down a column, and leaves prose on the left.
- **ERD is selectable as the default data view.** It was already a per-tab view; it just never reached the setting.

### Bug Fixes

#### Exports — every one of these silently did nothing
- **Database backup** was the worst: it reported "Backup file downloaded" every time, whether or not anything had been written.
- Also fixed: query result CSV/JSON, chart PNGs, AI chart and diagram exports, notebook cell CSV, and the ERD's PNG/SVG/Mermaid exports.
- **Exported diagrams are no longer blurry** — they were rasterized at 1× and scaled up, which resampled the 10px label text.
- **Exported diagrams follow your theme.** The sheet was a hardcoded dark palette, so a diagram exported from a light theme came back as somebody else's dark diagram.

#### AI agent
- **The agent no longer invents reasons for failures.** A wrong column name was being reported as "there is an access restriction preventing me from querying this table" — the harness had asked the model to explain a failure it had no information about, and hid the real error from you. Failed queries now show the SQL and the actual message.
- **Driver errors are readable.** A D1 error filled the screen with its whole HTTP envelope to say five words; the cause is shown, with the raw text one click away.
- A fresh install starts with a working model. The free-tier profile was written but never actually created, so the picker said "No model" until you configured one yourself.
- The result card no longer shouts — one neutral card per outcome instead of a red-on-red block for a typo the model fixes on its next turn. Embedded result tables are sized for a chat bubble.
- Conversation list: the rename and delete buttons no longer sit on top of the title and timestamp.

#### Connections
- **Cloudflare D1 sessions survive token expiry.** The access token was captured when you connected and never updated, so after a while every query answered `401 Unauthorized` until you reconnected. It now refreshes once and retries; if that fails you get the real error.
- **Opening the first table is faster** — the connection pool is filled during connect, where a progress indicator is already showing, instead of paying for four TLS handshakes at once on your first query.

#### Interface
- **Editor themes are consistent.** Opening any editor re-tinted every other editor in the app onto a stale palette — most visible in the JSON view, which sat a shade lighter than everything around it.
- Dropdown menus always show their search field instead of growing one past five items.
- JSON gutter line numbers are evenly padded rather than flush against the edge.

### Changes
- **The ER diagram loads in one request.** It was fetching every table's columns one table at a time — two catalog queries and a round trip each — so on a large schema most of the load time was latency, not work.

#### Navigation
- **Back / forward now works with more than a few tables open** — a tab whose rows had been evicted refetches when you return to it, and the restore was placing the cursor before those rows arrived. The grid won't focus a cell in a table it holds no rows for, so the jump was silently dropped and the tab opened at the top. Travel now waits for that fetch.
- **Holding `Alt+←` no longer crawls or overshoots** — a burst of presses walks the history and travels once, to wherever it lands, instead of activating (and possibly refetching) every tab it passes through. A newer press supersedes an older one outright rather than racing it for the cursor.
- Rows arriving mid-restore are no longer mistaken for a new jump, which used to wipe the forward branch.

#### Performance
- **First click into a prefetched table is no longer sticky** — background fetches kept their rows only inside tab state, which hands them back through Svelte's `$state` proxy. The grid indexes `rows[row][col]` per visible cell per frame, so every one of those reads went through a proxy trap. Background fetches now keep the raw array, as switching away from a tab already did.
- Switching tabs no longer rebuilds the tab array when nothing needs evicting — that write invalidated the tab strip, the panes and `tabsById` on every switch for no change at all.


## [1.19.0] - 2026-07-31

### New Features

#### Connections
- **Save without connecting** — the connection dialog has a Save button alongside Save & Connect, so you can store a connection's details without dialling it.

#### Navigation
- **Back / forward now restores your cursor, not just the tab** — like an editor's Go Back. Focus a cell, go somewhere else, press `Alt+←` and you land back on the same row and column, scrolled into view; `Alt+→` goes forward. Works across tabs and within a single table (jump far down a table and come straight back).
- **The mouse's back / forward buttons** drive the same two actions, alongside the arrows in the title bar.

### Bug Fixes

#### Connections
- **Fixed connections hanging or failing outright** — the reachability preflight could veto a connection that would have succeeded (observed vetoing a database that then handshook in 335ms). It now races the probe against the real connect: zero added latency, fails fast only when the host is *definitively* unreachable, and never overrides a working handshake. Addresses are probed concurrently (happy eyeballs) instead of walked one at a time, which is what made this far worse on Windows with unroutable IPv6.
- **Fixed retry amplification** — a definitively unreachable host was being retried as if it were a transient error, turning one dead host into ~17s of spinner. Unreachable errors now fail immediately; only genuinely transient ones retry, under a total time budget.
- **Fixed `pool timed out` errors during normal browsing** — sidebar row counts ran unbounded `COUNT(*)` queries that pinned most of the connection pool, starving interactive queries and leaving counts stuck on `…`. Counts are now time-bounded (Postgres via `SET LOCAL statement_timeout` inside a transaction, MySQL via `MAX_EXECUTION_TIME` plus a client-side backstop for MariaDB) and run below the pool's capacity.
- **Release builds now write logs** — logging was compiled out of release builds, so failures on user machines produced nothing to diagnose.

#### Window
- **The window no longer turns white while macOS asks for keychain access** — the *"Stroke wants to use your confidential information"* prompt ran on the main thread, stalling the event loop so the window could not repaint until the prompt was answered. Secret-store reads now happen off the main thread, the first read is deferred until after the app has painted, and the window's own surface matches the theme instead of defaulting to white.
- **No white flash on launch** — the window and webview had no background colour, so any frame before the UI painted showed white against the dark themes.

#### Navigation
- **`Alt+←` / `Alt+→` reach the history at all** — the grid claimed modified arrows for its own cell cursor, so the shortcut nudged the cursor sideways instead of navigating.
- **Positions inside a single table are recorded** — an entry was only kept once the cursor crossed twelve rows, so clicking between two nearby cells left nothing to go back to and back / forward looked like a cross-tab-only feature. Clicking a cell is aimed, so it now counts however short the move; arrow-key roaming still does not fill the history.
- **Back / forward now works for freshly opened tables** — opening a table from the sidebar bypassed the history entirely, so the back arrow did nothing.
- The focused column is kept in each tab's state, so switching tabs no longer loses which column you were on.

#### Data grid
- **Focused row highlight** — the row your cursor is on now carries a visible tint that runs edge to edge, across pinned columns and the row gutter, instead of stopping at the frozen edge.

#### Editing
- **Editing a date cell no longer crashes the view** — a `created_at`-style column that stores Unix millis opened the calendar picker, which then threw `null is not an object`. Committing an edit clears the cell while the picker is still mounted, and the picker's lazily-read props dereferenced the cleared value.
- **Editing a date no longer destroys the stored timestamp** — the calendar could not parse epoch numbers, so it showed *"Pick a date…"* for a row that plainly held a timestamp, and picking a date then wrote an ISO string into a column storing epoch millis. The picker now detects the stored format (epoch millis, epoch seconds, or ISO) and writes back in that same format, keeping the seconds and milliseconds it never displays.

#### Theming
- **Text colours are no longer dropped on the dark themes** — any element given a colour alongside a `text-ui-*` size lost the colour and inherited the ambient one. The "Add connection" button was the visible case: white text and icon on a white button, so it read as a blank slab.
- **Editors match the app's background** — the JSON view, SQL editors and data-diff panes painted a slightly different shade than the app around them, most visibly in the dark Studio theme. Editor surfaces are now taken from the live theme, so every theme matches.
- **Opening the data-diff page no longer re-tints every other editor** — it pinned itself to VS Code's dark theme, and an editor theme is applied globally.
- **Switching light / dark keeps the theme you chose** — toggling away from a hand-picked theme and back could land on plain Dark Studio instead of the one you were using.

#### Menus
- **Menus can be searched by what they show** — typing a database name in the Cloudflare D1 picker matched nothing, because rows were scored against their internal id rather than their label. Also fixes the Cloudflare account picker and the connection switcher.

### Changes

#### Data grid
- **The cell context menu is shorter and opens instantly** — the Copy and Filter variants moved into submenus (seventeen top-level rows down to thirteen), and building the quick-filter list no longer scans every loaded row on every right-click. Menus across the app are a touch denser, and submenus open without the scale animation that made them feel late.

#### Connections
- **The Cloudflare D1 database picker is a dropdown**, matching the engine and account pickers beside it, and a saved D1 connection opened from the sidebar now shows its account and database already selected instead of restarting on the first account.

#### AI
- **The conversation tab strip is gone from full-window AI mode**, where the sidebar already lists every chat and carries its own New-chat button. The docked panel keeps it.


## [1.18.0] - 2026-07-29

### Changes

#### Window & Title Bar
- **Windows title bar** — Reworked the custom title bar with a dedicated `WindowControls` component so minimize / maximize / close behave and render correctly on Windows.
- **Trial expired screen** — Updated layout to sit correctly within the new window chrome.


## [1.17.0] - 2026-07-28

### New Features

#### Installer
- **One-click Windows installer** — The setup exe now installs immediately with just a progress bar and launches Stroke when done, instead of the classic next-next-next wizard. Shortcuts are created automatically. Power users can run the installer with `/WIZARD` to get the full wizard back (install directory choice, etc.); `/S` silent installs and auto-updates behave exactly as before.

#### Canvas Table
- **Quick Filter** — Right-click a cell and open "Quick filter" for one-click filters shaped by the column's type: date/timestamp columns get date-range presets (Today, Last 7/30 days, This month/year) plus before/after this value; boolean and enum columns list their values; low-cardinality text columns surface the distinct values present in the loaded rows; number and text columns get the comparison operators seeded with the cell's value; JSON/JSONB columns offer "has key" filters for the object's top-level keys. Applying one drops a normal filter into the filter bar so it composes with everything else.

### Changes

#### UI Polish
- **Unified menu & popover design** — Every dropdown, context menu, and select now shares one recipe: roomier rows with consistent icon sizing, plain-case group labels, subtler separators, and cleaner shortcut hints. Menus size to their content instead of clipping long labels.
- **Consistent overlays** — All dialogs, tooltips, and floating panels use the same elevation, radius, and scrim treatment across the app.
- **Semantic status colors everywhere** — Success/warning/error/info states (diff views, status bar, MCP panel, backups, licenses, toasts) now follow the theme palette, so every theme renders them with correct contrast.
- **Pixel pass across pages** — Dashboards, data diff, onboarding, schema timeline, ERD, Redis, backup, settings, and 40+ other surfaces aligned to the design-system type scale, control heights, radius scale, and 4px spacing grid.


## [1.16.2] - 2026-07-28

### Bug Fixes

#### Data Table
- Dropped the per-cell "JSON" badge on JSON/JSONB columns — it cluttered the grid and was the main cause of scroll lag. Clicking a cell still opens the JSON viewer.
- Much smoother scrolling on tables large and small: plain vertical scroll runs on the compositor again instead of being blocked by the wheel handler on every tick. Ctrl+wheel zoom and Shift+wheel horizontal scroll still work.
- Expanded inline-JSON rows no longer black out or leave the bottom unreachable on very large (millions of rows) tables — their reserved height stays stable while scrolling.

#### Onboarding & Window
- The window can be dragged, minimized, maximized, and closed during onboarding — the tour no longer sits on top of the titlebar.
- The app opens at the full usable size on every platform without sliding under the taskbar; on Windows and macOS it now keeps its maximized state across minimize/restore.
- The date/time picker in the insert/edit form is selectable again.

#### Connections
- Connecting auto-retries transient failures (timeouts, network blips) a few times with fast backoff, but surfaces auth/config errors (e.g. 401 Unauthorized) immediately instead of spinning.

#### Update Dialog
- The "update available" dialog is centered on top of the connection screen instead of hidden behind it, with cleaner buttons.

### Changes

#### Design
- New installs follow the system light/dark theme on first launch, and default to 125% zoom on Windows.
- Refined the light-theme primary accent to a deeper, higher-contrast blue.
- Redesigned the filter bar: consistent control heights and radii, subtle fills, and a neutral AND/OR toggle.
- Slimmer 3-step onboarding with an inline license-activation row.
- Smaller tab labels and removed the blue active-tab underline.
- Removed em dashes from UI copy throughout the app.


## [1.16.1] - 2026-07-27

### Bug Fixes
- **Menus no longer clip long labels.** Context-menu and dropdown panels used fixed widths, so a label that did not fit painted straight through the rounded border — most visibly `Count chars / words / bytes` in the cell **Transform** submenu. Panels now size to their longest label, are capped at a shared maximum, and clip inside the border rather than over it. Labels that do exceed the cap now end in an ellipsis instead of a mid-glyph cut.
- **Chart axis dropdowns are readable again.** The axis pickers in the chart view opened from the SQL editor are native `<select>` elements whose options inherited a transparent background and near-white text, leaving the popup unreadable against the system menu surface. Options are now pinned to the popover colors, which fixes every native dropdown in the app.
- **Image columns no longer stall the grid.** Applying the avatar / image-thumbnail transform to a column started a full-resolution decode for every visible cell at once; on multi-megapixel sources that meant gigabytes of decoded image data. Decodes are now limited to a few at a time, restricted to rows actually on screen, downscaled once and released, and cached with an eviction policy that keeps visible thumbnails. Thumbnails are also freed when a table closes.
- **Fixed a memory leak when exporting CSV from a notebook cell**, which held the entire exported file in memory until the window was reloaded.


## [1.16.0] - 2026-07-27

### Improvements

#### Design
- Unified every text input, search box, and textarea onto one shape — `rounded-lg` corners with a consistent 2px border — so fields no longer vary between screens. Applied at the shared `Input`/`Textarea`/`InputGroup` primitives and across ~30 raw inputs in dialogs, pages, and panels. Seamless inline editors (grid cells, borderless search) are intentionally left untouched.
- Normalized card and panel corner radii to the design-system scale: content cards use `rounded-lg`, floating panels use `rounded-[10px]` (Instance Insights, Backup, Dashboard, ERD detail panel, connection pickers, confirm dialogs).
- Right-click cell menu: sub-menu rows (Transform, Insert, Copy row as) now match the size of their sibling rows instead of rendering a step larger.
- Wider table search field with a clearer resting border, and the six “Export as …” actions in the table’s more-actions menu are now grouped under a single **Export** submenu.

### Internal
- Repaired the unit-test harness (the `$lib` alias was missing from the vitest config, so no tests could run) and added `npm test`. Expanded coverage to the table-query filter/sort/search builder across engines, pagination helpers, response mapping, tab/SQL-format helpers, and the oversized-cell capping guard on the Rust side.

## [1.15.0] - 2026-07-23

### Improvements

#### Search
- Search options (match case, whole word, regex) now work per engine: full support on PostgreSQL and MySQL; case-sensitive matching on SQLite, Cloudflare D1, and libSQL. Toggles the engine can't honor are hidden per connection.
- Redesigned the table search bar — the match/word/regex toggles moved into a compact options popover so the input stays clean.
- Data-table search now uses `instr()` on SQLite/D1 instead of `LIKE`, fixing "LIKE or GLOB pattern too complex" errors on long search terms.

#### Data table
- Relationship columns can now be resized independently (previously resizing one resized all of them).
- Column widths for relationship and expression columns now persist across reloads.

#### Design
- Standardized all in-app text onto the unified `text-ui-*` type scale for consistent sizing across the app.

### Bug Fixes

#### SQL & query execution
- Statement splitting now respects semicolons inside string literals, comments, and `$$…$$` bodies, so multi-statement execution no longer breaks (PostgreSQL).
- MySQL: sorted queries no longer fail — the unsupported `NULLS LAST/FIRST` clause is replaced with an `ISNULL()` equivalent, and null-placement preference is honored.
- Keyset pagination no longer skips or reorders NULLs on nullable sort columns (falls back to offset paging).

#### Filters & search (cross-engine)
- DuckDB, ClickHouse, and MS SQL Server: the filters *not equals*, *not contains*, *starts with*, *ends with*, and *between* were silently ignored — they now apply correctly.
- *Not contains* now includes NULL rows (a null cell doesn't contain the term) on SQLite/D1, MySQL, and PostgreSQL.
- Search/LIKE patterns now escape `%`, `_` (and `[` on SQL Server) so those characters match literally instead of acting as wildcards (DuckDB, MS SQL Server).
- DuckDB: equality compares values natively instead of as text (`5` no longer differs from `05`).
- ClickHouse: *contains* and *ends with* are now case-insensitive, matching the search box.
- MySQL: filter and sort columns are validated, returning a clear error instead of a raw database failure.

#### Editing & data grid
- Duplicating a row that contains an oversized/truncated cell no longer corrupts that cell.
- Quick Look can now save an empty string distinctly from NULL, and switching a NULL cell to an empty string is no longer dropped.
- Saving a cell immediately no longer leaves a stale undo-stack entry that could re-write the value.
- Fixed a keyboard-navigation trap when starting an edit on a hidden column.
- Array cell editor: fixed stale input focus after removing or reordering elements.
- Edited cells now repaint immediately in all cases.

#### App & UI
- Infinite scroll no longer mixes in rows from a previously viewed table when switching quickly.
- The sidebar no longer gets stuck on "loading tables" when schema loading fails.
- Failed row deletions now surface an error toast.
- A single corrupt saved-connection entry no longer wipes the entire connection list.
- Live mode stops its backend watcher cleanly on teardown.
- Ctrl/Cmd+P no longer disrupts the command palette when it is already open.

#### Data diff & schema
- Data diff no longer drops rows when key columns contain duplicate values.
- Data diff now detects changes in JSON/array/object columns (previously always reported "unchanged").
- Data diff matches key columns case-sensitively (no more `ID`/`id` collisions).
- Schema timeline now detects column DEFAULT changes.

#### Import/export & tooling
- CSV exports include a UTF-8 BOM for correct encoding in spreadsheet apps.
- Fixed an object-URL leak in the browser download fallback.
- JSONPath: fixed parsing of quoted keys containing `]`, and `[*]` projection followed by an index, slice, or filter.
- AI: parallel streaming tool calls no longer merge together when the provider omits chunk indexes.

#### Security
- MCP read-only mode can no longer be bypassed via CTE-wrapped writes (`WITH … DELETE`) or `TABLE`/`VALUES` statements.
- LibSQL / D1: fixed incorrect result-truncation reporting in the MCP server.
- Hardened identifier validation (embedded `.` is rejected).

---

## [1.14.0] - 2026-07-23

### New Features

#### Cross-engine parity
- Redis keyspace browser — binary-safe values, non-blocking SCAN, CRUD, DB switcher, and streams
- EXPLAIN / query-plan visualization for DuckDB, ClickHouse, D1, and MS SQL Server (alongside Postgres/MySQL/SQLite)
- Instance Insights for SQLite, ClickHouse, and DuckDB
- Backup export/import for DuckDB (`EXPORT/IMPORT DATABASE`) and MS SQL Server (`BACKUP/RESTORE`)
- Schema introspection: MySQL & SQLite triggers, MySQL function introspection, and incoming foreign keys for more engines

#### Data & editing
- Multi-format export (CSV / JSON / SQL / TSV / Markdown / JSONL) in both the SQL console and the table view
- Inline cell editors for array, date/time, and JSON columns; bulk "fill selected rows"; copy cell as hex
- Query history favorites and run counts

#### Connections
- Saved-connection grouping in the sidebar
- Compact database pickers with keyboard navigation and friendlier connection errors

### Bug Fixes
- Table view now refetches after deleting rows (no more empty grid on paginated/large tables) and shows an "Applying…" indicator while changes are written
- The connection dialog no longer closes when you drag or resize the window by the titlebar
- Standard text-editing shortcuts (undo/redo, word- and line-delete) now work in inputs across the app
- Prisma: request `offline_access` scope and surface a clear "session expired" message on 401
- Hardened database write paths (no panics on bad binds), guarded localStorage writes, and sanitized Redis connection input

### Changes
- Release notes / changelog now open on the website instead of an in-app page
- Performance: heavy views are torn down after being hidden a while to reclaim memory, background polling (live mode, insights, Redis TTL) pauses while the window is hidden, and Monaco editors share a single theme observer
- Design system: refined font-size tokens, radii, accent selection, focus rings, and accessibility; ORM SQL highlighting; updated input borders; window maximizes on launch


## [1.13.0] - 2026-07-22

### New Features

#### Redis (new engine)
- **Redis support** — connect to Redis end-to-end: a keyspace browser (scan keys by type and inspect values), a redis-cli-style command console with formatted replies, and capability-gated UI that hides SQL-only surfaces (table browser, SQL editor, ERD, insights) for key-value connections. Redis replaces BigQuery in the "coming soon" slot.

#### SQL Editor
- **Multiple SQL editor tabs** — open several independent SQL editor tabs from the command palette ("New SQL Editor") and work across them, each with its own draft and results.

#### Charts
- **Charts overhaul** — theme-aware series colours that follow `--primary`, categorical bars sorted by value with ellipsis on truncated labels, bar shadow-hover, a compact Y axis and horizontal scroll, plus crash-safety and large-data handling across sankey / tree / dendrogram / word-cloud.

#### Design System
- **App-wide design system** — a documented `DESIGN_SYSTEM.md` type scale (`text-ui-*`) swept across every surface, aligned `ui/*` primitives (standardized `rounded-[10px]` menus and submenus), a unified searchable-dropdown component, a widened AI model picker with 2-column provider/model grids, and grayscale font antialiasing for crisper text.
- **Connection modal redesign** — a full-page providers-as-tabs layout with a searchable engine picker, a full-width Advanced section and smaller inputs.
- **Command-palette page navigator** — the vertical activity bar is replaced by a ⌘P "Go to page" navigator; a `+` button in the status bar opens new surfaces, and the Tools launcher is removed.

#### Instance Insights
- **Instance Insights** — a live monitoring dashboard for PostgreSQL and MySQL, opened from the command palette or the welcome screen. Activity / State / Config / Replication sub-tabs surface session, TPS, tuple and block-I/O stat cards, ECharts timelines (per-second rates diffed from cumulative counters), sessions / locks / prepared-transaction tables, a searchable `pg_settings` / server-variables browser, and replication stats & slots. Auto-refreshes, and degrades gracefully when catalogs or permissions are missing.

#### Database Objects
- **Database Objects overview** — a database-wide catalog of Tables (name, schema, kind, owner, estimated rows, total / data / index size, comment), Views, Functions / Routines and Triggers, per dialect (Postgres/CockroachDB, MySQL/MariaDB, SQLite/D1/libsql, ClickHouse, DuckDB, SQL Server). Open it from the command palette or the welcome screen.

#### Security
- **Roles & Permissions (RLS)** — a new Permissions view on the Security page: a grouped role tree (superusers / login / group), a role-attributes panel, inherited-from membership, and per-database access grants.

#### AI
- **Agent settings** — a new Agent tab in Settings for the default model, per-provider API keys (OpenAI / Gemini / Anthropic / OpenRouter), chat & code font sizes, and the thinking-indicator style.
- **Multi-tab AI chat** — a horizontal conversation tab bar over the multi-conversation / history model.
- **Command-palette quick-ask** — ask the AI straight from ⌘K as a tool-using, multi-turn chat.
- **Slash-command quick actions** — slash commands in the AI sidebar for common asks.
- **Table mentions as badges** — `@`-mentioned tables become removable badges above the chat input.
- **Downloadable AI results** — an `export_data` tool with JSON / CSV / Markdown downloads (progress + toast), plus an `export_query` MCP tool.

#### Data Table
- **Data view modes** — a segmented switcher renders the current page as Table, JSON (Monaco-based, with JSONPath filtering), Record (one row at a time, DBeaver-style, with field search and inline editing), Text (CSV / TSV / Markdown / JSON Lines), Chart, or ERD. The grid stays mounted so edits, selection and scroll survive switching, and each tab remembers its mode.
- **Per-table ERD** — an entity-relationship diagram scoped to one table and its foreign-key neighbours, with a decrossed layout and orthogonal edge routing; also available as an ERD data view in the switcher above.
- **Saved views** — bookmark the current search + filters + sort + hidden columns + view mode under a name (per connection / schema / table), re-apply with one click, and see a count badge on the toolbar.
- **Find & Replace** — replace text across an editable column with contains / exact / regex matching (capture groups, live pattern errors, optional case sensitivity) and a full before → after preview; changes route through the parameterized cell-save pipeline. Disabled on tables without a primary key and on read-only connections.
- **Column reorder, colours & tags** — move columns left / right / first / last from the header menu (display-only, persisted), paint header bands in six muted tones, and add short badge tags to columns.
- **Docked relation panel** — the foreign-key related-rows sub-view moved into a resizable bottom dock (height persisted) instead of fighting the grid's scroll.
- **Cell display markers** — tell NULL (∅), empty (`""`) and whitespace-only (·) cells apart at a glance; tint timestamp cells by freshness; and render image / avatar columns as thumbnails.
- **Pagination strategies** — choose offset (default), cursor, keyset or temporal paging in Database settings; every mode safely falls back to offset when preconditions aren't met.
- **Configurable NULL sort order** — set where NULLs land in browse queries.
- **Richer tab bar** — an expanded tab context menu and middle-click to close.

#### SQL Editor
- **Run split-button + query parameters** — a DataGrip-style Run dropdown (all statements / statement at cursor with preview / selection) plus `:name` parameters detected by a string-, comment- and cast-aware scanner, with an Auto / Text / Raw SQL / NULL mode per parameter; values inline as escaped literals so history stays reproducible.
- **Generate SQL** — a Generate SQL dialog (with `:name` skeletons), alongside console / count / copy-columns table actions and a redesigned View DDL dialog.
- **Error tab** — failed SQL runs open in a dedicated, copyable Error tab.
- **Query draft restore** — the Query Editor restores its unsaved draft on reopen.
- **Query-log console** — every executed statement logged in a console at the bottom.

#### Interface
- **Panel-switchable sidebar** — a Connections panel (list / add / switch connections inline) and an Extensions panel (VSCode-style list with Install toggles; clicking an extension opens its detail as a tab) alongside the Tables panel.
- **Unified tooltips** — a single delegated GlobalTooltip styles every tooltip app-wide with a consistent arrow, 8px offset, 450ms delay, hover-persistence and viewport-aware flipping, replacing native `title` and per-component tooltips.
- **Status bar** — shows the app version, a searchable connection switcher with provider brand icons, and a last-fetch timing readout; switch database straight from the ⌘K root.

#### Extensions
- **Extensions gallery** — a launcher-style card grid (responsive 2–4 columns) replaces the master-detail list, each card showing its icon, name, kind and an inline enable toggle; click to drill into detail.
- **Cell transform library** — a richer set of per-column cell transforms with a result toast.
- **Data generators** — split into ID Generators (IDs only) and a new Data Gen generator.

#### Themes & Localization
- **High Contrast themes** — new Dark and Light High Contrast themes for accessibility.
- **Localization** — a localization foundation and language picker (English, Spanish, French, German) covering the sidebar, tabs, column menu and Settings.
- **Phosphor icons** — the Phosphor icon family is selectable in the icon wrapper (~80 semantic glyphs mapped).
- **More fonts** — additional UI and editor fonts.

#### Vim
- **Experimental Vim mode** — an off-by-default modal keyboard layer (toggled in Settings) with a status-bar mode indicator, `hjkl` / edit / delete / yank / search bindings in the data grid, monaco-vim in the SQL and ORM editors, and `:` / `gt` / `gT` at the app level.

### Performance

#### Data Grids
- **Non-reactive row storage** — large browse / SQL / ORM row arrays moved to `$state.raw`, removing per-cell Proxy overhead on the canvas draw path and the retained per-index signals when scrolling huge tables; redraws are still driven by explicit signals.
- **Huge tables** — the row cap is raised to 5M with sparse row-top tracking and normalized scroll height so every row is reachable, plus windowed loading for large "All" result sets.
- **Fewer redundant paints** — no double-paint on structural changes, and a per-frame allocation during range-select was removed.

#### App
- **Memory across tabs** — cold background tabs release their result rows at a much lower threshold; the three most-recent tabs stay warm, and closed tabs no longer pin result sets in memory.
- **Faster first open** — table metadata is fetched concurrently with the first page of rows.
- **Instance Insights config** — the large `pg_settings` table uses fixed layout + content-visibility for smooth scrolling.
- **Lighter sidebar & stores** — debounced filtering, memoized sort and capped un-virtualized lists in the sidebar; debounced JSONPath evaluation and localStorage persistence off the keystroke; image thumbnails downscaled once to fix scroll lag.

### Bug Fixes

#### Data Table
- **Huge / scaled tables** — the inline row-expand panel and the cell editor now paint correctly on very large or zoomed tables.
- **Avatar / image cells** — transient avatar loads no longer freeze as a "broken image", and avatar transform detection is fixed.
- **Sticky header seams** — scrolled rows no longer bleed through the sticky header of the data-diff and foreign-key sub-views.
- **Hidden columns** — expanded-row JSON, row copy and exports now respect hidden columns (exporting only the visible ones).
- **Resize & toggle glitches** — fixed resize jitter, a relation-header gap and expand-toggle flicker.

#### Data
- **MySQL DECIMAL** — DECIMAL columns decode exactly instead of through the integer path.
- **Cross-dialect column stats** — column statistics work across engines, D1 shows the right welcome name, and per-connection table state is kept separate.
- **Row counts** — the sidebar counts rows on every engine.
- **Session timezone** — the session timezone setting is actually applied.
- **Clipboard** — copy routes through Tauri's native clipboard plugin.

#### Interface
- **Extensions panel** — per-extension icons and kind labels.
- **Diagram export** — success / error toasts on ERD export.
- **Off-screen window** — an off-screen app window is recovered, and split-pane layout no longer shifts.
- **AI chat** — chat font settings apply; fixed undefined table names and quick-ask auto-scroll / flicker.

#### Stability
- **Crash fixes** — a duplicate keyed-each key and an infinite loop in the sidebar row-height measure no longer crash the app.

### Changes

#### Interface
- **Move sidebar right** — right-click to dock the sidebar to the right side.
- **Split panes** — tmux-style active / inactive pane styling and smoother resizing.
- **Unified input focus** — inputs across the app share a single fused two-tone focus stroke.


## [1.12.0] - 2026-07-14

### New Features

#### Data Table
- **Array cell editor** — Postgres array columns (`text[]`, `int[]`, …) get a dedicated add / remove / reorder editor instead of being edited as raw JSON, and now display as `{a,b}` array literals.
- **Grid styles** — choose from six table grid styles in Settings: Lines, Bordered, Striped, Dotted, Dots, and Minimal.
- **DML preview** — review (and edit) the prettified SQL for a change before it's applied.

#### Interface
- **Provider sign-in** — refreshed the connection screen and provider sign-in flow.
- **More keyboard shortcuts** — reopen closed tab, jump to tab 1–9, toggle the tab bar, and disconnect.
- **Auto-reconnect** — optionally reconnect to your last database on startup.

### Bug Fixes

#### Data
- **Array columns rendered as garbage** — Postgres array values were decoded from the binary wire format as lossy UTF-8 (□ boxes); they now decode into proper arrays.

#### Security
- **Saved credentials not persisting** — the OS keychain integration now enables a real per-platform backend, so AI keys and OAuth tokens actually persist (previously a missing backend feature silently used a non-persistent in-memory store on some setups).


## [1.11.0] - 2026-07-14

### Bug Fixes

#### Backup & Restore
- **Cloudflare D1 export** — internal `_cf_*` tables (e.g. `_cf_KV`) are now hidden and skipped, so D1 backups no longer fail with `SQLITE_AUTH`, and one unreadable table can't abort the whole export.
- **Table selection** — deselecting tables in the backup panel now sticks instead of instantly resetting to all.
- **Restoring routines** — PostgreSQL functions, triggers, and enums (dollar-quoted bodies) and MySQL `DELIMITER` blocks now restore intact rather than being split at their internal semicolons.
- **Restore stability** — a failed statement containing multibyte text no longer panics the restore, and a single PostgreSQL error no longer rolls back the entire restore (per-statement savepoints).
- **Data fidelity** — PostgreSQL `smallint`/`numeric`/`money` values and non-finite floats now export correctly (previously lost or written as `NULL`), and D1 BLOB columns export as hex literals instead of corrupt text.
- **Stop button** — stopping a backup or restore now actually halts the backend, not just the UI.
- **Filtered exports** — backing up a subset of SQLite/D1 tables no longer emits indexes or triggers for tables outside the selection.

### Changes

#### Interface
- **Provider sign-in** — the "Sign in with …" OAuth button now shows the provider's brand mark and lifts subtly on hover.


## [1.10.0] - 2026-07-13

### New Features

#### Interface
- **Signature accent** — the default Studio light & dark themes gain a refined indigo primary, focus ring, and text selection (verified AA).
- **Display headings** — titles use a tighter, optically-sized heading treatment for a more premium feel.
- **Elevation system** — dialogs, menus, popovers and cards use layered, theme-aware shadows with a catch-light rim instead of flat borders.
- **Searchable dropdowns** — the filter condition/value pickers and the status-bar AI model picker gain a search box with a checkmark on the selected item.

#### Data Table
- **Date filter presets** — a relative-range menu (Today, Last 7 / 30 / 90 days, This month, Year to date, and "In the last N hours/days/weeks/months") beside the calendar.
- **Enum value dropdown** — enum columns filter by picking from a searchable list of their values instead of typing free text.
- **Instant rows, count later** — opening a table paints rows immediately and fills in the total ("… of N") in the background instead of waiting on COUNT(*).
- **Multi-column sort** — shift-click column headers to add secondary sort keys; sorted headers show their priority number.
- **Dismiss an expanded row** — press Esc (closes the most recent) or the new close button to collapse an inline expanded-row JSON panel.

#### Security
- **OS keychain storage** — AI keys and provider OAuth tokens now live in the OS keychain (Keychain / Credential Manager / Secret Service), migrated automatically from the old plaintext file.

#### Relation Tree
- **Row counts stream in** — each related table's row count now loads in the background without blocking the relation tree.

### Bug Fixes

#### AI
- **Add-model dialog** — stepper, buttons and provider selection use the app accent consistently.
- **AI empty state** — "Configure a model" is a clear primary action rather than an alarming amber warning.
- **@-mention picker** — decluttered (proper elevation, no redundant per-row schema labels).

#### Data Table
- **Table navigation shortcuts** — Cmd/Ctrl+Arrow now scrolls/paginates cleanly instead of also jumping the cell cursor.
- **Date filters match** — "equals" and "is between" on timestamp columns now match the whole day instead of returning no rows.
- **JSON cell badge** — the braces icon no longer overlaps the "JSON" label; the pill is sized to fit.
- **LIVE indicator** — a calm pill (no neon ring/ping), theme-correct and reduced-motion aware.
- **Row inspector density** — the expanded-row JSON view uses smaller, right-sized text.
- **Image previews** — cell URLs are percent-encoded before preview/open, fixing images that failed to load.

#### Interface
- **Graphite contrast** — button labels on the Graphite theme now meet WCAG AA.
- **Icon-set picker** — preview no longer crams two glyphs together; the description text is legible.
- **Studio Light depth** — cards and menus no longer read as sunken; raised surfaces now sit above the canvas.
- **MCP panel** — client tiles use one consistent neutral style instead of mismatched colors.
- **Title-bar hovers** — control hovers are visible on light themes (were near-invisible white overlays).

#### Connection
- **Provider sessions persist** — connections via Neon/Supabase/etc. no longer prompt for re-login roughly every day.

#### Sidebar
- **Auto-deselect** — multi-select table actions (Open/Close/Copy/Pin) clear the selection when finished.

#### SQL Editor
- **SQL error panel** — errors are now selectable/copyable with a copy button and a terminal-style red gutter.

### Changes

#### Interface
- **Dialog surfaces** — dialogs adopt the elevation system (rounded, layered shadow) in place of flat hairline borders.
- **Button feedback** — primary buttons gain subtle elevation and a press (scale) response.
- **Cleaner tab bar** — removed the recent-tables and new-tab buttons from the tab strip (new tabs open via ⌘T / the command palette).
- **Text selection** — selection colour is brand-tinted from the theme's primary, kept legible over editor and cell content.

#### Data Table
- **Range selection removed** — drag/shift row & cell range selection is disabled (unused); single-cell focus, editing and copy are unchanged.


## [1.9.0] - 2026-07-11

### New Features

#### Connection
- **Silent auto-reconnect** — a dropped database connection (network blip, sleep/wake, idle timeout) now reconnects automatically when the network returns, the window refocuses, or you switch tables / refresh, showing only a subtle status-bar indicator instead of a "Connection lost" popup.

### Bug Fixes

#### Data Table
- **Grid no longer renders blank** — on macOS WebKit the canvas colour reader returned a stale computed colour, collapsing every theme token to one value so cell text and grid lines vanished (only badges/icons showed). Each colour is now resolved on its own node.
- **Grid no longer blacks out when switching tabs** — the cached 2D context could keep pointing at a canvas WebKit had recreated, so draws landed on an off-screen element. The context now always tracks the live canvas and repaints on mount/tab switch.

#### Schema Visualizer
- **Opens readable instead of a tiny sliver** — a deep schema no longer shrinks to an unreadable horizontal band; it lands at a legible zoom at the top-left of the graph (pan to explore), with roomier, larger table cards and more spacing.

#### Interface
- **Spinners and loaders animate at normal speed** — a global reduced-motion rule was forcing every animation to 0.12s, so spinners/pulses ran frantically fast when macOS "Reduce Motion" was on. Removed the global motion overrides.

### Changes

#### Performance
- **Smoother scrolling and lower memory on large tables** — removed per-frame allocations in the grid render loop, decoupled selection/hover repaints from canvas resizing, and avoid allocating a per-page row-offset array on million-row tables.


## [1.8.0] - 2026-07-10

### Changes

#### Connect experience (redesigned) (#40)
- Rebuilt the connection screen as a **two-pane layout** — choose your connection type, driver, and saved connections in a left rail while editing details in a focused right pane, so the fields are reachable without scrolling.
- **Pick one input method** — a segmented switch toggles between pasting a **connection string** and filling **manual fields**, instead of showing both at once. Pasting a string auto-fills the fields and validates on connect.
- **Cleaner provider picker** — Neon, Supabase, PlanetScale, and Prisma Postgres now appear as a single calm list with a clear selected state and a compact sign-in action, replacing the bulky cards.
- Redesigned the **connection error** as a readable inline alert and gave the footer a clear status chip (Ready / Unsaved / Connection OK / Failed) alongside unambiguous Test and Connect actions.
- Inputs now use a **crisp outline and a single accent focus ring**, consistent across the app.
- Widened the connections rail and stopped the connection screen from flickering when switching between saved connections or drivers.

#### Interface polish
- Added **press feedback** to buttons and gave menus, selects, popovers, tooltips, and dialogs consistent, snappier open/close motion; menus now scale from the control that opened them.
- The app now respects the system **"reduce motion"** setting — movement is minimized while feedback is preserved.
- **Sidebar** table rows: names and row counts sit on a shared baseline, resting contrast is higher for easier scanning, and section headers are refined.
- Tightened **Settings** spacing and control feedback.

### Bug Fixes
- Fixed **"Limit must be at least 1"** when opening a table after choosing "All" rows — the page size now falls back to a valid default (50) instead of erroring.
- The data grid no longer offers **Set NULL** on NOT NULL columns; it shows a clear message instead of attempting a write that would fail.
- Text on the connection screen is no longer accidentally selectable, and the cursor now shows a pointer on every clickable control.


## [1.7.0] - 2026-07-06

### New Features

#### Database Providers
- **Sign in to a database provider** — connect **Neon, Supabase, PlanetScale, and Prisma Postgres** from inside Stroke. Authorize once (OAuth in your browser, or a pasted key), see every database on your account, and connect in one click — no hunting for connection strings. Supabase remembers your database password after the first connect.
- **Switch provider databases from the status bar** — the database switcher now lists your account's other databases and jumps to them directly.

### Bug Fixes
- **Ctrl/Cmd+A** now selects the text inside a cell editor instead of selecting every row
- **Status badges** and **boolean glyphs** are now legible in light mode (they were tuned only for dark)
- The **foreign-key sub-view** now scrolls vertically and horizontally instead of trapping the wheel


## [1.5.1] - 2026-07-06

### Bug Fixes
- Fixed a console warning in the JSON tree view where the initial expand state read reactive props directly

### Changes

#### Updates
- Added a **View changelog** link in the updater toast that opens the online changelog
- Windows updates now install in place with a passive progress bar (no setup wizard to click through)


## [1.5.0] - 2026-07-05

### Changes
- Data-table, SQL editor, JSON viewer & extensions improvements (minor release) (#36)


## [1.4.0] - 2026-07-04

### Changes
- Release v1.3.0 — split panes, block selection, staged deletes & more (#18)


## [1.2.0] - 2026-07-03

### New Features

#### Data Table
- Search highlighting: the toolbar search now shows exactly where each match falls — matched text is highlighted inside the visible cells, across every column
- JSON tree view in expanded rows: fold and unfold any nested object or array with proper chevrons, type-colored values, and item/key counts — plus per-field **Copy value** and **Open in JSON viewer** actions that jump straight into the full Monaco viewer. A Tree/Raw toggle is remembered across sessions

### Bug Fixes

#### Stability
- Fixed the app freezing when opening tables with very large cells — multi-MB file buffers stored in `jsonb`/`text`/blob columns could lock up the whole app. Cells above 256 KB now load as a truncated preview with the real size shown; the grid, JSON viewer, copy, and export all mark the truncation clearly, and editing or filtering on a truncated cell is blocked so a preview can never be saved back over real data. Applies to PostgreSQL, MySQL, SQLite, and the built-in MCP server
- A crash inside one table view can no longer take down the whole app: rendering and interaction errors are contained to that tab, which shows a "Reload this view" card while every other tab, the sidebar, and the status bar keep working
- SQLite: large blobs no longer render as unbounded hex text (the worst case doubled the data size on screen)

#### Database Switcher
- The search box is now always there and focused the moment the switcher opens — start typing immediately, with any number of databases
- New shortcuts: **⌘D** opens the database switcher, **⇧⌘C** the connection switcher; arrows, Tab, and Enter drive the list right from the search box

#### Sidebar
- Restored smooth scrolling to the bottom of large table lists: removed the inline column-expansion rows that broke the virtualized list
- "View data structure" is now "View structure"

#### Licensing
- Active trials always keep Pro access: only a definitive "trial expired" verdict locks Pro features — a transient license-check failure can no longer lock out trial or paid users

### Changes

#### AI Chat
- The conversation column now matches the composer width for a comfortable reading measure, with more breathing room between messages

#### Performance
- Sorting SQL results by JSON columns no longer re-stringifies every cell on every comparison
- Copy and export of object cells reuse one serialization path and stay fast on large result sets


## [1.1.0] - 2026-07-03

### New Features

#### Tabs
- Pin tabs: pinned tabs stay grouped at the front, show a pin badge, and survive "Close Others" / "Close All"

#### Themes
- New **Graphite** theme, wired through the editor and diagram themes

#### Licensing
- New in-app license page: "Activate Pro" now opens a dedicated tab with key entry, plan status, what Pro unlocks, and a purchase link — confetti included on activation
- License entry in Settings with a live plan badge (Pro / Trial / Free)

#### SQL Editor
- Column sorting on query results — numbers sort numerically, NULLs last, and the sorted order carries into the JSON view, charts, and CSV/JSON exports

### Bug Fixes

#### Data Table
- Pinned columns now freeze flush to the left edge — no more empty gutter-wide gap after scrolling
- Query results grid is read-only: removed actions that can't apply to ad-hoc results (Filter/Exclude by value, Edit, Set NULL, Duplicate/Delete row, INSERT copy)
- Column stats no longer error with "Invalid identifier" on SQL editor results
- Sidebar rows no longer shift as row counts load in

#### Window
- Double-clicking the titlebar now maximizes reliably (a duplicate handler was maximizing and instantly restoring)

### Changes

#### Performance
- Reconnect feels instant: the overlay drops as soon as the connection is live, and schemas/tables stream into the sidebar instead of blocking the screen
- Table row counts load lazily — the sidebar renders immediately with placeholders and exact counts fill in from a background pass; schema switches get the same speedup
- Faster Postgres connect: one fewer round trip per pooled connection, and the reachability check runs concurrently with the real handshake
- AI streaming responses parse markdown at most once per frame instead of once per token

#### Settings
- Settings dialog redesigned: grouped sections (Appearance / Behavior / More), clearer hierarchy, keyboard-shortcut chips

#### Release pipeline
- Releases now publish to the public `stroke` repository — auto-update, Homebrew, and Scoop keep working with the private source repo


## [1.0.0] - 2026-07-01

### Performance
- Faster (re)connect: `onConnected` now loads query history + MCP autostart concurrently with the schema/table load instead of behind it, and no longer double-fetches the query stores on startup.
- Fixed the reconnect "slow acquire" storm: per-table `COUNT(*)` on large Postgres schemas now runs with bounded concurrency (capped at the pool size) instead of firing every count at once and starving the 4-connection pool.
- Sidebar row counts for SQLite / D1 / LibSQL / DuckDB are batched into one `UNION ALL` round-trip per chunk (a 40-table Turso/D1 sidebar goes from ~40 requests to 1), with a per-table fallback.
- Extended the `include_meta` fast-path to MySQL / MSSQL / ClickHouse / DuckDB so pagination, sort and filter stop re-fetching primary-key / foreign-key / column metadata every page (MSSQL plain paging drops from 4 serialized round-trips to 2).
- Table list is cached per connection + schema (short TTL) so rapid tab/schema navigation stops re-hitting the catalog; refresh and DDL force a fresh load.
- Real query cancellation for PostgreSQL (`pg_cancel_backend`) and MySQL (`KILL QUERY`): cancelling now stops the statement server-side instead of only abandoning the client future.

### Themes & UI
- Retuned every theme's `muted-foreground` to clear WCAG AA (≥4.5:1); secondary text (sidebar row counts, labels) was previously failing contrast and hard to read.
- Added four curated themes: **GitHub** (light), **Rosé Pine**, **Catppuccin Mocha**, and **Solarized** (dark) — wired through the registry, editor themes, and Mermaid.
- New selectable **icon weight** (Light / Regular / Bold) in Settings, applied globally to every Lucide icon.
- Extensions page: refreshed detail header into an accent-tinted card that adapts to the active theme.
- Connection modal: removed the Environment selector and reworked the layout so the action buttons are always visible and the form body scrolls cleanly.

### Refactoring
- Introduced a shared `db/sql_util` module (statement classification, identifier quoting, LIKE/literal escaping) and collapsed the near-identical D1 and LibSQL row helpers into generic functions behind a `RemoteSqlite` trait (~210 lines removed from `query.rs`).
- Collapsed 13 duplicated `open*Tab` functions in the shell into one data-driven helper; extracted column-shape normalization (`column.js`), table/UI preferences (`stores/table-prefs.js`), and the row-response mapper (`readRowsResponse`).

### Fixes
- Fixed a crash on the schema Enums page (`each_key_duplicate`): the Postgres enum query fanned out labels once per column that used the type; values are now computed in a subquery, with a defensive frontend dedupe.
- Guarded persisted store writes (connections / settings / layout) so a full/blocked `localStorage` can't throw into connect/disconnect flows.
- `formatInvokeError` no longer masks genuine backend errors that happen to mention "invoke" / "Tauri".
- Removed a shadowed duplicate `Mod+T` hotkey.

### Chore
- Removed build cruft committed to the repo (an extracted AppImage `squashfs-root/`, a stale `schema.rs.tmp` copy, `dummy.txt`, `file.txt`) and hardened `.gitignore`.


## [0.7.2] - 2026-06-26

### Changes
- feat:: a11y, font rasterization, and more fixes. (#11)


## [0.7.0] - 2026-06-24

### Changes
- fix: cross-driver correctness, a11y, and faster command-palette, ux and ui improvement, rendering fps improvements (#10)


## [0.6.0] - 2026-06-23

### Changes
- feat:: features and improvements (#9)


## [0.5.0] - 2026-06-17

### Changes
- feat:: bugs, optimizations, virtual cols and many more  (#8)


## [0.4.3] - 2026-06-06

### Bug Fixes

#### Auto-update
- Fixed auto-update failing on macOS (and all platforms) with "invalid encoding in minisign data" — the updater public key in `tauri.conf.json` was accidentally base64-encoded twice, so the embedded key couldn't be parsed. It's now the correct single-encoded value.

> **Note:** 0.4.1 and 0.4.2 shipped with the malformed key baked in, so those installs cannot auto-update — download 0.4.3 manually (or via `brew`/`scoop`) once. Auto-update works normally from 0.4.3 onward.

## [0.4.2] - 2026-06-06

### Bug Fixes

#### Release Pipeline
- Fixed the Linux release job failing to find its bundles — Tauri names them after the product name (`Stroke_*`), but the upload step expected lowercase `stroke_*`; it now globs the real files and normalizes the asset names. This also restores `latest.json` (auto-update) publishing.

### Changes

#### Distribution
- **Homebrew (macOS):** `brew install --cask broisnischal/tap/stroke` — installs warning-free, no manual `xattr`
- **Scoop (Windows):** `scoop bucket add stroke https://github.com/broisnischal/stroke && scoop install stroke` — installs without the SmartScreen block
- Dropped the self-signed Windows certificate (it never cleared SmartScreen, so it added nothing); direct `.exe` downloads are unsigned — use Scoop, or click "More info → Run anyway"

## [0.4.1] - 2026-06-06

### Bug Fixes

#### Release Pipeline
- Fixed release builds failing on all platforms — regenerated the Tauri updater signing key so the private key and password match again (`incorrect updater private key password: failed to fill whole buffer`)
- Fixed macOS builds failing during code signing (`PKCS12 import MAC verification failed`) by switching macOS to ad-hoc signing; mac users run `xattr -cr /Applications/Stroke.app` once after install

## [0.4.0] - 2026-06-05

### New Features

#### Release Pipeline
- **In-app release notes** — the update dialog now shows a structured release notes page with ✨ New Features, 🐛 Bug Fixes, and 🔧 Improvements sections parsed from the changelog
- **Code signing** — builds are now signed on macOS (Developer ID + notarization) and Windows (certificate thumbprint via cert store) in CI

### Bug Fixes
- Fixed rel columns remaining visible in the canvas after being hidden from the columns panel — the toolbar was capped at showing 5 rel entries, so hiding all 5 caused DataTable to promote the next batch of FK tables into view with no way to hide them
- Fixed the `rel` badge not dimming visually when a relationship column is toggled to hidden in the columns panel
- Fixed the "Hide all" button in the columns panel not including relationship columns — clicking Hide now hides both regular and rel columns together

### Changes
- Renamed app references from **DB Studio** to **Stroke** across all files, workflows, and documentation
- Updated domain from `dbstudio.app` to `stroke.app` in license gate and about dialog
- Updated GitHub repo references from `broisnischal/studio` to `broisnischal/stroke` in updater endpoint, release workflow, and README
- Updated macOS bundle artifact name from `db-studio.app` to `Stroke.app` and all installer filenames from `db-studio_*` to `stroke_*` to match the `productName` in tauri config


## [0.3.4] - 2026-06-04

### New Features

#### Canvas Table
- **Canvas zoom** — `Ctrl/Cmd + Scroll` or `Ctrl/Cmd + =/-/0` zooms the entire table (rows, fonts, columns) proportionally; zoom level is shared across all open tabs and persisted to `localStorage`
- **FK inline sub-view** — clicking a foreign-key cell opens a compact panel below the row showing the referenced record(s); `Ctrl/Cmd + click` navigates to the full table; `Esc` closes
- **Reverse FK relationship columns** — tables that reference the current table appear as pill-badge columns on the right; clicking a badge shows related rows in the inline panel (max 5 columns, deduplicated by `fromTable`)
- **Per-tab expand state** — JSON expand rows and FK sub-view state are saved per table tab and restored when switching back; different tables start with their own clean slate
- **Shift + scroll for horizontal** — default mouse wheel scrolls vertically; Shift + scroll moves the table horizontally, preventing accidental horizontal hijack on wide tables
- **`Ctrl/Cmd + T` clears table search** — when viewing a table tab, this shortcut clears and focuses the row search input
- **Go-to-top / bottom buttons** always visible in the status bar when on a table tab

#### SQL Editor
- **Ctrl/Cmd + Enter runs query from first keystroke** — uses a document-level `capture` listener so it fires even before the editor is clicked into; also works from the very first open without clicking Run first
- **Auto-focus on tab open** — the SQL editor receives focus automatically whenever the SQL tab becomes active

#### SQL Console
- **Export results** — `CSV` and `JSON` download buttons appear in the result toolbar when a query returns data
- **0-row results hidden** — when a query returns 0 rows, no result card is added to the AI chat; the AI still knows the result was empty

#### MCP Server
- **Read-only mode** — toggle in the MCP dialog restricts the agent to `SELECT`-only queries; `INSERT`, `UPDATE`, `DELETE`, `DROP`, `ALTER`, and other write operations are rejected at the server level with a clear error; setting is persisted and synced to the Rust backend live
- **MCP panel revamp** — redesigned in the Linear/Resend/Raycast style: compact list rows instead of large cards, inline Start/Stop button, amber-accented read-only toggle, theme-aware borders throughout

#### AI Chat
- **Image rendering removed** — database columns containing image URLs no longer trigger dozens of simultaneous network requests; images are replaced with compact link chips (`filename.ext`) that open on click; resolves major performance lag on tables with image URL columns
- **Schema/describe results hidden** — internal AI tool calls (`describe_table`, `get_schema`) no longer show result cards in the chat; the AI still receives the data and answers correctly but the intermediate steps stay invisible
- **Inline code chip styling** — column name chips (`created_at`, `user_id`) are more polished: 5px radius, proper padding, `Geist Mono` font, `white-space: nowrap`
- **AI chat table styling** — uppercase small-caps headers, hover-highlight rows (replaces static zebra striping), 8px border-radius with `overflow: hidden` clipping
- **Input textarea** — replaced hardcoded `#3a3a3a` border with `var(--border)` tokens; `focus-within` ring; `font-family: inherit` ensures Inter is used for typed text
- **AI chart save preserves spec** — AI-generated charts (including `meter` and `choropleth` types that don't use ECharts) now save the full `aiSpec` so previews render correctly in Charts and Dashboard pages

### Bug Fixes
- Fixed `onCanvasPointerLeave` missing after tooltip removal — hover state now clears correctly on mouse leave
- Fixed `cy` variable not defined in virtual column badge drawing — was causing a `ReferenceError` silently
- Fixed `run` not in scope in `docRunHandler` — `Ctrl+Enter` in SQL editor would silently fail with `ReferenceError` instead of running the query
- Fixed connection pool exhaustion — reverse FK relationships are now cached per `schema.table` and loaded only once, not on every page/sort/filter reload
- Fixed `canvasZoom` not live-updating open tabs — replaced local `$state` with a module-level `$state` store so all DataTable instances react immediately; also fixed passive wheel listener that prevented `preventDefault`
- Fixed `Cmd+K` command palette search persisting between opens — `paletteSearch` is now cleared when the dialog closes
- Fixed FK sub-view "Open in sub view" not navigating for reverse FK — now correctly passes `reverseRel` info to `handleFollowForeignKey` and navigates to the referencing table with filters applied
- Fixed virtual relationship column width too narrow — minimum raised to 300px with conservative `10px/char` estimate; eliminates `cart_it...` truncation
- Fixed row lines not extending into virtual columns — bottom grid line now draws to full `_viewportWidth` and is painted after cell fills so it stays on top
- Fixed FK sub-view panel position — panel now anchors to viewport left edge using `transform: translateX(_scrollLeft)` (GPU-composited, no layout thrashing) regardless of horizontal scroll
- Fixed FK sub-view lag on open — replaced debounced `ResizeObserver` height chain with zero-cost overlay (panel doesn't push rows down, no `rowTops` recomputation)
- Fixed horizontal scroll hijack in FK sub-view — only horizontal `wheel` events are stopped from propagating; vertical scroll always passes through to the main table

### Changes
- Removed column collapse (drag-to-hide) — columns now have a hard 48px minimum width instead of snapping to a 12px strip
- Removed hover tooltips on canvas cells — eliminated the 450ms `setTimeout` loop that was firing on every mouse move at 120Hz
- Not-null dot badge removed from column headers — was visually indistinguishable from the `·` type separator and caused confusion
- FK sub-view and JSON expand are mutually exclusive per row — opening one closes the other
- Ctrl+T in table view clears search instead of opening the command palette tables page (the command palette shortcut is unchanged from other views)
- `prose-ai` table rows use hover highlight instead of static even/odd zebra striping for a cleaner look

---

## [0.3.3] - 2026-06-04

### New Features
- Canvas-based table renderer handles 1M+ rows without freezing
- Foreign key sub-view panel — click any FK cell to see related rows inline
- Reverse FK navigation — view all rows in a child table referencing the current row
- Update dialog now shows a proper "What's New" changelog with sections per update

### Bug Fixes
- Fixed query filter not applying correctly on certain column types
- Fixed table-query building incorrect WHERE clause for nullable columns
- Fixed row expand viewer not reflecting cell edits immediately
- Fixed FK dialog not refreshing after schema changes

### Changes
- DataTable split into canvas renderer + geometry helpers for maintainability
- SQL console result panel now persists last query result across tab switches

---

## [0.3.2] - 2026-06-02

### New Features
- JSON cell lightbox — view large JSON values full-screen with syntax highlighting
- Row expand viewer for inspecting all columns of a row in a side panel
- Structure view shows indexes, constraints, and triggers per table

### Bug Fixes
- Fixed slow query performance on large schemas
- Fixed status bar not updating after table switch
- Fixed command palette search not matching table names with underscores

### Changes
- Database switcher redesigned for clarity
- Foreign key dialog now shows column mappings inline
- Sidebar table list performance improved for schemas with 500+ tables
- Sonner toast positioning adjusted to avoid overlap with status bar

---

## [0.3.1] - 2026-06-01

### New Features
- Tab bar with per-tab state — switch between multiple tables without losing scroll/filter state

### Bug Fixes
- Fixed table loading spinner not dismissing on empty result sets
- Fixed structure view crashing on tables with no primary key

### Changes
- StudioShell refactored for faster tab routing

---

## [0.3.0] - 2026-06-01

### New Features
- Charts & Dashboard page — visualize query results as bar, line, pie, and scatter charts
- Diagrams page with Mermaid ER diagram viewer — auto-generated from schema
- AI Chat redesigned with conversation history and query suggestion chips
- Cloudflare D1 authentication flow added
- Command palette overhauled — full-width rows, keyboard shortcut badges, grouped results

### Bug Fixes
- Fixed EGL rendering crash on Wayland (Linux)
- Fixed chart re-renders causing CPU spikes

### Changes
- App shell layout refactored to support tabbed side panels
- Status bar now shows active connection and row count at all times

---

## [0.2.2] - 2026-05-29

### Bug Fixes
- Fixed chart rendering causing CPU overload — disabled aspect ratio maintenance on resize
- Fixed virtualized row rendering dropping rows at certain scroll speeds

### Changes
- Query execution pipeline optimized — large result sets stream in chunks

---

## [0.2.0] - 2026-05-28

### New Features
- Comprehensive rendering and memory optimizations for the data table
- X11 fallback added for EGL stability on Linux

### Bug Fixes
- Fixed EGL_BAD_ALLOC crash on Wayland with WebKitGTK
- Fixed AUR package build missing `patchelf` dependency

---

## [0.1.22] - 2026-05-28

### New Features
- UI styling pass — refined spacing, typography, and color tokens across all panels

### Bug Fixes
- Fixed multiple Svelte reactivity warnings causing unnecessary re-renders

---

## [0.1.12] - 2026-05-24

### New Features
- Built-in MCP server — expose your connected database to AI agents (Cursor, VS Code, Claude)
- One-click deep links to install the MCP server into Cursor and VS Code
- MCP server toggle moved to Settings (off by default)
- Themes — light, dark, and system-follow support

### Bug Fixes
- Fixed `latest.json` generation in release workflow — updater now works on all platforms
- Fixed URL opener blocking `cursor://` and `vscode:` protocol links

---

## [0.1.11] - 2026-05-24

### New Features
- AI can now fix SQL errors automatically when a query fails
- Manual "Check for updates" action added to command palette

---

## [0.1.7] - 2026-05-24

### New Features
- URL preview and media lightbox for URL-type columns in the data table

---

## [0.1.5] - 2026-05-23

### Bug Fixes
- macOS ad-hoc signing added — Gatekeeper workaround instructions in release notes

---

## [0.1.2] - 2026-05-21

### New Features
- Table filters — filter by any column with eq/neq/contains/gt/lt operators
- Foreign key navigation — click FK values to jump to the referenced row
- SQL editor improvements — syntax highlighting, history, saved queries
- GitHub Actions release workflow — automated builds for Linux, Windows, and macOS
