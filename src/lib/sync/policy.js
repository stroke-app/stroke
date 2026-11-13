// What sync carries and how its state reads. No store imports, so tests and
// light components can use it.

/**
 * The app settings that follow the account: appearance, the editor and grid,
 * query behaviour. Not synced: window/zoom state, platform-specific defaults
 * (native scroll, menu bar), per-device behaviour (launch at login, updates,
 * MCP autostart, reconnect, timeouts), and consent choices (telemetry, agent
 * web access). AI keys never leave the keychain; AI profiles sync without them.
 */
export const SYNCED_SETTINGS = /** @type {const} */ ([
  'theme', 'font', 'iconStyle', 'iconSet', 'motion',
  'tableStyle', 'rowSpacing', 'zebraRows', 'showRowNumbers', 'numberGrouping', 'gridFontSize',
  'tableTextAlign', 'highlightActiveRow', 'imagePreview', 'openUrlsOnClick', 'fkAutoExpandJson',
  'jsonTheme', 'jsonWordWrap', 'editorTheme', 'vimMode', 'sqlFormat', 'sqlEditor',
  'defaultDataView', 'paginationMode', 'nullSortOrder', 'previewDmlBeforeApply', 'sqlUndo',
  'maxQueryHistory', 'autoSaveQueries', 'cmdkAiEnabled', 'sidebarComments', 'sidebarRememberGroups',
  'agentChatFontSize', 'agentCodeFontSize', 'agentThinkingStyle', 'agentShowQueryCards',
])

/** @param {Record<string, any>} s */
export function pickSyncedSettings(s) {
  /** @type {Record<string, any>} */
  const out = {}
  for (const k of SYNCED_SETTINGS) if (s && s[k] !== undefined) out[k] = s[k]
  return out
}

/** @typedef {import('./runtime.js').SyncStatus} SyncStatus */

/**
 * "Synced 2 min ago" and friends, for the account menu and page.
 * @param {SyncStatus} s @param {number} [now]
 */
export function syncLabel(s, now = Date.now()) {
  switch (s.phase) {
    case 'signed-out': return 'Sync off'
    case 'checking': return 'Checking sync…'
    case 'setup': return 'Sync not set up'
    case 'locked': return 'Sync locked'
    case 'syncing': return 'Syncing…'
    case 'offline': return 'Sync offline'
    case 'error': return 'Sync error'
    default: {
      if (!s.lastSyncedAt) return 'Not synced yet'
      const sec = Math.max(0, Math.round((now - s.lastSyncedAt) / 1000))
      if (sec < 45) return 'Synced just now'
      const min = Math.round(sec / 60)
      if (min < 60) return `Synced ${min} min ago`
      const h = Math.round(min / 60)
      if (h < 24) return `Synced ${h} h ago`
      return `Synced ${Math.round(h / 24)} d ago`
    }
  }
}
