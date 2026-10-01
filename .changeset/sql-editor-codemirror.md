### New Features

#### SQL Editor
- **CodeMirror SQL editor** - The console and notebook cells run on CodeMirror, the same editor as the cell dock: faster to open and type in, with no suggestion list landing far from the caret
- **Schema-aware suggestions** - Tables after FROM / UPDATE / INTO, the named table's columns (with their types) first in SELECT / WHERE / SET, `alias.` and `schema.` lookups, functions with fill-in arguments, enum values and your own functions
- **Columns of any table** - A table you have never opened still gets its columns suggested: the schema's columns load on first need
- **Suggestions inside quotes** - Typing `"` lists column names; picking one steps past the closing quote
- **Better snippets** - 34 snippets named after their keyword (`sel`, `ins`, `upd`, `join` find theirs), inserted on one line, with a preview beside the list; each field opens its own suggestions, and Tab moves to the next
- **SQL editor settings** - Settings → Database → SQL editor: text size, line wrap, line numbers, fold arrows, suggestions while typing and problem markers

#### Canvas Table
- **Review changes in a dock** - The SQL for staged edits opens in a resizable panel under the grid instead of a dialog, so the rows being changed stay in view; the SQL follows the grid as you keep editing, and can be edited in place with completion

### Bug Fixes
- The connecting spinner turns with Reduce Motion on, and the connect screen no longer shows the form through it
- The Stop button's spinner is centred
- Format keeps short clauses on one line (`SELECT *` / `FROM users`), not four lines for a one-line query
- A query without a trailing `;` is no longer marked as a problem; a `;` missing between two queries is
- Line numbers stay aligned with their lines after zoom or a font change

### Changes
- `id`, `name`, `type` and other everyday column names are no longer coloured as keywords
