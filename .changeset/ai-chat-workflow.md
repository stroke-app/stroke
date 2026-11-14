### Bug Fixes
- AI chat on MySQL and TiDB: a statement batch sent in one tool call (`CREATE DATABASE shop; USE shop; CREATE TABLE …`) no longer fails as a whole. MySQL refused it with a syntax error and TiDB with "Can not prepare multiple statements"; the statements now run one by one on one connection, and a failure says which statement broke and which ones already ran
- MySQL and TiDB: a `USE` no longer leaks into the connection pool. The connection that ran it went back to the pool pointed at the other database, so later statements landed there at random and failed with "Table 'ecommerce.customers' doesn't exist". A lone `USE` now also says it only applies to that call
- AI chat: a failed SQL step shows the database's own error in red under the step, and the model gets the full error, a hint, and an instruction to fix and retry. A double-quoted string on MySQL (read as a column under ANSI_QUOTES) now gets a hint that says so

### New Features

#### AI Chat
- **Plan checklist** - a multi-step request opens with a short numbered plan that ticks off as the steps finish
- **Step timeline** - each tool call is one row with a status icon, a plain label ("Create table customers", "Insert 3 rows into orders"), its duration, and the SQL and result a click away. A retry folds under the step it fixes ("Retried · succeeded")
- **Turn summary** - a finished turn says what changed ("Created database ecommerce · Created 3 tables · Inserted 12 rows into 3 tables"), with a button to copy every statement it ran
