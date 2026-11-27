---
"stroke": patch
---

### New Features

#### Layout and motion
- **Inset content panel** - The page sits in a rounded panel, with the sidebar, title bar and status bar around it on one background. Settings, Appearance, Inset content panel turns it off.
- **Sliding sidebar** - Ctrl+B / ⌘B slides the sidebar out and back while the page moves into the space. Menus grow from the button that opened them and dialogs settle into place. On by default on macOS and Windows; Settings, Appearance, Animate panels. While the sidebar is hidden, a button at the start of the tab bar brings it back.
- **Back and forward** - Move through the tabs you visited with the ‹ › buttons at the start of the tab bar, or Ctrl+[ and Ctrl+] (⌘[ and ⌘]).
- **New tab button** - A + after the last tab.
- **Rename a query editor tab** - Double-click it, or right-click and Rename Tab. The name stays when the app reopens.

#### Keyboard and search
- **Shortcuts in tooltips** - Every tooltip shows its shortcut as keys, and the next tooltip along a toolbar appears at once.
- **Looser command search** - Type letters with some left out: "stng" finds Settings. The pages you picked last sit at the top of an empty command menu.
- **Table row keys** - On a focused sidebar table, Space peeks at its columns, P pins it, X selects it and Ctrl+C / ⌘C copies its name.
- **Recent tables** - The new tab page lists the tables you opened last.
- **Searchable tab list** - The menu at the end of the tab bar has a search field and opens on the tab in front.

### Bug Fixes
- Clearing AI chat history deleted every conversation with no way back. It offers Undo now, as clearing the activity log does. Key changes in the document and DynamoDB editors ask first.
- The sidebar filter shrank to a few letters beside the schema picker.
- Success, warning, error, info and accent text, and highlighted SQL, now read clearly in every theme.
- At the smallest window size the SQL results pane kept its width and its toolbar stayed whole.
- ? and Ctrl+? / ⌘? never opened the keyboard shortcuts.
- A connection that fails because nothing answered says where, instead of showing the driver's full report. The connection being opened shows a spinner in the sidebar, and clicking another one meanwhile says it is still connecting.
- / focuses the sidebar's table filter on pages without a search of their own (it went to the connection search).
- After clicking a button, the browser's own tooltip no longer shows over the menu it opened.
- The command menu kept its groups in a fixed order whatever you typed, so "ai" put Advisor first and Enter opened it. The best match now comes first, and two letters no longer match inside the middle of a hidden keyword.
- Back and forward stopped at pages that put the caret in their editor on their own (the SQL editor, an empty JSON viewer).
- Each snapshot on the Schema timeline shows its table count again.
- The theme list in the command menu opened at the top with the current theme out of sight, and every dropdown opened on its first row. Both open on the current choice now, scrolled into view.
- The theme picker previewed whatever rose to the top as you typed, so typing a name repainted the whole app at each letter. Only arrow keys and the pointer preview now.
- Settings searches and other search fields lit up purple instead of the accent colour on most Linux and Windows screens.
- The Resume button on the connect screen cut the connection name short with room to spare.
- In the sidebar's More menu, Recent's count sat where its shortcut belongs.
- Typing "queries" in Go to page opened Instance insights: a command whose keywords start with the word now ranks above one whose keywords end with it.
- After a new row was discarded, the next Add left the caret on the Add button. A new row also put the caret in a uuid key with a default instead of the first column it needs.
- Ctrl+Shift+6 showed Find & replace with the caret still in the grid.
- The AI model picker opened on Stroke Free instead of the model in use.
- A connection's hover card printed the engine's id (sqlite) instead of its name.
- With the database unreachable, a table refresh waited over a minute with every control greyed out, and a SQL Stop sat on Stopping. The refresh button stops a slow load, a first load offers Stop after 6 seconds, and Stop ends a run whose cancel goes unanswered.
- The Enums tab called every enum unused, and Find in database cut a long value before the match it found.
- The Enums tab and Database info: enum values read as one run of words, and a 1.4 GB database said 1444 MB.
- The New snapshot window had no focus, Escape or dialog role, and Enter in its title did nothing.
- After choosing an engine, the first key typed filtered the saved connections instead of going into Host.
- With the pointer resting over the command menu or a select menu, typing moved the highlight to whatever row slid under the pointer, and Enter picked that row. The best match keeps the highlight until the pointer moves.

### Changes
- Deleting a saved connection, AI chat, dashboard, dashboard card, saved chart, saved table view, virtual column, diagram or schema snapshot, or clearing the activity log or chat history, asks nothing and offers Undo for 8 seconds instead.
- Every dialog's main button is drawn in the accent colour.
- Toolbars group their buttons with space instead of thin dividers, and empty-state headings are larger and lighter.
- Shortcuts print the same way in every menu row, radio and checkbox rows included.
- On a new install, notifications appear in the bottom right corner, clear of the tab bar. Settings, Appearance moves them.
- A notification's Undo, or any other action, sits beside its message.
- Dialogs and the command menu are white in light themes (they took the grey of the page around the panel) and a step lighter than the page in dark ones. Every page in the content panel now shares one background; the AI view, Advisor and Diagrams sat a shade darker.
- The command menu, the Schema explorer and switching between tables are faster. Typing in the command menu takes about half the time per letter it did, and drilling into Tables or Connections starts with an empty search.
- Grid checkboxes show on the row you point at, and on every row once you select one.
- Empty pages share one design, with the first step as a button. Diagrams, Dashboard and Schema timeline hide their lists and toolbars until there is something in them.
- Picking a "Did you mean" name after a failed SELECT runs it again, so the error gives way to the rows. A write still waits for Run.
- Search tables, Go to page and the theme picker in the command menu take a third to half the time per letter, and moving through grid cells with the arrow keys does less work per key.
- Tooltips wait for the pointer to move, so a button that slides under a still cursor no longer pops one up.
- The Snippets settings and the result map use the app's own select menu instead of the system one.
- The sidebar's connections take arrow keys from the search, and the row with focus shows it.
- Closing a notebook with unsaved changes offers Undo.
- Data diff picks its connection, database, schema and table from the app's select menu, and every table row's menu offers Open in schema diagram.
- A new notebook opens with the caret in its cell, Recent table names use the full row width, and the peek card shows long table names whole.
- Searching Settings or the keyboard shortcuts matches at the start of words, so "port" no longer lists every Import and Export, and "tab" no longer lists Switch database.
- Both export menus list each format with a short description and without a repeated icon. Ctrl+Shift+E is named Toggle AI panel, which is what it opens.
- Menus and dialogs lost their leftover icon tiles and repeated row icons; the tab menu, Relations headers and Structure view's Unique column read like the rest of the app, and Structure says No default where a NOT NULL column has none.
- Empty Dashboard, Policies and Charts pages show one way to add the first item instead of two.
- Show / hide columns keeps the keyboard on the row just toggled.
