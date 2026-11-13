# Accounts, license and sync: design

Status: proposal · 2026-11-10

Move Stroke from "activate a license key" to "sign in with GitHub or Google",
like Cursor or Claude. The account carries the license, and the user's
workspaces, connections and settings sync across their devices, end-to-end
encrypted. Existing key holders keep working without signing in.

## Decisions (made)

| Question | Decision |
|---|---|
| How synced secrets are protected | **End-to-end encrypted.** Stroke's servers store ciphertext only. |
| Existing activated keys | **Keep working; prompt to link.** Nobody is locked out by the update. |
| Where sync lives | **stroke-web on Cloudflare**: new D1 tables (and R2 if blobs get large), behind the existing Better Auth. |

## What exists today

- **Website (`stroke-web`)**: Better Auth with GitHub and Google
  (`src/lib/auth/auth.ts`), Dodo Payments checkout, and `licenses` rows that
  already belong to a user (`licenses.user_id`). Device seats live in
  `activations`; trials in `device_trials` (per device).
- **App**: an Ed25519-signed key activated through `/api/license/activate`,
  stored in `license.json`, checked on launch (`run_license_check`). Trial is per
  device. Connections are in `localStorage` (`stroke:connections`) **with
  passwords in plain text**; AI keys and OAuth tokens are in the OS keychain
  vault (`secrets.rs`). Settings are in `localStorage` (`stores/*`).

## 1. Sign-in (desktop)

Browser-based OAuth with a loopback redirect: the standard desktop pattern, no
custom URL scheme to register, and the password never touches the app.

1. App starts a one-shot HTTP listener on `127.0.0.1:<random port>` and opens
   `https://stroke.click/desktop/sign-in?port=<p>&state=<random>&challenge=<S256(verifier)>`.
2. The user signs in on the website with GitHub or Google (Better Auth, already
   live). The page shows "Signing in to Stroke on this Mac" and the device name.
3. The site redirects to `http://127.0.0.1:<p>/callback?code=<one-time code>&state=…`.
4. The app checks `state`, then exchanges `code + verifier` (PKCE) at
   `POST /api/desktop/token` for a **desktop session token**: a long-lived Better
   Auth bearer token bound to this device id, revocable from the account page.
5. The token goes into the keychain vault (`secrets.rs`), never `localStorage`.

Fallback when a browser can't reach loopback (locked-down machines): the same
page shows a short code to paste into the app (device-code style).

App UI:
- **First launch (new install)**: a welcome screen with "Continue with GitHub" /
  "Continue with Google". The app is usable after sign-in; the trial starts then.
- **Account menu** (status bar / settings): avatar, email, plan, devices,
  "Sign out", "Manage subscription" (opens the website).
- Sign-out removes the token and stops sync. Local data stays.

## 2. License from the account

- `GET /api/me` returns `{ user, plan, licenses[], trial, devices[] }`. The app
  calls it on launch and caches the answer (signed by the server with the
  existing Ed25519 key, so it works offline until `expires_at`, the same grace
  the key has today).
- **Trial moves to the account** (one trial per person, not per device), which
  also closes the reinstall/clock-rollback gaps listed in TODO_SECURITY.md.
- **Seats** stay `activations`, keyed by the account's device id instead of the
  key.
- **Buying**: checkout already requires a signed-in user and writes
  `licenses.user_id`, so a purchase shows up in the app on the next `/api/me`.

### Existing customers (no lock-out)

| Situation after updating | Behaviour |
|---|---|
| Activated key, not signed in | Pro keeps working exactly as today. A one-time, dismissible prompt: "Sign in to sync your connections; your license comes with you." |
| Signs in, key's purchase email = account email | The key is linked automatically (`POST /api/license/link`). |
| Signs in with a different email | The app offers to link the key it holds (possession proves ownership; the server checks seats). |
| Trial user, not signed in | Sign-in is required to continue; their device trial carries over to the account. |
| Offline at update time | Nothing changes until they're online. |

The "Enter license key" screen stays as a small fallback (offline/air-gapped
machines, enterprise keys) but is no longer the main path.

## 3. Sync

### What syncs

| Kind | Notes |
|---|---|
| Workspaces | name, colour, icon, order (Home, Office, …) |
| Connections | full config incl. secrets, assigned to a workspace |
| Settings | appearance, editor, AI provider choices (not AI keys unless the user opts in) |
| Saved queries, folders, pinned tables | phase 2 |
| Query history | opt-in, phase 3 (can be large and sensitive) |

Never synced: `license.json`, the trial file, the desktop token, caches.

### End-to-end encryption

- Each account has one random 256-bit **sync key**, generated on the first device.
- Every synced record is encrypted on the device with XChaCha20-Poly1305 under
  the sync key: the whole record, not only passwords, since hostnames, database
  names and usernames are sensitive too. The server sees an id, a kind, a
  version, a timestamp and ciphertext.
- The sync key is stored wrapped two ways:
  - in each device's keychain;
  - on the server, wrapped by a key derived from a **sync passphrase** the user
    picks (Argon2id), so a new device can unlock it.
- A new device: sign in, then enter the sync passphrase. A later improvement can
  also approve a new device from an existing one, with no passphrase.
- **Lost passphrase**: the encrypted data cannot be recovered. The user resets
  sync, keeps whatever is on the current device, and re-enters passwords
  elsewhere. Say this plainly in the UI when the passphrase is set.
- **Rust does the crypto** (`chacha20poly1305`, `argon2` crates), so the sync
  key never sits in the webview's JS heap longer than a call.

### Protocol

Last-writer-wins per record, with a server sequence number for cheap
incremental pulls:

- `GET  /api/sync/changes?since=<seq>` returns records changed since `seq`
  (tombstones included) and the new `seq`.
- `POST /api/sync/push` sends `[{ id, kind, version, deleted, ciphertext }]`.
  The server accepts when `version` is the next one, otherwise returns the
  current record as a conflict. The client resolves it (newest edit wins, with
  the losing copy kept locally as "Conflicted copy" for connections).
- The app pulls on launch, on focus, and every few minutes while open, and pushes
  after local edits (debounced). No realtime socket in phase 1.

### Server tables (D1)

```
sync_keys    user_id PK, wrapped_key, kdf_params, created_at, rotated_at
sync_records user_id, id, kind, version, deleted, ciphertext (blob), updated_at, seq
             PK (user_id, id), index (user_id, seq)
desktop_tokens id, user_id, device_id, token_hash, created_at, last_seen_at, revoked_at
```

Limits: about 1 MB per record and 50 MB per account (R2 beyond that if
needed); rate limits on push.

## 4. Workspaces (app)

- A workspace switcher at the top of the sidebar: coloured dot plus name, ⌘1…⌘9
  to switch.
- Connections belong to one workspace. "Default" holds everything at migration.
- Each workspace can remember its last open connection and tabs (local, not synced).

## 5. Security fixes this requires first

1. **Connection passwords move out of `localStorage` into the keychain vault**
   (`secrets.rs`), referenced by connection id. Needed regardless of sync;
   today any process that can read the webview's storage can read every saved
   password.
2. The desktop token lives in the keychain, is device-bound and revocable.
3. Section 2 of TODO_SECURITY.md (license deletion on bad answers) is already
   fixed; `/api/me` follows the same "only `revoked`/`expired` end Pro" rule.

## 6. Phases

| Phase | Scope | Ships when |
|---|---|---|
| 0 | Passwords → keychain (migration on launch). Design sign-off. | first, standalone |
| 1 | Desktop sign-in (loopback + PKCE), `/api/desktop/token`, `/api/me`, account menu, welcome screen. License from account; key linking; account trial. Old keys untouched. | release N |
| 2 | Workspaces UI (local). E2E sync of workspaces, connections, settings. Sync passphrase setup and new-device unlock. | release N+1 |
| 3 | Saved queries, pinned tables, opt-in history. Approve a new device from an existing one. Conflict review UI. | later |

Everything ships behind a setting until it has been used daily for a week. The
email to customers goes out with phase 1 (wording below).

## 7. Customer email (draft)

> **Stroke now has accounts.** You can sign in with GitHub or Google, and your
> license comes with your account. Your connections, workspaces and settings
> sync between your Macs and PCs, end-to-end encrypted: we can't read them.
>
> **If you already have a license key, nothing changes until you choose.** Your
> key keeps working. When you sign in, it's linked to your account automatically.

## Open questions

- Team/enterprise licenses (`enterpriseDomains`): does a domain license grant
  sync, and can workspaces be shared with a team later? Out of scope for phase 1–2.
- Keep the per-device trial for users who never sign in, or require sign-in for
  the trial too? (This doc assumes sign-in is required for new installs.)

## Appendix A: Phase 1 API contract (shared by the app and stroke-web)

Base URL: `https://stroke.click`. For local development the app reads
`STROKE_API_BASE` (e.g. `http://localhost:3000`). All JSON. Errors are
`{ "error": "<code>", "message": "<human text>" }` with a 4xx/5xx status.

### `GET /desktop/sign-in`

Website page (not JSON). Query: `port` (1024-65535), `state` (opaque, ≤128
chars), `code_challenge` (base64url SHA-256 of the verifier, PKCE S256),
`device_id`, `device_name` (≤80 chars).

- Not signed in: show "Continue with GitHub" / "Continue with Google" (Better
  Auth), then come back to this page with the same query.
- Signed in: show "Sign in to Stroke on <device_name>?" with Continue / Cancel.
  Continue mints a one-time code (random 32 bytes, base64url, valid 5 minutes,
  single use, bound to user + code_challenge + device_id) and redirects to
  `http://127.0.0.1:<port>/callback?code=<code>&state=<state>`.
  Cancel redirects to `http://127.0.0.1:<port>/callback?error=cancelled&state=<state>`.
- The redirect target must be exactly `http://127.0.0.1:<port>/callback`.

### `POST /api/desktop/token`

Body `{ code, code_verifier, device_id, device_name, os, version }`. Verifies
the code (unused, unexpired, `S256(code_verifier) == code_challenge`, same
device_id), marks it used, and creates a `desktop_tokens` row (store only the
SHA-256 of the token). Response `200`:

```json
{ "token": "<random 32 bytes base64url>", "user": { "id": "…", "email": "…", "name": "…", "image": "…|null" } }
```

Errors: `invalid_code`, `expired_code`, `pkce_mismatch`.

### `GET /api/me`

Header `Authorization: Bearer <token>`. Updates `desktop_tokens.last_seen_at`.
Response `200`:

```json
{
  "user": { "id": "…", "email": "…", "name": "…", "image": null },
  "entitlement": {
    "plan": "pro" | "free",
    "source": "license" | "trial" | "none",
    "license": { "plan": "pro", "expires_at": 0, "key_hint": "…last 6 chars" } | null,
    "trial": { "started_at": 1700000000, "ends_at": 1701468800 } | null,
    "issued_at": 1700000000,
    "valid_until": 1700604800
  },
  "entitlement_sig": "<base64url Ed25519 signature over the exact JSON text of `entitlement` as sent>",
  "devices": [{ "device_id": "…", "name": "…", "os": "macos", "last_seen_at": 0, "current": true }]
}
```

- `plan` is `pro` when the user has an active, unrevoked, unexpired license, or
  a trial still running. The account trial starts on the first `/api/me` and
  lasts 17 days (the app's `TRIAL_DAYS`), keyed by user id.
- `valid_until` = `issued_at` + 7 days: the app may trust a cached, signature-
  checked entitlement offline until then.
- Signing reuses `LICENSE_PRIVATE_KEY_HEX`; the app verifies with the existing
  embedded public key. The signed bytes are the `entitlement` object's JSON text
  exactly as it appears in the response, so the app must keep it verbatim.
- `401 { error: "unauthorized" }` when the token is unknown or revoked.

### `POST /api/license/link`

Bearer token. Body `{ key }`. Verifies the key's signature, finds the license
row, and sets `licenses.user_id` to this user when it is unset, or already
this user. If it belongs to a different user, returns `409 key_owned_elsewhere`.
The response is the same as `/api/me`.

### `POST /api/desktop/sign-out`

Bearer token. Revokes it (`revoked_at`). Returns `204`. Also exposed on the
account page: list `desktop_tokens` per device, "Sign out" per device.

### App behaviour (phase 1)

- An existing `license.json` keeps working with no sign-in (no behaviour change).
- With a token: the entitlement comes from `/api/me` (cached + signature-checked)
  and takes precedence over the key; a held key is linked once via `/api/license/link`.
- New installs without a license and without a token see the sign-in screen first.

## Appendix B: Phase 2 sync contract (shared by the app and stroke-web)

All endpoints take `Authorization: Bearer <desktop token>` (Appendix A) and
return `{ error, message }` on failure. The server never sees plaintext.

### Crypto (app side; the server only stores opaque strings)

- **Sync key:** 32 random bytes, one per account.
- **Record ciphertext:** `base64url(nonce24 || XChaCha20-Poly1305(sync_key,
  nonce, plaintext, aad = "<kind>:<id>"))`. The AAD binds a ciphertext to its
  record, so the server can't swap records.
- **Wrapped key:** `base64url(nonce24 || XChaCha20-Poly1305(kek, nonce,
  sync_key, aad = "stroke-sync-key-v1"))`, where `kek = Argon2id(passphrase,
  salt16, m = 65536 KiB, t = 3, p = 1, len 32)`.
- **Plaintext:** JSON `{ "updated_at": <unix ms>, "data": <the record> }`.

### Record kinds (phase 2)

| kind | id | data |
|---|---|---|
| `workspace` | workspace id | `{ id, name, color, createdAt }` (the default workspace syncs too) |
| `connection` | connection id | the whole saved connection, **secrets included** (E2E), plus `workspaceId`, `color` |
| `settings` | `settings` | the synced subset of app settings (appearance, editor, AI provider choices; not AI keys, not window state) |

### `GET /api/sync/key`

`200 { wrapped_key, kdf: { alg: "argon2id", salt, m, t, p }, created_at }` or
`404 { error: "no_sync_key" }`.

### `PUT /api/sync/key`

Body `{ wrapped_key, kdf, replace?: boolean }`.
- No key yet: stores it and returns `201`.
- A key exists and `replace` isn't true: `409 key_exists`.
- `replace: true` (reset sync): replaces the key and **deletes all of the
  user's records** in the same transaction; returns `200`.

### `GET /api/sync/changes?since=<seq>&limit=<1..500>`

`200 { records: [ { id, kind, version, deleted, ciphertext | null, updated_at, seq } ], next_since, more }`
- Records with `seq > since`, in seq order. Tombstones have `deleted: true`
  and `ciphertext: null`.
- `seq` is one strictly increasing counter per user.

### `POST /api/sync/push`

Body `{ records: [ { id, kind, version, deleted, ciphertext | null } ] }`, at
most 200 records.

The server accepts a record when `version == current_version + 1`, where a
missing record counts as `current_version = 0`. It assigns the next `seq` and
sets `updated_at = now`.

Response:

```json
{ "accepted": [ { "id", "version", "seq" } ], "conflicts": [ <the current server record, same shape as /changes> ] }
```

- **Limits:** ciphertext at most 1,500,000 characters; at most 50 MB in total
  per account (`413 quota_exceeded`); `kind` must be one of the kinds above
  (`400 invalid_kind`); `id` 1–128 chars of `[A-Za-z0-9_-]`.

### App sync behaviour

- **When it runs:** pull, then push dirty records. On launch, on window focus
  (throttled to 1 per 60 s), 2 s after a local change (debounced), and from
  "Sync now".
- **Conflicts:** a conflict keeps the side with the newer `updated_at`. For
  connections, the losing copy is saved as "<name> (conflicted copy)".
- **Unlocking:** the sync key lives in the OS keychain once unlocked; a new
  device asks for the passphrase. A lost passphrase means "Reset sync": a new
  key, server records deleted, then this device's data is pushed.

### Paste-a-code fallback (phase 1 addendum)

- **Website:** when the redirect to `127.0.0.1` fails, or on request, the
  `/desktop/sign-in` page offers "Copy code" after Continue. The text is
  `stroke-code:<code>`, valid 5 minutes.
- **App:** the sign-in waiting state has "Paste code". It accepts
  `stroke-code:<code>` or the bare code, and exchanges it at
  `/api/desktop/token` with the pending PKCE verifier and device id, exactly as
  the loopback path does.
