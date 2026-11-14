<!--
Target `nightly` (the default). Merging bumps the version by one patch; label
`release:minor` / `release:major`, or `release:skip` for no bump. Only hotfixes
target `master`.
-->

## What this changes

<!-- What is different afterwards, and why it is worth doing. -->

## Why

<!-- The problem. If it fixes an issue: Fixes #123 -->

## How it was checked

<!-- What you ran, and what you saw. Delete what does not apply. -->

- [ ] `npm test`
- [ ] `npm run build`
- [ ] `cargo check` (in `src-tauri/`)
- [ ] Ran the app and used the change against a real database

<!--
Engine-specific? Say which you tested on. Stroke supports eleven, and a change
that is right for Postgres is not automatically right for SQLite or D1.
-->
