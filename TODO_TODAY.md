# TODO

What is open as of 2026-11-14 (master at 2.5.5). Security items live in
`TODO_SECURITY.md`.

## Before the next stable release

- [ ] Test the 2.5.5 Windows build: theme switching and zoom without flicker,
      Ctrl+K rebound to Alt+K working in the SQL editor, the title bar with the
      sidebar on the right.
- [ ] Test offline Pro on macOS: sign in as Pro, turn Wi-Fi off, Pro stays on.
- [ ] Ship it: promote `nightly` to `master` (see CONTRIBUTING.md).

## Next

- [ ] **AI access levels for built-in models.** The Read only / Read & write
      control exists for local agents only (enforced by the MCP server). Built-in
      models (Stroke Free, API keys) can run writes, asking only before
      destructive ones. Enforce a level in `AiChat`'s `execute_sql` with the
      same classifier as `src-tauri/src/mcp/access.rs`, then show the control.
- [ ] **echarts 6** (PR #14): clears the echarts XSS advisory; the charts need
      testing.
- [ ] Decide on the old PRs: #105 (license wording), #91 (ship-safely gate),
      #56 and #54 (older feature/tooltip work, likely superseded).
- [ ] RHEL / Rocky / Alma 9 can't run Stroke (no WebKitGTK 4.1). Documented in
      the README and release notes; nothing to do unless that changes.

## Performance pass (paused)

The render and memory work is on `perf/render-and-memory` (last commit
2026-08-31, not merged, 14 commits ahead). Its notes are in that branch's
`TODO_TODAY.md`; several items there are obsolete since the editor moved
from Monaco to CodeMirror. Rebase onto `nightly` and re-measure before
picking it up.

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
