### New Features

#### Terminal
- A Terminal tab runs the database's own CLI in the app: psql, mysql, sqlite3, sqlcmd and redis-cli, signed in with the saved connection (Ctrl+`)
- Suggestions for commands, tables and columns, and `show databases` in psql offers `\l`
- Enter runs a finished statement and adds the `;`, Shift+Enter starts a new line
- Wide results print one record per block, and error lines show in red
- Shows the install command when the client is missing

#### SQL editor
- Colour themes for the code editor: One Dark, GitHub, Dracula, Monokai, Nord, Solarized, Tokyo Night, Catppuccin, Rosé Pine and Gruvbox
- Errors in plain words, with a one-click fix for a mistyped table, column or keyword
- Revert a single UPDATE, DELETE or INSERT, and schema changes such as CREATE TABLE or ADD COLUMN
- Completion follows the grammar: DROP TABLE offers IF EXISTS, ORDER offers BY, ON CONFLICT offers DO NOTHING

#### Updates
- Updates download in the background and install when you quit, and the status bar offers Restart once one is ready
- A What's new dialog after each update. Background downloads can be turned off in Settings, Updates

### Bug Fixes
- A SQLite or DuckDB connection with no file no longer loses its tables on disconnect. It asks for a file, and New creates one
- Duplicating or adding a row with page size All no longer drops the last row from the grid
- A row added with infinite scroll on no longer vanishes when more rows load
- A SELECT no longer shows its row count as rows affected

### Changes
- The new tab page shows which database it is on, with smaller tiles that now include Terminal and Search
- Command palette shortcuts show Ctrl on Windows and Linux instead of macOS symbols
- A narrower SQL editor gutter, and an error view laid out like the editor
- .deb, .rpm and Scoop installs still ask before updating
