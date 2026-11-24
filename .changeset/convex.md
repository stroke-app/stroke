### New Features

#### Connections
- **Convex** - Connect to Convex two ways. The Convex provider card signs in with Convex in the browser, lists every cloud deployment on the team you authorize (production, development and previews), and creates a deploy key named Stroke for the one you pick. Convex (URL + key) connects any deployment by its URL and a deploy or admin key, including self-hosted backends and `npx convex dev --local`.
- **Browse and edit Convex tables** - Tables and their document counts in the sidebar, newest documents first, filters like `age > 30 AND plan == "pro"`, inline cell edits, and insert, replace and delete. Int64 values show and edit as whole numbers without losing precision.
- **Convex console** - Run a deployed function with its arguments, `messages:list {"limit": 5}`, or read a table with `users.limit(20)` and `users.get("<id>")`. A function can write, so read-only mode and the MCP server's read-only access refuse function calls.

### Bug Fixes
- Rejected tokens for PostHog, Xata and SingleStore no longer ask to check "the email and API key": the message names that provider's own fields.
