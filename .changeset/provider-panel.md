### Bug Fixes
- Provider panel: an account with one database no longer connects to it unasked; you pick it
- A database just created on Prisma Postgres (or any provider) is connected once it takes connections, instead of failing with "Failed to connect to upstream database" the first time
- Region and organization in "New database" are searchable, like the database list
- The generated password shows in one field with its copy button inside it
