/**
 * Mermaid diagram colours, read off the app's own theme tokens, so a diagram
 * sits on the page like any other surface in every theme - the Mermaid
 * views and the diagrams in the assistant's replies alike.
 *
 * A fixed palette per theme id drifted from the themes it was copied from and
 * gave any theme without an entry the generic grey. Read as values rather than
 * var() references, so an exported SVG or PNG keeps them away from the app.
 *
 * @returns {{ bg: string, fg: string, muted?: string, border?: string, line?: string, accent?: string }}
 */
export function liveMermaidTheme() {
  const cs = getComputedStyle(document.documentElement)
  const v = (/** @type {string} */ name) => cs.getPropertyValue(name).trim()
  /** @type {{ bg: string, fg: string, muted?: string, border?: string, line?: string, accent?: string }} */
  const theme = { bg: v('--background') || '#1c1c1c', fg: v('--foreground') || '#f0f0f0' }
  const muted = v('--muted-foreground'), border = v('--border')
  if (muted) theme.muted = muted
  if (border) theme.border = border
  return theme
}
