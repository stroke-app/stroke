### New Features

#### Sidebar
- Objects tab: views, functions, procedures, triggers, sequences, types and events in one tree, filterable and fast at any size
- Open any object as its CREATE statement, or start a new one from a template
- Queries tab: saved queries in folders. Click to open, double-click to run, drag to file
- Sort tables by when they were created

#### SQL editor
- Run, New tab, JSON and Ask AI on the statement under the caret, with its time and row count after the `;`
- `$name` and `${name}` variables, remembered per tab
- Quick fixes for a missing `;` or `)` and unclosed strings (Ctrl+. / ⌘.)
- Completion and checks for triggers, functions, procedures and views before they are created
- Fold statements, multiple cursors, and new editing settings (tab size, close brackets, current line)
- History, Saved and Charts open beside the results; editor tabs come back after a restart

#### Schema Diagram
- Hierarchy view: tables top to bottom in foreign-key order, with row counts and path highlighting

#### ORM runner
- Drizzle and Prisma completion

#### AI
- Shortcuts in replies show as keys, and inline code reads lighter

### Bug Fixes
- Stop and Esc always end an AI reply, and stopping no longer breaks the rest of the chat
- HTML in an AI reply shows as text instead of restyling or covering the app
- ⌘R refreshes the open table
- Shortcuts that fired twice (⌘/, ⌘W) or did nothing (⌘J, ⌘⇧B) work once and as listed
- Free AI switches to its other model when one is busy, and creates tables when asked
- AI charts draw whatever shape their data arrives in
- MySQL creates procedures, functions, triggers and events from the editor
- Saving an already-saved query updates it instead of filing a copy
- The sidebar keeps every tab and action at its narrowest width

### Changes
- Every editor runs on CodeMirror. Monaco is gone, and the app is 19 MB instead of 37 MB
- Faster start: 2.49 MB loads before the first paint instead of 3.05 MB
- Data Model is now Schema Diagram
- Updated devalue and DOMPurify for security advisories
