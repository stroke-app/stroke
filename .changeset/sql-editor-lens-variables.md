### New Features

#### SQL Editor
- **Statement actions** - A row of Run, Select, New tab, JSON, Variables and Ask AI above the statement under the caret (or above every statement, in Settings → SQL editor → Statement actions). New tab runs the statement in a fresh editor tab, JSON runs it straight into the JSON view, and Ask AI opens the chat with the statement in its message box, ready for a question
- **Time and rows after each statement** - A statement that ran shows how long it took and what it returned after its `;`, as `478ms · 12 rows` or `35ms · 3 affected`. The note and the ✓ stay through edits elsewhere and go when that statement is edited
- **`$name` and `${name}` variables** - Alongside `:name`. The same name is one variable however it is written, the editor asks for missing values before the run, and Enter in the Variables panel runs the statement that was waiting. Dollar-quoted bodies, `$1` positional parameters, strings and comments are left alone, and so are SQL Server's `$action` and `$identity`
- **Variable values per tab** - Each editor tab keeps its own values across restarts, keyed by its saved query or its title, with the last value used anywhere filling the gaps. Settings → SQL editor → Remember variable values turns it off and clears what was kept
- **Add LIMIT to SELECT** - Off by default. When set (100 to 5,000), a run appends LIMIT to each SELECT that has no row limit of its own. Statements that write, lock or end in FORMAT or SETTINGS are left alone, as is SQL Server
- **Quote object names** - SQL the app writes for you (Open in SQL editor, Generate SQL) quotes table and column names only when needed, always, or never; "when needed" reads each engine's own reserved words and Postgres case folding. **Qualify tables with their schema** can leave out the default schema (public, dbo, main)
- **Statement highlight setting** - The band behind the statement under the caret can be switched off

### Bug Fixes
- Open in SQL editor quotes names with brackets on SQL Server instead of double quotes
