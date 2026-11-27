---
"stroke": patch
---

### Bug Fixes
- Stop in Find in database left each table's search running on the server, holding connections. Stopping, or starting a new search, now cancels them (Postgres).

### Changes
- The command menu opens in under 100 ms on Linux: it appears without a fade, over the page without dimming it.
- The sidebar's table list is one Tab stop: the arrow keys move through the tables, Tab leaves for the grid and Shift+Tab goes back to the filter.
- Applying a single cell edit saves it at once and offers Undo, which puts the old value back. Several changes, deletes and new rows still open the review first.
