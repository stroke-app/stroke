---
"stroke": patch
---

Keep Cloudflare and provider sign-ins across restarts when Stroke and Stroke Nightly are both installed. Each app wrote its whole cached copy of the shared keychain vault back on every save, so a sign-in made in one was erased the next time the other saved anything. Writes now touch only the keys they changed, a keychain that can't be read is never overwritten, and refresh tokens are read fresh.
