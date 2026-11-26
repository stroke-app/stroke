/**
 * The Undo toast after a delete that can be put back (DESIGN_SYSTEM.md §10b):
 * what went, its name, and Undo for UNDO_MS.
 */
import { toast } from '$lib/components/ui/sonner/toast.svelte.js'

export const UNDO_MS = 8000

/**
 * @param {string} title e.g. "Column deleted"
 * @param {string} description the item's name
 * @param {(() => boolean | void | Promise<boolean | void>) | null | undefined} undo
 *   puts it back; false when it can no longer (the list changed since)
 */
export function offerUndo(title, description, undo) {
  if (!undo) return
  toast.message(title, {
    description,
    duration: UNDO_MS,
    action: {
      label: 'Undo',
      onClick: async () => {
        try {
          if ((await undo()) === false) toast.error("Couldn't undo: it changed since.")
        } catch (err) {
          toast.error(`Couldn't undo: ${err instanceof Error ? err.message : String(err)}`)
        }
      },
    },
  })
}
