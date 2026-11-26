---
"stroke": patch
---

### New Features

#### Layout and motion
- **Inset content panel** - The page sits in a rounded panel, with the sidebar, title bar and status bar around it on one background. Settings, Appearance, Inset content panel turns it off.
- **Sliding sidebar** - Ctrl+B / ⌘B slides the sidebar out and back while the page moves into the space. Menus grow from the button that opened them and dialogs settle into place. On by default on macOS and Windows; Settings, Appearance, Animate panels.
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
- Clear all history in the AI chat deleted every conversation without asking. It asks first now, as do key changes in the document and DynamoDB editors.
- The sidebar filter shrank to a few letters beside the schema picker.
- Success, warning, error, info and accent text, and highlighted SQL, now read clearly in every theme.
- At the smallest window size the SQL results pane kept its width and its toolbar stayed whole.
- ? and Ctrl+? / ⌘? never opened the keyboard shortcuts.
- The command menu kept its groups in a fixed order whatever you typed, so "ai" put Advisor first and Enter opened it. The best match now comes first, and two letters no longer match inside the middle of a hidden keyword.
- Back and forward stopped at pages that put the caret in their editor on their own (the SQL editor, an empty JSON viewer).
- Each snapshot on the Schema timeline shows its table count again.
- The theme list in the command menu opened at the top with the current theme out of sight, and every dropdown opened on its first row. Both open on the current choice now, scrolled into view.

### Changes
- Deleting a saved connection, AI chat, dashboard, dashboard card, saved chart, saved table view, virtual column, diagram or schema snapshot asks nothing and offers Undo for 8 seconds instead.
- Every dialog's main button is drawn in the accent colour.
- Toolbars group their buttons with space instead of thin dividers, and empty-state headings are larger and lighter.
- Shortcuts print the same way in every menu row, radio and checkbox rows included.
- On a new install, notifications appear in the bottom right corner, clear of the tab bar. Settings, Appearance moves them.
- Dialogs and the command menu are white in light themes (they took the grey of the page around the panel) and a step lighter than the page in dark ones. Every page in the content panel now shares one background; the AI view, Advisor and Diagrams sat a shade darker.
- The command menu, the Schema explorer and switching between tables are faster. Typing in the command menu takes about half the time per letter it did, and drilling into Tables or Connections starts with an empty search.
- Grid checkboxes show on the row you point at, and on every row once you select one.
- Empty pages share one design, with the first step as a button. Diagrams, Dashboard and Schema timeline hide their lists and toolbars until there is something in them.
