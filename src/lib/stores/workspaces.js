// Workspaces: named, coloured sets of connections (Home, Office, a client, …).
// A connection belongs to one workspace; one without `workspaceId` belongs to
// the default workspace, so every connection saved before workspaces existed
// lands in "Personal" without a migration. The active workspace decides which
// connections the lists show. Persisted in localStorage like the other stores;
// the sync layer (docs/accounts-and-sync.md) carries the same records later.
import { writable, get } from 'svelte/store'
import { notifyLocalChange, recordDeletion } from '$lib/sync/changes.js'

/**
 * @typedef {'gray' | 'red' | 'orange' | 'amber' | 'green' | 'teal' | 'blue' | 'violet' | 'pink' | string} ColorId
 *   A palette id, or a custom `#rrggbb` from the swatch row's picker.
 * @typedef {{ id: string, name: string, color: ColorId, createdAt: number }} Workspace
 */

export const DEFAULT_WORKSPACE_ID = 'default'
const STORAGE_KEY = 'stroke:workspaces'
const ACTIVE_KEY = 'stroke:workspace:active'

/**
 * The one palette for workspace and connection colours. Classes live here and
 * nowhere else, like the engine tints: a dot, a text tint, a soft wash.
 *
 * One lightness (0.55) for every hue, each at 85% of the chroma sRGB allows
 * there, so the white initial on a WorkspaceMark clears 4.5:1 (4.6–5.4) and a
 * dot clears 3:1 on the light, dark and mono themes (3.7–4.9). The old mix of
 * Tailwind steps put amber and teal under 2:1 on light.
 * @type {{ id: ColorId, label: string, dot: string, text: string, soft: string }[]}
 */
export const COLORS = [
  { id: 'gray', label: 'Gray', dot: 'bg-[oklch(0.55_0.020_286)]', text: 'text-[oklch(0.55_0.020_286)]', soft: 'bg-[oklch(0.55_0.020_286/0.12)]' },
  { id: 'red', label: 'Red', dot: 'bg-[oklch(0.55_0.190_25)]', text: 'text-[oklch(0.55_0.190_25)]', soft: 'bg-[oklch(0.55_0.190_25/0.12)]' },
  { id: 'orange', label: 'Orange', dot: 'bg-[oklch(0.55_0.134_45)]', text: 'text-[oklch(0.55_0.134_45)]', soft: 'bg-[oklch(0.55_0.134_45/0.12)]' },
  { id: 'amber', label: 'Amber', dot: 'bg-[oklch(0.55_0.101_70)]', text: 'text-[oklch(0.55_0.101_70)]', soft: 'bg-[oklch(0.55_0.101_70/0.12)]' },
  { id: 'green', label: 'Green', dot: 'bg-[oklch(0.55_0.106_160)]', text: 'text-[oklch(0.55_0.106_160)]', soft: 'bg-[oklch(0.55_0.106_160/0.12)]' },
  { id: 'teal', label: 'Teal', dot: 'bg-[oklch(0.55_0.082_185)]', text: 'text-[oklch(0.55_0.082_185)]', soft: 'bg-[oklch(0.55_0.082_185/0.12)]' },
  { id: 'blue', label: 'Blue', dot: 'bg-[oklch(0.55_0.191_260)]', text: 'text-[oklch(0.55_0.191_260)]', soft: 'bg-[oklch(0.55_0.191_260/0.12)]' },
  { id: 'violet', label: 'Violet', dot: 'bg-[oklch(0.55_0.235_293)]', text: 'text-[oklch(0.55_0.235_293)]', soft: 'bg-[oklch(0.55_0.235_293/0.12)]' },
  { id: 'pink', label: 'Pink', dot: 'bg-[oklch(0.55_0.192_355)]', text: 'text-[oklch(0.55_0.192_355)]', soft: 'bg-[oklch(0.55_0.192_355/0.12)]' },
]

/** A custom colour is stored as its own id: `#rrggbb`. */
const HEX = /^#[0-9a-f]{6}$/i

/** @param {unknown} id @returns {id is string} */
export function isCustomColor(id) {
  return typeof id === 'string' && HEX.test(id)
}

/**
 * A palette entry, or one built for a custom hex. Custom entries carry the
 * colour in `style` (`--cc`) and use classes that read it, so a call site only
 * needs `style={colorOf(x)?.style}` next to the class it already sets.
 * @param {string | null | undefined} id
 * @returns {{ id: string, label: string, dot: string, text: string, soft: string, style?: string } | null}
 */
export function colorOf(id) {
  if (isCustomColor(id)) {
    return { id, label: `Custom ${id}`, dot: 'bg-(color:--cc)', text: 'text-(color:--cc)', soft: 'bg-(color:--cc)/12', style: `--cc: ${id}` }
  }
  return COLORS.find((c) => c.id === id) ?? null
}

/** @returns {Workspace} */
function defaultWorkspace() {
  return { id: DEFAULT_WORKSPACE_ID, name: 'Personal', color: 'blue', createdAt: 0 }
}

/** @param {unknown} w @returns {w is Workspace} */
function valid(w) {
  return !!w && typeof w === 'object' && typeof (/** @type {any} */ (w).id) === 'string' && typeof (/** @type {any} */ (w).name) === 'string'
}

/** @returns {Workspace[]} */
export function loadWorkspaces() {
  /** @type {Workspace[]} */
  let list = []
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) list = JSON.parse(raw).filter(valid)
  } catch {
    list = []
  }
  for (const w of list) if (!colorOf(w.color)) w.color = 'gray'
  // The default workspace always exists, first.
  if (!list.some((w) => w.id === DEFAULT_WORKSPACE_ID)) list.unshift(defaultWorkspace())
  return list
}

/** @param {Workspace[]} list */
function persist(list) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(list))
  } catch {
    /* storage full or blocked: the in-memory store still holds it */
  }
  notifyLocalChange('workspace')
}

export const workspaces = writable(/** @type {Workspace[]} */ (typeof localStorage === 'undefined' ? [defaultWorkspace()] : loadWorkspaces()))

function loadActive() {
  try {
    return localStorage.getItem(ACTIVE_KEY) || DEFAULT_WORKSPACE_ID
  } catch {
    return DEFAULT_WORKSPACE_ID
  }
}

export const activeWorkspaceId = writable(typeof localStorage === 'undefined' ? DEFAULT_WORKSPACE_ID : loadActive())

/** @param {string} id */
export function setActiveWorkspace(id) {
  const ws = get(workspaces)
  const next = ws.some((w) => w.id === id) ? id : DEFAULT_WORKSPACE_ID
  activeWorkspaceId.set(next)
  try {
    localStorage.setItem(ACTIVE_KEY, next)
  } catch {}
}

/** @param {string} name @param {ColorId} color @returns {Workspace} */
export function createWorkspace(name, color) {
  const w = {
    id: crypto.randomUUID(),
    name: name.trim() || 'Untitled',
    color: colorOf(color) ? color : 'gray',
    createdAt: Date.now(),
  }
  workspaces.update((list) => {
    const next = [...list, w]
    persist(next)
    return next
  })
  return w
}

/** @param {string} id @param {Partial<Pick<Workspace, 'name' | 'color'>>} patch */
export function updateWorkspace(id, patch) {
  workspaces.update((list) => {
    const next = list.map((w) =>
      w.id === id
        ? {
            ...w,
            ...(patch.name != null && { name: patch.name.trim() || w.name }),
            ...(patch.color && colorOf(patch.color) && { color: patch.color }),
          }
        : w,
    )
    persist(next)
    return next
  })
}

/**
 * Delete a workspace. Its connections move to the default workspace (they are
 * never deleted with it); the default workspace itself can't be deleted.
 * @param {string} id
 * @param {(id: string) => void} moveConnectionsOut called before the workspace goes
 */
export function deleteWorkspace(id, moveConnectionsOut) {
  if (id === DEFAULT_WORKSPACE_ID) return
  recordDeletion('workspace', id)
  moveConnectionsOut(id)
  workspaces.update((list) => {
    const next = list.filter((w) => w.id !== id)
    persist(next)
    return next
  })
  if (get(activeWorkspaceId) === id) setActiveWorkspace(DEFAULT_WORKSPACE_ID)
}

/**
 * Replace the whole list with what sync brought in (the default workspace is
 * kept, first). An active workspace that's gone falls back to the default.
 * @param {Workspace[]} list
 */
export function replaceWorkspaces(list) {
  const next = list.filter(valid).map((w) => ({ ...w, color: colorOf(w.color) ? w.color : 'gray' }))
  if (!next.some((w) => w.id === DEFAULT_WORKSPACE_ID)) next.unshift(defaultWorkspace())
  else next.sort((a, b) => (a.id === DEFAULT_WORKSPACE_ID ? -1 : b.id === DEFAULT_WORKSPACE_ID ? 1 : 0))
  workspaces.set(next)
  persist(next)
  if (!next.some((w) => w.id === get(activeWorkspaceId))) setActiveWorkspace(DEFAULT_WORKSPACE_ID)
}

/**
 * The workspace a connection belongs to (missing or unknown → default).
 * @param {{ workspaceId?: string | null }} c @param {Workspace[]} list
 */
export function workspaceOf(c, list) {
  const id = c.workspaceId
  return id && list.some((w) => w.id === id) ? id : DEFAULT_WORKSPACE_ID
}

/**
 * Connections in one workspace, in their saved order.
 * @template {{ workspaceId?: string | null }} T
 * @param {T[]} connections @param {string} workspaceId @param {Workspace[]} list
 * @returns {T[]}
 */
export function inWorkspace(connections, workspaceId, list) {
  return connections.filter((c) => workspaceOf(c, list) === workspaceId)
}
