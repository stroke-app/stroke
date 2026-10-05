### New Features

#### SQL Editor
- **Completion in triggers** - CREATE TRIGGER suggests its timing, events and `ON` table in turn, then FOR EACH ROW and the body; inside the body `NEW.` and `OLD.` (SQL Server: `inserted.` / `deleted.`) list the trigger table's columns, and in a Postgres trigger function `NEW.` reads the table of the CREATE TRIGGER that runs it
- **Triggers, routines and views are checked before they are created** - A table or view the schema does not have, a `NEW.` / `OLD.` or SET column the table does not have, and a BEGIN with no END are underlined in CREATE TRIGGER / FUNCTION / PROCEDURE / VIEW / EVENT, and Run stops with what is wrong (Run anyway sends it as written). The engines create these as written and they fail the first time they run - a trigger on every write to its table
- **Multiple cursors** - Every editor takes several selections, so a template field used twice is edited in both places at once

#### ORM Runner
- **Drizzle and Prisma completion** - `db.` methods, the builder methods for the chain, a table's columns, the condition helpers; `prisma.` models and their methods, and inside a call the keys that fit where the caret is: arguments, columns, AND / OR / NOT, a column's filters

### Bug Fixes
- A trigger made from the template no longer keeps `table_name` in its body when the table is set after ON
- The sidebar's header row keeps all its tabs and actions at its narrowest width
- Diagrams in the assistant's replies take the app theme's colours, and their arrows no longer pick up the accent
- The JSON, text, DDL, data diff and policy SQL editors, and the ORM runner, follow the app theme and zoom like the SQL editor

### Changes
- **Every editor runs on CodeMirror** - Monaco is gone from the app: the JSON tab, the table's JSON and Text views, DDL, ORM schemas, the data diff, the security SQL editor and the ORM runner use the same editor as the SQL console. The app's code is 19 MB instead of 37 MB, and the table's text view keeps its CSV / TSV colouring; ORM schemas get Prisma highlighting
- **Faster start** - The code loaded before the first window paints is 2.49 MB instead of 3.05 MB: the SQL formatter and the Phosphor icon set load when first needed, and code blocks ship the ~30 languages they are written in rather than ~290
- Updated devalue and DOMPurify for their security advisories
