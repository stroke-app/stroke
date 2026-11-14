### Bug Fixes
- Oracle: the status bar shows the container you are in (FREEPDB1) instead of "No database", and warns when a blank service name put you in the root container (CDB$ROOT), which holds only Oracle's own schemas
- Oracle: "Hide system tables" hides the tables Oracle maintains itself (AQ$_, LOGMNR, MVIEW$ and the rest)
- Oracle: paging, sorting and filtering a table no longer re-read the catalog each time (3.5 s to a few ms once the table is open)
- The SQL filter bar applies on Enter again; a typed-out word before the cursor made Enter insert a space instead
