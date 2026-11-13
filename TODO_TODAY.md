# TODO today: performance pass

Working notes so I can pick this up on another machine. Branch:
`perf/render-and-memory`, based on `feat/advisor-and-large-table-performance`
(not on `master`, because these changes build on the advisor/windowing work
already on that branch).

## Why I started

The app felt laggy and low-framerate across the board, the data table worst of
all. I profiled it instead of guessing, using a temporary in-page probe that
POSTed frame times and canvas call counts to a local sink, plus a remote-eval
channel so I could drive the running app from the shell. All of that
instrumentation is removed again; nothing in this branch ships it.

Test data: local Postgres on `127.0.0.1:5441` (`stroke_vec`), tables `events`
(1M rows), `articles` (100k), `openai_docs` (10k, pgvector embeddings).
Display is 60Hz, so the frame budget is 16.6ms.

## What the numbers actually said

The canvas render path was already well optimised for wide-but-plain tables.
`events` held a clean 60fps before I touched anything (p95 17ms, draw ~6.5ms).
So the general "everything is slow" feeling was not the row count.

Two things were real and measured:

1. **Text-heavy tables dropped frames.** `openai_docs` ran at p50 18ms /
   p95 22ms / max 30ms, i.e. 45-55fps. `measureText` was being called 113 times
   per frame, shaping 16,014 characters through HarfBuzz, costing 3.08ms of an
   11.7ms draw. It was recomputing the identical truncation every single frame.

2. **Memory was never given back.** Opening one 1M-row table took the app from
   45MB/48MB (idle baseline, measured against a second dev instance) to 587MB
   Rust / 891MB WebKit. Closing every tab released nothing. A full webview
   reload released nothing. On the Rust side the cause was visible in
   `/proc/PID/smaps`: four ~50MB glibc malloc arenas, one per worker thread,
   retained for the rest of the session.

## Done (in this branch)

- **Vite dep-optimizer crash.** `canvas-confetti` is only reachable through a
  lazy `await import()` on the license pages, so Vite's initial scan missed it.
  Discovering it mid-session re-ran the optimizer, rehashed every pre-bundled
  chunk, and the webview then 404'd on the old `runtime-*` / `index-client-*` /
  `Icon-*` paths. Losing the Svelte runtime chunk killed every page at once.
  Declared it in `optimizeDeps.include`. `vite.config.js`.

- **Truncation cache.** `truncText` now memoises by (font, column width,
  string) in `_truncCache`. Measured on `openai_docs`: `measureText` 113 -> 31
  calls/frame, 16,014 -> 1,521 chars/frame, 3.08ms -> 0.46ms; draw 11.7ms ->
  8.7ms; frames p50 18ms -> 17ms, p95 22ms -> 17ms, max 30ms -> 20ms, and only
  1-3 of 118 frames now exceed 18ms. This is the single biggest win here.
  Deliberately survives a `rows` swap (that is what keeps windowed scrolling on
  cache hits) and is dropped on a column change. `DataTable.svelte`.

- **Per-column type predicates moved into `_colCache`.** `isBooleanType`,
  `isSqlArrayType`, `isVectorType`, `isGeometryType` and the right-alignment
  test were each running per cell per frame: four regexes and roughly six
  throwaway strings per cell, about 40,000 regex executions a second while
  scrolling, all of it a pure function of the column's declared type.
  `DataTable.svelte`.

- **Frame-constant draw context.** `drawCell` was reading about 20 values
  straight off the reactive graph per cell. A `$derived` read is not a property
  access, it re-checks the derived against every dependency's write version, so
  a 150-cell viewport paid roughly 3,000 graph reads a frame for values that
  cannot change mid-frame. They are now snapshotted once per frame into the
  existing `bodyC` object. `DataTable.svelte`.

- **Row loop invariants.** `geom.cols.length` was in the loop condition, so the
  `geom` derived was re-entered once per column per row. The pinned-column pass
  also scanned every column of every row even with nothing pinned; pinned
  columns are now collected once per frame and the loop is skipped entirely when
  the list is empty. `DataTable.svelte`.

- **mimalloc as global allocator.** `src-tauri/src/lib.rs` +
  `src-tauri/Cargo.toml`. Same open-scroll-seek workload as before, measured:
  Rust retained growth **527MB -> 27MB**. This is the fix for the process never
  shrinking after you move on from a big table.

- **Dev-profile optimisation for the IPC path.** `serde_json`, `serde`, `itoa`,
  `ryu`, the four `sqlx` crates, `rust_decimal`, `chrono`, `uuid`, `base64` now
  build at `opt-level = 3` in the dev profile. Every command result is
  serialised to JSON for the webview and a browse is megabytes of it; at
  opt-level 0 that generated code is roughly 20x slower than release, so a dev
  build spent most of a row fetch inside serialisation rather than in the
  database. These are dependencies, so it costs nothing on incremental rebuilds
  of our own code. Follows the existing crypto block in the same file.

- **Reverse-FK request storm.** `loadIncomingForeignKeys` checked the cache
  before awaiting but only populated it after, so restoring a session fired one
  query per restored tab at the same instant. Measured nine concurrent calls
  totalling 37.7s of backend time against D1, where each call is several HTTP
  round trips to Cloudflare; everything the user was actually waiting for queued
  behind them. Added in-flight dedup plus a generation counter so a reply from
  before a connection change is discarded instead of writing another database's
  FKs into the cache under the same `schema.table` key. `StudioShell.svelte`.

- **`matchMedia` off the wheel path.** `createSmoothScroll` called
  `matchMedia('(prefers-reduced-motion: reduce)')` on every wheel tick, about
  5us of style resolution each time, on the one path that must not touch the
  style system. Resolved once and kept live by the MediaQueryList.
  `smooth-scroll.js`.

- **Monaco entry trimmed to the languages the app has.** `monaco-editor`'s
  default entry is an everything-bundle: 80-odd basic languages plus the css,
  html, json and typescript services, each service dragging its own worker. The
  app displays six languages, and the blocker last time was not being able to
  prove that - `MonacoTextView` takes `language` as a prop. It is provable: the
  only two dynamic call sites are `OrmSchemaPage` (`sql` | `rust` for Prisma |
  `typescript`) and `TableTextView` (`stroke-csv`, `stroke-tsv`, `markdown`,
  `json`), and every other site passes a literal. So the full set is sql, json,
  javascript, typescript, rust, markdown - note `markdown`, which the earlier
  list here missed. `src/lib/monaco.js` now composes the entry by hand out of
  the same pieces `editor.main.js` uses: `edcore.main.js` (all editor features,
  no languages), the six language contributions, and the json + typescript
  services republished on `monaco.languages.*` by hand, which `editor.main.js`
  does itself and the contributions do not. All 11 importers now go through it.
  Measured over `dist/assets`: **34,821,807 -> 32,585,794 bytes, -2.24MB
  (-6.4%)**, 81 chunks gone, `css.worker` (1.05MB) and `html.worker` (719KB)
  among them - both were being emitted for grammars nothing in the app can open.
  `ts.worker` stays: `OrmRunner` really does use
  `languages.typescript.javascriptDefaults` for its IntelliSense.

- **The startup prefetch is engine-aware, and the tail is staying.** Measured
  properly this time, on a release build, against the manifest's static import
  graph (warming a chunk pulls its static imports; its own dynamic imports stay
  lazy). The eager entry graph is 2.71MB/25 chunks and the warm set adds
  6.01MB/49 - but the shape is nothing like the guess above. 5.35MB of it is two
  entries: `SqlConsole` pulls monaco (3.78MB) and `AiChat` the markdown/highlight
  stack (1.57MB). The other 22 pages cost 0.66MB *between them*, 0.01-0.11MB
  each. So "trim the tail" buys ~nothing and would regress first-open latency on
  21 pages to save half a megabyte; the tail stays, and the note explaining why
  is now in the code so nobody re-derives it.
  What the measurement does justify is the engine gate: a warmed chunk is never
  freed, and on a Redis connection every relational page is unreachable UI -
  monaco included - so ~4.3MB of that 6.01MB was being pinned for tabs that do
  not exist. The list is now per-entry gated on the same `isRedis` /
  `hasSchemaExplorer` / `hasSecurity` flags the tab affordances use, it waits for
  a connection (before one exists the engine is unknown and no tab can open
  anyway), and `RedisKeyspacePage` warms only on Redis, where it goes first.
  `StudioShell.svelte`.

- **Scroll blitting.** The remaining draw cost was `fillText`, and it is now
  skipped for every row that did not change. On a frame that differs from the
  last one only by a vertical scroll, `drawImage` moves the still-valid band and
  only the strip the scroll uncovered is repainted - a one-row step paints one
  row instead of every visible one. `DataTable.svelte`.
  The safety argument matters more than the mechanism here:
  - The scroll loop already distinguished a position-only frame from a content
    change (`_loopNeedsDraw`, set by `scheduleDraw`, which every content, hover,
    focus, selection, theme and geometry change goes through). That existing flag
    is the gate - nothing new had to be invented to know when a blit is legal.
  - draw() adds the geometric half: no animating skeletons, no insert draft, no
    expanded rows (non-uniform row heights), integer dy, and dy and the header
    offset both landing on whole device pixels.
  - The copy runs with the transform reset, source and destination the same
    size, so it is an exact 1:1 move of whole device pixels. Going through the
    DPR-scaled transform would let a fractional viewport width or a 1.5x DPR
    resample the band, and since each frame copies the last, that blur would
    compound over a scroll into smeared text. This is the trap in this approach.
  - The strip is rounded outward and the repaint band is widened a pixel at each
    edge, so a row separator bleeding outside its own band cannot leave a seam.
  - The loop now forces a full repaint when scrolling stops. Any artifact this
    path could produce therefore lives only while content is moving and is gone
    within one frame of the user letting go - it can never persist in a resting
    grid.
  The blit arithmetic is verified exhaustively rather than by eye: across 2,280
  (DPR, header height, viewport height, dy) combinations, every body pixel in the
  new frame is either copied from exactly the right source row or falls inside
  the repainted strip. What I could *not* do is drive a live scroll and look at
  it, so the pixel-level "does it look right" check is still owed.

- **Rust verification (was item 1).** `cargo check --no-default-features` passes
  clean; `tauri:build` succeeds on macOS (3m00s, both bundles produced), so
  mimalloc is exercised in the release profile and not just in dev - and this
  branch is running on it. Windows is still untried.

Verification: `npx vitest run` is 467/467 green, `npx vite build` is clean,
`cargo check --no-default-features` passes, and the app runs with all of this in
place. The canvas draw path has no unit coverage, so the blit is covered by the
geometry check described above rather than by the suite.

## Remaining

Ordered by expected payoff.

1. **Look at a live scroll.** The blit is verified arithmetically (2,280
   configurations, no uncovered or mis-sourced pixel) and every gate is
   conservative, but nobody has watched a real grid scroll with it on. Open a
   text-heavy table, scroll hard in both directions, and check the seam where the
   strip meets the blitted band, the hover highlight, the focused-cell ring and
   the frozen columns. Then re-run the frame-time probe: the prediction is
   `fillText` ~167/frame -> ~10 during a smooth scroll, so a draw around 4ms
   instead of 8.7ms. If something is wrong it will be a band of stale rows during
   the scroll that snaps correct the moment you stop - that shape of bug means a
   missed `scheduleDraw`, i.e. some state changes the body without going through
   it.

2. **Windows.** Everything else on item 1 is closed: `cargo check
   --no-default-features` is clean and `tauri:build` succeeds on macOS in 3m00s,
   bundling both `Stroke.app` and `Stroke_1.22.0_aarch64.dmg`, so mimalloc is
   fine in the release profile on macOS as well as Linux. (Notarization is
   skipped locally for want of `APPLE_ID` / `APPLE_API_KEY` in the environment -
   that is the machine, not the build.) Windows is the one platform nobody has
   run this on. It is a one-line global allocator swap, so the expectation is
   that CI just goes green, but it should be watched on the first Windows build.

3. **WebKit's idle memory, unexplained.** Unchanged from before. A fresh process
   with no table open and the chunk prefetch disabled still sat at 370MB Rust /
   836-1038MB WebKit, with only 505 DOM nodes and 246 modules loaded. `smaps`
   shows it is all allocator memory (234MB `[heap]` plus 346MB across 6,215
   anonymous mappings), not DOM/JS I can point at. Next step is a clean run with
   no instrumentation at all, and a release build for comparison, since a lot of
   it may just be dev-mode module overhead. The 2.24MB the Monaco trim removed
   and the ~4.3MB the engine gate stops pinning on Redis both come off this
   number, but neither is close to explaining it.

4. **Eased scrolling is the default** (`nativeScroll: false`). It puts a
   non-passive wheel listener in front of every tick and animates `scrollTop`
   from JS across roughly 20 frames per notch, reading `scrollHeight` each frame
   (a forced layout, measured at 15us). The repo's own notes already flag this
   mechanism as a stutter source. It measured small, and it is a deliberate
   product choice with a setting behind it, so the default is untouched. Worth an
   A/B on a slower machine. Note it interacts with the blit now: those ~20
   small-step frames per notch are exactly the case the blit is best at, so this
   may have got cheaper on its own.

5. **`[will-change:transform]` on nine DOM scroll containers.** Unchanged. The
   repo's own DataTable notes say that promoting a scroller like this created a
   heavy compositing layer that lagged vertical scroll, and it is the reason
   `RowExpandViewer` has to portal its context menu out. No promoted layers were
   live at idle when I checked, so I could not measure a cost and left it. Needs
   a real before/after on a long list.

6. **Monaco's remaining weight.** `ts.worker` is still 6.9MB and `editor.api2`
   2.59MB. The worker is genuinely used (`OrmRunner` IntelliSense) so it cannot
   just go, but it is loaded for anyone who opens the ORM runner once. Worth
   checking whether the ORM runner needs full type-checking or only completions.
   Separately, the largest chunks in `dist` are no longer Monaco's at all -
   `brain` (1.53MB), `emacs-lisp` (780KB), `cpp` (626KB), `wasm` (622KB) are
   Shiki grammars, and the app highlights nothing like that many languages. That
   is the same trim, one library over, and probably a bigger win than what is
   left in Monaco.

## Things I ruled out, so nobody re-checks them

- No CSS animations run at idle. `document.getAnimations()` returned 0 running,
  so there is no continuous compositor churn from a stray spinner or shimmer.
- Only one element carries `will-change` at idle (the sidebar,
  `scroll-position`), so the app is not drowning in composited layers.
- Byte-aware row windowing works. `events` fetched in 9 windows over a
  scroll-and-seek workload rather than one giant response.
- The per-frame canvas call counts are fine outside text: 35 `fillRect` and 70
  `stroke` per frame. Rects and lines are not the problem, glyphs are.

## How to reproduce the measurements

The probe is gone, but it was small: a module imported first from `src/main.js`
that `fetch`-POSTed frame-time percentiles and canvas call counts to a local
python sink on 127.0.0.1:9911, and polled the same server for a JS snippet to
`new Function`-eval inside the page. That eval channel is what let me open
tables, dispatch synthetic `WheelEvent`s and read results back without touching
the GUI, which mattered because `hyprctl dispatch` is broken on this machine by
a lua plugin and no input tool could drive the window.

To count canvas work, patch `CanvasRenderingContext2D.prototype` from the eval
channel (`measureText`, `fillText`, `fillRect`, `stroke`, and the `font`
setter), reset the counters, dispatch ~100 wheel events one per animation frame,
then divide by the draw count. Per-frame counts are the useful unit, not totals.

## Themes: follow-ups (2026-11-12)

Glass (`glass-dark` / `glass-light`) and the Custom theme with opacity below
100% are macOS-only today: `set_window_glass` in `src-tauri/src/commands.rs`
is a no-op elsewhere, and every glass rule in `src/lib/themes/app-themes.css`
is scoped to `html[data-os="macos"]`.

- [ ] **Glass on Windows (Mica / Acrylic).** Needs testing on a real Windows
      build (`npm run tauri:build:win`), not just a compile.
  - Create the window with `.transparent(true)` on Windows too
    (`src-tauri/src/lib.rs`, the window builder).
  - In `set_window_glass`, add a Windows branch: `Effect::Mica`, with
    `Effect::Acrylic` as a fallback before Windows 11.
  - Let the WebView2 backdrop go transparent while glass is on.
    `set_windows_webview_backdrop` in `lib.rs` currently forces alpha to opaque.
  - Widen the CSS scope to `html:is([data-os="macos"], [data-os="windows"])`.
  - Set `data-glass` on Windows too: `syncWindowGlass` in
    `src/lib/stores/settings.js` only does it for macOS.
  - Risk: the Windows window is hidden until first paint specifically to avoid
    a startup flicker. Re-check that it still opens cleanly.
- [ ] **Page-by-page pass in Glass and in Custom with opacity below 100%.**
      The structural cases are already fixed:
  - sticky headers, overlays and fixed panels (frosted rule in
    `app-themes.css`)
  - the data grid's erase fills and `withAlpha`
  - the terminal
  - the ERD and map colour probes

  What's left are one-off components that paint their own solid colour and
  stand out as patches over the glass. Go through each page, screenshot the
  odd ones, and fix them per component.
