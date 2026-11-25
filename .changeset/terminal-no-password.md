### Bug Fixes

#### Terminal
- **Terminal on password-less connections** - The terminal no longer fails with "missing field `password`" on a connection that saves no password (local trust auth), and it now passes the keychain password through to psql, mysql and sqlcmd.
