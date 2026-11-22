### Bug Fixes
- **Infinite scroll counter** - It counts the rows that have loaded instead of staying at "50 of 100,000".
- **Connection latency** - The first reading waits until the connect settles: a local database no longer flashes a red "958 ms", and one database's reading no longer carries over to the next.
- **Query history** - Errors read as the database wrote them, without "error returned from database:". Runs under a millisecond show "<1ms".
- **Settings search** - Every word matches in any order, so "page size" and "dark mode" find their settings.
- **Find in database** - Counts views as views, and its warning no longer starts with a stray dot or outlives the query.
