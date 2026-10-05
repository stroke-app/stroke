### New Features

#### SQL Editor
- **Queries in the sidebar** - A Queries tab lists the connection's saved queries as a tree with folders. Click opens a query in its own tab (or brings its tab forward), double-click runs it there, F2 renames it, and the context menu runs, duplicates, moves, copies or deletes it with Undo. Drag a query onto a folder to file it. New query opens a tab tied to "Untitled query N", so the first Ctrl+S saves it without asking for a name. Deleting a folder with queries in it moves them out unless you choose to delete them too
- **History, Saved and Charts in the results pane** - They open beside the result views instead of narrowing the editor, share one list across every editor tab, and work from the keyboard: arrows move, Enter loads, Delete removes with Undo, Esc goes back to the editor
- **Results beside the editor** - Settings → SQL editor can put the results to the right of the editor, open them when a query runs, and count a large table's rows exactly instead of estimating them (PostgreSQL)
- **Editor tabs are kept** - Every query editor tab comes back after a restart or a switch to another database and back, with its title, its text and the saved query it belongs to

### Bug Fixes
- Saving a query that was already saved filed a new copy every time; Ctrl+S now saves into it, and Ctrl+Shift+S saves a copy
- Ctrl+/ commented the line and also opened the keyboard shortcuts panel; Ctrl+W closed two tabs; Ctrl+J and Ctrl+Shift+B did nothing in the editor
- Loading from History in a second editor tab replaced the first tab's query
- History lists failed runs too, marked with their error, and a query run again moves up instead of being listed twice
- A script's duration in History was its last statement's
- Switching databases overwrote the other database's editor draft with `SELECT 1;`
