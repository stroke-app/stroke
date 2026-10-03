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
