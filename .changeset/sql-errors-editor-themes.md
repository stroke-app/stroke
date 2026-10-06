### New Features

#### SQL editor
- Pick a colour theme for the code editor in Settings, Appearance: One Dark, GitHub, Dracula, Monokai, Nord, Solarized, Tokyo Night, Catppuccin, Rosé Pine and Gruvbox
- A failed query says what went wrong in plain words, like "No table named orders", with the database's own message and line and column under it
- Did you mean: one click swaps a mistyped table, column or keyword for the closest real one, quoted when Postgres needs it
- Revert a single UPDATE, DELETE or INSERT from its statement's Revert button, on Postgres, MySQL, MariaDB and SQLite (inserts on Postgres). Rows changed since are left alone
- Revert schema changes too: CREATE TABLE, VIEW, INDEX, SEQUENCE or SCHEMA drops what it made, ADD COLUMN drops the column, a rename renames back
- Completion follows the grammar: DROP TABLE offers IF EXISTS then the tables, ALTER TABLE its actions and the table's columns, ORDER offers BY, IS offers NOT NULL, ON CONFLICT offers DO NOTHING. Where the next word is certain the list opens after a space

### Bug Fixes
- A SELECT run in the console no longer shows its row count as rows affected

### Changes
- The editor gutter is about a third narrower: the run mark and the fold arrow share one column, and the arrow shows on hover
- The query error view is laid out like the editor, without the boxed ERROR label
