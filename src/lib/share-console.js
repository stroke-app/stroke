/**
 * The shared console of a shared studio: one SQL document everyone on the link
 * edits together, with each person's cursor and selection drawn under their
 * name. Yjs merges simultaneous typing; the awareness carries the cursors.
 * Both travel over the share's presence socket, sealed with the link's key.
 *
 * Same protocol as the web studio (stroke-web src/lib/relay/shared-console.ts):
 * `y` an update, `y-sync` someone joining asks for state, `aw` an awareness
 * update, each base64.
 */
import * as Y from 'yjs'
import { Awareness, applyAwarenessUpdate, encodeAwarenessUpdate } from 'y-protocols/awareness'
import { yCollab } from 'y-codemirror.next'

/** @param {Uint8Array} bytes */
function toB64(bytes) {
  let s = ''
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000))
  return btoa(s)
}
/** @param {string} text */
const fromB64 = (text) => Uint8Array.from(atob(text), (c) => c.charCodeAt(0))

/**
 * Join the share's console. `presence` is share-presence.js's handle.
 * @param {ReturnType<typeof import('$lib/share-presence.js').joinPresence>} presence
 * @param {{ name: string, color: string }} me
 */
export function createSharedConsole(presence, me) {
  const doc = new Y.Doc()
  const text = doc.getText('sql')
  const awareness = new Awareness(doc)
  awareness.setLocalState({ user: { name: me.name, color: me.color, colorLight: `${me.color}33` } })

  /** @param {Uint8Array} update @param {unknown} origin */
  const onUpdate = (update, origin) => {
    if (origin !== 'remote') presence.sendDoc({ t: 'y', u: toB64(update) })
  }
  /** @param {{ added: number[], updated: number[], removed: number[] }} ch @param {unknown} origin */
  const onAwareness = ({ added, updated, removed }, origin) => {
    if (origin === 'remote') return
    presence.sendDoc({ t: 'aw', u: toB64(encodeAwarenessUpdate(awareness, [...added, ...updated, ...removed])) })
  }
  doc.on('update', onUpdate)
  awareness.on('update', onAwareness)

  const sync = () => {
    presence.sendDoc({ t: 'y-sync' })
    presence.sendDoc({ t: 'y', u: toB64(Y.encodeStateAsUpdate(doc)) })
  }
  const offDoc = presence.onDoc((msg) => {
    if (msg.t === 'y') Y.applyUpdate(doc, fromB64(msg.u), 'remote')
    else if (msg.t === 'aw') applyAwarenessUpdate(awareness, fromB64(msg.u), 'remote')
    else {
      presence.sendDoc({ t: 'y', u: toB64(Y.encodeStateAsUpdate(doc)) })
      presence.sendDoc({ t: 'aw', u: toB64(encodeAwarenessUpdate(awareness, [doc.clientID])) })
    }
  })
  const offConnect = presence.onConnect(sync)
  sync()

  return {
    doc,
    text,
    awareness,
    /**
     * The editor extension that binds a CodeMirror view to the document. The
     * view must hold the document's text when it is added. The editor keeps
     * its own undo history (it only records this side's edits).
     */
    extension: () => yCollab(text, awareness, { undoManager: false }),
    stop() {
      awareness.setLocalState(null)
      doc.off('update', onUpdate)
      awareness.off('update', onAwareness)
      offDoc()
      offConnect()
      doc.destroy()
    },
  }
}
