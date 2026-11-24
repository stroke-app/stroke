// Marks an open modal dialog on <html> as `data-dialog-open`.
//
// app.css hides the scrollbars behind a dialog on Linux. It used to find the
// open dialog with a root `:has([role="dialog"][data-state="open"])`, which
// made WebKitGTK re-check that selector against the whole document on every
// DOM change anywhere in the app. An attribute on <html> costs nothing until
// it flips.

let openDialogs = 0

/** Count a dialog in; DialogContent calls this when it mounts. */
export function beginDialogOpen() {
  openDialogs++
  if (typeof document !== 'undefined') {
    document.documentElement.setAttribute('data-dialog-open', '')
  }
}

/** Count a dialog out; the marker goes when the last one unmounts. */
export function endDialogOpen() {
  openDialogs = Math.max(0, openDialogs - 1)
  if (openDialogs > 0) return
  if (typeof document !== 'undefined') {
    document.documentElement.removeAttribute('data-dialog-open')
  }
}
