### Bug Fixes
- **ClickHouse decimals** - Keep their scale and every digit: a Decimal(10,2) of 12.50 showed 12.5, and a Decimal(38) past a double's precision came back rounded.
- **Opening a connection** - Lands on the database the connection names instead of the first schema alphabetically (INFORMATION_SCHEMA on ClickHouse).
- **Command palette** - Finds the activity log by "logs".
