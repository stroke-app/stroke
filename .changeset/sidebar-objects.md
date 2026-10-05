### New Features

#### Sidebar
- **Objects tab** - The Views tab is now Objects: one tree of the schema's views, materialized views, functions, procedures, triggers, sequences, types and events, whichever the engine keeps (Postgres, MySQL and MariaDB, SQL Server, SQLite, D1, libSQL, DuckDB, ClickHouse). Kinds with nothing in them stay out of the way, and the sidebar's filter box filters every group at once with the match highlighted
- **Extension functions under their extension** - pgvector's and pg_trgm's functions are listed under their own node inside Functions, so the count matches what the database holds, and every overload is its own row with its argument types
- **Definitions in an editor** - Enter or a double-click opens a function, procedure, trigger, sequence or type as the statement that recreates it (CREATE OR REPLACE where the engine has one); the context menu copies the name or the definition and drops the object, with the exact DROP shown first
- **New from a template** - New in the tab's header, or + on a group, opens an editor on a CREATE template for that kind and engine; Tab walks its name, arguments and body
- **Fast at any size** - The tree draws only the rows in view, so pg_catalog's 3,000 functions scroll and filter as smoothly as twenty, and it reads like a native tree: arrows, Left and Right to fold, type-ahead, Home and End, Shift+F10 for the menu
- **Sidebar settings** - Settings → Appearance: comments from the catalog next to table, view and routine names, and whether the Objects tab remembers which groups were open

### Bug Fixes
- The sidebar refreshes its Objects groups after a CREATE, DROP or ALTER of a function, procedure, trigger, view, sequence, type or event in the SQL editor
- MySQL runs CREATE / DROP PROCEDURE, FUNCTION, TRIGGER and EVENT from the SQL editor instead of failing with "This command is not supported in the prepared statement protocol yet"
- A routine or trigger body with statements of its own (`BEGIN ... END` in MySQL, SQLite, SQL Server and Postgres `BEGIN ATOMIC`) runs as one statement instead of being cut at its first semicolon
