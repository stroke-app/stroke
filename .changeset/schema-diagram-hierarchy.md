### New Features

#### Schema Diagram
- **Hierarchy view** - The tables on the page top to bottom in foreign-key order, read like a roadmap: each table sits under the tables it points at, and lines leave a table as one trunk that branches to every table referencing it. Cards carry the table's estimated rows as a bar. Hovering or selecting a table lights everything above and below it, Enter in the find box steps through matches and centres each one, and lines into hub tables (`tenants`, `users`) can be left out and counted on the hub's card instead

#### Sidebar
- **Sort tables by when they were created** - Display options → Sort by → Created, newest first (click again for oldest first). MySQL, MariaDB and SQL Server sort by their creation time; Postgres, SQLite, D1, libSQL and DuckDB keep no such time, so they sort by the order the tables were created in

#### SQL Editor
- **Quick fixes** - Hovering a problem shows it in the editor's own styled tooltip with a fix button: Add ; at a statement's end or before the next one, Add ) for an unclosed bracket, Remove ) for a stray one, and close an unterminated string, name, comment or dollar quote. Ctrl+. (⌘. on macOS) applies the fix at the caret
- **Suggested `;`** - At the end of a statement that reads finished, a faint `;` sits after the caret and Tab writes it; it stays away from a statement still being written (after FROM, AND, a comma, an open bracket or string). Statement snippets (`sel`, `ins`, `upd`, `ct`...) now end with `;` when they finish the statement
- **Editing basics** - Settings → SQL editor gains tab size, indent with tabs, close brackets and quotes, Enter accepts a suggestion, highlight the current line, show whitespace and a blinking caret. A new line keeps the indentation of the line above, as in VS Code (Indent continuation lines turns the old behaviour back on), and Shift+Enter breaks the line the same way
- **Fold and unfold statements** - Fold arrows in the gutter (on by default, shown while the pointer is over it), Ctrl+Shift+[ / ] for the statement at the caret, Ctrl+Alt+[ / ] or the editor menu's Fold all / Unfold all for every statement; a multi-line statement folds to its first line

### Changes
- **Statement actions no longer move or hide the text** - In the at-caret mode the actions are a small `▶ Run ⋯` chip at the right of the statement's first line (Select, New tab, JSON, Variables and Ask AI behind ⋯) instead of a row of its own, so moving the caret no longer shifts every line; lines keep room for the chip, so a long one wraps short of it instead of running underneath
- **Variables panel** - One field per variable: its name as the prefix, the value in the middle and how it goes into the SQL (Auto, Text, Raw SQL, NULL) as a small menu at the end; Escape closes it
- **Objects tab rows** - Each kind has its own coloured icon (views, functions, procedures, triggers, types), groups read "Functions (68)", and a function row is its name and arguments, with the return type in the tooltip so long names are not cut short
- **Chat reads in a proportional face under the Mono font preset** - Replies, your messages and the message box use Inter there; chrome, code and table names stay monospace. Mentioned tables in the message box drop the schema in use, so several fit on a line
- **Assistant sidebar** - Starters sit just above the message box with an icon for what each one does; the SQL from the editor's Ask AI arrives as a removable card instead of a fenced block in the box (Enter on its own explains it, Backspace takes it off); sent SQL shows as code in the message bubble; the box is one surface without inner rules, and New chat, History and Close have clear icons and labels
- **Query toolbar** - Copy is a clipboard button with one menu for Copy SQL, Copy as Drizzle and Copy as Prisma, and Save is a labelled button at the trailing edge instead of a bookmark icon in the middle
- **Data Model is now Schema Diagram** - Same views under a plainer name, in the tab, the command palette, the menu bar, the sidebar's context menu and settings

### Bug Fixes
- The assistant is told what is open every turn, including when nothing is, so "explain the open table" no longer gets "I can't see your screen"
- Ctrl/⌘F focuses the Schema Diagram's search in every view (Mermaid and DDL open the editor's find) instead of the sidebar filter
- "N row(s) affected" shows for statements whose reply carries only a row count
- Row counts just under a million read 1M instead of 1000k, and counts past a billion read 1.2B instead of 1200M
