/** @typedef {'table' | 'sql' | 'ddl' | 'welcome' | 'ai' | 'schema' | 'orm' | 'security' | 'logs' | 'extensions' | 'extension-detail' | 'backup' | 'json' | 'charts' | 'dashboard' | 'erd' | 'reltree' | 'diagrams' | 'search' | 'notebook' | 'schema-timeline' | 'data-diff' | 'insights' | 'objects' | 'redis' | 'license' | 'orm-schema' | 'map' | 'advisor' | 'golf'} StudioTabKind */

import { loadDefaultPageSize } from '$lib/table-query.js'

/** @typedef {import('$lib/table-query.js').TableSort} TableSort */
/** @typedef {import('$lib/table-query.js').TableFilter} TableFilter */
/** @typedef {import('$lib/foreign-key-nav.js').ForeignKeyInfo} ForeignKeyInfo */

/** @typedef {object} TableTabState
 * @property {string} schema
 * @property {string | null} table
 * @property {'table' | 'view' | 'materialized_view' | 'foreign_table'} [tableKind]
 * @property {number} page
 * @property {number} pageSize
 * @property {string} rowSearch
 * @property {TableSort | null} rowSort
 * @property {TableFilter[]} rowFilters
 * @property {Array<{ name: string, dataType?: string }>} columns
 * @property {string[]} primaryKey
 * @property {ForeignKeyInfo[]} foreignKeys
 * @property {unknown[][]} rows
 * @property {number} total
 * @property {number} queryMs
 * @property {boolean} loadingRows
 * @property {string} error
 * @property {Set<number>} selected
 * @property {number | null} focusedRow
 * @property {number | null} [focusedCol] - focused column, in *visible* column space
 * @property {number | null} inspectorRow
 * @property {{ rowIdx: number, colIdx: number, draft: string, original: string } | null} editingCell
 * @property {boolean} savingCell
 * @property {Set<string>} hiddenColumns
 * @property {boolean} filterBarOpen
 * @property {'table' | 'json' | 'record' | 'text' | 'chart' | 'erd'} [dataViewMode]
 * @property {'data' | 'structure'} [tableViewMode] - data grid vs the structure editor
 * @property {unknown[]} [structureColumns] - the structure editor's rows for THIS table
 * @property {string} [structureSearch]
 * @property {number} [scrollLeft]
 * @property {number} [scrollTop]
 * @property {number[]} [expandedRows] - row indices with an open inline detail panel; restored in background/snapshot panes
 * @property {boolean} [expandSingleRow] - opened by following a foreign key: the one row it lands on opens as JSON
 * @property {boolean} [windowedHead] - result set is browsed in windows (huge/heavy result); `rows` lives outside the reactive tree
 * @property {number[]} [windowedLoaded] - window indices resident when the tab was last left
 * @property {number} [windowRows] - rows per window, measured from the payload when the view was built
 * @property {number} [windowBase] - absolute row offset the windowed view starts at
 * @property {number} [windowCount] - rows the windowed view covers
 * @property {number} [windowBytesPerRow] - measured payload per row
 * @property {{ sortColumn: string, sortDirection: string, sorts: Array<{column:string,direction:string}> } | null} [windowOrder] - the one total order every window of this view slices
 */

/** @typedef {object} SqlTabState
 * @property {string} sqlText
 * @property {Array<{ name: string, dataType?: string }>} sqlColumns
 * @property {unknown[][]} sqlRows
 * @property {number} sqlQueryMs
 * @property {string} sqlMessage
 * @property {boolean} sqlLoading
 * @property {string} sqlError
 */

/** @typedef {object} StudioTab
 * @property {string} id
 * @property {StudioTabKind} kind
 * @property {string} title
 * @property {TableTabState | SqlTabState | null} state
 * @property {boolean} [pinned] - pinned tabs stay grouped at the front and survive "Close Others"/"Close All"
 */

let tabSeq = 0

export function nextTabId() {
  tabSeq += 1
  return `tab-${tabSeq}`
}

/** @param {TableTabState} state */
export function cloneTableTabState(state) {
  return {
    ...state,
    // Shallow-copy mutable primitives only - rows/columns/foreignKeys are
    // treated as immutable value arrays; no deep clone needed.
    columns: state.columns,
    primaryKey: state.primaryKey,
    foreignKeys: state.foreignKeys,
    rows: state.rows,
    rowFilters: state.rowFilters,
    rowSort: state.rowSort,
    // Sets must be new instances so mutations don't bleed between tabs
    selected: new Set(state.selected),
    hiddenColumns: new Set(state.hiddenColumns),
    editingCell: state.editingCell ? { ...state.editingCell } : null,
    expandedRows: [...(state.expandedRows ?? [])],
    expandSingleRow: state.expandSingleRow,
    // Immutable value array, like `columns` - reassigned wholesale by loadStructure().
    structureColumns: state.structureColumns,
  }
}

/** @param {SqlTabState} state */
export function cloneSqlTabState(state) {
  // Arrays are immutable value objects; no copy needed
  return { ...state }
}

/** @param {string} [schema] @param {string | null} [table] @param {'table'|'view'|'materialized_view'|'foreign_table'} [tableKind] @returns {TableTabState} */
export function createTableTabState(schema = 'public', table = null, tableKind = 'table') {
  return {
    schema,
    table,
    tableKind,
    page: 1,
    pageSize: loadDefaultPageSize(),
    rowSearch: '',
    rowSort: null,
    rowFilters: [],
    columns: [],
    primaryKey: [],
    foreignKeys: [],
    rows: [],
    total: 0,
    queryMs: 0,
    loadingRows: false,
    error: '',
    selected: new Set(),
    focusedRow: null,
    focusedCol: null,
    inspectorRow: null,
    editingCell: null,
    savingCell: false,
    hiddenColumns: new Set(),
    filterBarOpen: false,
    dataViewMode: 'table',
    tableViewMode: 'data',
    structureColumns: [],
    structureSearch: '',
    scrollLeft: 0,
    scrollTop: 0,
    expandedRows: [],
  }
}

/** @returns {SqlTabState} */
export function createSqlTabState(sqlText = 'SELECT 1;') {
  return {
    sqlText,
    sqlColumns: [],
    sqlRows: [],
    sqlQueryMs: 0,
    sqlMessage: '',
    sqlLoading: false,
    sqlError: '',
  }
}

/** @param {string} [schema] @param {string | null} [table] @param {'table'|'view'|'materialized_view'|'foreign_table'} [tableKind] */
export function createTableTab(schema = 'public', table = null, tableKind = 'table') {
  const state = createTableTabState(schema, table, tableKind)
  return /** @type {StudioTab} */ ({
    id: nextTabId(),
    kind: 'table',
    title: tableTabTitle(state),
    state,
  })
}

/** @param {string} [sqlText] @param {string} [title] */
export function createSqlTab(sqlText, title = 'Query Editor') {
  return /** @type {StudioTab} */ ({
    id: nextTabId(),
    kind: 'sql',
    title,
    state: createSqlTabState(sqlText),
  })
}

/** @typedef {object} DdlTabState
 * @property {string} ddlText
 */

/**
 * A read-only DDL view. Distinct from a `sql` tab on purpose: DDL is something
 * you read, so it gets a bare editor with no Run button, no query toolbar and no
 * results pane - none of which do anything useful for a CREATE TABLE you can't
 * execute against the table it already describes.
 * @param {string} ddlText @param {string} title
 */
export function createDdlTab(ddlText, title) {
  return /** @type {StudioTab} */ ({
    id: nextTabId(),
    kind: 'ddl',
    title,
    state: /** @type {any} */ ({ ddlText }),
  })
}

export function createWelcomeTab() {
  return /** @type {StudioTab} */ ({
    id: nextTabId(),
    kind: 'welcome',
    title: 'New tab',
    state: null,
  })
}

// ── Singleton tabs ────────────────────────────────────────────────────────────
// One stateless instance per kind, looked up by kind alone. The kind → title
// mapping lives here once; tabDisplayTitle falls back to the same map.
/** @type {Partial<Record<StudioTabKind, string>>} */
const SINGLETON_TAB_TITLES = {
  ai: 'AI Chat',
  schema: 'Schema Explorer',
  orm: 'ORM Runner',
  'orm-schema': 'Codegen',
  security: 'Security',
  logs: 'Activity Log',
  insights: 'Instance Insights',
  advisor: 'Advisor',
  objects: 'Database Objects',
  redis: 'Redis',
  map: 'Map',
  extensions: 'Extensions',
  backup: 'Backup & Restore',
  json: 'JSON Viewer',
  charts: 'Charts',
  dashboard: 'Dashboard',
  reltree: 'Relation Tree',
  diagrams: 'Diagrams',
  erd: 'Schema Diagram',
  license: 'Stroke Pro',
  search: 'Find in database',
  'schema-timeline': 'Schema Timeline',
  'data-diff': 'Data Diff',
  golf: 'VACUUM',
}

/** @param {StudioTabKind} kind */
function createKindTab(kind) {
  return /** @type {StudioTab} */ ({
    id: nextTabId(),
    kind,
    title: SINGLETON_TAB_TITLES[kind],
    state: null,
  })
}

/** @param {StudioTab[]} tabs @param {StudioTabKind} kind */
function findTabByKind(tabs, kind) {
  return tabs.find((t) => t.kind === kind) ?? null
}

export const createAiTab = () => createKindTab('ai')
export const createSchemaTab = () => createKindTab('schema')
export const createOrmTab = () => createKindTab('orm')
/** The connected schema rendered as Prisma / Drizzle source. */
export const createOrmSchemaTab = () => createKindTab('orm-schema')
export const createSecurityTab = () => createKindTab('security')
export const createLogsTab = () => createKindTab('logs')
export const createInsightsTab = () => createKindTab('insights')
export const createAdvisorTab = () => createKindTab('advisor')
export const createGolfTab = () => createKindTab('golf')
export const createObjectsTab = () => createKindTab('objects')
export const createRedisTab = () => createKindTab('redis')
export const createMapTab = () => createKindTab('map')
export const createExtensionsTab = () => createKindTab('extensions')
export const createBackupTab = () => createKindTab('backup')
export const createJsonTab = () => createKindTab('json')
export const createChartsTab = () => createKindTab('charts')
export const createDashboardTab = () => createKindTab('dashboard')
export const createRelTreeTab = () => createKindTab('reltree')
export const createDiagramsTab = () => createKindTab('diagrams')
export const createErdTab = () => createKindTab('erd')
export const createLicenseTab = () => createKindTab('license')
export const createSearchTab = () => createKindTab('search')
export const createSchemaTimelineTab = () => createKindTab('schema-timeline')
export const createDataDiffTab = () => createKindTab('data-diff')

/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findAiTab = (tabs) => findTabByKind(tabs, 'ai')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findSchemaTab = (tabs) => findTabByKind(tabs, 'schema')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findOrmTab = (tabs) => findTabByKind(tabs, 'orm')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findOrmSchemaTab = (tabs) => findTabByKind(tabs, 'orm-schema')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findSecurityTab = (tabs) => findTabByKind(tabs, 'security')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findLogsTab = (tabs) => findTabByKind(tabs, 'logs')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findInsightsTab = (tabs) => findTabByKind(tabs, 'insights')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findAdvisorTab = (tabs) => findTabByKind(tabs, 'advisor')
export const findGolfTab = (tabs) => findTabByKind(tabs, 'golf')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findObjectsTab = (tabs) => findTabByKind(tabs, 'objects')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findRedisTab = (tabs) => findTabByKind(tabs, 'redis')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findMapTab = (tabs) => findTabByKind(tabs, 'map')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findExtensionsTab = (tabs) => findTabByKind(tabs, 'extensions')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findBackupTab = (tabs) => findTabByKind(tabs, 'backup')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findJsonTab = (tabs) => findTabByKind(tabs, 'json')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findChartsTab = (tabs) => findTabByKind(tabs, 'charts')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findDashboardTab = (tabs) => findTabByKind(tabs, 'dashboard')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findRelTreeTab = (tabs) => findTabByKind(tabs, 'reltree')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findDiagramsTab = (tabs) => findTabByKind(tabs, 'diagrams')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findErdTab = (tabs) => findTabByKind(tabs, 'erd')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findLicenseTab = (tabs) => findTabByKind(tabs, 'license')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findSearchTab = (tabs) => findTabByKind(tabs, 'search')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findSchemaTimelineTab = (tabs) => findTabByKind(tabs, 'schema-timeline')
/** @type {(tabs: StudioTab[]) => StudioTab | null} */
export const findDataDiffTab = (tabs) => findTabByKind(tabs, 'data-diff')

/** @param {string} extensionId @param {string} name */
export function createExtensionDetailTab(extensionId, name) {
  return /** @type {StudioTab} */ ({
    id: nextTabId(),
    kind: 'extension-detail',
    title: name,
    state: { extensionId },
  })
}

/** @param {StudioTab[]} tabs @param {string} extensionId */
export function findExtensionDetailTab(tabs, extensionId) {
  return tabs.find((t) => t.kind === 'extension-detail' && t.state?.extensionId === extensionId) ?? null
}

/** @param {TableTabState} state */
export function tableTabTitle(state) {
  if (!state.table) return 'Table'
  return state.table
}

/** @param {StudioTab} tab */
export function tabDisplayTitle(tab) {
  if (tab.kind === 'table' && tab.state) return tableTabTitle(/** @type {TableTabState} */ (tab.state))
  // Honour a custom title: a DDL viewer ("DDL · users") and the 2nd/3rd editor
  // ("Query Editor 2") are separate buffers and must not all read "Query Editor".
  if (tab.kind === 'sql') return tab.title || 'Query Editor'
  if (tab.kind === 'notebook') return tab.title || 'Untitled Notebook'
  if (tab.kind === 'extension-detail') return tab.title || tab.state?.extensionId || 'Extension'
  return SINGLETON_TAB_TITLES[tab.kind] ?? tab.title
}

/** @param {StudioTab[]} tabs @param {string} schema @param {string} table */
export function findTableTab(tabs, schema, table) {
  return tabs.find(
    (t) =>
      t.kind === 'table' &&
      t.state &&
      /** @type {TableTabState} */ (t.state).schema === schema &&
      /** @type {TableTabState} */ (t.state).table === table,
  )
}

/** @param {StudioTab[]} tabs */
export function findSqlTab(tabs) {
  return tabs.find((t) => t.kind === 'sql') ?? null
}

/** @param {StudioTab[]} tabs */
export function findLastTableTab(tabs) {
  for (let i = tabs.length - 1; i >= 0; i -= 1) {
    if (tabs[i].kind === 'table') return tabs[i]
  }
  return null
}

/** @param {StudioTab[]} tabs @param {number} fromIndex @param {1 | -1} direction */
export function cycleTabIndex(tabs, fromIndex, direction) {
  if (tabs.length === 0) return -1
  return (fromIndex + direction + tabs.length) % tabs.length
}

/**
 * @typedef {object} NotebookTabState
 * @property {string | null} filePath
 * @property {import('$lib/notebook.js').Notebook} notebook
 * @property {boolean} dirty
 */

/**
 * @param {import('$lib/notebook.js').Notebook} notebook
 * @param {string | null} [filePath]
 */
export function createNotebookTab(notebook, filePath = null) {
  return /** @type {StudioTab} */ ({
    id: nextTabId(),
    kind: 'notebook',
    title: notebook.title || 'Untitled Notebook',
    state: /** @type {NotebookTabState} */ ({ filePath, notebook, dirty: false }),
  })
}
