/**
 * State behind the sidebar's Objects tab: which tree nodes are open (per
 * connection, when the setting keeps it), a counter bumped whenever the
 * objects may have changed, and the helpers that open SQL for an object.
 */
import { writable } from 'svelte/store'
import { definitionTitle, objectTemplate } from '$lib/object-templates.js'
import { nodeStartsOpen } from '$lib/objects-tree.js'
import { getObjectDefinition } from '$lib/api.js'
import { toast } from '$lib/components/ui/sonner/toast.svelte.js'

/** @typedef {{ text: string, title: string, snippet?: string }} ObjectSql */

/**
 * Bumped after anything that may have changed the objects: a CREATE or DROP
 * run in an editor, a drop from the sidebar, Ctrl+R. The groups reload on it.
 */
export const objectsVersion = writable(0)
export function bumpObjects() {
  objectsVersion.update((n) => n + 1)
}

const KEY = 'stroke:sidebar-objects'

/** @returns {Record<string, { open?: Record<string, boolean> }>} */
function readAll() {
  try {
    const parsed = JSON.parse(localStorage.getItem(KEY) ?? '{}')
    return parsed && typeof parsed === 'object' ? parsed : {}
  } catch {
    return {}
  }
}

/** @param {ReturnType<typeof readAll>} all */
function writeAll(all) {
  try { localStorage.setItem(KEY, JSON.stringify(all)) } catch { /* storage full or blocked */ }
}

class ObjectGroups {
  /** The connection the state belongs to. */
  key = ''
  remember = true
  /** Nodes opened or closed by hand; anything absent takes its default. @type {Record<string, boolean>} */
  open = $state({})

  /**
   * Point the state at a connection and read what it remembered.
   * @param {string} key @param {boolean} remember
   */
  use(key, remember) {
    if (key === this.key && remember === this.remember) return
    this.key = key
    this.remember = remember
    const saved = readAll()[key]?.open ?? {}
    this.open = remember ? { ...saved } : {}
  }

  /** @param {string} key */
  isOpen(key) {
    return nodeStartsOpen(key, this.open, true)
  }

  /** @param {string} key @param {boolean} [to] */
  toggle(key, to = !this.isOpen(key)) {
    this.open = { ...this.open, [key]: to }
    if (!this.key || !this.remember) return
    const all = readAll()
    all[this.key] = { open: this.open }
    writeAll(all)
  }
}

export const objectGroups = new ObjectGroups()

/**
 * Open an object's definition in a new editor tab, as the statement that
 * recreates it.
 * @param {string} schema
 * @param {{ kind: string, name: string, args?: string, table?: string }} obj
 * @param {(sql: ObjectSql) => void} open
 */
export async function openDefinition(schema, obj, open) {
  try {
    const text = await getObjectDefinition(schema, obj)
    open({ text, title: definitionTitle(obj) })
  } catch (e) {
    toast.error(`Couldn't read ${obj.name}`, { description: String(e) })
  }
}

/**
 * Copy an object's definition to the clipboard.
 * @param {string} schema
 * @param {{ kind: string, name: string, args?: string, table?: string }} obj
 */
export async function copyDefinition(schema, obj) {
  try {
    await navigator.clipboard.writeText(await getObjectDefinition(schema, obj))
    toast.success(`Copied the definition of ${obj.name}`)
  } catch (e) {
    toast.error(`Couldn't copy ${obj.name}`, { description: String(e) })
  }
}

/**
 * Open a new editor tab on the CREATE template for a kind.
 * @param {string | null | undefined} type connection type
 * @param {import('$lib/object-templates.js').ObjectKind} kind
 * @param {string} schema
 * @param {(sql: ObjectSql) => void} open
 */
export function openTemplate(type, kind, schema, open) {
  const t = objectTemplate(type, kind, schema)
  if (t) open(t)
}
