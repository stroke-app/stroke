# Stroke — Design System

Single source of truth for UI in this app. Every surface (sidebar, toolbars,
modals, pages, menus, tables) must follow this. When a component disagrees with
this file, the component is wrong — fix the component.

Goal: **modern, minimal, production-grade, and identical across the whole app.**
The tokens already exist in `src/app.css` + the theme files. The problem this
file solves is *consistent use* of them — no more arbitrary `text-[13px]`,
mismatched control heights, or one-off selected states.

---

## 0. Golden rules (read first)

1. **Never use an arbitrary font size** (`text-[13px]`, `text-[10.5px]`, …).
   Use the `text-ui-*` scale only. (There are ~370 legacy `text-[Npx]` uses —
   these are being migrated; do not add new ones.)
2. **Never use an arbitrary color.** Use semantic tokens (`text-foreground`,
   `bg-muted`, `border-border`, …). No hex, no `bg-[#…]`, no raw `text-gray-500`.
3. **Control heights are fixed:** `h-7` compact · `h-8` default · `h-9` field.
   Never invent `h-[34px]`.
4. **Icons use the sizing table** (§5). Body-adjacent icons are `size-3.5`.
5. **Radius is a 3-step scale** (§4). Interactive → `rounded-md`; containers →
   `rounded-lg`; floating panels → `rounded-[10px]`.
6. **Reuse the component before styling a new one.** Dropdown = `SearchableMenu`.
   List row = the row pattern (§8). Icon button = the icon-button class (§6).
7. **Spacing is the 4px grid** via Tailwind `gap-*`/`p-*`. Prefer `gap-*` on flex.
8. Match the density of the surrounding surface. Toolbars/status bar are dense
   (`text-ui-2xs`/`text-ui-xs`); page content breathes more.

---

## 1. Foundations

### Fonts (`src/app.css`)
| Role | Variable | Stack |
|---|---|---|
| UI / sans | `--font-sans` | Geist Variable (default) → system-ui |
| Headings | `--heading-font` (utility `font-heading`) | Source Serif 4 Variable (Claude preset), else follows `--font-sans` |
| Data / mono | `--font-mono` | Geist Mono Variable (default) → ui-monospace |
| Code editor | `--editor-font-family` | JetBrains Mono → Geist Mono |

- **Sans** for all chrome, labels, prose, buttons.
- **Heading** (`font-heading`) for dialog titles, via `Dialog.Title`. Nothing
  smaller than a title uses it.
- **Mono** (`font-mono`) for data: identifiers, values, counts, IDs, SQL,
  hostnames, table/column names, timings. Numbers in stat cards use
  `font-mono tabular-nums`.

### Base size
`1rem = --app-font-size`. Each platform's 100% rung is its own comfortable
reading size, so "zoom 0" means the same thing everywhere:

| Platform | 100% root | Why |
|---|---|---|
| Linux, Windows | 18px | 1x-DPI desktops; a 14px stroke is too thin to read reliably |
| macOS | 14px | Retina plus SF's own hinting; 18 is oversized there |

Windows used to reach ~17.5px by shipping a **1.25 default zoom** instead, which
made its 100% rung a lie and left no headroom below it. That is gone: zoom 1 is
the default on every platform. The app supports zoom by design, so **never
hard-code a px font size**.

**The root steps by one whole pixel per zoom rung**, taken from the rung's index
in `ZOOM_STEPS` rather than from `basePx × zoom`. Multiplying and rounding
collapses rungs into each other - at a 16px base, 110% and 115% both round to
18px - so the window stayed pixel-identical between them while the `--fs-*`
steps, rounded independently from different numerators, still moved. That is
zoom which changes the text and not the layout. Indexing guarantees a strictly
increasing root on every base, so a zoom step always moves both.

The `text-ui-*` classes resolve to `--fs-*` rather than a rem calc.
`applySettings()` (`src/lib/stores/settings.js`) recomputes every step for the
active base size and zoom level and **rounds each one to a whole pixel**. The
rounding is the point: the old `calc(N / 14 * 1rem)` form only landed on whole
pixels when the root happened to be exactly 14px, so at the 15px Linux base a
13px caption rendered at 13.93px and WebKit rasterised it off the pixel grid.
That is what made the whole UI look soft at 100% zoom. Add a step by editing
`UI_TYPE_SCALE` in `settings.js` and its `:root` fallback in `app.css` together.

---

## 2. Type scale (the only sizes allowed)

| Class | px @100% | Use for |
|---|---|---|
| `text-ui-2xl` | 20 | page/section hero titles (rare) |
| `text-ui-xl` | 18 | dialog titles, empty-state headings |
| `text-ui-lg` | 16 | card/panel titles, primary headings |
| `text-ui` | 14 | **default body**, inputs, buttons, menu items |
| `text-ui-sm` | 13 | secondary body, dense menu items, table cells |
| `text-ui-xs` | 12 | metadata, toolbar labels, helper text |
| `text-ui-2xs` | 11 | dense chrome (status bar, chips, counts) |
| `text-ui-3xs` | 10 | micro-labels only (uppercase section labels) — floor |

> If `text-ui-2xl/xl/lg` are missing in `app.css`, add them alongside the
> existing scale (same `calc(N/14 * 1rem)` pattern) — see §12.

**Weights:** `font-normal` body · `font-medium` emphasis/active · `font-semibold`
titles + micro-labels. Never `font-bold` in chrome.

**Line-height:** default for single-line controls; `leading-relaxed` for
multi-line helper text; `leading-snug` for tight list rows.

**Never below 10px.** `text-[8px]/[9px]` are shimmed up to 10px — don't rely on it.

---

## 3. Color (semantic tokens only)

Themeable via the theme files (oklch, WCAG-AA contrast floor). Use tokens, never
literal colors.

| Token | Meaning |
|---|---|
| `background` | app base |
| `panel` | content/main region surface |
| `sidebar` / `sidebar-foreground` | sidebar surface |
| `popover` / `popover-foreground` | floating menus/dialogs |
| `muted` / `muted-foreground` | subtle fills / secondary text |
| `accent` / `accent-foreground` | hover + active chrome fill |
| `foreground` | primary text |
| `border` | all hairlines/dividers |
| `primary` / `primary-foreground` | primary actions, active accents |
| `destructive` | delete/danger |
| `ring` | focus ring |

**Opacity conventions (consistency matters most here):**
- Secondary text: `text-muted-foreground`. There is no contrast-safe tertiary
  text tier. Measured on the dark theme, `--muted-foreground` is a clean 7.85:1
  at full strength, but every alpha step below **74%** drops under the 4.5:1 text
  floor: `/70` is 4.27:1, `/60` 3.40:1, `/50` 2.69:1, `/40` 2.11:1. On light
  themes the cutoff is 89%, so `/60` and below fail there too. **De-emphasise
  with size, weight, or position - not with alpha on the text colour.** The
  codebase still carries ~1,370 `text-muted-foreground/NN` uses from before this
  was measured; treat each one as a bug when you touch its component, and never
  add a new one to text a user has to read.
- Decorative marks that carry no meaning (separator dots, watermarks) may go
  below the floor. Anything that carries meaning needs 3:1, which is 58% alpha
  on dark and 69% on light.
- Hairlines: `border-border` (structural) · `border-border/50` (internal dividers).
- Idle icon buttons: `text-muted-foreground` → hover `text-foreground`.
- **Field chrome is one class: `.field-surface`** (`app.css`). It sets the field
  radius and a 1px `--field-border` hairline, and nothing else - no fill, no
  gradient, no shadow. Inputs, textareas, select triggers, the filter-row
  controls, the connection form and the outline/secondary buttons all wear it, so
  a text field and the select beside it are drawn the same way. Anything that
  needs a background adds its own `bg-*`.
- **The border never changes.** Hover does not move it and neither does focus.
  Focus is `outline: 2px solid var(--ring)` at `outline-offset: 1px`, sitting
  just outside the edge - one focus language for fields and buttons, no layout
  shift, and no shadow anywhere in the system.
- `--field-border` is `color-mix(in oklch, var(--foreground) 30%, var(--background))`,
  derived per theme. It measures 1.87-1.96:1 on dark and 2.21-2.44:1 on light, so
  it misses WCAG 1.4.11's 3:1 for a control boundary - a deliberate call for the
  hairline look, with the visible-state burden carried by the focus outline
  (4.63:1) instead. Do not raise it without raising the whole system together.
- **Selected/active row (canonical):** `bg-accent text-foreground` for chrome;
  `bg-primary/10 text-foreground ring-1 ring-primary/25` for prominent pick lists
  (provider DB lists, account lists). **Pick one per context and never mix
  `/10` vs `/15`, `/25` vs `/30`.**
- Never use a full `bg-accent`/`bg-primary` *fill* on a small icon — reads loud.
  Subtle chip = `bg-foreground/[0.08]`.

---

## 4. Radius

`--radius` is **absolute px**, not rem: a corner is a physical size and has no
reason to grow when the type scale or the zoom level does. 6px by default, 8px on
macOS, 7px on Windows. Fields and the buttons beside them use `--radius-field`
(10px), which is rounder on purpose; small buttons step down from it rather than
running their own ladder.

| Class | Use |
|---|---|
| `rounded-md` | buttons, icon buttons, inputs, chips |
| `rounded-sm` | menu / select rows — the concentric radius inside a `rounded-[10px]` panel with `p-1` (10 − 4 = 6px) |
| `rounded-lg` | cards, panels, grouped containers, field wrappers |
| `rounded-[10px]` | floating popovers / command menus |
| `rounded-2xl` | modal dialog contents only (`bg-background border-border/60 elevate-3-rim`, overlay `bg-black/65`) |
| `rounded-full` | dots, pills, avatars, status indicators |

No other radii. No `rounded-xl`/`rounded-sm` in new code unless matching a
`rounded-[10px]` popover neighbor.

---

## 5. Icons (`@lucide/svelte` only)

| Size | px | Use |
|---|---|---|
| `size-3` | 12 | inside dense chips / status bar |
| `size-3.5` | 14 | **default** — buttons, menu items, toolbars, inline |
| `size-4` | 16 | list rows, card headers, section titles |
| `size-5` | 20 | empty-state / feature icons |

- Always `shrink-0` on icons inside flex rows.
- Square sizing via `size-*` (never `w-4 h-4`).
- One icon set. No custom SVG unless truly bespoke.
- **`size-3` (12px) is the floor.** Below that a Lucide glyph is sub-pixel mush
  at 1x DPI, which is where the old `size-2.5` RLS lock and Pro badge ended up.
- An icon that carries meaning on its own (table vs view, locked vs unlocked,
  a status glyph) needs **3:1 against its surface**. On the dark theme that is
  `text-muted-foreground` at 58% alpha or stronger, so anything at
  `text-muted-foreground/50` or below fails. Do not stack `opacity-*` on top of
  an already-thinned colour - they multiply.
- An icon button smaller than `size-6` needs the `.hit-area` class, which grows
  the pointer/touch target to the WCAG 2.5.8 24x24 baseline without changing how
  big the button looks. Keep 4px between neighbouring ones so the targets do not
  overlap.

---

## 6. Controls

### Heights
`h-7` compact (toolbar/menu triggers, chips) · `h-8` medium · `h-9` primary
form fields & the main toolbar row.

### Icon button (canonical class)
```
inline-flex size-7 items-center justify-center rounded-md
text-muted-foreground transition-colors
hover:bg-accent hover:text-foreground
disabled:pointer-events-none disabled:opacity-40
```
Active/toggled adds `bg-accent text-foreground` (or `text-primary` for a
semantic on-state like AI/read-only).

### Text/label button
```
flex items-center gap-1.5 rounded-md px-2 py-1 text-ui-sm
transition-colors hover:bg-accent hover:text-foreground
data-[state=open]:bg-accent data-[state=open]:text-foreground
```

### Primary button
Use `Button` from `ui/button`. Solid: `bg-primary text-primary-foreground
hover:opacity-90`. Never hand-roll a primary button.

### Inputs / selects
`h-9 rounded-lg border-2 border-border bg-muted/30 px-3 text-ui`, focus:
`focus:border-ring/55 focus:ring-2 focus:ring-ring/15 focus:outline-none`. Prefer
the `Input` / `Select` wrappers in `ui/*`.

**The border is 2px on every field** — inputs, textareas and select triggers
alike. A 1px control sitting next to a 2px one reads as a different component,
which is exactly what the select trigger used to do.

### Focus ring (one convention, everywhere)
A focused control gets a **muted accent border plus a tight, low-alpha halo** —
never a full-chroma outline or a wide glow:

- `focus:border-ring/55 focus:ring-2 focus:ring-ring/15` (bare elements)
- `focus-visible:border-ring/55 focus-visible:ring-2 focus-visible:ring-ring/18`
  (`ui/*` primitives: button, select trigger, tabs, checkbox, input group)

Do not use `border-ring` at full opacity, `ring-ring` without an alpha, or
`ring-3` on focus. Invalid/destructive states stay loud (`aria-invalid:ring-3`) —
that contrast is the point.

---

## 7. Dropdowns & menus

- **Searchable dropdown** (account/db/column pickers, anything filterable) →
  `SearchableMenu.svelte`. Panel: `rounded-[10px] border-border/60 bg-popover
  elevate-2-rim`, width matches trigger via `w-[var(--bits-popover-anchor-width)]`
  (add `min-w-[240px]`).
- **Action menu** (non-filterable) → `ui/dropdown-menu`. Items `text-ui-xs`,
  leading `size-3.5` icon, `gap-1.5`.
- **Submenu panels** animate fade-only (`duration-75`). No scale/slide: a submenu
  opens under a pointer already moving toward it, so a panel still growing into
  place reads as lag.
- Prefer folding variants of one action into a submenu over a long flat list —
  a menu past ~10 top-level rows is hard to scan (see the DataTable cell menu:
  Copy as ›, Filter ›, Transform ›, Insert ›).
- One active dialog/menu at a time from the global hotkey flow.

### Canonical menu recipe (dropdown / context / select — all identical)

The primitives in `ui/dropdown-menu`, `ui/context-menu`, and `ui/select` share
ONE recipe. Never re-densify a menu at the call site with `[&_[data-slot=…]]`
overrides — if a menu looks off, the primitive is wrong; fix it there.

| Part | Recipe |
|---|---|
| Panel | `rounded-[10px] border border-border/60 bg-popover p-1 elevate-2-rim` |
| Item | `gap-1.5 rounded-sm px-1.5 py-1 text-ui-2xs` (a 25px row) · focus/highlight `bg-accent text-foreground` · disabled `opacity-40` · icons `size-3.5 shrink-0` · **no `transition-colors`** — a colour fade on hover makes highlight tracking feel laggy in a long menu |
| Checkbox/radio/select item | same, with `pl-1.5 pr-7` and the indicator at `right-1.5` |
| Group label | `px-1.5 py-1 text-ui-3xs font-medium text-muted-foreground` — **plain case**, never uppercase (uppercase micro-labels are for page/sidebar sections only, §10) |
| Separator | `bg-border/60 -mx-1 my-1 h-px` |
| Shortcut / trailing hint | `ml-auto shrink-0 pl-3 text-ui-3xs tracking-widest text-muted-foreground` |
| Tooltip | `rounded-lg border-border/60 bg-popover px-2.5 py-1.5 text-ui-xs elevate-2-rim` |

### Menu width (hard rule)

- **Never put a fixed `w-*` on a menu panel** (`Content` / `SubContent`). Use
  `min-w-*`. Panels shrink-to-fit, so a fixed width cannot grow for a label that
  does not fit — the text simply paints outside the rounded border. Labels are
  `$t(...)` strings, and a width tuned to English overflows in longer locales.
- Every panel is capped at `--menu-max-w` (`src/app.css`) and clips with
  `overflow-x-hidden`, so nothing can escape the panel even at the ceiling.
- Menu items carry `min-w-0`. Wrap any label that can run long — a generator or
  transform label, a column name, a path, a cell value — in
  `<span data-slot="menu-label">`. The item primitives truncate that slot with an
  ellipsis, mirroring `data-slot="command-label"` in the command palette.
  Do **not** add `flex-1` to it: that sets `flex-basis: 0`, which defeats the
  panel's grow-to-fit sizing and makes every label truncate at `min-w`.

---

## 8. List row pattern (files, tables, dbs, connections)

```
flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left transition-colors
<idle>     hover:bg-muted/50   text-foreground/85
<selected> bg-primary/10 text-foreground ring-1 ring-primary/25
```
- Leading `size-4` icon (`text-muted-foreground` idle, `text-foreground`
  selected), `shrink-0`. The old `/45` measured 2.38:1 against `--background`,
  under the 3:1 floor a meaning-carrying glyph needs.
- Label: `min-w-0 flex-1 truncate` (+ `font-mono` for identifiers).
- Trailing metadata: `text-ui-2xs text-muted-foreground/40`, `ml-auto` or after
  the flex-1 label; selected → trailing `Check size-3.5 text-primary`.
- Search box above long lists (>6): `h-9`, leading `Search size-3.5` at `left-3`,
  input `pl-9 text-ui-sm`, divider `border-b border-border/50`.

---

## 9. Cards / panels / stat blocks

- Card: `rounded-lg border border-border bg-panel` (or `bg-muted/[0.04]` for a
  softer inset). Padding `p-3`/`p-4`. Title `text-ui-lg font-semibold`.
- Stat card: label `text-ui-2xs font-semibold uppercase tracking-[0.06em]
  text-muted-foreground/60`, value `text-ui-lg font-mono tabular-nums
  text-foreground`. **Align every card in a row to the same internal grid** —
  the Insights screenshot fails here; use a shared stat sub-component so label/
  value/secondary rows line up across all cards.
- Section grids: `grid gap-3` (cards), consistent column counts; never mix
  `gap-1.5` and `gap-4` in sibling grids.

---

## 10. Micro-labels (uppercase section labels)

```
text-ui-3xs font-semibold uppercase tracking-[0.06em] text-muted-foreground/55
```
Used for ACCOUNT / D1 DATABASE / PINNED / TABLES etc. Always this exact recipe.
**Page/sidebar sections only** — menu group labels inside dropdowns are plain
case (§7); do not put uppercase micro-labels inside a menu panel.

---

## 11. Do / Don't

**Do**
- Reuse `ui/*` wrappers and the shared patterns above.
- Keep one selected-state recipe per context.
- Right-size touch targets (≥ `size-7` for clickable icons).
- Use `tabular-nums` for any changing/aligned numbers.

**Don't**
- No arbitrary `text-[Npx]`, `h-[Npx]`, hex colors, `rounded-xl`.
- No loud full-accent fills on small icons.
- No new bespoke dropdown/list markup — extend the shared component.
- No `font-bold` in chrome; no ALL-CAPS body text.

---

## 12. Rollout plan (apply this file to the app)

Execute in phases, one focused pass each (compact context between phases):

1. **Tokens** — ensure `app.css` has `text-ui-lg/xl/2xl` (add if missing). Keep
   the `text-[Npx]→rem` shims as a safety net during migration.
2. **Primitives audit** — `ui/*` (button, input, select, dropdown-menu, dialog,
   command) conform to §6/§7. Fix these first; everything inherits.
3. **Shared patterns** — extract/verify: icon-button class, list-row, stat-card,
   micro-label, `SearchableMenu`, provider DB picker (dedupe CloudflareLogin ↔
   ProviderConnect into one component).
4. **Chrome** — TitleBar, ActivityBar, Sidebar, TableToolbar, SqlConsole header,
   StatusBar: unify heights, icon sizes, label sizes to the scale.
5. **Pages** — Insights, Security, Schema, ERD, ORM, Backup, Dashboard, Diagrams,
   Connection modal: card grids aligned, titles on `text-ui-lg`, spacing on grid.
6. **Sweep** — grep-replace arbitrary `text-[Npx]` → nearest `text-ui-*`; fix
   stray radii/colors. Verify build + a visual pass per page.

Track each phase's diffs against this file; when in doubt, this file wins.
