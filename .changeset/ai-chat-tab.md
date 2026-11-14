### Features
- AI chat in a tab (beta, Settings → AI chat in a tab): AI opens as a tab beside your tables instead of taking over the window, and its chats are listed in the sidebar with the database and schema each one worked in, and "Working" while one replies
- The AI can run statements in another database on the same server (a database it just created, or one you name) instead of asking you to switch connections

### Bug Fixes
- AI chats belong to the connection, not the database: switching databases keeps your chats, and SQLite files, MongoDB, D1, DynamoDB and Firestore connections no longer share one chat list
