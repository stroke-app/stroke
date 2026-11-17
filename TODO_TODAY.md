# TODO

What is open as of 2026-11-17 (nightly at 2.16.8, stable at 2.3.0). Security
items live in `TODO_SECURITY.md`.

## Website

The site is `stroke-web` (TanStack Start on Cloudflare Workers).

- [ ] **Redesign the whole site.** Every page, not a refresh of the home page:
      home, features, pricing, download, databases, compare, docs, changelog,
      blog and the legal pages. Keep the app's look (dark first, the same type
      scale and accent) so the site and the app read as one product.
- [ ] **New pictures.** Fresh screenshots of the app as it is now: the tree
      sidebar, several databases open at once, the data grid, the AI chat, ER
      diagrams, the sign-in flows. Light and dark, the same window size.
- [ ] **A demo on the site.** A short screen recording of real use (connect,
      browse, query, ask the AI) near the top of the home page, with a poster
      frame so the page doesn't wait on the video.

## Email

Mail goes out through Plunk (`stroke-web/src/lib/mail`). There are templates
for purchases and failed payments only; nothing sends to every user yet.

- [ ] **Major update and What's new.** One email to every user about the big
      release: what changed since 2.3.0, with the new pictures and a link to
      the changelog and the download. Send it when the release is on Stable.
- [ ] **License update.** One email to license holders about the license
      change. Decide the wording first; it has to match what the site's legal
      pages and the app say.
- [ ] Both need a broadcast path: a template per email, the recipient list
      (users, or license holders only), an unsubscribe link, and a test send to
      myself before the real one.
