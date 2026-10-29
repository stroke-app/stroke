### Bug Fixes
- Applying edits the database rejects now shows one message saying how many failed and whether they are still staged, instead of one toast per edit followed by "Changes applied"
- A search or filter that matches nothing now says "No rows match these filters" and offers Clear filters, instead of "No rows in this table"
- Close All Tabs and Close Other Tabs now ask before discarding unsaved edits
- Arrow keys on the sidebar tab strip switch tabs again
- The Databases tab no longer says "No databases" while its list is loading
- Find & replace keeps what was typed when switching sidebar tabs

### Changes
- NULL and empty values are marked in the grid by default, so a NULL no longer looks like the text 'NULL'
- Scrolling through long text columns does less work, because each cell measures only the part that fits
- Opening a table keeps the previous one on screen, dimmed, until the new rows arrive
- The toolbar pager, the status bar and the filter button keep their size and place while a table loads
- Refreshing keeps the sidebar list and its row counts on screen, and the row count no longer blinks back to counting on a refresh or page change
- Changing schema no longer flashes the empty "Select a table" screen
- Switching connections shows the connecting screen straight away instead of a blank window
- Sidebar tabs switch without rebuilding their lists, and each keeps its scroll position
- The Objects tab has its own icon, and Close All Tabs is no longer styled as a destructive action
