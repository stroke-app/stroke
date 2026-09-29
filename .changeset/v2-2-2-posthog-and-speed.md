### New Features
- Connect PostHog with a personal API key and browse or query your product analytics with HogQL, read-only
- Railway is available in the provider picker
- Right-click a Docker database to restart or stop it, or copy its connection URL
- Connecting shows a full loading screen with the database's logo, its host, elapsed time and Cancel

### Bug Fixes
- The expanded row JSON and the row panel update right after a cell edit
- MySQL 8 and 9 tables show their columns when empty, and inserted rows appear without a refresh
- Nile no longer drops its connection after every query
- The connect dialog footer names the host being dialled, and Resume only spins when you pressed it

### Changes
- Connecting to a far database costs one handshake instead of two (Prisma Postgres: 6.2s to 0.6s)
- Far databases keep a warm pool, so opening a table never waits on new connections
- Every query to a far database is one round trip shorter, and a table's first open skips a lookup
- Provider pickers answer sooner: the API connection opens with the dialog, and hovering a database starts building its connection
- PlanetScale lists databases and connects in fewer API calls
- MySQL tables open in one round trip
- Provider sign-in is sturdier across IPv4 and IPv6 callbacks
- A tidier menu bar, and the sidebar shows its Enter hint only while searching
