### Changes
#### Performance
- **Deep scrolling in large tables** - A large table opened with All rows is copied in the background to a local file, in the grid's own order, and windows already copied are read from it instead of asking the database for a deep OFFSET. A jump to row 900,000 no longer waits on the server reading every row before it.
- **Connect** - Background warm-ups (editor chunks, the terminal dock, the CLI lookup) wait until the first screen after a connect has painted, and idle work waits for the main thread to be idle rather than just for no input.
- **Loading pill** - Says "deep in the table, fetching" when the link is fast and the query is what is slow, instead of "slow connection".
