/**
 * Deleting a saved connection asks nothing: it goes at once and the toast
 * offers Undo for UNDO_REMOVE_MS. A confirm dialog charged every delete a
 * second click to guard against the rare wrong one; Undo charges only that one.
 */
import { toast } from '$lib/components/ui/sonner/toast.svelte.js'
import { UNDO_REMOVE_MS, restoreConnection } from '$lib/stores/connections.js'

/** @param {{ name?: string, database?: string, host?: string, filePath?: string }} c */
export function connectionLabel(c) {
  return c?.name || c?.database || c?.host || c?.filePath || 'Connection'
}

/**
 * Call right after `removeConnection(id, { undoable: true })`.
 * @param {{ id: string, name?: string, database?: string, host?: string, filePath?: string }} conn
 * @param {() => void} [onrestored] for a list that keeps its own copy
 */
export function toastConnectionRemoved(conn, onrestored) {
  toast.message('Connection deleted', {
    description: connectionLabel(conn),
    duration: UNDO_REMOVE_MS,
    action: {
      label: 'Undo',
      onClick: () => {
        if (restoreConnection(conn.id)) onrestored?.()
        else toast.error("Couldn't undo: the delete already went through.")
      },
    },
  })
}
