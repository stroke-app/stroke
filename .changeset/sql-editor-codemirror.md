### New Features

#### SQL Editor
- **CodeMirror SQL editor** - The console and notebook cells run on CodeMirror, the same editor as the cell dock: faster to open and type in, with no suggestion list landing far from the caret
- **Schema-aware suggestions** - Tables after FROM / UPDATE / INTO, the named table's columns (with their types) first in SELECT / WHERE / SET, `alias.` and `schema.` lookups, functions with fill-in arguments, enum values and your own functions
- **Columns of any table** - A table you have never opened still gets its columns suggested: the schema's columns load on first need
- **Suggestions inside quotes** - Typing `"` lists column names; picking one steps past the closing quote
- **Better snippets** - 34 snippets named after their keyword (`sel`, `ins`, `upd`, `join` find theirs), inserted on one line, with a preview beside the list; a SELECT asks for its table first, so its column fields suggest that table's columns, and a field stays quiet until something is typed over it
- **SQL editor settings** - Settings → Database → SQL editor: text size, line wrap, line numbers, fold arrows, suggestions while typing and problem markers
- **Editor options in the toolbar** - The same switches sit behind the sliders button at the right end of the console toolbar; Alt+Z turns line wrap on and off
- **Run progress in the gutter** - A spinner sits beside each statement while it runs and turns into a ✓ when it finishes
- **Console toolbar** - Run comes first and Stop takes its exact place, so nothing moves when a run starts; the tools are grouped (format, explain, copy as ORM, then save and history)

#### AI
- **A lean request per turn** - The chart, diagram and schema-design skills and their tools ride along only once the conversation asks for them, the table list is one line, the rules are half their length, and old query results are elided from the history before each request. A plain question went from ~8k tokens of prompt and tool schemas to ~2k, which is most of the wait for the first token on the free tier. The context panel now counts the tool schemas too, and names the compression point it really uses
- **No second model call per turn** - The conversation title comes from the first message instead of another request, which is what the free tier's rate limit kept tripping on
- **Chat type follows the app** - The chat and code sizes scale with the app zoom, headings and tables are relative to the chat size, and code and result tables use the app's mono font

#### Data Model (was ER Diagram)
- **Renamed** - The ER diagram is the Data Model: one tab with five views of the same tables, Diagram, Mermaid, Tree, Dictionary and DDL, picked from a dropdown at the right of the toolbar. Menu, command palette, settings, the table toolbar and the sidebar say Data model
- **Dictionary view** - Every column on the page in one list: type, nullability, PK / FK / UK marks and what a key references, filtered by any of those; a table or reference name jumps to its card on the diagram
- **DDL view** - The CREATE statements of the tables on the page as the database reports them, read-only in CodeMirror, with copy and export to a .sql file
- **Mermaid code pane folds away**, leaving the preview the whole width
- **Mermaid colours and spacing** - Every colour the renderer uses is pinned on the SVG, so edge labels no longer pick up the app's muted fill (invisible on dark) and arrows no longer pick up its accent; the relation flow and the ER preview get more room between nodes and layers
- **Relation tree, redrawn to the design system** - The table is a plain card with one primary action and its columns behind a disclosure; each relationship is a list row (table name, column pair underneath, row count, columns toggle, expand, open) with children nested under a hairline instead of tinted boxes; List is the default, Flow has a Hops control, All draws every table on the page in one flowchart with hub links left out and counted, and the table list folds away
- **Hub tables stop drawing a line from every card** - A table most of the schema points at (`tenants`, `users`) is named in a pill on each referencing row (`→ tenants`) and counts its referrers on its header, instead of sending a line from nearly every card to one place. Hover or select a card and its hub lines appear; move off and the page is clean. The layout forms its clusters around the links that remain. Settings → Hub links → Lines draws everything as before
- **Mermaid view** - The page as `erDiagram` source in a CodeMirror editor beside a live preview with zoom and fit; the code follows the scope, filter and keys-only setting until it is edited, can be copied, and "Edit in Diagrams" saves it as a diagram of the connection and opens it there
- **Tree view** - The relation tree, fed from the tables the page already loaded: pick a table on the left, see what it points at and what points at it as a flowchart (one or two hops, zoomable) or as the list with row counts. The standalone Relation Tree tab gets the same Flow / List switch
- **Chrome** - Filter chips are neutral instead of tinted with the theme accent; Mermaid arrowheads are drawn in the line colour at a smaller size everywhere Mermaid renders, and the Diagrams page edits code in CodeMirror too
- **Table picker** - All / Linked / Related are chips with icons, rows keep their foreign-key count in a quiet pill, and the per-row "only" shows for the row under the pointer instead of twenty of them at once
- **Motion** - A re-layout glides the cards to their new places, fit, focus and the zoom buttons ease the camera, and Enter in the table search flies to the first match
- **A 135-table schema lays out in under a second** - It took 45 seconds: the layout engine's network-simplex placement at its highest effort. Brandes-Köpf placement at a lighter effort gives the same picture in a fraction of the time. The Mermaid preview and the whole-page tree map wait for a click above 60 and 80 tables, since those renders are seconds of work, while the code and the per-table flow stay instant
- **Full screen** - Every view of the data model goes full window (below the title bar) from a toolbar button; Esc ends it. The nav sidebar folds while the data model tab is active and returns when another tab is
- **A big schema reads as cards, not lines** - A hub is any table at least 15% of the page points at (or 25 tables, whatever the page size), so `tenants` on a 135-table schema folds into pills like it does on a 30-table one; at fit zoom the remaining lines recede to 30% so the cards are the subject, and above 60 links lines into one row share a trunk instead of a track each
- **Mermaid renders stay smooth** - A change to the code or the hops waits until typing stops before the diagram is redrawn, and the drawn SVG moves on its own compositor layer, so panning no longer repaints every glyph per frame
- **Primary keys come from the catalog** - Column structure rows carry an `isPrimaryKey` flag from every engine (Postgres constraints, MySQL `COLUMN_KEY`, PRAGMA `pk` on SQLite, D1 and LibSQL), so a `uuid` or text key is a key on the cards, in keys-only mode and in the Mermaid source; the old name-and-default guess only applies where an engine gives nothing
- **Mermaid keeps every relationship** - Hub links are in the `erDiagram` source too; the canvas is where they fold into pills
- **Key colours in Mermaid** - PK, FK and UK badges in the ER preview use the same amber and blue as the cards, and the focused table in a flow is set apart
- **Laid out and routed by one engine** - The diagram runs on ELK's layered algorithm: cards are placed and every relationship line is routed around them in the same pass, so a line never crosses a card, lines that share a channel get a track each instead of one pixel column, and a hub table no longer turns the schema into a vertical strip (cards per layer are capped, the fan spreads across layers). The whole layout lands at once, in a worker: no lines re-drawing themselves a few per frame after the cards appear

#### Canvas Table
- **Review changes in a dock** - The SQL for staged edits opens in a resizable panel under the grid instead of a dialog, so the rows being changed stay in view; the SQL follows the grid as you keep editing, and can be edited in place with completion
- **A single related row opens as JSON** - Following a foreign key into a sub view with one row opens that row as a JSON tree. Settings → Data grid → Expand single related row turns it off

### Bug Fixes
- A very large result no longer closes the app: a 5,011,000-row `SELECT *` used to come back as one message the window could not take. Postgres results now stream into the grid in chunks, so the first rows show right away and the count climbs while the rest arrive, with no row limit added
- A query in the console waits on half the network round trips on Postgres: 4 instead of 8 on a fresh connection, 3 instead of 5 for a repeat, so on a far database (Prisma Postgres, Neon, Supabase) a small query comes back in about half the time. Stop still cancels the statement on the server
- The connecting spinner turns with Reduce Motion on, and the connect screen no longer shows the form through it
- The Stop button's spinner is centred
- Format keeps short clauses on one line (`SELECT *` / `FROM users`), not four lines for a one-line query
- A query without a trailing `;` is no longer marked as a problem; a `;` missing between two queries is
- Line numbers stay aligned with their lines after zoom or a font change
- Scrolling with an expanded JSON row open ran at 11 to 20 fps: the grid re-measured its viewport on every scroll frame, which cancelled the scroll blit and re-laid-out every open panel. The measurement runs once, the blit runs again, and the panels move on the compositor
- The title bar no longer slides up under the window edge: the SQL editor's tooltip host made the page scrollable, and the page can no longer scroll at all
- Switching back to a tab with an expanded JSON row no longer jumps: the measured panel heights travel with the tab
- After zooming the app or opening a dock, the grid no longer shows torn rows and a blank band while scrolling: the scroll blit moved the whole canvas, including the stale part below the shorter viewport

### Changes
- `id`, `name`, `type` and other everyday column names are no longer coloured as keywords
