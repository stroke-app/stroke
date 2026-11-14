---
name: ship-safely
description: Pre-release gate for Stroke that catches the class of bug that made v2.1.0 unquittable - Tauri ACL/capability gaps, silently swallowed IPC failures, and critical paths nobody exercised in a real build. Use before merging a `release:*` PR, after touching src-tauri/capabilities/*, window lifecycle or close/quit code in src-tauri/src/lib.rs, WindowControls.svelte or TitleBar.svelte, and whenever a change removes a handler, a tray, a menu item or a shortcut.
---

# Ship safely

## Quick start

```bash
node .claude/skills/ship-safely/scripts/check-tauri-acl.mjs .
```

Exits non-zero and names the gap. Needs `src-tauri/gen/schemas/acl-manifests.json`,
which a Tauri build writes, so run `npm run tauri dev` once on a fresh clone.

## What went wrong in v2.1.0

The capability file granted `core:window:allow-close` but never
`core:window:allow-destroy`. That gap sat there harmlessly for months because
the tray build intercepted `CloseRequested` and hid the window, so `close()`
never ran through to `destroy`. Removing the tray let it through, the ACL
refused the call, and the rejected promise went nowhere. The window is
frameless with no tray, no File > Quit and no quit shortcut, so the app shipped
with no way to close it.

Three separate failures lined up. The gate below covers each one.

## 1. Permissions live in a file the code does not mention

`src-tauri/capabilities/default.json` is the only thing standing between a
frontend `getCurrentWindow().x()` call and a denial, and nothing in the calling
code points at it. Run the checker after any change to window handling, and
read its two rules:

- A JS call can require more than one permission. `close()` needs
  **both** `allow-close` and `allow-destroy`.
- `"windows"` is a list of label globs. `open_new_window` creates `main-2`
  onward, and a capability scoped to `["main"]` denies every core call from
  those windows, including dragging the title bar.

## 2. A denied core call is silent in release

The rejected promise had no handler, and release builds have no devtools, so
nothing surfaced. Any call into Tauri from the UI gets awaited inside
`try/catch` with a `console.error`, the way `WindowControls.svelte` does it now.
Never leave `void somePromise()` on a path the user depends on.

## 3. Nobody clicked the button

Static checks would have caught this one, but not the next one. Exercise the
path you changed in a running build before merging. Both of these work without
taking over the desktop, which matters because I am usually working on it:

- **Headless DOM.** With `npm run tauri dev` up, `chromium --headless=new
  --dump-dom --virtual-time-budget=9000 http://localhost:1420/` renders the real
  frontend. Grep the output for the state you expect.
- **HMR self-trigger.** Temporarily call the thing on mount, save, watch the dev
  log, revert. Back up the file first and restore from the backup, never by hand.

`pcvision` screenshots of a whole monitor catch whatever else is open. Target
the window, or use the two above.

## 4. Do not remove the last way out

Before deleting a tray, a menu item, a shortcut or a handler, count what is left
for that action. The tray removal was correct on its own and only became a
critical bug because it was the last fallback for quitting. If a change leaves
exactly one path to something the user cannot work without, say so in the PR.

## Releasing

Releases run four platform builds and the org is short on Actions minutes, so
**never merge a `release:*` PR unless I asked for a release in that message.**
Open the PR, say the release is waiting on me, and stop. See
`.changeset/README.md` for the changeset format.
