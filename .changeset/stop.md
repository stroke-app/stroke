### Bug Fixes
- **Stopping a large query** - Stop ends the query at once instead of after the row being read (one large JSON value could take seconds), and the next query no longer waits while the stopped one's remaining rows are drained.
