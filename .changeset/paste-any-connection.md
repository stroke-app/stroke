### Features
- Paste any connection string to connect: whole .env lines (DATABASE_URL="…"), SQLAlchemy/ORM URLs (postgresql+psycopg2://), libpq key/value (host=… dbname=…), ADO.NET SQL Server strings (Server=…;Database=…;), valkey://, couchdb://, cassandra://, mysqlx://, Turso https:// and ClickHouse http:// endpoints, and TiDB Cloud, Upstash, Nile and Railway hosts

### Bug Fixes
- A pasted redis:// or libsql:// URL fills the Redis or LibSQL form correctly instead of being read as a Postgres URL
