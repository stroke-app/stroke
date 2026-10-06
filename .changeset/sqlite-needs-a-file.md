### Bug Fixes
- A SQLite or DuckDB connection saved without a database file no longer opens a temporary database that is thrown away on disconnect, taking every table made in it. It now says it needs a file
- A new SQLite file can be made from the connection form with New, next to Browse
