### New Features

#### Terminal
- A Terminal tab runs the connection's own command-line client in the app: psql, mysql or mariadb, sqlite3, sqlcmd and redis-cli. Every meta-command, completion and your own .psqlrc work as they do in a terminal. Ctrl+` opens it and goes back
- Signs in with the saved connection: the password goes through the environment, never the command line, and the SSH tunnel is reused
- Suggestions as you type: the client's commands with what each does, your tables and columns, and SQL keywords. A command from another database is translated, so `show databases` in psql offers `\l`
- Enter runs a statement that looks finished and adds the `;`, Shift+Enter starts a new line, and the footer says when a statement is still open
- Wide results print one record per block instead of wrapping, tables draw with box lines, and NULL shows as ∅
- Error, warning and notice lines are coloured; quick commands sit in the footer
- Editor keys work: Ctrl+Backspace and Alt+Backspace delete a word, Ctrl+A selects the line, Ctrl+Z undoes, Home and End move to the ends
- When the client is not installed, the tab shows the install command for your system
