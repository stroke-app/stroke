### Bug Fixes
- **Postgres backup and restore** - Backups record the extensions their tables use (uuid-ossp, citext, PostGIS and others), and leave out what those extensions own. Restoring an older backup into a fresh database adds the extensions its tables need first, instead of failing on `uuid_generate_v4()` and rolling back.
