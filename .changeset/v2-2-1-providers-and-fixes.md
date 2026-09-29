### New Features
- Connect TiDB Cloud, Turso, Nile and Upstash from the provider picker. TiDB shows its sign-in code right in the app
- Alt+wheel scrolls the table a few rows at a time, for landing on a precise row in a huge table
- Sign out of a provider from its card, with a confirm first
- Saved connections show their provider's logo and a small chip naming the engine
- Keyboard shortcuts in the connections dialog: new connection, paste a URL, filter and resume the last one
- The MCP dialog copies a ready config for every client
- A Claude font preset in Settings → Appearance

### Bug Fixes
- A SQL run that ends in a SELECT keeps its earlier writes instead of rolling them back
- Picking a saved provider database connects to that database, never one with a similar name
- Passwords with a raw %, #, ? or @ import from a connection URL whole
- Stopping a query in the editor stops it on the server too
- Deleted connections stay deleted, and deleting one asks first
- Provider sign-in survives a restart on Windows
- PlanetScale lists databases across organizations and creates passwords again
- Upstash connections keep their provider after a restart
- One organization the token can't read no longer empties the whole database list
- The splash screen no longer flashes one colour and then another
- A chart on a windowed table uses only the rows that are loaded

### Changes
- Provider databases open on the last list instead of a spinner, and reconnecting skips the API calls entirely
- Connecting to far away databases is faster: no restarted handshakes, no double connect, one Redis connection reused
- Geist is the default font again
- The connect page uses the full width with four cards per row, and the New table dialog reads as one table
- An empty sidebar tab shows one empty state instead of two
- Failed database picks show as a toast and leave the picker open
