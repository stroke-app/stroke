### Bug Fixes

#### Terminal
- On Windows, editing keys in psql, mysql and sqlite3 no longer show up as `^W` or `^E^U`. Tab completes, Enter adds the missing `;` and Ctrl+L clears the screen
- On macOS, Ctrl+Backspace deletes a word, and Cmd+Left, Cmd+Right and Cmd+Backspace work at the prompt

#### AI
- A chart the model writes out as text is drawn instead of shown as raw JSON
- No more empty gaps in the chat where hidden query results were

#### App
- A long password no longer runs under the show button
- Tooltips always use the app's style, not the system one

### Changes
- Format SQL keeps a short statement on one line and breaks a long one clause by clause, in the connection's SQL dialect
- Open in SQL editor writes the query on one line and leaves out the default schema, so `campaigns` instead of `public.campaigns`
- Open in SQL editor is back on the table toolbar, and Ctrl+click runs the query too
- The AI takes the steps a request needs before it answers, and charts only real data
